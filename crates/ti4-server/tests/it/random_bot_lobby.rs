//! Lobby HTTP for in-process random bots: add, fill, validation, authorization, start,
//! presence, takeover, removal and recovery.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{Value, json};
use ti4_model::id::PlayerId;
use ti4_server::create_app;
use ti4_server::session::{BotServiceConfig, GameRegistry, SeatController};
use ti4_server::storage::FileGameStore;

struct Server {
    addr: std::net::SocketAddr,
    registry: Arc<GameRegistry>,
    client: reqwest::Client,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve(registry: Arc<GameRegistry>) -> Server {
    ti4_server::session::random_bot::set_random_bot_delay_override(Some(0));
    // The model of the tests is loaded, as a server loads its model directory at start.
    let _ = ti4_host::bots::test_model();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = create_app(registry.clone());
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Server {
        addr,
        registry,
        client: reqwest::Client::new(),
        handle,
    }
}

impl Server {
    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Creates a lobby of `players` seats; returns (game id, host token, host player id).
    async fn lobby(&self, players: usize) -> (String, String, String) {
        let created: Value = self
            .client
            .post(self.url("/api/games"))
            .json(&json!({"player_count": players, "seed": 42, "nickname": "Host"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (
            created["game_id"].as_str().unwrap().to_owned(),
            created["player_session"].as_str().unwrap().to_owned(),
            created["player"]["id"].as_str().unwrap().to_owned(),
        )
    }

    async fn post(
        &self,
        game: &str,
        route: &str,
        token: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut request = self
            .client
            .post(self.url(&format!("/api/games/{game}/lobby/{route}")))
            .json(&body);
        if let Some(token) = token {
            request = request.header("x-ti4-player-session", token);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, value)
    }

    async fn view(&self, game: &str) -> Value {
        self.client
            .get(self.url(&format!("/api/games/{game}/lobby")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }
}

fn open_slots(view: &Value) -> usize {
    view["slots"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["occupant"].is_null())
        .count()
}

#[tokio::test]
async fn the_host_adds_one_random_bot_with_its_own_credential_and_no_password() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (game, host, _) = server.lobby(4).await;
    let view = server.view(&game).await;
    assert_eq!(view["bot_kinds"], json!(["random", "smart", "mlp:test"]));
    assert_eq!(view["bot_service_enabled"], false);

    let (status, body) = server
        .post(&game, "bots", Some(&host), json!({"kind": "random"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let slot = &body["slots"][1];
    assert_eq!(slot["nickname"], "Random bot 1");
    assert_eq!(slot["bot"], "random");
    assert_eq!(slot["connected"], true);
    assert_eq!(slot["can_take_over"], false);
    assert_eq!(open_slots(&body), 2);
    // No credential of the bot appears anywhere in the public view.
    assert!(!body.to_string().contains("session"));

    let (status, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "nickname": "Robo"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slots"][2]["nickname"], "Robo");
}

#[tokio::test]
async fn fill_seats_every_open_slot_and_count_adds_that_many() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (game, host, _) = server.lobby(5).await;
    let (status, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "count": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(open_slots(&body), 2);
    assert_eq!(body["slots"][2]["nickname"], "Random bot 2");
    let (status, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(open_slots(&body), 0);
    let names: Vec<_> = body["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["nickname"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Host",
            "Random bot 1",
            "Random bot 2",
            "Random bot 3",
            "Random bot 4"
        ]
    );
    // Nothing is left to fill.
    let (status, _) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn validation_and_authorization() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (game, host, _) = server.lobby(3).await;
    let guest: Value = server
        .client
        .post(server.url(&format!("/api/games/{game}/lobby/join")))
        .json(&json!({"kind": "new", "nickname": "Guest"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let guest_token = guest["player_session"].as_str().unwrap().to_owned();

    // Not the host: 403. No credential: 403. Unknown credential: 403.
    for token in [Some(guest_token.as_str()), Some("nope")] {
        let (status, _) = server
            .post(&game, "bots", token, json!({"kind": "random"}))
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _) = server
        .post(&game, "bots", None, json!({"kind": "random"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Unknown kind, mlp through this route, fill with count, count 0, nickname with several.
    for body in [
        json!({"kind": "wizard"}),
        json!({"kind": "mlp"}),
        json!({"kind": "random", "fill": true, "count": 1}),
        json!({"kind": "random", "count": 0}),
        json!({"kind": "random", "nickname": "  "}),
        json!({"kind": "random", "password": "x"}),
    ] {
        let (status, text) = server.post(&game, "bots", Some(&host), body.clone()).await;
        assert!(
            status == StatusCode::BAD_REQUEST
                || status == StatusCode::UNPROCESSABLE_ENTITY
                || status == StatusCode::CONFLICT,
            "{body} -> {status} {text}"
        );
    }
    // More than the open slots (1 left of 3 after the guest): refused whole, nothing added.
    let (status, _) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "count": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(open_slots(&server.view(&game).await), 1);

    // A nickname names a single bot only.
    let (other, other_host, _) = server.lobby(4).await;
    let (status, _) = server
        .post(
            &other,
            "bots",
            Some(&other_host),
            json!({"kind": "random", "nickname": "A", "fill": true}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(open_slots(&server.view(&other).await), 3);

    // Unknown game.
    let (status, _) = server
        .post(
            "missing_game",
            "bots",
            Some(&host),
            json!({"kind": "random"}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The MLP route is unchanged: disabled without a bot service.
    let (status, _) = server
        .post(&game, "add-bot", Some(&host), json!({"password": "x"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn mlp_bots_stay_listed_when_the_service_is_configured() {
    let registry = GameRegistry::new().with_bot_service(BotServiceConfig {
        password: "pw".to_owned(),
        advisor_url: "http://127.0.0.1:1".to_owned(),
        bot_agent_bin: std::path::PathBuf::from("/bin/true"),
        server_port: 1,
        max_active_bots: 0,
    });
    let server = serve(Arc::new(registry)).await;
    let (game, host, _) = server.lobby(3).await;
    assert_eq!(
        server.view(&game).await["bot_kinds"],
        json!(["random", "smart", "mlp:test", "mlp"])
    );
    // Random bots do not count against the MLP child-process limit (0 here) and need no password.
    let (status, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    // The MLP path still checks its password and its limit.
    let (status, _) = server
        .post(&game, "add-bot", Some(&host), json!({"password": "wrong"}))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn smart_bots_are_added_named_and_started_as_smart_seats() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (game, host, host_id) = server.lobby(4).await;
    let (status, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "smart", "count": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slots"][1]["nickname"], "Rule bot 1");
    assert_eq!(body["slots"][2]["nickname"], "Rule bot 2");
    assert_eq!(body["slots"][1]["bot"], "smart");
    assert_eq!(body["slots"][1]["can_take_over"], false);
    let (status, body) = server
        .post(&game, "bots", Some(&host), json!({"kind": "random"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slots"][3]["nickname"], "Random bot 1");
    let (status, _) = server
        .post(&game, "bots", Some(&host), json!({"kind": "genius"}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, started) = server.post(&game, "start", Some(&host), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    let session = server.registry.get_game(&game).expect("session");
    let seats = session.lobby_details().1;
    let count = |kind: SeatController| seats.values().filter(|c| **c == kind).count();
    assert_eq!(count(SeatController::SMART), 2);
    assert_eq!(count(SeatController::RANDOM), 1);
    // The bots play on their own until the host is asked.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some((seat, _, _)) = session.current_pending_decision() {
            assert_eq!(seat.as_str(), host_id, "a bot seat waited for a person");
            break;
        }
        assert!(Instant::now() < deadline, "the host was never asked");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.error().is_none());
}

#[tokio::test]
async fn a_lobby_of_a_host_and_bots_starts_and_the_bots_play() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (game, host, host_id) = server.lobby(4).await;
    let (status, _) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    // The host readies up (a bot never needs to) and starts.
    let (status, started) = server.post(&game, "start", Some(&host), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    // A late bot request is refused: the lobby has started.
    let (status, _) = server
        .post(&game, "bots", Some(&host), json!({"kind": "random"}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let session = server.registry.get_game(&game).expect("session");
    let seats = session.lobby_details().1;
    assert_eq!(
        seats[&PlayerId::new(host_id.clone())],
        SeatController::Human
    );
    assert_eq!(
        seats
            .values()
            .filter(|c| **c == SeatController::RANDOM)
            .count(),
        3
    );
    // The bots act on their own, and the host is asked when it is the host's turn.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some((seat, _, _)) = session.current_pending_decision() {
            assert_eq!(seat.as_str(), host_id, "a bot seat waited for a person");
            break;
        }
        assert!(Instant::now() < deadline, "the host was never asked");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(session.error().is_none());

    // Presence: bots are always present, and a takeover of a bot is refused.
    let view = server.view(&game).await;
    for slot in view["slots"].as_array().unwrap() {
        if slot["bot"] == "random" {
            assert_eq!(slot["connected"], true);
            assert_eq!(slot["can_take_over"], false);
            let (status, _) = server
                .post(
                    &game,
                    "join",
                    None,
                    json!({"kind": "takeover", "player_id": slot["occupant"], "nickname": "Thief"}),
                )
                .await;
            assert_eq!(status, StatusCode::CONFLICT);
        }
    }
}

#[tokio::test]
async fn removing_a_random_bot_frees_its_seat_and_the_name_is_reused() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (game, host, _) = server.lobby(3).await;
    let (_, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    let bot_id = body["slots"][1]["occupant"].as_str().unwrap().to_owned();
    let (status, body) = server
        .post(
            &game,
            "remove-bot",
            Some(&host),
            json!({"player_id": bot_id}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(open_slots(&body), 1);
    let (_, body) = server
        .post(&game, "bots", Some(&host), json!({"kind": "random"}))
        .await;
    assert_eq!(body["slots"][1]["nickname"], "Random bot 1");
    assert_eq!(body["slots"][2]["nickname"], "Random bot 2");
}

#[tokio::test]
async fn bot_seats_survive_a_restart_of_the_lobby_and_of_the_running_game() {
    let dir = std::env::temp_dir().join(format!("ti4_rb_lobby_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let server = serve(Arc::new(GameRegistry::new().with_store(store.clone()))).await;
    let (game, host, _) = server.lobby(4).await;
    server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "count": 1}),
        )
        .await;
    let (status, body) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "mlp:test", "count": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slots"][2]["nickname"], "Test model 1");
    drop(server);

    // The lobby comes back with its bots.
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    registry.recover_all_games().expect("recover lobby");
    let server = serve(registry).await;
    let view = server.view(&game).await;
    assert_eq!(view["slots"][1]["bot"], "random");
    assert_eq!(view["slots"][2]["bot"], "mlp:test");
    let (status, _) = server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = server.post(&game, "start", Some(&host), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let session = server.registry.get_game(&game).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while session.current_pending_decision().is_none() {
        assert!(Instant::now() < deadline, "host never asked");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.stop();
    drop(session);
    drop(server);

    // The started game recovers with the same bot seats (the MLP seat with its model) and keeps
    // playing to the host's prompt.
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    registry.recover_all_games().expect("recover game");
    let session = registry.get_game(&game).expect("recovered");
    let seats = session.lobby_details().1;
    assert_eq!(
        seats
            .values()
            .filter(|c| **c == SeatController::RANDOM)
            .count(),
        2
    );
    assert_eq!(
        seats
            .values()
            .filter(|c| **c
                == SeatController::Bot(ti4_host::bots::BotKind::Mlp(ti4_host::bots::test_model())))
            .count(),
        1
    );
    session.wait_replayed().expect("replayed");
    assert!(session.error().is_none());
    // The bot's credential is not usable after recovery either.
    for token in session.seat_tokens().values() {
        let _ = token; // tokens exist in the record; viewer_for_seat_token is the gate
    }
    let bot_tokens: Vec<_> = seats
        .iter()
        .filter(|(_, c)| **c == SeatController::RANDOM)
        .map(|(seat, _)| session.seat_tokens().get(seat).cloned())
        .collect();
    for token in bot_tokens.into_iter().flatten() {
        assert!(session.viewer_for_seat_token(&token).is_none());
        assert!(registry.authenticate_player_session(&game, &token).is_err());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
