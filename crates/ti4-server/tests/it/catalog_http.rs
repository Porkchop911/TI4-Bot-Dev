//! `GET /api/catalog`: the catalogue that the engine of a page gives, with the digest of the
//! corpus.

use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::Value;
use ti4_content::ContentStore;
use ti4_host::view::components::catalog;
use ti4_model::content_types::POK;
use ti4_server::session::GameRegistry;

use crate::table_http::serve;

#[tokio::test]
async fn the_server_serves_the_catalogue_of_the_engine_with_the_digest_of_the_corpus() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let response = server
        .client
        .get(server.url("/api/catalog"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"].to_str().unwrap(),
        "application/json"
    );
    let served: Value = response.json().await.unwrap();
    let content = ContentStore::embedded();
    assert_eq!(
        served,
        serde_json::to_value(catalog(content, POK)).unwrap(),
        "the JSON of `{{\"query\":\"catalog\"}}`"
    );
    assert_eq!(
        served["content"],
        ti4_content::provenance::digest_of(content).corpus
    );
    assert_eq!(
        served["records"]["action_cards"].as_array().unwrap().len(),
        142
    );
}
