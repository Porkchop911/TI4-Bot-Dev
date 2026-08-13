//! One training generation, timed to the M00-012 protocol.
//!
//! Emits a single JSON sample on stdout so an orchestrator can interleave it with the Python
//! measurement of the same workload shape. The unit is a whole generation — play the games, credit
//! the decisions, apply one update — because that is what a training run is made of and what a
//! comparison between the two implementations has to be about.
//!
//! `cargo run -p ti4-training --example bench_generation --release -- --seed 0 --games 4 --seats 3`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;
use ti4_policy::learned::{DEFAULT_DIMENSIONS, Profile, blank_profile};
use ti4_sim::benchmark::{Sample, SemanticGate, WARMUP};
use ti4_training::gradient::{Step, apply, batch_statistics};
use ti4_training::reward::{Reward, Stage};
use ti4_training::rollout::{Horizon, play_batch};

fn argument(name: &str, fallback: u64) -> u64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

/// Play, credit, update — one generation. Returns games and decisions, or `None` if anything
/// failed, which the protocol treats as invalidating the sample rather than as a fast result.
fn generation(seed: u64, games: u64, players: &[PlayerId]) -> Option<(usize, usize)> {
    let content = ContentStore::embedded();
    let factions = ti4_engine::seating::seat_in_scope(players);
    let mut profiles: BTreeMap<PlayerId, Profile> = players
        .iter()
        .map(|player| {
            let faction = factions
                .get(player)
                .map_or_else(String::new, ToString::to_string);
            (player.clone(), blank_profile(&faction, DEFAULT_DIMENSIONS))
        })
        .collect();

    let seeds: Vec<u64> = (seed..seed + games).collect();
    let rollouts = play_batch(
        content,
        players,
        &profiles,
        DEFAULT,
        &seeds,
        Horizon::short(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
    );
    if rollouts.iter().any(|one| one.error.is_some()) {
        return None;
    }

    let reward = Reward::for_stage(Stage::Two);
    let collected = batch_statistics(&rollouts, &profiles, &reward);
    let decisions: usize = collected
        .values()
        .flat_map(std::collections::BTreeMap::values)
        .map(|row| row.actions)
        .sum();
    for (player, rows) in &collected {
        if let Some(profile) = profiles.get_mut(player) {
            apply(profile, rows, Step::default());
        }
    }
    (decisions > 0).then_some((rollouts.len(), decisions))
}

fn main() {
    let seed = argument("--seed", 0);
    let games = argument("--games", 4);
    let seats = usize::try_from(argument("--seats", 3)).unwrap_or(3);
    let warmup = argument("--warmup", 0) == 1;

    let players: Vec<PlayerId> = (0..seats).map(|i| PlayerId::new(format!("p{i}"))).collect();

    if warmup {
        // The protocol's ten unmeasured iterations, run on the same shape but not reported.
        for index in 0..WARMUP {
            let _ = generation(seed.wrapping_add(index as u64), games, &players);
        }
        println!("{{\"warmup\":{WARMUP}}}");
        return;
    }

    let started = std::time::Instant::now();
    let outcome = generation(seed, games, &players);
    let nanos = started.elapsed().as_nanos();

    let sample = match outcome {
        Some((played, decisions)) => Sample {
            pair: 0,
            seed,
            nanos,
            games: played,
            decisions,
            gate: SemanticGate::Pass,
        },
        None => Sample {
            pair: 0,
            seed,
            nanos,
            games: 0,
            decisions: 0,
            gate: SemanticGate::Fail,
        },
    };
    println!(
        "{}",
        serde_json::to_string(&sample).expect("a sample serialises")
    );
}
