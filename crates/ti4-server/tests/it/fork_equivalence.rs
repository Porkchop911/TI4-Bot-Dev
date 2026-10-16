//! `Game::fork` must continue EXACTLY like the original: same offers, same step results, same
//! decision log, same final state, events, timing log, dice and RNG positions.
//!
//! This is the enforcement behind the FORK RULE in AGENTS.md: the exhaustive destructuring in
//! `Game::snapshot` stops a new field from being forgotten silently; this test proves that what
//! is copied is enough, on a generated game (always) and on real saved games (when present).

use crate::support;

use std::collections::BTreeSet;

use ti4_engine::game::Game;
use ti4_host::rng_force::RngForce;

use crate::support::{
    Source, forced_decider, generated, generated_with_factions, saved_games, start_game,
};

#[derive(Default, Debug)]
struct Stats {
    forks: usize,
    skipped_pending_rng: usize,
    steps_compared: usize,
    prompts: BTreeSet<String>,
    final_state: serde_json::Value,
    final_rng: ti4_engine::rng::RngPositions,
}

fn fork_at(source: &Source, original: &Game<'static>) -> Game<'static> {
    let from = original.table.log.records.len();
    let mut fork = original.fork_keeping_log();
    let force = RngForce::new(&source.marks);
    fork.table
        .set_default(Box::new(forced_decider(source, from, force.clone())));
    if let Some(force) = &force {
        force.attach(&mut fork);
    }
    fork
}

fn assert_same_end(original: &Game<'static>, fork: &Game<'static>, context: &str) {
    let len = fork.table.log.records.len();
    assert_eq!(
        original.table.log.records[..len],
        fork.table.log.records[..],
        "{context}: decision logs"
    );
    assert_eq!(
        serde_json::to_value(&original.state).unwrap(),
        serde_json::to_value(&fork.state).unwrap(),
        "{context}: states"
    );
    assert_eq!(original.events, fork.events, "{context}: events");
    assert_eq!(
        original.timing.log(),
        fork.timing.log(),
        "{context}: timing log"
    );
    assert_eq!(
        original.timing.applied_events(),
        fork.timing.applied_events(),
        "{context}: applied events"
    );
    assert_eq!(original.rolls(), fork.rolls(), "{context}: dice");
    assert_eq!(
        original.rng_positions(),
        fork.rng_positions(),
        "{context}: rng positions"
    );
    assert_eq!(
        original.legal_options(),
        fork.legal_options(),
        "{context}: offer"
    );
}

/// Walk `source` with one original; fork it every `stride` step boundaries and run each fork
/// beside it for `window` decisions (two forks, at the start and the middle, run to the end).
fn check(source: &Source, stride: usize, window: usize) -> Stats {
    let mut stats = Stats::default();
    let total = source.records.len();
    let mut original = start_game(source);
    let mut forks: Vec<(Game<'static>, usize)> = Vec::new();
    let mut boundary = 0usize;
    let mut middle_done = false;
    loop {
        let len = original.table.log.records.len();
        if len >= total || original.state.finished {
            break;
        }
        let take_middle = !middle_done && len >= total / 2;
        if boundary % stride == 0 || boundary == 0 || take_middle {
            // A forced position waits for the next draw; apply it now (equivalent) so the
            // snapshot holds no deferred work.
            original.flush_rng_sync();
            if original.rng_sync_pending() {
                stats.skipped_pending_rng += 1;
            } else {
                let end = if boundary == 0 || take_middle {
                    usize::MAX
                } else {
                    len + window
                };
                middle_done |= take_middle;
                if let Some(choice) = original.legal_options() {
                    stats.prompts.insert(choice.prompt);
                }
                forks.push((fork_at(source, &original), end));
                stats.forks += 1;
            }
        }
        boundary += 1;
        let offered = original.legal_options();
        let result = original.step();
        let mut finished = Vec::new();
        for (index, (fork, end)) in forks.iter_mut().enumerate() {
            assert_eq!(
                fork.legal_options(),
                offered,
                "{}: offer before step {boundary} of a fork taken earlier",
                source.name
            );
            let fork_result = fork.step();
            assert_eq!(
                fork_result, result,
                "{}: step result at {boundary}",
                source.name
            );
            assert!(fork.table.log.records.len() <= original.table.log.records.len());
            assert_eq!(
                fork.table.log.records.last(),
                original.table.log.records.last(),
                "{}: newest decision at {boundary}",
                source.name
            );
            assert!(
                fork.state.identical(&original.state),
                "{}: state after step {boundary}",
                source.name
            );
            stats.steps_compared += 1;
            if fork.table.log.records.len() >= *end {
                finished.push(index);
            }
        }
        for index in finished.into_iter().rev() {
            let (fork, _) = forks.remove(index);
            assert_same_end(&original, &fork, &source.name);
        }
        if let Some(error) = &result.error {
            assert!(
                error.to_string().contains("recorded decisions exhausted") || len + 1 >= total,
                "{}: the recorded game stopped replaying at decision {len}: {error}",
                source.name
            );
            break;
        }
    }
    for (fork, _) in forks {
        assert_same_end(&original, &fork, &source.name);
    }
    stats.final_state = serde_json::to_value(&original.state).unwrap();
    stats.final_rng = original.rng_positions();
    stats
}

#[test]
fn slow_a_fork_of_a_generated_game_continues_identically() {
    let source = generated("generated-31", 31, 700, 0);
    assert!(source.records.len() > 300, "{}", source.records.len());
    let stats = check(&source, 5, 40);
    eprintln!("{stats:?}");
    assert!(stats.forks > 60, "{stats:?}");
    assert!(stats.prompts.len() > 12, "{stats:?}");
}

/// The factions added after the first six (Last Bastion, Deepwrought, Crimson, Ral Nel, Nekro,
/// Firmament, with galvanize, breaches, plots and similar shared state) must fork like any other.
#[test]
fn slow_a_fork_of_games_seating_the_newer_factions_continues_identically() {
    for (seed, factions) in [
        (5_u64, ["bastion", "deepwrought", "crimson", "ralnel"]),
        (8_u64, ["nekro", "firmament", "sol", "bastion"]),
    ] {
        let source = generated_with_factions(&format!("newer-{seed}"), seed, factions, 500, 17);
        assert!(
            source.records.len() > 150,
            "{factions:?}: {}",
            source.records.len()
        );
        let stats = check(&source, 4, 30);
        eprintln!("{factions:?}: {stats:?}");
        assert!(stats.forks > 30, "{factions:?}: {stats:?}");
    }
}

#[test]
fn slow_a_fork_of_a_game_with_forced_rng_positions_continues_identically() {
    // A turn redo's marks restore stream positions when a decision is answered; until a draw
    // applies one, a snapshot would lose it, so those boundaries must be skipped, never forked.
    let source = generated("generated-marks-12", 12, 500, 17);
    assert!(!source.marks.is_empty());
    let stats = check(&source, 3, 30);
    eprintln!("{stats:?}");
    assert!(stats.forks > 60, "{stats:?}");
    assert_eq!(stats.skipped_pending_rng, 0, "{stats:?}");
    // Flushing at every fork point must not change the game: the checked original ends exactly
    // where a plain replay (which never flushes) ends.
    let mut plain = start_game(&source);
    while plain.table.log.records.len() < source.records.len() && plain.step().error.is_none() {}
    assert_eq!(
        stats.final_state,
        serde_json::to_value(&plain.state).unwrap(),
        "flushing the RNG side channel changed the game"
    );
    assert_eq!(stats.final_rng, plain.rng_positions());
}

#[test]
fn a_fork_of_saved_real_games_continues_identically() {
    // Saves that activated a hyperlane tile (offered before hyperlanes were barred) no longer
    // replay; only those that replay completely on this engine are forked.
    let games: Vec<_> = saved_games(12)
        .into_iter()
        .filter(support::strictly_replayable)
        .take(3)
        .collect();
    if games.is_empty() {
        eprintln!("no saved games on this machine; skipped");
        return;
    }
    for source in games {
        let stats = check(&source, 41, 30);
        eprintln!("{}: {stats:?}", source.name);
        assert!(stats.forks > 10, "{stats:?}");
    }
}

/// Scratch tool (run with `-- --ignored --nocapture`): how far each saved game under
/// `TI4_SCAN_ROOT` (default: the nightly reports) replays on this engine.
#[test]
#[ignore = "diagnostic scan of saved games"]
fn scan_saved_games() {
    let root = std::env::var("TI4_SCAN_ROOT")
        .unwrap_or_else(|_| "/root/TI4-Bot-Dev/nightly-reports".to_owned());
    let mut dirs = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("history.json").exists() && path.join("init.json").exists() {
                    dirs.push(path);
                } else {
                    stack.push(path);
                }
            }
        }
    }
    dirs.sort();
    for dir in dirs {
        match support::load_saved(&dir) {
            Err(error) => eprintln!("SCAN {} load error {error}", dir.display()),
            Ok(source) => {
                let mut game = start_game(&source);
                let mut outcome = "complete".to_owned();
                while game.table.log.records.len() < source.records.len() {
                    let result = game.step();
                    if let Some(error) = result.error {
                        outcome = format!(
                            "stopped: {}",
                            error.to_string().chars().take(100).collect::<String>()
                        );
                        break;
                    }
                }
                eprintln!(
                    "SCAN {} marks={} replayed {}/{} {outcome}",
                    dir.display(),
                    source.marks.len(),
                    game.table.log.records.len(),
                    source.records.len()
                );
            }
        }
    }
}

#[test]
fn a_snapshot_can_be_instantiated_repeatedly_and_across_threads() {
    let source = generated("generated-7", 7, 120, 0);
    let mut game = start_game(&source);
    for _ in 0..40 {
        if game.table.log.records.len() >= 60 {
            break;
        }
        assert!(game.step().error.is_none());
    }
    let snapshot = game.snapshot();
    let at = snapshot.decisions().len();
    assert_eq!(at, game.table.log.records.len());
    // `GameSnapshot` is `Send`: a worker publishes it, a request thread forks it.
    let handle = std::thread::spawn(move || {
        let first = snapshot.instantiate();
        let second = snapshot.instantiate();
        assert_eq!(first.table.log.records, second.table.log.records);
        assert!(first.state.identical(&second.state));
        first.table.log.records.len()
    });
    assert_eq!(handle.join().unwrap(), at);
}

/// Measurement helper (run with `--release -- --ignored --nocapture`): what a snapshot costs
/// next to the work the session worker already does at every step boundary.
#[test]
#[ignore = "measurement"]
fn measure_snapshot_cost() {
    let games = saved_games(1);
    let Some(source) = games.first() else { return };
    let mut game = start_game(source);
    let total = source.records.len();
    let mut next_report = total / 4;
    let mut step_time = std::time::Duration::ZERO;
    let mut steps = 0u32;
    while game.table.log.records.len() < total {
        let started = std::time::Instant::now();
        let result = game.step();
        step_time += started.elapsed();
        steps += 1;
        if result.error.is_some() {
            break;
        }
        if game.table.log.records.len() >= next_report {
            next_report += total / 4;
            let reps = 20u32;
            let t = std::time::Instant::now();
            for _ in 0..reps {
                std::hint::black_box(game.snapshot());
            }
            let snapshot = t.elapsed() / reps;
            let t = std::time::Instant::now();
            for _ in 0..reps {
                std::hint::black_box(game.state.clone());
            }
            let state = t.elapsed() / reps;
            let t = std::time::Instant::now();
            for _ in 0..reps {
                std::hint::black_box(game.table.log.records.clone());
            }
            let log = t.elapsed() / reps;
            let t = std::time::Instant::now();
            for _ in 0..reps {
                std::hint::black_box(game.events.clone());
            }
            let events = t.elapsed() / reps;
            eprintln!(
                "at {} decisions: snapshot {snapshot:?} (state clone {state:?}, log clone {log:?}, engine events {events:?} x{}); mean step so far {:?}",
                game.table.log.records.len(),
                game.events.len(),
                step_time / steps
            );
        }
    }
}
