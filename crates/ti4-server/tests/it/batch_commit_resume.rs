//! A committed batch, checked from a step-boundary copy and replaced by a session that resumes
//! from the old session's copy, must leave exactly the game the full-replay path leaves:
//! decision log, events, batch records, state, versions, history generation, the saved
//! `history.json`, and what a recovery from disk replays.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_host::batch::{BatchKind, BatchRequest, FORKED_CHECKS, MovementPlan, MovementStep};
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_server::session::registry::HistoryAction;
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

fn pending(session: &GameSession) -> (PlayerId, String, u64) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(pending) = session.current_pending_decision() {
            return pending;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for engine choice: {:?}",
            session.error()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn offered(session: &GameSession, seat: &PlayerId) -> ti4_engine::choice::Choice {
    session
        .get_snapshot(&ViewerRole::Player(seat.clone()))
        .pending_choice
        .unwrap()
        .choice
}

/// Blank the random (batch ids) and wall-clock (timestamps) parts of a saved or live record.
fn mask(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for key in ["timestamp", "batch_id"] {
                if map.contains_key(key) {
                    map.insert(key.to_owned(), serde_json::Value::Null);
                }
            }
            map.values_mut().for_each(mask);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(mask),
        _ => {}
    }
}

/// Everything observable about a game, with the random and wall-clock parts masked.
fn observe(session: &GameSession) -> serde_json::Value {
    let mut observed = serde_json::json!({
        "decisions": session.decision_log(),
        "events": session.event_log(),
        "state": session.current_state(),
        "version": session.game_version(),
        "history": session.history_status(),
        "batches": session.batches(),
        "rng_marks": session.rng_marks(),
    });
    mask(&mut observed);
    observed
}

struct Outcome {
    live: serde_json::Value,
    saved: serde_json::Value,
    recovered: serde_json::Value,
    redone: serde_json::Value,
    forked_checks: usize,
    resumed_from: Option<usize>,
}

/// Play a short game, commit one movement batch, play on, recover, undo and redo the batch.
fn scenario(step_snapshots: bool) -> Outcome {
    let path = std::env::temp_dir().join(format!("ti4_resume_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
            .unwrap();
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new("resume_probe", state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    config.step_snapshots = step_snapshots;
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    for _ in 0..4 {
        let (seat, nonce, version) = pending(&session);
        let choice = offered(&session, &seat);
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &option.id)
            .unwrap();
    }
    let (seat, nonce, version) = pending(&session);
    assert_eq!(seat, host);
    let choice = offered(&session, &seat);
    assert_eq!(choice.context.as_ref().unwrap().subtype, "movement_step");
    if step_snapshots {
        // The worker kept a copy at a boundary of this very history.
        let copy = session.step_snapshot_len().expect("a step-boundary copy");
        assert!(copy > 0 && copy <= session.decision_log().len());
    } else {
        assert_eq!(session.step_snapshot_len(), None);
    }
    let destination = session
        .current_state()
        .active_system
        .unwrap()
        .as_str()
        .to_owned();
    let mut steps = Vec::new();
    if let Some(option) = choice.options.iter().find(|option| option.kind == "move") {
        steps.push(MovementStep::Move {
            origin: option.payload["origin"].as_str().unwrap().to_owned(),
            unit: option.payload["unit"].as_str().unwrap().to_owned(),
            damaged: option.payload["damaged"].as_bool().unwrap(),
        });
        if option.payload["capacity"].as_i64().unwrap_or(0) > 0 {
            steps.push(MovementStep::DoneLoading);
        }
    }
    steps.push(MovementStep::DoneMoving);
    let checks_before = FORKED_CHECKS.load(Ordering::Relaxed);
    registry
        .submit_batch(
            "resume_probe",
            &token,
            BatchRequest {
                request_id: "confirm_1".into(),
                expected_version: version,
                nonce,
                plan: MovementPlan {
                    kind: BatchKind::TacticalMovement,
                    destination,
                    steps,
                },
            },
        )
        .unwrap();
    let forked_checks = FORKED_CHECKS.load(Ordering::Relaxed) - checks_before;
    let committed = registry.get_game("resume_probe").unwrap();
    let resumed_from = committed.resumed_from();
    // Play on from the replacement session.
    for _ in 0..6 {
        let (seat, nonce, version) = pending(&committed);
        let choice = offered(&committed, &seat);
        committed
            .submit_choice(&seat, &nonce, version, &choice.options[0].id)
            .unwrap();
    }
    let _ = pending(&committed);
    let live = observe(&committed);
    let mut saved = serde_json::to_value(store.load_history("resume_probe").unwrap()).unwrap();
    mask(&mut saved);
    let recovered = {
        let recovered = store.recover_session("resume_probe").unwrap();
        recovered.wait_replayed().unwrap();
        let observed = observe(&recovered);
        recovered.stop();
        observed
    };
    registry
        .change_history(
            "resume_probe",
            &token,
            committed.game_version(),
            HistoryAction::UndoBatch,
        )
        .unwrap();
    let undone = registry.get_game("resume_probe").unwrap();
    let _ = pending(&undone);
    registry
        .change_history(
            "resume_probe",
            &token,
            undone.game_version(),
            HistoryAction::RedoBatch,
        )
        .unwrap();
    let redone_session = registry.get_game("resume_probe").unwrap();
    let _ = pending(&redone_session);
    let redone = observe(&redone_session);
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
    Outcome {
        live,
        saved,
        recovered,
        redone,
        forked_checks,
        resumed_from,
    }
}

#[test]
fn a_batch_checked_and_resumed_from_step_copies_leaves_the_same_game_as_full_replays() {
    let with_copies = scenario(true);
    let without = scenario(false);
    assert!(
        with_copies.forked_checks >= 1,
        "the check should fork the worker's copy"
    );
    assert!(
        with_copies.resumed_from.is_some_and(|at| at > 0),
        "the replacement should resume from the old session's copy: {:?}",
        with_copies.resumed_from
    );
    assert_eq!(without.resumed_from, None);
    assert_eq!(
        with_copies.live, without.live,
        "live game after the batch and six decisions"
    );
    assert_eq!(with_copies.saved, without.saved, "history.json");
    assert_eq!(
        with_copies.recovered, without.recovered,
        "recovery from disk"
    );
    assert_eq!(
        with_copies.redone, without.redone,
        "undo then redo of the batch"
    );
    for key in [
        "decisions",
        "events",
        "state",
        "batches",
        "history",
        "rng_marks",
    ] {
        assert_eq!(
            with_copies.live[key], with_copies.recovered[key],
            "recovery replays the saved log to the same {key}"
        );
    }
}
