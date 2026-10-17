//! The backends give the same bits: each lane operation on awkward values, and the whole forward
//! pass of every backend against `Scalar` on many shapes. `Avx2Fma` is the exception: its
//! multiply-add is fused, and its forward is within `1e-4 · max(1, |logit|)` of `Scalar`.
//!
//! A backend that this CPU does not have is skipped, with a message.

use crate::simd::{self, Lanes, Readout, Run};
use crate::synthetic::{self, Rng};
use crate::{Backend, Model, Scratch, format};

/// Eight lanes as bits: one vector of an 8-lane backend, two of a 4-lane one.
fn each<L: Lanes>(f: impl Fn(usize) -> L::V) -> [u32; 8] {
    let mut out = [0.0f32; 8];
    for start in (0..8).step_by(L::N) {
        L::store(&mut out[start..], f(start));
    }
    out.map(f32::to_bits)
}

/// Every operation of a backend, as bits, over some vectors of awkward values.
struct Operations<'a>(&'a [[f32; 8]]);

impl Run for Operations<'_> {
    type Output = Vec<[u32; 8]>;

    #[inline(always)]
    fn run<L: Lanes>(self) -> Self::Output {
        let mut out = vec![each::<L>(|_| L::zero()), each::<L>(|_| L::splat(-0.0))];
        for a in self.0 {
            for b in self.0 {
                out.push(each::<L>(|i| L::add(L::load(&a[i..]), L::load(&b[i..]))));
                out.push(each::<L>(|i| L::mul(L::load(&a[i..]), L::load(&b[i..]))));
                out.push(each::<L>(|i| {
                    L::mul_add(L::load(&b[i..]), L::load(&a[i..]), L::load(&b[i..]))
                }));
            }
            out.push(each::<L>(|i| L::relu(L::load(&a[i..]))));
            out.push(each::<L>(|_| L::load_splat(&[a[0]; 8])));
            let quad = |half: &[f32]| L::Readout::hsum(L::Readout::load(half)).to_bits();
            let (low, high) = (quad(&a[..4]), quad(&a[4..]));
            out.push([low, high, low, high, low, high, low, high]);
        }
        out
    }
}

/// `acc + a · b` with `a · b` not exact in f32: zero when the multiply rounds before the add,
/// 2⁻²⁴ when the two are fused.
struct Residue;

impl Run for Residue {
    type Output = f32;

    #[inline(always)]
    fn run<L: Lanes>(self) -> f32 {
        // (1 + 2⁻¹²)² = 1 + 2⁻¹¹ + 2⁻²⁴, which rounds (to even) to 1 + 2⁻¹¹.
        let a = L::splat(1.0 + 1.0 / 4096.0);
        let mut out = [0.0f32; 8];
        L::store(&mut out, L::mul_add(L::splat(-(1.0 + 1.0 / 2048.0)), a, a));
        out[0]
    }
}

fn skipped(backend: Backend) -> bool {
    let skip = !backend.is_available();
    if skip {
        eprintln!(
            "skipped: this target or CPU has no {} backend",
            backend.name()
        );
    }
    skip
}

/// Finite values (and their overflows): which NaN an operation on a NaN gives is not specified
/// in Rust, and the model refuses a weight or a feature that is not finite. The ReLU of NaN is
/// defined (+0), so one vector has a NaN.
#[test]
fn every_lane_operation_gives_the_bits_of_scalar() {
    let values = [
        [0.0, -0.0, 1.0, -1.0, 1e8, 1.0, -1e8, 1.0],
        [
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            1e-45,
            -1e-45,
            1e30,
            -1e30,
            3.0e38,
            0.1,
        ],
        [0.3, 1e-8, -0.7, 16_777_217.0, -2.5, 7.0, 1e-3, -1e-20],
    ];
    let scalar = simd::run(Backend::Scalar, Operations(&values));
    for backend in Backend::ALL {
        if !backend.is_deterministic() || skipped(backend) {
            continue;
        }
        let ops = simd::run(backend, Operations(&values));
        assert_eq!(ops, scalar, "{}", backend.name());
        let relu = [-0.0, f32::NAN, -2.0, 2.0, -1.0, 1.0, f32::NAN, -0.0];
        let relu = simd::run(backend, Operations(&[relu]))[2 + 3];
        assert_eq!(relu, [0, 0, 0, 2.0f32.to_bits(), 0, 1.0f32.to_bits(), 0, 0]);
    }
    // The horizontal sum is `(l0 + l1) + (l2 + l3)`, not `(l0 + l2) + (l1 + l3)`.
    let hsum = scalar[2 + 3 * 3 + 2];
    assert_eq!(f32::from_bits(hsum[1]), (1e8f32 + 1.0) + (-1e8f32 + 1.0));
    // Every multiply-add but that of `Avx2Fma` rounds the product first.
    for backend in Backend::ALL {
        if skipped(backend) {
            continue;
        }
        let residue = simd::run(backend, Residue);
        let fused = if backend.is_deterministic() {
            0.0
        } else {
            1.0 / 16_777_216.0
        };
        assert_eq!(residue, fused, "{}", backend.name());
    }
}

/// Widths 128 and 256 and two that use the short blocks of the bag and an odd number of panels
/// (24 and 40), 0 to 2 residual blocks, and decisions of 1 to 37 options: every tail of the
/// micro-kernels of four and six rows, alone and after whole blocks of rows. `tolerance` `None`
/// asks for the same bits, else `|logit − scalar| ≤ tolerance · max(1, |scalar|)`.
fn agrees_with_scalar(backend: Backend, tolerance: Option<f32>) {
    let counts = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 22, 37];
    let mut rng = Rng::new(99);
    let mut scratch = Scratch::new();
    let (mut logits, mut scalar) = (Vec::new(), Vec::new());
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
                    .forward_on(backend, &mut scratch, faction, head, &options, &mut logits)
                    .unwrap();
                model
                    .forward_on(
                        Backend::Scalar,
                        &mut scratch,
                        faction,
                        head,
                        &options,
                        &mut scalar,
                    )
                    .unwrap();
                let shape = format!("width {width}, {blocks} blocks, {count} options");
                assert!(logits.iter().all(|logit| logit.is_finite()), "{shape}");
                match tolerance {
                    None => assert_eq!(bits(&logits), bits(&scalar), "{shape}"),
                    Some(tolerance) => {
                        for (logit, reference) in logits.iter().zip(&scalar) {
                            let bound = tolerance * reference.abs().max(1.0);
                            assert!(
                                (logit - reference).abs() <= bound,
                                "{shape}: {logit} against {reference}"
                            );
                        }
                    }
                }
            }
        }
    }
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

#[test]
fn the_sse_forward_is_the_scalar_forward_bit_for_bit() {
    if !skipped(Backend::Sse) {
        agrees_with_scalar(Backend::Sse, None);
    }
}

#[test]
fn the_avx2_forward_is_the_scalar_forward_bit_for_bit() {
    if !skipped(Backend::Avx2) {
        agrees_with_scalar(Backend::Avx2, None);
    }
}

#[test]
fn the_avx2_fma_forward_is_within_its_bound_of_the_scalar_forward() {
    if !skipped(Backend::Avx2Fma) {
        agrees_with_scalar(Backend::Avx2Fma, Some(1e-4));
    }
}

#[test]
fn a_model_is_loaded_on_the_best_deterministic_backend_and_refuses_one_it_lacks() {
    let parts = synthetic::parts(5, 128, 100, 1);
    let bytes = format::write(&synthetic::meta(), &parts).unwrap();
    let model = Model::from_bytes(&bytes).unwrap();
    assert_eq!(model.backend(), Backend::best());
    assert_eq!(crate::backend(), Backend::best());
    assert!(Backend::best().is_deterministic());
    if cfg!(target_arch = "x86_64") {
        let expected = if Backend::Avx2.is_available() {
            Backend::Avx2
        } else {
            Backend::Sse
        };
        assert_eq!(Backend::best(), expected);
    }
    let model = model.with_backend(Backend::Scalar).unwrap();
    assert_eq!(model.backend(), Backend::Scalar);
    assert!(
        matches!(
            model.with_backend(Backend::Simd128),
            Err(crate::InferError::Backend(Backend::Simd128))
        ) || cfg!(target_arch = "wasm32")
    );
    assert_eq!(
        Backend::fastest(),
        if Backend::Avx2Fma.is_available() {
            Backend::Avx2Fma
        } else {
            Backend::best()
        }
    );
}

/// An int8 model of `parts` on `Avx2Vnni`.
fn int8_model(parts: &crate::Parts) -> Model {
    let bytes = format::write_as(&synthetic::meta(), parts, crate::Quant::Int8).unwrap();
    let model = Model::from_bytes(&bytes).expect("an int8 file reads on avx2-vnni");
    assert_eq!(model.backend(), Backend::Avx2Vnni);
    assert_eq!(model.quant(), crate::Quant::Int8);
    model
}

/// The int8 forward of `Avx2Vnni` against `quant::reference`, plain loops over the unpacked
/// quantized weights (the integer dot of each output, its zero-point correction, the scales and
/// the f32 epilogue): the same bits, on the shapes of `agrees_with_scalar` (every tail of the
/// micro-kernel, an odd number of panels, the short blocks of the bag and of the quantization).
#[test]
fn the_int8_forward_is_the_plain_integer_reference_bit_for_bit() {
    if skipped(Backend::Avx2Vnni) {
        return;
    }
    let counts = [1, 2, 3, 5, 6, 7, 11, 12, 13, 37, 50];
    let mut rng = Rng::new(808);
    let mut scratch = Scratch::new();
    let mut logits = Vec::new();
    for width in [24, 40, 128, 256] {
        for blocks in 0..=2 {
            let capacity = 64 + rng.below(400);
            let parts = synthetic::parts((width * 10 + blocks) as u64, width, capacity, blocks);
            let model = int8_model(&parts);
            for count in counts {
                let options = synthetic::options(rng.next_u64(), count, capacity, 60);
                let (faction, head) = (rng.below(33), rng.below(14));
                model
                    .forward_with(&mut scratch, faction, head, &options, &mut logits)
                    .unwrap();
                let reference = crate::quant::reference(&parts, faction, head, &options);
                let shape = format!("width {width}, {blocks} blocks, {count} options");
                assert!(logits.iter().all(|logit| logit.is_finite()), "{shape}");
                assert_eq!(bits(&logits), bits(&reference), "{shape}");
            }
        }
    }
}

/// The int8 model against the f32 one of the same weights (`Scalar`), on the synthetic models at
/// width 128 and 256 with 0 to 2 residual blocks: every logit within `INT8_BOUND · max(1,
/// |logit|)` (section H of the plan; the worst measured is printed).
#[test]
fn the_int8_forward_is_within_its_bound_of_the_f32_forward() {
    if skipped(Backend::Avx2Vnni) {
        return;
    }
    let mut rng = Rng::new(1_015);
    let mut scratch = Scratch::new();
    let (mut logits, mut f32_logits) = (Vec::new(), Vec::new());
    let mut worst = 0.0f32;
    let (mut sum_error, mut sum_logit, mut count_all) = (0.0f64, 0.0f64, 0usize);
    for width in [128, 256] {
        for blocks in 0..=2 {
            let capacity = 500 + rng.below(2_000);
            let parts = synthetic::parts(rng.next_u64(), width, capacity, blocks);
            let int8 = int8_model(&parts);
            let f32_model =
                Model::from_bytes(&format::write(&synthetic::meta(), &parts).unwrap()).unwrap();
            for count in [1, 6, 40] {
                let options = synthetic::options(rng.next_u64(), count, capacity, 85);
                let (faction, head) = (rng.below(33), rng.below(14));
                int8.forward_with(&mut scratch, faction, head, &options, &mut logits)
                    .unwrap();
                f32_model
                    .forward_on(
                        Backend::Scalar,
                        &mut scratch,
                        faction,
                        head,
                        &options,
                        &mut f32_logits,
                    )
                    .unwrap();
                for (logit, reference) in logits.iter().zip(&f32_logits) {
                    let error = (logit - reference).abs() / reference.abs().max(1.0);
                    sum_error += f64::from((logit - reference).abs());
                    sum_logit += f64::from(reference.abs());
                    count_all += 1;
                    worst = worst.max(error);
                    assert!(
                        error <= INT8_BOUND,
                        "width {width}, {blocks} blocks: int8 {logit}, f32 {reference}"
                    );
                }
            }
        }
    }
    println!(
        "int8 against f32 on the synthetic models: worst |gap| / max(1, |logit|) {worst:.2e}, \
         mean |gap| {:.2e}, mean |logit| {:.2e}",
        sum_error / count_all as f64,
        sum_logit / count_all as f64
    );
}

/// The bound of an int8 logit against the f32 one, relative to `max(1, |logit|)`.
const INT8_BOUND: f32 = 0.05;

#[test]
fn an_int8_model_runs_on_avx2_vnni_alone_and_an_f32_model_never_does() {
    let parts = synthetic::parts(5, 128, 100, 1);
    let int8 = format::write_as(&synthetic::meta(), &parts, crate::Quant::Int8).unwrap();
    let f32_bytes = format::write(&synthetic::meta(), &parts).unwrap();
    let f32_model = Model::from_bytes(&f32_bytes).unwrap();
    assert_eq!(f32_model.quant(), crate::Quant::F32);
    assert!(matches!(
        f32_model.with_backend(Backend::Avx2Vnni),
        Err(crate::InferError::Quant { .. })
    ));
    if !Backend::Avx2Vnni.is_available() {
        assert!(matches!(
            Model::from_bytes(&int8),
            Err(crate::InferError::Backend(Backend::Avx2Vnni))
        ));
        return;
    }
    let model = Model::from_bytes(&int8).unwrap();
    let options = synthetic::options(3, 4, 100, 20);
    let mut scratch = Scratch::new();
    let mut out = Vec::new();
    for backend in [Backend::Scalar, Backend::Avx2Fma] {
        assert!(matches!(
            model.forward_on(backend, &mut scratch, 0, 0, &options, &mut out),
            Err(crate::InferError::Quant { .. })
        ));
    }
    assert!(matches!(
        model.with_backend(Backend::Avx2),
        Err(crate::InferError::Quant { .. })
    ));
    // The value head of training is the f32 readout on the f32 rows of the int8 trunk: the logit
    // of the position as a decision of one option (a decision of several quantizes its rows as a
    // base and deltas, so a row's logit depends on the others).
    let model = Model::from_bytes(&int8).unwrap();
    let heads = parts.heads.len();
    let (faction, head) = (4, 2);
    let at = faction * heads + head;
    let w: Vec<f32> = (0..128)
        .map(|i| parts.w_shared[head * 128 + i] + parts.delta[at * 128 + i])
        .collect();
    let b = parts.b_shared[head] + parts.b_delta[at];
    let model = model.with_value(w, b).unwrap();
    for option in &options {
        let logit = model
            .forward(faction, head, std::slice::from_ref(option))
            .unwrap()[0];
        let value = model.value_with(&mut scratch, faction, option).unwrap();
        assert_eq!(value.to_bits(), logit.to_bits());
    }
}
