use ti4_content::ContentStore;
use ti4_engine::choice::{AlwaysDecline, Choice, ChoiceOption, Decider, Scripted, Table};
use ti4_engine::decision_context::{DecisionContext, DecisionSource};
use ti4_engine::fixtures::{game, plain_hub, put};
use ti4_engine::game::Game;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::Phase;
use ti4_server::planning::{RecordedDecision, RecordingDecider, ReplayDecider, ReplayStopReason};

fn checkpoint() -> (Game<'static>, SystemId, SystemId) {
    let hub = plain_hub();
    let destination = SystemId::new(&hub.centre);
    let origin = SystemId::new(&hub.outer[0]);
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    put(&mut state, &origin, "destroyer", &PlayerId::new("b"), 1);
    (
        Game::new(state, ContentStore::embedded()).with_galaxy(hub.galaxy),
        destination,
        origin,
    )
}

fn prepared(checkpoint: &Game<'static>) -> Game<'static> {
    let mut fork = checkpoint.fork();
    fork.prepare_hypothetical_turn(&PlayerId::new("b")).unwrap();
    fork
}

fn finish_tactical(game: &mut Game<'_>) {
    for _ in 0..8 {
        assert_eq!(game.step().error, None);
        if game
            .events
            .iter()
            .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        {
            return;
        }
    }
    panic!("tactical action did not complete");
}

fn record_move(
    checkpoint: &Game<'static>,
    destination: &SystemId,
    origin: &SystemId,
) -> (Game<'static>, Vec<RecordedDecision>) {
    let mut fork = prepared(checkpoint);
    let (recorder, recording) = RecordingDecider::new(Box::new(Scripted::new([
        "tactical".to_owned(),
        destination.to_string(),
        format!("move|{origin}|0"),
        "done_moving".to_owned(),
    ])));
    fork.table = Table::with_default(Box::new(recorder));
    finish_tactical(&mut fork);
    let decisions = recording.lock().unwrap().clone();
    assert_eq!(decisions.len(), 4);
    (fork, decisions)
}

#[test]
fn a_recorded_move_survives_disposal_serialization_and_reconstruction() {
    let (checkpoint, destination, origin) = checkpoint();
    let original_state = serde_json::to_value(&checkpoint.state).unwrap();
    let original_choice = checkpoint.legal_options();
    let original_events = checkpoint.events.clone();
    let original_log = checkpoint.table.log.clone();

    let (draft, decisions) = record_move(&checkpoint, &destination, &origin);
    let expected_state = serde_json::to_value(&draft.state).unwrap();
    let expected_events = draft.events.clone();
    let expected_log = draft.table.log.clone();
    let expected_choice = draft.legal_options();
    assert!(draft.state.system_state(&origin).units.is_empty());
    assert_eq!(
        draft.state.system_state(&destination).units[0].owner,
        PlayerId::new("b")
    );
    assert!(draft.rolls().is_empty());
    drop(draft);

    let bytes = serde_json::to_vec(&decisions).unwrap();
    let decisions = serde_json::from_slice(&bytes).unwrap();
    let mut rebuilt = prepared(&checkpoint);
    let (replay, progress) = ReplayDecider::new(decisions);
    rebuilt.table = Table::with_default(Box::new(replay));
    finish_tactical(&mut rebuilt);
    assert_eq!(
        serde_json::to_value(&rebuilt.state).unwrap(),
        expected_state
    );
    assert_eq!(rebuilt.events, expected_events);
    assert_eq!(rebuilt.table.log, expected_log);
    assert_eq!(rebuilt.legal_options(), expected_choice);
    assert!(rebuilt.rolls().is_empty());
    {
        let progress = progress.lock().unwrap();
        assert_eq!(progress.consumed(), 4);
        assert!(progress.remaining().is_empty());
        assert_eq!(progress.stop_reason(), None);
    }

    // The next question requires new input; exhaustion must not select a default.
    let decisions_before = rebuilt.table.log.len();
    let _ = rebuilt.step();
    assert_eq!(
        progress.lock().unwrap().stop_reason(),
        Some(ReplayStopReason::Exhausted)
    );
    assert_eq!(rebuilt.table.log.len(), decisions_before);
    drop(rebuilt); // An interrupted attempt is disposable, not a new checkpoint.

    assert_eq!(
        serde_json::to_value(&checkpoint.state).unwrap(),
        original_state
    );
    assert_eq!(checkpoint.legal_options(), original_choice);
    assert_eq!(checkpoint.events, original_events);
    assert_eq!(checkpoint.table.log, original_log);
    assert!(checkpoint.rolls().is_empty());
}

#[test]
fn replay_survives_an_unrelated_change_but_stops_if_the_ship_is_missing() {
    let (checkpoint, destination, origin) = checkpoint();
    let (_, decisions) = record_move(&checkpoint, &destination, &origin);

    let mut changed = checkpoint.fork();
    changed
        .state
        .player_mut(&PlayerId::new("a"))
        .unwrap()
        .trade_goods += 1;
    let mut rebuilt = prepared(&changed);
    let (replay, progress) = ReplayDecider::new(decisions.clone());
    rebuilt.table = Table::with_default(Box::new(replay));
    finish_tactical(&mut rebuilt);
    assert_eq!(progress.lock().unwrap().consumed(), 4);
    assert_eq!(progress.lock().unwrap().stop_reason(), None);

    changed.state.system_mut(&origin).units.clear();
    let mut invalidated = prepared(&changed);
    let (replay, progress) = ReplayDecider::new(decisions.clone());
    invalidated.table = Table::with_default(Box::new(replay));
    for _ in 0..8 {
        let _ = invalidated.step();
        if progress.lock().unwrap().stop_reason().is_some() {
            break;
        }
    }
    {
        let progress = progress.lock().unwrap();
        assert_eq!(
            progress.stop_reason(),
            Some(ReplayStopReason::SelectionMissing)
        );
        assert_eq!(progress.consumed(), 2);
        assert_eq!(
            progress.remaining().iter().cloned().collect::<Vec<_>>(),
            decisions[2..]
        );
    }
    assert_eq!(invalidated.table.log.len(), 2);
    drop(invalidated);
}

fn record_answer(choice: &Choice) -> RecordedDecision {
    let (mut recorder, recording) = RecordingDecider::new(Box::new(Scripted::new(["pick"])));
    recorder.choose(choice).unwrap();
    let decision = recording.lock().unwrap()[0].clone();
    decision
}

#[test]
fn replay_checks_payloads_actor_and_prompt_and_latches_the_first_stop() {
    let choice = Choice::new(
        PlayerId::new("a"),
        "select a unit",
        vec![ChoiceOption::new("pick", "unit").with("index", 0)],
    );
    let decision = record_answer(&choice);
    for (changed, reason) in [
        (
            Choice {
                player: PlayerId::new("b"),
                ..choice.clone()
            },
            ReplayStopReason::QuestionMismatch,
        ),
        (
            Choice {
                prompt: "unrelated question".into(),
                ..choice.clone()
            },
            ReplayStopReason::QuestionMismatch,
        ),
        (
            Choice {
                options: vec![ChoiceOption::new("pick", "unit").with("index", 1)],
                ..choice.clone()
            },
            ReplayStopReason::SelectionMissing,
        ),
        (
            Choice {
                options: vec![choice.options[0].clone(), choice.options[0].clone()],
                ..choice.clone()
            },
            ReplayStopReason::SelectionAmbiguous,
        ),
    ] {
        let (mut replay, progress) = ReplayDecider::new(vec![decision.clone()]);
        assert!(replay.choose(&changed).is_err());
        // A later matching offer cannot restart a stopped replay, even when an
        // engine path catches the first error and goes on to ask another question.
        assert!(replay.choose(&choice).is_err());
        let progress = progress.lock().unwrap();
        assert_eq!(progress.stop_reason(), Some(reason));
        assert_eq!(progress.consumed(), 0);
        assert_eq!(progress.remaining().front(), Some(&decision));
    }
}

#[test]
fn typed_context_must_match_and_success_returns_the_fresh_option() {
    let context = DecisionContext::new(
        PlayerId::new("a"),
        DecisionSource::Rule("test".into()),
        "select_unit",
        Phase::Action,
        1,
    );
    let choice = Choice::new(
        PlayerId::new("a"),
        "select a unit",
        vec![ChoiceOption::labelled("pick", "unit", "old label").with("index", 0)],
    )
    .contextualized(context);
    let decision = record_answer(&choice);
    let mut fresh = choice.clone();
    fresh.prompt = "updated wording".into();
    fresh.options[0].label = "fresh label".into();
    let (mut replay, _) = ReplayDecider::new(vec![decision.clone()]);
    assert_eq!(replay.choose(&fresh).unwrap().label, "fresh label");

    for context in [
        None,
        Some(DecisionContext {
            subtype: "different_question".into(),
            ..choice.context.clone().unwrap()
        }),
    ] {
        let changed = Choice {
            context,
            ..choice.clone()
        };
        let (mut replay, progress) = ReplayDecider::new(vec![decision.clone()]);
        assert!(replay.choose(&changed).is_err());
        assert_eq!(
            progress.lock().unwrap().stop_reason(),
            Some(ReplayStopReason::QuestionMismatch)
        );
    }
}

#[test]
fn replay_handles_nested_choices_and_reports_exhaustion_inside_a_step() {
    let mut checkpoint = Game::with_table(
        game(&["a", "b", "c"]),
        ContentStore::embedded(),
        Table::with_default(Box::new(AlwaysDecline)),
    );
    while checkpoint.state.phase == Phase::Strategy {
        assert_eq!(checkpoint.step().error, None);
    }
    let follower = PlayerId::new("b");
    checkpoint.state.player_mut(&follower).unwrap().trade_goods = 3;
    assert_eq!(checkpoint.step().error, None); // Leadership primary.
    assert_eq!(checkpoint.legal_options().unwrap().player, follower);
    let original_state = serde_json::to_value(&checkpoint.state).unwrap();

    let mut draft = checkpoint.fork();
    let (recorder, recording) =
        RecordingDecider::new(Box::new(Scripted::new(["yes", "fleet_tokens"])));
    draft.table = Table::with_default(Box::new(recorder));
    assert_eq!(draft.step().error, None);
    let decisions = recording.lock().unwrap().clone();
    assert_eq!(
        decisions.len(),
        2,
        "the purchase and pool selection happen in one step"
    );

    let mut rebuilt = checkpoint.fork();
    let (replay, progress) = ReplayDecider::new(decisions.clone());
    rebuilt.table = Table::with_default(Box::new(replay));
    assert_eq!(rebuilt.step().error, None);
    assert_eq!(progress.lock().unwrap().consumed(), 2);
    assert_eq!(progress.lock().unwrap().stop_reason(), None);
    assert_eq!(
        serde_json::to_value(&rebuilt.state).unwrap(),
        serde_json::to_value(&draft.state).unwrap()
    );
    assert_eq!(rebuilt.table.log, draft.table.log);

    let mut interrupted = checkpoint.fork();
    let (replay, progress) = ReplayDecider::new(decisions[..1].to_vec());
    interrupted.table = Table::with_default(Box::new(replay));
    let _ = interrupted.step(); // The progress latch, not StepResult, is authoritative.
    assert_eq!(
        progress.lock().unwrap().stop_reason(),
        Some(ReplayStopReason::Exhausted)
    );
    assert_eq!(progress.lock().unwrap().consumed(), 1);
    assert_eq!(interrupted.table.log.len(), 1);
    drop(interrupted);
    assert_eq!(
        serde_json::to_value(&checkpoint.state).unwrap(),
        original_state
    );
}
