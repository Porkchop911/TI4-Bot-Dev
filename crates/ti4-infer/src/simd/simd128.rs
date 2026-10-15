//! The wasm backend: `core::arch::wasm32` `f32x4_*`, built with `target-feature=+simd128`
//! (`.cargo/config.toml`).

use core::arch::wasm32::{
    f32x4_add, f32x4_extract_lane, f32x4_mul, f32x4_pmax, f32x4_splat, v128, v128_load, v128_store,
};

use super::Lanes;

/// Four f32 in a `v128`.
pub(crate) struct Simd128;

impl Lanes for Simd128 {
    type V = v128;

    #[inline(always)]
    fn zero() -> Self::V {
        f32x4_splat(0.0)
    }

    #[inline(always)]
    fn splat(x: f32) -> Self::V {
        f32x4_splat(x)
    }

    #[inline(always)]
    fn load(src: &[f32]) -> Self::V {
        let src = &src[..4];
        // SAFETY: `src` has four floats (16 bytes); `v128.load` takes any alignment.
        unsafe { v128_load(src.as_ptr().cast::<v128>()) }
    }

    #[inline(always)]
    fn store(dst: &mut [f32], v: Self::V) {
        let dst = &mut dst[..4];
        // SAFETY: `dst` has room for four floats (16 bytes); `v128.store` takes any alignment.
        unsafe { v128_store(dst.as_mut_ptr().cast::<v128>(), v) }
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        f32x4_add(a, b)
    }

    #[inline(always)]
    fn mul(a: Self::V, b: Self::V) -> Self::V {
        f32x4_mul(a, b)
    }

    /// `pmax(0, x)` is `0 < x ? x : 0` lane by lane, so −0 and NaN become +0, as in `Scalar`.
    /// (`f32x4_max` is not: it propagates NaN and orders −0 below +0.)
    #[inline(always)]
    fn relu(v: Self::V) -> Self::V {
        f32x4_pmax(f32x4_splat(0.0), v)
    }

    #[inline(always)]
    fn hsum(v: Self::V) -> f32 {
        (f32x4_extract_lane::<0>(v) + f32x4_extract_lane::<1>(v))
            + (f32x4_extract_lane::<2>(v) + f32x4_extract_lane::<3>(v))
    }
}
