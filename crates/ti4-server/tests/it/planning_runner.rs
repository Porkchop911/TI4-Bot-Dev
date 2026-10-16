//! The transcript, rather than the discarded fork, is the privacy contract.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use ti4_content::{ContentStore, galaxy::all_systems};
use ti4_engine::choice::{AlwaysDecline, Scripted, Table};
use ti4_engine::fixtures::{
    a_system_where, game, hub_with_centre, hub_with_outer, put, put_on_planet,
};
use ti4_engine::game::Game;
use ti4_engine::observation::ExecutionObservation;
use ti4_engine::timing::{Ability, Relation};
use ti4_model::content_types::POK;
use ti4_model::id::{ActionCardId, PlanetId, PlayerId, SecretObjectiveId, SystemId, TechnologyId};
use ti4_model::state::Phase;
use ti4_server::planning::RecordingDecider;
use ti4_server::planning::runner::{
    ASSUMPTION, FailureCategory, PlanningEnvelope, PlanningRunner, PlanningUpdate, PlayerPlan,
    StopReason, SubmissionError,
};
use ti4_server::projection::project_game_view;
use ti4_server::protocol::status::ViewerRole;

const DEADLINE: Duration = Duration::from_secs(5);

fn within_deadline<T: Send + 'static>(operation: impl FnOnce() -> T + Send + 'static) -> T {
    let (finished, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        finished.send(operation()).ok();
    });
    let value = result
        .recv_timeout(DEADLINE)
        .expect("worker shutdown is bounded");
    worker.join().unwrap();
    value
}

fn empty_system() -> String {
    all_systems(ContentStore::embedded(), POK)
        .iter()
        .find(|(_, system)| {
            system.planets().is_empty() && !system.is_anomaly() && !system.is_hyperlane()
        })
        .unwrap()
        .0
        .to_string()
}

fn checkpoint(rift: bool, carrier: bool) -> (Game<'static>, SystemId, SystemId) {
    let hub = if rift {
        hub_with_outer(&a_system_where("gravity rift"))
    } else {
        hub_with_centre(&empty_system())
    };
    let destination = SystemId::new(&hub.centre);
    let origin = SystemId::new(&hub.outer[0]);
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    put(
        &mut state,
        &origin,
        if carrier { "carrier" } else { "destroyer" },
        &PlayerId::new("b"),
        1,
    );
    if carrier {
        put(&mut state, &origin, "fighter", &PlayerId::new("b"), 1);
    }
    (
        Game::new(state, ContentStore::embedded()).with_galaxy(hub.galaxy),
        destination,
        origin,
    )
}

fn runner(game: &Game<'static>) -> PlanningRunner {
    PlanningRunner::start(game, PlayerId::new("b"), PlayerPlan::new(10), 32)
}

#[test]
fn editing_an_early_move_rebinds_later_ships_and_cargo_to_fresh_indexes() {
    let (mut game, target, origin) = checkpoint(false, true);
    put(&mut game.state, &origin, "cruiser", &PlayerId::new("b"), 1);
    game.state.system_mut(&origin).units.rotate_right(1);
    // Cruiser index 0 precedes carrier index 1. After moving the cruiser,
    // the recorded carrier's ID is 0; deleting that early move must rebind it to 1.
    let mut runner = runner(&game);
    let transcript = collect(
        &runner,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move|{origin}|0"),
            format!("move|{origin}|0"),
            "load|0".into(),
            "done_moving".into(),
        ],
    );
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    let mut revised = runner.plan().recorded_decisions;
    assert_eq!(revised.remove(2).payload["unit"], "cruiser");
    assert_eq!(revised[2].option_id, format!("move|{origin}|0"));
    runner.edit(&game, 11, revised);
    let updated = collect(&runner, &[]);
    assert_eq!(reason(&updated), StopReason::MovementComplete);
    let fresh = runner.plan().recorded_decisions;
    assert_eq!(fresh[2].option_id, format!("move|{origin}|1"));
    assert_eq!(fresh[2].payload["unit"], "carrier");
    assert_eq!(fresh[3].payload["unit"], "fighter");
    let PlanningUpdate::Stopped {
        last_safe_publication: Some(publication),
        ..
    } = &updated.last().unwrap().update
    else {
        panic!("stopped publication")
    };
    let units = &publication.position.board.systems[&target].units;
    assert_eq!(units.len(), 2);
    assert!(
        units
            .iter()
            .all(|unit| unit.unit_type.as_str() != "cruiser")
    );
    assert_eq!(game.state.system_state(&origin).units.len(), 3);
}

#[test]
fn editing_one_load_keeps_later_cargo_and_rebinds_hold_counters() {
    let (mut game, target, origin) = checkpoint(false, true);
    put(&mut game.state, &origin, "fighter", &PlayerId::new("b"), 1);
    let mut runner = runner(&game);
    collect(
        &runner,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move|{origin}|0"),
            "load|0".into(),
            "load|1".into(),
            "done_moving".into(),
        ],
    );
    let mut revised = runner.plan().recorded_decisions;
    revised.remove(3);
    // With one candidate left behind the new hold needs an explicit finish.
    // Record that instruction from the normal engine instead of inventing its payload.
    revised.pop();
    runner.edit(&game, 11, revised);
    let updated = collect(&runner, &["done_loading".into(), "done_moving".into()]);
    // The fighter left at origin now needs a live capacity decision, outside
    // the preview slice. The revised movement prefix still fully reconstructs.
    assert_eq!(reason(&updated), StopReason::UnsupportedOffer);
    assert_eq!(updated.last().unwrap().progress.remaining, 0);
    let fresh = runner.plan().recorded_decisions;
    assert_eq!(fresh[3].option_id, "load|0");
    assert_eq!(fresh[3].payload["loaded_fighters"], 0);
    assert_eq!(fresh[3].payload["capacity_remaining"], 4);
}

#[test]
fn cargo_replay_keeps_the_pickup_system_when_identical_origin_cargo_appears() {
    let hub = ti4_engine::fixtures::plain_hub();
    let origin = SystemId::new(&hub.outer[0]);
    let target = SystemId::new(hub.across(&hub.outer[0]));
    let pickup = SystemId::new(&hub.centre);
    let player = PlayerId::new("b");
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    state
        .player_mut(&player)
        .unwrap()
        .technologies
        .insert(TechnologyId::new("gd"));
    put(&mut state, &origin, "carrier", &player, 1);
    put(&mut state, &pickup, "fighter", &player, 1);
    let mut game = Game::new(state, ContentStore::embedded()).with_galaxy(hub.galaxy);
    let mut runner = runner(&game);
    collect(
        &runner,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move_gd|{origin}|0"),
            "load|0".into(),
            "done_moving".into(),
        ],
    );
    let mut revised = runner.plan().recorded_decisions;
    // A new indistinguishable fighter at origin changes the candidate indexes.
    // Replay must still load the fighter from the intermediate system, not this one.
    put(&mut game.state, &origin, "fighter", &PlayerId::new("b"), 1);
    revised.pop();
    runner.edit(&game, 11, revised);
    let updated = collect(&runner, &["done_loading".into(), "done_moving".into()]);
    assert_eq!(updated.last().unwrap().progress.remaining, 0);
    let fresh = runner.plan().recorded_decisions;
    assert_eq!(fresh[3].option_id, "load|1");
    assert_eq!(fresh[3].payload["pickup_system"], pickup.to_string());
}

#[test]
fn recorded_gravity_drive_then_ionian_moves_replay_in_sequence() {
    let hub = hub_with_outer(&empty_system());
    let origin = SystemId::new(hub.across(&hub.outer[0]));
    let target = SystemId::new(&hub.outer[0]);
    let player = PlayerId::new("b");
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    state
        .player_mut(&player)
        .unwrap()
        .technologies
        .insert(TechnologyId::new("gd"));
    state
        .system_mut(&SystemId::new(&hub.centre))
        .set_control(PlanetId::new("tempesta"), player.clone());
    put(&mut state, &origin, "carrier", &player, 2);
    let game = Game::new(state, ContentStore::embedded()).with_galaxy(hub.galaxy);
    let mut runner = runner(&game);
    let transcript = collect(
        &runner,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move_gd|{origin}|0"),
            format!("move_ion|{origin}|0"),
            "done_moving".into(),
        ],
    );
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    let recorded = runner.plan().recorded_decisions;
    assert_eq!(recorded[2].payload["gravity_drive"], true);
    assert_eq!(recorded[3].payload["ionian"], true);
    runner.edit(&game, 11, recorded);
    assert_eq!(reason(&collect(&runner, &[])), StopReason::MovementComplete);
}

fn next(runner: &PlanningRunner) -> PlanningEnvelope {
    runner
        .recv_timeout(DEADLINE)
        .expect("worker publishes before deadline")
}

fn collect(runner: &PlanningRunner, answers: &[String]) -> Vec<PlanningEnvelope> {
    let mut transcript = Vec::new();
    let mut answers = answers.iter();
    loop {
        let envelope = next(runner);
        assert_eq!(envelope.assumptions, vec![ASSUMPTION]);
        if let PlanningUpdate::SafeOffer(offer) = &envelope.update {
            if envelope.awaiting_answer {
                let answer = answers.next().expect("expected question");
                runner
                    .submit(envelope.identity, answer)
                    .unwrap_or_else(|error| {
                        panic!("safe answer {answer}: {error:?}; offer: {:?}", offer.choice)
                    });
            }
            assert!(offer.choice.is_some());
        }
        let terminal = matches!(
            envelope.update,
            PlanningUpdate::Stopped { .. } | PlanningUpdate::Failed(_)
        );
        transcript.push(envelope);
        if terminal {
            assert!(
                answers.next().is_none(),
                "unexpected terminal: {:?}",
                transcript.last()
            );
            return transcript;
        }
    }
}

fn reason(transcript: &[PlanningEnvelope]) -> StopReason {
    match &transcript.last().unwrap().update {
        PlanningUpdate::Stopped { reason, .. } => *reason,
        other => panic!("expected stop, got {other:?}"),
    }
}

fn answers(destination: &SystemId, origin: &SystemId, cargo: bool) -> Vec<String> {
    let mut answers = vec![
        "tactical".into(),
        destination.to_string(),
        format!("move|{origin}|0"),
    ];
    if cargo {
        answers.push("load|0".into());
    }
    answers.push("done_moving".into());
    answers
}

#[test]
fn recording_receipts_exclude_rejected_requests_and_survive_refresh_and_restoration() {
    let (live, _, _) = checkpoint(false, false);
    let mut runner = runner(&live);
    let old = next(&runner);
    assert!(old.awaiting_answer);
    assert!(old.recorded_request_ids.is_empty());
    runner
        .submit_with_request_id(old.identity, "tactical", Some("winner"))
        .unwrap();
    let recorded = loop {
        let update = next(&runner);
        if update.awaiting_answer {
            break update;
        }
    };
    assert_eq!(recorded.recorded_request_ids, ["winner"]);
    assert_eq!(
        runner.submit_with_request_id(old.identity, "tactical", Some("loser")),
        Err(SubmissionError::Retired)
    );
    let retained = runner.plan();
    assert_eq!(retained.recorded_request_ids, ["winner"]);
    runner.refresh(&live, 11);
    loop {
        let update = next(&runner);
        assert_eq!(update.recorded_request_ids, ["winner"]);
        if update.awaiting_answer {
            break;
        }
    }
    let saved = serde_json::to_string(&retained).unwrap();
    let restored = PlanningRunner::start(
        &live,
        PlayerId::new("b"),
        serde_json::from_str(&saved).unwrap(),
        32,
    );
    assert_eq!(next(&restored).recorded_request_ids, ["winner"]);
    runner.edit(&live, 12, vec![]);
    assert!(next(&runner).recorded_request_ids.is_empty());
}

#[test]
fn two_player_movement_and_cargo_match_the_normal_fork_without_touching_live_state() {
    let (live, destination, origin) = checkpoint(false, true);
    let original = serde_json::to_value(&live.state).unwrap();
    let script = answers(&destination, &origin, true);
    let runner = runner(&live);
    let transcript = collect(&runner, &script);
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    let offered: Vec<_> = transcript
        .iter()
        .filter_map(|envelope| match &envelope.update {
            PlanningUpdate::SafeOffer(offer) => Some(
                offer
                    .choice
                    .as_ref()
                    .unwrap()
                    .context
                    .as_ref()
                    .unwrap()
                    .subtype
                    .as_str(),
            ),
            _ => None,
        })
        .collect();
    assert!(offered.contains(&"activate_system"));
    assert!(offered.contains(&"movement_step"));
    assert!(offered.contains(&"load_cargo"));

    let mut normal = live.fork();
    normal
        .prepare_hypothetical_turn(&PlayerId::new("b"))
        .unwrap();
    normal.timing.set_participation(Arc::new(|ability| {
        ability.owner == PlayerId::new("b") || !ability.optional
    }));
    normal.table = Table::with_default(Box::new(Scripted::new(script)));
    for _ in 0..16 {
        assert!(normal.step().error.is_none());
        if normal
            .events
            .iter()
            .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        {
            break;
        }
    }
    let expected = project_game_view(&normal.state, &ViewerRole::Player(PlayerId::new("b")));
    let PlanningUpdate::Stopped {
        last_safe_publication: Some(last),
        ..
    } = &transcript.last().unwrap().update
    else {
        panic!("last safe state")
    };
    assert_eq!(last.position, expected);
    assert_eq!(runner.plan().recorded_decisions.len(), 5);
    assert_eq!(serde_json::to_value(&live.state).unwrap(), original);
    assert!(live.rolls().is_empty());
    assert!(live.table.log.is_empty());
    // Driving both original RNG copies later also compares their stream positions.
    let original_fork = live.fork();
    assert_eq!(
        serde_json::to_value(&original_fork.state).unwrap(),
        original
    );
}

#[test]
fn gravity_rift_transcripts_are_identical_and_keep_the_uncertainty_crossing_answer() {
    let mut transcripts = Vec::new();
    let mut live_outcomes = std::collections::BTreeSet::new();
    for seed in 0..12 {
        let (mut live, destination, origin) = checkpoint(true, false);
        live.state.rng_seed = seed;
        // Game owns its RNG separately from state, so construct it with this seed.
        let galaxy = live.galaxy().unwrap().clone();
        live = Game::new(live.state.clone(), ContentStore::embedded()).with_galaxy(galaxy);
        let live_observation = ExecutionObservation::default();
        live.bind_observation(live_observation.clone());
        let untouched_rng = live.fork();
        let input = vec![
            "tactical".into(),
            destination.to_string(),
            format!("move|{origin}|0"),
        ];
        let runner = runner(&live);
        let transcript = collect(&runner, &input);
        assert_eq!(reason(&transcript), StopReason::Uncertainty);
        assert_eq!(runner.plan().recorded_decisions.len(), 3);
        assert_eq!(
            transcript
                .last()
                .unwrap()
                .progress
                .nested_answers_since_checkpoint,
            1
        );
        assert!(transcript.iter().all(|envelope| {
            match &envelope.update {
                PlanningUpdate::SafeStep(state) => !state
                    .events
                    .iter()
                    .any(|event| event == "SHIP_MOVED" || event.contains("LOST")),
                _ => true,
            }
        }));
        transcripts.push(serde_json::to_value(transcript).unwrap());

        let mut actual = live.fork();
        actual
            .prepare_hypothetical_turn(&PlayerId::new("b"))
            .unwrap();
        actual.table = Table::with_default(Box::new(Scripted::new(input)));
        for _ in 0..3 {
            assert!(actual.step().error.is_none());
        }
        live_outcomes.insert(actual.state.system_state(&destination).units.len());
        assert!(live.rolls().is_empty());
        assert_eq!(live_observation.activity(), Default::default());
        let mut expected = untouched_rng.fork();
        expected
            .prepare_hypothetical_turn(&PlayerId::new("b"))
            .unwrap();
        expected.table = Table::with_default(Box::new(Scripted::new(vec![
            "tactical".into(),
            destination.to_string(),
            format!("move|{origin}|0"),
        ])));
        for _ in 0..3 {
            assert!(expected.step().error.is_none());
        }
        assert_eq!(
            actual.rolls(),
            expected.rolls(),
            "live RNG positions are unchanged"
        );
    }
    assert_eq!(live_outcomes.len(), 2, "seeds really change survival");
    assert!(transcripts.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn frontier_ordered_draw_stops_without_card_rewards_or_followup_offers() {
    let mut transcripts = Vec::new();
    for card in ["ent", "lc1"] {
        let (mut live, destination, origin) = checkpoint(false, false);
        live.state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .technologies
            .insert(TechnologyId::new("det"));
        live.state.frontier_tokens.insert(destination.clone());
        live.state
            .exploration_decks
            .insert("FRONTIER".into(), vec![card.into()]);
        let runner = runner(&live);
        let transcript = collect(&runner, &answers(&destination, &origin, false));
        assert_eq!(reason(&transcript), StopReason::Uncertainty);
        assert_eq!(runner.plan().recorded_decisions.len(), 4);
        let text = serde_json::to_string(&transcript).unwrap();
        assert!(!text.contains(&format!("\"{card}\"")));
        assert!(!text.contains("FRONTIER_EXPLORED"));
        transcripts.push(serde_json::to_value(transcript).unwrap());
    }
    assert_eq!(transcripts[0], transcripts[1]);
}

#[test]
fn opponent_optional_conditions_are_never_consulted_and_live_timing_still_consults_them() {
    let mut transcripts = Vec::new();
    for held in [false, true] {
        let (mut live, destination, origin) = checkpoint(false, false);
        live.state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards = vec![ActionCardId::new(if held {
            "flank_speed"
        } else {
            "unplayable"
        })];
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        live.timing.register([Ability::new(
            "test_reaction",
            PlayerId::new("a"),
            "TURN_BEGAN",
            Relation::When,
            Arc::new(|_, _| Ok(())),
        )
        .with_optional(true)
        .with_condition(Arc::new(move |_, _| {
            counted.fetch_add(1, Ordering::SeqCst);
            held
        }))]);
        let runner = runner(&live);
        let transcript = collect(&runner, &answers(&destination, &origin, false));
        assert_eq!(reason(&transcript), StopReason::MovementComplete);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        transcripts.push(serde_json::to_value(transcript).unwrap());
        live.table = Table::with_default(Box::new(AlwaysDecline));
        let _ = live.step();
        assert!(calls.load(Ordering::SeqCst) > 0);
    }
    assert_eq!(transcripts[0], transcripts[1]);
}

/// Mandatory effects run on the fork whoever owns them. One that hands the
/// planner a hidden card stops the preview without publishing which card.
#[test]
fn a_mandatory_effect_that_reveals_a_hidden_card_stops_identically_for_either_card() {
    let mut transcripts = Vec::new();
    for card in ["flank_speed", "sabotage"] {
        let (mut live, _, _) = checkpoint(false, false);
        live.state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards = vec![ActionCardId::new(card)];
        live.timing.register([Ability::stateful(
            "private_mandatory",
            PlayerId::new("a"),
            "TURN_BEGAN",
            Relation::When,
            Arc::new(|_, _, context| {
                let taken = std::mem::take(
                    &mut context
                        .state
                        .player_mut(&PlayerId::new("a"))
                        .unwrap()
                        .action_cards,
                );
                context
                    .state
                    .player_mut(&PlayerId::new("b"))
                    .unwrap()
                    .action_cards
                    .extend(taken);
                Ok(())
            }),
        )]);
        let transcript = collect(&runner(&live), &[]);
        assert_eq!(reason(&transcript), StopReason::KnowledgeChanged);
        let text = serde_json::to_string(&transcript).unwrap();
        assert!(!text.contains(card), "{text}");
        transcripts.push(serde_json::to_value(transcript).unwrap());
    }
    assert_eq!(transcripts[0], transcripts[1]);
}

/// A mandatory effect may read what the planner cannot see as long as nothing
/// the planner is shown depends on it.
#[test]
fn a_mandatory_effect_reading_an_opponents_hand_leaves_the_transcript_unchanged() {
    let mut transcripts = Vec::new();
    for card in ["flank_speed", "sabotage"] {
        let (mut live, destination, origin) = checkpoint(false, false);
        live.state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .action_cards = vec![ActionCardId::new(card)];
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        live.timing.register([Ability::stateful(
            "private_mandatory",
            PlayerId::new("a"),
            "TURN_BEGAN",
            Relation::When,
            Arc::new(move |_, _, context| {
                let hand = &context
                    .state
                    .player(&PlayerId::new("a"))
                    .unwrap()
                    .action_cards;
                counted.fetch_add(hand.len(), Ordering::SeqCst);
                Ok(())
            }),
        )]);
        let transcript = collect(&runner(&live), &answers(&destination, &origin, false));
        assert_eq!(reason(&transcript), StopReason::MovementComplete);
        assert!(calls.load(Ordering::SeqCst) > 0);
        transcripts.push(serde_json::to_value(transcript).unwrap());
    }
    assert_eq!(transcripts[0], transcripts[1]);
}

#[test]
fn refresh_wakes_waiting_input_and_rejects_old_submissions_and_queued_output() {
    let (live, _, _) = checkpoint(false, false);
    let mut runner = runner(&live);
    let old = next(&runner);
    assert!(matches!(old.update, PlanningUpdate::SafeOffer(_)));
    let checkpoint = live.fork_for_worker();
    runner = within_deadline(move || {
        let checkpoint = checkpoint();
        runner.refresh(&checkpoint, 11);
        runner
    });
    assert_eq!(
        runner.submit(old.identity, "tactical"),
        Err(SubmissionError::Retired)
    );
    let new = next(&runner);
    assert_eq!(new.identity.generation_id, old.identity.generation_id + 1);
    assert_eq!(new.identity.checkpoint_id, 11);
    within_deadline(move || runner.cancel());
}

#[test]
fn refresh_replays_nested_answers_and_mismatch_preserves_the_full_script() {
    let (live, destination, origin) = checkpoint(false, true);
    let mut runner = runner(&live);
    let script = answers(&destination, &origin, true);
    let first = collect(&runner, &script);
    assert_eq!(reason(&first), StopReason::MovementComplete);
    let retained = runner.plan().recorded_decisions;
    runner.refresh(&live, 11);
    let replayed = collect(&runner, &[]);
    assert_eq!(reason(&replayed), StopReason::MovementComplete);
    assert_eq!(replayed.last().unwrap().progress.replayed, retained.len());
    assert_eq!(runner.plan().recorded_decisions, retained);
    let mut changed = live.fork();
    changed.state.system_mut(&origin).units.clear();
    runner.refresh(&changed, 12);
    let mismatch = collect(&runner, &[]);
    assert_eq!(reason(&mismatch), StopReason::ReplayMismatch);
    assert_eq!(mismatch.last().unwrap().progress.replayed, 2);
    assert_eq!(
        mismatch.last().unwrap().progress.remaining,
        retained.len() - 2
    );
    assert_eq!(runner.plan().recorded_decisions, retained);
}

#[test]
fn preparation_failure_is_sanitized_and_separate_from_expected_stops() {
    let (mut live, _, _) = checkpoint(false, false);
    live.state.phase = Phase::Status;
    let runner = runner(&live);
    let envelope = next(&runner);
    assert_eq!(
        envelope.update,
        PlanningUpdate::Failed(FailureCategory::Preparation)
    );
    assert!(
        !serde_json::to_string(&envelope)
            .unwrap()
            .contains("action phase")
    );
}

fn scanlink_checkpoint() -> (Game<'static>, SystemId) {
    let planets = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
    let (planet, record) = planets
        .iter()
        .find(|(_, planet)| {
            planet.system_id().is_some()
                && planet.homeworld_of().is_none()
                && !planet.planet_types().is_empty()
        })
        .unwrap();
    let origin = SystemId::new(record.system_id().unwrap());
    let hub = hub_with_centre(origin.as_str());
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    let mut live = Game::new(state, ContentStore::embedded()).with_galaxy(hub.galaxy);
    live.state
        .player_mut(&PlayerId::new("b"))
        .unwrap()
        .technologies
        .insert(TechnologyId::new("sdn"));
    put_on_planet(
        &mut live.state,
        &origin,
        &PlanetId::new(*planet),
        "infantry",
        &PlayerId::new("b"),
        1,
    );
    (live, origin)
}

/// The planner's own technology asks its ordinary question in a draft.
/// Declining goes on; exploring draws a card, which ends the preview there
/// without showing what was drawn.
#[test]
fn the_planners_own_ability_is_offered_and_its_hidden_outcome_is_never_shown() {
    let (live, target) = scanlink_checkpoint();
    let runner = runner(&live);
    let declined = collect(
        &runner,
        &[
            "tactical".into(),
            target.to_string(),
            "decline".into(),
            "done_moving".into(),
        ],
    );
    assert_eq!(reason(&declined), StopReason::MovementComplete);
    assert!(
        serde_json::to_string(&declined)
            .unwrap()
            .contains("scanlink_explore")
    );
    assert_eq!(runner.plan().recorded_decisions.len(), 4);

    // The one planet the fixture garrisons.
    let planet = live
        .state
        .system_state(&target)
        .planet_units
        .iter()
        .find(|(_, units)| !units.is_empty())
        .map(|(planet, _)| planet.to_string())
        .unwrap();
    let mut transcripts = Vec::new();
    for reverse in [false, true] {
        let (mut live, target) = scanlink_checkpoint();
        if reverse {
            for deck in live.state.exploration_decks.values_mut() {
                deck.reverse();
            }
        }
        let cards: Vec<String> = live
            .state
            .exploration_decks
            .values()
            .flatten()
            .map(ToString::to_string)
            .collect();
        let explored = collect(
            &crate::planning_runner::runner(&live),
            &["tactical".into(), target.to_string(), planet.to_string()],
        );
        assert!(matches!(
            reason(&explored),
            StopReason::Uncertainty | StopReason::KnowledgeChanged
        ));
        let text = serde_json::to_string(&explored).unwrap();
        assert!(
            cards
                .iter()
                .all(|card| !text.contains(&format!("\"{card}\"")))
        );
        transcripts.push(
            explored
                .into_iter()
                .map(|envelope| serde_json::to_value(envelope.update).unwrap())
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(transcripts[0], transcripts[1]);
}

#[test]
fn replay_supplies_the_recorded_answer_to_the_planners_own_ability() {
    let (live, target) = scanlink_checkpoint();
    let mut normal = live.fork();
    normal
        .prepare_hypothetical_turn(&PlayerId::new("b"))
        .unwrap();
    let (recorder, recording) = RecordingDecider::new(Box::new(Scripted::new([
        "tactical".into(),
        target.to_string(),
        "decline".into(),
    ])));
    normal.table = Table::with_default(Box::new(recorder));
    assert!(normal.step().error.is_none());
    assert!(normal.step().error.is_none());
    let retained = recording.lock().unwrap().clone();
    assert_eq!(retained.len(), 3);
    let plan = PlayerPlan {
        revision: 1,
        reset_revision: 0,
        editing_movement: false,
        movement_edit_revision: 0,
        revalidation_start: None,
        replacing_movement: false,
        base_checkpoint_id: 10,
        recorded_decisions: retained.clone(),
        recorded_request_ids: vec![],
    };
    let runner = PlanningRunner::start(&live, PlayerId::new("b"), plan, 16);
    // The recorded answer to the planner's own ability is replayed like any other.
    let transcript = collect(&runner, &["done_moving".into()]);
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    assert_eq!(runner.plan().recorded_decisions.len(), 4);
    assert_eq!(runner.plan().recorded_decisions[..3], retained[..]);
}

#[test]
fn replay_stops_at_a_rift_before_consuming_a_matching_later_answer() {
    let (live, destination, origin) = checkpoint(true, false);
    let mut normal = live.fork();
    normal
        .prepare_hypothetical_turn(&PlayerId::new("b"))
        .unwrap();
    let (recorder, recording) = RecordingDecider::new(Box::new(Scripted::new(answers(
        &destination,
        &origin,
        false,
    ))));
    normal.table = Table::with_default(Box::new(recorder));
    for _ in 0..4 {
        let _ = normal.step();
    }
    let retained = recording.lock().unwrap().clone();
    assert_eq!(retained.len(), 4);
    let runner = PlanningRunner::start(
        &live,
        PlayerId::new("b"),
        PlayerPlan {
            revision: 1,
            reset_revision: 0,
            editing_movement: false,
            movement_edit_revision: 0,
            revalidation_start: None,
            replacing_movement: false,
            base_checkpoint_id: 10,
            recorded_decisions: retained.clone(),
            recorded_request_ids: vec![],
        },
        16,
    );
    let transcript = collect(&runner, &[]);
    assert_eq!(reason(&transcript), StopReason::Uncertainty);
    assert_eq!(transcript.last().unwrap().progress.replayed, 3);
    assert_eq!(transcript.last().unwrap().progress.remaining, 1);
    assert_eq!(runner.plan().recorded_decisions, retained);
}

#[test]
fn other_player_question_stops_without_publishing_their_options() {
    let (mut live, target, _) = checkpoint(false, false);
    put(
        &mut live.state,
        &target,
        "destroyer",
        &PlayerId::new("a"),
        1,
    );
    live.state
        .player_mut(&PlayerId::new("a"))
        .unwrap()
        .technologies
        .insert(TechnologyId::new("nf"));
    let transcript = collect(&runner(&live), &["tactical".into(), target.to_string()]);
    assert_eq!(reason(&transcript), StopReason::OtherPlayerRequired);
    assert!(
        !serde_json::to_string(&transcript)
            .unwrap()
            .contains("nullification_field_end_turn")
    );
}

/// The planner's own optional abilities are offered at their window, as in
/// live play. Declining leaves the effect unrun; accepting runs it on the fork.
#[test]
fn the_planners_own_optional_ability_can_be_declined_or_used_in_a_draft() {
    for accept in [false, true] {
        let (mut live, destination, origin) = checkpoint(false, false);
        let ran = Arc::new(AtomicUsize::new(0));
        let counted = ran.clone();
        live.timing.register([Ability::new(
            "inspect_and_restore",
            PlayerId::new("b"),
            "TURN_BEGAN",
            Relation::When,
            Arc::new(move |_, _| {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
        )
        .with_optional(true)
        .with_condition(Arc::new(|_, _| true))]);
        let mut script = vec![if accept {
            "inspect_and_restore".to_owned()
        } else {
            "decline".to_owned()
        }];
        script.extend(answers(&destination, &origin, false));
        let transcript = collect(&runner(&live), &script);
        assert_eq!(reason(&transcript), StopReason::MovementComplete);
        assert_eq!(ran.load(Ordering::SeqCst), usize::from(accept));
    }
}

#[test]
fn uncontested_planetary_aftermath_can_complete_without_revealing_events() {
    let (live, _, origin) = checkpoint(false, false);
    let transcript = collect(
        &runner(&live),
        &["tactical".into(), origin.to_string(), "done_moving".into()],
    );
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    assert_no_events(&transcript);
}

fn planetary_checkpoint(production: bool) -> (Game<'static>, SystemId, Vec<PlanetId>) {
    let content = ContentStore::embedded();
    let hub = hub_with_centre("35");
    let system = SystemId::new(&hub.centre);
    let planets: Vec<_> = ti4_content::galaxy::system(content, "35", POK)
        .unwrap()
        .planets()
        .iter()
        .map(|id| PlanetId::new(*id))
        .collect();
    let player = PlayerId::new("b");
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    put(&mut state, &system, "carrier", &player, 1);
    put(&mut state, &system, "infantry", &player, 1);
    if production {
        for planet in &planets {
            state
                .system_mut(&system)
                .set_control(planet.clone(), player.clone());
            put_on_planet(&mut state, &system, planet, "spacedock", &player, 1);
        }
        state.player_mut(&player).unwrap().trade_goods = 5;
    }
    (
        Game::new(state, content).with_galaxy(hub.galaxy),
        system,
        planets,
    )
}

fn assert_no_events(transcript: &[PlanningEnvelope]) {
    for envelope in transcript {
        let publication = match &envelope.update {
            PlanningUpdate::SafeOffer(publication) | PlanningUpdate::SafeStep(publication) => {
                Some(publication)
            }
            PlanningUpdate::Stopped {
                last_safe_publication,
                ..
            } => last_safe_publication.as_ref(),
            _ => None,
        };
        if let Some(publication) = publication {
            assert!(publication.events.is_empty());
        }
    }
}

#[test]
fn expansion_records_landing_but_stops_before_exploration_results() {
    let (live, system, planets) = planetary_checkpoint(false);
    let original = live.state.clone();
    let planner = runner(&live);
    let transcript = collect(
        &planner,
        &[
            "tactical".into(),
            system.to_string(),
            "done_moving".into(),
            format!("commit|0|{}", planets[0]),
        ],
    );
    assert_eq!(reason(&transcript), StopReason::Uncertainty);
    assert_eq!(
        planner
            .plan()
            .recorded_decisions
            .last()
            .unwrap()
            .context
            .as_ref()
            .unwrap()
            .subtype,
        "commit_ground_forces"
    );
    assert_no_events(&transcript);
    assert_eq!(live.state, original);
    let PlanningUpdate::Stopped {
        last_safe_publication: Some(publication),
        ..
    } = &transcript.last().unwrap().update
    else {
        panic!("safe landing offer")
    };
    assert_eq!(
        publication
            .choice
            .as_ref()
            .unwrap()
            .context
            .as_ref()
            .unwrap()
            .subtype,
        "commit_ground_forces"
    );
    assert!(
        publication.position.board.systems[&system]
            .planets
            .get(&planets[0])
            .is_none()
    );
    let mut different_draws = live.fork();
    for deck in different_draws.state.exploration_decks.values_mut() {
        deck.reverse();
    }
    let alternate = collect(
        &runner(&different_draws),
        &[
            "tactical".into(),
            system.to_string(),
            "done_moving".into(),
            format!("commit|0|{}", planets[0]),
        ],
    );
    assert_eq!(
        serde_json::to_value(&transcript).unwrap(),
        serde_json::to_value(&alternate).unwrap()
    );
}

#[test]
fn landing_production_payment_and_placement_match_normal_execution_and_replay() {
    let (mut live, system, planets) = planetary_checkpoint(true);
    let player = PlayerId::new("b");
    live.state
        .player_mut(&player)
        .unwrap()
        .technologies
        .insert(TechnologyId::new("st"));
    let original = live.state.clone();
    let script = vec![
        "tactical".into(),
        system.to_string(),
        "done_moving".into(),
        format!("commit|0|{}", planets[0]),
        "build|carrier|1".into(),
        format!("exhaust|{}", planets[0]),
        "build|infantry|2".into(),
        format!("place|{}", planets[1]),
        "done_producing".into(),
    ];
    let mut runner = runner(&live);
    let transcript = collect(&runner, &script);
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    assert_no_events(&transcript);
    let mut normal = live.fork();
    normal.prepare_hypothetical_turn(&player).unwrap();
    normal.table = Table::with_default(Box::new(Scripted::new(script.clone())));
    for _ in 0..32 {
        let result = normal.step();
        assert!(result.error.is_none(), "{:?}", result.error);
        if normal
            .events
            .iter()
            .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        {
            break;
        }
    }
    let PlanningUpdate::Stopped {
        last_safe_publication: Some(publication),
        ..
    } = &transcript.last().unwrap().update
    else {
        panic!("complete position")
    };
    assert_eq!(
        publication.position,
        project_game_view(&normal.state, &ViewerRole::Player(player))
    );
    assert_eq!(runner.plan().recorded_decisions.len(), script.len());
    live.state.activation_seq += 5;
    runner.refresh(&live, 11);
    let replay = collect(&runner, &[]);
    assert_eq!(reason(&replay), StopReason::MovementComplete);
    assert_eq!(replay.last().unwrap().progress.remaining, 0);
    let restored_plan =
        serde_json::from_str(&serde_json::to_string(&runner.plan()).unwrap()).unwrap();
    let restored = PlanningRunner::start(&live, PlayerId::new("b"), restored_plan, 32);
    assert_eq!(
        reason(&collect(&restored, &[])),
        StopReason::MovementComplete
    );
    live.state.activation_seq -= 5;
    assert_eq!(live.state, original);
}

#[test]
fn repairing_a_retained_landing_resumes_payment_and_placement_after_restoration() {
    let (mut live, system, planets) = planetary_checkpoint(true);
    let player = PlayerId::new("b");
    let script = vec![
        "tactical".into(),
        system.to_string(),
        "done_moving".into(),
        format!("commit|0|{}", planets[0]),
        "build|infantry|2".into(),
        "trade_good".into(),
        format!("place|{}", planets[1]),
        "done_producing".into(),
    ];
    let initial = runner(&live);
    collect(&initial, &script);
    let mut plan = initial.plan();
    plan.revalidation_start = Some(3);
    let tail = plan.recorded_decisions[4..].to_vec();
    let ground = live
        .state
        .system_mut(&system)
        .units
        .iter_mut()
        .find(|unit| unit.type_id.as_str() == "infantry")
        .unwrap();
    ground.type_id = ti4_model::id::UnitTypeId::new("mech");
    let restored = PlanningRunner::start(&live, player, plan, 32);
    loop {
        let blocked = next(&restored);
        if blocked.awaiting_answer {
            assert_eq!(blocked.progress.remaining, 5);
            assert_eq!(&blocked.recorded_decisions[4..], tail.as_slice());
            restored.submit(blocked.identity, &script[3]).unwrap();
            break;
        }
        assert!(!matches!(
            blocked.update,
            PlanningUpdate::Stopped { .. } | PlanningUpdate::Failed(_)
        ));
    }
    let resumed = collect(&restored, &[]);
    assert_eq!(reason(&resumed), StopReason::MovementComplete);
    assert_eq!(resumed.last().unwrap().progress.remaining, 0);
    let repaired = restored.plan();
    assert_eq!(repaired.recorded_decisions[3].payload["unit"], "mech");
    assert_eq!(&repaired.recorded_decisions[4..], tail.as_slice());
}

#[test]
fn revalidation_recomputes_costs_and_inserts_extra_payment_without_losing_placement() {
    let (mut live, system, planets) = planetary_checkpoint(true);
    let player = PlayerId::new("b");
    live.state
        .player_mut(&player)
        .unwrap()
        .technologies
        .insert(TechnologyId::new("st"));
    let script = vec![
        "tactical".into(),
        system.to_string(),
        "done_moving".into(),
        format!("commit|0|{}", planets[0]),
        "build|carrier|1".into(),
        format!("exhaust|{}", planets[0]),
        "build|infantry|2".into(),
        format!("place|{}", planets[1]),
        "done_producing".into(),
    ];
    let initial = runner(&live);
    collect(&initial, &script);
    let mut plan = initial.plan();
    plan.revalidation_start = Some(3);
    live.state
        .player_mut(&player)
        .unwrap()
        .technologies
        .remove(&TechnologyId::new("st"));
    let refreshed = PlanningRunner::start(&live, player, plan, 32);
    let transcript = collect(&refreshed, &["trade_good".into()]);
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    let repaired = refreshed.plan().recorded_decisions;
    assert_eq!(repaired.len(), script.len() + 1);
    assert_eq!(repaired[4].payload["cost"], 3);
    assert_eq!(repaired[6].payload["cost"], 1);
    assert_eq!(repaired[7].option_id, "trade_good");
    assert_eq!(repaired[8].option_id, script[7]);
    assert_eq!(repaired[9].option_id, "done_producing");
}

#[test]
fn ai_development_discount_is_recorded_before_normal_production() {
    let (mut live, system, _) = planetary_checkpoint(true);
    let player = PlayerId::new("b");
    live.state
        .player_mut(&player)
        .unwrap()
        .technologies
        .extend([TechnologyId::new("aida"), TechnologyId::new("cr2")]);
    let runner = runner(&live);
    let transcript = collect(
        &runner,
        &[
            "tactical".into(),
            system.to_string(),
            "done_moving".into(),
            "done_committing".into(),
            "exhaust".into(),
            "build|destroyer|1".into(),
            "done_producing".into(),
        ],
    );
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    assert!(runner.plan().recorded_decisions.iter().any(|answer| {
        answer.context.as_ref().unwrap().subtype == "exhaust_for_production_discount"
    }));
    let PlanningUpdate::Stopped {
        last_safe_publication: Some(publication),
        ..
    } = &transcript.last().unwrap().update
    else {
        panic!("complete")
    };
    let player = publication
        .position
        .players
        .iter()
        .find(|seat| seat.id == player)
        .unwrap();
    assert!(
        player
            .exhausted_technologies
            .contains(&TechnologyId::new("aida"))
    );
    assert_eq!(player.trade_goods, 5);
}

#[test]
fn taking_an_undefended_controlled_planet_does_not_explore_and_can_reach_production() {
    let (mut live, system, planets) = planetary_checkpoint(true);
    live.state
        .system_mut(&system)
        .set_control(planets[0].clone(), PlayerId::new("a"));
    let runner = runner(&live);
    let transcript = collect(
        &runner,
        &[
            "tactical".into(),
            system.to_string(),
            "done_moving".into(),
            format!("commit|0|{}", planets[0]),
            "build|fighter|2".into(),
            "trade_good".into(),
            "done_producing".into(),
        ],
    );
    assert_eq!(reason(&transcript), StopReason::MovementComplete);
    let PlanningUpdate::Stopped {
        last_safe_publication: Some(publication),
        ..
    } = &transcript.last().unwrap().update
    else {
        panic!("complete")
    };
    assert_eq!(
        publication.position.board.systems[&system].planets[&planets[0]].controlled_by,
        Some(PlayerId::new("b"))
    );
    assert_no_events(&transcript);
}

#[test]
fn uncontested_home_planet_capture_transcripts_do_not_depend_on_defenders_secret() {
    let mut transcripts = Vec::new();
    let mut public_states = Vec::new();
    for secret in ["bam", "eap"] {
        let (mut live, system, planets) = planetary_checkpoint(false);
        let defender = PlayerId::new("a");
        let seat = live.state.player_mut(&defender).unwrap();
        seat.home_system = Some(system.clone());
        seat.secret_objectives = vec![SecretObjectiveId::new(secret)];
        live.state
            .system_mut(&system)
            .set_control(planets[0].clone(), defender);
        public_states.push(project_game_view(
            &live.state,
            &ViewerRole::Player(PlayerId::new("b")),
        ));
        let original = live.state.clone();
        let planner = runner(&live);
        let transcript = collect(
            &planner,
            &[
                "tactical".into(),
                system.to_string(),
                "done_moving".into(),
                format!("commit|0|{}", planets[0]),
            ],
        );
        assert_eq!(reason(&transcript), StopReason::Uncertainty);
        assert_eq!(planner.plan().recorded_decisions.len(), 4);
        assert_no_events(&transcript);
        assert_eq!(live.state, original);
        transcripts.push(serde_json::to_value(transcript).unwrap());
    }
    assert_eq!(public_states[0], public_states[1]);
    assert_eq!(transcripts[0], transcripts[1]);
}

#[test]
fn ground_combat_stops_after_recording_the_landing_without_publishing_dice() {
    let (mut live, system, planets) = planetary_checkpoint(false);
    let defender = PlayerId::new("a");
    live.state
        .system_mut(&system)
        .set_control(planets[0].clone(), defender.clone());
    put_on_planet(
        &mut live.state,
        &system,
        &planets[0],
        "infantry",
        &defender,
        1,
    );
    let runner = runner(&live);
    let transcript = collect(
        &runner,
        &[
            "tactical".into(),
            system.to_string(),
            "done_moving".into(),
            format!("commit|0|{}", planets[0]),
        ],
    );
    assert_eq!(reason(&transcript), StopReason::Uncertainty);
    assert_eq!(runner.plan().recorded_decisions.len(), 4);
    assert_no_events(&transcript);
    let serialized = serde_json::to_string(&transcript).unwrap();
    assert!(!serialized.contains("GROUND_ROLLS_MADE"));
    assert!(!serialized.contains("assign_ground"));
}

#[test]
fn bombardment_stops_before_offering_landings() {
    let (mut live, system, planets) = planetary_checkpoint(false);
    let defender = PlayerId::new("a");
    live.state
        .system_mut(&system)
        .set_control(planets[0].clone(), defender.clone());
    put_on_planet(
        &mut live.state,
        &system,
        &planets[0],
        "infantry",
        &defender,
        1,
    );
    put(
        &mut live.state,
        &system,
        "dreadnought",
        &PlayerId::new("b"),
        1,
    );
    let runner = runner(&live);
    let transcript = collect(
        &runner,
        &["tactical".into(), system.to_string(), "done_moving".into()],
    );
    assert_eq!(reason(&transcript), StopReason::Uncertainty);
    assert_eq!(runner.plan().recorded_decisions.len(), 3);
    assert_no_events(&transcript);
    assert!(
        !serde_json::to_string(&transcript)
            .unwrap()
            .contains("commit_ground_forces")
    );
}

#[test]
fn automatic_step_bound_and_edit_have_explicit_generation_boundaries() {
    let (live, _, _) = checkpoint(false, false);
    let mut runner = PlanningRunner::start(&live, PlayerId::new("b"), PlayerPlan::new(10), 1);
    let transcript = collect(&runner, &["tactical".into()]);
    assert_eq!(reason(&transcript), StopReason::StepLimit);
    assert_eq!(runner.plan().recorded_decisions.len(), 1);
    let old = transcript[0].identity;
    runner.edit(&live, 11, vec![]);
    assert!(matches!(next(&runner).update, PlanningUpdate::Preparing));
    let offer = next(&runner);
    assert!(matches!(offer.update, PlanningUpdate::SafeOffer(_)));
    assert_eq!(
        runner.submit(old, "tactical"),
        Err(SubmissionError::Retired)
    );
    assert_eq!(
        runner.submit(offer.identity, "pass"),
        Err(SubmissionError::UnknownOption)
    );
    assert!(runner.plan().recorded_decisions.is_empty());
}

/// Use a standing-slot identity so this test reaches the normal condition
/// boundary. The condition simulates activity inside a temporary engine helper.
fn observed_condition(
    live: &mut Game<'static>,
    condition: ti4_engine::timing::StatefulAbilityCondition,
) {
    let player = PlayerId::new("b");
    let owner = ti4_engine::promissory::faction_name(&live.state, &player);
    live.timing.register([Ability::stateful(
        format!("reaction:{owner}:TURN_BEGAN:when"),
        player,
        "TURN_BEGAN",
        Relation::When,
        Arc::new(|_, _, _| panic!("condition must not select an effect")),
    )
    .with_optional(true)
    .with_stateful_condition(condition)]);
}

#[test]
fn temporary_rng_and_direct_stream_activity_are_stopped_before_the_first_offer() {
    for direct in [false, true] {
        let (mut live, _, _) = checkpoint(false, false);
        observed_condition(
            &mut live,
            Arc::new(move |_, _, context| {
                let mut temporary = context.rng.clone();
                if direct {
                    let _ = temporary.stream("direct");
                } else {
                    let _ = temporary.die("temporary", 10);
                }
                false
            }),
        );
        let transcript = collect(&runner(&live), &[]);
        assert_eq!(reason(&transcript), StopReason::Uncertainty);
        assert_eq!(transcript.len(), 1);
    }
}

#[test]
fn zero_dice_does_not_stop_a_draft_but_positive_dice_does() {
    for count in [0, 1] {
        let (mut live, destination, origin) = checkpoint(false, false);
        observed_condition(
            &mut live,
            Arc::new(move |_, _, context| {
                let mut dice = context.dice.clone();
                let mut rng = context.rng.clone();
                dice.roll(&mut rng, count, "test boundary", None);
                false
            }),
        );
        let input = if count == 0 {
            answers(&destination, &origin, false)
        } else {
            vec![]
        };
        let transcript = collect(&runner(&live), &input);
        assert_eq!(
            reason(&transcript),
            if count == 0 {
                StopReason::MovementComplete
            } else {
                StopReason::Uncertainty
            }
        );
    }
}

#[test]
fn engine_failure_is_sanitized_and_uncertainty_takes_precedence_over_failure() {
    for uncertain in [false, true] {
        let (mut live, _, _) = checkpoint(false, false);
        observed_condition(
            &mut live,
            Arc::new(move |_, _, context| {
                if uncertain {
                    let mut temporary = context.rng.clone();
                    let _ = temporary.die("temporary", 10);
                }
                panic!("private card identity and speculative diagnostic");
            }),
        );
        let transcript = collect(&runner(&live), &[]);
        if uncertain {
            assert_eq!(reason(&transcript), StopReason::Uncertainty);
        } else {
            assert_eq!(
                transcript[0].update,
                PlanningUpdate::Failed(FailureCategory::Worker)
            );
        }
        assert_eq!(transcript.len(), 1);
        assert!(
            !serde_json::to_string(&transcript)
                .unwrap()
                .contains("private card identity")
        );
    }
}
