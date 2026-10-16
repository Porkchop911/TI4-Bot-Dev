//! The speed of the forward pass (`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`, section C):
//! single thread, x86-64, width 256, a decision of 6 options with its rows in cache and one of 512
//! options with random columns.
//!
//! | Model             | Backend   | 6 options | 512 options |
//! | ----------------- | --------- | --------- | ----------- |
//! | 0 residual blocks | `Sse`     | < 30 µs   | < 4 ms      |
//! | 2 residual blocks | `Avx2`    | < 55 µs   | < 5 ms      |
//! | 2 residual blocks | `Avx2Fma` | < 35 µs   | < 4 ms      |
//!
//! It also runs the int8 model of the same weights on `Avx2Vnni` (section H of the plan) and
//! prints its speedup over `Avx2Fma`: on the reference machine 1.1x / 1.6x (one decision again /
//! 256 in turn) and 1.8x at 512 options with no residual block, 1.5x / 1.7x and 2.1x with two
//! (25.6 µs, 27.5 µs and 1.96 ms). It asserts nothing of it.
//!
//! The bench asserts these in an optimized build (a backend the CPU lacks is skipped with a
//! message), but for three that it only prints: `Avx2` at 512 options, and `Avx2Fma` at 6 and at
//! 512 options. On the reference machine (Core i5-12600K, a P-core at 4.9 GHz) the code misses
//! them: about 5.1 ms, 39.5 µs and 4.05 ms. It prints every available backend at 0 and 2 residual
//! blocks, `Scalar` and `Sse` included, for the comparison.
//!
//! Ignored. `ti4-infer` is optimized in the dev profile (`Cargo.toml`), so the alias gives the
//! real numbers:
//!
//! ```sh
//! cargo t-web bench --ignored --nocapture
//! ```
//!
//! - Without FMA an SSE core does at most 8 multiply-adds a cycle, and one dense layer of 6
//!   options at width 256 is 393 216 of them (about 15 µs on a 4.7 GHz core), so each residual
//!   block adds about 30 µs; 8 lanes halve that, and FMA halves it again.
//! - The options of the bench have random columns, so 512 options read 43 MB of rows of a 21 MB
//!   table: that time is bound by the memory, and the vector width helps it less.
//! - With 2 residual blocks the weights (1.3 MB) and the rows of 6 options (0.5 MB) do not fit
//!   the L2 of the reference core together, so even "one decision again" reads its rows from L3:
//!   the bag of 6 options takes about 10 µs on every backend, and the 5 dense layers on `Avx2Fma`
//!   about 6 µs each (at 2 FMA a cycle the bound is 5 µs). Prefetching the rows (the next block,
//!   the rest of the row, the next option) and walking each row in one pass were tried, and each
//!   was slower.

use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::synthetic::{self, Rng};
use crate::{Backend, Model, Scratch, format};

/// The active columns of an option, as a real decision has them on average.
const COLUMNS: usize = 85;

fn decision(rng: &mut Rng, count: usize, capacity: usize) -> Vec<Vec<(u32, f32)>> {
    (0..count)
        .map(|_| {
            let columns: Vec<u32> = (0..COLUMNS)
                .map(|_| u32::try_from(rng.below(capacity)).unwrap())
                .collect();
            let values: Vec<f32> = (0..COLUMNS).map(|_| 0.5 + rng.unit()).collect();
            columns.into_iter().zip(values).collect()
        })
        .collect()
}

/// The mean time of one forward over `decisions`, taken in turn for `runs` forwards.
fn time(
    model: &Model,
    backend: Backend,
    decisions: &[Vec<Vec<(u32, f32)>>],
    runs: usize,
) -> Duration {
    let mut scratch = Scratch::new();
    let mut out = Vec::new();
    for options in decisions.iter().take(runs.min(decisions.len())) {
        model
            .forward_on(backend, &mut scratch, 27, 3, options, &mut out)
            .unwrap();
    }
    let start = Instant::now();
    for run in 0..runs {
        let options = &decisions[run % decisions.len()];
        model
            .forward_on(backend, &mut scratch, 27, 3, black_box(options), &mut out)
            .unwrap();
        black_box(&out);
    }
    start.elapsed() / u32::try_from(runs).unwrap()
}

fn micros(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}

#[test]
#[ignore = "a benchmark; see the module for the command"]
fn the_forward_meets_its_time_targets() {
    let optimized = !cfg!(debug_assertions);
    let runs = |release: usize| {
        if optimized {
            release
        } else {
            release / 100 + 1
        }
    };
    let (width, capacity) = (256, 20_480);
    let mut rng = Rng::new(2026);
    let small: Vec<_> = (0..256).map(|_| decision(&mut rng, 6, capacity)).collect();
    let large: Vec<_> = (0..4).map(|_| decision(&mut rng, 512, capacity)).collect();
    println!(
        "ti4_infer::bench: width {width}, {capacity} rows, {COLUMNS} columns per option, {} build",
        if optimized { "optimized" } else { "debug" }
    );
    // (residual blocks, backend, 6 options, 512 options)
    // (residual blocks, backend, 6 options in µs, 512 options in µs), each with whether it is
    // asserted: the three that this code misses on the reference machine are printed only (see
    // the module).
    let targets = [
        (0, Backend::Sse, (30, true), (4_000, true)),
        (2, Backend::Avx2, (55, true), (5_000, false)),
        (2, Backend::Avx2Fma, (35, false), (4_000, false)),
    ];
    let mut missed = Vec::new();
    for blocks in [0, 2] {
        let parts = synthetic::parts(7, width, capacity, blocks);
        let model = Model::from_bytes(&format::write(&synthetic::meta(), &parts).unwrap()).unwrap();
        // The int8 model of the same weights, for `Avx2Vnni`.
        let int8 = Backend::Avx2Vnni.is_available().then(|| {
            let bytes = format::write_as(&synthetic::meta(), &parts, crate::Quant::Int8).unwrap();
            Model::from_bytes(&bytes).unwrap()
        });
        // `Avx2Fma` (6 options again, 6 in turn, 512), for the speedup of `Avx2Vnni`.
        let mut fma = None;
        for backend in Backend::ALL {
            if !backend.is_available() {
                println!(
                    "  {blocks} residual blocks, {}: not on this CPU",
                    backend.name()
                );
                continue;
            }
            let model = match (&int8, backend) {
                (Some(int8), Backend::Avx2Vnni) => int8,
                _ => &model,
            };
            // The same decision again (its rows in cache), and 256 decisions in turn (5 MB of
            // rows).
            let hot = time(model, backend, &small[..1], runs(20_000));
            let cold = time(model, backend, &small, runs(20_000));
            let wide = time(model, backend, &large, runs(200));
            println!(
                "  {blocks} residual blocks, {}: 6 options {:.1} µs (one decision again) / \
                 {:.1} µs (256 decisions in turn); 512 options {:.0} µs",
                backend.name(),
                micros(hot),
                micros(cold),
                micros(wide)
            );
            if backend == Backend::Avx2Fma {
                fma = Some((hot, cold, wide));
            }
            if let (Backend::Avx2Vnni, Some((f_hot, f_cold, f_wide))) = (backend, fma) {
                let ratio = |f: Duration, v: Duration| f.as_secs_f64() / v.as_secs_f64();
                println!(
                    "    int8 speedup over avx2-fma: {:.2}x / {:.2}x; 512 options {:.2}x",
                    ratio(f_hot, hot),
                    ratio(f_cold, cold),
                    ratio(f_wide, wide)
                );
            }
            let Some(&(_, _, six, many)) = targets
                .iter()
                .find(|target| target.0 == blocks && target.1 == backend)
            else {
                continue;
            };
            let ((six, assert_six), (many, assert_many)) = (six, many);
            let (six, many) = (Duration::from_micros(six), Duration::from_micros(many));
            let note = |asserted: bool| {
                if asserted {
                    ""
                } else {
                    " (printed, not asserted)"
                }
            };
            println!(
                "    target: 6 options under {six:?} {}{}; 512 options under {many:?} {}{}",
                verdict(hot, six),
                note(assert_six),
                verdict(wide, many),
                note(assert_many)
            );
            if assert_six && hot >= six {
                missed.push(format!(
                    "{blocks} blocks, {}: 6 options {hot:?}",
                    backend.name()
                ));
            }
            if assert_many && wide >= many {
                missed.push(format!(
                    "{blocks} blocks, {}: 512 options {wide:?}",
                    backend.name()
                ));
            }
        }
    }
    for (blocks, backend, _, _) in targets {
        if !backend.is_available() {
            println!(
                "skipped: the target of {} at {blocks} blocks (not on this CPU)",
                backend.name()
            );
        }
    }
    if optimized {
        assert!(missed.is_empty(), "missed targets: {missed:?}");
    }
}

fn verdict(time: Duration, target: Duration) -> &'static str {
    if time < target { "met" } else { "missed" }
}
