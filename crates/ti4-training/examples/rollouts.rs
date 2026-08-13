//! What a batch of rollouts produces, printed rather than asserted.
//!
//! Written to answer one question no unit test can: whether a from-blank policy sees any gradient
//! at all. A trainer whose returns are identical across every decision in a seat updates nothing,
//! and reports a clean run while doing it.
//!
//! `cargo run -p ti4-training --example rollouts --release`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_training::reward::{Reward, Stage, returns};
use ti4_training::rollout::{Horizon, play};

fn main() {
    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();
    for (label, horizon, stage) in [
        ("stage 1 (one round)", Horizon::opening(), Stage::One),
        ("stage 2 (four rounds)", Horizon::short(), Stage::Two),
    ] {
        let reward = Reward::for_stage(stage);
        let (mut decisions, mut cleared, mut seats, mut flat, mut errors) =
            (0usize, 0usize, 0usize, 0usize, 0usize);
        let mut spread: Vec<f64> = Vec::new();
        let mut gains: Vec<i64> = Vec::new();
        let started = std::time::Instant::now();
        for seed in 0..16u64 {
            let played = play(
                ContentStore::embedded(),
                &players,
                &BTreeMap::new(),
                POK,
                seed,
                horizon,
                ti4_engine::opening::DEFAULT_REQUIREMENT,
            );
            if played.error.is_some() {
                errors += 1;
                continue;
            }
            for seat in &played.seats {
                seats += 1;
                decisions += seat.trajectory.len();
                if seat.episode.cleared {
                    cleared += 1;
                }
                gains.push(seat.episode.final_progress.planets_gained);
                let credited = returns(&seat.episode, &reward);
                let low = credited.iter().copied().reduce(f64::min).unwrap_or(0.0);
                let high = credited.iter().copied().reduce(f64::max).unwrap_or(0.0);
                spread.push(high - low);
                if (high - low).abs() < 1e-12 {
                    flat += 1;
                }
            }
        }
        let as_float = |value: usize| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
        let n = as_float(seats.max(1));
        println!(
            "{label}: {errors} errors, {seats} seat-games, {:.1}s",
            started.elapsed().as_secs_f64()
        );
        println!("  decisions/seat:        {:.0}", as_float(decisions) / n);
        println!("  cleared the bar:       {cleared} of {seats}");
        println!(
            "  planets gained (mean): {:.2}",
            f64::from(i32::try_from(gains.iter().sum::<i64>()).unwrap_or(i32::MAX)) / n
        );
        println!(
            "  mean spread of returns: {:.3}",
            spread.iter().sum::<f64>() / n
        );
        println!("  seats with NO gradient: {flat} of {seats}");
    }
}
