//! Does training make the policy play better? Measured, not asserted.
//!
//! Trains a profile, then plays fresh games with it against the seeds it never trained on, and
//! compares what it produced with what a blank profile produces on the same seeds.
//!
//! `cargo run -p ti4-training --example train --release`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_model::id::PlayerId;
use ti4_policy::learned::Profile;
use ti4_training::reward::Stage;
use ti4_training::rollout::{Horizon, play};
use ti4_training::stage1::{Plan, train};

fn evaluate(
    profiles: &BTreeMap<PlayerId, Profile>,
    players: &[PlayerId],
    from: u64,
    games: u64,
) -> (f64, f64, f64) {
    let (mut planets, mut points, mut scoreable) = (0i64, 0i64, 0i64);
    let mut seats = 0i64;
    for seed in from..from + games {
        let played = play(
            ContentStore::embedded(),
            players,
            profiles,
            POK,
            seed,
            Horizon::short(),
            ti4_engine::opening::DEFAULT_REQUIREMENT,
        );
        for seat in &played.seats {
            seats += 1;
            planets += seat.episode.final_progress.planets_gained;
            points += seat.episode.final_progress.victory_points;
            scoreable += seat.episode.final_progress.scoreable_public
                + seat.episode.final_progress.scoreable_secret;
        }
    }
    let n = f64::from(i32::try_from(seats.max(1)).unwrap_or(1));
    (
        f64::from(i32::try_from(planets).unwrap_or(0)) / n,
        f64::from(i32::try_from(points).unwrap_or(0)) / n,
        f64::from(i32::try_from(scoreable).unwrap_or(0)) / n,
    )
}

fn main() {
    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();
    let plan = Plan {
        stage: Stage::Two,
        players: players.clone(),
        generations: 12,
        games: 8,
        seed: 0,
        ..Plan::smoke(Stage::Two)
    };

    let started = std::time::Instant::now();
    let run = train(ContentStore::embedded(), &plan);
    println!(
        "trained {} generations x {} games in {:.1}s",
        plan.generations,
        plan.games,
        started.elapsed().as_secs_f64()
    );
    for generation in &run.generations {
        println!(
            "  gen {:>2}: {:>5} decisions, spread {:.3}, movement {:.4}, {} errors",
            generation.index,
            generation.decisions,
            generation.best_spread(),
            generation.movement(),
            generation.errors
        );
    }

    // Held-out seeds: games no generation trained on.
    let held = 10_000u64;
    let blank: BTreeMap<PlayerId, Profile> = BTreeMap::new();
    let evaluation = 40u64;
    let (bp, bv, bs) = evaluate(&blank, &players, held, evaluation);
    let (tp, tv, ts) = evaluate(&run.profiles, &players, held, evaluation);
    println!(
        "\non {evaluation} held-out games ({} seat-games), per seat:",
        evaluation * 6
    );
    println!("                    blank   trained");
    println!("  planets gained   {bp:>6.2}   {tp:>6.2}");
    println!("  victory points   {bv:>6.2}   {tv:>6.2}");
    println!("  scoreable now    {bs:>6.2}   {ts:>6.2}");
}
