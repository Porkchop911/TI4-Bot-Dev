//! Is grouping decisions by (faction, head) worth restarting a distillation for?
//!
//! Measured rather than assumed: two previous performance guesses in M09-029 were wrong, and one
//! "optimisation" made things slower.

#![allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]

use ti4_mlp::{Actor, FactionRow, SparseOption, Width};
use ti4_tensor::Tensor;

const CAPACITY: i64 = 16_384;
const DECISIONS: usize = 512;
const OPTIONS: usize = 6;
const FEATURES: usize = 85;

fn options(seed: usize) -> Vec<SparseOption> {
    (0..OPTIONS)
        .map(|option| SparseOption {
            columns: (0..FEATURES)
                .map(|f| {
                    if f < (FEATURES * 3) / 4 {
                        (f as i64 * 131) % CAPACITY
                    } else {
                        ((seed * 31 + option * 977 + f * 131) as i64) % CAPACITY
                    }
                })
                .collect(),
            values: (0..FEATURES).map(|f| 0.5 + (f as f32) * 0.01).collect(),
        })
        .collect()
}

fn main() {
    ti4_tensor::configure_deterministic(1).expect("configured");
    let actor = Actor::zeros(Width::W256, CAPACITY);
    let row = FactionRow::of("sol").expect("roster");
    let batch: Vec<Vec<SparseOption>> = (0..DECISIONS).map(options).collect();

    // As the trainer does it today: one forward per decision.
    let started = std::time::Instant::now();
    let mut sink = 0.0f64;
    for decision in &batch {
        let logits = actor.logits(decision, "production", row).expect("logits");
        let log_q = logits.log_softmax(0, ti4_tensor::Kind::Float);
        sink += log_q.sum(ti4_tensor::Kind::Float).double_value(&[]);
    }
    let individual = started.elapsed();

    // Grouped: every option of every decision through one trunk, then split for the softmax.
    let started = std::time::Instant::now();
    let flat: Vec<SparseOption> = batch.iter().flatten().cloned().collect();
    let all = actor.logits(&flat, "production", row).expect("logits");
    let mut sink2 = 0.0f64;
    for index in 0..DECISIONS {
        let slice = all.narrow(0, (index * OPTIONS) as i64, OPTIONS as i64);
        let log_q = slice.log_softmax(0, ti4_tensor::Kind::Float);
        sink2 += log_q.sum(ti4_tensor::Kind::Float).double_value(&[]);
    }
    let grouped = started.elapsed();

    println!("{DECISIONS} decisions x {OPTIONS} options x {FEATURES} features");
    println!(
        "  per decision   {:>8.1} ms  ({:>6.1} us/decision)",
        individual.as_secs_f64() * 1e3,
        individual.as_secs_f64() * 1e6 / DECISIONS as f64
    );
    println!(
        "  grouped        {:>8.1} ms  ({:>6.1} us/decision)",
        grouped.as_secs_f64() * 1e3,
        grouped.as_secs_f64() * 1e6 / DECISIONS as f64
    );
    println!(
        "  speedup        {:>8.2}x",
        individual.as_secs_f64() / grouped.as_secs_f64()
    );
    // Both paths must agree, or the speedup is measuring a different computation.
    println!("  agreement      {sink:.6} vs {sink2:.6}");
    // Forward + backward, which is what the trainer actually pays.
    let mut trainable = Actor::zeros(Width::W256, CAPACITY);
    *trainable.input_mut() = trainable.input().detach().copy().set_requires_grad(true);
    *trainable.shared_readout_mut() = trainable
        .shared_readout()
        .detach()
        .copy()
        .set_requires_grad(true);

    let n = 64;
    let started = std::time::Instant::now();
    for decision in batch.iter().take(n) {
        let logits = trainable
            .logits(decision, "production", row)
            .expect("logits");
        let loss = logits
            .log_softmax(0, ti4_tensor::Kind::Float)
            .sum(ti4_tensor::Kind::Float);
        loss.backward();
    }
    let bw = started.elapsed();
    println!(
        "  fwd+bwd        {:>8.1} ms  ({:>6.1} us/decision)",
        bw.as_secs_f64() * 1e3,
        bw.as_secs_f64() * 1e6 / n as f64
    );

    // Grouped forward + backward: one dense scatter for the whole group instead of one each.
    trainable.input_mut().zero_grad();
    let started = std::time::Instant::now();
    let flat_n: Vec<SparseOption> = batch.iter().take(n).flatten().cloned().collect();
    let all2 = trainable
        .logits(&flat_n, "production", row)
        .expect("logits");
    let mut total: Option<Tensor> = None;
    for index in 0..n {
        let slice = all2.narrow(0, (index * OPTIONS) as i64, OPTIONS as i64);
        let term = slice
            .log_softmax(0, ti4_tensor::Kind::Float)
            .sum(ti4_tensor::Kind::Float);
        total = Some(total.map_or_else(|| term.shallow_clone(), |acc| acc + &term));
    }
    total.expect("loss").backward();
    let gbw = started.elapsed();
    println!(
        "  grouped f+b    {:>8.1} ms  ({:>6.1} us/decision)",
        gbw.as_secs_f64() * 1e3,
        gbw.as_secs_f64() * 1e6 / n as f64
    );

    // The suspected cost: index_select backward scatters into a dense [CAPACITY, width] tensor,
    // 16.8 MB per decision, however few rows were actually gathered.
    let grad = trainable.input().grad();
    println!(
        "  input grad     defined={} shape={:?}",
        grad.defined(),
        grad.size()
    );
    let _ = Tensor::from_slice(&[0.0f32]);
}
