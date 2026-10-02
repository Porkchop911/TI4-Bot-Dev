#![allow(clippy::too_many_lines)]

use std::fs;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameRegistry, GameSession, MockClient, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

#[test]
fn test_crash_recovery_persists_and_resumes_cleanly() {
    let temp_dir =
        std::env::temp_dir().join(format!("ti4_crash_test_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&temp_dir).expect("create file store"));

    let p1 = PlayerId::new("p1");
    let p2 = PlayerId::new("p2");
    let p3 = PlayerId::new("p3");
    let players = vec![p1.clone(), p2.clone(), p3.clone()];

    let content = ContentStore::embedded();
    let seed = 424_242;
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(content, &players, seed).expect("create map");
    let map_tiles = ti4_server::map::build_board_tiles(content, &galaxy);

    let game_id = "test_recovery_game";
    let config = SessionConfig::new(game_id, state.clone())
        .with_seed(seed)
        .with_player_ids(players)
        .with_store(store.clone())
        .with_galaxy(galaxy.clone(), map_tiles)
        .with_seat(p1.clone(), SeatController::Human)
        .with_seat(p2.clone(), SeatController::Human)
        .with_seat(p3.clone(), SeatController::BotFirstOption);

    // 1. Start game via GameRegistry
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    let session = registry
        .create_game(config)
        .expect("create game in registry");

    // Verify init.json exists on disk
    let init_record = store.load_init(game_id).expect("load init record");
    assert_eq!(init_record.game_id, game_id);
    assert_eq!(init_record.seed, Some(seed));

    // 2. Play two choices (P1 and P2)
    let client1 = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));

    let snap1 = client1.snapshot();
    let pending_1 = if let Some(choice) = snap1.pending_choice {
        choice
    } else {
        loop {
            match client1.recv().expect("recv") {
                ServerMessage::PendingChoice(choice_msg) if choice_msg.choice.player == p1 => {
                    break ti4_server::protocol::PendingChoiceEnvelope {
                        nonce: choice_msg.nonce,
                        choice: choice_msg.choice,
                    };
                }
                _ => {}
            }
        }
    };

    let chosen_1 = &pending_1.choice.options[0].id;
    let (p1_seat, p1_nonce, p1_version) = session
        .current_pending_decision()
        .expect("pending decision for p1");
    assert_eq!(p1_seat, p1);

    session
        .submit_choice(&p1, &p1_nonce, p1_version, chosen_1)
        .expect("submit p1");
    assert_eq!(
        store
            .load_decisions(game_id)
            .expect("load persisted decisions")
            .len(),
        1,
        "a successful response is returned only after the decision is durable"
    );

    // Wait for P2 turn to start and obtain choice
    let client2 = MockClient::connect(session.clone(), ViewerRole::Player(p2.clone()));
    let snap2 = client2.snapshot();
    let p2_choice = if let Some(choice) = snap2.pending_choice {
        choice
    } else {
        loop {
            match client2.recv().expect("recv") {
                ServerMessage::PendingChoice(choice_msg) if choice_msg.choice.player == p2 => {
                    break ti4_server::protocol::PendingChoiceEnvelope {
                        nonce: choice_msg.nonce,
                        choice: choice_msg.choice,
                    };
                }
                _ => {}
            }
        }
    };
    let chosen_2 = &p2_choice.choice.options[0].id;

    let (p2_seat, p2_nonce, p2_version) = session
        .current_pending_decision()
        .expect("pending decision for p2");
    assert_eq!(p2_seat, p2);

    session
        .submit_choice(&p2, &p2_nonce, p2_version, chosen_2)
        .expect("submit p2");

    // Await bot P3 move and next pending choice
    while session.decision_log().len() < 3 || session.current_pending_decision().is_none() {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Capture state immediately before crash
    let pre_crash_hashes = session.decision_hashes();
    assert!(
        pre_crash_hashes.len() >= 3,
        "Expected at least 3 decisions (p1, p2, p3 bot)"
    );
    let pre_crash_records = session.decision_log();
    let pre_crash_events = session.event_log();
    let (pre_pending_seat, _, _) = session
        .current_pending_decision()
        .expect("pending choice before crash");

    // 3. SIMULATE SERVER CRASH / TERMINATION
    session.stop();
    drop(session);
    drop(registry);

    // 4. Verify disk files
    let saved_decisions = store.load_decisions(game_id).expect("load decisions");
    assert_eq!(saved_decisions.len(), pre_crash_records.len());
    let saved_events = store.load_events(game_id).expect("load events");
    assert_eq!(saved_events.len(), pre_crash_events.len());

    // 5. RECOVER SESSION FROM DISK
    let recovered_session = Arc::new(store.recover_session(game_id).expect("recover session"));

    // Verify canonical hashes match exactly
    assert_eq!(
        recovered_session.decision_hashes(),
        pre_crash_hashes,
        "Recovered session canonical hashes must match pre-crash exactly"
    );

    // Verify decision log matches exactly
    assert_eq!(recovered_session.decision_log(), pre_crash_records);

    // Verify event log matches exactly
    let recovered_events = recovered_session.event_log();
    assert_eq!(recovered_events.len(), pre_crash_events.len());
    for (orig, rec) in pre_crash_events.iter().zip(recovered_events.iter()) {
        assert_eq!(orig.id, rec.id);
        assert_eq!(orig.event, rec.event);
        assert_eq!(orig.version, rec.version);
    }

    // Wait for recovered session worker to complete replay and raise pending choice
    let (rec_pending_seat, rec_nonce, rec_version) = {
        let start = std::time::Instant::now();
        loop {
            if let Some(pending) = recovered_session.current_pending_decision() {
                break pending;
            }
            assert!(
                start.elapsed() <= std::time::Duration::from_secs(5),
                "Timed out waiting for recovered session to raise pending choice"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    assert_eq!(rec_pending_seat, pre_pending_seat);

    // 6. CONNECT CLIENT TO RECOVERED SESSION AND PLAY NEXT MOVE
    let rec_client = MockClient::connect(
        recovered_session.clone(),
        ViewerRole::Player(rec_pending_seat.clone()),
    );
    let rec_snapshot = rec_client.snapshot();

    // Snapshot has all events including newly raised decision
    assert_eq!(
        rec_snapshot.events.len(),
        recovered_session.event_log().len()
    );
    assert!(!rec_snapshot.view.board.map_tiles.is_empty());

    let pending_choice = rec_snapshot
        .pending_choice
        .expect("snapshot pending choice");
    assert_eq!(pending_choice.choice.player, rec_pending_seat);
    let next_option = &pending_choice.choice.options[0].id;

    // Submit next choice on recovered session
    let accepted = recovered_session
        .submit_choice(&rec_pending_seat, &rec_nonce, rec_version, next_option)
        .expect("submit on recovered session");
    assert_eq!(&accepted.option_id, next_option);

    // Allow worker loop to append to disk
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Verify that the new decision was appended to decisions.jsonl on disk
    let post_recovery_decisions = store
        .load_decisions(game_id)
        .expect("load updated decisions");
    assert_eq!(
        post_recovery_decisions.len(),
        pre_crash_records.len() + 1,
        "New decision must be persisted to disk after recovery"
    );

    // 7. TEST REGISTRY BOOT RECOVERY
    let new_registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    let recovered_ids = new_registry.recover_all_games().expect("recover all games");
    assert_eq!(recovered_ids, vec![game_id.to_owned()]);

    let registry_session = new_registry
        .get_game(game_id)
        .expect("lookup game in registry");
    assert_eq!(registry_session.id(), game_id);

    recovered_session.stop();
    registry_session.stop();
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn recovery_rejects_mid_log_corruption_but_allows_a_torn_final_record() {
    let temp_dir = std::env::temp_dir().join(format!(
        "ti4_corrupt_log_test_{:016x}",
        rand::random::<u64>()
    ));
    let store = FileGameStore::new(&temp_dir).expect("create file store");
    let game_id = "valid_game";
    let game_dir = store.game_dir(game_id).expect("valid game path");
    fs::create_dir_all(&game_dir).expect("create game directory");

    fs::write(game_dir.join("decisions.jsonl"), b"not-json\n{}").expect("write corrupt log");
    let error = store
        .load_decisions(game_id)
        .expect_err("middle corruption must fail recovery");
    assert!(error.to_string().contains("line 1"));

    fs::write(game_dir.join("decisions.jsonl"), b"{").expect("write torn final record");
    assert!(
        store
            .load_decisions(game_id)
            .expect("a torn final record is recoverable")
            .is_empty()
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn storage_rejects_path_traversal_game_ids() {
    let temp_dir =
        std::env::temp_dir().join(format!("ti4_game_id_test_{:016x}", rand::random::<u64>()));
    let store = FileGameStore::new(&temp_dir).expect("create file store");

    for game_id in ["../escape", "nested/game", "", "game name"] {
        assert!(
            store.game_dir(game_id).is_err(),
            "game id {game_id:?} must not resolve to a storage path"
        );
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn storage_rejects_tampered_envelopes_and_missing_player_order() {
    let temp_dir = std::env::temp_dir().join(format!(
        "ti4_storage_envelope_test_{:016x}",
        rand::random::<u64>()
    ));
    let store = Arc::new(FileGameStore::new(&temp_dir).expect("create file store"));
    let game_id = "enveloped_game";
    let state = ti4_server::fixtures::create_sample_game();
    let registry = GameRegistry::new().with_store(store.clone());
    let missing_order = SessionConfig::new(game_id, state.clone())
        .with_seat(PlayerId::new("seat_a"), SeatController::Human);
    assert!(
        registry
            .create_game(missing_order)
            .err()
            .expect("durable seating order is mandatory")
            .contains("explicit player order")
    );

    let record = ti4_server::GameInitRecord {
        game_id: game_id.to_owned(),
        seed: None,
        player_ids: vec![PlayerId::new("seat_a")],
        initial_state: state,
        seats: std::collections::BTreeMap::new(),
        seat_tokens: std::collections::BTreeMap::new(),
        map_tiles: Vec::new(),
    };
    store.save_init(&record).expect("save enveloped init");
    let init_path = store.game_dir(game_id).expect("game dir").join("init.json");
    let tampered = fs::read_to_string(&init_path).expect("read init").replacen(
        "\"checksum\": \"",
        "\"checksum\": \"0",
        1,
    );
    fs::write(&init_path, tampered).expect("tamper checksum");
    assert!(
        store
            .load_init(game_id)
            .expect_err("tampered init must be rejected")
            .to_string()
            .contains("checksum mismatch")
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn durable_event_failure_stops_the_session() {
    let temp_dir = std::env::temp_dir().join(format!(
        "ti4_event_failure_test_{:016x}",
        rand::random::<u64>()
    ));
    let store = Arc::new(FileGameStore::new(&temp_dir).expect("create file store"));
    let game_id = "event_failure";
    let seat = PlayerId::new("seat_a");
    let config = SessionConfig::new(game_id, ti4_server::fixtures::create_sample_game())
        .with_store(store.clone())
        .with_seat(seat.clone(), SeatController::Human);
    let session = GameSession::start(config);

    let (pending_seat, nonce, version) = loop {
        if let Some(pending) = session.current_pending_decision() {
            break pending;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert_eq!(pending_seat, seat);
    let option = session
        .get_snapshot(&ViewerRole::Player(seat.clone()))
        .pending_choice
        .expect("pending choice")
        .choice
        .options[0]
        .id
        .clone();
    let event_path = store
        .game_dir(game_id)
        .expect("valid game path")
        .join("events.jsonl");
    fs::remove_file(&event_path).expect("remove initial event log");
    fs::create_dir(&event_path).expect("block decision event append with a directory");

    assert!(
        session
            .submit_choice(&seat, &nonce, version, &option)
            .is_err()
    );
    assert!(
        session
            .error()
            .is_some_and(|error| error.contains("failed to persist event")),
        "the storage fault must stop the session instead of being ignored"
    );
    assert!(session.current_pending_decision().is_some());

    session.stop();
    let _ = fs::remove_dir_all(&temp_dir);
}
