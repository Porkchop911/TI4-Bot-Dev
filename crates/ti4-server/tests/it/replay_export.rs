//! `GET /api/games/{id}/replay`: any seated player may copy the game's history out; everyone
//! else is refused.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::Value;
use tokio::net::TcpListener;

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_server::create_app;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};

async fn spawn(registry: Arc<GameRegistry>) -> String {
    let app = create_app(registry);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{address}")
}

fn answer_first_choice(session: &GameSession) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some((seat, nonce, version)) = session.current_pending_decision() {
            let option = session
                .get_snapshot(&ViewerRole::Player(seat.clone()))
                .pending_choice
                .expect("pending choice")
                .choice
                .options[0]
                .id
                .clone();
            session
                .submit_choice(&seat, &nonce, version, &option)
                .expect("submit");
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out: {:?}",
            session.error()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

async fn replay(base: &str, game: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = reqwest::Client::new().get(format!("{base}/api/games/{game}/replay"));
    if let Some(token) = token {
        request = request.header("x-ti4-player-session", token);
    }
    let response = request.send().await.expect("replay request");
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let json = serde_json::from_str(&body).unwrap_or(Value::String(body));
    (status, json)
}

#[tokio::test(flavor = "multi_thread")]
async fn seated_players_get_the_replay_and_everyone_else_is_refused() {
    let registry = Arc::new(GameRegistry::new());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("replay_probe", state)
        .with_seed(42)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest.clone(), SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let tokens = session.seat_tokens();
    for _ in 0..3 {
        answer_first_choice(&session);
    }
    let base = spawn(registry).await;

    // No credential, and a credential that belongs to nobody.
    let (status, _) = replay(&base, "replay_probe", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "missing token");
    let (status, _) = replay(&base, "replay_probe", Some("not-a-token")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "unknown token");
    // A known game id is not needed to be refused, and an unknown game is a 404 for a real token.
    let (status, _) = replay(&base, "no_such_game", Some(&tokens[&host])).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Every seated player, not only the host, gets the same replay.
    let (status, from_host) = replay(&base, "replay_probe", Some(&tokens[&host])).await;
    assert_eq!(status, StatusCode::OK, "{from_host}");
    let (status, from_guest) = replay(&base, "replay_probe", Some(&tokens[&guest])).await;
    assert_eq!(status, StatusCode::OK, "{from_guest}");
    assert_eq!(from_host, from_guest);

    assert_eq!(from_host["format"], "ti4-replay");
    assert_eq!(from_host["version"], 1);
    assert_eq!(from_host["game_id"], "replay_probe");
    assert_eq!(from_host["seed"], 42);
    assert_eq!(from_host["player_ids"].as_array().unwrap().len(), 2);
    let history = &from_host["history"];
    assert_eq!(
        history["decisions"].as_array().unwrap().len(),
        session.decision_log().len(),
        "the live decisions, not a stale saved file"
    );
    assert!(history["decisions"].as_array().unwrap().len() >= 3);
    assert!(!history["events"].as_array().unwrap().is_empty());
    assert!(history["revision"].as_u64().unwrap() > 1);
}
