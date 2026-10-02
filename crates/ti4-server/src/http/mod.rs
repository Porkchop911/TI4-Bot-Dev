//! HTTP and WebSocket router configuration.

pub mod content;
pub mod games;
pub mod health;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::session::GameRegistry;
use crate::ws::ws_handler;

async fn root_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(
        r#"<!DOCTYPE html>
<html>
<head><title>Twilight Imperium 4 Server</title></head>
<body style="font-family: system-ui, sans-serif; background: #090d16; color: #f8fafc; padding: 40px; line-height: 1.5;">
  <h1 style="color: #38bdf8;">Twilight Imperium 4 — Server Online</h1>
  <p>The authoritative engine and WebSocket backend is running (Protocol v1).</p>
  <ul>
    <li>Health Check: <a style="color: #38bdf8;" href="/health">/health</a></li>
    <li>Games List: <a style="color: #38bdf8;" href="/api/games">/api/games</a></li>
  </ul>
  <h2>Web Client</h2>
  <p>To access the visual game board and controls, start the web client in another terminal:</p>
  <pre style="background: #1e293b; padding: 12px; border-radius: 6px; color: #fbbf24; display: inline-block;">cd web && npm run dev</pre>
  <p>Then open <a style="color: #38bdf8;" href="http://127.0.0.1:3000">http://127.0.0.1:3000</a> in your browser.</p>
</body>
</html>"#,
    )
}

/// Construct the authoritative application router with all HTTP and WebSocket endpoints.
pub fn create_app(registry: Arc<GameRegistry>) -> Router {
    const MAX_HTTP_REQUEST_BYTES: usize = 8 * 1024;
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health::health_handler))
        .route(
            "/api/games",
            get(games::list_games).post(games::create_game),
        )
        .route("/api/games/{game_id}/lobby", get(games::get_lobby))
        .route(
            "/api/games/{game_id}/lobby/join",
            axum::routing::post(games::join_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/leave",
            axum::routing::post(games::leave_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/heartbeat",
            axum::routing::post(games::heartbeat),
        )
        .route(
            "/api/games/{game_id}/lobby/ready",
            axum::routing::post(games::set_ready),
        )
        .route(
            "/api/games/{game_id}/lobby/reorder",
            axum::routing::post(games::reorder_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/start",
            axum::routing::post(games::start_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/add-bot",
            axum::routing::post(games::add_bot_to_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/remove-bot",
            axum::routing::post(games::remove_bot_from_lobby),
        )
        .route("/api/games/{game_id}/snapshot", get(games::get_snapshot))
        .route(
            "/api/games/{game_id}/batches",
            axum::routing::post(games::submit_batch),
        )
        .route(
            "/api/games/{game_id}/history",
            axum::routing::post(games::change_history),
        )
        .route("/api/games/{game_id}/map", get(games::get_map))
        .route("/api/content/catalog", get(content::get_catalog))
        .route("/api/dev/scenarios", get(crate::dev::list_scenarios))
        .route(
            "/api/dev/scenarios/launch",
            axum::routing::post(crate::dev::launch_scenario),
        )
        .route("/ws/games/{game_id}", get(ws_handler))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(DefaultBodyLimit::max(MAX_HTTP_REQUEST_BYTES))
        .with_state(registry)
}
