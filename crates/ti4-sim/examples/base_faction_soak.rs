//! Deterministic seated soak for the base-faction packages (and the factions added after them).
//!
//! Always run optimized: `cargo run --release -p ti4-sim --example base_faction_soak`.
//!
//! Arguments: `[faction|all|pairs] [first_seed] [count] [replay_every]`. Defaults are every
//! faction, seed 0, 25 seeds (the routine size, operator 2026-10-05), and a determinism replay of
//! every 10th case.
//! The milestone exit gate runs `all 0 200 10`. `replay_every` 1 replays every case.
//! A single faction replaces one of six originals, rotating its seat by seed. `pairs` seats every
//! unordered pair from the original 18 factions together on the same seed sample (default: 2 seeds).
//! No production faction-scope list is widened.

use std::io::{self, Write};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_engine::game::Game;
use ti4_engine::seating::IN_SCOPE_FACTIONS;
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
const ORIGINAL_18: [&str; 18] = [
    "sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x", "sardakk", "winnu", "arborec", "yin",
    "mentak", "saar", "yssaril", "naalu", "muaat", "ghost", "argent", "naaz",
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

fn legacy_assignments(
    players: &[PlayerId],
    faction: &str,
    seed: u64,
) -> Result<std::collections::BTreeMap<PlayerId, FactionId>, String> {
    let mut assignments = ti4_engine::seating::seat_in_scope(players);
    let mut target = players[usize::try_from(seed % 6).map_err(|error| error.to_string())?].clone();
    // The Tribuni: a Keleres variant takes the seat of the faction it replaces, when that faction
    // is already at the table, so its base faction is not played twice.
    if let Some(base) = ti4_engine::seating::tribuni_base(faction)
        && let Some((seat, _)) = assignments.iter().find(|(_, held)| held.as_str() == base)
    {
        target = seat.clone();
    }
    assignments.insert(target, FactionId::new(faction));
    Ok(assignments)
}

fn pair_assignments(
    players: &[PlayerId],
    pair: (&str, &str),
    seed: u64,
) -> std::collections::BTreeMap<PlayerId, FactionId> {
    let first_seat = usize::try_from(seed % 6).expect("seed modulo six fits usize");
    let second_seat = (first_seat + 1) % 6;
    let mut assignments = std::collections::BTreeMap::new();
    assignments.insert(players[first_seat].clone(), FactionId::new(pair.0));
    assignments.insert(players[second_seat].clone(), FactionId::new(pair.1));

    let mut fillers: Vec<&str> = IN_SCOPE_FACTIONS
        .iter()
        .copied()
        .filter(|faction| *faction != pair.0 && *faction != pair.1)
        .collect();
    let rotate =
        usize::try_from((seed / 6) % u64::try_from(fillers.len()).expect("nonempty fillers"))
            .expect("filler rotation fits usize");
    fillers.rotate_left(rotate);
    let mut next_filler = 0;
    for (seat, player) in players.iter().enumerate() {
        if seat != first_seat && seat != second_seat {
            assignments.insert(player.clone(), FactionId::new(fillers[next_filler]));
            next_filler += 1;
        }
    }
    assignments
}

fn play_assignments(
    content: &ContentStore,
    players: &[PlayerId],
    assignments: std::collections::BTreeMap<PlayerId, FactionId>,
    seed: u64,
) -> Result<Replay, String> {
    let mut state = start_game_seeded(content, players, DEFAULT, None, seed)
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

fn play(content: &ContentStore, faction: &str, seed: u64) -> Result<Replay, String> {
    let players: Vec<PlayerId> = SEATS.iter().map(|name| PlayerId::new(*name)).collect();
    let assignments = legacy_assignments(&players, faction, seed)?;
    play_assignments(content, &players, assignments, seed)
}

#[derive(Clone, Copy)]
enum SoakCase {
    Faction(&'static str),
    Pair(&'static str, &'static str),
}

impl SoakCase {
    fn failure_label(self) -> String {
        match self {
            Self::Faction(faction) => format!("faction={faction}"),
            Self::Pair(first, second) => format!("pair={first}+{second}"),
        }
    }

    fn play(self, content: &ContentStore, seed: u64) -> Result<Replay, String> {
        match self {
            Self::Faction(faction) => play(content, faction, seed),
            Self::Pair(first, second) => {
                let players: Vec<PlayerId> =
                    SEATS.iter().map(|name| PlayerId::new(*name)).collect();
                let assignments = pair_assignments(&players, (first, second), seed);
                play_assignments(content, &players, assignments, seed)
            }
        }
    }
}

fn pair_cases() -> Vec<SoakCase> {
    let mut cases = Vec::with_capacity(153);
    for first in 0..ORIGINAL_18.len() {
        for second in first + 1..ORIGINAL_18.len() {
            cases.push(SoakCase::Pair(ORIGINAL_18[first], ORIGINAL_18[second]));
        }
    }
    cases
}

fn run_case(content: &ContentStore, case: SoakCase, seed: u64, replay: bool) -> Option<String> {
    let label = case.failure_label();
    match case.play(content, seed) {
        Err(error) => Some(format!("FAIL {label} seed={seed}: {error}")),
        Ok(_) if !replay => None,
        Ok(first) => match case.play(content, seed) {
            Err(error) => Some(format!("FAIL {label} seed={seed} replay: {error}")),
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
                    "FAIL {label} seed={seed}: replay differs at decision {divergence}; prior={prior}; first={:?}; replay={:?}; events_equal={} state_equal={}",
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
    case_index: usize,
    seed: u64,
}

struct JobCursor {
    case_index: usize,
    offset: u64,
    start: u64,
    count: u64,
}

impl JobCursor {
    fn next(&mut self, case_count: usize) -> Option<Job> {
        if self.case_index >= case_count {
            return None;
        }
        let job = Job {
            case_index: self.case_index,
            seed: self.start + self.offset,
        };
        self.offset += 1;
        if self.offset == self.count {
            self.offset = 0;
            self.case_index += 1;
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
    chosen: &[SoakCase],
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
        case_index: 0,
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
                            chosen[job.case_index],
                            job.seed,
                            (job.seed - start) % replay_every == 0,
                        )
                    }));
                    let failure = match result {
                        Ok(failure) => failure,
                        Err(payload) => Some(format!(
                            "FAIL {} seed={}: worker case panicked: {}",
                            chosen[job.case_index].failure_label(),
                            job.seed,
                            panic_message(payload.as_ref())
                        )),
                    };
                    if let Some(message) = failure {
                        failed_by_faction[job.case_index].fetch_add(1, Ordering::Relaxed);
                        failures
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push((job.case_index, job.seed, message));
                    }
                    completed_by_faction[job.case_index].fetch_add(1, Ordering::Relaxed);
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
                        "FAIL {} worker pool: completed {actual} of {count} scheduled cases",
                        chosen[index].failure_label()
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
    failures.sort_by_key(|(case_index, seed, _)| (*case_index, *seed));
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
    let pairs = args.first().is_some_and(|mode| mode == "pairs");
    let chosen: Vec<SoakCase> = match args.first() {
        Some(mode) if mode == "pairs" => pair_cases(),
        Some(faction) if faction == "all" => {
            FACTIONS.iter().copied().map(SoakCase::Faction).collect()
        }
        Some(faction) if FACTIONS.contains(&faction.as_str()) => vec![SoakCase::Faction(
            FACTIONS
                .iter()
                .copied()
                .find(|candidate| *candidate == faction)
                .expect("matched faction"),
        )],
        Some(faction) => {
            eprintln!(
                "unknown faction {faction}; expected one of {}",
                FACTIONS.join(", ")
            );
            std::process::exit(2);
        }
        None => FACTIONS.iter().copied().map(SoakCase::Faction).collect(),
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
    let count = parse(2, if pairs { 2 } else { 25 });
    let replay_every = parse(3, 10);
    if count == 0 || replay_every == 0 || start.checked_add(count).is_none() {
        eprintln!("count and replay_every must be positive and the seed range must not overflow");
        std::process::exit(2);
    }
    let full_campaign = !pairs && chosen.len() == FACTIONS.len() && start == 0 && count >= 200;
    let (completed, failures, messages) = run_parallel(&chosen, start, count, replay_every);
    for message in &messages {
        eprintln!("{message}");
    }
    for (index, case) in chosen.iter().enumerate() {
        match case {
            SoakCase::Faction(faction) => println!(
                "result faction={faction} games={} failures={}",
                completed[index], failures[index]
            ),
            SoakCase::Pair(first, second) => println!(
                "result pair={first}+{second} games={} failures={}",
                completed[index], failures[index]
            ),
        }
    }
    let total = messages.len() as u64;
    println!("replay_every={replay_every}");
    println!(
        "{}: {}={} games={} failures={total}",
        if pairs {
            "PAIR SOAK"
        } else if full_campaign {
            "EXIT-GATE SOAK"
        } else {
            "routine soak (not the exit gate)"
        },
        if pairs { "pairs" } else { "factions" },
        chosen.len(),
        u128::try_from(chosen.len()).unwrap_or(u128::MAX) * u128::from(count)
    );
    if total != 0 {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IN_SCOPE_FACTIONS, ORIGINAL_18, SEATS, SoakCase, legacy_assignments, pair_assignments,
        pair_cases,
    };
    use std::collections::BTreeSet;
    use ti4_model::id::{FactionId, PlayerId};

    #[test]
    fn pair_schedule_is_exactly_the_153_unordered_original_18_pairs() {
        let cases = pair_cases();
        assert_eq!(cases.len(), 153);
        let pairs: BTreeSet<_> = cases
            .iter()
            .map(|case| match case {
                SoakCase::Pair(first, second) => (*first, *second),
                SoakCase::Faction(_) => panic!("pair schedule contained a singleton"),
            })
            .collect();
        assert_eq!(pairs.len(), 153);
        for first in 0..ORIGINAL_18.len() {
            for second in first + 1..ORIGINAL_18.len() {
                assert!(pairs.contains(&(ORIGINAL_18[first], ORIGINAL_18[second])));
            }
        }
    }

    #[test]
    fn pair_seating_is_deterministic_and_has_six_distinct_factions() {
        let players: Vec<PlayerId> = SEATS.iter().map(|seat| PlayerId::new(*seat)).collect();
        for case in pair_cases() {
            let SoakCase::Pair(first, second) = case else {
                panic!("pair schedule contained a singleton");
            };
            for seed in 0..6 {
                let assignment = pair_assignments(&players, (first, second), seed);
                assert_eq!(
                    assignment,
                    pair_assignments(&players, (first, second), seed)
                );
                assert_eq!(assignment.len(), 6);
                let seated: BTreeSet<String> = assignment
                    .values()
                    .map(|faction| faction.as_str().to_owned())
                    .collect();
                assert_eq!(seated.len(), 6);
                assert!(seated.contains(first) && seated.contains(second));
                assert!(seated.iter().all(|faction| {
                    faction == first
                        || faction == second
                        || IN_SCOPE_FACTIONS.contains(&faction.as_str())
                }));
            }
        }
    }

    #[test]
    fn singleton_assignment_keeps_legacy_rotation_and_tribuni_override() {
        let players: Vec<PlayerId> = SEATS.iter().map(|seat| PlayerId::new(*seat)).collect();
        let rotated = legacy_assignments(&players, "yin", 3).expect("valid seed");
        assert_eq!(rotated.get(&players[3]), Some(&FactionId::new("yin")));
        assert_eq!(rotated.get(&players[0]), Some(&FactionId::new("sol")));

        let tribuni = legacy_assignments(&players, "keleresx", 4).expect("valid seed");
        assert_eq!(tribuni.get(&players[3]), Some(&FactionId::new("keleresx")));
        assert_eq!(tribuni.get(&players[4]), Some(&FactionId::new("jolnar")));
    }
}