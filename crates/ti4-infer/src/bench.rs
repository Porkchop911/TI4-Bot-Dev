//! The speed of the forward pass (`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`, section C):
//! single thread, x86-64, a decision of 6 options at width 256 in under 30 µs, and 512 options in
//! under 2 ms. It also prints `Scalar`, to show what the SIMD gains.
//!
//! Ignored. The aliases build without optimization, so run it with `ti4-infer` optimized (the
//! package set of `t-web`; only `ti4-infer` is built again):
//!
//! ```sh
//! cargo --config 'profile.dev.package.ti4-infer.opt-level=3' \
//!   --config 'profile.dev.package.ti4-infer.debug-assertions=false' \
//!   --config 'profile.dev.package.ti4-infer.overflow-checks=false' \
//!   t-web bench --ignored --nocapture
//! ```
//!
//! Each target is printed as met or missed for a model without residual blocks (the reviewer
//! checkpoint) and one with two. Only the target of 6 options without blocks is asserted, in an
//! optimized build, with the rows of the decision in cache:
//!
//! - Without FMA an SSE core does at most 8 multiply-adds a cycle, and one dense layer of 6
//!   options at width 256 is 393 216 of them (about 15 µs on a 4.7 GHz core), so each residual
//!   block adds about 30 µs.
//! - The options of the bench have random columns, so 512 options read 43 MB of rows of a 21 MB
//!   table: that time is bound by the memory.

use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::simd::{Lanes, Native, Scalar};
use crate::synthetic::{self, Rng};
use crate::{Model, Scratch, SparseOption, format};

/// The active columns of an option, as a real decision has them on average.
const COLUMNS: usize = 85;

fn decision(rng: &mut Rng, count: usize, capacity: usize) -> Vec<SparseOption> {
    (0..count)
        .map(|_| SparseOption {
            columns: (0..COLUMNS)
                .map(|_| i64::try_from(rng.below(capacity)).unwrap())
                .collect(),
            values: (0..COLUMNS).map(|_| 0.5 + rng.unit()).collect(),
        })
        .collect()
}

/// The mean time of one forward over `decisions`, taken in turn for `runs` forwards.
fn time<L: Lanes>(model: &Model, decisions: &[Vec<SparseOption>], runs: usize) -> Duration {
    let mut scratch = Scratch::new();
    let mut out = Vec::new();
    for options in decisions.iter().take(runs.min(decisions.len())) {
        model
            .forward_lanes::<L>(&mut scratch, 27, 3, options, &mut out)
            .unwrap();
    }
    let start = Instant::now();
    for run in 0..runs {
        let options = &decisions[run % decisions.len()];
        model
            .forward_lanes::<L>(&mut scratch, 27, 3, black_box(options), &mut out)
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
    for blocks in [0, 2] {
        let parts = synthetic::parts(7, width, capacity, blocks);
        let model = Model::from_bytes(&format::write(&synthetic::meta(), &parts).unwrap()).unwrap();
        // The same decision again (its rows in cache), and 256 decisions in turn (5 MB of rows).
        let hot = time::<Native>(&model, &small[..1], runs(20_000));
        let cold = time::<Native>(&model, &small, runs(20_000));
        let wide = time::<Native>(&model, &large, runs(200));
        let scalar_hot = time::<Scalar>(&model, &small[..1], runs(20_000));
        let scalar_cold = time::<Scalar>(&model, &small, runs(20_000));
        let scalar_wide = time::<Scalar>(&model, &large, runs(200));
        println!(
            "  {blocks} residual blocks, {}: 6 options {:.1} µs (one decision again) / {:.1} µs \
             (256 decisions in turn); 512 options {:.0} µs",
            crate::backend(),
            micros(hot),
            micros(cold),
            micros(wide)
        );
        println!(
            "  {blocks} residual blocks, scalar: 6 options {:.1} µs / {:.1} µs; 512 options {:.0} µs",
            micros(scalar_hot),
            micros(scalar_cold),
            micros(scalar_wide)
        );
        let verdict = |time: Duration, target: Duration| {
            if time < target { "met" } else { "missed" }
        };
        println!(
            "  targets: 6 options under 30 µs {}; 512 options under 2 ms {}",
            verdict(hot, Duration::from_micros(30)),
            verdict(wide, Duration::from_millis(2))
        );
        if optimized && blocks == 0 {
            assert!(hot < Duration::from_micros(30), "6 options: {hot:?}");
        }
    }
}
