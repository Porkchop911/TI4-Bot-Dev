//! Where a single MLP decision's time actually goes.
//!
//! Written because two successive guesses about the bottleneck were wrong, each after a multi-minute
//! full-game measurement. This times the forward pass alone at realistic sizes, so a hypothesis
//! costs seconds instead of minutes.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "a synthetic fixture: the indices and values are small by construction"
)]

use ti4_mlp::{Actor, FactionRow, SparseOption, Width};
use ti4_tensor::Tensor;

/// The shape of a real decision, from the M09-029 run: ~20 options, ~40 features each.
const OPTIONS: usize = 6;
const FEATURES: usize = 85;
const CAPACITY: i64 = 16_384;
const REPEATS: usize = 2_000;

fn options() -> Vec<SparseOption> {
    (0..OPTIONS)
        .map(|option| SparseOption {
            // Realistic sharing: measured over 40,000 real decisions, 528.8 gathered rows per
            // decision are only 131.9 distinct. Three quarters of each option's features are the
            // position facts every option repeats; the rest are option-specific.
            columns: (0..FEATURES)
                .map(|f| {
                    if f < (FEATURES * 3) / 4 {
                        (f as i64 * 131) % CAPACITY
                    } else {
                        ((option * 977 + f * 131) as i64) % CAPACITY
                    }
                })
                .collect(),
            values: (0..FEATURES).map(|f| 0.5 + (f as f32) * 0.01).collect(),
        })
        .collect()
}

fn time<T>(label: &str, repeats: usize, mut body: impl FnMut() -> T) {
    // One untimed pass, so allocation warm-up is not attributed to the operation.
    let _ = body();
    let started = std::time::Instant::now();
    for _ in 0..repeats {
        std::hint::black_box(body());
    }
    let each = started.elapsed().as_secs_f64() / repeats as f64;
    println!("  {label:<44} {:>9.1} us", each * 1e6);
}

fn main() {
    ti4_tensor::configure_deterministic(1).expect("configured");
    let actor = Actor::zeros(Width::W256, CAPACITY);
    let row = FactionRow::of("sol").expect("roster");
    let options = options();
    let batch: Vec<(&[i64], &[f32])> = options
        .iter()
        .map(|o| (o.columns.as_slice(), o.values.as_slice()))
        .collect();

    println!("forward profile: {OPTIONS} options x {FEATURES} features, capacity {CAPACITY}\n");

    time("gather, per option (old)", REPEATS, || {
        let mut gathered = Vec::with_capacity(options.len());
        for option in &options {
            gathered.push(
                ti4_tensor::gather_reduce(actor.input(), &option.columns, &option.values)
                    .expect("gather"),
            );
        }
        Tensor::stack(&gathered, 0)
    });

    time("gather, batched index_add (new)", REPEATS, || {
        ti4_tensor::gather_reduce_batch(actor.input(), &batch).expect("gather")
    });

    time("trunk (gather + hidden)", REPEATS, || {
        actor.trunk(&options, row).expect("trunk")
    });

    time("logits (trunk + readout)", REPEATS, || {
        actor.logits(&options, "production", row).expect("logits")
    });

    time("probabilities (logits + softmax)", REPEATS, || {
        actor
            .probabilities(&options, "production", row, 1.0)
            .expect("probabilities")
    });

    // The critic: one row through the same trunk.
    let critic_single = vec![options[0].clone()];
    time("one-row trunk (the critic's shape)", REPEATS, || {
        actor.trunk(&critic_single, row).expect("trunk")
    });
}
