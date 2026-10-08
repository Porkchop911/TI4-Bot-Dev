//! Seat a faction outside the trained six and see whether a game survives it.
//!
//! Replaces one faction of the standard lineup with `--faction`, rotates the lineup through every
//! seat as the evaluations do, and plays greedy games with the bundle's policy (which carries a row
//! for every faction in `FACTION_ROSTER`, trained or not). Reports engine errors verbatim, how far
//! each game got, and VP by faction. Read-only with respect to training: nothing here changes
//! `IN_SCOPE_FACTIONS`.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example faction_trial -- \
//!   --bundle <checkpoint> --faction winnu --replace letnev --seeds 10 --diplomacy --rounds 4
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let faction = argument("--faction").unwrap_or_else(|| refuse("--faction is required"));
    let replace = argument("--replace").unwrap_or_else(|| "letnev".to_owned());
    let seeds: u64 =
        argument("--seeds").map_or(10, |v| v.parse().unwrap_or_else(|_| refuse("--seeds")));
    let seed_base: u64 = argument("--seed-base").map_or(900_000_000, |v| {
        v.parse().unwrap_or_else(|_| refuse("--seed-base"))
    });
    let rounds: u32 =
        argument("--rounds").map_or(4, |v| v.parse().unwrap_or_else(|_| refuse("--rounds")));
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    let temperature = 0.001;

    let lineup: Vec<String> = FACTIONS
        .iter()
        .map(|f| {
            if *f == replace {
                faction.clone()
            } else {
                (*f).to_owned()
            }
        })
        .collect();
    if !lineup.contains(&faction) {
        refuse(&format!("--replace {replace} is not one of {FACTIONS:?}"));
    }
    ti4_mlp::FactionRow::of(&faction)
        .unwrap_or_else(|error| refuse(&format!("the policy has no row for {faction}: {error}")));

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
    let roster: Vec<FactionId> = lineup.iter().map(FactionId::new).collect();
    println!(
        "faction trial: {faction} in place of {replace} · lineup {lineup:?} · {seeds} seeds x 6 rotations"
    );

    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..6).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);
    let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect();
    let started = std::time::Instant::now();
    #[allow(clippy::type_complexity, reason = "one row per game")]
    let results: Vec<(u64, usize, Option<String>, Vec<(String, i64, bool)>, u32)> = chunks
        .into_par_iter()
        .flat_map(|(local, chunk)| {
            let local = std::rc::Rc::new(local);
            let mut out = Vec::new();
            for (seed, rotation) in chunk {
                let seated: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(index, player)| {
                        (
                            player.clone(),
                            ti4_training::rollout::seated_faction(&roster, seed, rotation, index),
                        )
                    })
                    .collect();
                let (rollout, _) =
                    ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                        content,
                        &players,
                        &seated,
                        DEFAULT,
                        seed,
                        ti4_training::rollout::Horizon {
                            rounds,
                            steps: 10_000,
                        },
                        ti4_engine::opening::DEFAULT_REQUIREMENT,
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(&pool),
                            tile_seed_offset: 0,
                        },
                        ti4_training::rollout::SimulationCapabilities { diplomacy },
                        false,
                        |baselines| {
                            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                BTreeMap::new();
                            for (index, player) in players.iter().enumerate() {
                                let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                                    .map_err(|e| format!("{player}: {e}"))?;
                                let baseline = baselines
                                    .get(player)
                                    .copied()
                                    .ok_or_else(|| format!("{player} has no baseline"))?;
                                let stream = seed
                                    .wrapping_mul(1_000_003)
                                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                                let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                    &local,
                                    vocabulary.clone(),
                                    row,
                                    stream,
                                )
                                .at_temperature(temperature)
                                .from_setup(baseline)
                                .seat();
                                deciders.insert(player.clone(), decider);
                            }
                            Ok(deciders)
                        },
                    );
                let last_round = rollout
                    .seats
                    .iter()
                    .map(|s| s.episode.final_progress.round_number)
                    .max()
                    .unwrap_or(0);
                let seats = rollout
                    .seats
                    .iter()
                    .map(|s| {
                        (
                            s.faction.to_string(),
                            s.episode.final_progress.victory_points,
                            s.episode.cleared,
                        )
                    })
                    .collect();
                out.push((seed, rotation, rollout.error.clone(), seats, last_round));
            }
            out
        })
        .collect();

    let mut errors: BTreeMap<String, usize> = BTreeMap::new();
    let mut vp: BTreeMap<String, (i64, usize, usize)> = BTreeMap::new();
    let mut short = 0usize;
    for (seed, rotation, error, seats, last_round) in &results {
        if let Some(error) = error {
            *errors.entry(error.clone()).or_default() += 1;
            if errors[error] == 1 {
                println!("  ERROR seed {seed} rotation {rotation}: {error}");
            }
            continue;
        }
        if *last_round < rounds {
            short += 1;
        }
        for (f, points, cleared) in seats {
            let e = vp.entry(f.clone()).or_default();
            e.0 += points;
            e.1 += 1;
            e.2 += usize::from(*cleared);
        }
    }
    println!(
        "  games {} · failed {} · ended before round {rounds}: {short} · {:.1}s",
        results.len(),
        errors.values().sum::<usize>(),
        started.elapsed().as_secs_f64()
    );
    for (error, n) in &errors {
        println!("  {n} x {error}");
    }
    println!("  faction      seats  mean VP  clearance");
    for (f, (points, n, cleared)) in &vp {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        {
            println!(
                "  {f:<12} {n:>5}  {:>7.3}  {:>8.1}%",
                *points as f64 / *n as f64,
                100.0 * *cleared as f64 / *n as f64
            );
        }
    }
}
