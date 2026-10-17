//! The lanes the kernels are written in, their backends, and the dispatch to a backend.
//!
//! Every backend but `Avx2Fma` has the same arithmetic: a multiply, then an add (no FMA); the
//! ReLU `x > 0 ? x : 0`; and the horizontal sum of the readout `(l0 + l1) + (l2 + l3)` over four
//! lanes. In `bag` and `gemm` each lane is its own output, so a backend of 8 lanes (`Avx2`) gives
//! the bits of one of 4; the readout runs on 4 lanes on every backend ([`Lanes::Readout`]). So the
//! kernels give the same bits on every deterministic backend.
//!
//! The trait is safe: a load reads the first `N` floats of a slice and a store writes the first
//! `N`, and both panic on a shorter slice. A backend that uses intrinsics checks the length and
//! keeps its `unsafe` here, so the kernels have none.
//!
//! # The dispatch
//!
//! A [`Model`](crate::Model) holds its [`Backend`], chosen once when it is loaded. [`run`] matches
//! it once per forward pass and calls the forward monomorphized for that backend: the kernels
//! have no dispatch of their own. The AVX2 backends need their instructions compiled in: `run`
//! calls a `#[target_feature(enable = "avx2")]` (or `"avx2,fma"`) function, only after
//! `is_x86_feature_detected!` has said yes, and the forward, the kernels and the lane operations
//! are all `#[inline(always)]`, so the whole forward is inlined into that function and compiled
//! with its features. (A function without them that called an AVX intrinsic could not inline it.)
//! The `Avx2` and `Avx2Fma` types are private to this module, so no other path reaches them.
//!
//! The int8 models of training (section H of the plan) run on [`Backend::Avx2Vnni`] alone, through
//! [`run_int8`] and the [`Int8`] kernels of `vnni.rs`, compiled with `"avx2,fma,avxvnni"` the same
//! way. Their f32 arithmetic (the bag on the dequantized rows, the epilogues) fuses as `Avx2Fma`
//! does, so [`run`] runs other work on `Avx2Vnni` with the lanes of `Avx2Fma`.
//!
//! This module also holds [`AlignedF32`], the 64-byte-aligned buffer the sections of a model live
//! in. It and the intrinsic backends are the only code of the crate allowed `unsafe`.

#![allow(unsafe_code)]

#[cfg(target_arch = "x86_64")]
mod avx2;
mod scalar;
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
mod simd128;
#[cfg(target_arch = "x86_64")]
mod sse;
#[cfg(target_arch = "x86_64")]
mod vnni;

pub(crate) use scalar::Scalar;
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
pub(crate) use simd128::Simd128;
#[cfg(target_arch = "x86_64")]
pub(crate) use sse::Sse;

/// The backend a model runs its forward pass on.
///
/// Every backend but [`Backend::Avx2Fma`] gives the same bits, so a game is the same game on the
/// server and in the page. A model is loaded on [`Backend::best`], and
/// [`Model::with_backend`](crate::Model::with_backend) picks another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// `[f32; 4]` lanes: every target, and the reference of the tests.
    Scalar,
    /// `_mm_*_ps`, 4 lanes: x86-64, whose baseline has SSE2.
    Sse,
    /// `f32x4_*`: wasm built with `simd128`.
    Simd128,
    /// `_mm256_*_ps`, 8 lanes, a multiply then an add: x86-64 with AVX2, detected at run time.
    Avx2,
    /// `_mm256_fmadd_ps`: x86-64 with AVX2 and FMA. Each multiply-add rounds once, so its logits
    /// are not the bits of the others (within `1e-4 · max(1, |logit|)` of them).
    Avx2Fma,
    /// The int8 models of training (`"quant": "int8"`): `vpdpbusd` (AVX-VNNI, u8 × i8 summed into
    /// i32) in the dense layers, the bag on the int8 rows in f32 with FMA. x86-64 with AVX2, FMA
    /// and AVX-VNNI, detected at run time. It runs an int8 model and nothing else, and an int8
    /// model runs on it alone.
    Avx2Vnni,
}

impl Backend {
    /// Every backend, the deterministic ones from the slowest to the fastest, then `Avx2Fma`, then
    /// `Avx2Vnni` (int8 models only).
    pub const ALL: [Self; 6] = [
        Self::Scalar,
        Self::Sse,
        Self::Simd128,
        Self::Avx2,
        Self::Avx2Fma,
        Self::Avx2Vnni,
    ];

    /// Its name in a report: `"scalar"`, `"sse"`, `"simd128"`, `"avx2"`, `"avx2-fma"` or
    /// `"avx2-vnni"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Sse => "sse",
            Self::Simd128 => "simd128",
            Self::Avx2 => "avx2",
            Self::Avx2Fma => "avx2-fma",
            Self::Avx2Vnni => "avx2-vnni",
        }
    }

    /// Whether its logits are the bits of [`Backend::Scalar`] (all but `Avx2Fma` and `Avx2Vnni`).
    #[must_use]
    pub const fn is_deterministic(self) -> bool {
        !matches!(self, Self::Avx2Fma | Self::Avx2Vnni)
    }

    /// Whether it runs the int8 models (`"quant": "int8"`): `Avx2Vnni`, and only it.
    #[must_use]
    pub const fn is_int8(self) -> bool {
        matches!(self, Self::Avx2Vnni)
    }

    /// Whether this build and this CPU can run it. The AVX2 backends are detected at run time
    /// (`is_x86_feature_detected!`, which the standard library caches).
    #[must_use]
    pub fn is_available(self) -> bool {
        match self {
            Self::Scalar => true,
            Self::Sse => cfg!(target_arch = "x86_64"),
            Self::Simd128 => cfg!(all(target_arch = "wasm32", target_feature = "simd128")),
            #[cfg(target_arch = "x86_64")]
            Self::Avx2 => avx2::has_avx2(),
            #[cfg(target_arch = "x86_64")]
            Self::Avx2Fma => avx2::has_avx2_fma(),
            #[cfg(target_arch = "x86_64")]
            Self::Avx2Vnni => vnni::has_avx2_vnni(),
            #[cfg(not(target_arch = "x86_64"))]
            Self::Avx2 | Self::Avx2Fma | Self::Avx2Vnni => false,
        }
    }

    /// The fastest deterministic backend available: `Avx2` on an x86-64 CPU with AVX2, else `Sse`
    /// on x86-64, `Simd128` on wasm with `simd128`, else `Scalar`. A model is loaded on it.
    #[must_use]
    pub fn best() -> Self {
        Self::ALL
            .into_iter()
            .rev()
            .find(|backend| backend.is_deterministic() && backend.is_available())
            .unwrap_or(Self::Scalar)
    }

    /// The fastest f32 backend available, deterministic or not: `Avx2Fma` where the CPU has it,
    /// else [`Backend::best`]. For hosts that no other host must reproduce (the rollouts of
    /// training, a server with `TI4_INFER_FMA=1`). `Avx2Vnni` runs int8 models only, so it is
    /// asked for by name.
    #[must_use]
    pub fn fastest() -> Self {
        if Self::Avx2Fma.is_available() {
            Self::Avx2Fma
        } else {
            Self::best()
        }
    }
}

/// Work generic over the lanes of a backend: a forward pass. [`run`] calls it on one backend.
pub(crate) trait Run {
    /// What the work gives.
    type Output;
    /// The work on the lanes `L`. Implementations are `#[inline(always)]`, so that the work is
    /// compiled with the target features of the function [`run`] calls it from.
    fn run<L: Lanes>(self) -> Self::Output;
}

/// `work` on `backend`: one match, then the work monomorphized for that backend.
///
/// # Panics
/// If the backend is not available ([`Backend::is_available`]); a [`Model`](crate::Model) holds
/// only an available one.
pub(crate) fn run<R: Run>(backend: Backend, work: R) -> R::Output {
    match backend {
        Backend::Scalar => work.run::<Scalar>(),
        #[cfg(target_arch = "x86_64")]
        Backend::Sse => work.run::<Sse>(),
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        Backend::Simd128 => work.run::<Simd128>(),
        #[cfg(target_arch = "x86_64")]
        Backend::Avx2 if avx2::has_avx2() => {
            // SAFETY: the CPU has AVX2, which is all that `run_avx2` is compiled with.
            unsafe { avx2::run_avx2(work) }
        }
        #[cfg(target_arch = "x86_64")]
        Backend::Avx2Fma if avx2::has_avx2_fma() => {
            // SAFETY: the CPU has AVX2 and FMA, which is all that `run_avx2_fma` is compiled with.
            unsafe { avx2::run_avx2_fma(work) }
        }
        // The f32 lanes of `Avx2Vnni` are those of `Avx2Fma` (its CPU has AVX2 and FMA).
        #[cfg(target_arch = "x86_64")]
        Backend::Avx2Vnni if vnni::has_avx2_vnni() => {
            // SAFETY: the CPU has AVX2 and FMA, which is all that `run_avx2_fma` is compiled with.
            unsafe { avx2::run_avx2_fma(work) }
        }
        _ => panic!(
            "the backend {} is not available on this target or CPU",
            backend.name()
        ),
    }
}

/// The kernels of an int8 model (section H of the plan), on one backend: `Avx2Vnni`.
///
/// The arithmetic, which `quant::reference` repeats in plain loops bit for bit:
///
/// - `bag`: per element, from zero, `acc = fma(v · scale[column], W1[column][o], acc)` over the
///   entries of a canonical bag in order (one rounding per entry); then `+ identity`, `+ b1`, and
///   the ReLU.
/// - `quantize`: rows together, with one scale and one zero point: `lo = min(0, min x)`, `hi = max(0, max x)`; rows whose
///   `255 / (hi − lo)` is not finite (`hi == lo`, or a range too small to matter) are all zero
///   with scale 0 and zero point 0; else `inv = 255 / (hi − lo)`, `scale = (hi − lo) / 255`,
///   `zero = round(−lo · inv)` and `q = clamp(round(x · inv) + zero, 0, 255)`, each rounding to the
///   nearest even. Rows after a ReLU have `lo = 0`, so their zero point is 0 and their scale
///   `max / 255`.
/// - `gemm`: per output `o` of a row, the integer dot `Σ_k q[k] · W[o][k]` (i32, exact: `vpdpbusd`
///   adds four u8 × i8 products into each i32 lane, with no i16 step to saturate), minus
///   `zero · Σ_k W[o][k]`; that as f32 times `scale · scale_w[o]`; then
///   ([`Int8Epilogue`](crate::quant::Int8Epilogue)) that alone (the product of the base row), or
///   plus the product of the base row and then the layer's epilogue in f32
///   ([`Epilogue`](crate::kernel::Epilogue): `relu(y + b)`, or `(z + y) + b`), no multiply fused.
///   Rows of scale 0 have the product +0.
///
/// A layer quantizes its input as a base row and the deltas from it (`quant::split_base`), and
/// runs `gemm` on each.
pub(crate) trait Int8 {
    /// The 4 lanes of the readout of the final f32 rows.
    type Readout: Readout;
    /// The option rows of the matrix micro-kernel.
    const MR: usize;
    /// The first layer of one option on the int8 rows `w1 [capacity][width]` with one scale per
    /// row.
    fn bag(
        w1: &[i8],
        scale: &[f32],
        width: usize,
        entries: &[(usize, f32)],
        identity: &[f32],
        b1: &[f32],
        out: &mut [f32],
    );
    /// The rows `x` of a decision as u8 `q`; gives their scale and zero point.
    fn quantize(x: &[f32], q: &mut [u8]) -> (f32, i32);
    /// `c[rows][outs] = epilogue(dequantized q · Wᵀ)` with `W` in VNNI panels (`quant`). `packed`
    /// is scratch.
    fn gemm(
        q: &[u8],
        rows: usize,
        k: usize,
        scale: f32,
        zero: i32,
        layer: crate::quant::Layer<'_>,
        epilogue: crate::quant::Int8Epilogue<'_>,
        packed: &mut Vec<u8>,
        c: &mut [f32],
    );
}

/// Work generic over the int8 kernels: the forward pass of an int8 model.
pub(crate) trait RunInt8 {
    /// What the work gives.
    type Output;
    /// The work on the kernels `K`, `#[inline(always)]` as [`Run::run`] is.
    fn run<K: Int8>(self) -> Self::Output;
}

/// `work` on `Avx2Vnni`.
///
/// # Panics
/// If the CPU lacks it ([`Backend::is_available`]); an int8 model holds `Avx2Vnni` only where the
/// CPU has it.
pub(crate) fn run_int8<R: RunInt8>(work: R) -> R::Output {
    #[cfg(target_arch = "x86_64")]
    if vnni::has_avx2_vnni() {
        // SAFETY: the CPU has AVX2, FMA and AVX-VNNI, which is all `run_vnni` is compiled with.
        return unsafe { vnni::run_vnni(work) };
    }
    let _ = work;
    panic!("the backend avx2-vnni is not available on this target or CPU")
}

/// `N` f32 lanes (4, or 8 on AVX2).
pub(crate) trait Lanes {
    /// The lanes of a vector.
    const N: usize;
    /// The floats that [`gemm`](crate::kernel) keeps per element of its broadcast activations:
    /// `N` when a backend loads a stored broadcast (a broadcast from a register is a shuffle on
    /// SSE), 1 when it broadcasts from memory in one load (`vbroadcastss` on AVX), which reads
    /// the activations in place.
    const SPLAT: usize;
    /// The option rows of the matrix micro-kernel: 4 (8 accumulators of the 16 XMM registers) or
    /// 6 (12 of the 16 YMM registers).
    const MR: usize;
    /// One vector of `N` lanes.
    type V: Copy;
    /// The 4-lane backend of the readout, whose horizontal sum is the same on every backend.
    type Readout: Readout;
    /// All lanes zero.
    fn zero() -> Self::V;
    /// All lanes `x`.
    fn splat(x: f32) -> Self::V;
    /// `src[0..N]`. Panics if `src` is shorter.
    fn load(src: &[f32]) -> Self::V;
    /// All lanes `src[0]`, from `src[0..SPLAT]` as `gemm` stores a broadcast. Panics if `src` is
    /// shorter.
    fn load_splat(src: &[f32]) -> Self::V;
    /// Writes `dst[0..N]`. Panics if `dst` is shorter.
    fn store(dst: &mut [f32], v: Self::V);
    /// Lane-wise `a + b`.
    fn add(a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `a * b`.
    fn mul(a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `acc + a * b`: the multiply, then the add, on every backend but `Avx2Fma`,
    /// which fuses them.
    #[inline(always)]
    fn mul_add(acc: Self::V, a: Self::V, b: Self::V) -> Self::V {
        Self::add(acc, Self::mul(a, b))
    }
    /// Lane-wise `x > 0 ? x : 0`.
    fn relu(v: Self::V) -> Self::V;
}

/// Four lanes with the horizontal sum of the readout.
pub(crate) trait Readout: Lanes {
    /// `(l0 + l1) + (l2 + l3)`.
    fn hsum(v: Self::V) -> f32;
}

/// One cache line of floats.
#[derive(Clone, Copy)]
#[repr(C, align(64))]
struct Line([f32; 16]);

/// A buffer of f32 whose first element is 64-byte aligned. The model file is copied into it once
/// at load, and every section of the model is a slice of it.
pub(crate) struct AlignedF32 {
    lines: Vec<Line>,
    len: usize,
}

impl AlignedF32 {
    /// The little-endian floats of `bytes`, whose length must be a multiple of 4.
    pub(crate) fn from_le_bytes(bytes: &[u8]) -> Self {
        assert!(bytes.len() % 4 == 0, "a whole number of floats");
        let len = bytes.len() / 4;
        let mut lines = vec![Line([0.0; 16]); len.div_ceil(16)];
        for (index, chunk) in bytes.chunks_exact(4).enumerate() {
            lines[index / 16].0[index % 16] =
                f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        Self { lines, len }
    }

    /// The bytes `range` as `i8`: an int8 section, which `from_le_bytes` read as floats. On a
    /// little-endian target (every target with `Avx2Vnni`, the only reader of these sections)
    /// those are the bytes of the file.
    pub(crate) fn i8s(&self, range: std::ops::Range<usize>) -> &[i8] {
        // SAFETY: the floats are `4 * len` initialised bytes, any byte is a valid `i8`, and the
        // slice borrows `self`.
        let bytes =
            unsafe { std::slice::from_raw_parts(self.lines.as_ptr().cast::<i8>(), self.len * 4) };
        &bytes[range]
    }

    /// The floats.
    pub(crate) fn as_slice(&self) -> &[f32] {
        // SAFETY: `Line` is `repr(C)` around `[f32; 16]` with no padding (64 bytes, align 64), so
        // `lines` is `16 * lines.len()` contiguous, initialised f32, and `len` is at most that.
        // The slice borrows `self`, so the vector cannot move or shrink while it lives.
        unsafe { std::slice::from_raw_parts(self.lines.as_ptr().cast::<f32>(), self.len) }
    }
}

impl std::fmt::Debug for AlignedF32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AlignedF32({} floats)", self.len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_aligned_buffer_is_aligned_and_holds_the_floats() {
        let values: Vec<f32> = (0..37).map(|i| i as f32 * 0.5 - 3.0).collect();
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let buffer = AlignedF32::from_le_bytes(&bytes);
        assert_eq!(buffer.as_slice(), values.as_slice());
        assert_eq!(buffer.as_slice().as_ptr() as usize % 64, 0);
        assert!(AlignedF32::from_le_bytes(&[]).as_slice().is_empty());
    }
}
