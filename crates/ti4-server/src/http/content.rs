//! HTTP endpoint for card metadata from the embedded content store.

use axum::Json;
use serde::Serialize;
use ti4_content::ContentStore;
use ti4_model::content_types::ContentType;

#[derive(Debug, Clone, Serialize)]
pub struct ContentCatalogResponse {
    pub strategy_cards: Vec<StrategyCardSummary>,
    pub secret_objectives: Vec<ObjectiveSummary>,
    pub public_objectives: Vec<ObjectiveSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyCardSummary {
    pub id: String,
    pub name: String,
    pub initiative: i64,
    pub primary_text: String,
    pub secondary_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ObjectiveSummary {
    pub id: String,
    pub name: String,
    pub phase: String,
    pub points: i64,
    pub description: String,
}

fn array_to_strings(rec: &ti4_content::Record, key: &str) -> Vec<String> {
    if let Some(serde_json::Value::Array(arr)) = rec.raw(key) {
        arr.iter()
            .filter_map(|v| v.as_str().map(ToOwned::to_owned))
            .collect()
    } else {
        Vec::new()
    }
}

/// Handler for `GET /api/content/catalog`.
pub async fn get_catalog() -> Json<ContentCatalogResponse> {
    let content = ContentStore::embedded();

    let mut strategy_cards = Vec::new();
    for rec in content.records(ContentType::StrategyCards) {
        if let Some(id) = rec.id() {
            strategy_cards.push(StrategyCardSummary {
                id: id.to_owned(),
                name: rec.text("name").unwrap_or(id).to_owned(),
                initiative: rec.int("initiative").unwrap_or(0),
                primary_text: array_to_strings(rec, "primaryTexts").join("\n"),
                secondary_text: array_to_strings(rec, "secondaryTexts").join("\n"),
            });
        }
    }

    let mut secret_objectives = Vec::new();
    for rec in content.records(ContentType::SecretObjectives) {
        if let Some(id) = rec.id() {
            secret_objectives.push(ObjectiveSummary {
                id: id.to_owned(),
                name: rec.text("name").unwrap_or(id).to_owned(),
                phase: rec.text("phase").unwrap_or("Status").to_owned(),
                points: rec.int("points").unwrap_or(1),
                description: rec.text("text").unwrap_or_default().to_owned(),
            });
        }
    }

    let mut public_objectives = Vec::new();
    for rec in content.records(ContentType::PublicObjectives) {
        if let Some(id) = rec.id() {
            public_objectives.push(ObjectiveSummary {
                id: id.to_owned(),
                name: rec.text("name").unwrap_or(id).to_owned(),
                phase: rec.text("phase").unwrap_or("Status").to_owned(),
                points: rec.int("points").unwrap_or(1),
                description: rec.text("text").unwrap_or_default().to_owned(),
            });
        }
    }

    Json(ContentCatalogResponse {
        strategy_cards,
        secret_objectives,
        public_objectives,
    })
}
