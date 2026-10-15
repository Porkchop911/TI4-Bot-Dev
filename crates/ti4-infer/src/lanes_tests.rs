//! The backends give the same bits: each lane operation on awkward values, and the whole forward
//! pass of `Native` against `Scalar` on many shapes.

use crate::simd::{Lanes, Native, Scalar};
use crate::synthetic::{self, Rng};
use crate::{Model, Scratch, format};

fn lanes<L: Lanes>(v: L::V) -> [u32; 4] {
    let mut out = [0.0f32; 4];
    L::store(&mut out, v);
    out.map(f32::to_bits)
}

/// Every operation of `L`, as bits, over some vectors of awkward values.
fn operations<L: Lanes>(values: &[[f32; 4]]) -> Vec<[u32; 4]> {
    let mut out = vec![lanes::<L>(L::zero()), lanes::<L>(L::splat(-0.0))];
    for a in values {
        for b in values {
            let (x, y) = (L::load(a), L::load(b));
            out.push(lanes::<L>(L::add(x, y)));
            out.push(lanes::<L>(L::mul(x, y)));
        }
        let x = L::load(a);
        out.push(lanes::<L>(L::relu(x)));
        out.push([L::hsum(x).to_bits(); 4]);
    }
    out
}

/// Finite values (and their overflows): which NaN an operation on a NaN gives is not specified
/// in Rust, and the model refuses a weight or a feature that is not finite.
#[test]
fn every_lane_operation_gives_the_bits_of_scalar() {
    let values = [
        [0.0, -0.0, 1.0, -1.0],
        [f32::MIN_POSITIVE, -f32::MIN_POSITIVE, 1e-45, -1e-45],
        [1e30, -1e30, 3.0e38, 0.1],
        [0.3, 1e-8, -0.7, 16_777_217.0],
    ];
    assert_eq!(operations::<Native>(&values), operations::<Scalar>(&values));
    // The ReLU is `x > 0 ? x : 0`: −0 and NaN give +0.
    let relu = lanes::<Native>(Native::relu(Native::load(&[-0.0, f32::NAN, -2.0, 2.0])));
    assert_eq!(relu, [0, 0, 0, 2.0f32.to_bits()]);
    // The horizontal sum is `(l0 + l1) + (l2 + l3)`, not `(l0 + l2) + (l1 + l3)`.
    let v = Native::load(&[1e8, 1.0, -1e8, 1.0]);
    assert_eq!(Native::hsum(v), (1e8f32 + 1.0) + (-1e8f32 + 1.0));
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

/// Widths 128 and 256 and two that use the short blocks of the bag (24 and 40), 0 to 2 residual
/// blocks, and decisions of 1 to 37 options: every tail of the four-row micro-kernel, alone and
/// after whole blocks of rows.
#[test]
fn the_native_forward_is_the_scalar_forward_bit_for_bit() {
    let counts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 22, 37];
    let mut rng = Rng::new(99);
    let mut scratch = Scratch::new();
    let (mut native, mut scalar) = (Vec::new(), Vec::new());
    for width in [24, 40, 128, 256] {
        for blocks in 0..=2 {
            let seed = (width * 10 + blocks) as u64;
            let capacity = 64 + rng.below(400);
            let parts = synthetic::parts(seed, width, capacity, blocks);
            let model = Model::from_bytes(&format::write(&synthetic::meta(), &parts).unwrap())
                .expect("reads");
            for count in counts {
                let options = synthetic::options(rng.next_u64(), count, capacity, 60);
                let (faction, head) = (rng.below(33), rng.below(14));
                model
                    .forward_lanes::<Native>(&mut scratch, faction, head, &options, &mut native)
                    .unwrap();
                model
                    .forward_lanes::<Scalar>(&mut scratch, faction, head, &options, &mut scalar)
                    .unwrap();
                assert_eq!(
                    bits(&native),
                    bits(&scalar),
                    "width {width}, {blocks} blocks, {count} options"
                );
                assert!(native.iter().all(|logit| logit.is_finite()));
            }
        }
    }
}
