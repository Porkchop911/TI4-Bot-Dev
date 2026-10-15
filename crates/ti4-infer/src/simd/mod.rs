//! The lanes the kernels are written in, and their backends.
//!
//! Every backend has 4 × f32 lanes and the same arithmetic: a multiply, then an add (no FMA); the
//! ReLU `x > 0 ? x : 0`; and the horizontal sum `(l0 + l1) + (l2 + l3)`. So the kernels give the
//! same bits on every backend.
//!
//! The trait is safe: a load reads the first four floats of a slice and a store writes the first
//! four, and both panic on a shorter slice. A backend that uses intrinsics checks the length and
//! keeps its `unsafe` here, so the kernels have none.
//!
//! This module also holds [`AlignedF32`], the 64-byte-aligned buffer the sections of a model live
//! in. It and the intrinsic backends are the only code of the crate allowed `unsafe`.

#![allow(unsafe_code)]

mod scalar;

pub(crate) use scalar::Scalar;

/// The backend of this target. `Scalar` everywhere for now; SSE (x86-64) and simd128 (wasm) are
/// added beside it with the same results.
pub(crate) type Native = Scalar;

/// Four f32 lanes.
pub(crate) trait Lanes {
    /// One vector of four lanes.
    type V: Copy;
    /// All lanes zero.
    fn zero() -> Self::V;
    /// All lanes `x`.
    fn splat(x: f32) -> Self::V;
    /// `src[0..4]`. Panics if `src` is shorter.
    fn load(src: &[f32]) -> Self::V;
    /// Writes `dst[0..4]`. Panics if `dst` is shorter.
    fn store(dst: &mut [f32], v: Self::V);
    /// Lane-wise `a + b`.
    fn add(a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `a * b`.
    fn mul(a: Self::V, b: Self::V) -> Self::V;
    /// Lane-wise `x > 0 ? x : 0`.
    fn relu(v: Self::V) -> Self::V;
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
