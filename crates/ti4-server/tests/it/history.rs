use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_model::id::{PlayerId, SystemId, UnitTypeId};
use ti4_model::units::Unit;
use ti4_server::session::batch::{BatchKind, BatchRequest, MovementPlan, MovementStep};
use ti4_server::session::registry::{HistoryAction, HistoryError};
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

#[test]
fn movement_batch_is_atomic_idempotent_and_undoable() {
    let path = std::env::temp_dir().join(format!("ti4_batch_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("batch_probe", state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    let sequential_config = config.clone();
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    for _ in 0..4 {
        let (seat, nonce, version, _) = pending(&session);
        let choice = session
            .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice;
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    let (seat, nonce, version, _) = pending(&session);
    assert_eq!(seat, host);
    let choice = session
        .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
            host.clone(),
        ))
        .pending_choice
        .unwrap()
        .choice;
    assert_eq!(choice.context.as_ref().unwrap().subtype, "movement_step");
    let destination = session
        .current_state()
        .active_system
        .unwrap()
        .as_str()
        .to_owned();
    let mut request = BatchRequest {
        request_id: "confirm_1".into(),
        expected_version: version,
        nonce,
        plan: MovementPlan {
            kind: BatchKind::TacticalMovement,
            destination,
            steps: vec![MovementStep::DoneMoving],
        },
    };
    let original = session.decision_log();
    if let Some(option) = choice.options.iter().find(|option| option.kind == "move") {
        let origin = option.payload["origin"].as_str().unwrap().to_owned();
        let unit = option.payload["unit"].as_str().unwrap().to_owned();
        let damaged = option.payload["damaged"].as_bool().unwrap();
        let mut steps = vec![MovementStep::Move {
            origin,
            unit,
            damaged,
        }];
        if option.payload["capacity"].as_i64().unwrap_or(0) > 0 {
            steps.push(MovementStep::DoneLoading);
        }
        steps.push(MovementStep::DoneMoving);
        let mut late = request.clone();
        late.request_id = "late_invalid".into();
        late.plan.steps = vec![
            steps[0].clone(),
            MovementStep::Move {
                origin: "missing".into(),
                unit: "carrier".into(),
                damaged: false,
            },
            MovementStep::DoneMoving,
        ];
        assert_eq!(
            registry
                .submit_batch("batch_probe", &token, late)
                .unwrap_err()
                .failed_step,
            1
        );
        assert_eq!(session.decision_log(), original);
        assert!(store.load_history("batch_probe").unwrap().is_none());
        let mut probe = request.clone();
        probe.request_id = "move_probe".into();
        probe.plan.steps = steps;
        let result = registry.submit_batch("batch_probe", &token, probe);
        assert!(result.is_ok(), "ship movement batch: {result:?}");
        let moved = registry.get_game("batch_probe").unwrap();
        assert_eq!(
            moved.decision_log().len(),
            original.len()
                + if option.payload["capacity"].as_i64().unwrap_or(0) > 0 {
                    3
                } else {
                    2
                }
        );
        let sequential =
            GameSession::start_recovered(sequential_config.clone(), original.clone(), vec![]);
        for record in &moved.decision_log()[original.len()..] {
            let (seat, nonce, version, _) = pending(&sequential);
            assert_eq!(seat, record.player);
            sequential
                .submit_choice(&seat, &nonce, version, &record.chosen)
                .unwrap();
        }
        let _ = pending(&sequential);
        let _ = pending(&moved);
        assert_eq!(moved.decision_log(), sequential.decision_log());
        assert_eq!(moved.decision_hashes(), sequential.decision_hashes());
        assert_eq!(moved.current_state(), sequential.current_state());
        sequential.stop();
        registry
            .change_history(
                "batch_probe",
                &token,
                moved.game_version(),
                HistoryAction::UndoBatch,
            )
            .unwrap();
        let restored = registry.get_game("batch_probe").unwrap();
        let (_, fresh_nonce, fresh_version, _) = pending(&restored);
        request.nonce = fresh_nonce;
        request.expected_version = fresh_version;
    }
    let mut invalid = request.clone();
    invalid.plan.steps.insert(
        0,
        MovementStep::Move {
            origin: "missing".into(),
            unit: "carrier".into(),
            damaged: false,
        },
    );
    assert_eq!(
        registry
            .submit_batch("batch_probe", &token, invalid)
            .unwrap_err()
            .failed_step,
        0
    );
    assert_eq!(session.decision_log(), original);
    let committed = registry
        .submit_batch("batch_probe", &token, request.clone())
        .unwrap();
    assert_eq!(committed.start_cursor, original.len());
    assert_eq!(committed.end_cursor, original.len() + 1);
    let duplicate = registry
        .submit_batch("batch_probe", &token, request)
        .unwrap();
    assert_eq!(committed.batch_id, duplicate.batch_id);
    let active = registry.get_game("batch_probe").unwrap();
    assert_eq!(active.decision_log().len(), original.len() + 1);
    let action = active
        .event_log()
        .iter()
        .find(|event| event.decision_count == Some(original.len()))
        .and_then(|event| event.action_id.clone());
    assert!(
        action.is_some(),
        "the engine's action selection should associate follow-up events"
    );
    assert_eq!(active.event_log().last().unwrap().action_id, action);
    let recovered = store.recover_session("batch_probe").unwrap();
    recovered.wait_replayed().unwrap();
    assert_eq!(recovered.decision_log(), active.decision_log());
    assert_eq!(recovered.batches().len(), 1);
    recovered.stop();
    registry
        .change_history(
            "batch_probe",
            &token,
            active.game_version(),
            HistoryAction::UndoBatch,
        )
        .unwrap();
    let undone = registry.get_game("batch_probe").unwrap();
    assert_eq!(undone.decision_log(), original);
    let before_failure = store.load_history("batch_probe").unwrap().unwrap();
    let before_events = undone.event_log();
    let before_status = undone.history_status();
    let (seat, nonce, version, _) = pending(&undone);
    assert_eq!(seat, host);
    let blocked_write = store.game_dir("batch_probe").unwrap().join("history.tmp");
    std::fs::create_dir(&blocked_write).unwrap();
    let failed = registry.submit_batch(
        "batch_probe",
        &token,
        BatchRequest {
            request_id: "failed_write".into(),
            expected_version: version,
            nonce,
            plan: MovementPlan {
                kind: BatchKind::TacticalMovement,
                destination: undone.current_state().active_system.unwrap().to_string(),
                steps: vec![MovementStep::DoneMoving],
            },
        },
    );
    assert!(
        failed.unwrap_err().reason.contains("storage error"),
        "a failed atomic write must reject the batch"
    );
    std::fs::remove_dir(&blocked_write).unwrap();
    let after_failure = registry.get_game("batch_probe").unwrap();
    let _ = pending(&after_failure);
    assert_eq!(after_failure.decision_log(), original);
    assert_eq!(after_failure.event_log(), before_events);
    assert_eq!(after_failure.history_status(), before_status);
    assert_eq!(after_failure.batches().len(), 1);
    assert_eq!(
        serde_json::to_value(store.load_history("batch_probe").unwrap().unwrap()).unwrap(),
        serde_json::to_value(before_failure).unwrap(),
    );
    let recovered_failure = store.recover_session("batch_probe").unwrap();
    recovered_failure.wait_replayed().unwrap();
    assert_eq!(recovered_failure.history_status(), before_status);
    assert_eq!(recovered_failure.decision_log(), original);
    recovered_failure.stop();
    registry
        .change_history(
            "batch_probe",
            &token,
            after_failure.game_version(),
            HistoryAction::RedoBatch,
        )
        .unwrap();
    assert_eq!(
        registry
            .get_game("batch_probe")
            .unwrap()
            .decision_log()
            .len(),
        original.len() + 1
    );
    let redone = registry.get_game("batch_probe").unwrap();
    registry
        .change_history(
            "batch_probe",
            &token,
            redone.game_version(),
            HistoryAction::UndoBatch,
        )
        .unwrap();
    let branch = registry.get_game("batch_probe").unwrap();
    let (seat, nonce, version, option) = pending(&branch);
    branch
        .submit_choice(&seat, &nonce, version, &option)
        .unwrap();
    let _ = pending(&branch);
    assert!(
        branch.batches().is_empty(),
        "a fork must discard the undone batch's request ID"
    );
    let recovered_branch = store.recover_session("batch_probe").unwrap();
    recovered_branch.wait_replayed().unwrap();
    assert!(recovered_branch.batches().is_empty());
    recovered_branch.stop();
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn excess_fleet_batch_replays_at_the_next_choice_and_recovers_its_state() {
    let path =
        std::env::temp_dir().join(format!("ti4_fleet_batch_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let (mut state, galaxy) = ti4_server::map::create_game_with_map(
        ContentStore::embedded(),
        &[host.clone(), guest.clone()],
        42,
    )
    .unwrap();
    let origin = SystemId::new(galaxy.adjacent("22").into_iter().next().unwrap());
    state
        .system_mut(&origin)
        .units
        .extend((0..4).map(|_| Unit::new(UnitTypeId::new("destroyer"), host.clone())));
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("fleet_batch", state)
        .with_seed(42)
        .with_player_ids(vec![host.clone(), guest.clone()])
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    for _ in 0..4 {
        let (seat, nonce, version, _) = pending(&session);
        let choice = session
            .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice;
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    let (seat, nonce, version, _) = pending(&session);
    assert_eq!(seat, host);
    let mut steps = vec![
        MovementStep::Move {
            origin: origin.to_string(),
            unit: "destroyer".into(),
            damaged: false,
        };
        4
    ];
    steps.push(MovementStep::DoneMoving);
    let result = registry
        .submit_batch(
            "fleet_batch",
            &token,
            BatchRequest {
                request_id: "over_supply".into(),
                expected_version: version,
                nonce,
                plan: MovementPlan {
                    kind: BatchKind::TacticalMovement,
                    destination: "22".into(),
                    steps,
                },
            },
        )
        .unwrap();
    assert_eq!(result.end_cursor - result.start_cursor, 5);
    let live = registry.get_game("fleet_batch").unwrap();
    assert_eq!(
        live.current_state()
            .board
            .get(&SystemId::new("22"))
            .unwrap()
            .units_of(&host)
            .iter()
            .filter(|unit| unit.type_id.as_str() == "destroyer")
            .count(),
        4,
    );
    let _ = pending(&live);
    let recovered = store.recover_session("fleet_batch").unwrap();
    recovered.wait_replayed().unwrap();
    assert_eq!(recovered.current_state(), live.current_state());
    assert_eq!(recovered.decision_log(), live.decision_log());
    recovered.stop();
    registry
        .change_history(
            "fleet_batch",
            &token,
            live.game_version(),
            HistoryAction::UndoBatch,
        )
        .unwrap();
    let undone = registry.get_game("fleet_batch").unwrap();
    assert!(
        undone
            .current_state()
            .board
            .get(&SystemId::new("22"))
            .is_none_or(|system| {
                system
                    .units_of(&host)
                    .iter()
                    .all(|unit| unit.type_id.as_str() != "destroyer")
            })
    );
    registry
        .change_history(
            "fleet_batch",
            &token,
            undone.game_version(),
            HistoryAction::RedoBatch,
        )
        .unwrap();
    assert_eq!(
        registry.get_game("fleet_batch").unwrap().current_state(),
        live.current_state()
    );
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn fiftieth_movement_step_failure_preserves_the_durable_redo_branch() {
    let path = std::env::temp_dir().join(format!("ti4_batch_late_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (mut state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let origin = SystemId::new(galaxy.adjacent("22").into_iter().next().unwrap());
    state
        .system_mut(&origin)
        .units
        .extend((0..49).map(|_| Unit::new(UnitTypeId::new("destroyer"), host.clone())));
    // Keep the setup legal when the baseline batch ends movement; otherwise the
    // fleet-supply removal window interrupts replay before the redo branch is made.
    state.player_mut(&host).unwrap().fleet_tokens = 100;
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("late_batch", state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    for _ in 0..4 {
        let (seat, nonce, version, _) = pending(&session);
        let choice = session
            .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice;
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    let (seat, nonce, version, _) = pending(&session);
    assert_eq!(seat, host);
    assert_eq!(
        session
            .current_state()
            .active_system
            .as_ref()
            .unwrap()
            .as_str(),
        "22"
    );
    let choice = session
        .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
            host.clone(),
        ))
        .pending_choice
        .unwrap()
        .choice;
    assert!(choice.options.iter().any(|option| {
        option.kind == "move"
            && option.payload["origin"] == origin.as_str()
            && option.payload["unit"] == "destroyer"
    }));
    let done = BatchRequest {
        request_id: "redo_boundary".into(),
        expected_version: version,
        nonce,
        plan: MovementPlan {
            kind: BatchKind::TacticalMovement,
            destination: "22".into(),
            steps: vec![MovementStep::DoneMoving],
        },
    };
    registry.submit_batch("late_batch", &token, done).unwrap();
    let committed = registry.get_game("late_batch").unwrap();
    registry
        .change_history(
            "late_batch",
            &token,
            committed.game_version(),
            HistoryAction::UndoBatch,
        )
        .unwrap();
    let undone = registry.get_game("late_batch").unwrap();
    let (seat, nonce, version, _) = pending(&undone);
    assert_eq!(seat, host);
    let before_decisions = undone.decision_log();
    let before_events = undone.event_log();
    let before_state = undone.current_state();
    let before_status = undone.history_status();
    let before_batches = serde_json::to_value(undone.batches()).unwrap();
    let history_file = store.game_dir("late_batch").unwrap().join("history.json");
    let before_file = std::fs::read(&history_file).unwrap();
    assert_eq!(before_status.redo_count, 1);

    let mut steps = vec![
        MovementStep::Move {
            origin: origin.to_string(),
            unit: "destroyer".into(),
            damaged: false,
        };
        49
    ];
    steps.push(MovementStep::Move {
        origin: "missing".into(),
        unit: "destroyer".into(),
        damaged: false,
    });
    steps.push(MovementStep::DoneMoving);
    let error = registry
        .submit_batch(
            "late_batch",
            &token,
            BatchRequest {
                request_id: "late_step_50".into(),
                expected_version: version,
                nonce: nonce.clone(),
                plan: MovementPlan {
                    kind: BatchKind::TacticalMovement,
                    destination: "22".into(),
                    steps,
                },
            },
        )
        .unwrap_err();
    assert_eq!(error.failed_step, 49, "{error:?}");
    assert_eq!(error.reason, "option unavailable");
    assert_eq!(
        registry.get_game("late_batch").unwrap().game_version(),
        version
    );
    assert_eq!(
        undone.current_pending_decision(),
        Some((host, nonce, version))
    );
    assert_eq!(undone.decision_log(), before_decisions);
    assert_eq!(undone.event_log(), before_events);
    assert_eq!(undone.current_state(), before_state);
    assert_eq!(undone.history_status(), before_status);
    assert_eq!(
        serde_json::to_value(undone.batches()).unwrap(),
        before_batches
    );
    assert_eq!(std::fs::read(&history_file).unwrap(), before_file);
    let recovered = store.recover_session("late_batch").unwrap();
    recovered.wait_replayed().unwrap();
    assert_eq!(recovered.decision_log(), before_decisions);
    assert_eq!(recovered.event_log(), before_events);
    assert_eq!(recovered.current_state(), before_state);
    assert_eq!(recovered.history_status(), before_status);
    assert_eq!(
        serde_json::to_value(recovered.batches()).unwrap(),
        before_batches
    );
    recovered.stop();
    registry
        .change_history("late_batch", &token, version, HistoryAction::RedoBatch)
        .unwrap();
    assert_eq!(
        registry
            .get_game("late_batch")
            .unwrap()
            .history_status()
            .redo_count,
        0
    );
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

fn pending(session: &GameSession) -> (PlayerId, String, u64, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some((seat, nonce, version)) = session.current_pending_decision() {
            let snapshot = session.get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ));
            let option = snapshot.pending_choice.unwrap().choice.options[0]
                .id
                .clone();
            return (seat, nonce, version, option);
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for engine choice: {:?}",
            session.error()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn live_movement_batch_loads_only_the_selected_galvanized_variant() {
    use ti4_content::galaxy::all_systems;
    use ti4_engine::fixtures::{game, hub_from, plain_systems, put, put_on_planet};
    use ti4_model::{content_types::POK, id::PlanetId, state::Phase};

    for galvanized in [false, true] {
        let owner = PlayerId::new("p1");
        let origin = SystemId::new("01");
        let planet = PlanetId::new("jord");
        let target = SystemId::new(
            *all_systems(ContentStore::embedded(), POK)
                .iter()
                .find(|(_, system)| {
                    system.planets().is_empty() && !system.is_anomaly() && !system.is_hyperlane()
                })
                .unwrap()
                .0,
        );
        let mut ids = vec![target.to_string(), origin.to_string()];
        ids.extend(
            plain_systems(9)
                .into_iter()
                .filter(|id| id != target.as_str() && id != origin.as_str())
                .take(5),
        );
        let mut state = game(&["p1", "p2"]);
        state.phase = Phase::Action;
        state.active = Some(owner.clone());
        put(&mut state, &origin, "carrier", &owner, 1);
        put_on_planet(&mut state, &origin, &planet, "infantry", &owner, 2);
        state
            .system_mut(&origin)
            .planet_units
            .get_mut(&planet)
            .unwrap()[1]
            .galvanized = true;
        let registry = GameRegistry::new();
        let session = registry
            .create_game(
                SessionConfig::new("cargo_variants", state)
                    .with_galaxy(hub_from(&ids).galaxy, vec![])
                    .with_seat(owner.clone(), SeatController::Human)
                    .with_seat(PlayerId::new("p2"), SeatController::Human),
            )
            .unwrap();
        for option in ["tactical", target.as_str()] {
            let (actor, nonce, version, _) = pending(&session);
            session
                .submit_choice(&actor, &nonce, version, option)
                .unwrap();
        }
        let (_, nonce, version, _) = pending(&session);
        let token = session.seat_tokens()[&owner].clone();
        let request: BatchRequest = serde_json::from_value(serde_json::json!({
            "request_id": "cargo-confirmation", "expected_version": version, "nonce": nonce,
            "plan": {
                "kind": "tactical_movement", "destination": target.as_str(),
                "steps": [
                    {"kind": "move", "origin": origin.as_str(), "unit": "carrier", "damaged": false},
                    {"kind": "load", "origin": origin.as_str(), "unit": "infantry", "source": planet.as_str(), "damaged": false, "galvanized": galvanized},
                    {"kind": "done_loading"}, {"kind": "done_moving"}
                ]
            }
        })).unwrap();
        registry
            .submit_batch("cargo_variants", &token, request)
            .unwrap();
        let moved = registry.get_game("cargo_variants").unwrap();
        let _ = pending(&moved);
        let state = moved.current_state();
        let loaded: Vec<_> = state
            .system_state(&target)
            .units
            .iter()
            .filter(|unit| unit.owner == owner && unit.type_id.as_str() == "infantry")
            .map(|unit| unit.galvanized)
            .collect();
        assert_eq!(loaded, vec![galvanized]);
        let left = &state.system_state(&origin).planet_units[&planet];
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].galvanized, !galvanized);
        moved.stop();
    }
}

#[test]
fn movement_batch_survives_a_declined_cargo_hold_for_ground_forces_in_the_active_system() {
    // 95.1: a carrier moving into the active system may pick up own ground forces there, so
    // the engine opens a cargo hold the plan never mentioned. The batch declines it; the
    // recorded decisions then outnumber the planned steps, which must not read as divergence.
    let registry = GameRegistry::new();
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let (mut state, galaxy) = ti4_server::map::create_game_with_map(
        ContentStore::embedded(),
        &[host.clone(), guest.clone()],
        42,
    )
    .unwrap();
    let origin = SystemId::new(galaxy.adjacent("22").into_iter().next().unwrap());
    state
        .system_mut(&origin)
        .units
        .push(Unit::new(UnitTypeId::new("carrier"), host.clone()));
    state
        .system_mut(&SystemId::new("22"))
        .planet_units
        .entry(ti4_model::id::PlanetId::new("tarmann"))
        .or_default()
        .push(Unit::new(UnitTypeId::new("infantry"), host.clone()));
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("cargo_hold_batch", state)
        .with_seed(42)
        .with_player_ids(vec![host.clone(), guest.clone()])
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    for _ in 0..4 {
        let (seat, nonce, version, _) = pending(&session);
        let choice = session
            .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice;
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    let (seat, nonce, version, _) = pending(&session);
    assert_eq!(seat, host);
    let before = session.decision_log().len();
    let request = BatchRequest {
        request_id: "carrier_only".into(),
        expected_version: version,
        nonce,
        plan: MovementPlan {
            kind: BatchKind::TacticalMovement,
            destination: "22".into(),
            steps: vec![
                MovementStep::Move {
                    origin: origin.to_string(),
                    unit: "carrier".into(),
                    damaged: false,
                },
                MovementStep::DoneMoving,
            ],
        },
    };
    let result = registry.submit_batch("cargo_hold_batch", &token, request);
    assert!(
        result.is_ok(),
        "carrier batch with an unplanned hold: {result:?}"
    );
    let log = registry
        .get_game("cargo_hold_batch")
        .unwrap()
        .decision_log();
    assert!(
        log[before..].iter().any(|d| d.chosen == "done_loading"),
        "the hold was offered and declined: {:?}",
        log[before..].iter().map(|d| &d.chosen).collect::<Vec<_>>()
    );
}

/// A two-seat game at the first movement step into system 22: the host has a destroyer in each of
/// two neighbouring systems; the guest holds Rescue ("after a player moves ships into a system that
/// contains your ships") with a ship waiting in 22, so each ship the host moves opens the guest's
/// SHIP_MOVED window.
fn game_with_a_reaction_holder(
    game_id: &str,
    store: Arc<FileGameStore>,
    holds_card: bool,
) -> (GameRegistry, Arc<GameSession>, String, [String; 2]) {
    let registry = GameRegistry::new().with_store(store);
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (mut state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let neighbours: Vec<String> = galaxy
        .adjacent("22")
        .into_iter()
        .map(|id| id.to_string())
        .take(2)
        .collect();
    for origin in &neighbours {
        state
            .system_mut(&SystemId::new(origin.clone()))
            .units
            .push(Unit::new(UnitTypeId::new("carrier"), host.clone()));
    }
    state
        .system_mut(&SystemId::new("22"))
        .units
        .push(Unit::new(UnitTypeId::new("destroyer"), guest.clone()));
    // Ground forces in the active system give every carrier a cargo hold (95.1). The engine
    // announces an arrival only when a hold was opened, so that is what opens the window.
    state
        .system_mut(&SystemId::new("22"))
        .planet_units
        .entry(ti4_model::id::PlanetId::new("tarmann"))
        .or_default()
        .push(Unit::new(UnitTypeId::new("infantry"), host.clone()));
    if holds_card {
        state.player_mut(&guest).unwrap().action_cards =
            vec![ti4_model::id::ActionCardId::new("rescue")];
    }
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new(game_id, state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    for _ in 0..4 {
        let (seat, nonce, version, _) = pending(&session);
        let choice = session
            .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice;
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    (
        registry,
        session,
        token,
        [neighbours[0].clone(), neighbours[1].clone()],
    )
}

fn move_carrier(origin: &str) -> MovementStep {
    MovementStep::Move {
        origin: origin.into(),
        unit: "carrier".into(),
        damaged: false,
    }
}

#[test]
fn movement_batch_stops_at_a_ship_moved_reaction_and_replays_after_a_restart() {
    let path = std::env::temp_dir().join(format!("ti4_react_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let (registry, session, token, [a, b]) =
        game_with_a_reaction_holder("react_batch", store.clone(), true);
    let (seat, nonce, version, _) = pending(&session);
    assert_eq!(seat, PlayerId::new("p1"));
    let before = session.decision_log().len();
    let request = BatchRequest {
        request_id: "two_ships".into(),
        expected_version: version,
        nonce,
        plan: MovementPlan {
            kind: BatchKind::TacticalMovement,
            destination: "22".into(),
            steps: vec![move_carrier(&a), move_carrier(&b), MovementStep::DoneMoving],
        },
    };
    let result = registry
        .submit_batch("react_batch", &token, request.clone())
        .expect("a reaction window is a boundary, not a rejection");
    let interrupted = result.interrupted.clone().expect("structured interruption");
    assert_eq!(interrupted.applied_steps, 1);
    assert_eq!(interrupted.remaining_steps.len(), 2);
    assert!(
        interrupted
            .offered
            .subtype
            .as_deref()
            .unwrap()
            .starts_with("reaction_")
    );
    assert!(!interrupted.offered.own_seat);
    assert!(
        interrupted.offered.option_ids.is_empty(),
        "another seat's options stay private"
    );
    let live = registry.get_game("react_batch").unwrap();
    let recorded = live.decision_log();
    assert_eq!(
        recorded.len(),
        before + 2,
        "the first move and its declined cargo hold"
    );
    assert_eq!(recorded.last().unwrap().chosen, "done_loading");
    let (waiting, _, _) = live.current_pending_decision().unwrap();
    assert_eq!(
        waiting,
        PlayerId::new("p2"),
        "the reaction stays pending for its holder"
    );
    // A repeated request answers the same way instead of re-running anything.
    let again = registry
        .submit_batch("react_batch", &token, request)
        .unwrap();
    assert_eq!(again.batch_id, result.batch_id);
    assert_eq!(again.interrupted.unwrap().applied_steps, 1);
    // Recovery from disk replays the partial batch into the same log and the same pending seat.
    let recovered = store.recover_session("react_batch").unwrap();
    recovered.wait_replayed().unwrap();
    assert_eq!(recovered.decision_log(), live.decision_log());
    assert_eq!(recovered.batches().len(), 1);
    assert_eq!(recovered.current_pending_decision().unwrap().0, waiting);
    assert_eq!(recovered.current_state(), live.current_state());
    recovered.stop();
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn the_remaining_plan_is_revalidated_when_it_is_sent_again_after_the_reaction() {
    let path =
        std::env::temp_dir().join(format!("ti4_react_resume_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let (registry, session, token, [a, b]) =
        game_with_a_reaction_holder("react_resume", store, true);
    let (_, nonce, version, _) = pending(&session);
    let steps = vec![move_carrier(&a), move_carrier(&b), MovementStep::DoneMoving];
    let plan = |steps: Vec<MovementStep>| MovementPlan {
        kind: BatchKind::TacticalMovement,
        destination: "22".into(),
        steps,
    };
    let first = registry
        .submit_batch(
            "react_resume",
            &token,
            BatchRequest {
                request_id: "first".into(),
                expected_version: version,
                nonce,
                plan: plan(steps),
            },
        )
        .unwrap();
    let remaining = first.interrupted.unwrap().remaining_steps;
    let guest = PlayerId::new("p2");
    // Sending the remainder while the reaction is still pending is stale, and changes nothing.
    let live = registry.get_game("react_resume").unwrap();
    let before = live.decision_log();
    let (_, stale_nonce, stale_version) = live.current_pending_decision().unwrap();
    let early = registry.submit_batch(
        "react_resume",
        &token,
        BatchRequest {
            request_id: "too_early".into(),
            expected_version: stale_version,
            nonce: stale_nonce,
            plan: plan(remaining.clone()),
        },
    );
    assert!(early.is_err());
    assert_eq!(live.decision_log(), before);
    // The holder declines; then the remainder is accepted against the fresh offers.
    let (seat, nonce, version) = live.current_pending_decision().unwrap();
    assert_eq!(seat, guest);
    live.submit_choice(&guest, &nonce, version, "decline")
        .unwrap();
    let (seat, nonce, version, _) = pending(&registry.get_game("react_resume").unwrap());
    assert_eq!(seat, PlayerId::new("p1"));
    let rest = registry
        .submit_batch(
            "react_resume",
            &token,
            BatchRequest {
                request_id: "rest".into(),
                expected_version: version,
                nonce,
                plan: plan(remaining),
            },
        )
        .expect("the remaining plan is legal against the current offers");
    // The second ship's arrival opens the reaction again; the plan is interrupted once more.
    let again = rest.interrupted.expect("the holder still has the card");
    assert_eq!(again.applied_steps, 1);
    assert_eq!(again.remaining_steps.len(), 1);
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn movement_batch_with_an_illegal_second_step_is_rejected_without_a_reaction_in_play() {
    let path = std::env::temp_dir().join(format!("ti4_react_bad_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let (registry, session, token, [a, _]) = game_with_a_reaction_holder("react_bad", store, false);
    let (_, nonce, version, _) = pending(&session);
    let before = session.decision_log();
    let request = BatchRequest {
        request_id: "bad_plan".into(),
        expected_version: version,
        nonce,
        plan: MovementPlan {
            kind: BatchKind::TacticalMovement,
            destination: "22".into(),
            steps: vec![
                move_carrier(&a),
                MovementStep::Move {
                    origin: "999".into(),
                    unit: "carrier".into(),
                    damaged: false,
                },
                MovementStep::DoneMoving,
            ],
        },
    };
    let error = registry
        .submit_batch("react_bad", &token, request)
        .unwrap_err();
    assert_ne!(error.reason, "stale decision boundary");
    assert_eq!(
        registry.get_game("react_bad").unwrap().decision_log(),
        before
    );
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn undo_action_rewinds_the_whole_movement_pipeline_and_preserves_redo() {
    let registry = GameRegistry::new();
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("probe", state)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let host_token = session.seat_tokens()[&host].clone();
    for i in 0..6 {
        let (seat, nonce, version, _) = pending(&session);
        let snapshot = session.get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
            seat.clone(),
        ));
        let choice = snapshot.pending_choice.unwrap().choice;
        if i == 4 || i == 5 {
            assert_eq!(choice.prompt, "movement");
        }
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    let _ = pending(&session);
    let decisions = session.decision_log();
    assert_eq!(decisions[2].chosen, "tactical");
    assert_eq!(decisions[4].prompt, "movement");
    let restored = registry
        .change_history(
            "probe",
            &host_token,
            session.game_version(),
            HistoryAction::UndoPipeline,
        )
        .unwrap();
    assert_eq!(restored.history.cursor, 2);
    assert_eq!(restored.history.redo_count, 4);
    let replayed = registry.get_game("probe").unwrap();
    let _ = pending(&replayed);
    assert_eq!(replayed.decision_log(), decisions[..2]);
    registry
        .change_history(
            "probe",
            &host_token,
            replayed.game_version(),
            HistoryAction::RedoPipeline,
        )
        .unwrap();
    let whole_action = registry.get_game("probe").unwrap();
    let _ = pending(&whole_action);
    assert_eq!(whole_action.decision_log(), decisions);
    registry
        .change_history(
            "probe",
            &host_token,
            whole_action.game_version(),
            HistoryAction::UndoPipeline,
        )
        .unwrap();
    let replayed = registry.get_game("probe").unwrap();
    let _ = pending(&replayed);
    registry
        .change_history(
            "probe",
            &host_token,
            replayed.game_version(),
            HistoryAction::Redo,
        )
        .unwrap();
    let redone = registry.get_game("probe").unwrap();
    let _ = pending(&redone);
    assert_eq!(redone.decision_log(), decisions[..3]);
    assert_eq!(redone.history_status().redo_count, 3);
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "exercises a complete durable branch lifecycle"
)]
fn host_rewinds_replays_and_branches_durably() {
    let path = std::env::temp_dir().join(format!("ti4_history_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("history_game", state.clone())
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest.clone(), SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let host_token = session.seat_tokens()[&host].clone();
    let guest_token = session.seat_tokens()[&guest].clone();
    let (seat, nonce, version, option) = pending(&session);
    session
        .submit_choice(&seat, &nonce, version, &option)
        .unwrap();
    let first = pending(&session);
    let after_first = session.current_state();
    session
        .submit_choice(&first.0, &first.1, first.2, &first.3)
        .unwrap();
    let _ = pending(&session);
    let second_log = session.decision_log();
    let second_hashes = session.decision_hashes();
    let snapshot = session.get_snapshot(&ti4_server::protocol::status::ViewerRole::Spectator);
    let first_event = snapshot
        .events
        .iter()
        .find(|event| event.decision_count == Some(1))
        .unwrap()
        .id
        .clone();
    assert!(matches!(
        registry.change_history(
            "history_game",
            &guest_token,
            session.game_version(),
            HistoryAction::Undo
        ),
        Err(HistoryError::Forbidden(_))
    ));
    assert!(matches!(
        registry.change_history("history_game", &host_token, version, HistoryAction::Undo),
        Err(HistoryError::Conflict(_))
    ));
    let restored = registry
        .change_history(
            "history_game",
            &host_token,
            session.game_version(),
            HistoryAction::Restore {
                event_id: first_event,
            },
        )
        .unwrap();
    let replayed = registry.get_game("history_game").unwrap();
    let _ = pending(&replayed);
    assert_eq!(restored.history.cursor, 1);
    assert_eq!(replayed.current_state(), after_first);
    assert_eq!(replayed.decision_log(), second_log[..1]);
    assert_eq!(replayed.history_status().redo_count, 1);
    let restarted_at_cursor = store.recover_session("history_game").unwrap();
    let _ = pending(&restarted_at_cursor);
    assert_eq!(restarted_at_cursor.history_status().cursor, 1);
    assert_eq!(restarted_at_cursor.history_status().redo_count, 1);
    assert_eq!(restarted_at_cursor.current_state(), after_first);
    restarted_at_cursor.stop();
    let history = store.load_history("history_game").unwrap().unwrap();
    let mut invalid_future = history.clone();
    invalid_future.redo[0].chosen = "not_an_offered_option".to_owned();
    store.save_history("history_game", &invalid_future).unwrap();
    assert!(
        store.recover_session("history_game").is_err(),
        "recovery must validate redo, not just the active prefix"
    );
    store.save_history("history_game", &history).unwrap();
    assert!(
        session
            .submit_choice(&first.0, &first.1, first.2, &first.3)
            .is_err()
    );
    let replayed_version = replayed.game_version();
    registry
        .change_history(
            "history_game",
            &host_token,
            replayed_version,
            HistoryAction::Redo,
        )
        .unwrap();
    let redone = registry.get_game("history_game").unwrap();
    let _ = pending(&redone);
    assert_eq!(redone.decision_hashes(), second_hashes);
    assert_eq!(redone.history_status().redo_count, 0);
    registry
        .change_history(
            "history_game",
            &host_token,
            redone.game_version(),
            HistoryAction::RestoreCursor { cursor: 1 },
        )
        .unwrap();
    let forked = registry.get_game("history_game").unwrap();
    let (seat, nonce, version, option) = pending(&forked);
    forked
        .submit_choice(&seat, &nonce, version, &option)
        .unwrap();
    let _ = pending(&forked);
    assert_eq!(forked.history_status().redo_count, 0);
    assert!(matches!(
        registry.change_history(
            "history_game",
            &host_token,
            forked.game_version(),
            HistoryAction::Redo
        ),
        Err(HistoryError::InvalidTarget(_))
    ));
    forked.stop();
    let recovered = store.recover_session("history_game").unwrap();
    let _ = pending(&recovered);
    assert_eq!(recovered.decision_log(), forked.decision_log());
    assert_eq!(recovered.history_status().redo_count, 0);
    recovered.stop();
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "exercises HTTP authorization and socket replacement together"
)]
async fn http_history_requires_the_current_host_and_replaces_connected_sessions() {
    use futures_util::{SinkExt, StreamExt};
    let registry = Arc::new(GameRegistry::new());
    let host = PlayerId::new("host");
    let guest = PlayerId::new("guest");
    let (state, galaxy) = ti4_server::map::create_game_with_map(
        ContentStore::embedded(),
        &[host.clone(), guest.clone()],
        84,
    )
    .unwrap();
    let config = SessionConfig::new("history_http", state)
        .with_galaxy(
            galaxy.clone(),
            ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy),
        )
        .with_player_ids(vec![host.clone(), guest.clone()])
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest.clone(), SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let host_token = session.seat_tokens()[&host].clone();
    let guest_token = session.seat_tokens()[&guest].clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = ti4_server::create_app(registry.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/games/history_http"))
            .await
            .unwrap();
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            serde_json::json!({
                "type": "subscribe", "protocol_version": 3, "game_id": "history_http",
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let first_message = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        first_message
            .to_text()
            .unwrap()
            .contains("initial_snapshot")
    );
    let (seat, nonce, version, option) = pending(&session);
    session
        .submit_choice(&seat, &nonce, version, &option)
        .unwrap();
    let _ = pending(&session);
    let client = reqwest::Client::new();
    let url = format!("http://{addr}/api/games/history_http/history");
    let payload = serde_json::json!({
        "expected_version": session.game_version(),
        "action": "undo_pipeline"
    });
    let guest_result = client
        .post(&url)
        .header("x-ti4-player-session", guest_token)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(
        guest_result.status(),
        reqwest::StatusCode::FORBIDDEN,
        "{}",
        guest_result.text().await.unwrap_or_default()
    );
    let result = client
        .post(&url)
        .header("x-ti4-player-session", host_token.clone())
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(
        result.status(),
        reqwest::StatusCode::OK,
        "{}",
        result.text().await.unwrap_or_default()
    );
    let replacement = registry.get_game("history_http").unwrap();
    let _ = pending(&replacement);
    assert_eq!(replacement.history_status().cursor, 0);
    let mut closed = false;
    let mut replacement_code = None;
    for _ in 0..8 {
        match tokio::time::timeout(Duration::from_secs(1), socket.next()).await {
            Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Close(frame)))) => {
                replacement_code = frame.map(|frame| frame.code);
                closed = true;
                break;
            }
            Ok(None | Some(Err(_))) => {
                closed = true;
                break;
            }
            _ => {}
        }
    }
    assert!(closed, "old connection must close on session replacement");
    assert_eq!(
        replacement_code,
        Some(tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Library(4001))
    );
    let redo = client
        .post(&url)
        .header("x-ti4-player-session", host_token)
        .json(&serde_json::json!({
            "expected_version": replacement.game_version(),
            "action": "redo_pipeline"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        redo.status(),
        reqwest::StatusCode::OK,
        "{}",
        redo.text().await.unwrap_or_default()
    );
    assert_eq!(
        registry
            .get_game("history_http")
            .unwrap()
            .history_status()
            .cursor,
        1
    );
    replacement.stop();
    server.abort();
}
