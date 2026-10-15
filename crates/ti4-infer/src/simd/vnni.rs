//! The int8 kernels on x86-64 with AVX2, FMA and AVX-VNNI (`Avx2Vnni`): the arithmetic of
//! [`Int8`], which `quant::reference` repeats in plain loops bit for bit.
//!
//! - `gemm`: `vpdpbusd` (`_mm256_dpbusd_avx_epi32`, the VEX encoding of AVX-VNNI) multiplies the
//!   4 unsigned activation bytes of a group with the 4 signed weight bytes of each output and adds
//!   the 4 products into the output's i32 lane. The products are summed in i32, so nothing
//!   saturates (unlike `pmaddubsw`, whose i16 pair sums do): at width 256 a dot is at most
//!   256 · 255 · 127 in magnitude. The micro-kernel is `MR = 6` rows × 2 panels (16 outputs): 12
//!   accumulators, two weight vectors and a broadcast of the 4 bytes of a row.
//! - `bag`: the int8 rows are widened (`vpmovsxbd`), converted and multiplied by
//!   `v · scale[row]` with an FMA into f32 accumulators.
//! - `quantize`: the min and the max of the rows (exact in any order), then `cvtps2dq` (to the
//!   nearest even, the rounding mode Rust keeps) and the saturating packs.
//!
//! The CPU has these instructions only if `is_x86_feature_detected!` says so. The type is private
//! to `simd`, and the only path to it is [`run_vnni`], which [`super::run_int8`] calls after that
//! check: the safety argument of each `unsafe` block.

use core::arch::x86_64::{
    __m256, __m256i, _mm_loadl_epi64, _mm256_add_epi32, _mm256_add_ps, _mm256_cvtepi8_epi32,
    _mm256_cvtepi32_ps, _mm256_cvtps_epi32, _mm256_dpbusd_avx_epi32, _mm256_fmadd_ps,
    _mm256_loadu_ps, _mm256_loadu_si256, _mm256_max_ps, _mm256_min_ps, _mm256_mul_ps,
    _mm256_mullo_epi32, _mm256_packs_epi32, _mm256_packus_epi16, _mm256_permutevar8x32_epi32,
    _mm256_set1_epi32, _mm256_set1_ps, _mm256_setr_epi32, _mm256_setzero_ps, _mm256_setzero_si256,
    _mm256_storeu_ps, _mm256_storeu_si256, _mm256_sub_epi32,
};

use super::{Int8, RunInt8, Sse};
use crate::kernel::Epilogue;
use crate::quant::{GROUP, Int8Epilogue, Layer, PANEL};

/// The option rows that `gemm` packs at once.
const ROW_BLOCK: usize = 48;

/// The f32 vectors of the bag that one pass over the entries keeps in registers.
const BAG_UNROLL: usize = 8;

/// Whether the CPU has AVX2, FMA and AVX-VNNI.
pub(super) fn has_avx2_vnni() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
        && std::arch::is_x86_feature_detected!("fma")
        && std::arch::is_x86_feature_detected!("avxvnni")
}

/// `work` on `Vnni`, compiled with AVX2, FMA and AVX-VNNI.
#[target_feature(enable = "avx2,fma,avxvnni")]
pub(super) fn run_vnni<R: RunInt8>(work: R) -> R::Output {
    work.run::<Vnni>()
}

/// The int8 kernels of `Avx2Vnni`.
pub(super) struct Vnni;

#[inline(always)]
fn load_ps(src: &[f32]) -> __m256 {
    let src = &src[..8];
    // SAFETY: eight floats; unaligned load; the CPU has AVX2 (the module).
    unsafe { _mm256_loadu_ps(src.as_ptr()) }
}

#[inline(always)]
fn store_ps(dst: &mut [f32], v: __m256) {
    let dst = &mut dst[..8];
    // SAFETY: room for eight floats; unaligned store; the CPU has AVX2 (the module).
    unsafe { _mm256_storeu_ps(dst.as_mut_ptr(), v) }
}

#[inline(always)]
fn load_si(src: &[i8]) -> __m256i {
    let src = &src[..32];
    // SAFETY: 32 bytes; unaligned load; the CPU has AVX2 (the module).
    unsafe { _mm256_loadu_si256(src.as_ptr().cast()) }
}

#[inline(always)]
fn load_i32(src: &[i32]) -> __m256i {
    let src = &src[..8];
    // SAFETY: eight i32; unaligned load; the CPU has AVX2 (the module).
    unsafe { _mm256_loadu_si256(src.as_ptr().cast()) }
}

/// Eight i8 widened to eight f32.
#[inline(always)]
fn widen(src: &[i8]) -> __m256 {
    let src = &src[..8];
    // SAFETY: `_mm_loadl_epi64` reads 8 bytes, which `src` has (unaligned is allowed); the CPU has
    // AVX2 (the module).
    unsafe { _mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(_mm_loadl_epi64(src.as_ptr().cast()))) }
}

#[inline(always)]
fn splat(x: f32) -> __m256 {
    // SAFETY: the CPU has AVX2 (the module).
    unsafe { _mm256_set1_ps(x) }
}

#[inline(always)]
fn relu(v: __m256) -> __m256 {
    // SAFETY: the CPU has AVX2 (the module). `vmaxps(v, 0)` gives +0 for −0, as `x > 0 ? x : 0`.
    unsafe { _mm256_max_ps(v, _mm256_setzero_ps()) }
}

#[inline(always)]
fn add(a: __m256, b: __m256) -> __m256 {
    // SAFETY: the CPU has AVX2 (the module).
    unsafe { _mm256_add_ps(a, b) }
}

#[inline(always)]
fn mul(a: __m256, b: __m256) -> __m256 {
    // SAFETY: the CPU has AVX2 (the module).
    unsafe { _mm256_mul_ps(a, b) }
}

/// The lanes of a vector, low first.
#[inline(always)]
fn lanes(v: __m256) -> [f32; 8] {
    let mut out = [0.0f32; 8];
    store_ps(&mut out, v);
    out
}

/// `round(x · inv) + zero` of 8 floats, as i32. (A function, not a closure: a closure is compiled
/// without the target features and could not inline the intrinsics.)
#[inline(always)]
fn rounded(x: &[f32], inv: __m256, zero: __m256i) -> __m256i {
    // SAFETY: the CPU has AVX2 (the module); `cvtps2dq` rounds to the nearest even.
    unsafe { _mm256_add_epi32(_mm256_cvtps_epi32(mul(load_ps(x), inv)), zero) }
}

/// `round(x · inv) + zero` of 32 floats, clamped to `0..=255`, into 32 bytes.
#[inline(always)]
fn quantize32(x: &[f32], inv: __m256, zero: __m256i, q: &mut [u8]) {
    let q = &mut q[..32];
    let x = &x[..32];
    let (a, b, c, d) = (
        rounded(&x[0..], inv, zero),
        rounded(&x[8..], inv, zero),
        rounded(&x[16..], inv, zero),
        rounded(&x[24..], inv, zero),
    );
    // SAFETY: the CPU has AVX2 (the module); 32 bytes of room in `q`. The packs saturate to i16,
    // then to u8 (the clamp); they work in 128-bit halves, so the result is
    // `a0-3 b0-3 c0-3 d0-3 a4-7 b4-7 c4-7 d4-7` and the permute puts the dwords in order.
    unsafe {
        let ab = _mm256_packs_epi32(a, b);
        let cd = _mm256_packs_epi32(c, d);
        let bytes = _mm256_packus_epi16(ab, cd);
        let order = _mm256_setr_epi32(0, 4, 1, 5, 2, 6, 3, 7);
        _mm256_storeu_si256(
            q.as_mut_ptr().cast(),
            _mm256_permutevar8x32_epi32(bytes, order),
        );
    }
}

impl Int8 for Vnni {
    type Readout = Sse;
    const MR: usize = 6;

    #[inline(always)]
    fn bag(
        w1: &[i8],
        scale: &[f32],
        width: usize,
        entries: &[(usize, f32)],
        identity: &[f32],
        b1: &[f32],
        out: &mut [f32],
    ) {
        let step = 8 * BAG_UNROLL;
        let wide = width - width % step;
        for start in (0..wide).step_by(step) {
            bag_block::<BAG_UNROLL>(w1, scale, width, start, entries, identity, b1, out);
        }
        for start in (wide..width).step_by(8) {
            bag_block::<1>(w1, scale, width, start, entries, identity, b1, out);
        }
    }

    #[inline(always)]
    fn quantize(x: &[f32], q: &mut [u8]) -> (f32, i32) {
        // SAFETY: the CPU has AVX2 (the module).
        let (mut lo, mut hi) = unsafe { (_mm256_setzero_ps(), _mm256_setzero_ps()) };
        for x in x.chunks_exact(8) {
            let v = load_ps(x);
            // SAFETY: the CPU has AVX2 (the module).
            unsafe {
                lo = _mm256_min_ps(lo, v);
                hi = _mm256_max_ps(hi, v);
            }
        }
        // The min and the max are exact in any order; a zero of either sign is the same below.
        let lo = lanes(lo).into_iter().fold(0.0f32, f32::min);
        let hi = lanes(hi).into_iter().fold(0.0f32, f32::max);
        let range = hi - lo;
        let inv = 255.0 / range;
        let q = &mut q[..x.len()];
        if !inv.is_finite() {
            q.fill(0);
            return (0.0, 0);
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "within 0..=255 of finite rows"
        )]
        let point = ((-lo * inv).round_ties_even() as i32).clamp(0, 255);
        let wide = x.len() - x.len() % 32;
        // SAFETY: the CPU has AVX2 (the module).
        let point_v = unsafe { _mm256_set1_epi32(point) };
        for start in (0..wide).step_by(32) {
            quantize32(&x[start..], splat(inv), point_v, &mut q[start..]);
        }
        for (q, &x) in q[wide..].iter_mut().zip(&x[wide..]) {
            #[expect(clippy::cast_possible_truncation, reason = "clamped")]
            let value = ((x * inv).round_ties_even() as i32 + point).clamp(0, 255);
            *q = u8::try_from(value).unwrap_or(0);
        }
        (range / 255.0, point)
    }

    #[inline(always)]
    fn gemm(
        q: &[u8],
        rows: usize,
        k: usize,
        scale: f32,
        zero: i32,
        layer: Layer<'_>,
        epilogue: Int8Epilogue<'_>,
        packed: &mut Vec<u8>,
        c: &mut [f32],
    ) {
        let outs = layer.outs;
        let groups = k / GROUP;
        // Rows quantized to zero (all deltas zero: one row, or rows alike): every product is +0,
        // and only the epilogue is left.
        if scale == 0.0 {
            if let Int8Epilogue::Shifted { base, epilogue } = epilogue {
                for (r, c) in c[..rows * outs].chunks_exact_mut(outs).enumerate() {
                    for o in (0..outs).step_by(8) {
                        // SAFETY: the CPU has AVX2 (the module).
                        let y = add(unsafe { _mm256_setzero_ps() }, load_ps(&base[o..]));
                        store_ps(&mut c[o..], finish(epilogue, y, o, r * outs + o));
                    }
                }
                return;
            }
        }
        packed.resize(ROW_BLOCK.min(rows) * k, 0);
        for first in (0..rows).step_by(ROW_BLOCK) {
            let block = ROW_BLOCK.min(rows - first);
            // The rows of the block by groups of the rows of one micro-kernel call, and in a group
            // `[k / 4][row][4]`, so that the micro-kernel walks the activations and the panels
            // side by side with no bounds check.
            let a = &q[first * k..(first + block) * k];
            let mut group = 0;
            while group < block {
                let size = Self::MR.min(block - group);
                let dst = &mut packed[group * k..(group + size) * k];
                for (m, row) in a[group * k..(group + size) * k].chunks_exact(k).enumerate() {
                    for (g, bytes) in row.chunks_exact(GROUP).enumerate() {
                        let at = (g * size + m) * GROUP;
                        dst[at..at + GROUP].copy_from_slice(bytes);
                    }
                }
                group += size;
            }
            let packed = &packed[..block * k];
            let c = &mut c[first * outs..(first + block) * outs];
            let epilogue = epilogue_rows(epilogue, first * outs, block * outs);
            let rows = Rows {
                packed,
                groups,
                scale,
                zero,
            };
            let mut column = 0;
            while column + 2 * PANEL <= outs {
                rows_of::<2>(&rows, block, layer, column, epilogue, c);
                column += 2 * PANEL;
            }
            if column < outs {
                rows_of::<1>(&rows, block, layer, column, epilogue, c);
            }
        }
    }
}

/// The epilogue of the elements `start .. start + len` of the output.
#[inline(always)]
fn epilogue_rows(epilogue: Int8Epilogue<'_>, start: usize, len: usize) -> Int8Epilogue<'_> {
    match epilogue {
        Int8Epilogue::Raw => Int8Epilogue::Raw,
        Int8Epilogue::Shifted {
            base,
            epilogue: Epilogue::BiasRelu(bias),
        } => Int8Epilogue::Shifted {
            base,
            epilogue: Epilogue::BiasRelu(bias),
        },
        Int8Epilogue::Shifted {
            base,
            epilogue: Epilogue::Residual { z, bias },
        } => Int8Epilogue::Shifted {
            base,
            epilogue: Epilogue::Residual {
                z: &z[start..start + len],
                bias,
            },
        },
    }
}

/// The layer's epilogue of `y` (the product plus that of the base row) at the output `o`,
/// element `at` of the output rows: `relu(y + b)` or `(z + y) + b`.
#[inline(always)]
fn finish(epilogue: Epilogue<'_>, y: __m256, o: usize, at: usize) -> __m256 {
    match epilogue {
        Epilogue::BiasRelu(bias) => relu(add(y, load_ps(&bias[o..]))),
        Epilogue::Residual { z, bias } => add(add(load_ps(&z[at..]), y), load_ps(&bias[o..])),
    }
}

/// The packed activations of a block of rows, with their scale and zero point.
struct Rows<'a> {
    packed: &'a [u8],
    groups: usize,
    scale: f32,
    zero: i32,
}

/// The elements `start .. start + 8·VECTORS` of the bag of one option.
#[inline(always)]
fn bag_block<const VECTORS: usize>(
    w1: &[i8],
    scale: &[f32],
    width: usize,
    start: usize,
    entries: &[(usize, f32)],
    identity: &[f32],
    b1: &[f32],
    out: &mut [f32],
) {
    let len = 8 * VECTORS;
    // SAFETY: the CPU has AVX2 (the module).
    let mut acc = [unsafe { _mm256_setzero_ps() }; VECTORS];
    for &(column, value) in entries {
        let row = &w1[column * width + start..][..len];
        let v = splat(value * scale[column]);
        for (j, acc) in acc.iter_mut().enumerate() {
            // SAFETY: the CPU has FMA (the module).
            *acc = unsafe { _mm256_fmadd_ps(v, widen(&row[8 * j..]), *acc) };
        }
    }
    let identity = &identity[start..][..len];
    let b1 = &b1[start..][..len];
    let out = &mut out[start..][..len];
    for (j, acc) in acc.iter().enumerate() {
        let x = add(*acc, load_ps(&identity[8 * j..]));
        let x = add(x, load_ps(&b1[8 * j..]));
        store_ps(&mut out[8 * j..], relu(x));
    }
}

/// Every row of a block against the `VECS` panels from `column`: 6 rows at a time, then the tail.
#[inline(always)]
fn rows_of<const VECS: usize>(
    rows: &Rows<'_>,
    block: usize,
    layer: Layer<'_>,
    column: usize,
    epilogue: Int8Epilogue<'_>,
    c: &mut [f32],
) {
    let mut row = 0;
    while row + 6 <= block {
        micro::<6, VECS>(rows, layer, column, row, epilogue, c);
        row += 6;
    }
    match block - row {
        5 => micro::<5, VECS>(rows, layer, column, row, epilogue, c),
        4 => micro::<4, VECS>(rows, layer, column, row, epilogue, c),
        3 => micro::<3, VECS>(rows, layer, column, row, epilogue, c),
        2 => micro::<2, VECS>(rows, layer, column, row, epilogue, c),
        1 => micro_one::<VECS>(rows, layer, column, row, epilogue, c),
        _ => {}
    }
}

/// `ROWS` rows from `row` against the `VECS` panels from `column`: one i32 accumulator per row
/// and panel, then the epilogue of each.
#[inline(always)]
fn micro<const ROWS: usize, const VECS: usize>(
    rows: &Rows<'_>,
    layer: Layer<'_>,
    column: usize,
    row: usize,
    epilogue: Int8Epilogue<'_>,
    c: &mut [f32],
) {
    let k = rows.groups * GROUP;
    let a = rows.packed[row * k..][..k * ROWS].chunks_exact(ROWS * GROUP);
    let panel = |j: usize| {
        let p = column / PANEL + j.min(VECS - 1);
        &layer.panels[p * k * PANEL..][..k * PANEL]
    };
    let (w0, w1) = (panel(0), panel(1));
    // SAFETY: the CPU has AVX2 (the module).
    let mut acc = [[unsafe { _mm256_setzero_si256() }; VECS]; ROWS];
    for ((a, w0), w1) in a
        .zip(w0.chunks_exact(GROUP * PANEL))
        .zip(w1.chunks_exact(GROUP * PANEL))
    {
        let wv = [load_si(w0), load_si(w1)];
        for (m, acc) in acc.iter_mut().enumerate() {
            let bytes = &a[m * GROUP..][..GROUP];
            let word = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            // SAFETY: the CPU has AVX2 and AVX-VNNI (the module).
            unsafe {
                let x = _mm256_set1_epi32(word);
                for (acc, wv) in acc.iter_mut().zip(&wv) {
                    *acc = _mm256_dpbusd_avx_epi32(*acc, x, *wv);
                }
            }
        }
    }
    store(rows, layer, column, row, epilogue, c, &acc);
}

/// [`micro`] of one row (the base row of every layer, a decision of one option): four sets of
/// accumulators over the groups `4i`, `4i + 1`, `4i + 2`, `4i + 3`, so that the `vpdpbusd` of a
/// row are not one chain of its latency, added at the end. The sums are of i32, exact in any
/// order, so the result is that of one accumulator.
#[inline(always)]
fn micro_one<const VECS: usize>(
    rows: &Rows<'_>,
    layer: Layer<'_>,
    column: usize,
    row: usize,
    epilogue: Int8Epilogue<'_>,
    c: &mut [f32],
) {
    const SETS: usize = 4;
    let k = rows.groups * GROUP;
    let a = &rows.packed[row * k..][..k];
    let panel = |j: usize| {
        let p = column / PANEL + j.min(VECS - 1);
        &layer.panels[p * k * PANEL..][..k * PANEL]
    };
    let (w0, w1) = (panel(0), panel(1));
    // SAFETY: the CPU has AVX2 (the module).
    let mut acc = [[unsafe { _mm256_setzero_si256() }; VECS]; SETS];
    let wide = rows.groups - rows.groups % SETS;
    for ((a, w0), w1) in a[..wide * GROUP]
        .chunks_exact(SETS * GROUP)
        .zip(w0.chunks_exact(SETS * GROUP * PANEL))
        .zip(w1.chunks_exact(SETS * GROUP * PANEL))
    {
        for (set, acc) in acc.iter_mut().enumerate() {
            let at = set * GROUP * PANEL;
            step(acc, &a[set * GROUP..], &w0[at..], &w1[at..]);
        }
    }
    for g in wide..rows.groups {
        let at = g * GROUP * PANEL;
        step(&mut acc[0], &a[g * GROUP..], &w0[at..], &w1[at..]);
    }
    let mut sum = acc[0];
    for set in &acc[1..] {
        for (sum, part) in sum.iter_mut().zip(set) {
            // SAFETY: the CPU has AVX2 (the module).
            *sum = unsafe { _mm256_add_epi32(*sum, *part) };
        }
    }
    store(rows, layer, column, row, epilogue, c, &[sum]);
}

/// One group of one row: the 4 activation bytes at `a` against the panels at `w0` and `w1`.
#[inline(always)]
fn step<const VECS: usize>(acc: &mut [__m256i; VECS], a: &[u8], w0: &[i8], w1: &[i8]) {
    let word = i32::from_le_bytes([a[0], a[1], a[2], a[3]]);
    let wv = [load_si(w0), load_si(w1)];
    dot4(acc, word, &wv);
}

/// `acc[j]` plus the dot of the 4 bytes of `word` with the 4 weights of each output in `wv[j]`.
#[inline(always)]
fn dot4<const VECS: usize>(acc: &mut [__m256i; VECS], word: i32, wv: &[__m256i; 2]) {
    // SAFETY: the CPU has AVX2 and AVX-VNNI (the module).
    unsafe {
        let x = _mm256_set1_epi32(word);
        for (acc, wv) in acc.iter_mut().zip(wv) {
            *acc = _mm256_dpbusd_avx_epi32(*acc, x, *wv);
        }
    }
}

/// The epilogue of the i32 dots `acc` of `ROWS` rows from `row` and the `VECS` panels from
/// `column`, into `c`.
#[inline(always)]
fn store<const ROWS: usize, const VECS: usize>(
    rows: &Rows<'_>,
    layer: Layer<'_>,
    column: usize,
    row: usize,
    epilogue: Int8Epilogue<'_>,
    c: &mut [f32],
    acc: &[[__m256i; VECS]; ROWS],
) {
    let outs = layer.outs;
    let scale = splat(rows.scale);
    // SAFETY: the CPU has AVX2 (the module).
    let zero = unsafe { _mm256_set1_epi32(rows.zero) };
    for (m, acc) in acc.iter().enumerate() {
        for (j, sum) in acc.iter().enumerate() {
            let o = column + j * PANEL;
            let at = (row + m) * outs + o;
            // SAFETY: the CPU has AVX2 (the module). The correction and the dot are exact in i32.
            let y = unsafe {
                let exact =
                    _mm256_sub_epi32(*sum, _mm256_mullo_epi32(zero, load_i32(&layer.sum[o..])));
                mul(
                    _mm256_cvtepi32_ps(exact),
                    mul(scale, load_ps(&layer.scale[o..])),
                )
            };
            let v = match epilogue {
                Int8Epilogue::Raw => y,
                Int8Epilogue::Shifted { base, epilogue } => {
                    finish(epilogue, add(y, load_ps(&base[o..])), o, at)
                }
            };
            store_ps(&mut c[at..], v);
        }
    }
}
