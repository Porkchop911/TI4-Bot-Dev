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
}

impl Backend {
    /// Every backend, the deterministic ones from the slowest to the fastest, then `Avx2Fma`.
    pub const ALL: [Self; 5] = [
        Self::Scalar,
        Self::Sse,
        Self::Simd128,
        Self::Avx2,
        Self::Avx2Fma,
    ];

    /// Its name in a report: `"scalar"`, `"sse"`, `"simd128"`, `"avx2"` or `"avx2-fma"`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Sse => "sse",
            Self::Simd128 => "simd128",
            Self::Avx2 => "avx2",
            Self::Avx2Fma => "avx2-fma",
        }
    }

    /// Whether its logits are the bits of [`Backend::Scalar`] (all but `Avx2Fma`).
    #[must_use]
    pub const fn is_deterministic(self) -> bool {
        !matches!(self, Self::Avx2Fma)
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
            #[cfg(not(target_arch = "x86_64"))]
            Self::Avx2 | Self::Avx2Fma => false,
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

    /// The fastest backend available, deterministic or not: `Avx2Fma` where the CPU has it, else
    /// [`Backend::best`]. For hosts that no other host must reproduce (the rollouts of training,
    /// a server with `TI4_INFER_FMA=1`).
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
        _ => panic!(
            "the backend {} is not available on this target or CPU",
            backend.name()
        ),
    }
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
