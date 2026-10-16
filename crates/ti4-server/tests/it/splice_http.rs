//! `POST /api/games/{id}/history/splice-preview`: host-only, read-only, typed report.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{Value, json};
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

async fn preview(base: &str, game: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut request = reqwest::Client::new()
        .post(format!("{base}/api/games/{game}/history/splice-preview"))
        .json(&body);
    if let Some(token) = token {
        request = request.header("x-ti4-player-session", token);
    }
    let response = request.send().await.expect("preview request");
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let json = serde_json::from_str(&text).unwrap_or(Value::String(text));
    (status, json)
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_host_gets_a_report_and_nothing_changes() {
    let registry = Arc::new(GameRegistry::new());
    let host = PlayerId::new("p1");
    let guest = PlayerId::new("p2");
    let players = vec![host.clone(), guest.clone()];
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("splice_http", state)
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
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.current_pending_decision().is_none() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    let log = session.decision_log();
    let version = session.game_version();
    let base = spawn(registry).await;
    let host_token = tokens[&host].as_str();
    let guest_token = tokens[&guest].as_str();

    let edit = json!({ "edit": { "kind": "remove", "cursor": 1 } });
    let (status, body) = preview(&base, "splice_http", Some(host_token), edit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["edit"], edit["edit"]);
    assert_eq!(body["original_decisions"], log.len());
    assert!(body["first_conflict"].is_object() || body["survives_to_end"] == true);
    assert!(body["rng"]["status"].is_string());
    assert!(body["alignment"].is_array());

    let (status, _) = preview(&base, "splice_http", Some(guest_token), edit.clone()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = preview(&base, "splice_http", None, edit.clone()).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "{status}"
    );
    let (status, _) = preview(&base, "nope", Some(host_token), edit).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = preview(
        &base,
        "splice_http",
        Some(host_token),
        json!({ "edit": { "kind": "remove", "cursor": 999 } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = preview(
        &base,
        "splice_http",
        Some(host_token),
        json!({ "edit": { "kind": "remove", "cursor": 1 }, "commit": true }),
    )
    .await;
    assert!(
        status.is_client_error(),
        "unknown fields are refused: {status}"
    );

    assert_eq!(session.decision_log(), log);
    assert_eq!(session.game_version(), version);
}
