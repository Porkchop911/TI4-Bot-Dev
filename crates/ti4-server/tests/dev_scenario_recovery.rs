use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ti4_engine::choice::Choice;
use ti4_model::id::PlayerId;
use ti4_server::dev::execute_launch_scenario;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::registry::LobbyPhase;
use ti4_server::session::{GameRegistry, MockClient, SeatController};
use ti4_server::storage::{
    FileGameStore, PLAYER_RECORD_VERSION, PlayerGameInitRecord, PlayerLobbyRecord,
    PlayerSessionsRecord,
};

fn wait_for_choice(client: &MockClient) -> Option<Choice> {
    for _ in 0..150 {
        if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
            return Some(msg.choice);
        }
        thread::sleep(Duration::from_millis(10));
    }
    None
}

#[test]
fn dev_scenario_space_combat_crash_recovery_preserves_bots_and_play() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_dev_rec_space_combat_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).expect("create store"));
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));

    let launch = execute_launch_scenario(&registry, "space_combat", Some(12345))
        .expect("launch space_combat scenario");
    assert!(launch.game_id.starts_with("dev_combat_"));

    let session = registry
        .get_game(&launch.game_id)
        .expect("session in registry");
    let p1 = PlayerId::new(&launch.player_id);
    let client = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));

    // 1. Initial choice -> "tactical"
    let choice = wait_for_choice(&client).expect("initial tactical choice");
    assert_eq!(choice.player, p1);
    let (_, nonce_1, ver_1) = session.current_pending_decision().unwrap();
    client.submit(&nonce_1, ver_1, "tactical").unwrap();

    // 2. Activate border system where Letnev ships are located
    let initial_snapshot = session.get_snapshot(&ViewerRole::Player(p1.clone()));
    let letnev_id = initial_snapshot
        .view
        .players
        .iter()
        .find(|p| p.faction.as_str() == "letnev")
        .map(|p| p.id.clone())
        .expect("letnev player");
    let border_sys_id = initial_snapshot
        .view
        .board
        .systems
        .values()
        .find(|s| {
            s.system_id.as_str() != "10"
                && s.system_id.as_str() != "01"
                && s.units.iter().any(|u| u.owner == letnev_id)
        })
        .map(|s| s.system_id.clone())
        .expect("border system with Letnev units");

    let choice = wait_for_choice(&client).expect("activate choice");
    assert_eq!(choice.player, p1);
    let activate_opt = choice
        .options
        .iter()
        .find(|o| o.id == border_sys_id.as_str())
        .expect("activate border option");
    let (_, nonce_2, ver_2) = session.current_pending_decision().unwrap();
    client.submit(&nonce_2, ver_2, &activate_opt.id).unwrap();

    // 3. Move ships to trigger space combat
    let mut choice = wait_for_choice(&client).expect("movement choice");
    while !choice
        .options
        .iter()
        .any(|o| o.kind == "retreat" || o.kind == "sustain" || o.kind == "casualty")
    {
        let (_, nonce, ver) = session.current_pending_decision().unwrap();
        if let Some(load_opt) = choice.options.iter().find(|o| o.id.starts_with("load|0")) {
            client.submit(&nonce, ver, &load_opt.id).unwrap();
        } else if let Some(done_load) = choice.options.iter().find(|o| o.id == "done_loading") {
            client.submit(&nonce, ver, &done_load.id).unwrap();
        } else if let Some(move_opt) = choice.options.iter().find(|o| o.id.starts_with("move|")) {
            client.submit(&nonce, ver, &move_opt.id).unwrap();
        } else if let Some(done_move) = choice.options.iter().find(|o| o.id == "done_moving") {
            client.submit(&nonce, ver, &done_move.id).unwrap();
        } else {
            panic!("unhandled choice in movement loop: {:?}", choice);
        }
        choice = wait_for_choice(&client).expect("next choice in combat setup");
    }

    let pre_crash_choice = choice;
    let pre_crash_decision = session.current_pending_decision().unwrap();
    let pre_crash_state = session.current_state();
    let pre_crash_hashes = session.decision_hashes();
    let pre_crash_decisions = session.decision_log();
    let pre_crash_seats = session.restart_config().seats;

    // Verify bots are SeatController::BotFirstOption
    let bot_seats: Vec<_> = pre_crash_seats
        .iter()
        .filter(|(id, _)| **id != p1)
        .map(|(_, controller)| controller)
        .collect();
    assert_eq!(bot_seats.len(), 2);
    for controller in bot_seats {
        assert_eq!(*controller, SeatController::BotFirstOption);
    }

    // Simulate server crash / restart: drop registry, session, client
    drop(client);
    drop(session);
    drop(registry);

    // Restart with fresh registry pointing to same store
    let restarted = Arc::new(GameRegistry::new().with_store(store.clone()));
    let report = restarted
        .recover_all_games_report()
        .expect("recover all games report");
    assert_eq!(report.recovered, vec![launch.game_id.clone()]);
    assert!(report.failed.is_empty());

    // Public lobby and player session access work
    let (public_view, public_player) = restarted
        .player_lobby_status(&launch.game_id, None)
        .expect("public lobby view");
    assert!(public_player.is_none());
    assert_eq!(public_view.phase, LobbyPhase::Running);

    let (player_view, authenticated_player) = restarted
        .player_lobby_status(&launch.game_id, Some(&launch.player_session))
        .expect("player lobby view");
    assert_eq!(authenticated_player, Some(p1.clone()));
    assert_eq!(player_view.phase, LobbyPhase::Running);

    // Recovered game session state matches
    let recovered_session = restarted
        .get_game(&launch.game_id)
        .expect("recovered session");
    recovered_session.wait_replayed().expect("wait replayed");
    assert_eq!(recovered_session.current_state(), pre_crash_state);
    assert_eq!(recovered_session.decision_hashes(), pre_crash_hashes);
    assert_eq!(recovered_session.decision_log(), pre_crash_decisions);
    let recovered_decision = recovered_session.current_pending_decision().unwrap();
    assert_eq!(recovered_decision.0, pre_crash_decision.0); // seat

    // Bots retain their controllers after recovery
    let recovered_seats = recovered_session.restart_config().seats;
    for (id, controller) in recovered_seats {
        if id == p1 {
            assert_eq!(controller, SeatController::Human);
        } else {
            assert_eq!(
                controller,
                SeatController::BotFirstOption,
                "bot {id} must retain BotFirstOption controller"
            );
        }
    }

    // Play continues after restart: human submits combat choice, and bots respond automatically
    let recovered_client =
        MockClient::connect(recovered_session.clone(), ViewerRole::Player(p1.clone()));
    let rec_snapshot = recovered_client.snapshot();
    let pending = rec_snapshot
        .pending_choice
        .expect("snapshot pending choice");
    assert_eq!(pending.choice.prompt, pre_crash_choice.prompt);
    assert_eq!(pending.choice.options.len(), pre_crash_choice.options.len());

    let (_, nonce_c, ver_c) = recovered_session.current_pending_decision().unwrap();
    let opt_id = &pending.choice.options[0].id;
    recovered_client.submit(&nonce_c, ver_c, opt_id).unwrap();

    // Verify subsequent choice arrives (bots answered automated choices)
    let next_choice = wait_for_choice(&recovered_client).expect("next choice after bot response");
    assert_eq!(next_choice.player, p1);

    // Verify new decision is persisted
    let decisions_path = store
        .game_dir(&launch.game_id)
        .unwrap()
        .join("decisions.jsonl");
    assert!(decisions_path.exists());
    let decisions_content = fs::read_to_string(&decisions_path).unwrap();
    assert!(decisions_content.lines().count() >= 4);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn scripted_ongoing_combat_and_four_view_presets_recover_cleanly() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_dev_rec_scripted_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).expect("create store"));
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));

    // 1. Launch scripted ongoing_combat (has bots)
    let combat_launch = execute_launch_scenario(&registry, "ongoing_combat", Some(54321))
        .expect("launch ongoing_combat");
    let combat_session = registry.get_game(&combat_launch.game_id).unwrap();
    let combat_version = combat_session.game_version();
    let combat_state = combat_session.current_state();
    let combat_hashes = combat_session.decision_hashes();
    let combat_decisions = combat_session.decision_log();
    assert!(combat_version > 1, "scripted decisions were executed");

    // 2. Launch scripted ongoing_combat_four_views (all human)
    let four_view_launch =
        execute_launch_scenario(&registry, "ongoing_combat_four_views", Some(54322))
            .expect("launch ongoing_combat_four_views");
    let four_view_session = registry.get_game(&four_view_launch.game_id).unwrap();
    assert!(
        four_view_session
            .restart_config()
            .seats
            .values()
            .all(|c| *c == SeatController::Human)
    );

    // 3. Ordinary player lobby as compatibility case
    let (_, host_id, host_token) = registry
        .create_player_lobby("player_lobby_compat".into(), 2, 777, "Host")
        .expect("create player lobby");
    let (_, guest_id, guest_token) = registry
        .join_player_lobby("player_lobby_compat", None, Some("Guest"))
        .expect("join player lobby");
    registry
        .set_player_ready("player_lobby_compat", host_token.as_str(), true)
        .unwrap();
    registry
        .set_player_ready(
            "player_lobby_compat",
            guest_token.as_ref().unwrap().as_str(),
            true,
        )
        .unwrap();
    registry
        .start_player_lobby("player_lobby_compat", host_token.as_str())
        .unwrap();

    // Drop all in-memory references and restart
    drop(combat_session);
    drop(four_view_session);
    drop(registry);

    let restarted = Arc::new(GameRegistry::new().with_store(store.clone()));
    let report = restarted.recover_all_games_report().unwrap();
    assert_eq!(report.recovered.len(), 3);
    assert!(report.failed.is_empty());
    assert!(report.recovered.contains(&combat_launch.game_id));
    assert!(report.recovered.contains(&four_view_launch.game_id));
    assert!(
        report
            .recovered
            .contains(&"player_lobby_compat".to_string())
    );

    // Verify ongoing_combat recovered bot controllers and replayed scripted choices
    let rec_combat = restarted.get_game(&combat_launch.game_id).unwrap();
    rec_combat.wait_replayed().expect("wait replayed combat");
    assert_eq!(rec_combat.current_state(), combat_state);
    assert_eq!(rec_combat.decision_hashes(), combat_hashes);
    assert_eq!(rec_combat.decision_log(), combat_decisions);
    let p_combat = PlayerId::new(&combat_launch.player_id);
    for (id, controller) in rec_combat.restart_config().seats {
        if id == p_combat {
            assert_eq!(controller, SeatController::Human);
        } else {
            assert_eq!(controller, SeatController::BotFirstOption);
        }
    }

    // Verify four_views recovered all seats as Human
    let rec_four_view = restarted.get_game(&four_view_launch.game_id).unwrap();
    rec_four_view
        .wait_replayed()
        .expect("wait replayed four_view");
    for (_id, controller) in rec_four_view.restart_config().seats {
        assert_eq!(
            controller,
            SeatController::Human,
            "four-views must retain all-human seats"
        );
    }

    // Verify player_lobby_compat recovered
    let rec_lobby = restarted.get_game("player_lobby_compat").unwrap();
    rec_lobby.wait_replayed().expect("wait replayed lobby");
    let rec_lobby_seats = rec_lobby.restart_config().seats;
    assert_eq!(rec_lobby_seats.len(), 2);
    assert_eq!(rec_lobby_seats[&host_id], SeatController::Human);
    assert_eq!(rec_lobby_seats[&guest_id], SeatController::Human);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn launch_failure_cleanup_and_backwards_compatibility() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_dev_rec_failure_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).expect("create store"));
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));

    // 1. Establish an unrelated, valid game
    let valid_launch = execute_launch_scenario(&registry, "tactical_action", Some(111))
        .expect("launch tactical_action");

    // 2. Test generated-ID collision semantics: if directory already exists, launch must error
    // and must NEVER delete or mutate the existing directory.
    let collision_dir = store.game_dir("dev_collision_game").unwrap();
    fs::create_dir_all(&collision_dir).unwrap();
    let canary_file = collision_dir.join("canary.txt");
    fs::write(&canary_file, "do not touch").unwrap();

    let (lobby_record, host, _) =
        PlayerLobbyRecord::create("dev_collision_game".into(), 1, 222, "Host").unwrap();
    let config = ti4_server::session::SessionConfig::new(
        "dev_collision_game",
        ti4_server::fixtures::create_sample_game(),
    )
    .with_player_ids(vec![host.clone()])
    .with_seat(host, SeatController::Human);

    let collision_result = registry.launch_dev_scenario(config, lobby_record);
    assert!(
        matches!(&collision_result, Err(msg) if msg.contains("already exists")),
        "launch must fail on existing directory"
    );
    assert_eq!(
        fs::read_to_string(&canary_file).unwrap(),
        "do not touch",
        "existing files must not be touched on collision"
    );

    // 3. Test backwards compatibility: old player init records without "seats" field
    // deserialized and recover with all-human seats
    let old_game_id = "old_player_game";
    let old_dir = store.game_dir(old_game_id).unwrap();
    fs::create_dir_all(&old_dir).unwrap();

    let (mut old_lobby, old_host, old_session) =
        PlayerLobbyRecord::create(old_game_id.into(), 1, 999, "OldHost").unwrap();
    old_lobby.phase = ti4_server::storage::PersistedLobbyPhase::Running;
    store.save_player_lobby(&old_lobby).unwrap();

    let old_sessions = PlayerSessionsRecord {
        schema_version: PLAYER_RECORD_VERSION,
        game_id: old_game_id.to_string(),
        sessions: BTreeMap::from([(old_host.clone(), old_session)]),
        nicknames: BTreeMap::from([(old_host.clone(), "OldHost".to_string())]),
    };
    store.save_player_sessions(&old_sessions).unwrap();

    // Write init.json using a struct where `seats` is None (omitted in JSON serialization)
    let old_init = PlayerGameInitRecord {
        schema_version: PLAYER_RECORD_VERSION,
        game_id: old_game_id.to_string(),
        seed: 999,
        player_ids: vec![old_host.clone()],
        initial_state: ti4_server::fixtures::create_sample_game(),
        map_tiles: Vec::new(),
        seats: None, // Will not be serialized in JSON due to skip_serializing_if
    };
    store.save_player_init(&old_init).unwrap();

    // Verify the serialized init.json payload has no "seats" key
    let init_content = fs::read_to_string(old_dir.join("init.json")).unwrap();
    assert!(
        !init_content.contains("\"seats\""),
        "legacy init.json should not contain 'seats' key"
    );

    // 4. Simulate a failed/torn write in a broken directory
    let broken_dir = store.game_dir("broken_launch").unwrap();
    fs::create_dir_all(&broken_dir).unwrap();
    // Only player_sessions.json was written (e.g. crash before init.json)
    let broken_sessions = PlayerSessionsRecord {
        schema_version: PLAYER_RECORD_VERSION,
        game_id: "broken_launch".to_string(),
        sessions: BTreeMap::from([(
            old_host.clone(),
            ti4_server::storage::PlayerSession::generate(),
        )]),
        nicknames: BTreeMap::from([(old_host.clone(), "Broken".to_string())]),
    };
    store.save_player_sessions(&broken_sessions).unwrap();

    // Drop and restart
    drop(registry);
    let restarted = Arc::new(GameRegistry::new().with_store(store.clone()));
    let report = restarted.recover_all_games_report().unwrap();

    // The valid launch and the legacy game are recovered
    assert!(report.recovered.contains(&valid_launch.game_id));
    assert!(report.recovered.contains(&old_game_id.to_string()));
    // The broken directory (only player_sessions.json) is not published as a phantom lobby
    assert!(!report.recovered.contains(&"broken_launch".to_string()));
    assert!(!restarted.contains_game("broken_launch"));

    // Legacy game seats default to Human
    let rec_old = restarted.get_game(old_game_id).unwrap();
    assert_eq!(
        rec_old.restart_config().seats[&old_host],
        SeatController::Human
    );

    let _ = fs::remove_dir_all(dir);
}
