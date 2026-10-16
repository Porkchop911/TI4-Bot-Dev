//! `GET /api/catalog`: the printed cards, as the engine of a page gives them
//! (`{"query":"catalog"}` of `ti4-wasm`), with the digest of the corpus in `content`.

use std::sync::OnceLock;

use axum::http::header;
use axum::response::IntoResponse;
use ti4_content::ContentStore;
use ti4_host::view::components::catalog;
use ti4_model::content_types::POK;

/// The catalogue as JSON. It does not change while the server runs, so it is written once.
fn catalog_json() -> &'static str {
    static JSON: OnceLock<String> = OnceLock::new();
    JSON.get_or_init(|| {
        serde_json::to_string(&catalog(ContentStore::embedded(), POK))
            .expect("the catalogue serializes")
    })
}

/// Handler for `GET /api/catalog`.
pub async fn get_catalog() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], catalog_json())
}
