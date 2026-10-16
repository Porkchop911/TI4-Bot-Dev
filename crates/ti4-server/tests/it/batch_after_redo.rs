//! A movement batch confirmed on a timeline a turn redo produced: the check that forks the
//! worker's step-boundary copy, the replacement session that resumes from it, and a recovery
//! from disk must all leave the game the full-replay path leaves, although the history now
//! carries forced RNG positions and card reservations (`RngMarks` with a deck plan).
//!
//! Run it also with `TI4_BATCH_VERIFY=1`: every batch check then runs the forked path AND the
//! full replay and panics when they differ.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_host::batch::{BatchKind, BatchRequest, FORKED_CHECKS, MovementPlan, MovementStep};
use ti4_host::history::turn_redo::find_turns;
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

const GAME: &str = "batch_after_redo";

fn pending(session: &GameSession) -> (PlayerId, String, u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(pending) = session.current_pending_decision() {
            return pending;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for engine choice: {:?}",
            session.error()
        );
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn offered(session: &GameSession, seat: &PlayerId) -> ti4_engine::choice::Choice {
    session
        .get_snapshot(&ViewerRole::Player(seat.clone()))
        .pending_choice
        .unwrap()
        .choice
}

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

fn observe(session: &GameSession) -> serde_json::Value {
    let mut observed = serde_json::json!({
        "decisions": session.decision_log(),
        "events": session.event_log(),
        "state": session.current_state(),
        "version": session.game_version(),
        "batches": session.batches(),
        "rng_marks": session.rng_marks(),
    });
    mask(&mut observed);
    observed
}

fn settle(session: &GameSession) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !session.history_ready() {
        assert!(Instant::now() < deadline, "history never became ready");
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// Answer the pending decision with the option `pick` chooses (by option ids).
fn answer(session: &GameSession, pick: impl Fn(&[String]) -> usize) {
    for _ in 0..50 {
        let (seat, nonce, version) = pending(session);
        let ids: Vec<String> = offered(session, &seat)
            .options
            .iter()
            .map(|o| o.id.clone())
            .collect();
        if session
            .submit_choice(&seat, &nonce, version, &ids[pick(&ids)])
            .is_ok()
        {
            return;
        }
    }
    panic!("no decision could be submitted");
}

fn two_turns_done(log: &[ti4_engine::choice::DecisionRecord]) -> bool {
    let spans = find_turns(log);
    let done = |seat: &str| {
        spans
            .iter()
            .filter(|t| t.seat.as_str() == seat && t.complete)
            .count()
    };
    done("p1") >= 2 && done("p2") >= 2 && spans.last().is_some_and(|t| t.end + 2 <= log.len())
}

struct Outcome {
    live: serde_json::Value,
    saved: serde_json::Value,
    recovered: serde_json::Value,
    forked_checks: usize,
    resumed_from: Option<usize>,
    batch_decisions: usize,
}

fn scenario(step_snapshots: bool) -> Outcome {
    let path = std::env::temp_dir().join(format!("ti4_batch_redo_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let (host, guest, third) = (
        PlayerId::new("p1"),
        PlayerId::new("p2"),
        PlayerId::new("p3"),
    );
    let players = vec![host.clone(), guest.clone(), third.clone()];
    let (state, galaxy) =
        ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
            .unwrap();
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(GAME, state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest.clone(), SeatController::Human)
        .with_seat(third, SeatController::BotFirstOption)
        .with_store(store.clone());
    config.step_snapshots = step_snapshots;
    let session = registry.create_game(config).unwrap();
    let tokens = session.seat_tokens();
    let host_token = tokens[&host].clone();

    // Two turns each, then redo the host's last turn with a different new turn and auto-play.
    for _ in 0..600 {
        if two_turns_done(&registry.get_game(GAME).unwrap().decision_log()) {
            break;
        }
        answer(&registry.get_game(GAME).unwrap(), |_| 0);
    }
    let session = registry.get_game(GAME).unwrap();
    settle(&session);
    registry
        .turn_redo_request(
            GAME,
            &host_token,
            session.game_version(),
            Some("p1"),
            Some(1),
        )
        .expect("redo");
    for _ in 0..200 {
        let status = registry
            .turn_redo_status(GAME, &host_token)
            .unwrap()
            .unwrap();
        if status.turn_complete {
            break;
        }
        answer(&registry.get_game(GAME).unwrap(), |ids| ids.len() - 1);
    }
    let session = registry.get_game(GAME).unwrap();
    settle(&session);
    registry
        .turn_redo_autoplay(GAME, &host_token, session.game_version())
        .expect("auto-play");
    let session = registry.get_game(GAME).unwrap();
    settle(&session);
    assert!(
        !session.rng_marks().is_empty(),
        "the redone history carries marks / reservations"
    );

    // Play on (the human decisions after the hand-off) until a seat asks for a movement step.
    let mut found = None;
    for _ in 0..500 {
        let session = registry.get_game(GAME).unwrap();
        let (seat, _, _) = pending(&session);
        let choice = offered(&session, &seat);
        if choice
            .context
            .as_ref()
            .is_some_and(|c| c.subtype == "movement_step")
        {
            found = Some(seat);
            break;
        }
        answer(&session, |ids| {
            ids.iter().position(|id| id == "tactical").unwrap_or(0)
        });
    }
    let actor = found.expect("a movement step is reached after the redo");
    let session = registry.get_game(GAME).unwrap();
    settle(&session);
    let (seat, nonce, version) = pending(&session);
    assert_eq!(seat, actor);
    let choice = offered(&session, &seat);
    if step_snapshots {
        assert!(
            session.step_snapshot_len().is_some(),
            "the redone session kept a step-boundary copy"
        );
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
    let before = session.decision_log().len();
    let checks_before = FORKED_CHECKS.load(Ordering::Relaxed);
    let actor_token = tokens[&actor].clone();
    registry
        .submit_batch(
            GAME,
            &actor_token,
            BatchRequest {
                request_id: "confirm_after_redo".into(),
                expected_version: version,
                nonce,
                plan: MovementPlan {
                    kind: BatchKind::TacticalMovement,
                    destination,
                    steps,
                },
            },
        )
        .expect("the batch is accepted on the redone timeline");
    let forked_checks = FORKED_CHECKS.load(Ordering::Relaxed) - checks_before;
    let committed = registry.get_game(GAME).unwrap();
    let resumed_from = committed.resumed_from();
    let batch_decisions = committed.decision_log().len() - before;
    for _ in 0..6 {
        answer(&committed, |_| 0);
    }
    let _ = pending(&committed);
    let live = observe(&committed);
    let mut saved = serde_json::to_value(store.load_history(GAME).unwrap()).unwrap();
    mask(&mut saved);
    let recovered = {
        let recovered = store.recover_session(GAME).unwrap();
        recovered.wait_replayed().unwrap();
        let observed = observe(&recovered);
        recovered.stop();
        observed
    };
    drop(registry);
    std::fs::remove_dir_all(path).unwrap();
    Outcome {
        live,
        saved,
        recovered,
        forked_checks,
        resumed_from,
        batch_decisions,
    }
}

#[test]
fn slow_a_batch_confirmed_after_a_turn_redo_matches_the_full_replay_in_game_marks_and_recovery() {
    let with_copies = scenario(true);
    let without = scenario(false);
    assert!(
        with_copies.forked_checks >= 1,
        "the check should fork the redone session's copy"
    );
    assert!(with_copies.batch_decisions >= 1);
    assert!(
        with_copies.resumed_from.is_some_and(|at| at > 0),
        "the replacement should resume from the redone session's copy: {:?}",
        with_copies.resumed_from
    );
    assert_eq!(without.resumed_from, None);
    assert_eq!(with_copies.live, without.live, "live game after the batch");
    assert_eq!(with_copies.saved, without.saved, "saved history");
    assert_eq!(
        with_copies.recovered, without.recovered,
        "recovery from disk"
    );
    assert_eq!(
        with_copies.live["rng_marks"], with_copies.recovered["rng_marks"],
        "marks survive recovery"
    );
    assert_eq!(with_copies.live["state"], with_copies.recovered["state"]);
    assert_eq!(
        with_copies.live["decisions"],
        with_copies.recovered["decisions"]
    );
}
