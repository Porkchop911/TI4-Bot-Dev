//! A session made by a history change (undo, redo, restore), a batch commit or a recovery keeps
//! `history_active` (its timeline lives in `history.json`) for its whole life, but it is only
//! *replaying* until the recorded decisions are used up. Afterwards it is a live game: bluff
//! holds fire, auto-resolve notes are published. During the replay they stay quiet.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::setup::start_game_seeded;
use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::ViewerRole;
use ti4_model::POK;
use ti4_model::id::PlayerId;
use ti4_server::dev::launch_scenario;
use ti4_server::session::bluff::BluffPolicy;
use ti4_server::session::registry::HistoryAction;
use ti4_server::session::{GameRegistry, GameSession, MockClient, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

fn temp_store() -> (std::path::PathBuf, Arc<FileGameStore>) {
    let dir = std::env::temp_dir().join(format!("ti4_hist_live_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).expect("store"));
    (dir, store)
}

fn draft_session(
    registry: &GameRegistry,
    game_id: &str,
) -> (Arc<GameSession>, String, Vec<PlayerId>) {
    let ids: Vec<PlayerId> = ["p1", "p2", "p3", "p4"]
        .iter()
        .map(|id| PlayerId::new(*id))
        .collect();
    let state = start_game_seeded(ContentStore::embedded(), &ids, POK, None, 42).expect("game");
    let mut config = SessionConfig::new(game_id, state)
        .with_seed(42)
        .with_player_ids(ids.clone());
    for id in &ids {
        config = config.with_seat(id.clone(), SeatController::Human);
    }
    let session = registry.create_game(config).expect("create");
    let token = session.seat_tokens()[&ids[0]].clone();
    (session, token, ids)
}

fn clients(session: &Arc<GameSession>, ids: &[PlayerId]) -> Vec<MockClient> {
    ids.iter()
        .map(|id| MockClient::connect(session.clone(), ViewerRole::Player(id.clone())))
        .collect()
}

/// Auto-resolve reasons heard by any seat so far.
fn heard(clients: &[MockClient]) -> Vec<String> {
    let mut reasons = Vec::new();
    for client in clients {
        while let Ok(message) = client.try_recv() {
            if let ServerMessage::StateUpdate(update) = message {
                reasons.extend(update.auto_resolved.into_iter().map(|note| note.reason));
            }
        }
    }
    reasons
}

/// Take the first unclaimed card for whoever is asked, until the draft is over.
fn finish_draft(session: &GameSession) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while session.current_state().phase == ti4_model::Phase::Strategy {
        assert!(
            Instant::now() < deadline,
            "the draft stalled: {:?}",
            session.error()
        );
        if let Some((seat, nonce, version)) = session.current_pending_decision() {
            let option = session.current_state().unclaimed_strategy_cards[0].to_string();
            let _ = session.submit_choice(&seat, &nonce, version, &option);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn pick(session: &GameSession, times: usize) {
    for _ in 0..times {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            assert!(Instant::now() < deadline, "no pick: {:?}", session.error());
            if let Some((seat, nonce, version)) = session.current_pending_decision() {
                let option = session.current_state().unclaimed_strategy_cards[0].to_string();
                if session
                    .submit_choice(&seat, &nonce, version, &option)
                    .is_ok()
                {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn after_an_undo_the_auto_resolve_note_is_published_as_in_a_fresh_game() {
    let (dir, store) = temp_store();
    let registry = GameRegistry::new().with_store(store);
    let (session, token, ids) = draft_session(&registry, "hist_note_undo");
    pick(&session, 4);
    wait_ready(&session);
    let version = session.game_version();
    registry
        .change_history("hist_note_undo", &token, version, HistoryAction::Undo)
        .expect("undo");
    let after = registry.get_game("hist_note_undo").expect("replacement");
    after.wait_replayed().expect("replayed");
    assert!(
        !after.is_replaying(),
        "the replay is over once a live decision is pending"
    );
    let seats = clients(&after, &ids);
    finish_draft(&after);
    std::thread::sleep(Duration::from_millis(100));
    let reasons = heard(&seats);
    assert_eq!(
        reasons,
        ["only one strategy card left"],
        "the last card is taken for its picker"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_recovered_session_stays_quiet_while_it_replays_then_is_live() {
    let (dir, store) = temp_store();
    {
        let registry = GameRegistry::new().with_store(store.clone());
        let (session, _, _) = draft_session(&registry, "hist_note_recover");
        finish_draft(&session);
        session.stop();
    }
    let recovered = Arc::new(store.recover_session("hist_note_recover").expect("recover"));
    let ids: Vec<PlayerId> = ["p1", "p2", "p3", "p4"]
        .iter()
        .map(|id| PlayerId::new(*id))
        .collect();
    // Subscribed before the replay can finish: nothing the players already saw is repeated.
    let seats = clients(&recovered, &ids);
    recovered.wait_replayed().expect("replayed");
    assert!(!recovered.is_replaying());
    std::thread::sleep(Duration::from_millis(100));
    let h = heard(&seats);
    assert!(
        h.is_empty(),
        "the note of the recorded draft is not repeated: {h:?} {}",
        recovered.decision_log().len()
    );
    let _ = std::fs::remove_dir_all(dir);
}

const SEED: u64 = 54_322;

fn faction_seat(session: &GameSession, faction: &str) -> PlayerId {
    session
        .current_state()
        .players
        .iter()
        .find(|player| player.faction.as_str() == faction)
        .map(|player| player.id.clone())
        .expect("seat")
}

/// Answer pending decisions with their first option; returns the seats seen holding.
fn drive(session: &GameSession, steps: usize) -> Vec<PlayerId> {
    let mut held = Vec::new();
    for _ in 0..steps {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(seat) = session.bluff_holding()
                && held.last() != Some(&seat)
            {
                held.push(seat);
            }
            if let Some((seat, nonce, version)) = session.current_pending_decision()
                && let Some(envelope) = session
                    .get_snapshot(&ViewerRole::Player(seat.clone()))
                    .pending_choice
            {
                let option = envelope.choice.options[0].id.clone();
                if session
                    .submit_choice(&seat, &nonce, version, &option)
                    .is_ok()
                {
                    break;
                }
            }
            if session.is_finished() {
                return held;
            }
            assert!(
                Instant::now() < deadline,
                "no decision: {:?}",
                session.error()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    held
}

#[test]
fn after_an_undo_a_declared_bluff_holds_the_game_as_in_a_fresh_one() {
    let short = BluffPolicy {
        min_hold: Duration::from_millis(150),
        max_hold: Duration::from_millis(250),
        hard_cap: Duration::from_secs(1),
        poll: Duration::from_millis(10),
        ..BluffPolicy::default()
    };
    let registry = Arc::new(GameRegistry::new());
    let launch =
        launch_scenario(&registry, "ongoing_combat_four_views", Some(SEED)).expect("launch");
    let session = registry.get_game(&launch.game_id).expect("game");
    wait_ready(&session);
    let version = session.game_version();
    // Rewinding one decision builds a history session (`history_active`) that replays the rest.
    registry
        .change_history(
            &launch.game_id,
            &launch.player_session,
            version,
            HistoryAction::RestoreCursor {
                cursor: session.decision_log().len() - 1,
            },
        )
        .expect("restore");
    let after = registry.get_game(&launch.game_id).expect("replacement");
    after.wait_replayed().expect("replayed");
    assert!(!after.is_replaying());
    after.set_bluff_policy(short);
    let letnev = faction_seat(&after, "letnev");
    after
        .set_reaction_intent(&letnev, &["space_combat".to_owned()])
        .expect("declared");
    let held = drive(&after, 10);
    assert!(
        held.contains(&letnev),
        "the declared window stalled the game: {held:?}"
    );
}

fn wait_pending(session: &GameSession) -> (PlayerId, String, u64) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(pending) = session.current_pending_decision() {
            return pending;
        }
        assert!(
            Instant::now() < deadline,
            "no decision: {:?}",
            session.error()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_session_made_by_a_batch_commit_is_not_replaying_once_live() {
    use ti4_host::batch::{BatchKind, BatchRequest, MovementPlan, MovementStep};
    let (dir, store) = temp_store();
    let registry = GameRegistry::new().with_store(store);
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
            .unwrap();
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new("hist_batch_live", state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest, SeatController::Human);
    config.step_snapshots = true;
    let session = registry.create_game(config).unwrap();
    let token = session.seat_tokens()[&host].clone();
    assert!(!session.is_replaying(), "a fresh game never replays");
    for _ in 0..4 {
        let (seat, nonce, version) = wait_pending(&session);
        let choice = session
            .get_snapshot(&ViewerRole::Player(seat.clone()))
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
    let (seat, nonce, version) = wait_pending(&session);
    let choice = session
        .get_snapshot(&ViewerRole::Player(seat))
        .pending_choice
        .unwrap()
        .choice;
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
    registry
        .submit_batch(
            "hist_batch_live",
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
    let committed = registry.get_game("hist_batch_live").unwrap();
    let _ = wait_pending(&committed);
    assert!(
        !committed.is_replaying(),
        "the replacement is live once it asks a live question"
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn wait_ready(session: &GameSession) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !session.history_ready() {
        assert!(
            Instant::now() < deadline,
            "never ready for a history change"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
