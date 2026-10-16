//! The AVX2 backends of x86-64: `core::arch::x86_64` `_mm256_*_ps`, 8 lanes.
//!
//! - `Avx2` multiplies, then adds, so it gives the bits of `Scalar`: in the bag and the matrix
//!   product each lane is its own output, and the readout runs on `Sse`.
//! - `Avx2Fma` fuses the multiply-add of the bag and the matrix product (`_mm256_fmadd_ps`). Its
//!   readout runs on `Sse` too, unfused.
//!
//! The CPU has these instructions only if `is_x86_feature_detected!` says so. Both types are
//! private to `simd`, and the only path to them is [`run_avx2`] and [`run_avx2_fma`], which
//! [`super::run`] calls after that check. So every lane operation below runs on a CPU with AVX2
//! (and FMA for `Avx2Fma`), which is the safety argument of each `unsafe` block.

use core::arch::x86_64::{
    __m256, _mm256_add_ps, _mm256_broadcast_ss, _mm256_fmadd_ps, _mm256_loadu_ps, _mm256_max_ps,
    _mm256_mul_ps, _mm256_set1_ps, _mm256_setzero_ps, _mm256_storeu_ps,
};

use super::{Lanes, Run, Sse};

/// Whether the CPU has AVX2.
pub(super) fn has_avx2() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}

/// Whether the CPU has AVX2 and FMA.
pub(super) fn has_avx2_fma() -> bool {
    has_avx2() && std::arch::is_x86_feature_detected!("fma")
}

/// `work` on `Avx2`, compiled with AVX2: the work and everything it calls are inlined here.
#[target_feature(enable = "avx2")]
pub(super) fn run_avx2<R: Run>(work: R) -> R::Output {
    work.run::<Avx2>()
}

/// `work` on `Avx2Fma`, compiled with AVX2 and FMA.
#[target_feature(enable = "avx2,fma")]
pub(super) fn run_avx2_fma<R: Run>(work: R) -> R::Output {
    work.run::<Avx2Fma>()
}

/// Eight f32 in a YMM register, a multiply then an add.
pub(super) struct Avx2;

impl Lanes for Avx2 {
    const N: usize = 8;
    const SPLAT: usize = 1;
    const MR: usize = 6;
    type V = __m256;
    type Readout = Sse;

    #[inline(always)]
    fn zero() -> Self::V {
        // SAFETY: reached only on a CPU with AVX2 (the module).
        unsafe { _mm256_setzero_ps() }
    }

    #[inline(always)]
    fn splat(x: f32) -> Self::V {
        // SAFETY: reached only on a CPU with AVX2 (the module).
        unsafe { _mm256_set1_ps(x) }
    }

    #[inline(always)]
    fn load(src: &[f32]) -> Self::V {
        let src = &src[..8];
        // SAFETY: `src` has eight floats; the load is unaligned; the CPU has AVX2 (the module).
        unsafe { _mm256_loadu_ps(src.as_ptr()) }
    }

    /// `vbroadcastss` from memory: one load, no shuffle.
    #[inline(always)]
    fn load_splat(src: &[f32]) -> Self::V {
        // SAFETY: a reference to one float; the CPU has AVX2 (the module).
        unsafe { _mm256_broadcast_ss(&src[0]) }
    }

    #[inline(always)]
    fn store(dst: &mut [f32], v: Self::V) {
        let dst = &mut dst[..8];
        // SAFETY: `dst` has room for eight floats; the store is unaligned; the CPU has AVX2.
        unsafe { _mm256_storeu_ps(dst.as_mut_ptr(), v) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        // SAFETY: reached only on a CPU with AVX2 (the module).
        unsafe { _mm256_add_ps(a, b) }
    }

    #[inline(always)]
    fn mul(a: Self::V, b: Self::V) -> Self::V {
        // SAFETY: reached only on a CPU with AVX2 (the module).
        unsafe { _mm256_mul_ps(a, b) }
    }

    /// `vmaxps(x, 0)` gives its second operand when `x > 0` is false, so −0 and NaN become +0, as
    /// in `Scalar`.
    #[inline(always)]
    fn relu(v: Self::V) -> Self::V {
        // SAFETY: reached only on a CPU with AVX2 (the module).
        unsafe { _mm256_max_ps(v, _mm256_setzero_ps()) }
    }
}

/// Eight f32 in a YMM register, the multiply-add fused.
pub(super) struct Avx2Fma;

impl Lanes for Avx2Fma {
    const N: usize = 8;
    const SPLAT: usize = 1;
    const MR: usize = 6;
    type V = __m256;
    type Readout = Sse;

    #[inline(always)]
    fn zero() -> Self::V {
        Avx2::zero()
    }

    #[inline(always)]
    fn splat(x: f32) -> Self::V {
        Avx2::splat(x)
    }

    #[inline(always)]
    fn load(src: &[f32]) -> Self::V {
        Avx2::load(src)
    }

    #[inline(always)]
    fn load_splat(src: &[f32]) -> Self::V {
        Avx2::load_splat(src)
    }

    #[inline(always)]
    fn store(dst: &mut [f32], v: Self::V) {
        Avx2::store(dst, v);
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        Avx2::add(a, b)
    }

    #[inline(always)]
    fn mul(a: Self::V, b: Self::V) -> Self::V {
        Avx2::mul(a, b)
    }

    /// `acc + a · b` rounded once.
    #[inline(always)]
    fn mul_add(acc: Self::V, a: Self::V, b: Self::V) -> Self::V {
        // SAFETY: reached only on a CPU with AVX2 and FMA (the module).
        unsafe { _mm256_fmadd_ps(a, b, acc) }
    }

    #[inline(always)]
    fn relu(v: Self::V) -> Self::V {
        Avx2::relu(v)
    }
}
