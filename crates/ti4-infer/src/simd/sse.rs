//! The x86-64 backend: `core::arch::x86_64` `_mm_*_ps`. SSE2 is the baseline of x86-64, so the
//! instructions are always there and nothing is detected at run time. (The intrinsics are still
//! `unsafe` to call outside a `#[target_feature]` function.)

use core::arch::x86_64::{
    __m128, _mm_add_ps, _mm_add_ss, _mm_cvtss_f32, _mm_loadu_ps, _mm_max_ps, _mm_movehl_ps,
    _mm_mul_ps, _mm_set1_ps, _mm_setzero_ps, _mm_shuffle_ps, _mm_storeu_ps,
};

use super::{Lanes, Readout};

/// Four f32 in an XMM register.
pub(crate) struct Sse;

impl Lanes for Sse {
    const N: usize = 4;
    const SPLAT: usize = 4;
    const MR: usize = 6;
    type V = __m128;
    type Readout = Self;

    #[inline(always)]
    fn zero() -> Self::V {
        // SAFETY: SSE is part of the x86-64 baseline.
        unsafe { _mm_setzero_ps() }
    }

    #[inline(always)]
    fn splat(x: f32) -> Self::V {
        // SAFETY: SSE is part of the x86-64 baseline.
        unsafe { _mm_set1_ps(x) }
    }

    #[inline(always)]
    fn load(src: &[f32]) -> Self::V {
        let src = &src[..4];
        // SAFETY: `src` has four floats; the load is unaligned.
        unsafe { _mm_loadu_ps(src.as_ptr()) }
    }

    #[inline(always)]
    fn load_splat(src: &[f32]) -> Self::V {
        Self::load(src)
    }

    #[inline(always)]
    fn store(dst: &mut [f32], v: Self::V) {
        let dst = &mut dst[..4];
        // SAFETY: `dst` has room for four floats; the store is unaligned.
        unsafe { _mm_storeu_ps(dst.as_mut_ptr(), v) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        // SAFETY: SSE is part of the x86-64 baseline.
        unsafe { _mm_add_ps(a, b) }
    }

    #[inline(always)]
    fn mul(a: Self::V, b: Self::V) -> Self::V {
        // SAFETY: SSE is part of the x86-64 baseline.
        unsafe { _mm_mul_ps(a, b) }
    }

    /// `maxps(x, 0)` is `x > 0 ? x : 0` lane by lane: it gives its second operand when the
    /// comparison is false, so −0 and NaN become +0, as in `Scalar`.
    #[inline(always)]
    fn relu(v: Self::V) -> Self::V {
        // SAFETY: SSE is part of the x86-64 baseline.
        unsafe { _mm_max_ps(v, _mm_setzero_ps()) }
    }
}

impl Readout for Sse {
    /// `[l1, l0, l3, l2]` added to `v` gives `l0 + l1` in lane 0 and `l2 + l3` in lane 2 (an f32
    /// add is commutative bit for bit); then lane 0 plus lane 2.
    #[inline(always)]
    fn hsum(v: Self::V) -> f32 {
        // SAFETY: SSE is part of the x86-64 baseline.
        unsafe {
            let pairs = _mm_add_ps(v, _mm_shuffle_ps::<0b10_11_00_01>(v, v));
            _mm_cvtss_f32(_mm_add_ss(pairs, _mm_movehl_ps(pairs, pairs)))
        }
    }
}
