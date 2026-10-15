//! The files of the MLP models, for the engine of a page (`crate::models`).
//!
//! - `GET /api/models`: `[{ id, label, sha256, bytes }]`, the offered models. `id` is the `<name>`
//!   of the kind `mlp:<name>`, `sha256` the SHA-256 of the whole file and `bytes` its size.
//! - `GET /api/models/<sha256>.ti4m`: the file of an offered model. The address names the content,
//!   so it is cached for a year as immutable. When the client accepts brotli and the pack script
//!   wrote `<file>.br` next to it, that is sent with `Content-Encoding: br`.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::session::GameRegistry;

/// A model of `GET /api/models`.
#[derive(Debug, Serialize)]
pub struct ModelListing {
    /// The `<name>` of the kind `mlp:<name>`.
    pub id: String,
    /// What the lobby shows.
    pub label: String,
    /// The SHA-256 of the file: its address, `/api/models/<sha256>.ti4m`.
    pub sha256: String,
    /// The size of the file, not compressed.
    pub bytes: u64,
}

/// Handler for `GET /api/models`.
pub async fn list_models(State(registry): State<Arc<GameRegistry>>) -> Response {
    let models: Vec<ModelListing> = registry
        .models()
        .iter()
        .filter(|served| served.offered)
        .map(|served| ModelListing {
            id: served.model.name().to_owned(),
            label: served.model.label().to_owned(),
            sha256: served.sha256.clone(),
            bytes: served.bytes,
        })
        .collect();
    let mut response = Json(models).into_response();
    // The list changes with a deploy.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// Handler for `GET /api/models/{file}`, `file` being `<sha256>.ti4m`.
pub async fn model_file(
    Path(file): Path<String>,
    State(registry): State<Arc<GameRegistry>>,
    req: Request,
) -> Response {
    let Some(served) = file.strip_suffix(".ti4m").and_then(|sha| {
        registry
            .models()
            .iter()
            .find(|served| served.offered && served.sha256 == sha)
    }) else {
        return (StatusCode::NOT_FOUND, "no such model").into_response();
    };
    let Ok(response) = ServeFile::new(&served.path)
        .precompressed_br()
        .oneshot(req)
        .await;
    let mut response = response.map(axum::body::Body::new);
    if response.status().is_success() {
        let headers = response.headers_mut();
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        );
        // The same address answers .br or plain depending on the request.
        headers.append(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    }
    response
}
