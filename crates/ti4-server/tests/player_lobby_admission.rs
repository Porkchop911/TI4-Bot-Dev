use std::sync::{Arc, Barrier};
use std::time::Duration;

use ti4_server::session::GameRegistry;
use ti4_server::session::registry::LobbyError;
use ti4_server::storage::FileGameStore;
use ti4_server::storage::LobbySlotId;

fn slot(id: &str) -> LobbySlotId {
    LobbySlotId(id.to_owned())
}

#[test]
fn a_bad_saved_lobby_does_not_hide_a_recoverable_running_game() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_recovery_isolation_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let original = GameRegistry::new().with_store(store.clone());
    original
        .create_player_lobby("a_bad".into(), 2, 1, "Bad")
        .unwrap();
    let (_, host, token) = original
        .create_player_lobby("z_good".into(), 2, 19, "Host")
        .unwrap();
    let (_, _, guest_token) = original
        .join_player_lobby("z_good", None, Some("Guest"))
        .unwrap();
    original
        .set_player_ready("z_good", token.as_str(), true)
        .unwrap();
    original
        .set_player_ready("z_good", guest_token.unwrap().as_str(), true)
        .unwrap();
    original
        .start_player_lobby("z_good", token.as_str())
        .unwrap();

    let bad_path = store.game_dir("a_bad").unwrap().join("lobby.json");
    let bytes = std::fs::read_to_string(&bad_path).unwrap();
    std::fs::write(
        &bad_path,
        bytes.replacen("\"checksum\": \"", "\"checksum\": \"0", 1),
    )
    .unwrap();

    let restarted = GameRegistry::new().with_store(store);
    let report = restarted.recover_all_games_report().unwrap();
    assert_eq!(report.recovered, ["z_good"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].game_id, "a_bad");
    assert_eq!(report.failed[0].stage, "lobby.json");
    assert!(
        report.failed[0]
            .error
            .to_string()
            .contains("checksum mismatch")
    );
    assert_eq!(
        std::fs::read_to_string(&bad_path).unwrap(),
        bytes.replacen("\"checksum\": \"", "\"checksum\": \"0", 1)
    );
    assert!(restarted.player_lobby_status("a_bad", None).is_err());
    assert_eq!(restarted.list_games()[0].game_id, "z_good");
    assert_eq!(
        restarted
            .join_player_lobby("z_good", Some(token.as_str()), None)
            .unwrap()
            .1,
        host
    );
    assert!(restarted.get_game("z_good").is_some());

    restarted.remove_game("z_good");
    original.remove_game("z_good");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_running_recovery_does_not_publish_a_phantom_lobby() {
    let dir = std::env::temp_dir().join(format!(
        "ti4_recovery_atomic_{:032x}",
        rand::random::<u128>()
    ));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let original = GameRegistry::new().with_store(store.clone());
    let (_, _, host_token) = original
        .create_player_lobby("a_running".into(), 2, 19, "Host")
        .unwrap();
    let (_, _, guest_token) = original
        .join_player_lobby("a_running", None, Some("Guest"))
        .unwrap();
    original
        .set_player_ready("a_running", host_token.as_str(), true)
        .unwrap();
    original
        .set_player_ready("a_running", guest_token.unwrap().as_str(), true)
        .unwrap();
    original
        .start_player_lobby("a_running", host_token.as_str())
        .unwrap();
    let (_, _, waiting_token) = original
        .create_player_lobby("z_waiting".into(), 2, 21, "Waiting")
        .unwrap();

    let init_path = store.game_dir("a_running").unwrap().join("init.json");
    let bytes = std::fs::read_to_string(&init_path).unwrap();
    std::fs::write(
        &init_path,
        bytes.replacen("\"checksum\": \"", "\"checksum\": \"0", 1),
    )
    .unwrap();
    let restarted = GameRegistry::new().with_store(store);
    let report = restarted.recover_all_games_report().unwrap();
    assert_eq!(report.recovered, ["z_waiting"]);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].game_id, "a_running");
    assert_eq!(report.failed[0].stage, "init.json");
    assert!(restarted.player_lobby_status("a_running", None).is_err());
    assert!(restarted.get_game("a_running").is_none());
    assert!(
        restarted
            .join_player_lobby("z_waiting", Some(waiting_token.as_str()), None)
            .is_ok()
    );

    original.remove_game("a_running");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn nicknames_follow_identity_through_reorder_and_running_takeover_recovery() {
    let dir = std::env::temp_dir().join(format!("ti4_pil09_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new()
        .with_store(store.clone())
        .with_presence_grace(Duration::from_millis(10));
    let (_, host, host_token) = registry
        .create_player_lobby("names".into(), 2, 42, "Élodie")
        .unwrap();
    let (joined, guest, guest_token) = registry
        .join_player_lobby("names", None, Some("Élodie"))
        .unwrap();
    let guest_token = guest_token.unwrap();
    assert_eq!(joined.slots[1].nickname.as_deref(), Some("Élodie"));
    assert!(matches!(
        registry.join_player_lobby("names", Some(guest_token.as_str()), Some("Other")),
        Err(LobbyError::InvalidNickname)
    ));
    registry
        .reorder_player_lobby(
            "names",
            host_token.as_str(),
            &[slot("slot_2"), slot("slot_1")],
        )
        .unwrap();
    let moved = registry.player_lobby_status("names", None).unwrap().0;
    assert_eq!(moved.slots[0].occupant.as_ref(), Some(&guest));
    assert_eq!(moved.slots[1].occupant.as_ref(), Some(&host));
    for token in [&host_token, &guest_token] {
        registry
            .set_player_ready("names", token.as_str(), true)
            .unwrap();
    }
    registry
        .start_player_lobby("names", host_token.as_str())
        .unwrap();
    std::thread::sleep(Duration::from_millis(15));
    let (taken, new_token) = registry
        .take_over_player("names", &guest, "新しい名前")
        .unwrap();
    assert_eq!(taken.slots[0].nickname.as_deref(), Some("新しい名前"));
    assert_eq!(taken.slots[1].nickname.as_deref(), Some("Élodie"));
    let stale = store.load_player_lobby("names").unwrap().unwrap();
    assert_eq!(stale.players[&guest].nickname, "Élodie");
    let recovered = GameRegistry::new().with_store(store.clone());
    recovered.recover_all_games().unwrap();
    assert_eq!(
        recovered
            .player_lobby_status("names", None)
            .unwrap()
            .0
            .slots[0]
            .nickname
            .as_deref(),
        Some("新しい名前")
    );
    assert_eq!(
        recovered
            .authenticate_player_session("names", new_token.as_str())
            .unwrap(),
        guest
    );
    assert!(matches!(
        recovered.authenticate_player_session("names", guest_token.as_str()),
        Err(LobbyError::InvalidCapability)
    ));
    assert_eq!(
        store.load_player_sessions("names").unwrap().nicknames[&host],
        "Élodie"
    );
    recovered.remove_game("names");
    registry.remove_game("names");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_nickname_rotation_keeps_name_and_credential_in_lobby_and_running_game() {
    let dir =
        std::env::temp_dir().join(format!("ti4_pil09_failure_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new()
        .with_store(store.clone())
        .with_presence_grace(Duration::from_millis(10));
    let (_, host, token) = registry
        .create_player_lobby("names_failure".into(), 2, 42, "Original")
        .unwrap();
    let (_, _, other) = registry
        .join_player_lobby("names_failure", None, Some("Guest"))
        .unwrap();
    let other = other.unwrap();
    std::thread::sleep(Duration::from_millis(15));
    let dir_path = store.game_dir("names_failure").unwrap();
    std::fs::create_dir(dir_path.join("lobby.tmp")).unwrap();
    assert!(matches!(
        registry.take_over_player("names_failure", &host, "Changed"),
        Err(LobbyError::Storage(_))
    ));
    assert_eq!(
        registry
            .player_lobby_status("names_failure", None)
            .unwrap()
            .0
            .slots[0]
            .nickname
            .as_deref(),
        Some("Original")
    );
    assert_eq!(
        registry
            .authenticate_player_session("names_failure", token.as_str())
            .unwrap(),
        host
    );
    std::fs::remove_dir(dir_path.join("lobby.tmp")).unwrap();
    for credential in [&token, &other] {
        registry
            .set_player_ready("names_failure", credential.as_str(), true)
            .unwrap();
    }
    registry
        .start_player_lobby("names_failure", token.as_str())
        .unwrap();
    std::fs::create_dir(dir_path.join("player_sessions.tmp")).unwrap();
    assert!(matches!(
        registry.take_over_player("names_failure", &host, "Changed"),
        Err(LobbyError::Storage(_))
    ));
    assert_eq!(
        registry
            .player_lobby_status("names_failure", None)
            .unwrap()
            .0
            .slots[0]
            .nickname
            .as_deref(),
        Some("Original")
    );
    assert_eq!(
        store
            .load_player_sessions("names_failure")
            .unwrap()
            .nicknames[&host],
        "Original"
    );
    assert_eq!(
        registry
            .authenticate_player_session("names_failure", token.as_str())
            .unwrap(),
        host
    );
    std::fs::remove_dir(dir_path.join("player_sessions.tmp")).unwrap();
    registry.remove_game("names_failure");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reorder_moves_empty_slots_and_preserves_host_readiness_and_credentials() {
    let registry = Arc::new(GameRegistry::new());
    let (_, host, host_token) = registry
        .create_player_lobby("order".into(), 3, 77, "Host")
        .unwrap();
    let (_, other, other_token) = registry
        .join_player_lobby("order", None, Some("Guest"))
        .unwrap();
    let other_token = other_token.unwrap();
    registry
        .set_player_ready("order", other_token.as_str(), true)
        .unwrap();
    let before = registry.player_lobby_status("order", None).unwrap().0;
    for invalid in [
        vec![slot("slot_1"), slot("slot_2")],
        vec![slot("slot_1"), slot("slot_1"), slot("slot_3")],
        vec![slot("slot_1"), slot("slot_2"), slot("slot_99")],
    ] {
        assert!(matches!(
            registry.reorder_player_lobby("order", host_token.as_str(), &invalid),
            Err(LobbyError::InvalidSlotOrder)
        ));
        assert_eq!(
            registry
                .player_lobby_status("order", None)
                .unwrap()
                .0
                .lobby_version,
            before.lobby_version
        );
    }
    assert!(matches!(
        registry.reorder_player_lobby(
            "order",
            other_token.as_str(),
            &[slot("slot_3"), slot("slot_2"), slot("slot_1")]
        ),
        Err(LobbyError::HostRequired)
    ));
    let moved = registry
        .reorder_player_lobby(
            "order",
            host_token.as_str(),
            &[slot("slot_3"), slot("slot_2"), slot("slot_1")],
        )
        .unwrap();
    assert_eq!(moved.lobby_version, before.lobby_version + 1);
    assert_eq!(moved.host_player_id, host);
    assert_eq!(moved.slots[0].slot_id, slot("slot_3"));
    assert!(moved.slots[0].occupant.is_none());
    assert_eq!(moved.slots[1].occupant.as_ref(), Some(&other));
    assert!(moved.slots[1].ready);
    assert_eq!(moved.slots[2].occupant.as_ref(), Some(&host));
    let (joined, third, _) = registry
        .join_player_lobby("order", None, Some("Third"))
        .unwrap();
    assert_eq!(joined.slots[0].occupant.as_ref(), Some(&third));
    assert_eq!(
        registry
            .join_player_lobby("order", Some(host_token.as_str()), None)
            .unwrap()
            .1,
        host
    );
    assert_eq!(
        registry
            .join_player_lobby("order", Some(other_token.as_str()), None)
            .unwrap()
            .1,
        other
    );
}

#[test]
fn join_and_reorder_serialize_under_registry_lock() {
    let registry = Arc::new(GameRegistry::new());
    let (_, _, token) = registry
        .create_player_lobby("race".into(), 3, 77, "Host")
        .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let join = {
        let (registry, barrier) = (registry.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            registry
                .join_player_lobby("race", None, Some("Guest"))
                .unwrap()
        })
    };
    let reorder = {
        let (registry, barrier) = (registry.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            registry
                .reorder_player_lobby(
                    "race",
                    token.as_str(),
                    &[slot("slot_3"), slot("slot_1"), slot("slot_2")],
                )
                .unwrap()
        })
    };
    barrier.wait();
    let (join_view, player, _) = join.join().unwrap();
    let reorder_view = reorder.join().unwrap();
    let final_view = registry.player_lobby_status("race", None).unwrap().0;
    assert_eq!(final_view.lobby_version, 3);
    assert_eq!(
        final_view
            .slots
            .iter()
            .filter(|s| s.occupant.is_some())
            .count(),
        2
    );
    assert_eq!(
        final_view
            .slots
            .iter()
            .filter(|s| s.occupant.as_ref() == Some(&player))
            .count(),
        1
    );
    if join_view.lobby_version == 3 {
        assert_eq!(join_view.slots[0].occupant.as_ref(), Some(&player));
    } else {
        assert_eq!(reorder_view.lobby_version, 3);
        assert_eq!(final_view.slots[2].occupant.as_ref(), Some(&player));
    }
}

#[test]
fn start_commit_recovers_exact_order_and_current_rotated_sessions() {
    let dir = std::env::temp_dir().join(format!("ti4_pil05_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new()
        .with_store(store.clone())
        .with_presence_grace(Duration::from_millis(10));
    let (_, host, token) = registry
        .create_player_lobby("commit".into(), 2, 99, "Host")
        .unwrap();
    let (_, other, other_token) = registry
        .join_player_lobby("commit", None, Some("Guest"))
        .unwrap();
    let other_token = other_token.unwrap();
    registry
        .reorder_player_lobby("commit", token.as_str(), &[slot("slot_2"), slot("slot_1")])
        .unwrap();
    registry
        .set_player_ready("commit", token.as_str(), true)
        .unwrap();
    assert!(matches!(
        registry.start_player_lobby("commit", token.as_str()),
        Err(LobbyError::HumansNotReady)
    ));
    registry
        .set_player_ready("commit", other_token.as_str(), true)
        .unwrap();
    let obstruction = store.game_dir("commit").unwrap().join("init.tmp");
    std::fs::create_dir(&obstruction).unwrap();
    assert!(matches!(
        registry.start_player_lobby("commit", token.as_str()),
        Err(LobbyError::Storage(_))
    ));
    // Simulate power loss after the Running lobby file but before the init
    // record was committed, regardless of a best-effort rollback on error.
    let mut pre_init = store.load_player_lobby("commit").unwrap().unwrap();
    pre_init.phase = ti4_server::storage::PersistedLobbyPhase::Running;
    store.save_player_lobby(&pre_init).unwrap();
    let pre_commit = GameRegistry::new().with_store(store.clone());
    pre_commit.recover_all_games().unwrap();
    assert!(pre_commit.get_game("commit").is_none());
    assert_eq!(
        pre_commit
            .player_lobby_status("commit", None)
            .unwrap()
            .0
            .phase,
        ti4_server::session::registry::LobbyPhase::Lobby
    );
    std::fs::remove_dir(&obstruction).unwrap();
    let running = registry
        .start_player_lobby("commit", token.as_str())
        .unwrap();
    assert_eq!(
        running.phase,
        ti4_server::session::registry::LobbyPhase::Running
    );
    assert_eq!(
        store.load_player_init("commit").unwrap().player_ids,
        vec![other.clone(), host.clone()]
    );
    assert_eq!(
        store
            .load_player_init("commit")
            .unwrap()
            .initial_state
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect::<Vec<_>>(),
        vec![other.clone(), host.clone()]
    );
    assert!(matches!(
        registry.reorder_player_lobby("commit", token.as_str(), &[slot("slot_1"), slot("slot_2")]),
        Err(LobbyError::AlreadyRunning)
    ));
    assert!(matches!(
        registry.join_player_lobby("commit", None, Some("Third")),
        Err(LobbyError::AlreadyRunning)
    ));
    std::thread::sleep(Duration::from_millis(15));
    let (_, rotated) = registry
        .take_over_player("commit", &other, "New Guest")
        .unwrap();
    // Simulate the post-init crash boundary with an older lobby on disk: init
    // commits the game and the separate current-session record commits rotation.
    let mut stale_lobby = store.load_player_lobby("commit").unwrap().unwrap();
    stale_lobby.phase = ti4_server::storage::PersistedLobbyPhase::Lobby;
    store.save_player_lobby(&stale_lobby).unwrap();
    let recovered = GameRegistry::new().with_store(store.clone());
    recovered.recover_all_games().unwrap();
    assert!(recovered.get_game("commit").is_some());
    assert_eq!(
        recovered
            .player_lobby_status("commit", None)
            .unwrap()
            .0
            .phase,
        ti4_server::session::registry::LobbyPhase::Running
    );
    assert_eq!(
        recovered
            .authenticate_player_session("commit", rotated.as_str())
            .unwrap(),
        other
    );
    assert!(matches!(
        recovered.authenticate_player_session("commit", other_token.as_str()),
        Err(LobbyError::InvalidCapability)
    ));
    assert_eq!(
        recovered
            .authenticate_player_session("commit", token.as_str())
            .unwrap(),
        host
    );
    recovered.remove_game("commit");
    pre_commit.remove_game("commit");
    registry.remove_game("commit");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reorder_storage_failure_leaves_lobby_unchanged() {
    let dir =
        std::env::temp_dir().join(format!("ti4_pil05_failure_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let (before, _, token) = registry
        .create_player_lobby("failure_order".into(), 2, 1, "Host")
        .unwrap();
    let obstruction = store.game_dir("failure_order").unwrap().join("lobby.tmp");
    std::fs::create_dir(&obstruction).unwrap();
    assert!(matches!(
        registry.reorder_player_lobby(
            "failure_order",
            token.as_str(),
            &[slot("slot_2"), slot("slot_1")]
        ),
        Err(LobbyError::Storage(_))
    ));
    let after = registry
        .player_lobby_status("failure_order", None)
        .unwrap()
        .0;
    assert_eq!(after.lobby_version, before.lobby_version);
    assert_eq!(after.slots[0].slot_id, before.slots[0].slot_id);
    assert_eq!(
        store
            .load_player_lobby("failure_order")
            .unwrap()
            .unwrap()
            .slots[0]
            .slot_id,
        slot("slot_1")
    );
    std::fs::remove_dir(&obstruction).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn http_reorder_requires_host_and_strict_slot_ids() {
    let registry = Arc::new(GameRegistry::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(axum::serve(listener, ti4_server::create_app(registry)).into_future());
    let client = reqwest::Client::new();
    let base = format!("http://{addr}/api/games");
    let created: serde_json::Value = client
        .post(&base)
        .json(&serde_json::json!({"player_count": 2, "nickname": "Host"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let url = format!("{base}/{}/lobby", created["game_id"].as_str().unwrap());
    let host = created["player_session"].as_str().unwrap();
    let joined: serde_json::Value = client
        .post(format!("{url}/join"))
        .json(&serde_json::json!({"kind":"new", "nickname":"Guest"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let other = joined["player_session"].as_str().unwrap();
    for (auth, body, status) in [
        (
            None,
            serde_json::json!({"slot_ids":["slot_2","slot_1"]}),
            reqwest::StatusCode::FORBIDDEN,
        ),
        (
            Some(other),
            serde_json::json!({"slot_ids":["slot_2","slot_1"]}),
            reqwest::StatusCode::FORBIDDEN,
        ),
        (
            Some(host),
            serde_json::json!({"slot_ids":["slot_2","slot_2"]}),
            reqwest::StatusCode::BAD_REQUEST,
        ),
        (
            Some(host),
            serde_json::json!({"slot_ids":["slot_2","slot_1"],"player_id":"x"}),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let mut request = client.post(format!("{url}/reorder")).json(&body);
        if let Some(auth) = auth {
            request = request.header("x-ti4-player-session", auth);
        }
        assert_eq!(request.send().await.unwrap().status(), status);
    }
    let moved: serde_json::Value = client
        .post(format!("{url}/reorder"))
        .header("x-ti4-player-session", host)
        .json(&serde_json::json!({"slot_ids":["slot_2","slot_1"]}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(moved["slots"][0]["occupant"], joined["player"]["id"]);
    assert_eq!(moved["host_player_id"], created["player"]["id"]);
    assert!(!moved.to_string().contains(host));
    server.abort();
}

#[test]
fn takeover_requires_absence_rotates_once_and_preserves_lobby_identity() {
    let registry = Arc::new(GameRegistry::new().with_presence_grace(Duration::from_millis(40)));
    let (_, host, host_token) = registry
        .create_player_lobby("takeover".into(), 2, 9, "Host")
        .unwrap();
    let (_, other, old) = registry
        .join_player_lobby("takeover", None, Some("Guest"))
        .unwrap();
    let old = old.unwrap();
    registry
        .set_player_ready("takeover", old.as_str(), true)
        .unwrap();
    let (_, connection) = registry.connect_player("takeover", old.as_str()).unwrap();
    assert!(matches!(
        registry.take_over_player("takeover", &other, "New Guest"),
        Err(LobbyError::TakeoverUnavailable)
    ));
    registry.disconnect_player("takeover", &other, connection);
    std::thread::sleep(Duration::from_millis(55));
    let barrier = Arc::new(Barrier::new(3));
    let attempts: Vec<_> = (0..2)
        .map(|_| {
            let registry = registry.clone();
            let barrier = barrier.clone();
            let other = other.clone();
            std::thread::spawn(move || {
                barrier.wait();
                registry.take_over_player("takeover", &other, "New Guest")
            })
        })
        .collect();
    barrier.wait();
    let outcomes: Vec<_> = attempts.into_iter().map(|t| t.join().unwrap()).collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(
        outcomes
            .iter()
            .any(|r| matches!(r, Err(LobbyError::TakeoverUnavailable)))
    );
    let (view, new) = outcomes.into_iter().find_map(Result::ok).unwrap();
    assert_eq!(view.slots[0].occupant.as_ref(), Some(&host));
    assert_eq!(view.slots[1].occupant.as_ref(), Some(&other));
    assert!(view.slots[1].ready);
    assert_eq!(view.host_player_id, host);
    assert_ne!(new, old);
    assert!(matches!(
        registry.player_heartbeat("takeover", old.as_str()),
        Err(LobbyError::InvalidCapability)
    ));
    assert!(matches!(
        registry.ping_player("takeover", new.as_str(), connection),
        Err(LobbyError::InvalidCapability)
    ));
    registry.disconnect_player("takeover", &other, connection);
    assert!(!registry.player_disconnected("takeover", &other));
    assert_eq!(
        registry
            .join_player_lobby("takeover", Some(new.as_str()), None)
            .unwrap()
            .1,
        other
    );
    assert_eq!(
        registry
            .join_player_lobby("takeover", Some(host_token.as_str()), None)
            .unwrap()
            .1,
        host
    );
}

#[test]
fn takeover_persistence_failure_preserves_old_credential_and_restart_recovers_new_one() {
    let dir = std::env::temp_dir().join(format!("ti4_pil04_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new()
        .with_store(store.clone())
        .with_presence_grace(Duration::from_millis(20));
    let (_, host, old) = registry
        .create_player_lobby("rotate".into(), 2, 42, "Host")
        .unwrap();
    let (_, _, second) = registry
        .join_player_lobby("rotate", None, Some("Guest"))
        .unwrap();
    let second = second.unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let obstruction = store.game_dir("rotate").unwrap().join("lobby.tmp");
    std::fs::create_dir(&obstruction).unwrap();
    assert!(matches!(
        registry.take_over_player("rotate", &host, "New Host"),
        Err(LobbyError::Storage(_))
    ));
    assert_eq!(
        registry
            .authenticate_player_session("rotate", old.as_str())
            .unwrap(),
        host
    );
    std::fs::remove_dir(&obstruction).unwrap();
    let (_, new) = registry
        .take_over_player("rotate", &host, "New Host")
        .unwrap();
    let recovered = GameRegistry::new()
        .with_store(store.clone())
        .with_presence_grace(Duration::from_millis(20));
    recovered.recover_all_games().unwrap();
    assert!(matches!(
        recovered.authenticate_player_session("rotate", old.as_str()),
        Err(LobbyError::InvalidCapability)
    ));
    assert_eq!(
        recovered
            .authenticate_player_session("rotate", new.as_str())
            .unwrap(),
        host
    );
    assert_eq!(
        recovered
            .player_lobby_status("rotate", None)
            .unwrap()
            .0
            .slots[0]
            .nickname
            .as_deref(),
        Some("New Host")
    );
    assert!(!recovered.player_disconnected("rotate", &host));
    registry
        .set_player_ready("rotate", new.as_str(), true)
        .unwrap();
    registry
        .set_player_ready("rotate", second.as_str(), true)
        .unwrap();
    registry.start_player_lobby("rotate", new.as_str()).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let obstruction = store
        .game_dir("rotate")
        .unwrap()
        .join("player_sessions.tmp");
    std::fs::create_dir(&obstruction).unwrap();
    assert!(matches!(
        registry.take_over_player("rotate", &host, "Newest Host"),
        Err(LobbyError::Storage(_))
    ));
    assert_eq!(
        registry
            .authenticate_player_session("rotate", new.as_str())
            .unwrap(),
        host
    );
    std::fs::remove_dir(&obstruction).unwrap();
    let (_, latest) = registry
        .take_over_player("rotate", &host, "Newest Host")
        .unwrap();
    assert_eq!(
        registry
            .get_game("rotate")
            .unwrap()
            .viewer_for_seat_token(latest.as_str()),
        Some(ti4_server::protocol::status::ViewerRole::Player(
            host.clone()
        ))
    );
    assert!(
        registry
            .get_game("rotate")
            .unwrap()
            .viewer_for_seat_token(new.as_str())
            .is_none()
    );
    let post_start = GameRegistry::new().with_store(store.clone());
    post_start.recover_all_games().unwrap();
    assert!(matches!(
        post_start.authenticate_player_session("rotate", new.as_str()),
        Err(LobbyError::InvalidCapability)
    ));
    assert_eq!(
        post_start
            .authenticate_player_session("rotate", latest.as_str())
            .unwrap(),
        host
    );
    assert_eq!(
        post_start
            .get_game("rotate")
            .unwrap()
            .viewer_for_seat_token(latest.as_str()),
        Some(ti4_server::protocol::status::ViewerRole::Player(host))
    );
    post_start.remove_game("rotate");
    registry.remove_game("rotate");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn concurrent_joins_resume_and_leave_preserve_stable_identities() {
    let registry = Arc::new(GameRegistry::new());
    let (initial, host, credential) = registry
        .create_player_lobby("players".into(), 3, 42, "Host")
        .unwrap();
    assert_eq!(initial.slots[0].occupant.as_ref(), Some(&host));
    assert!(initial.slots[1].occupant.is_none());
    let barrier = Arc::new(Barrier::new(3));
    let joins: Vec<_> = (0..2)
        .map(|_| {
            let registry = registry.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                registry
                    .join_player_lobby("players", None, Some("Guest"))
                    .unwrap()
            })
        })
        .collect();
    barrier.wait();
    let joined: Vec<_> = joins
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_ne!(joined[0].1, joined[1].1);
    let (view, _) = registry.player_lobby_status("players", None).unwrap();
    assert_eq!(
        view.slots[1].occupant.as_ref().unwrap(),
        &joined
            .iter()
            .find(|entry| entry.0.lobby_version == 2)
            .unwrap()
            .1
    );
    assert_eq!(
        view.slots[2].occupant.as_ref().unwrap(),
        &joined
            .iter()
            .find(|entry| entry.0.lobby_version == 3)
            .unwrap()
            .1
    );
    assert!(matches!(
        registry.join_player_lobby("players", None, Some("Guest")),
        Err(LobbyError::SeatUnavailable)
    ));
    let token = joined[0].2.as_ref().unwrap().as_str();
    let (same, player, new_token) = registry
        .join_player_lobby("players", Some(token), None)
        .unwrap();
    assert_eq!(player, joined[0].1);
    assert!(new_token.is_none());
    assert_eq!(same.lobby_version, 3);
    assert!(matches!(
        registry.join_player_lobby("players", Some("invalid"), None),
        Err(LobbyError::InvalidCapability)
    ));
    assert!(matches!(
        registry.leave_player_lobby("players", credential.as_str()),
        Err(LobbyError::HostRequired)
    ));
    let left = registry.leave_player_lobby("players", token).unwrap();
    assert_eq!(left.lobby_version, 4);
    assert!(matches!(
        registry.join_player_lobby("players", Some(token), None),
        Err(LobbyError::InvalidCapability)
    ));
    let (_, replacement, _) = registry
        .join_player_lobby("players", None, Some("New Guest"))
        .unwrap();
    assert_ne!(replacement, joined[0].1);
}

#[test]
fn readiness_start_and_recovery_preserve_current_credentials() {
    let dir = std::env::temp_dir().join(format!("ti4_pil02_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let (_, host, token) = registry
        .create_player_lobby("durable".into(), 2, 19, "Host")
        .unwrap();
    let (_, other, other_token) = registry
        .join_player_lobby("durable", None, Some("Guest"))
        .unwrap();
    let other_token = other_token.unwrap();
    assert!(matches!(
        registry.start_player_lobby("durable", token.as_str()),
        Err(LobbyError::HumansNotReady)
    ));
    assert!(matches!(
        registry.set_player_ready("durable", "invalid", true),
        Err(LobbyError::InvalidCapability)
    ));
    assert!(matches!(
        registry.start_player_lobby("durable", other_token.as_str()),
        Err(LobbyError::HostRequired)
    ));
    let restarted = GameRegistry::new().with_store(store.clone());
    assert_eq!(restarted.recover_all_games().unwrap(), vec!["durable"]);
    assert_eq!(
        restarted
            .join_player_lobby("durable", Some(token.as_str()), None)
            .unwrap()
            .1,
        host
    );
    restarted
        .set_player_ready("durable", token.as_str(), true)
        .unwrap();
    restarted
        .set_player_ready("durable", other_token.as_str(), true)
        .unwrap();
    assert_eq!(
        restarted
            .start_player_lobby("durable", token.as_str())
            .unwrap()
            .phase,
        ti4_server::session::registry::LobbyPhase::Running
    );
    assert!(matches!(
        restarted.join_player_lobby("durable", None, Some("Third")),
        Err(LobbyError::AlreadyRunning)
    ));
    assert_eq!(
        restarted
            .join_player_lobby("durable", Some(other_token.as_str()), None)
            .unwrap()
            .1,
        other
    );
    assert_eq!(
        store.load_player_init("durable").unwrap().player_ids,
        vec![host, other]
    );
    let post_start = GameRegistry::new().with_store(store.clone());
    post_start.recover_all_games().unwrap();
    assert_eq!(
        post_start
            .join_player_lobby("durable", Some(token.as_str()), None)
            .unwrap()
            .2,
        None
    );
    post_start.remove_game("durable");
    restarted.remove_game("durable");
    drop(post_start);
    drop(restarted);
    drop(registry);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_persistence_rolls_back_admission_readiness_and_leave() {
    let dir =
        std::env::temp_dir().join(format!("ti4_pil02_failure_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let (_, host, host_token) = registry
        .create_player_lobby("failure".into(), 3, 9, "Host")
        .unwrap();
    let (_, other, other_token) = registry
        .join_player_lobby("failure", None, Some("Guest"))
        .unwrap();
    let other_token = other_token.unwrap();
    let before = registry.player_lobby_status("failure", None).unwrap().0;
    let temporary = store.game_dir("failure").unwrap().join("lobby.tmp");
    std::fs::create_dir(&temporary).unwrap();
    assert!(matches!(
        registry.join_player_lobby("failure", None, Some("Third")),
        Err(LobbyError::Storage(_))
    ));
    assert!(matches!(
        registry.set_player_ready("failure", host_token.as_str(), true),
        Err(LobbyError::Storage(_))
    ));
    assert!(matches!(
        registry.leave_player_lobby("failure", other_token.as_str()),
        Err(LobbyError::Storage(_))
    ));
    let after = registry.player_lobby_status("failure", None).unwrap().0;
    assert_eq!(after.lobby_version, before.lobby_version);
    assert_eq!(after.slots[0].occupant.as_ref(), Some(&host));
    assert_eq!(after.slots[1].occupant.as_ref(), Some(&other));
    assert!(after.slots[2].occupant.is_none());
    assert!(!after.slots[0].ready);
    let recovered = GameRegistry::new().with_store(store.clone());
    recovered.recover_all_games().unwrap();
    assert_eq!(
        recovered
            .player_lobby_status("failure", None)
            .unwrap()
            .0
            .lobby_version,
        before.lobby_version
    );
    std::fs::remove_dir(&temporary).unwrap();
    registry
        .join_player_lobby("failure", None, Some("Third"))
        .unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn presence_has_a_grace_window_but_never_expires_player_sessions() {
    let registry = GameRegistry::new().with_presence_grace(Duration::from_millis(40));
    let (initial, host, token) = registry
        .create_player_lobby("presence".into(), 2, 5, "Host")
        .unwrap();
    assert!(!initial.slots[0].connected);
    let version = initial.lobby_version;
    assert_eq!(
        registry
            .player_lobby_status("presence", None)
            .unwrap()
            .0
            .lobby_version,
        version
    );
    assert!(matches!(
        registry.player_heartbeat("presence", "invalid"),
        Err(LobbyError::InvalidCapability)
    ));
    assert!(
        !registry
            .player_lobby_status("presence", None)
            .unwrap()
            .0
            .slots[0]
            .connected
    );
    assert!(
        registry
            .player_heartbeat("presence", token.as_str())
            .unwrap()
            .slots[0]
            .connected
    );
    assert_eq!(
        registry
            .player_lobby_status("presence", None)
            .unwrap()
            .0
            .lobby_version,
        version
    );
    let (authenticated, connection) = registry.connect_player("presence", token.as_str()).unwrap();
    assert_eq!(authenticated, host);
    registry.disconnect_player("presence", &host, connection);
    assert!(!registry.player_disconnected("presence", &host));
    std::thread::sleep(Duration::from_millis(55));
    assert!(registry.player_disconnected("presence", &host));
    let visible = registry.player_lobby_status("presence", None).unwrap().0;
    assert!(!visible.slots[0].connected);
    assert!(visible.slots[0].can_take_over);
    assert_eq!(
        registry
            .authenticate_player_session("presence", token.as_str())
            .unwrap(),
        host
    );
    assert!(
        registry
            .player_heartbeat("presence", token.as_str())
            .unwrap()
            .slots[0]
            .connected
    );
}

#[test]
fn restart_drops_presence_but_preserves_credentials() {
    let dir = std::env::temp_dir().join(format!("ti4_pil03_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let original = GameRegistry::new()
        .with_store(store.clone())
        .with_presence_grace(Duration::from_millis(25));
    let (_, player, token) = original
        .create_player_lobby("restart".into(), 2, 42, "Host")
        .unwrap();
    original
        .player_heartbeat("restart", token.as_str())
        .unwrap();
    let recovered = GameRegistry::new()
        .with_store(store)
        .with_presence_grace(Duration::from_millis(25));
    recovered.recover_all_games().unwrap();
    assert!(
        !recovered
            .player_lobby_status("restart", None)
            .unwrap()
            .0
            .slots[0]
            .connected
    );
    assert!(!recovered.player_disconnected("restart", &player));
    std::thread::sleep(Duration::from_millis(35));
    assert!(recovered.player_disconnected("restart", &player));
    assert_eq!(
        recovered
            .authenticate_player_session("restart", token.as_str())
            .unwrap(),
        player
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn http_create_join_spectate_and_takeover() {
    let registry = Arc::new(GameRegistry::new().with_presence_grace(Duration::from_millis(30)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server =
        tokio::spawn(axum::serve(listener, ti4_server::create_app(registry)).into_future());
    let client = reqwest::Client::new();
    let base = format!("http://{addr}/api/games");
    for nickname in [
        "",
        " Name",
        "Name ",
        "A\nB",
        "a\u{200d}b",
        "a\u{202e}b",
        &"é".repeat(33),
    ] {
        assert_eq!(
            client
                .post(&base)
                .json(&serde_json::json!({"player_count":2,"nickname":nickname}))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::BAD_REQUEST
        );
    }
    for (body, status) in [
        (
            serde_json::json!({"player_count": 0, "nickname":"Host"}),
            reqwest::StatusCode::BAD_REQUEST,
        ),
        (
            serde_json::json!({"player_count": 2, "bot_seats": ["p2"]}),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        assert_eq!(
            client
                .post(&base)
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            status
        );
    }
    let created: serde_json::Value = client
        .post(&base)
        .json(&serde_json::json!({"player_count": 2, "seed": 42, "nickname":"Host"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let game = created["game_id"].as_str().unwrap();
    let credential = created["player_session"].as_str().unwrap();
    assert_eq!(created["lobby"]["slots"][0]["nickname"], "Host");
    assert!(!created["lobby"].to_string().contains(credential));
    let url = format!("{base}/{game}/lobby");
    for body in [
        serde_json::json!({"kind":"new"}),
        serde_json::json!({"kind":"new","nickname":" "}),
    ] {
        assert_eq!(
            client
                .post(format!("{url}/join"))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::BAD_REQUEST
        );
    }
    let before: serde_json::Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    assert_eq!(before["lobby_version"], 1);
    let joined: serde_json::Value = client
        .post(format!("{url}/join"))
        .json(&serde_json::json!({"kind": "new", "nickname":"Guest"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        joined["lobby"]["slots"][1]["occupant"],
        joined["player"]["id"]
    );
    let resumed: serde_json::Value = client
        .post(format!("{url}/join"))
        .header("x-ti4-player-session", credential)
        .json(&serde_json::json!({"kind": "new"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resumed["player"]["id"], created["player"]["id"]);
    assert_eq!(resumed["lobby"]["slots"][0]["nickname"], "Host");
    assert_eq!(
        client
            .post(format!("{url}/join"))
            .header("x-ti4-player-session", credential)
            .json(&serde_json::json!({"kind":"new","nickname":"Renamed"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::BAD_REQUEST
    );
    assert!(resumed.get("player_session").is_none());
    assert_eq!(
        client
            .post(format!("{url}/join"))
            .json(&serde_json::json!({"kind": "takeover", "player_id": joined["player"]["id"], "nickname":"Guest"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .post(format!("{url}/join"))
            .json(&serde_json::json!({"kind": "new", "nickname":"Guest"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::CONFLICT
    );
    let after: serde_json::Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    assert_eq!(after["lobby_version"], 2);
    assert!(!after.to_string().contains(credential));
    let other_token = joined["player_session"].as_str().unwrap();
    std::thread::sleep(Duration::from_millis(45));
    let eligible: serde_json::Value = client.get(&url).send().await.unwrap().json().await.unwrap();
    assert_eq!(eligible["slots"][1]["can_take_over"], true);
    assert_eq!(
        client
            .post(format!("{url}/join"))
            .header("x-ti4-player-session", other_token)
            .json(&serde_json::json!({"kind":"takeover", "player_id":joined["player"]["id"], "nickname":"New Guest"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    let taken: serde_json::Value = client
        .post(format!("{url}/join"))
        .json(&serde_json::json!({"kind":"takeover", "player_id":joined["player"]["id"], "nickname":"New Guest"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let new_token = taken["player_session"].as_str().unwrap();
    assert_eq!(taken["player"]["id"], joined["player"]["id"]);
    assert_eq!(taken["lobby"]["slots"][1]["nickname"], "New Guest");
    assert_eq!(
        taken["lobby"]["slots"][1]["ready"],
        joined["lobby"]["slots"][1]["ready"]
    );
    assert!(!taken["lobby"].to_string().contains(new_token));
    assert_eq!(
        client
            .get(&url)
            .header("x-ti4-player-session", other_token)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/heartbeat"))
            .header("x-ti4-player-session", other_token)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/join"))
            .json(&serde_json::json!({"kind":"takeover", "player_id":joined["player"]["id"], "nickname":"New Guest"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::CONFLICT
    );
    assert_eq!(
        client
            .get(&url)
            .header("x-ti4-player-session", "invalid")
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/leave"))
            .header("x-ti4-player-session", credential)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{url}/leave"))
            .header("x-ti4-player-session", new_token)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(
        client
            .post(format!("{url}/join"))
            .header("x-ti4-player-session", other_token)
            .json(&serde_json::json!({"kind":"new"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    let replacement: serde_json::Value = client
        .post(format!("{url}/join"))
        .json(&serde_json::json!({"kind":"new", "nickname":"Replacement"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(replacement["player"]["id"], joined["player"]["id"]);
    server.abort();
}
