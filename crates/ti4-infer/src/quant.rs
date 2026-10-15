//! The int8 models of training (`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`, section H): the
//! quantization of the weights, the VNNI panels, and the int8 weights of a loaded model.
//!
//! - The dense matrices (`W2`, the residual `W_in` and `W_out`) are i8 with one f32 scale per
//!   output, symmetric (`scale = max |W[o]| / 127`, `q = round(W · 127 / max)`, to the nearest
//!   even), in VNNI panels: for 8 outputs, each group of 4 consecutive `k` holds the 4 weights of
//!   each output side by side, 32 bytes, as `vpdpbusd` reads them.
//! - `W1` is i8 rows with one scale per row, quantized the same way: the bag reads a quarter of
//!   the bytes and dequantizes on the fly.
//! - Biases, the identity embedding, the readout and the value readout stay f32.
//!
//! The activations are quantized when a dense layer reads them (`simd::Int8`): the rows of a
//! decision as a base row and the deltas from it ([`split_base`]), each quantized with one scale
//! and zero point, and the two products added in f32 before the epilogue.
//! The residual stream `z` stays f32: the residual add is `(z + y) + b` in f32, and the readout
//! reads the f32 rows. Only the input of a dense layer is quantized, with a zero point, because
//! after a residual add a row is no longer ≥ 0; rows after a ReLU have zero point 0.
//!
//! The int8 path runs on [`Backend::Avx2Vnni`](crate::Backend::Avx2Vnni) alone. [`reference`]
//! repeats it in plain loops over the unpacked weights, for the test that it gives the same bits.

use std::ops::Range;

use crate::kernel::Epilogue;

/// The weights of a model file: f32, or int8 for the rollouts of training.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quant {
    /// Every weight f32: the files of the server and of the page.
    F32,
    /// `"quant": "int8"` in the header: the dense matrices and `W1` in int8 (section H). It runs
    /// on [`Backend::Avx2Vnni`](crate::Backend::Avx2Vnni) only, and no server offers it.
    Int8,
}

impl Quant {
    /// Its name in a header and on the command line: `"f32"` or `"int8"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::F32 => "f32",
            Self::Int8 => "int8",
        }
    }
}

/// The outputs of one VNNI panel.
pub(crate) const PANEL: usize = 8;

/// The `k` of one group of a VNNI panel: the 4 bytes that `vpdpbusd` sums into one lane.
pub(crate) const GROUP: usize = 4;

/// `W [rows][cols]` as i8 with one scale per row: `scale = max |row| / 127` (0 for a row whose
/// `127 / max` is not finite: zero, or too small to matter),
/// `q = clamp(round(w · (127 / max)), −127, 127)`, rounding to the nearest even.
pub(crate) fn quantize_rows(w: &[f32], rows: usize, cols: usize) -> (Vec<i8>, Vec<f32>) {
    let mut q = Vec::with_capacity(rows * cols);
    let mut scales = Vec::with_capacity(rows);
    for row in w[..rows * cols].chunks_exact(cols) {
        let max = row.iter().fold(0.0f32, |max, w| max.max(w.abs()));
        let inv = 127.0 / max;
        // A row of zeros, or one so small that `127 / max` overflows, is a zero row.
        if inv.is_finite() {
            #[expect(clippy::cast_possible_truncation, reason = "clamped to ±127")]
            q.extend(
                row.iter()
                    .map(|w| (w * inv).round_ties_even().clamp(-127.0, 127.0) as i8),
            );
            scales.push(max / 127.0);
        } else {
            q.extend(std::iter::repeat_n(0, cols));
            scales.push(0.0);
        }
    }
    (q, scales)
}

/// `Q [outs][k]` (row-major) as VNNI panels: panel `p`, group `g`, output `j`, byte `i` at
/// `(p · k/4 + g) · 32 + 4j + i`, holding `Q[8p + j][4g + i]`.
pub(crate) fn pack_vnni(q: &[i8], outs: usize, k: usize) -> Vec<i8> {
    let mut panels = Vec::with_capacity(outs * k);
    for p in 0..outs / PANEL {
        for g in 0..k / GROUP {
            for j in 0..PANEL {
                let row = &q[(p * PANEL + j) * k + g * GROUP..][..GROUP];
                panels.extend_from_slice(row);
            }
        }
    }
    panels
}

/// `Σ_k Q[o][k]` of each output of VNNI panels: what a zero point subtracts.
pub(crate) fn panel_sums(panels: &[i8], outs: usize, k: usize) -> Vec<i32> {
    let mut sums = vec![0i32; outs];
    for (p, panel) in panels.chunks_exact(k * PANEL).enumerate() {
        for group in panel.chunks_exact(GROUP * PANEL) {
            for (j, weights) in group.chunks_exact(GROUP).enumerate() {
                sums[p * PANEL + j] += weights.iter().map(|&w| i32::from(w)).sum::<i32>();
            }
        }
    }
    sums
}

/// One int8 dense layer as the kernel reads it.
#[derive(Clone, Copy)]
pub(crate) struct Layer<'a> {
    /// VNNI panels of `W [outs][k]`.
    pub(crate) panels: &'a [i8],
    /// `[outs]`.
    pub(crate) scale: &'a [f32],
    /// `[outs]`: [`panel_sums`].
    pub(crate) sum: &'a [i32],
    pub(crate) outs: usize,
}

/// An int8 matrix in the buffer of a model.
#[derive(Debug, Clone)]
pub(crate) struct Int8Matrix {
    /// Bytes: the VNNI panels.
    pub(crate) panels: Range<usize>,
    /// Floats: one scale per output.
    pub(crate) scale: Range<usize>,
    /// The sum of each output's weights, computed at load.
    pub(crate) sum: Vec<i32>,
}

/// The int8 weights of a model (the f32 ranges of its biases, embedding and readout are those of
/// an f32 model).
#[derive(Debug, Clone)]
pub(crate) struct Int8Weights {
    /// Bytes: `W1 [capacity][width]`.
    pub(crate) w1: Range<usize>,
    /// Floats: `[capacity]`.
    pub(crate) w1_scale: Range<usize>,
    pub(crate) w2: Int8Matrix,
    /// `(W_in, W_out)` of each residual block.
    pub(crate) blocks: Vec<(Int8Matrix, Int8Matrix)>,
}

/// What follows the int8 product of one output vector of a layer.
#[derive(Clone, Copy)]
pub(crate) enum Int8Epilogue<'a> {
    /// The product itself: that of the base row of the layer's input.
    Raw,
    /// `epilogue(y + base[o])`: the product of the deltas, plus that of the base row, then the
    /// bias and the ReLU or the residual add of the layer.
    Shifted {
        /// `[outs]`: the product of the base row.
        base: &'a [f32],
        epilogue: Epilogue<'a>,
    },
}

/// The input of an int8 layer, `x [rows][width]`, as a base row and the deltas from it: `base[k]
/// = (lo + hi) · 0.5` of the least and the greatest `x[r][k]` over the rows, and `deltas[r][k] =
/// x[r][k] − base[k]`.
///
/// The layer multiplies each, quantized on its own, and adds the two products. The options of a
/// decision share most of their features, so their rows are close: the base carries what they
/// share, and its quantization error is the same for every option, which the softmax does not
/// see; the deltas are small, so their step is fine. Quantized together as they are, the rows
/// would differ by a rounding noise of the whole range, and the logits of close options would
/// differ by it. A decision of one row has zero deltas.
#[inline(always)]
pub(crate) fn split_base(x: &[f32], width: usize, base: &mut [f32], deltas: &mut [f32]) {
    let (base, hi) = (&mut base[..width], &mut deltas[..width]);
    base.copy_from_slice(&x[..width]);
    hi.copy_from_slice(&x[..width]);
    for row in x.chunks_exact(width).skip(1) {
        for ((lo, hi), &x) in base.iter_mut().zip(hi.iter_mut()).zip(row) {
            if x < *lo {
                *lo = x;
            }
            if x > *hi {
                *hi = x;
            }
        }
    }
    for (lo, hi) in base.iter_mut().zip(hi.iter()) {
        *lo = (*lo + *hi) * 0.5;
    }
    for (deltas, row) in deltas.chunks_exact_mut(width).zip(x.chunks_exact(width)) {
        for ((delta, &x), &base) in deltas.iter_mut().zip(row).zip(base.iter()) {
            *delta = x - base;
        }
    }
}

/// The quantization of the activation rows of one decision: `(scale, zero point)` and the u8
/// values into `q`. The arithmetic of `simd::Int8::quantize`.
#[cfg(test)]
fn quantize_rows_of(x: &[f32], q: &mut [u8]) -> (f32, i32) {
    let (mut lo, mut hi) = (0.0f32, 0.0f32);
    for &x in x {
        if x < lo {
            lo = x;
        }
        if x > hi {
            hi = x;
        }
    }
    let range = hi - lo;
    let inv = 255.0 / range;
    if !inv.is_finite() {
        q.fill(0);
        return (0.0, 0);
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "within 0..=255 of finite rows"
    )]
    let zero = ((-lo * inv).round_ties_even() as i32).clamp(0, 255);
    for (q, &x) in q.iter_mut().zip(x) {
        #[expect(clippy::cast_possible_truncation, reason = "clamped")]
        let value = ((x * inv).round_ties_even() as i32 + zero).clamp(0, 255);
        *q = u8::try_from(value).unwrap_or(0);
    }
    (range / 255.0, zero)
}

/// The int8 forward pass in plain loops over the unpacked quantized weights, in the order that
/// `simd::Int8` fixes: the reference of the test that `Avx2Vnni` gives the same bits.
#[cfg(test)]
pub(crate) fn reference(
    parts: &crate::Parts,
    faction: usize,
    head: usize,
    options: &[Vec<(u32, f32)>],
) -> Vec<f32> {
    let w = parts.width;
    let heads = parts.heads.len();
    let (w1, s1) = quantize_rows(&parts.w1, parts.capacity, w);
    let dense = |m: &[f32]| quantize_rows(m, w, w);
    let w2 = dense(&parts.w2);
    let blocks: Vec<_> = parts
        .blocks
        .iter()
        .map(|block| (dense(&block.w_in), dense(&block.w_out)))
        .collect();
    let relu = |x: f32| if x > 0.0 { x } else { 0.0 };
    // `epilogue(o, y)` of every output `o` of every row of `x [rows][w]` through the layer
    // `(q, scales)`, the rows quantized together.
    // The product of quantized rows with the layer, before the epilogue.
    let product = |x: &[f32], (q, scales): &(Vec<i8>, Vec<f32>)| {
        let mut rows = vec![0u8; x.len()];
        let (scale, zero) = quantize_rows_of(x, &mut rows);
        let mut out = Vec::with_capacity(x.len());
        for row in rows.chunks_exact(w) {
            for o in 0..w {
                let weights = &q[o * w..][..w];
                let dot: i32 = row
                    .iter()
                    .zip(weights)
                    .map(|(&a, &b)| i32::from(a) * i32::from(b))
                    .sum();
                let sum: i32 = weights.iter().map(|&b| i32::from(b)).sum();
                #[expect(clippy::cast_precision_loss, reason = "the conversion of the kernel")]
                out.push((dot - zero * sum) as f32 * (scale * scales[o]));
            }
        }
        out
    };
    // The base row and the deltas, each quantized and multiplied, then the epilogue of
    // `delta product + base product`.
    let layer =
        |x: &[f32], weights: &(Vec<i8>, Vec<f32>), epilogue: &dyn Fn(usize, usize, f32) -> f32| {
            let mut base = vec![0.0f32; w];
            let mut deltas = vec![0.0f32; x.len()];
            split_base(x, w, &mut base, &mut deltas);
            let shift = product(&base, weights);
            product(&deltas, weights)
                .into_iter()
                .enumerate()
                .map(|(i, y)| epilogue(i / w, i % w, y + shift[i % w]))
                .collect::<Vec<f32>>()
        };
    let mut entries = Vec::new();
    let mut sort = Vec::new();
    let mut z = Vec::with_capacity(options.len() * w);
    for option in options {
        crate::kernel::canonical_bag(option, parts.capacity, &mut entries, &mut sort)
            .expect("a valid option");
        for o in 0..w {
            let mut acc = 0.0f32;
            for &(column, value) in &entries {
                let coefficient = value * s1[column];
                acc = coefficient.mul_add(f32::from(w1[column * w + o]), acc);
            }
            let identity = if o < crate::EMBED_DIM {
                parts.embedding[faction * crate::EMBED_DIM + o]
            } else {
                0.0
            };
            z.push(relu((acc + identity) + parts.b1[o]));
        }
    }
    let mut z = layer(&z, &w2, &|_, o, y| relu(y + parts.b2[o]));
    for (block, (w_in, w_out)) in parts.blocks.iter().zip(&blocks) {
        let hidden = layer(&z, w_in, &|_, o, y| relu(y + block.b_in[o]));
        z = layer(&hidden, w_out, &|r, o, y| {
            (z[r * w + o] + y) + block.b_out[o]
        });
    }
    let at = faction * heads + head;
    z.chunks_exact(w)
        .map(|z| {
            let mut lanes = [0.0f32; 4];
            for i in 0..w {
                let weight = parts.w_shared[head * w + i] + parts.delta[at * w + i];
                lanes[i % 4] += z[i] * weight;
            }
            let bias = parts.b_shared[head] + parts.b_delta[at];
            ((lanes[0] + lanes[1]) + (lanes[2] + lanes[3])) + bias
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_quantized_symmetrically_within_half_a_step() {
        let w = [0.5f32, -1.27, 0.0, 0.004, 1.0, -0.3, 0.0, 0.0];
        let (q, scales) = quantize_rows(&w, 2, 4);
        assert_eq!(scales[1], 1.0 / 127.0);
        assert_eq!(q[4], 127);
        assert_eq!(q[..4].iter().copied().max_by_key(|q| q.abs()), Some(-127));
        for (row, scale) in scales.iter().enumerate() {
            for col in 0..4 {
                let back = f32::from(q[row * 4 + col]) * scale;
                assert!((back - w[row * 4 + col]).abs() <= scale * 0.5 + 1e-7);
            }
        }
        let (q, scales) = quantize_rows(&[0.0; 8], 1, 8);
        assert_eq!((q, scales), (vec![0; 8], vec![0.0]));
    }

    #[test]
    fn a_vnni_panel_holds_four_k_of_eight_outputs_and_its_sums() {
        let (outs, k) = (16, 8);
        let q: Vec<i8> = (0..outs * k)
            .map(|i| i8::try_from(i % 251).unwrap_or(0).wrapping_sub(60))
            .collect();
        let panels = pack_vnni(&q, outs, k);
        for p in 0..outs / PANEL {
            for g in 0..k / GROUP {
                for j in 0..PANEL {
                    for i in 0..GROUP {
                        assert_eq!(
                            panels[(p * k / GROUP + g) * 32 + 4 * j + i],
                            q[(p * PANEL + j) * k + g * GROUP + i]
                        );
                    }
                }
            }
        }
        let sums = panel_sums(&panels, outs, k);
        for (o, sum) in sums.iter().enumerate() {
            assert_eq!(
                *sum,
                q[o * k..][..k].iter().map(|&w| i32::from(w)).sum::<i32>()
            );
        }
    }

    #[test]
    fn activations_have_a_zero_point_only_when_they_have_negatives() {
        let mut q = [0u8; 4];
        let (scale, zero) = quantize_rows_of(&[0.0, 1.0, 2.55, 0.5], &mut q);
        assert_eq!((scale, zero), (2.55 / 255.0, 0));
        assert_eq!(q, [0, 100, 255, 50]);
        let (_, zero) = quantize_rows_of(&[-1.0, 1.0, 0.0, 0.0], &mut q);
        assert_eq!(zero, 128);
        assert_eq!(q[0], 0);
        assert_eq!(q[1], 255);
        assert_eq!(quantize_rows_of(&[0.0, -0.0, 0.0, 0.0], &mut q), (0.0, 0));
    }
}
