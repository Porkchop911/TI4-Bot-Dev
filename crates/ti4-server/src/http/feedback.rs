//! `POST /api/feedback`: stores one JSON report per request under the data directory.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::feedback::{FeedbackRequest, store_once, validate};
use crate::session::GameRegistry;

pub async fn post_feedback(
    State(registry): State<Arc<GameRegistry>>,
    Json(request): Json<FeedbackRequest>,
) -> Response {
    if let Err(message) = validate(&request) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid_feedback", "message": message })),
        )
            .into_response();
    }
    let public = registry.public();
    let Some(dir) = public.config.data_dir.clone() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "no_storage", "message": "feedback storage is not configured" })),
        )
            .into_response();
    };
    let commit = public.build_commit();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    match tokio::task::spawn_blocking(move || store_once(&dir, request, now, &commit)).await {
        // `duplicate`: a retry of a report that was stored before; nothing new was written.
        Ok(Ok((id, duplicate))) => {
            Json(serde_json::json!({ "id": id, "duplicate": duplicate })).into_response()
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "storage_failed", "message": "could not save the report" })),
        )
            .into_response(),
    }
}
