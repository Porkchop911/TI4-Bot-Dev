//! Development-only endpoints: the scenarios of `ti4_host::view::scenario`, listed and started as
//! tables of this server (`/api/dev/scenarios`, `/api/dev/scenarios/launch`). Only the HTTP glue
//! is here: a launch is a lobby that starts from the scenario, as any table can.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_engine::seating;
use ti4_host::setup::bot_kind;
use ti4_host::view::scenario::{self, SCENARIOS, Scenario};

use crate::session::GameRegistry;
use crate::session::table::TableSettings;

/// How long a launch waits for the opening of its scenario.
const OPENING_TIMEOUT: Duration = Duration::from_secs(30);

/// A scenario as the dev launcher of `web/` lists it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSummary {
    pub id: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub player_count: usize,
    /// The faction of the seat that the launch gives the caller.
    pub human_faction: String,
    pub opponent_factions: Vec<String>,
}

/// Request to launch a scenario.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchScenarioRequest {
    pub scenario_id: String,
    /// The seed of the table; a scenario of a fixed board plays its own.
    pub seed: Option<u64>,
}

/// What a launch returns: the game, and the seat of the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchScenarioResponse {
    pub game_id: String,
    /// The credential of the first seat that people play.
    pub player_session: String,
    pub player_id: String,
    pub scenario_id: String,
    /// The credential of every seat that people play, by player id (for the e2e tests that open
    /// several views).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_seats: Option<BTreeMap<String, String>>,
}

/// The printed name of the faction at the place `place` of a scenario.
fn faction_name(place: usize) -> String {
    let faction = seating::seat_faction(place);
    ti4_content::factions::get(ContentStore::embedded(), faction)
        .and_then(|record| record.name().map(str::to_owned))
        .unwrap_or_else(|| faction.to_owned())
}

fn summary(scenario: &Scenario) -> ScenarioSummary {
    let caller = scenario.humans.first().copied().unwrap_or(0);
    ScenarioSummary {
        id: scenario.id.to_owned(),
        title: scenario.title.to_owned(),
        category: scenario.category.to_owned(),
        description: scenario.description.to_owned(),
        player_count: scenario.players,
        human_faction: faction_name(caller),
        opponent_factions: (0..scenario.players)
            .filter(|place| *place != caller)
            .map(faction_name)
            .collect(),
    }
}

/// Every scenario of `ti4_host::view::scenario`.
#[must_use]
pub fn available_scenarios() -> Vec<ScenarioSummary> {
    SCENARIOS.iter().map(summary).collect()
}

/// Starts a table of this server from the scenario `scenario_id`: a lobby with a person in each
/// seat of [`Scenario::humans`] and the rule bot in the others (no wait), started from the
/// scenario. Returns once the game asks the first of these people, after the opening of the
/// scenario.
///
/// # Errors
/// A message for an unknown scenario, a table the lobby refuses, or a game that fails or does not
/// reach its question.
pub fn launch_scenario(
    registry: &Arc<GameRegistry>,
    scenario_id: &str,
    seed: Option<u64>,
) -> Result<LaunchScenarioResponse, String> {
    let scenario = scenario::find(scenario_id).ok_or_else(|| {
        let known: Vec<&str> = SCENARIOS.iter().map(|scenario| scenario.id).collect();
        format!(
            "Unknown scenario '{scenario_id}': the scenarios are {}",
            known.join(", ")
        )
    })?;
    let seed = seed.unwrap_or_else(rand::random::<u64>);
    let game_id = format!(
        "dev_{}_{:016x}",
        scenario.id.replace('-', "_"),
        rand::random::<u64>()
    );
    let lobby_error = |error: crate::session::registry::LobbyError| error.message();
    let nickname = |place: usize| format!("Player {} ({})", place + 1, faction_name(place));
    let host_place = scenario.humans.first().copied().unwrap_or(0);
    if host_place != 0 {
        return Err(format!(
            "the scenario '{}' has no person in its first seat",
            scenario.id
        ));
    }
    let (_, host, host_session) = registry
        .create_player_lobby(game_id.clone(), scenario.players, seed, &nickname(0))
        .map_err(lobby_error)?;
    let credential = host_session.as_str().to_owned();
    let mut people: BTreeMap<String, String> =
        BTreeMap::from([(host.to_string(), credential.clone())]);
    // The slots fill in their order, so each place gets its person or its bot.
    for place in 1..scenario.players {
        if scenario.humans.contains(&place) {
            let (_, player, session) = registry
                .join_player_lobby(&game_id, None, Some(&nickname(place)))
                .map_err(lobby_error)?;
            let session = session.ok_or("a new player has a session")?;
            people.insert(player.to_string(), session.as_str().to_owned());
        } else {
            registry
                .add_in_process_bots(&game_id, &credential, bot_kind::RULE, None, Some(1), None)
                .map_err(lobby_error)?;
        }
    }
    registry
        .change_player_lobby_settings(
            &game_id,
            &credential,
            &TableSettings {
                bot_delay_ms: Some(0),
                start_from: Some(Some(scenario.id.to_owned())),
                ..TableSettings::default()
            },
        )
        .map_err(lobby_error)?;
    registry
        .start_player_lobby(&game_id, &credential)
        .map_err(lobby_error)?;
    let session = registry
        .get_game(&game_id)
        .ok_or("the started game is not in the registry")?;
    session.wait_until_asked(&host, OPENING_TIMEOUT)?;
    Ok(LaunchScenarioResponse {
        game_id,
        player_session: credential,
        player_id: host.to_string(),
        scenario_id: scenario.id.to_owned(),
        test_seats: Some(people),
    })
}

/// Handler for `GET /api/dev/scenarios`.
pub async fn list_scenarios() -> Json<Vec<ScenarioSummary>> {
    Json(available_scenarios())
}

/// Handler for `POST /api/dev/scenarios/launch`.
pub async fn post_launch(
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<LaunchScenarioRequest>,
) -> Result<Json<LaunchScenarioResponse>, (StatusCode, String)> {
    // The launch waits for the opening of the scenario: not on the async runtime.
    tokio::task::spawn_blocking(move || {
        launch_scenario(&registry, &payload.scenario_id, payload.seed)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map(Json)
    .map_err(|err| (StatusCode::BAD_REQUEST, err))
}
