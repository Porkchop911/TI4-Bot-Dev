//! A batch diagnostic: what a run of games actually did, printed rather than asserted.
//!
//! Kept as a tool because every wrong claim this project has made about the engine was caught by
//! measuring instead of reasoning. Run with `cargo run -p ti4-sim --example diag --release`.

use std::collections::BTreeMap;
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
        0..24,
        Horizon::default(),
    );
    let mut endings: BTreeMap<&str, usize> = BTreeMap::new();
    let mut top = Vec::new();
    let mut rounds = Vec::new();
    let mut events: BTreeMap<String, usize> = BTreeMap::new();
    for r in &batch.results {
        *endings.entry(r.ended_because.label()).or_default() += 1;
        top.push(r.victory_points.values().copied().max().unwrap_or(0));
        rounds.push(r.rounds);
        for (k, v) in &r.events {
            *events.entry(k.clone()).or_default() += v;
        }
        if let Some(e) = &r.error {
            println!("seed {} error: {e}", r.seed);
        }
    }
    println!("endings: {endings:?}");
    println!("top VP per game: {top:?}");
    println!("rounds: {rounds:?}");
    println!(
        "decisions/game: {:?}",
        batch
            .results
            .iter()
            .map(|r| r.decisions)
            .collect::<Vec<_>>()
    );
    println!(
        "secs/game mean: {:.4}",
        batch.results.iter().map(|r| r.seconds).sum::<f64>()
            / f64::from(u32::try_from(batch.results.len()).unwrap_or(u32::MAX))
    );
    let mut ev: Vec<_> = events.into_iter().collect();
    ev.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (k, c) in ev {
        println!("  {k}: {c}");
    }
}
