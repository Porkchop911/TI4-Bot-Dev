//! Deterministic seated soak for the base-faction packages (and the factions added after them).
//!
//! Always run optimized: `cargo run --release -p ti4-sim --example base_faction_soak`.
//!
//! Arguments: `[faction|all] [first_seed] [count] [replay_every]`. Defaults: every faction, seed 0,
//! 25 seeds (the routine size, operator 2026-10-05), and a determinism replay of every 10th case.
//! The milestone exit gate runs `all 0 200 10`. `replay_every` 1 replays every case.
//! The target replaces one of six original factions, rotating its seat by seed.
//! No production faction-scope list is widened.

use std::io::{self, Write};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_engine::game::Game;
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

// The Obsidian cannot be chosen during setup (Nocturne); it is seated directly here only as a
// robustness soak of its windows. In a real game a seat becomes it through the Firmament's flip.
const FACTIONS: [&str; 27] = [
    "arborec",
    "argent",
    "bastion",
    "cabal",
    "crimson",
    "deepwrought",
    "empyrean",
    "firmament",
    "ghost",
    "keleresa",
    "keleresm",
    "keleresx",
    "mahact",
    "mentak",
    "muaat",
    "naalu",
    "naaz",
    "nekro",
    "nomad",
    "obsidian",
    "saar",
    "sardakk",
    "titans",
    "winnu",
    "yin",
    "yssaril",
    "ralnel",
];
const SEATS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];
const MAX_ROUNDS: u32 = 50;
const MAX_STEPS: usize = 25_000;

#[derive(PartialEq)]
struct Replay {
    state: serde_json::Value,
    events: Vec<String>,
    decisions: Vec<DecisionRecord>,
}

fn play(content: &ContentStore, faction: &str, seed: u64) -> Result<Replay, String> {
    let players: Vec<PlayerId> = SEATS.iter().map(|name| PlayerId::new(*name)).collect();
    let mut assignments = ti4_engine::seating::seat_in_scope(&players);
    let mut target = players[usize::try_from(seed % 6).map_err(|error| error.to_string())?].clone();
    // The Tribuni: a Keleres variant takes the seat of the faction it replaces, when that faction
    // is already at the table, so its base faction is not played twice.
    if let Some(base) = ti4_engine::seating::tribuni_base(faction)
        && let Some((seat, _)) = assignments.iter().find(|(_, held)| held.as_str() == base)
    {
        target = seat.clone();
    }
    assignments.insert(target, FactionId::new(faction));

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

    let mut game = Game::with_seeded_random(state, content, seed)
        .with_sources(DEFAULT)
        .with_galaxy(galaxy);
    if let Err(error) = game.run(MAX_ROUNDS, MAX_STEPS) {
        let last = game.table.log.records.last().map_or_else(
            || "<none>".to_owned(),
            |record| format!("{}: {} -> {}", record.player, record.prompt, record.chosen),
        );
        let pending = game.legal_options().map_or_else(
            || "<none>".to_owned(),
            |choice| {
                format!(
                    "{}: {} [{} options]",
                    choice.player,
                    choice.prompt,
                    choice.options.len()
                )
            },
        );
        return Err(format!(
            "round={} phase={:?} error={error}; last_decision={last}; pending={pending}",
            game.state.round, game.state.phase
        ));
    }
    if !game.state.finished {
        return Err(format!(
            "horizon reached without a finished game: round={} decisions={}",
            game.state.round,
            game.table.log.records.len()
        ));
    }
    Ok(Replay {
        state: serde_json::to_value(&game.state).map_err(|error| error.to_string())?,
        events: game.events,
        decisions: game.table.log.records,
    })
}

fn run_case(content: &ContentStore, faction: &str, seed: u64, replay: bool) -> Option<String> {
    match play(content, faction, seed) {
        Err(error) => Some(format!("FAIL faction={faction} seed={seed}: {error}")),
        Ok(_) if !replay => None,
        Ok(first) => match play(content, faction, seed) {
            Err(error) => Some(format!(
                "FAIL faction={faction} seed={seed} replay: {error}"
            )),
            Ok(second) if first != second => {
                let divergence = first
                    .decisions
                    .iter()
                    .zip(&second.decisions)
                    .position(|(left, right)| left != right)
                    .unwrap_or_else(|| first.decisions.len().min(second.decisions.len()));
                let prior = divergence
                    .checked_sub(1)
                    .and_then(|index| first.decisions.get(index))
                    .map_or_else(|| "<none>".to_owned(), |record| format!("{record:?}"));
                Some(format!(
                    "FAIL faction={faction} seed={seed}: replay differs at decision {divergence}; prior={prior}; first={:?}; replay={:?}; events_equal={} state_equal={}",
                    first.decisions.get(divergence),
                    second.decisions.get(divergence),
                    first.events == second.events,
                    first.state == second.state
                ))
            }
            Ok(_) => None,
        },
    }
}

#[derive(Clone, Copy)]
struct Job {
    faction_index: usize,
    seed: u64,
}

struct JobCursor {
    faction_index: usize,
    offset: u64,
    start: u64,
    count: u64,
}

impl JobCursor {
    fn next(&mut self, faction_count: usize) -> Option<Job> {
        if self.faction_index >= faction_count {
            return None;
        }
        let job = Job {
            faction_index: self.faction_index,
            seed: self.start + self.offset,
        };
        self.offset += 1;
        if self.offset == self.count {
            self.offset = 0;
            self.faction_index += 1;
        }
        Some(job)
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|message| (*message).to_owned())
        })
        .unwrap_or_else(|| "non-string panic payload".to_owned())
}

fn run_parallel(
    chosen: &[&str],
    start: u64,
    count: u64,
    replay_every: u64,
) -> (Vec<u64>, Vec<u64>, Vec<String>) {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    let content = ContentStore::embedded();
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, 16);
    let cursor = Arc::new(Mutex::new(JobCursor {
        faction_index: 0,
        offset: 0,
        start,
        count,
    }));
    let completed_by_faction: Vec<_> = (0..chosen.len()).map(|_| AtomicU64::new(0)).collect();
    let completed_total = AtomicU64::new(0);
    let last_progress = AtomicU64::new(0);
    let failed_by_faction: Vec<_> = (0..chosen.len()).map(|_| AtomicU64::new(0)).collect();
    let failures = Arc::new(Mutex::new(Vec::<(usize, u64, String)>::new()));
    let progress_lock = Mutex::new(());
    let total_cases = u128::try_from(chosen.len()).unwrap_or(u128::MAX) * u128::from(count);

    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker_index in 0..workers {
            let cursor = Arc::clone(&cursor);
            let failures = Arc::clone(&failures);
            let completed_by_faction = &completed_by_faction;
            let completed_total = &completed_total;
            let last_progress = &last_progress;
            let failed_by_faction = &failed_by_faction;
            let progress_lock = &progress_lock;
            handles.push(scope.spawn(move || {
                loop {
                    let job = cursor
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .next(chosen.len());
                    let Some(job) = job else { break };
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        run_case(
                            content,
                            chosen[job.faction_index],
                            job.seed,
                            (job.seed - start) % replay_every == 0,
                        )
                    }));
                    let failure = match result {
                        Ok(failure) => failure,
                        Err(payload) => Some(format!(
                            "FAIL faction={} seed={}: worker case panicked: {}",
                            chosen[job.faction_index],
                            job.seed,
                            panic_message(payload.as_ref())
                        )),
                    };
                    if let Some(message) = failure {
                        failed_by_faction[job.faction_index].fetch_add(1, Ordering::Relaxed);
                        failures
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((job.faction_index, job.seed, message));
                    }
                    completed_by_faction[job.faction_index].fetch_add(1, Ordering::Relaxed);
                    let completed = completed_total.fetch_add(1, Ordering::Relaxed) + 1;
                    if completed % 25 == 0 || u128::from(completed) == total_cases {
                        let _guard = progress_lock
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let latest = completed_total.load(Ordering::Relaxed);
                        let reported = last_progress.load(Ordering::Relaxed);
                        if latest / 25 > reported / 25 || u128::from(latest) == total_cases {
                            let mut stdout = io::stdout().lock();
                            let _ =
                                writeln!(stdout, "progress completed={latest} total={total_cases}");
                            let _ = stdout.flush();
                            last_progress.store(latest, Ordering::Relaxed);
                        }
                    }
                }
                worker_index
            }));
        }

        for (worker_index, handle) in handles.into_iter().enumerate() {
            if let Err(payload) = handle.join() {
                failures
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push((
                        usize::MAX,
                        u64::MAX,
                        format!(
                            "FAIL worker={worker_index}: worker panicked: {}",
                            panic_message(payload.as_ref())
                        ),
                    ));
            }
        }
    });

    let completed: Vec<u64> = completed_by_faction
        .iter()
        .map(|counter| counter.load(Ordering::Relaxed))
        .collect();
    let observed: u128 = completed.iter().map(|value| u128::from(*value)).sum();
    for (index, actual) in completed.iter().enumerate() {
        if *actual != count {
            failures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push((
                    index,
                    u64::MAX,
                    format!(
                        "FAIL faction={} worker pool: completed {actual} of {count} scheduled cases",
                        chosen[index]
                    ),
                ));
        }
    }
    let mut failures = failures
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if observed != total_cases {
        failures.push((
            usize::MAX,
            u64::MAX,
            format!("FAIL worker pool: completed {observed} of {total_cases} scheduled cases"),
        ));
    }
    failures.sort_by_key(|(faction_index, seed, _)| (*faction_index, *seed));
    let messages = std::mem::take(&mut *failures)
        .into_iter()
        .map(|(_, _, message)| message)
        .collect();
    let failed: Vec<u64> = failed_by_faction
        .iter()
        .map(|counter| counter.load(Ordering::Relaxed))
        .collect();
    (completed, failed, messages)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let chosen: Vec<&str> = match args.first() {
        Some(faction) if faction == "all" => FACTIONS.to_vec(),
        Some(faction) if FACTIONS.contains(&faction.as_str()) => vec![faction],
        Some(faction) => {
            eprintln!(
                "unknown faction {faction}; expected one of {}",
                FACTIONS.join(", ")
            );
            std::process::exit(2);
        }
        None => FACTIONS.to_vec(),
    };
    let parse = |index: usize, default: u64| -> u64 {
        args.get(index).map_or(default, |value| {
            value.parse::<u64>().unwrap_or_else(|_| {
                eprintln!("argument {} must be a non-negative integer", index + 1);
                std::process::exit(2);
            })
        })
    };
    let start = parse(1, 0);
    let count = parse(2, 25);
    let replay_every = parse(3, 10);
    if count == 0 || replay_every == 0 || start.checked_add(count).is_none() {
        eprintln!("count and replay_every must be positive and the seed range must not overflow");
        std::process::exit(2);
    }
    let full_campaign = chosen.len() == FACTIONS.len() && start == 0 && count >= 200;
    let (completed, failures, messages) = run_parallel(&chosen, start, count, replay_every);
    for message in &messages {
        eprintln!("{message}");
    }
    for (index, faction) in chosen.iter().enumerate() {
        println!(
            "result faction={faction} games={} failures={}",
            completed[index], failures[index]
        );
    }
    let total = messages.len() as u64;
    println!("replay_every={replay_every}");
    println!(
        "{}: factions={} games={} failures={total}",
        if full_campaign {
            "EXIT-GATE SOAK"
        } else {
            "routine soak (not the exit gate)"
        },
        chosen.len(),
        u128::try_from(chosen.len()).unwrap_or(u128::MAX) * u128::from(count)
    );
    if total != 0 {
        std::process::exit(1);
    }
}
