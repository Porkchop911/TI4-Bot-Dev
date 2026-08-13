//! A batch diagnostic: what a run of games actually did, printed rather than asserted.
//!
//! Kept as a tool because every wrong claim this project has made about the engine was caught by
//! measuring instead of reasoning. A completion rate of 1.00 over forty games once meant the
//! harness had no galaxy and had measured nothing at all.
//!
//! `cargo run -p ti4-sim --example diag --release [--random] [--games N]`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_sim::run::{Horizon, Seats, run_with};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seats = if args.iter().any(|arg| arg == "--random") {
        Seats::Random
    } else {
        Seats::Scored
    };
    let games: u64 = args
        .iter()
        .position(|arg| arg == "--games")
        .and_then(|at| args.get(at + 1))
        .and_then(|count| count.parse().ok())
        .unwrap_or(24);

    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();
    let batch = run_with(
        ContentStore::embedded(),
        &players,
        0..games,
        Horizon::default(),
        seats,
    );

    let mut endings: BTreeMap<&str, usize> = BTreeMap::new();
    let mut events: BTreeMap<String, usize> = BTreeMap::new();
    let mut tops = Vec::new();
    for result in &batch.results {
        *endings.entry(result.ended_because.label()).or_default() += 1;
        tops.push(result.victory_points.values().copied().max().unwrap_or(0));
        for (label, count) in &result.events {
            *events.entry(label.clone()).or_default() += count;
        }
        if let Some(error) = &result.error {
            println!("seed {} error: {error}", result.seed);
        }
    }

    let played = f64::from(u32::try_from(batch.results.len()).unwrap_or(u32::MAX));
    println!("seats: {}   games: {}", seats.label(), batch.results.len());
    println!("endings: {endings:?}");
    println!("top VP per game: {tops:?}");
    println!(
        "rounds: {:?}",
        batch.results.iter().map(|r| r.rounds).collect::<Vec<u32>>()
    );
    println!(
        "decisions/game: {:.0}   seconds/game: {:.4}",
        f64::from(
            u32::try_from(batch.results.iter().map(|r| r.decisions).sum::<usize>())
                .unwrap_or(u32::MAX)
        ) / played,
        batch.results.iter().map(|r| r.seconds).sum::<f64>() / played,
    );
    let mut ordered: Vec<(String, usize)> = events.into_iter().collect();
    ordered.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (label, count) in ordered {
        println!("  {label}: {count}");
    }
}
