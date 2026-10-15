//! The portable backend: `[f32; 4]` with the lane arithmetic of every backend.

use super::Lanes;

/// Four f32 in an array. The backend of every target without SIMD kernels, and the reference the
/// others are tested against.
#[cfg_attr(
    any(
        target_arch = "x86_64",
        all(target_arch = "wasm32", target_feature = "simd128")
    ),
    allow(
        dead_code,
        reason = "with a SIMD backend, only the tests and the bench use it"
    )
)]
pub(crate) struct Scalar;

impl Lanes for Scalar {
    type V = [f32; 4];

    #[inline(always)]
    fn zero() -> Self::V {
        [0.0; 4]
    }

    #[inline(always)]
    fn splat(x: f32) -> Self::V {
        [x; 4]
    }

    #[inline(always)]
    fn load(src: &[f32]) -> Self::V {
        [src[0], src[1], src[2], src[3]]
    }

    #[inline(always)]
    fn store(dst: &mut [f32], v: Self::V) {
        dst[..4].copy_from_slice(&v);
    }

    #[inline(always)]
    fn add(a: Self::V, b: Self::V) -> Self::V {
        [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
    }

    #[inline(always)]
    fn mul(a: Self::V, b: Self::V) -> Self::V {
        [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
    }

    #[inline(always)]
    fn relu(v: Self::V) -> Self::V {
        v.map(|x| if x > 0.0 { x } else { 0.0 })
    }

    #[inline(always)]
    fn hsum(v: Self::V) -> f32 {
        (v[0] + v[1]) + (v[2] + v[3])
    }
}
