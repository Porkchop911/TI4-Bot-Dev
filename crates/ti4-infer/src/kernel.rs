//! The kernels of the forward pass, generic over the [`Lanes`] of a backend.
//!
//! Every width is a multiple of 8, which the model file guarantees. Each sum has one fixed order,
//! written next to its kernel, and a multiply is never fused into an add.

use crate::InferError;
use crate::simd::{Lanes, Readout};

/// The output columns of one packed panel.
pub(crate) const NR: usize = 8;

/// The option rows that [`gemm`] broadcasts at once.
const ROW_BLOCK: usize = 48;

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

/// The value of a [`total_order_key`].
const fn from_total_order_key(key: u32) -> f32 {
    f32::from_bits(if key & 0x8000_0000 != 0 {
        key ^ 0x8000_0000
    } else {
        !key
    })
}

/// One option's `(column, value)` pairs as the bag reads them: columns strictly increasing, each
/// with the sum of the values it was named with. Written to `out`; `scratch` is reused across calls.
///
/// A column named more than once is folded in a fixed order: its values sorted by the
/// [`total_order_key`] of the value, and the values of one column are added in that order.
///
/// # Errors
/// [`InferError::OutOfRange`] or [`InferError::NotFinite`], as the gather of `ti4-tensor` refuses
/// them.
pub(crate) fn canonical_bag(
    option: &[(u32, f32)],
    capacity: usize,
    out: &mut Vec<(usize, f32)>,
    scratch: &mut Vec<u64>,
) -> Result<(), InferError> {
    if let Some(&(column, _)) = option
        .iter()
        .find(|(column, _)| *column as usize >= capacity)
    {
        return Err(InferError::OutOfRange { column, capacity });
    }
    if let Some(&(_, value)) = option.iter().find(|(_, value)| !value.is_finite()) {
        return Err(InferError::NotFinite(value));
    }
    out.clear();
    if option.windows(2).all(|pair| pair[0].0 < pair[1].0) {
        out.extend(
            option
                .iter()
                .map(|(column, value)| (*column as usize, *value)),
        );
        return Ok(());
    }
    // One key per pair, the column above the total-order key of the value: its order is the
    // order by column, then by value. Two equal keys are the same column and the same bits, so
    // the unstable sort gives the order of a stable one.
    scratch.clear();
    scratch.extend(
        option
            .iter()
            .map(|(column, value)| (u64::from(*column) << 32) | u64::from(total_order_key(*value))),
    );
    scratch.sort_unstable();
    for &key in scratch.iter() {
        let column = (key >> 32) as usize;
        let value = from_total_order_key(key as u32);
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
/// The row is walked in blocks of `BAG_UNROLL` vectors (then of one): a block stays in registers
/// while every entry is added to it. Each element still adds the entries in their order, so
/// neither the blocks nor the width of a vector change anything of the result.
#[inline(always)]
pub(crate) fn bag<L: Lanes>(
    w1: &[f32],
    width: usize,
    entries: &[(usize, f32)],
    identity: &[f32],
    b1: &[f32],
    out: &mut [f32],
) {
    let step = L::N * BAG_UNROLL;
    let wide = width - width % step;
    for start in (0..wide).step_by(step) {
        bag_block::<L, BAG_UNROLL>(w1, width, start, entries, identity, b1, out);
    }
    for start in (wide..width).step_by(L::N) {
        bag_block::<L, 1>(w1, width, start, entries, identity, b1, out);
    }
}

/// The elements `start .. start + N·VECTORS` of [`bag`].
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
    let len = L::N * VECTORS;
    let mut acc = [L::zero(); VECTORS];
    for &(column, value) in entries {
        let row = &w1[column * width + start..][..len];
        let v = L::splat(value);
        for (j, acc) in acc.iter_mut().enumerate() {
            *acc = L::mul_add(*acc, v, L::load(&row[L::N * j..]));
        }
    }
    let identity = &identity[start..][..len];
    let b1 = &b1[start..][..len];
    let out = &mut out[start..][..len];
    for (j, acc) in acc.iter().enumerate() {
        let x = L::add(*acc, L::load(&identity[L::N * j..]));
        let x = L::add(x, L::load(&b1[L::N * j..]));
        L::store(&mut out[L::N * j..], L::relu(x));
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
    #[inline(always)]
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
/// by side. Each output accumulates from zero over `k = 0, 1, …` in order (`acc = acc + a · w`),
/// whatever the width of a vector.
///
/// The rows go in blocks of `ROW_BLOCK`. Each block of `a` is first packed into `packed`, once
/// for all the panels: by groups of the rows of one micro-kernel call (`MR`, then the tail), and
/// in a group `[k][row][SPLAT]`, so that the micro-kernel reads the activations of one `k` side
/// by side, as it reads the weights, with no bounds check. On a backend that broadcasts from a
/// stored vector (`SPLAT > 1`) each element is stored `SPLAT` times, so the micro-kernel loads a
/// broadcast vector in place of building one at every panel (on SSE a broadcast is a shuffle); on
/// AVX it broadcasts from memory in one load. Then the columns are the outer loop, two vectors of
/// outputs at a time (one panel on 4 lanes, two on 8), so the panels stay in L1 while the rows of
/// the block pass them (the packed block, at most 192 KiB at width 256, stays in L2).
#[inline(always)]
pub(crate) fn gemm<L: Lanes>(
    a: &[f32],
    rows: usize,
    k: usize,
    panels: &[f32],
    outs: usize,
    epilogue: Epilogue<'_>,
    packed: &mut Vec<f32>,
    c: &mut [f32],
) {
    let s = L::SPLAT;
    packed.resize(ROW_BLOCK.min(rows) * k * s, 0.0);
    for first in (0..rows).step_by(ROW_BLOCK) {
        let block = ROW_BLOCK.min(rows - first);
        let a = &a[first * k..(first + block) * k];
        let mut group = 0;
        while group < block {
            let size = L::MR.min(block - group);
            let dst = &mut packed[group * k * s..(group + size) * k * s];
            for (m, a_row) in a[group * k..(group + size) * k].chunks_exact(k).enumerate() {
                for (kk, x) in a_row.iter().enumerate() {
                    let at = (kk * size + m) * s;
                    if s == 1 {
                        dst[at] = *x;
                    } else {
                        L::store(&mut dst[at..at + s], L::splat(*x));
                    }
                }
            }
            group += size;
        }
        let packed = &packed[..block * k * s];
        let c = &mut c[first * outs..(first + block) * outs];
        let epilogue = epilogue.rows(first * outs, block * outs);
        let mut column = 0;
        while column + 2 * L::N <= outs {
            rows_of::<L, 2>(packed, block, k, panels, column, outs, epilogue, c);
            column += 2 * L::N;
        }
        // Only on 8 lanes: a width that is an odd multiple of 8 leaves one panel.
        if 2 * L::N > NR && column < outs {
            rows_of::<L, 1>(packed, block, k, panels, column, outs, epilogue, c);
        }
    }
}

/// Every row of a block against the `VECS` output vectors from `column`: `MR` rows at a time,
/// then the tail.
#[inline(always)]
fn rows_of<L: Lanes, const VECS: usize>(
    packed: &[f32],
    block: usize,
    k: usize,
    panels: &[f32],
    column: usize,
    outs: usize,
    epilogue: Epilogue<'_>,
    c: &mut [f32],
) {
    let mut row = 0;
    if L::MR == 6 {
        while row + 6 <= block {
            micro::<L, 6, VECS>(packed, k, panels, column, row, outs, epilogue, c);
            row += 6;
        }
    } else {
        while row + 4 <= block {
            micro::<L, 4, VECS>(packed, k, panels, column, row, outs, epilogue, c);
            row += 4;
        }
    }
    match block - row {
        5 if L::MR > 5 => micro::<L, 5, VECS>(packed, k, panels, column, row, outs, epilogue, c),
        4 if L::MR > 4 => micro::<L, 4, VECS>(packed, k, panels, column, row, outs, epilogue, c),
        3 => micro::<L, 3, VECS>(packed, k, panels, column, row, outs, epilogue, c),
        2 => micro::<L, 2, VECS>(packed, k, panels, column, row, outs, epilogue, c),
        1 => micro::<L, 1, VECS>(packed, k, panels, column, row, outs, epilogue, c),
        _ => {}
    }
}

/// `ROWS` rows from `row` against the `VECS` output vectors from `column`: one accumulator per
/// row and vector. The weights of the output `o` at `k` are at `(o / 8)·k·8 + k·8 + o % 8`; the
/// activations of the group at `k` are the `ROWS · SPLAT` floats at `k · ROWS · SPLAT`.
#[inline(always)]
fn micro<L: Lanes, const ROWS: usize, const VECS: usize>(
    packed: &[f32],
    k: usize,
    panels: &[f32],
    column: usize,
    row: usize,
    outs: usize,
    epilogue: Epilogue<'_>,
    c: &mut [f32],
) {
    let s = L::SPLAT;
    let a = packed[row * k * s..][..k * ROWS * s].chunks_exact(ROWS * s);
    // The panel of each vector, and where in a group of 8 weights the vector starts. On 4 lanes
    // both vectors are in one panel; on 8, each is a panel (the second is the first again when
    // there is one vector).
    // `column` is a multiple of 8, so the offsets are constants.
    let (off0, off1) = (0, if VECS == 2 { L::N % NR } else { 0 });
    let panel = |j: usize| {
        let p = column / NR + (j.min(VECS - 1) * L::N) / NR;
        &panels[p * k * NR..][..k * NR]
    };
    let (w0, w1) = (panel(0), panel(1));
    let mut acc = [[L::zero(); VECS]; ROWS];
    for ((a, w0), w1) in a.zip(w0.chunks_exact(NR)).zip(w1.chunks_exact(NR)) {
        let wv = [L::load(&w0[off0..]), L::load(&w1[off1..])];
        for (m, acc) in acc.iter_mut().enumerate() {
            let x = L::load_splat(&a[m * s..]);
            for (acc, wv) in acc.iter_mut().zip(&wv) {
                *acc = L::mul_add(*acc, x, *wv);
            }
        }
    }
    for (m, acc) in acc.iter().enumerate() {
        for (j, sum) in acc.iter().enumerate() {
            let o = column + j * L::N;
            let at = (row + m) * outs + o;
            let v = match epilogue {
                Epilogue::BiasRelu(bias) => L::relu(L::add(*sum, L::load(&bias[o..]))),
                Epilogue::Residual { z, bias } => {
                    L::add(L::add(L::load(&z[at..]), *sum), L::load(&bias[o..]))
                }
            };
            L::store(&mut c[at..], v);
        }
    }
}

/// One logit per row of `z [rows][width]`: `z · w + b`, on the 4 lanes of `L::Readout` whatever
/// the width of `L`.
///
/// Lane `l` accumulates from zero the products of the columns `4i + l` in order; the logit is
/// `hsum` of the lanes, then `+ b`.
#[inline(always)]
pub(crate) fn readout<L: Lanes>(z: &[f32], width: usize, w: &[f32], b: f32, out: &mut Vec<f32>) {
    readout_quad::<L::Readout>(z, width, w, b, out);
}

/// [`readout`] on the 4 lanes `Q`.
#[inline(always)]
pub(crate) fn readout_quad<Q: Readout>(
    z: &[f32],
    width: usize,
    w: &[f32],
    b: f32,
    out: &mut Vec<f32>,
) {
    for row in z.chunks_exact(width) {
        let mut acc = Q::zero();
        for i in (0..width).step_by(4) {
            acc = Q::mul_add(acc, Q::load(&row[i..]), Q::load(&w[i..]));
        }
        out.push(Q::hsum(acc) + b);
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
        for value in ordered {
            let back = from_total_order_key(total_order_key(value));
            assert_eq!(back.to_bits(), value.to_bits());
        }
    }

    #[test]
    fn a_bag_is_sorted_and_its_duplicates_folded_in_order() {
        let mut out = Vec::new();
        let mut scratch = Vec::new();
        canonical_bag(
            &[(5, 1.0), (2, 0.5), (5, -3.0), (9, 2.0), (2, 0.25)],
            10,
            &mut out,
            &mut scratch,
        )
        .expect("valid");
        // Column 2: 0.25 then 0.5 (by value); column 5: -3.0 then 1.0.
        assert_eq!(out, vec![(2, 0.25 + 0.5), (5, -3.0 + 1.0), (9, 2.0)]);
        canonical_bag(&[(1, 1.0), (3, 2.0)], 10, &mut out, &mut scratch).expect("valid");
        assert_eq!(out, vec![(1, 1.0), (3, 2.0)]);
        assert!(matches!(
            canonical_bag(&[(10, 1.0)], 10, &mut out, &mut scratch),
            Err(InferError::OutOfRange { column: 10, .. })
        ));
        assert!(matches!(
            canonical_bag(&[(1, f32::NAN)], 10, &mut out, &mut scratch),
            Err(InferError::NotFinite(_))
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
