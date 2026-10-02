//! Health check endpoint.

use axum::Json;
use serde::Serialize;

use crate::protocol::PROTOCOL_VERSION;

/// Health check response payload.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub protocol_version: u16,
}

/// Handler for `GET /health`.
pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        protocol_version: PROTOCOL_VERSION,
    })
}
