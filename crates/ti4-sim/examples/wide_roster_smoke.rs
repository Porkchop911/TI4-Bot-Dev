//! Wide-roster seated smoke (BF-20/BF-21): six seats drawn from the wide roster by seed, played
//! to completion, with a determinism replay of every `replay_every`-th case.
//!
//! Always run optimized: `cargo run --release -p ti4-sim --example wide_roster_smoke`.
//!
//! Arguments: `[first_seed] [count] [replay_every]`, defaults `0 40 10`. Fails when any game does
//! not finish or replays differently, or when some choosable faction (every Keleres variant
//! included) is never seated in the sample.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_engine::game::Game;
use ti4_engine::seating::{FactionRoster, seat_roster, wide_roster};
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;

const SEATS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
const MAX_ROUNDS: u32 = 50;
const MAX_STEPS: usize = 25_000;
const WORKERS: usize = 12;

#[derive(PartialEq)]
struct Replay {
    state: serde_json::Value,
    events: Vec<String>,
    decisions: Vec<DecisionRecord>,
    seated: Vec<String>,
}

fn play(content: &ContentStore, seed: u64) -> Result<Replay, String> {
    let players: Vec<PlayerId> = SEATS.iter().map(|name| PlayerId::new(*name)).collect();
    let assignments = seat_roster(FactionRoster::Wide, content, &players, DEFAULT, seed)
        .map_err(|error| format!("draw: {error}"))?;
    let mut state = start_game_seeded(content, &players, DEFAULT, None, seed)
        .map_err(|error| format!("setup: {error}"))?;
    for (player, assigned) in &assignments {
        state
            .player_mut(player)
            .ok_or_else(|| format!("missing seat {player}"))?
            .faction = assigned.clone();
    }
    // Setup dealt the notes before factions were known: re-deal so every seat holds its own notes.
    ti4_engine::promissory::deal(&mut state, content, DEFAULT);

    let filler: Vec<String> = ti4_engine::seating::map_filler(content, 30, DEFAULT, seed)
        .into_iter()
        .map(|system| system.to_string())
        .collect();
    let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
    let galaxy = ti4_engine::seating::build_board(content, &assignments, &borrowed, DEFAULT)
        .map_err(|error| format!("board: {error}"))?;
    for (player, assigned) in &assignments {
        ti4_engine::seating::deploy(&mut state, content, player, assigned, DEFAULT)
            .map_err(|error| format!("deploy {player} as {assigned}: {error}"))?;
    }

    let seated: Vec<String> = assignments.values().map(ToString::to_string).collect();
    let mut game = Game::with_seeded_random(state, content, seed)
        .with_sources(DEFAULT)
        .with_galaxy(galaxy);
    if let Err(error) = game.run(MAX_ROUNDS, MAX_STEPS) {
        return Err(format!(
            "seats={seated:?} round={} phase={:?} error={error}",
            game.state.round, game.state.phase
        ));
    }
    if !game.state.finished {
        return Err(format!(
            "seats={seated:?} horizon reached without a finished game: round={}",
            game.state.round
        ));
    }
    Ok(Replay {
        state: serde_json::to_value(&game.state).map_err(|error| error.to_string())?,
        events: game.events,
        decisions: game.table.log.records,
        seated,
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parse = |index: usize, default: u64| -> u64 {
        args.get(index).map_or(default, |value| {
            value.parse::<u64>().unwrap_or_else(|_| {
                eprintln!("argument {} must be a non-negative integer", index + 1);
                std::process::exit(2);
            })
        })
    };
    let start = parse(0, 0);
    let count = parse(1, 40);
    let replay_every = parse(2, 10);
    if count == 0 || replay_every == 0 || start.checked_add(count).is_none() {
        eprintln!("count and replay_every must be positive and the seed range must not overflow");
        std::process::exit(2);
    }

    let content = ContentStore::embedded();
    let next = AtomicU64::new(0);
    let failures = Mutex::new(Vec::<(u64, String)>::new());
    let seen = Mutex::new(BTreeSet::<String>::new());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS {
            scope.spawn(|| {
                loop {
                    let offset = next.fetch_add(1, Ordering::Relaxed);
                    if offset >= count {
                        break;
                    }
                    let seed = start + offset;
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let first = play(content, seed)?;
                        if offset % replay_every == 0 {
                            let second = play(content, seed)?;
                            if first != second {
                                return Err("replay differs".to_owned());
                            }
                        }
                        Ok(first)
                    }));
                    let message = match outcome {
                        Ok(Ok(replay)) => {
                            seen.lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .extend(replay.seated);
                            continue;
                        }
                        Ok(Err(error)) => error,
                        Err(_) => "case panicked".to_owned(),
                    };
                    failures
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push((seed, message));
                }
            });
        }
    });

    let mut failures = failures
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    failures.sort();
    for (seed, message) in &failures {
        eprintln!("FAIL seed={seed}: {message}");
    }
    let seen = seen
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Every choosable faction, with the Keleres family expanded into its three variants.
    let expected: BTreeSet<String> = wide_roster(content, DEFAULT)
        .into_iter()
        .flat_map(|alias| {
            if alias == ti4_engine::seating::KELERES_FAMILY {
                ti4_engine::factions::keleres::VARIANTS.to_vec()
            } else {
                vec![alias]
            }
        })
        .map(str::to_owned)
        .collect();
    let missing: Vec<&String> = expected.difference(&seen).collect();
    println!(
        "wide-roster smoke: seeds={start}..{} games={count} failures={} replay_every={replay_every} workers={WORKERS}",
        start + count,
        failures.len()
    );
    println!(
        "factions expected={} seated={} missing={missing:?}",
        expected.len(),
        seen.len()
    );
    if !failures.is_empty() || !missing.is_empty() {
        std::process::exit(1);
    }
}
