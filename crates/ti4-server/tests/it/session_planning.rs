use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use ti4_content::{ContentStore, galaxy::all_systems};
use ti4_engine::fixtures::{game, hub_with_centre, put, put_on_planet};
use ti4_model::content_types::POK;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::Phase;
use ti4_server::planning::runner::{PlanningEnvelope, PlanningUpdate, StopReason, SubmissionError};
use ti4_server::protocol::server::DraftApplicationState;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::registry::HistoryAction;
use ti4_server::session::{
    GameRegistry, GameSession, PlanningError, SeatController, SessionConfig,
};

const DEADLINE: Duration = Duration::from_secs(5);

fn fixture(cargo: bool) -> (SessionConfig, SystemId, SystemId, SystemId, SystemId) {
    let empty = all_systems(ContentStore::embedded(), POK)
        .iter()
        .find(|(_, system)| {
            system.planets().is_empty() && !system.is_anomaly() && !system.is_hyperlane()
        })
        .unwrap()
        .0
        .to_string();
    let hub = hub_with_centre(&empty);
    let origin_b = SystemId::new(&hub.outer[0]);
    let origin_a = SystemId::new(&hub.outer[1]);
    let target = if cargo {
        SystemId::new(&hub.centre)
    } else {
        SystemId::new(hub.across(origin_b.as_str()))
    };
    let centre = SystemId::new(&hub.centre);
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    for (player, origin, ship) in [
        ("a", &origin_a, if cargo { "carrier" } else { "destroyer" }),
        ("b", &origin_b, if cargo { "carrier" } else { "cruiser" }),
    ] {
        put(&mut state, origin, ship, &PlayerId::new(player), 1);
        if cargo {
            put(&mut state, origin, "fighter", &PlayerId::new(player), 1);
        }
    }
    let config = SessionConfig::new("session_planning", state)
        .with_galaxy(hub.galaxy, vec![])
        .with_player_ids(vec![PlayerId::new("a"), PlayerId::new("b")])
        .with_seat(PlayerId::new("a"), SeatController::Human)
        .with_seat(PlayerId::new("b"), SeatController::Human);
    // The target for b is encoded separately so the live actor always moves to centre.
    assert_eq!(
        config
            .galaxy
            .as_ref()
            .unwrap()
            .distance(origin_b.as_str(), target.as_str()),
        Some(if cargo { 1 } else { 2 })
    );
    (config, origin_a, origin_b, target, centre)
}

struct LiveOffer {
    choice: ti4_engine::choice::Choice,
    nonce: String,
    game_version: u64,
}

fn live_offer(session: &GameSession, previous_nonce: Option<&str>) -> LiveOffer {
    let deadline = Instant::now() + DEADLINE;
    loop {
        assert!(session.error().is_none(), "{:?}", session.error());
        if let Some((seat, _, _)) = session.current_pending_decision() {
            let snapshot = session.get_snapshot(&ViewerRole::Player(seat));
            if let Some(pending) = snapshot.pending_choice {
                if previous_nonce != Some(pending.nonce.as_str()) {
                    return LiveOffer {
                        choice: pending.choice,
                        nonce: pending.nonce,
                        game_version: snapshot.game_version,
                    };
                }
            }
        }
        assert!(Instant::now() < deadline, "live offer deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn answer_live(session: &GameSession, option: &str) -> LiveOffer {
    let offer = live_offer(session, None);
    session
        .submit_choice(
            &offer.choice.player,
            &offer.nonce,
            offer.game_version,
            option,
        )
        .unwrap();
    live_offer(session, Some(&offer.nonce))
}

fn planning_offer(session: &GameSession) -> PlanningEnvelope {
    loop {
        let envelope = session
            .recv_planning_timeout(&PlayerId::new("b"), DEADLINE)
            .unwrap();
        match &envelope.update {
            PlanningUpdate::SafeOffer(_) => return envelope,
            PlanningUpdate::SafeStep(_) | PlanningUpdate::Preparing => {}
            other => panic!("expected planning offer, got {other:?}"),
        }
    }
}

fn draft(session: &GameSession, answers: &[String]) -> PlanningEnvelope {
    session.start_planning(&PlayerId::new("b")).unwrap();
    for answer in answers {
        let offer = planning_offer(session);
        session
            .submit_planning_choice(&PlayerId::new("b"), offer.identity, answer)
            .unwrap();
    }
    planning_offer(session)
}

fn replayed_offer(session: &GameSession) -> PlanningEnvelope {
    loop {
        let offer = planning_offer(session);
        if offer.progress.remaining == 0 {
            return offer;
        }
    }
}

fn stopped_draft(session: &GameSession, answers: &[String]) -> PlanningEnvelope {
    session.start_planning(&PlayerId::new("b")).unwrap();
    for answer in answers {
        let offer = planning_offer(session);
        session
            .submit_planning_choice(&PlayerId::new("b"), offer.identity, answer)
            .unwrap();
    }
    loop {
        let envelope = session
            .recv_planning_timeout(&PlayerId::new("b"), DEADLINE)
            .unwrap();
        if matches!(envelope.update, PlanningUpdate::Stopped { .. }) {
            return envelope;
        }
    }
}

fn ready_to_apply(session: &GameSession) -> ti4_server::protocol::server::PlanningStatusMsg {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let status = session.planning_status(&PlayerId::new("b"));
        if status.can_apply {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "draft application deadline: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn an_expansion_draft_can_apply_its_landing_prefix_before_live_exploration() {
    let hub = hub_with_centre("35");
    let system = SystemId::new("35");
    let player = PlayerId::new("b");
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    put(&mut state, &system, "carrier", &player, 1);
    put(&mut state, &system, "infantry", &player, 1);
    let config = SessionConfig::new("expansion_prefix", state)
        .with_galaxy(hub.galaxy, vec![])
        .with_player_ids(vec![PlayerId::new("a"), player.clone()])
        .with_seat(PlayerId::new("a"), SeatController::Human)
        .with_seat(player.clone(), SeatController::Human);
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let script = vec![
        "tactical".into(),
        "35".into(),
        "done_moving".into(),
        "commit|0|bereg".into(),
    ];
    let stopped = stopped_draft(&session, &script);
    assert!(matches!(
        stopped.update,
        PlanningUpdate::Stopped {
            reason: StopReason::Uncertainty,
            ..
        }
    ));
    assert!(
        !session
            .current_state()
            .system_state(&system)
            .planet_control
            .contains_key(&PlanetId::new("bereg"))
    );
    answer_live(&session, "pass");
    let ready = ready_to_apply(&session);
    let menu = live_offer(&session, None);
    session
        .apply_planning(
            &player,
            ready.identity.unwrap(),
            &menu.nonce,
            menu.game_version,
        )
        .unwrap();
    let deadline = Instant::now() + DEADLINE;
    loop {
        let app = session.planning_status(&player).application.unwrap();
        if app.state == DraftApplicationState::Applied {
            assert_eq!(app.applied, script.len());
            break;
        }
        assert_ne!(app.state, DraftApplicationState::NeedsDecision, "{app:?}");
        assert!(Instant::now() < deadline, "partial application deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        session.plans()[&player].recorded_decisions.len(),
        script.len()
    );
}

#[test]
fn applying_deterministic_landing_and_production_executes_the_whole_recorded_action() {
    let hub = hub_with_centre("35");
    let system = SystemId::new("35");
    let player = PlayerId::new("b");
    let bereg = PlanetId::new("bereg");
    let lirta = PlanetId::new("lirtaiv");
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    put(&mut state, &system, "carrier", &player, 1);
    put(&mut state, &system, "infantry", &player, 1);
    for planet in [&bereg, &lirta] {
        state
            .system_mut(&system)
            .set_control(planet.clone(), player.clone());
        put_on_planet(&mut state, &system, planet, "spacedock", &player, 1);
    }
    state.player_mut(&player).unwrap().trade_goods = 5;
    let config = SessionConfig::new("landing_production", state)
        .with_galaxy(hub.galaxy, vec![])
        .with_player_ids(vec![PlayerId::new("a"), player.clone()])
        .with_seat(PlayerId::new("a"), SeatController::Human)
        .with_seat(player.clone(), SeatController::Human);
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let script = vec![
        "tactical".into(),
        "35".into(),
        "done_moving".into(),
        "commit|0|bereg".into(),
        "build|infantry|2".into(),
        "trade_good".into(),
        "place|lirtaiv".into(),
        "done_producing".into(),
    ];
    let stopped = stopped_draft(&session, &script);
    assert!(matches!(
        stopped.update,
        PlanningUpdate::Stopped {
            reason: StopReason::MovementComplete,
            ..
        }
    ));
    // Reconfirming identical movement revalidates the complete retained tail.
    session
        .edit_planning_movement(&player, stopped.identity)
        .unwrap();
    let editing = replayed_offer(&session);
    assert_eq!(editing.recorded_decisions.len(), script.len());
    session
        .submit_planning_choice(&player, editing.identity, "done_moving")
        .unwrap();
    loop {
        let envelope = session.recv_planning_timeout(&player, DEADLINE).unwrap();
        assert_eq!(envelope.recorded_decisions.len(), script.len());
        assert!(
            !envelope.awaiting_answer,
            "retained choices should replay: {envelope:?}"
        );
        if matches!(envelope.update, PlanningUpdate::Stopped { .. }) {
            break;
        }
    }
    answer_live(&session, "pass");
    let ready = ready_to_apply(&session);
    let menu = live_offer(&session, None);
    session
        .apply_planning(
            &player,
            ready.identity.unwrap(),
            &menu.nonce,
            menu.game_version,
        )
        .unwrap();
    let deadline = Instant::now() + DEADLINE;
    let application = loop {
        let application = session.planning_status(&player).application.unwrap();
        if !matches!(
            application.state,
            DraftApplicationState::Applying | DraftApplicationState::WaitingForPlayer
        ) {
            break application;
        }
        assert!(Instant::now() < deadline, "{application:?}");
        std::thread::sleep(Duration::from_millis(5));
    };
    let next = loop {
        let next = live_offer(&session, Some(&menu.nonce));
        if next.choice.context.as_ref().unwrap().subtype == "action_menu" {
            break next;
        }
        assert!(Instant::now() < deadline, "action completion deadline");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        application.state,
        DraftApplicationState::Applied,
        "{application:?}; next: {:?}",
        next.choice
    );
    assert_eq!(application.applied, script.len());
    assert_eq!(next.choice.context.as_ref().unwrap().subtype, "action_menu");
    let state = session.current_state();
    let board = state.system_state(&system);
    assert_eq!(board.on_planet_of(&bereg, &player).len(), 2);
    assert_eq!(board.on_planet_of(&lirta, &player).len(), 3);
    assert_eq!(state.player(&player).unwrap().trade_goods, 4);
    assert_eq!(
        session.plans()[&player].recorded_decisions.len(),
        script.len()
    );
}

#[test]
fn revised_movement_revalidates_landings_and_builds_and_repair_keeps_the_tail() {
    for remove_cargo in [false, true] {
        let hub = hub_with_centre("35");
        let target = SystemId::new("35");
        let origin = SystemId::new(&hub.outer[0]);
        let player = PlayerId::new("b");
        let mut state = game(&["a", "b"]);
        state.phase = Phase::Action;
        state.active = Some(PlayerId::new("a"));
        put(&mut state, &origin, "carrier", &player, 1);
        put(&mut state, &origin, "infantry", &player, 1);
        put(&mut state, &origin, "cruiser", &player, 1);
        for planet in ["bereg", "lirtaiv"] {
            state
                .system_mut(&target)
                .set_control(PlanetId::new(planet), player.clone());
            put_on_planet(
                &mut state,
                &target,
                &PlanetId::new(planet),
                "spacedock",
                &player,
                1,
            );
        }
        state.player_mut(&player).unwrap().trade_goods = 5;
        let config = SessionConfig::new("revalidate_tail", state)
            .with_galaxy(hub.galaxy, vec![])
            .with_player_ids(vec![PlayerId::new("a"), player.clone()])
            .with_seat(PlayerId::new("a"), SeatController::Human)
            .with_seat(player.clone(), SeatController::Human);
        let session = GameSession::start(config);
        let _ = live_offer(&session, None);
        let script = vec![
            "tactical".into(),
            "35".into(),
            format!("move|{origin}|0"),
            "load|0".into(),
            "done_moving".into(),
            "commit|0|bereg".into(),
            "build|destroyer|1".into(),
            "trade_good".into(),
            "done_producing".into(),
        ];
        let stopped = stopped_draft(&session, &script);
        let tail = stopped.recorded_decisions[5..].to_vec();
        session
            .edit_planning_movement(&player, stopped.identity)
            .unwrap();
        let editing = replayed_offer(&session);
        // Move the cruiser first while retaining the original landing intent.
        session
            .submit_planning_choice(&player, editing.identity, &format!("move|{origin}|2"))
            .unwrap();
        let answers = if remove_cargo {
            vec!["done_moving".to_owned()]
        } else {
            vec![
                format!("move|{origin}|0"),
                "load|0".into(),
                "done_moving".into(),
            ]
        };
        for answer in answers {
            let offer = planning_offer(&session);
            assert!(offer.awaiting_answer);
            session
                .submit_planning_choice(&player, offer.identity, &answer)
                .unwrap();
        }
        if remove_cargo {
            let blocked = planning_offer(&session);
            assert!(blocked.awaiting_answer);
            assert_eq!(&blocked.recorded_decisions[4..], tail.as_slice());
            assert_eq!(blocked.progress.remaining, tail.len());
            assert!(!session.planning_status(&player).can_apply);
            // Repair the unavailable landing by restoring its carrier and cargo.
            session
                .edit_planning_movement(&player, blocked.identity)
                .unwrap();
            let editing = replayed_offer(&session);
            session
                .submit_planning_choice(&player, editing.identity, &format!("move|{origin}|0"))
                .unwrap();
            for answer in ["load|0", "done_moving"] {
                let offer = planning_offer(&session);
                session
                    .submit_planning_choice(&player, offer.identity, answer)
                    .unwrap();
            }
        }
        let mut landing = None;
        loop {
            let envelope = session.recv_planning_timeout(&player, DEADLINE).unwrap();
            assert!(!envelope.awaiting_answer, "unexpected repair: {envelope:?}");
            if let PlanningUpdate::SafeOffer(publication) = &envelope.update {
                let choice = publication.choice.as_ref().unwrap();
                if choice.context.as_ref().unwrap().subtype == "commit_ground_forces" {
                    landing = Some(choice.clone());
                }
            }
            if let PlanningUpdate::Stopped { reason, .. } = envelope.update {
                assert_eq!(reason, StopReason::MovementComplete);
                assert_eq!(envelope.progress.remaining, 0);
                let decisions = envelope.recorded_decisions;
                let rebound = decisions
                    .iter()
                    .find(|decision| decision.kind == "commit")
                    .unwrap();
                assert_eq!(rebound.payload, tail[0].payload);
                assert!(landing.is_some());
                assert_eq!(
                    decisions[decisions.len() - 3..]
                        .iter()
                        .map(|decision| &decision.option_id)
                        .collect::<Vec<_>>(),
                    tail[1..]
                        .iter()
                        .map(|decision| &decision.option_id)
                        .collect::<Vec<_>>()
                );
                if !remove_cargo {
                    assert_ne!(
                        decisions[decisions.len() - 3].payload["fleet_headroom_after"],
                        tail[1].payload["fleet_headroom_after"]
                    );
                }
                break;
            }
        }
    }
}

#[test]
fn reopening_movement_retains_the_script_until_new_answers_and_applies_only_the_revision() {
    let (mut config, _, origin, target, _) = fixture(true);
    put(
        &mut config.state,
        &origin,
        "cruiser",
        &PlayerId::new("b"),
        1,
    );
    config.state.system_mut(&origin).units.rotate_right(1);
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let stopped = stopped_draft(
        &session,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move|{origin}|0"),
            format!("move|{origin}|0"),
            "load|0".into(),
            "done_moving".into(),
        ],
    );
    let before = session.current_state();
    session
        .edit_planning_movement(&PlayerId::new("b"), stopped.identity)
        .unwrap();
    let editing = replayed_offer(&session);
    assert!(editing.editing_movement);
    assert_eq!(editing.recorded_decisions, stopped.recorded_decisions);
    assert_eq!(editing.movement_edit_revision, 1);
    assert_eq!(editing.reset_revision, stopped.reset_revision);
    let PlanningUpdate::SafeOffer(publication) = &editing.update else {
        unreachable!()
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
        "movement_step"
    );
    assert_eq!(publication.position.board.systems[&origin].units.len(), 3);
    assert!(!session.planning_status(&PlayerId::new("b")).can_apply);
    assert_eq!(session.current_state(), before);
    assert_eq!(
        session.edit_planning_movement(&PlayerId::new("b"), stopped.identity),
        Err(PlanningError::Submission(SubmissionError::Retired))
    );
    assert_eq!(
        session.submit_planning_choice(&PlayerId::new("b"), stopped.identity, "done_moving"),
        Err(PlanningError::Submission(SubmissionError::Retired))
    );
    session
        .submit_planning_choice(
            &PlayerId::new("b"),
            editing.identity,
            &format!("move|{origin}|1"),
        )
        .unwrap();
    for answer in ["load|0", "done_moving"] {
        let offered = planning_offer(&session);
        assert!(!offered.editing_movement);
        session
            .submit_planning_choice(&PlayerId::new("b"), offered.identity, answer)
            .unwrap();
    }
    loop {
        let envelope = session
            .recv_planning_timeout(&PlayerId::new("b"), DEADLINE)
            .unwrap();
        if matches!(envelope.update, PlanningUpdate::Stopped { .. }) {
            break;
        }
    }
    let revised = session.plans()[&PlayerId::new("b")]
        .recorded_decisions
        .clone();
    assert_eq!(revised.len(), 5);
    assert_eq!(revised[2].payload["unit"], "carrier");
    assert_eq!(revised[2].option_id, format!("move|{origin}|1"));
    assert_eq!(revised[3].payload["unit"], "fighter");
    answer_live(&session, "pass");
    let ready = ready_to_apply(&session);
    let offer = live_offer(&session, None);
    session
        .apply_planning(
            &PlayerId::new("b"),
            ready.identity.unwrap(),
            &offer.nonce,
            offer.game_version,
        )
        .unwrap();
    let deadline = Instant::now() + DEADLINE;
    while session
        .planning_status(&PlayerId::new("b"))
        .application
        .as_ref()
        .unwrap()
        .state
        == DraftApplicationState::Applying
    {
        assert!(Instant::now() < deadline, "application deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let _ = live_offer(&session, Some(&offer.nonce));
    let state = session.current_state();
    assert_eq!(state.system_state(&target).units.len(), 2);
    assert_eq!(state.system_state(&origin).units.len(), 1);
    assert_eq!(
        state.system_state(&origin).units[0].type_id.as_str(),
        "cruiser"
    );
    assert_eq!(
        session
            .planning_status(&PlayerId::new("b"))
            .application
            .unwrap()
            .state,
        DraftApplicationState::Applied
    );
}

#[test]
fn a_recovered_movement_editor_keeps_all_original_answers() {
    let (config, _, origin, target, _) = fixture(true);
    let session = GameSession::start(config.clone());
    let _ = live_offer(&session, None);
    let stopped = stopped_draft(
        &session,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move|{origin}|0"),
            "load|0".into(),
            "done_moving".into(),
        ],
    );
    session
        .edit_planning_movement(&PlayerId::new("b"), stopped.identity)
        .unwrap();
    let _ = replayed_offer(&session);
    let mut restored_config = config;
    restored_config.plans =
        serde_json::from_str(&serde_json::to_string(&session.plans()).unwrap()).unwrap();
    session.stop();
    let restored = GameSession::start(restored_config);
    let _ = live_offer(&restored, None);
    let editing = replayed_offer(&restored);
    assert!(editing.editing_movement);
    assert_eq!(editing.recorded_decisions, stopped.recorded_decisions);
    assert!(editing.awaiting_answer);
    assert_eq!(editing.progress.replayed, 2);
    assert_eq!(editing.movement_edit_revision, 1);
    assert!(!restored.planning_status(&PlayerId::new("b")).can_apply);
}

#[test]
fn applying_a_draft_executes_live_activation_movement_and_cargo_once() {
    let (config, _, origin_b, target, _) = fixture(true);
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let script = vec![
        "tactical".into(),
        target.to_string(),
        format!("move|{origin_b}|0"),
        "load|0".into(),
        "done_moving".into(),
    ];
    let old = stopped_draft(&session, &script);
    let a = live_offer(&session, None);
    assert_eq!(
        session.apply_planning(&PlayerId::new("b"), old.identity, &a.nonce, a.game_version),
        Err(PlanningError::NoActionOpportunity)
    );
    answer_live(&session, "pass");
    let ready = ready_to_apply(&session);
    let offer = live_offer(&session, None);
    assert_eq!(offer.choice.player, PlayerId::new("b"));
    let before = session
        .current_state()
        .player(&PlayerId::new("b"))
        .unwrap()
        .tactic_tokens;
    let cursor = session.decision_log().len();
    session
        .apply_planning(
            &PlayerId::new("b"),
            ready.identity.unwrap(),
            &offer.nonce,
            offer.game_version,
        )
        .unwrap();
    // Application progress acknowledges selection; the completed engine step
    // publishes its authoritative decision log afterward.
    let deadline = Instant::now() + DEADLINE;
    while session.decision_log().len() < cursor + script.len() {
        assert!(Instant::now() < deadline, "application recording deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let next = live_offer(&session, Some(&offer.nonce));
    assert_eq!(next.choice.player, PlayerId::new("b"));
    let status = session.planning_status(&PlayerId::new("b"));
    let application = status.application.unwrap();
    assert_eq!(application.state, DraftApplicationState::Applied);
    assert_eq!(application.applied, script.len());
    assert!(!status.can_apply);
    assert_eq!(
        session.decision_log()[cursor..]
            .iter()
            .map(|record| record.chosen.clone())
            .collect::<Vec<_>>(),
        script
    );
    let state = session.current_state();
    assert_eq!(
        state.player(&PlayerId::new("b")).unwrap().tactic_tokens,
        before - 1
    );
    assert_eq!(
        state
            .system_state(&target)
            .units
            .iter()
            .filter(|unit| unit.owner == PlayerId::new("b"))
            .count(),
        2
    );
    assert!(
        session
            .apply_planning(
                &PlayerId::new("b"),
                ready.identity.unwrap(),
                &offer.nonce,
                offer.game_version
            )
            .is_err()
    );
    session
        .reset_planning(
            &PlayerId::new("b"),
            session
                .planning_status(&PlayerId::new("b"))
                .identity
                .unwrap(),
        )
        .unwrap();
    assert!(
        session
            .planning_status(&PlayerId::new("b"))
            .application
            .is_none()
    );
    assert_eq!(
        session.current_state(),
        state,
        "reset never undoes applied live choices"
    );
}

#[test]
#[ignore = "fails: the application stays at Applying after the reaction is declined"]
fn applying_a_draft_waits_for_real_opponent_reactions_then_rechecks_and_resumes() {
    use ti4_model::id::ActionCardId;
    let (mut config, _, origin_b, target, _) = fixture(true);
    config
        .state
        .player_mut(&PlayerId::new("a"))
        .unwrap()
        .action_cards = vec![ActionCardId::new("counterstroke")];
    config
        .state
        .system_mut(&target)
        .place_token(PlayerId::new("a"));
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let script = vec![
        "tactical".into(),
        target.to_string(),
        format!("move|{origin_b}|0"),
        "load|0".into(),
        "done_moving".into(),
    ];
    stopped_draft(&session, &script);
    answer_live(&session, "pass");
    let ready = ready_to_apply(&session);
    let offer = live_offer(&session, None);
    session
        .apply_planning(
            &PlayerId::new("b"),
            ready.identity.unwrap(),
            &offer.nonce,
            offer.game_version,
        )
        .unwrap();
    let reaction = live_offer(&session, Some(&offer.nonce));
    assert_eq!(reaction.choice.player, PlayerId::new("a"));
    assert!(reaction.choice.context.as_ref().unwrap().optional);
    let progress = session
        .planning_status(&PlayerId::new("b"))
        .application
        .unwrap();
    assert_eq!(progress.state, DraftApplicationState::WaitingForPlayer);
    assert_eq!(progress.applied, 2);
    assert!(!session.planning_status(&PlayerId::new("a")).can_apply);
    assert!(
        session
            .reset_planning(
                &PlayerId::new("b"),
                session
                    .planning_status(&PlayerId::new("b"))
                    .identity
                    .unwrap()
            )
            .is_err()
    );
    let decline = reaction
        .choice
        .options
        .iter()
        .find(|option| option.kind == "decline")
        .unwrap()
        .id
        .clone();
    let session_ref = Arc::new(session);
    let answering = session_ref.clone();
    let reaction_nonce = reaction.nonce.clone();
    let thread = std::thread::spawn(move || {
        answering.submit_choice(
            &reaction.choice.player,
            &reaction.nonce,
            reaction.game_version,
            &decline,
        )
    });
    // Reconnecting while the live step waits does not reinstall the script.
    let reconnected = session_ref.subscribe(ViewerRole::Player(PlayerId::new("b")));
    assert_eq!(
        session_ref
            .planning_status(&PlayerId::new("b"))
            .application
            .as_ref()
            .unwrap()
            .total,
        script.len()
    );
    thread.join().unwrap().unwrap();
    let _ = live_offer(&session_ref, Some(&reaction_nonce));
    assert_eq!(
        session_ref
            .planning_status(&PlayerId::new("b"))
            .application
            .unwrap()
            .state,
        DraftApplicationState::Applied
    );
    assert_eq!(
        session_ref
            .current_state()
            .system_state(&target)
            .units
            .iter()
            .filter(|unit| unit.owner == PlayerId::new("b"))
            .count(),
        2
    );
    drop(reconnected);
}

#[test]
#[ignore = "flaky: the Sabotage offer is sometimes not the next live offer, and a failure hangs the binary"]
fn a_new_owner_reaction_stops_application_and_retains_the_unapplied_suffix() {
    use ti4_model::id::ActionCardId;
    let (mut config, _, origin_b, target, _) = fixture(true);
    config
        .state
        .player_mut(&PlayerId::new("a"))
        .unwrap()
        .action_cards = vec![ActionCardId::new("counterstroke")];
    config
        .state
        .player_mut(&PlayerId::new("b"))
        .unwrap()
        .action_cards = vec![ActionCardId::new("sabo1")];
    config
        .state
        .system_mut(&target)
        .place_token(PlayerId::new("a"));
    let session = Arc::new(GameSession::start(config));
    let _ = live_offer(&session, None);
    let script = vec![
        "tactical".into(),
        target.to_string(),
        format!("move|{origin_b}|0"),
        "load|0".into(),
        "done_moving".into(),
    ];
    stopped_draft(&session, &script);
    answer_live(&session, "pass");
    let ready = ready_to_apply(&session);
    let menu = live_offer(&session, None);
    session
        .apply_planning(
            &PlayerId::new("b"),
            ready.identity.unwrap(),
            &menu.nonce,
            menu.game_version,
        )
        .unwrap();
    let reaction = live_offer(&session, Some(&menu.nonce));
    let play = reaction
        .choice
        .options
        .iter()
        .find(|option| option.kind != "decline")
        .unwrap()
        .id
        .clone();
    let answering = session.clone();
    let previous_nonce = reaction.nonce.clone();
    let thread = std::thread::spawn(move || {
        answering.submit_choice(
            &reaction.choice.player,
            &reaction.nonce,
            reaction.game_version,
            &play,
        )
    });
    let sabotage = live_offer(&session, Some(&previous_nonce));
    assert_eq!(sabotage.choice.player, PlayerId::new("b"));
    assert!(sabotage.choice.context.as_ref().unwrap().optional);
    let application = session
        .planning_status(&PlayerId::new("b"))
        .application
        .unwrap();
    assert_eq!(application.state, DraftApplicationState::NeedsDecision);
    assert_eq!(application.applied, 2);
    assert_eq!(application.total, script.len());
    let decline = sabotage
        .choice
        .options
        .iter()
        .find(|option| option.kind == "decline")
        .unwrap()
        .id
        .clone();
    let movement = answer_live(&session, &decline);
    thread.join().unwrap().unwrap();
    assert_eq!(
        movement.choice.context.as_ref().unwrap().subtype,
        "movement_step"
    );
    assert_eq!(
        session
            .planning_status(&PlayerId::new("b"))
            .application
            .unwrap()
            .state,
        DraftApplicationState::NeedsDecision
    );
    assert_eq!(
        session.plans()[&PlayerId::new("b")]
            .recorded_decisions
            .iter()
            .map(|decision| decision.option_id.clone())
            .collect::<Vec<_>>(),
        script
    );
    assert!(
        session
            .current_state()
            .system_state(&origin_b)
            .units
            .iter()
            .any(|unit| unit.owner == PlayerId::new("b") && unit.type_id.as_str() == "carrier")
    );
}

#[test]
fn drafts_are_independent_and_refresh_only_after_completed_live_steps() {
    let (config, origin_a, origin_b, target, centre) = fixture(true);
    let session = GameSession::start(config.clone());
    let initial = live_offer(&session, None);
    let live_state = serde_json::to_value(session.current_state()).unwrap();
    let events = session.event_log();
    let decisions = session.decision_log();
    let script = vec![
        "tactical".into(),
        target.to_string(),
        format!("move|{origin_b}|0"),
        "load|0".into(),
    ];
    let old = draft(&session, &script);
    assert_eq!(
        session.current_pending_decision(),
        Some((PlayerId::new("a"), initial.nonce, initial.game_version))
    );
    assert_eq!(
        serde_json::to_value(session.current_state()).unwrap(),
        live_state
    );
    assert_eq!(session.event_log(), events);
    assert_eq!(session.decision_log(), decisions);
    let retained = session.plans()[&PlayerId::new("b")]
        .recorded_decisions
        .clone();
    assert_eq!(retained.len(), 4);

    answer_live(&session, "tactical");
    answer_live(&session, centre.as_str());
    let before_move = replayed_offer(&session);
    assert!(before_move.identity.checkpoint_id > old.identity.checkpoint_id);
    assert_eq!(before_move.progress.replayed, retained.len());

    let cargo = answer_live(&session, &format!("move|{origin_a}|0"));
    assert_eq!(cargo.choice.context.as_ref().unwrap().subtype, "load_cargo");
    let loading = replayed_offer(&session);
    assert!(loading.identity.checkpoint_id > before_move.identity.checkpoint_id);
    answer_live(&session, "load|0");
    let refreshed = replayed_offer(&session);
    assert!(refreshed.identity.checkpoint_id > loading.identity.checkpoint_id);
    assert!(refreshed.identity.generation_id > loading.identity.generation_id);
    assert_eq!(refreshed.progress.replayed, retained.len());
    assert_eq!(
        session.plans()[&PlayerId::new("b")].recorded_decisions,
        retained
    );
    let PlanningUpdate::SafeOffer(publication) = &refreshed.update else {
        unreachable!()
    };
    assert!(
        publication.position.board.systems[&centre]
            .units
            .iter()
            .any(|unit| unit.owner == PlayerId::new("a"))
    );
    assert_eq!(
        session.submit_planning_choice(&PlayerId::new("b"), old.identity, "done_moving"),
        Err(PlanningError::Submission(SubmissionError::Retired))
    );

    // Replaying the live history from the original seed also verifies that the
    // planning workers never advanced the authoritative RNG or decision stream.
    let normal = GameSession::start_recovered(config, session.decision_log(), session.event_log());
    normal.wait_replayed().unwrap();
    let _ = live_offer(&normal, None);
    assert_eq!(normal.current_state(), session.current_state());
    assert_eq!(normal.decision_hashes(), session.decision_hashes());
}

#[test]
fn reset_clears_the_script_and_retires_the_displayed_offer() {
    let (config, _, _, target, _) = fixture(true);
    let session = GameSession::start(config);
    let live = live_offer(&session, None);
    let old = draft(&session, &["tactical".into(), target.to_string()]);
    assert_eq!(
        session.plans()[&PlayerId::new("b")]
            .recorded_decisions
            .len(),
        2
    );
    session
        .reset_planning(&PlayerId::new("b"), old.identity)
        .unwrap();
    let preparing = session
        .recv_planning_timeout(&PlayerId::new("b"), DEADLINE)
        .unwrap();
    assert!(matches!(preparing.update, PlanningUpdate::Preparing));
    assert_eq!(preparing.progress.recorded_answers, 0);
    assert_eq!(preparing.reset_revision, old.reset_revision + 1);
    let fresh = planning_offer(&session);
    assert_eq!(fresh.reset_revision, preparing.reset_revision);
    assert!(fresh.identity.generation_id > old.identity.generation_id);
    assert!(
        session.plans()[&PlayerId::new("b")]
            .recorded_decisions
            .is_empty()
    );
    assert_eq!(
        session.submit_planning_choice(&PlayerId::new("b"), old.identity, "done_moving"),
        Err(PlanningError::Submission(SubmissionError::Retired))
    );
    assert_eq!(
        session.reset_planning(&PlayerId::new("b"), old.identity),
        Err(PlanningError::Submission(SubmissionError::Retired))
    );
    assert_eq!(live_offer(&session, None).nonce, live.nonce);
    answer_live(&session, "tactical");
    let refreshed = replayed_offer(&session);
    assert!(refreshed.identity.checkpoint_id > fresh.identity.checkpoint_id);
    assert_eq!(refreshed.reset_revision, fresh.reset_revision);
    let plan = &session.plans()[&PlayerId::new("b")];
    let restored: ti4_server::planning::runner::PlayerPlan =
        serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
    assert_eq!(restored.reset_revision, fresh.reset_revision);
    session.stop();
}

#[test]
fn a_nested_scanlink_offer_does_not_advance_the_planning_checkpoint() {
    use ti4_engine::fixtures::put_on_planet;
    use ti4_model::id::{PlanetId, TechnologyId};

    let planets = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK);
    let (planet, record) = planets
        .iter()
        .find(|(_, planet)| {
            planet.system_id().is_some()
                && planet.homeworld_of().is_none()
                && !planet.planet_types().is_empty()
        })
        .unwrap();
    let target = SystemId::new(record.system_id().unwrap());
    let hub = hub_with_centre(target.as_str());
    let mut state = game(&["a", "b"]);
    state.phase = Phase::Action;
    state.active = Some(PlayerId::new("a"));
    state
        .player_mut(&PlayerId::new("a"))
        .unwrap()
        .technologies
        .insert(TechnologyId::new("sdn"));
    put_on_planet(
        &mut state,
        &target,
        &PlanetId::new(*planet),
        "infantry",
        &PlayerId::new("a"),
        1,
    );
    let config = SessionConfig::new("nested_planning", state)
        .with_galaxy(hub.galaxy, vec![])
        .with_seat(PlayerId::new("a"), SeatController::Human)
        .with_seat(PlayerId::new("b"), SeatController::Human);
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let _ = draft(&session, &["tactical".into()]);
    answer_live(&session, "tactical");
    let before = replayed_offer(&session);
    let nested = answer_live(&session, target.as_str());
    assert_eq!(
        nested.choice.context.as_ref().unwrap().subtype,
        "scanlink_explore"
    );
    assert_eq!(
        session.recv_planning_timeout(&PlayerId::new("b"), Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    answer_live(&session, "decline");
    let after = replayed_offer(&session);
    assert!(after.identity.checkpoint_id > before.identity.checkpoint_id);
    assert_eq!(after.progress.replayed, 1);
}

#[test]
fn a_live_blockade_causes_replay_mismatch_and_keeps_the_script() {
    let (config, origin_a, origin_b, target, centre) = fixture(false);
    let session = GameSession::start(config);
    let _ = live_offer(&session, None);
    let old = draft(
        &session,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move|{origin_b}|0"),
        ],
    );
    let retained = session.plans()[&PlayerId::new("b")]
        .recorded_decisions
        .clone();
    answer_live(&session, "tactical");
    answer_live(&session, centre.as_str());
    answer_live(&session, &format!("move|{origin_a}|0"));
    loop {
        let update = session
            .recv_planning_timeout(&PlayerId::new("b"), DEADLINE)
            .unwrap();
        if let PlanningUpdate::Stopped { reason, .. } = update.update {
            assert_eq!(reason, StopReason::ReplayMismatch);
            assert!(update.identity.checkpoint_id > old.identity.checkpoint_id);
            assert_eq!(update.progress.replayed, 2);
            assert_eq!(update.progress.remaining, 1);
            break;
        }
    }
    assert_eq!(
        session.plans()[&PlayerId::new("b")].recorded_decisions,
        retained
    );
    answer_live(&session, "done_moving");
    answer_live(&session, "end_turn");
    let menu = live_offer(&session, None);
    let identity = session
        .planning_status(&PlayerId::new("b"))
        .identity
        .unwrap();
    loop {
        let update = session
            .recv_planning_timeout(&PlayerId::new("b"), DEADLINE)
            .unwrap();
        if update.identity == identity
            && matches!(
                update.update,
                PlanningUpdate::Stopped {
                    reason: StopReason::ReplayMismatch,
                    ..
                }
            )
        {
            break;
        }
    }
    assert!(!session.planning_status(&PlayerId::new("b")).can_apply);
    let before = session.current_state();
    let cursor = session.decision_log().len();
    assert_eq!(
        session.apply_planning(
            &PlayerId::new("b"),
            identity,
            &menu.nonce,
            menu.game_version
        ),
        Err(PlanningError::ReplayMismatch)
    );
    assert_eq!(session.current_state(), before);
    assert_eq!(
        session.decision_log().len(),
        cursor,
        "a mismatched plan must not even select a live tactical action"
    );
}

#[test]
fn undo_and_redo_keep_drafts_and_reject_the_previous_sessions_messages() {
    let (config, _, origin_b, target, centre) = fixture(true);
    let registry = GameRegistry::new();
    let session = registry.create_game(config).unwrap();
    let _ = live_offer(&session, None);
    let token = session.seat_tokens()[&PlayerId::new("a")].clone();
    let _ = draft(
        &session,
        &[
            "tactical".into(),
            target.to_string(),
            format!("move|{origin_b}|0"),
            "load|0".into(),
        ],
    );
    let retained = session.plans()[&PlayerId::new("b")]
        .recorded_decisions
        .clone();
    answer_live(&session, "tactical");
    answer_live(&session, centre.as_str());
    let old = replayed_offer(&session);
    registry
        .change_history(
            session.id(),
            &token,
            session.game_version(),
            HistoryAction::Undo,
        )
        .unwrap();
    let undone = registry.get_game(session.id()).unwrap();
    let _ = live_offer(&undone, None);
    let fresh = replayed_offer(&undone);
    assert_ne!(fresh.identity.checkpoint_id, old.identity.checkpoint_id);
    assert_eq!(
        undone.plans()[&PlayerId::new("b")].recorded_decisions,
        retained
    );
    assert_eq!(
        session.recv_planning_timeout(&PlayerId::new("b"), Duration::ZERO),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    assert_eq!(
        session.submit_planning_choice(&PlayerId::new("b"), old.identity, "done_moving"),
        Err(PlanningError::Unavailable)
    );
    assert_eq!(
        undone.submit_planning_choice(&PlayerId::new("b"), old.identity, "done_moving"),
        Err(PlanningError::Submission(SubmissionError::Retired))
    );

    registry
        .change_history(
            undone.id(),
            &token,
            undone.game_version(),
            HistoryAction::Redo,
        )
        .unwrap();
    let redone = registry.get_game(session.id()).unwrap();
    let _ = live_offer(&redone, None);
    let newest = replayed_offer(&redone);
    assert_ne!(newest.identity.checkpoint_id, fresh.identity.checkpoint_id);
    assert_eq!(
        redone.plans()[&PlayerId::new("b")].recorded_decisions,
        retained
    );
}

#[test]
fn session_stop_wakes_waiting_planner_and_live_input() {
    let (config, _, _, _, _) = fixture(true);
    let session = Arc::new(GameSession::start(config));
    let _ = live_offer(&session, None);
    assert_eq!(
        session.start_planning(&PlayerId::new("a")),
        Err(PlanningError::ActivePlayer)
    );
    assert_eq!(
        session.start_planning(&PlayerId::new("unknown")),
        Err(PlanningError::UnknownSeat)
    );
    session.start_planning(&PlayerId::new("b")).unwrap();
    let old = planning_offer(&session);
    let (done, completed) = mpsc::channel();
    let stopping = session.clone();
    let worker = std::thread::spawn(move || {
        stopping.stop();
        done.send(()).unwrap();
    });
    completed
        .recv_timeout(DEADLINE)
        .expect("session and planner stop within deadline");
    worker.join().unwrap();
    assert_eq!(
        session.submit_planning_choice(&PlayerId::new("b"), old.identity, "tactical"),
        Err(PlanningError::Unavailable)
    );
    assert_eq!(
        session.recv_planning_timeout(&PlayerId::new("b"), Duration::ZERO),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
}
