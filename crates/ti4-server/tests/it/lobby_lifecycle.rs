use std::sync::Arc;

use tokio::net::TcpListener;

use ti4_model::id::PlayerId;
use ti4_server::create_app;
use ti4_server::session::{GameRegistry, SeatController};
use ti4_server::storage::FileGameStore;

async fn spawn_server() -> String {
    let app = create_app(Arc::new(GameRegistry::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let address = listener.local_addr().expect("listener address");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve app") });
    format!("http://{address}")
}

#[tokio::test]
async fn a_lobby_needs_every_seat_filled_and_its_host_to_start_a_session() {
    let base_url = spawn_server().await;
    let client = reqwest::Client::new();
    let created = client
        .post(format!("{base_url}/api/games"))
        .json(&serde_json::json!({
            "player_count": 2,
            "seed": 42, "nickname": "Host"
        }))
        .send()
        .await
        .expect("create lobby");
    assert_eq!(created.status(), reqwest::StatusCode::OK);
    let created: serde_json::Value = created.json().await.expect("created lobby body");
    let game_id = created["game_id"].as_str().expect("generated game ID");
    let p1 = created["player_session"]
        .as_str()
        .expect("p1 token")
        .to_owned();
    // A seat is open: there is no "Ready", and the table starts when every seat is filled.
    let early_start = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/start"))
        .header("x-ti4-player-session", &p1)
        .send()
        .await
        .expect("early start");
    assert_eq!(early_start.status(), reqwest::StatusCode::CONFLICT);
    let p2 = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/join"))
        .json(&serde_json::json!({ "kind": "new", "nickname": "Guest" }))
        .send()
        .await
        .expect("claim p2")
        .json::<serde_json::Value>()
        .await
        .expect("claimed p2 body")["player_session"]
        .as_str()
        .expect("p2 token")
        .to_owned();
    assert_eq!(
        created["lobby"]["slots"][0]["occupant"],
        created["player"]["id"]
    );
    assert!(created["lobby"]["slots"][0].get("ready").is_none());
    assert!(!created["lobby"].to_string().contains(&p1));

    let snapshot = client
        .get(format!("{base_url}/api/games/{game_id}/snapshot"))
        .send()
        .await
        .expect("unstarted snapshot");
    assert_eq!(snapshot.status(), reqwest::StatusCode::NOT_FOUND);

    let non_host_start = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/start"))
        .header("x-ti4-player-session", &p2)
        .send()
        .await
        .expect("non-host start");
    assert_eq!(non_host_start.status(), reqwest::StatusCode::FORBIDDEN);

    let started = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/start"))
        .header("x-ti4-player-session", &p1)
        .send()
        .await
        .expect("host start");
    assert_eq!(started.status(), reqwest::StatusCode::OK);
    assert_eq!(
        started
            .json::<serde_json::Value>()
            .await
            .expect("running lobby")["phase"],
        "running"
    );

    let mutation_after_start = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/nickname"))
        .header("x-ti4-player-session", &p1)
        .json(&serde_json::json!({ "nickname": "Other" }))
        .send()
        .await
        .expect("rename after start");
    assert_eq!(mutation_after_start.status(), reqwest::StatusCode::CONFLICT);
    let snapshot = client
        .get(format!("{base_url}/api/games/{game_id}/snapshot"))
        .header("x-ti4-player-session", &p1)
        .send()
        .await
        .expect("started snapshot");
    assert_eq!(snapshot.status(), reqwest::StatusCode::OK);
}

#[tokio::test]
async fn lobby_creation_and_mutation_rejections_are_atomic_and_claim_bound() {
    let base_url = spawn_server().await;
    let client = reqwest::Client::new();

    for payload in [
        serde_json::json!({"player_count": 0, "nickname":"Host"}),
        serde_json::json!({"player_count": 1, "nickname":"Host"}),
        serde_json::json!({"player_count": 9, "nickname":"Host"}),
    ] {
        let response = client
            .post(format!("{base_url}/api/games"))
            .json(&payload)
            .send()
            .await
            .expect("reject invalid lobby creation");
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{payload}"
        );
    }
    let invalid = client
        .post(format!("{base_url}/api/games"))
        .json(&serde_json::json!({"player_count": 2, "bot_seats": ["p1"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), reqwest::StatusCode::UNPROCESSABLE_ENTITY);

    let created = client
        .post(format!("{base_url}/api/games"))
        .json(&serde_json::json!({
            "player_count": 2,
            "seed": 19, "nickname": "Host"
        }))
        .send()
        .await
        .expect("create lobby");
    assert_eq!(created.status(), reqwest::StatusCode::OK);
    let created: serde_json::Value = created.json().await.expect("created lobby");
    let game_id = created["game_id"].as_str().expect("generated game ID");
    let p1 = created["player_session"].as_str().expect("p1 token");
    let p2 = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/join"))
        .json(&serde_json::json!({ "kind": "new", "nickname": "Guest" }))
        .send()
        .await
        .expect("claim p2")
        .json::<serde_json::Value>()
        .await
        .expect("claimed p2 body")["player_session"]
        .as_str()
        .expect("p2 token")
        .to_owned();
    let other = client
        .post(format!("{base_url}/api/games"))
        .json(&serde_json::json!({
            "player_count": 2, "nickname": "Other"
        }))
        .send()
        .await
        .expect("create other lobby");
    assert_eq!(other.status(), reqwest::StatusCode::OK);
    let other: serde_json::Value = other.json().await.expect("other lobby body");
    let other_token = other["player_session"].as_str().expect("other token");

    let public = client
        .get(format!("{base_url}/api/games/{game_id}/lobby"))
        .send()
        .await
        .expect("get public lobby")
        .json::<serde_json::Value>()
        .await
        .expect("parse public lobby");
    assert_eq!(public["slots"][0]["occupant"], created["player"]["id"]);
    assert!(!public.to_string().contains(p1));
    assert!(!public.to_string().contains(&p2));

    for token in [None, Some("forged"), Some(other_token)] {
        let mut request = client
            .post(format!("{base_url}/api/games/{game_id}/lobby/nickname"))
            .json(&serde_json::json!({ "nickname": "Other" }));
        if let Some(token) = token {
            request = request.header("x-ti4-player-session", token);
        }
        let response = request.send().await.expect("reject invalid rename");
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);

        let mut request = client.post(format!("{base_url}/api/games/{game_id}/lobby/start"));
        if let Some(token) = token {
            request = request.header("x-ti4-player-session", token);
        }
        let response = request.send().await.expect("reject invalid start");
        assert_eq!(response.status(), reqwest::StatusCode::FORBIDDEN);
    }

    let unchanged = client
        .get(format!("{base_url}/api/games/{game_id}/lobby"))
        .send()
        .await
        .expect("get unchanged lobby")
        .json::<serde_json::Value>()
        .await
        .expect("parse unchanged lobby");
    assert_eq!(unchanged["lobby_version"], 2);
    assert_eq!(unchanged["slots"][0]["nickname"], "Host");
    assert_eq!(unchanged["slots"][1]["nickname"], "Guest");
}

#[test]
fn recovery_migrates_legacy_initialized_records_to_running_lobbies() {
    let path = std::env::temp_dir().join(format!(
        "ti4_legacy_lobby_recovery_{:016x}",
        rand::random::<u64>()
    ));
    let store = Arc::new(FileGameStore::new(&path).expect("create store"));
    let players = vec![PlayerId::new("p1"), PlayerId::new("p2")];
    let content = ti4_content::ContentStore::embedded();
    let (state, galaxy) = ti4_host::map::create_game_with_template(content, &players, 29, None)
        .expect("create legacy map");
    let map_tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    let registry = GameRegistry::new().with_store(store.clone());
    let legacy = registry
        .create_game(
            ti4_server::session::SessionConfig::new("legacy_running", state)
                .with_seed(29)
                .with_player_ids(players.clone())
                .with_galaxy(galaxy, map_tiles)
                .with_seat(players[0].clone(), SeatController::Human)
                .with_seat(players[1].clone(), SeatController::BotFirstOption),
        )
        .expect("create legacy active session");
    let live_lobby = registry
        .lobby_status("legacy_running", None)
        .expect("live legacy lobby")
        .lobby;
    assert_eq!(
        live_lobby.phase,
        ti4_server::session::registry::LobbyPhase::Running
    );
    assert_eq!(live_lobby.player_ids, players);
    let registered = GameRegistry::new();
    registered
        .register_recovered(legacy.clone())
        .expect("register recovered session");
    assert_eq!(
        registered
            .lobby_status("legacy_running", None)
            .expect("registered lobby")
            .lobby
            .phase,
        ti4_server::session::registry::LobbyPhase::Running
    );
    registered
        .remove_game("legacy_running")
        .expect("stop registered session");
    assert!(
        !store
            .game_dir("legacy_running")
            .expect("legacy game directory")
            .join("lobby.json")
            .exists()
    );

    let recovered = GameRegistry::new().with_store(store);
    assert_eq!(
        recovered.recover_all_games().expect("recover legacy game"),
        vec!["legacy_running"]
    );
    let lobby = recovered
        .lobby_status("legacy_running", None)
        .expect("migrated legacy lobby")
        .lobby;
    assert_eq!(
        lobby.phase,
        ti4_server::session::registry::LobbyPhase::Running
    );
    assert_eq!(lobby.host_seat, PlayerId::new("p1"));
    assert_eq!(lobby.player_ids, players);
    assert!(lobby.seats.values().all(|seat| seat.ready));
    recovered
        .remove_game("legacy_running")
        .expect("stop recovered legacy session");
    let _ = std::fs::remove_dir_all(path);
}
