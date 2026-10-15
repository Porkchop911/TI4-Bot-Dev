//! The kernels of the forward pass, generic over the [`Lanes`] of a backend.
//!
//! Every width is a multiple of 8, which the model file guarantees. Each sum has one fixed order,
//! written next to its kernel, and a multiply is never fused into an add.

use crate::InferError;
use crate::simd::Lanes;

/// The output columns of one packed panel.
pub(crate) const NR: usize = 8;

/// The option rows of one micro-kernel call.
const MR: usize = 4;

/// The option rows that [`gemm`] broadcasts at once.
const ROW_BLOCK: usize = 32;

/// A key whose unsigned order is the IEEE-754 total order of `value`: −NaN < −∞ < … < −0 < +0 < …
/// < +∞ < +NaN. Two values with the same bits have the same key.
///
/// The tie-break of [`canonical_bag`] when one column is named twice, as in
/// `ti4_tensor::gather_reduce_batch`, so the duplicates are folded in the same order there and here.
#[must_use]
pub const fn total_order_key(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 == 0 {
        bits ^ 0x8000_0000
    } else {
        !bits
    }
}

/// One option's columns and values as the bag reads them: columns strictly increasing, each with
/// the sum of the values it was named with. Appended to `out`; `scratch` is reused across calls.
///
/// The order and the fold are those of `ti4_tensor::gather_reduce_batch`: already strictly
/// increasing columns are taken as they are; otherwise the pairs are sorted by column, then by
/// [`total_order_key`] of the value, and the values of one column are added in that order.
///
/// # Errors
/// [`InferError::Ragged`], [`InferError::OutOfRange`] or [`InferError::NotFinite`], as the gather
/// of `ti4-tensor` refuses them.
pub(crate) fn canonical_bag(
    columns: &[i64],
    values: &[f32],
    capacity: usize,
    out: &mut Vec<(usize, f32)>,
    scratch: &mut Vec<(i64, f32)>,
) -> Result<(), InferError> {
    if columns.len() != values.len() {
        return Err(InferError::Ragged {
            columns: columns.len(),
            values: values.len(),
        });
    }
    let in_range = |column: i64| usize::try_from(column).is_ok_and(|c| c < capacity);
    if let Some(&column) = columns.iter().find(|column| !in_range(**column)) {
        return Err(InferError::OutOfRange { column, capacity });
    }
    if let Some(&value) = values.iter().find(|value| !value.is_finite()) {
        return Err(InferError::NotFinite(value));
    }
    // In range, so the casts below cannot fail.
    let index = |column: i64| usize::try_from(column).unwrap_or(0);
    out.clear();
    if columns.windows(2).all(|pair| pair[0] < pair[1]) {
        out.extend(
            columns
                .iter()
                .zip(values)
                .map(|(column, value)| (index(*column), *value)),
        );
        return Ok(());
    }
    scratch.clear();
    scratch.extend(columns.iter().copied().zip(values.iter().copied()));
    scratch.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| total_order_key(left.1).cmp(&total_order_key(right.1)))
    });
    for &(column, value) in scratch.iter() {
        let column = index(column);
        match out.last_mut() {
            Some(last) if last.0 == column => last.1 += value,
            _ => out.push((column, value)),
        }
    }
    Ok(())
}

/// The vectors of the bag that one pass over the entries keeps in registers.
const BAG_UNROLL: usize = 8;

/// The first layer of one option: `out = relu(Σ v · W1[column] + identity + b1)`.
///
/// `entries` is a [`canonical_bag`]. Each lane accumulates from zero over the entries in their
/// order (`acc = acc + v · w`); then the identity is added, then `b1`, then the ReLU. An empty bag
/// is a zero row.
///
/// The row is walked in blocks of `BAG_UNROLL` vectors (then of 2, as `width % 8 == 0`): a block
/// stays in registers while every entry is added to it. Each element still adds the entries in
/// their order, so the blocks change nothing of the result.
pub(crate) fn bag<L: Lanes>(
    w1: &[f32],
    width: usize,
    entries: &[(usize, f32)],
    identity: &[f32],
    b1: &[f32],
    out: &mut [f32],
) {
    let wide = width - width % (4 * BAG_UNROLL);
    for start in (0..wide).step_by(4 * BAG_UNROLL) {
        bag_block::<L, BAG_UNROLL>(w1, width, start, entries, identity, b1, out);
    }
    for start in (wide..width).step_by(8) {
        bag_block::<L, 2>(w1, width, start, entries, identity, b1, out);
    }
}

/// The elements `start .. start + 4·VECTORS` of [`bag`].
#[inline(always)]
fn bag_block<L: Lanes, const VECTORS: usize>(
    w1: &[f32],
    width: usize,
    start: usize,
    entries: &[(usize, f32)],
    identity: &[f32],
    b1: &[f32],
    out: &mut [f32],
) {
    let mut acc = [L::zero(); VECTORS];
    for &(column, value) in entries {
        let row = &w1[column * width + start..][..4 * VECTORS];
        let v = L::splat(value);
        for (j, acc) in acc.iter_mut().enumerate() {
            *acc = L::add(*acc, L::mul(v, L::load(&row[4 * j..])));
        }
    }
    let identity = &identity[start..][..4 * VECTORS];
    let b1 = &b1[start..][..4 * VECTORS];
    let out = &mut out[start..][..4 * VECTORS];
    for (j, acc) in acc.iter().enumerate() {
        let x = L::add(*acc, L::load(&identity[4 * j..]));
        let x = L::add(x, L::load(&b1[4 * j..]));
        L::store(&mut out[4 * j..], L::relu(x));
    }
}

/// What follows the product of one output vector.
#[derive(Clone, Copy)]
pub(crate) enum Epilogue<'a> {
    /// `relu(acc + bias)`: the second layer and the inner layer of a block.
    BiasRelu(&'a [f32]),
    /// `(z + acc) + bias`: the outer layer of a block, `z` its input (`[rows][outs]`).
    Residual { z: &'a [f32], bias: &'a [f32] },
}

impl<'a> Epilogue<'a> {
    /// The epilogue of the elements `start .. start + len` of the output.
    fn rows(self, start: usize, len: usize) -> Self {
        match self {
            Self::BiasRelu(bias) => Self::BiasRelu(bias),
            Self::Residual { z, bias } => Self::Residual {
                z: &z[start..start + len],
                bias,
            },
        }
    }
}

/// `c[rows][outs] = epilogue(a[rows][k] · Wᵀ)` with `W [outs][k]` in packed panels.
///
/// A panel holds `NR = 8` outputs: for each `k` in order, the 8 weights `W[p·8 .. p·8+8][k]` side
/// by side. Each output accumulates from zero over `k = 0, 1, …` in order (`acc = acc + a · w`).
///
/// The rows go in blocks of `ROW_BLOCK`. Each element of a block of `a` is first stored four
/// times in `splats` (`[rows][k][4]`), once for all the panels, so the micro-kernel loads a
/// broadcast vector in place of building one at every panel (on SSE a broadcast is a shuffle,
/// which was the bottleneck). Then the panels are the outer loop, so a panel stays in L1 while the
/// rows of the block pass it (the block, 128 KiB at width 256, stays in L2); the rows go four at a
/// time, then a tail of 1 to 3.
pub(crate) fn gemm<L: Lanes>(
    a: &[f32],
    rows: usize,
    k: usize,
    panels: &[f32],
    outs: usize,
    epilogue: Epilogue<'_>,
    splats: &mut Vec<f32>,
    c: &mut [f32],
) {
    splats.resize(ROW_BLOCK.min(rows) * k * 4, 0.0);
    for first in (0..rows).step_by(ROW_BLOCK) {
        let block = ROW_BLOCK.min(rows - first);
        let a = &a[first * k..(first + block) * k];
        for (x, splat) in a.iter().zip(splats.chunks_exact_mut(4)) {
            L::store(splat, L::splat(*x));
        }
        let splats = &splats[..block * k * 4];
        let c = &mut c[first * outs..(first + block) * outs];
        let epilogue = epilogue.rows(first * outs, block * outs);
        for p in 0..outs / NR {
            let panel = &panels[p * k * NR..(p + 1) * k * NR];
            let mut row = 0;
            while row + MR <= block {
                micro::<L, MR>(splats, k, panel, row, p, outs, epilogue, c);
                row += MR;
            }
            match block - row {
                3 => micro::<L, 3>(splats, k, panel, row, p, outs, epilogue, c),
                2 => micro::<L, 2>(splats, k, panel, row, p, outs, epilogue, c),
                1 => micro::<L, 1>(splats, k, panel, row, p, outs, epilogue, c),
                _ => {}
            }
        }
    }
}

/// `ROWS` rows from `row` against panel `p`: two vector accumulators per row.
#[inline(always)]
fn micro<L: Lanes, const ROWS: usize>(
    splats: &[f32],
    k: usize,
    panel: &[f32],
    row: usize,
    p: usize,
    outs: usize,
    epilogue: Epilogue<'_>,
    c: &mut [f32],
) {
    let a_rows: [&[f32]; ROWS] = std::array::from_fn(|m| &splats[(row + m) * k * 4..][..k * 4]);
    let mut acc = [[L::zero(); 2]; ROWS];
    for (kk, w) in panel[..k * NR].chunks_exact(NR).enumerate() {
        let w0 = L::load(w);
        let w1 = L::load(&w[4..]);
        for (a_row, acc) in a_rows.iter().zip(acc.iter_mut()) {
            let x = L::load(&a_row[kk * 4..]);
            acc[0] = L::add(acc[0], L::mul(x, w0));
            acc[1] = L::add(acc[1], L::mul(x, w1));
        }
    }
    for (m, acc) in acc.iter().enumerate() {
        for (half, sum) in acc.iter().enumerate() {
            let column = p * NR + half * 4;
            let at = (row + m) * outs + column;
            let v = match epilogue {
                Epilogue::BiasRelu(bias) => L::relu(L::add(*sum, L::load(&bias[column..]))),
                Epilogue::Residual { z, bias } => {
                    L::add(L::add(L::load(&z[at..]), *sum), L::load(&bias[column..]))
                }
            };
            L::store(&mut c[at..], v);
        }
    }
}

/// One logit per row of `z [rows][width]`: `z · w + b`.
///
/// Lane `l` accumulates from zero the products of the columns `4i + l` in order; the logit is
/// `hsum` of the lanes, then `+ b`.
pub(crate) fn readout<L: Lanes>(z: &[f32], width: usize, w: &[f32], b: f32, out: &mut Vec<f32>) {
    for row in z.chunks_exact(width) {
        let mut acc = L::zero();
        for i in (0..width).step_by(4) {
            acc = L::add(acc, L::mul(L::load(&row[i..]), L::load(&w[i..])));
        }
        out.push(L::hsum(acc) + b);
    }
}

/// `W [outs][k]` (row-major, as a bundle stores it) as packed panels for [`gemm`].
pub(crate) fn pack_panels(w: &[f32], outs: usize, k: usize) -> Vec<f32> {
    let mut panels = Vec::with_capacity(outs * k);
    for p in 0..outs / NR {
        for kk in 0..k {
            for j in 0..NR {
                panels.push(w[(p * NR + j) * k + kk]);
            }
        }
    }
    panels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_total_order_key_orders_as_ieee_total_order() {
        let ordered = [
            f32::NEG_INFINITY,
            -1.0,
            -f32::MIN_POSITIVE,
            -0.0,
            0.0,
            f32::MIN_POSITIVE,
            1.0,
            f32::INFINITY,
        ];
        for pair in ordered.windows(2) {
            assert!(
                total_order_key(pair[0]) < total_order_key(pair[1]),
                "{pair:?}"
            );
        }
    }

    #[test]
    fn a_bag_is_sorted_and_its_duplicates_folded_in_order() {
        let mut out = Vec::new();
        let mut scratch = Vec::new();
        canonical_bag(
            &[5, 2, 5, 9, 2],
            &[1.0, 0.5, -3.0, 2.0, 0.25],
            10,
            &mut out,
            &mut scratch,
        )
        .expect("valid");
        // Column 2: 0.25 then 0.5 (by value); column 5: -3.0 then 1.0.
        assert_eq!(out, vec![(2, 0.25 + 0.5), (5, -3.0 + 1.0), (9, 2.0)]);
        canonical_bag(&[1, 3], &[1.0, 2.0], 10, &mut out, &mut scratch).expect("valid");
        assert_eq!(out, vec![(1, 1.0), (3, 2.0)]);
        assert!(matches!(
            canonical_bag(&[10], &[1.0], 10, &mut out, &mut scratch),
            Err(InferError::OutOfRange { column: 10, .. })
        ));
        assert!(matches!(
            canonical_bag(&[-1], &[1.0], 10, &mut out, &mut scratch),
            Err(InferError::OutOfRange { column: -1, .. })
        ));
        assert!(matches!(
            canonical_bag(&[1], &[f32::NAN], 10, &mut out, &mut scratch),
            Err(InferError::NotFinite(_))
        ));
        assert!(matches!(
            canonical_bag(&[1, 2], &[1.0], 10, &mut out, &mut scratch),
            Err(InferError::Ragged { .. })
        ));
    }

    #[test]
    fn a_panel_holds_eight_outputs_per_k() {
        let (outs, k) = (16, 3);
        let w: Vec<f32> = (0..outs * k).map(|i| i as f32).collect();
        let panels = pack_panels(&w, outs, k);
        for p in 0..outs / NR {
            for kk in 0..k {
                for j in 0..NR {
                    assert_eq!(panels[p * k * NR + kk * NR + j], w[(p * NR + j) * k + kk]);
                }
            }
        }
    }
}
