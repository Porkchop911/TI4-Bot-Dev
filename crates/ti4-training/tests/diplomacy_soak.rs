//! Diplomacy soak: seeded-random seats play whole rounds on a real map with structured diplomacy
//! switched on.
//!
//! Every contact, offer, counter, signal and payment the engine offers is taken at random, so a
//! bundle the engine offers but cannot resolve refuses a step here, and state the validator would
//! reject on reload fails the check at the end.

use std::collections::BTreeMap;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_engine::choice::{Decider, Scripted, SeededRandom};
use ti4_model::DiplomacyEvent;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_training::rollout::{
    OpeningMap, SimulationCapabilities, seated_faction,
    setup_game_with_capabilities_and_decider_factory,
};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

/// A count from the environment, for a longer soak than the default test run
/// (`DIPLOMACY_SOAK_SEEDS=200 DIPLOMACY_SOAK_ROUNDS=6`).
fn from_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn seeded_random_tables_negotiate_through_three_rounds_without_a_refused_step() {
    let content = ContentStore::embedded();
    let factions = FACTIONS.map(FactionId::new);
    let seeds = from_env("DIPLOMACY_SOAK_SEEDS", 6);
    let rounds = u32::try_from(from_env("DIPLOMACY_SOAK_ROUNDS", 3)).expect("rounds fit");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for seed in 0..seeds {
        let players: Vec<PlayerId> = (0..6)
            .map(|index| PlayerId::new(format!("seat{index}")))
            .collect();
        let assignments: BTreeMap<_, _> = players
            .iter()
            .enumerate()
            .map(|(index, player)| (player.clone(), seated_faction(&factions, seed, 0, index)))
            .collect();
        let mut game = setup_game_with_capabilities_and_decider_factory(
            content,
            &players,
            &assignments,
            DEFAULT,
            seed,
            &OpeningMap::RustVaried,
            SimulationCapabilities { diplomacy: true },
            |_| {
                Ok(players
                    .iter()
                    .zip(0_u64..)
                    .map(|(player, index)| {
                        let decider: Box<dyn Decider> =
                            Box::new(SeededRandom::new(seed * 100 + index));
                        (player.clone(), decider)
                    })
                    .collect())
            },
        )
        .expect("the table seats");
        let target = game.state.round + rounds;
        let mut steps = 0_usize;
        while game.state.round < target && !game.state.finished {
            let result = game.step();
            assert!(
                result.error.is_none(),
                "seed {seed}, step {steps}: {:?}",
                result.error
            );
            steps += 1;
            assert!(
                steps < 500_000,
                "seed {seed} never finished {rounds} rounds"
            );
        }

        game.state
            .diplomacy
            .validate(&game.state.seating_order, game.state.round)
            .unwrap_or_else(|error| panic!("seed {seed}: diplomacy state invalid: {error}"));
        for entry in &game.state.diplomacy.journal {
            let name = match entry.event {
                DiplomacyEvent::Offered { .. } => "offered",
                DiplomacyEvent::Countered { .. } => "countered",
                DiplomacyEvent::Accepted { .. } => "accepted",
                DiplomacyEvent::Declined { .. } => "declined",
                DiplomacyEvent::DealSettled { .. } => "settled",
                DiplomacyEvent::SignalEmitted { .. } => "signals",
                DiplomacyEvent::ImmediateApplied { .. } | DiplomacyEvent::PromiseSettled { .. } => {
                    continue;
                }
            };
            *counts.entry(name).or_default() += 1;
        }
    }

    eprintln!("diplomacy soak: {seeds} seeds x {rounds} rounds: {counts:?}");
    for name in [
        "offered",
        "countered",
        "accepted",
        "declined",
        "settled",
        "signals",
    ] {
        assert!(
            counts.get(name).copied().unwrap_or(0) > 0,
            "no {name} across the soak: {counts:?}"
        );
    }
}

#[test]
fn ordinary_decision_ids_replay_the_diplomacy_journal_and_state() {
    let content = ContentStore::embedded();
    let factions = FACTIONS.map(FactionId::new);
    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let assignments: BTreeMap<_, _> = players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.clone(), seated_faction(&factions, 91, 0, index)))
        .collect();
    let mut original = setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &assignments,
        DEFAULT,
        91,
        &OpeningMap::RustVaried,
        SimulationCapabilities { diplomacy: true },
        |_| {
            Ok(players
                .iter()
                .zip(0_u64..)
                .map(|(player, index)| {
                    let decider: Box<dyn Decider> = Box::new(SeededRandom::new(9_100 + index));
                    (player.clone(), decider)
                })
                .collect())
        },
    )
    .expect("the original table seats");
    let target_round = original.state.round + 1;
    while original.state.round < target_round && !original.state.finished {
        let result = original.step();
        assert!(
            result.error.is_none(),
            "original refused: {:?}",
            result.error
        );
    }
    let scripts: BTreeMap<_, _> = players
        .iter()
        .map(|player| (player.clone(), original.table.log.as_script(Some(player))))
        .collect();
    let expected_state = original.state.clone();
    let expected_log = original.table.log.clone();
    let expected_journal = original.state.diplomacy.journal.clone();
    drop(original);

    let mut replay = setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &assignments,
        DEFAULT,
        91,
        &OpeningMap::RustVaried,
        SimulationCapabilities { diplomacy: true },
        |_| {
            Ok(players
                .iter()
                .map(|player| {
                    let decider: Box<dyn Decider> = Box::new(Scripted::new(
                        scripts.get(player).expect("player script").clone(),
                    ));
                    (player.clone(), decider)
                })
                .collect())
        },
    )
    .expect("the replay table seats");
    while replay.state.round < target_round && !replay.state.finished {
        let result = replay.step();
        assert!(result.error.is_none(), "replay refused: {:?}", result.error);
    }

    assert_eq!(replay.table.log, expected_log);
    assert_eq!(replay.state.diplomacy.journal, expected_journal);
    assert!(replay.state.identical(&expected_state));
}

#[derive(Debug)]
struct CostProbe {
    diplomacy: bool,
    decisions: usize,
    steps: usize,
    max_options: usize,
    elapsed_seconds: f64,
    state_bytes: usize,
    journal_events: usize,
    journal_bytes: usize,
}

fn cost_probe(diplomacy: bool) -> CostProbe {
    let content = ContentStore::embedded();
    let factions = FACTIONS.map(FactionId::new);
    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let assignments: BTreeMap<_, _> = players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.clone(), seated_faction(&factions, 117, 0, index)))
        .collect();
    let mut game = setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &assignments,
        DEFAULT,
        117,
        &OpeningMap::RustVaried,
        SimulationCapabilities { diplomacy },
        |_| {
            Ok(players
                .iter()
                .zip(0_u64..)
                .map(|(player, index)| {
                    let decider: Box<dyn Decider> = Box::new(SeededRandom::new(11_700 + index));
                    (player.clone(), decider)
                })
                .collect())
        },
    )
    .expect("probe table seats");
    let target_round = game.state.round + 2;
    let started = Instant::now();
    let mut steps = 0;
    while game.state.round < target_round && !game.state.finished {
        let result = game.step();
        assert!(result.error.is_none(), "probe refused: {:?}", result.error);
        steps += 1;
    }
    CostProbe {
        diplomacy,
        decisions: game.table.log.len(),
        steps,
        max_options: game
            .table
            .log
            .records
            .iter()
            .filter(|record| {
                record
                    .context
                    .as_ref()
                    .is_some_and(|context| context.subtype.starts_with("diplomacy_"))
            })
            .map(|record| record.offered.len())
            .max()
            .unwrap_or(0),
        elapsed_seconds: started.elapsed().as_secs_f64(),
        state_bytes: serde_json::to_vec(&game.state)
            .expect("state serializes")
            .len(),
        journal_events: game.state.diplomacy.journal.len(),
        journal_bytes: serde_json::to_vec(&game.state.diplomacy.journal)
            .expect("journal serializes")
            .len(),
    }
}

/// Run manually before widening diplomacy self-play:
/// `cargo test -p ti4-training --test diplomacy_soak diplomacy_cost_probe -- --ignored --nocapture`.
#[test]
#[ignore = "performance evidence is machine-dependent and intentionally opt-in"]
fn diplomacy_cost_probe() {
    let disabled = cost_probe(false);
    let enabled = cost_probe(true);
    for probe in [&disabled, &enabled] {
        eprintln!(
            "diplomacy={} decisions={} steps={} max_options={} seconds={:.3} state_bytes={} journal_events={} journal_bytes={}",
            probe.diplomacy,
            probe.decisions,
            probe.steps,
            probe.max_options,
            probe.elapsed_seconds,
            probe.state_bytes,
            probe.journal_events,
            probe.journal_bytes,
        );
    }
    assert!(!disabled.diplomacy && enabled.diplomacy);
    assert_eq!(disabled.journal_events, 0);
    assert_eq!(disabled.journal_bytes, 2);
    assert!(
        enabled.max_options <= 29,
        "24 candidates plus four signals and no-op"
    );
}
