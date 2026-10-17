//! Health check endpoint.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use ti4_host::protocol::PROTOCOL_VERSION;

use crate::session::GameRegistry;

/// Health check response payload.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub protocol_version: u16,
    pub commit: String,
    pub public: bool,
}

/// Handler for `GET /health`.
pub async fn health_handler(State(registry): State<Arc<GameRegistry>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        protocol_version: PROTOCOL_VERSION,
        commit: registry.public().build_commit(),
        public: registry.public().is_public(),
    })
}
