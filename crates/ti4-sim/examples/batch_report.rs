//! Run a batch and print what happened.
//!
//! The standing check in one command: how games ended, what was scored, and which subsystems were
//! reached. A batch that runs clean while reaching nothing is the failure this exists to make
//! visible — the first run of this harness did exactly that: forty games, no errors, a completion
//! rate of 1.00, and not one tactical action.
//!
//! `cargo run --release -p ti4-sim --example batch_report`

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_sim::run::{Horizon, run};

fn main() {
    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|n| PlayerId::new(*n))
        .collect();
    let batch = run(
        ContentStore::embedded(),
        &players,
        0..40,
        Horizon::default(),
    );
    println!(
        "games={} errors={}",
        batch.results.len(),
        batch.errors().len()
    );
    println!(
        "completion={:.2} games_won={}",
        batch.completion_rate(),
        batch.games_won(10)
    );
    println!(
        "mean_top={:.2} mean_total={:.2}",
        batch.mean_top_score(),
        batch.mean_points()
    );
    println!("endings={:?}", batch.endings());
    println!(
        "mean_rounds={:.1}",
        batch
            .results
            .iter()
            .map(|r| f64::from(r.rounds))
            .sum::<f64>()
            / 40.0
    );
    let ev = batch.events();
    let mut rows: Vec<(&String, &usize)> = ev.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    println!("-- events actually emitted --");
    for (name, count) in rows.iter().take(28) {
        println!("  {name:32} {count}");
    }
    println!(
        "mean_decisions={:.0} s/decision={:.9}",
        batch.mean_decisions(),
        batch.seconds_per_decision()
    );
}
