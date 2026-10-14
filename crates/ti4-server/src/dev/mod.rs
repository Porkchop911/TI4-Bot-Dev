//! Development-only endpoints and scenario setups for simulating authoritative engine flows.

pub mod scenarios;

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

pub use scenarios::{
    LaunchScenarioRequest, LaunchScenarioResponse, ScenarioSummary, available_scenarios,
    launch_scenario as execute_launch_scenario,
};

use crate::session::GameRegistry;

/// Handler for `GET /api/dev/scenarios`.
pub async fn list_scenarios() -> Json<Vec<ScenarioSummary>> {
    Json(available_scenarios())
}

/// Handler for `POST /api/dev/scenarios/launch`.
pub async fn launch_scenario(
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<LaunchScenarioRequest>,
) -> Result<Json<LaunchScenarioResponse>, (StatusCode, String)> {
    execute_launch_scenario(&registry, &payload.scenario_id, payload.seed)
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err))
}
