//! HTTP and WebSocket router configuration.

pub mod content;
pub mod feedback;
pub mod games;
pub mod health;
pub mod models;

use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
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
    let public = registry.public();
    let is_public = public.is_public();
    let serves_web = public.config.web_dir.is_some();

    let mut router = Router::new();
    if !serves_web {
        router = router.route("/", get(root_handler));
    }
    let mut router = router
        .route("/health", get(health::health_handler))
        .route(
            "/api/games",
            get(games::list_games).post(games::create_game),
        )
        .route("/api/setup-options", get(games::setup_options))
        .route("/api/maps", get(games::list_maps))
        .route("/api/maps/{alias}/preview", get(games::preview_map))
        .route(
            "/api/games/{game_id}/lobby/map",
            axum::routing::post(games::choose_lobby_map),
        )
        .route(
            "/api/games/{game_id}/lobby/map-preview",
            get(games::lobby_map_preview),
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
            "/api/games/{game_id}/lobby/settings",
            axum::routing::post(games::change_lobby_settings),
        )
        .route(
            "/api/games/{game_id}/lobby/nickname",
            axum::routing::post(games::rename_in_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/start",
            axum::routing::post(games::start_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/bots",
            axum::routing::post(games::add_bots_to_lobby),
        )
        .route(
            "/api/games/{game_id}/lobby/seat-faction",
            axum::routing::post(games::set_lobby_seat_faction),
        )
        .route(
            "/api/games/{game_id}/lobby/shuffle-seats",
            axum::routing::post(games::shuffle_lobby_seats),
        )
        .route(
            "/api/games/{game_id}/lobby/remove-bot",
            axum::routing::post(games::remove_bot_from_lobby),
        )
        .route(
            "/api/games/{game_id}/settings",
            axum::routing::post(games::change_running_settings),
        )
        .route(
            "/api/games/{game_id}/terminate",
            axum::routing::post(games::terminate_game),
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
        .route(
            "/api/games/{game_id}/history/splice-preview",
            axum::routing::post(games::splice_preview),
        )
        .route(
            "/api/games/{game_id}/turn-redo",
            axum::routing::get(games::turn_redo_status).post(games::turn_redo),
        )
        .route("/api/games/{game_id}/replay", get(games::get_replay))
        .route("/api/games/{game_id}/map", get(games::get_map))
        .route("/api/catalog", get(content::get_catalog))
        .route("/api/models", get(models::list_models))
        .route("/api/models/{file}", get(models::model_file))
        .route(
            "/api/access",
            get(crate::public::get_access).post(crate::public::post_access),
        )
        .route(
            "/api/feedback",
            post(feedback::post_feedback)
                .layer(DefaultBodyLimit::max(crate::feedback::MAX_BODY_BYTES)),
        )
        .route("/ws/games/{game_id}", get(ws_handler))
        .route(
            "/ws/tables/{game_id}",
            get(crate::ws::table::table_ws_handler),
        );
    if !is_public {
        router = router
            .route("/api/dev/scenarios", get(crate::dev::list_scenarios))
            .route("/api/dev/scenarios/launch", post(crate::dev::post_launch));
    }
    if serves_web {
        router = router.fallback(spa_fallback);
    }
    let router = router
        .layer(DefaultBodyLimit::max(MAX_HTTP_REQUEST_BYTES))
        .layer(axum::middleware::from_fn_with_state(
            registry.clone(),
            crate::public::guard,
        ))
        .layer(TraceLayer::new_for_http());
    let router = if !is_public {
        router.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
    } else if let Some(origin) = public
        .config
        .public_origin
        .as_deref()
        .and_then(|o| HeaderValue::from_str(o.trim_end_matches('/')).ok())
    {
        router.layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods([Method::GET, Method::POST])
                .allow_headers([
                    header::CONTENT_TYPE,
                    axum::http::HeaderName::from_static("x-ti4-player-session"),
                ])
                .allow_credentials(true),
        )
    } else {
        // Public without an origin: same-origin only, so no CORS headers at all.
        router
    };
    router.with_state(registry)
}

/// Serves the built web client; unknown non-API paths get `index.html` (SPA routing).
async fn spa_fallback(State(registry): State<Arc<GameRegistry>>, req: Request) -> Response {
    let path = req.uri().path().to_owned();
    if path.starts_with("/api/") || path.starts_with("/ws/") || path == "/api" || path == "/ws" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let Some(dir) = registry.public().config.web_dir.clone() else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    // `foo.wasm.br` / `foo.wasm.gz` next to `foo.wasm` (written by deploy.sh) are served to clients
    // that accept them, with the Content-Type of the original file (`application/wasm`).
    let service = ServeDir::new(&dir)
        .precompressed_br()
        .precompressed_gzip()
        .fallback(ServeFile::new(dir.join("index.html")));
    let Ok(response) = service.oneshot(req).await;
    let mut response = response.map(axum::body::Body::new);
    let cache = if path.starts_with("/assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    if response.status().is_success() {
        let headers = response.headers_mut();
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
        // The same URL answers .br, .gz or plain depending on the request.
        headers.append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    }
    response
}
