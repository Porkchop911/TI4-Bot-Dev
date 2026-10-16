//! HTTP endpoints for listing, creating, and inspecting game sessions.

use std::sync::{Arc, OnceLock};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use ti4_host::maps::MapTemplateSummary;
use ti4_host::maps::catalog::{self, MapChoice};
use ti4_host::protocol::server::ServerMessage;
use ti4_model::id::PlayerId;

use crate::session::GameRegistry;
use crate::session::registry::{GameSummary, LobbyError, PlayerLobbyView};
use crate::session::registry::{HistoryAction, HistoryError};
use ti4_host::batch::BatchRequest;

#[allow(
    clippy::result_large_err,
    reason = "a batch error is built once per rejected request and serialized to the client"
)]
pub async fn submit_batch(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(request): Json<BatchRequest>,
) -> Result<
    Json<crate::session::registry::BatchResult>,
    (StatusCode, Json<crate::session::registry::BatchError>),
> {
    let token = require_player_session(&headers).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(crate::session::registry::BatchError::explained(
                "unauthorized",
                "the x-ti4-player-session header is missing or invalid",
            )),
        )
    })?;
    let token = token.to_owned();
    tokio::task::spawn_blocking(move || registry.submit_batch(&game_id, &token, request))
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(crate::session::registry::BatchError::simple(&format!(
                    "batch worker failed: {error}"
                ))),
            )
        })?
        .map(Json)
        .map_err(|error| {
            let status = if error.reason == "unauthorized" {
                StatusCode::FORBIDDEN
            } else if error.reason == "game not found" {
                StatusCode::NOT_FOUND
            } else if error.reason.starts_with("storage error:") {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::CONFLICT
            };
            (status, Json(error))
        })
}

const MAX_PLAYERS: usize = 8;

/// Request to create a new game session.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGameRequest {
    pub player_count: usize,
    pub seed: Option<u64>,
    pub nickname: String,
    pub map_template: Option<String>,
    /// `"random"` asks for the seeded random board explicitly (no template).
    pub map: Option<String>,
    /// Opening-state preset for smoke runs (see [`ti4_host::preset`]); unknown names are a 400.
    /// Like the `/api/dev/scenarios` endpoints, this is not gated.
    pub start_preset: Option<String>,
    /// `"random"` fills every other seat with a random decider and starts the game in this
    /// request: there is no lobby to ready and start. The seats decide from the seed of the game.
    pub fill_seats: Option<String>,
    /// Strategy card set: `"te"` (default), `"pok"` or `"base_game_codex1"`; see
    /// [`ti4_host::setup::card_set`]. Anything else is a 400.
    pub strategy_card_set: Option<String>,
    /// Who opens as speaker: `"random"` (default; a seat drawn from the seed, so the same seed
    /// gives the same speaker) or `"seat"` (the first seat). Anything else is a 400.
    pub speaker_order: Option<String>,
    /// The wait before each answer of a seat that the server plays, in milliseconds (0 to
    /// 30 000). Without it the table waits as the process says, until the host sets a value.
    pub bot_delay_ms: Option<u64>,
}

/// Response after creating a game.
#[derive(Debug, Serialize)]
pub struct CreateGameResponse {
    pub game_id: String,
    /// Private to the creating client; never included in a public lobby view.
    pub player_session: String,
    pub player: PlayerIdentity,
    pub lobby: PlayerLobbyView,
}

#[derive(Debug, Serialize)]
pub struct PlayerIdentity {
    pub id: PlayerId,
}

/// Body of `POST /api/games/{game_id}/lobby/nickname`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NicknameRequest {
    pub nickname: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum JoinRequest {
    New {
        nickname: Option<String>,
    },
    Takeover {
        player_id: PlayerId,
        nickname: String,
    },
}

#[derive(Debug, Serialize)]
pub struct JoinResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_session: Option<String>,
    pub player: PlayerIdentity,
    pub lobby: PlayerLobbyView,
}

/// Handler for `GET /api/games`.
pub async fn list_games(State(registry): State<Arc<GameRegistry>>) -> Json<Vec<GameSummary>> {
    Json(registry.list_games())
}

/// Handler for `GET /api/maps`.
///
/// With `?player_count=N` only the templates that seat N and build are listed; without it every
/// template is, with `buildable` telling them apart.
pub async fn list_maps(
    Query(query): Query<MapsQuery>,
) -> Result<Json<Vec<MapTemplateSummary>>, (StatusCode, String)> {
    catalog::catalog(query.player_count)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(Debug, Deserialize)]
pub struct MapsQuery {
    pub player_count: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct MapPreviewQuery {
    pub player_count: Option<usize>,
    pub variant: Option<u32>,
}

/// Handler for `GET /api/maps/{alias}/preview?variant=K` (`alias` may be `random`, which needs
/// `player_count`). A picture of the card, built with a seed the server never reveals.
pub async fn preview_map(
    Path(alias): Path<String>,
    Query(query): Query<MapPreviewQuery>,
) -> Result<Json<ti4_host::maps::MapPreview>, (StatusCode, String)> {
    let bad = |message: String| (StatusCode::BAD_REQUEST, message);
    let (choice, count) = if alias == "random" {
        let count = query
            .player_count
            .ok_or_else(|| bad("random needs player_count".to_owned()))?;
        (MapChoice::Random, count)
    } else {
        let loader = ti4_host::maps::TemplateLoader::load()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        let template = loader
            .get(&alias)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("unknown map {alias:?}")))?;
        (MapChoice::Template { alias }, template.player_count)
    };
    if !(2..=MAX_PLAYERS).contains(&count) {
        return Err(bad("player_count must be 2-8".to_owned()));
    }
    let seed = variant_seed(&choice, count, query.variant.unwrap_or(0));
    catalog::preview(&choice, count, seed)
        .map(Json)
        .map_err(bad)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChooseMapRequest {
    pub map: ti4_host::maps::MapChoice,
    /// Dev only (like `start_preset` at creation): `\"\"` clears it. Never shown to players.
    pub start_preset: Option<String>,
}

/// Handler for `POST /api/games/{game_id}/lobby/map` (host only, before Start).
pub async fn choose_lobby_map(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<ChooseMapRequest>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?.to_owned();
    tokio::task::spawn_blocking(move || {
        registry.choose_player_lobby_map(
            &game_id,
            &token,
            &payload.map,
            payload.start_preset.as_deref(),
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(lobby_error)
}

/// Handler for `GET /api/games/{game_id}/lobby/map-preview`: the board this table's choice and
/// seat order give.
pub async fn lobby_map_preview(
    Path(game_id): Path<String>,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<ti4_host::maps::MapPreview>, (StatusCode, String)> {
    tokio::task::spawn_blocking(move || registry.player_lobby_map_preview(&game_id))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map(Json)
        .map_err(lobby_error)
}

/// Handler for `POST /api/games`.
pub async fn create_game(
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, (StatusCode, String)> {
    if !(2..=MAX_PLAYERS).contains(&payload.player_count) {
        return Err((
            StatusCode::BAD_REQUEST,
            "player_count must be 2-8".to_owned(),
        ));
    }

    let game_id = loop {
        let candidate = format!("game_{:032x}", rand::random::<u128>());
        if !registry.contains_game(&candidate) {
            break candidate;
        }
    };

    let loader = ti4_host::maps::TemplateLoader::load()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    if let Some(map) = &payload.map
        && (map != "random" || payload.map_template.is_some())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "map must be \"random\" and cannot be combined with map_template".to_owned(),
        ));
    }
    let map_template = match payload.map_template {
        _ if payload.map.is_some() => None,
        Some(alias) => {
            let template = loader.get(&alias).ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("unknown map_template {alias:?}"),
                )
            })?;
            if template.player_count != payload.player_count {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "map_template {alias:?} seats {} players, not {}",
                        template.player_count, payload.player_count
                    ),
                ));
            }
            Some(alias)
        }
        None => ti4_host::maps::default_template_for(
            ti4_content::ContentStore::embedded(),
            &loader,
            payload.player_count,
            ti4_model::content_types::POK,
        ),
    };

    if payload.start_preset.is_some() && registry.public().is_public() {
        return Err((
            StatusCode::BAD_REQUEST,
            "start_preset is not available on the public server".to_owned(),
        ));
    }
    if let Some(preset) = &payload.start_preset
        && !ti4_host::preset::is_known(preset)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unknown start_preset {preset:?}"),
        ));
    }

    let fill_random = match payload.fill_seats.as_deref() {
        None => false,
        Some("random") => true,
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown fill_seats {other:?}: only \"random\" is known"),
            ));
        }
    };

    let strategy_card_set = match payload.strategy_card_set.as_deref() {
        Some(name) => ti4_host::setup::card_set::validate(name)
            .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?,
        None => ti4_host::setup::card_set::DEFAULT,
    };

    let speaker_order = match payload.speaker_order.as_deref() {
        Some(name) => ti4_host::setup::SpeakerOrder::parse(name)
            .map_err(|error| (StatusCode::BAD_REQUEST, error))?,
        None => ti4_host::setup::SpeakerOrder::default(),
    };

    let bot_delay_ms = payload.bot_delay_ms;
    if bot_delay_ms.is_some_and(|delay| delay > crate::session::table::MAX_BOT_DELAY_MS) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "bot_delay_ms must be 0-{}",
                crate::session::table::MAX_BOT_DELAY_MS
            ),
        ));
    }

    let seed = payload.seed.unwrap_or_else(rand::random::<u64>);
    let (lobby, player, session) = if fill_random {
        // The start builds the board: not on the async runtime.
        let registry = registry.clone();
        let game_id = game_id.clone();
        tokio::task::spawn_blocking(move || {
            registry.create_player_game_with_random_seats(
                game_id,
                payload.player_count,
                seed,
                &payload.nickname,
                map_template,
                payload.start_preset,
                strategy_card_set,
                speaker_order,
                bot_delay_ms,
            )
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        registry
            .create_player_lobby_with_speaker(
                game_id.clone(),
                payload.player_count,
                seed,
                &payload.nickname,
                map_template,
                payload.start_preset,
                strategy_card_set,
                speaker_order,
            )
            .and_then(|(lobby, player, session)| {
                let lobby = match bot_delay_ms {
                    Some(delay) => registry.change_player_lobby_settings(
                        &game_id,
                        session.as_str(),
                        &crate::session::table::TableSettings {
                            bot_delay_ms: Some(delay),
                            ..crate::session::table::TableSettings::default()
                        },
                    )?,
                    None => lobby,
                };
                Ok((lobby, player, session))
            })
    }
    .map_err(lobby_error)?;

    Ok(Json(CreateGameResponse {
        game_id,
        player_session: session.as_str().to_owned(),
        player: PlayerIdentity { id: player },
        lobby,
    }))
}

/// One entry point for new admissions, credential reconnects and takeover.
pub async fn join_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<JoinRequest>,
) -> Result<Json<JoinResponse>, (StatusCode, String)> {
    let supplied = player_session(&headers)?;
    let (lobby, player, session) = match (payload, supplied) {
        (JoinRequest::New { nickname }, token) => registry
            .join_player_lobby(&game_id, token, nickname.as_deref())
            .map_err(lobby_error)?,
        (
            JoinRequest::Takeover {
                player_id,
                nickname,
            },
            None,
        ) => {
            let (lobby, session) = registry
                .take_over_player(&game_id, &player_id, &nickname)
                .map_err(lobby_error)?;
            (lobby, player_id, Some(session))
        }
        (JoinRequest::Takeover { .. }, Some(_)) => {
            return Err(lobby_error(LobbyError::InvalidCapability));
        }
    };
    Ok(Json(JoinResponse {
        player_session: session.map(|value| value.as_str().to_owned()),
        player: PlayerIdentity { id: player },
        lobby,
    }))
}

/// Authenticated lobby heartbeat; does not renew or rotate the credential.
pub async fn heartbeat(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    Ok(Json(
        registry
            .player_heartbeat(&game_id, token)
            .map_err(lobby_error)?,
    ))
}

/// Handler for `GET /api/games/{game_id}/lobby`.
pub async fn get_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    Ok(Json(
        registry
            .player_lobby_status(&game_id, player_session(&headers)?)
            .map_err(lobby_error)?
            .0,
    ))
}

/// `POST /api/games/{game_id}/terminate`: the host ends the game (lobby or started).
pub async fn terminate_game(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let token = require_player_session(&headers)?.to_owned();
    tokio::task::spawn_blocking(move || registry.terminate_game(&game_id, &token))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(lobby_error)?;
    Ok(Json(serde_json::json!({ "terminated": true })))
}

/// Retire an authenticated, non-host lobby participant.
pub async fn leave_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    Ok(Json(
        registry
            .leave_player_lobby(&game_id, require_player_session(&headers)?)
            .map_err(lobby_error)?,
    ))
}

/// Handler for `POST /api/games/{game_id}/lobby/settings` (host only, before Start): the player
/// count, the set of strategy cards, the wait of the seats that the server plays, and the
/// scenario that the game starts from. A field that is missing stays as it is.
pub async fn change_lobby_settings(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<crate::session::table::TableSettings>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?.to_owned();
    // A new count or a scenario builds a board to check it: not on the async runtime.
    tokio::task::spawn_blocking(move || {
        registry.change_player_lobby_settings(&game_id, &token, &payload)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
    .map_err(lobby_error)
}

/// Body of `POST /api/games/{game_id}/settings`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunningSettings {
    /// 0 to [`crate::session::table::MAX_BOT_DELAY_MS`]. Missing: the wait stays as it is.
    pub bot_delay_ms: Option<u64>,
}

/// Handler for `POST /api/games/{game_id}/settings` (host only, for a running game): the wait of
/// the seats that the server plays. It is the one value of a table that changes after Start.
pub async fn change_running_settings(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<RunningSettings>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    Ok(Json(
        match payload.bot_delay_ms {
            Some(delay) => registry.change_running_bot_delay(&game_id, token, delay),
            None => registry
                .player_lobby_status(&game_id, Some(token))
                .map(|(view, _)| view),
        }
        .map_err(lobby_error)?,
    ))
}

/// Handler for `POST /api/games/{game_id}/lobby/nickname`: the player of the credential takes
/// another name, before Start.
pub async fn rename_in_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<NicknameRequest>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    Ok(Json(
        registry
            .rename_player(
                &game_id,
                require_player_session(&headers)?,
                &payload.nickname,
            )
            .map_err(lobby_error)?,
    ))
}

/// Handler for `GET /api/setup-options`: what a host can choose for a table, with the names.
pub async fn setup_options(
    State(registry): State<Arc<GameRegistry>>,
) -> Json<crate::session::table::SetupOptions> {
    Json(crate::session::table::setup_options(
        registry.public().is_public(),
    ))
}

/// Handler for `POST /api/games/{game_id}/lobby/start`.
pub async fn start_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    Ok(Json(
        registry
            .start_player_lobby(&game_id, require_player_session(&headers)?)
            .map_err(lobby_error)?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveBotRequest {
    pub player_id: PlayerId,
}

/// Body of `POST /api/games/{game_id}/lobby/bots`.
///
/// `kind` is `random` or `smart` (in-process, authorized by the host's own credential, no password). With
/// `fill: true` every open slot gets a bot; otherwise `count` bots (default 1) are added.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddBotsRequest {
    pub kind: String,
    pub nickname: Option<String>,
    pub count: Option<usize>,
    #[serde(default)]
    pub fill: bool,
    /// The faction for the one bot being added (`None`: the seed deals it at start).
    #[serde(default)]
    pub faction: Option<String>,
}

/// Body of `POST /api/games/{game_id}/lobby/seat-faction`: `faction: null` goes back to random.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetSeatFactionRequest {
    pub player_id: PlayerId,
    #[serde(default)]
    pub faction: Option<String>,
}

/// Handler for `POST /api/games/{game_id}/lobby/bots`.
pub async fn add_bots_to_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<AddBotsRequest>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    if !crate::session::registry::is_in_process_bot_kind(&payload.kind) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown bot kind '{}': the kinds that can be added here are {}",
                payload.kind,
                crate::session::registry::in_process_bot_kinds().join(", ")
            ),
        ));
    }
    if payload.fill && payload.count.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Use either fill or count, not both".to_owned(),
        ));
    }
    let count = if payload.fill {
        None
    } else {
        Some(payload.count.unwrap_or(1))
    };
    Ok(Json(
        registry
            .add_in_process_bots(
                &game_id,
                token,
                &payload.kind,
                payload.nickname,
                count,
                payload.faction.as_deref(),
            )
            .map_err(lobby_error)?,
    ))
}

/// Handler for `POST /api/games/{game_id}/lobby/remove-bot`.
pub async fn remove_bot_from_lobby(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<RemoveBotRequest>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    Ok(Json(
        registry
            .remove_bot_or_player_from_lobby(&game_id, token, &payload.player_id)
            .map_err(lobby_error)?,
    ))
}

/// Handler for `POST /api/games/{game_id}/lobby/seat-faction`: the host for any seat, a person for
/// their own.
pub async fn set_lobby_seat_faction(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<SetSeatFactionRequest>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    Ok(Json(
        registry
            .set_seat_faction(
                &game_id,
                token,
                &payload.player_id,
                payload.faction.as_deref(),
            )
            .map_err(lobby_error)?,
    ))
}

/// Handler for `POST /api/games/{game_id}/lobby/shuffle-seats`: the host puts the players of
/// the table in another order of its rows, which are the seats of the game.
pub async fn shuffle_lobby_seats(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<PlayerLobbyView>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    Ok(Json(
        registry
            .shuffle_seats(&game_id, token)
            .map_err(lobby_error)?,
    ))
}

fn player_session(headers: &HeaderMap) -> Result<Option<&str>, (StatusCode, String)> {
    headers
        .get("x-ti4-player-session")
        .map(|header| {
            header
                .to_str()
                .map_err(|_| lobby_error(LobbyError::InvalidCapability))
        })
        .transpose()
}

fn require_player_session(headers: &HeaderMap) -> Result<&str, (StatusCode, String)> {
    player_session(headers)?.ok_or_else(|| lobby_error(LobbyError::InvalidCapability))
}

fn lobby_error(error: LobbyError) -> (StatusCode, String) {
    let message = error.message();
    let status = match error {
        LobbyError::NotFound => StatusCode::NOT_FOUND,
        LobbyError::FactionTaken(_) => StatusCode::CONFLICT,
        LobbyError::InvalidFaction(_) => StatusCode::BAD_REQUEST,
        LobbyError::InvalidCapability | LobbyError::HostRequired => StatusCode::FORBIDDEN,
        LobbyError::SeatsOpen
        | LobbyError::SeatsTaken(_)
        | LobbyError::ScenarioTable(_)
        | LobbyError::AlreadyRunning
        | LobbyError::NotRunning
        | LobbyError::NotInLobby
        | LobbyError::SeatUnavailable
        | LobbyError::TakeoverUnavailable => StatusCode::CONFLICT,
        LobbyError::InvalidPlayerId
        | LobbyError::InvalidSetting(_)
        | LobbyError::InvalidMap(_)
        | LobbyError::InvalidNickname => StatusCode::BAD_REQUEST,
        LobbyError::ModelMissing(_) => StatusCode::CONFLICT,
        LobbyError::Map(_) | LobbyError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, message)
}

/// Handler for `GET /api/games/{game_id}/map`.
pub async fn get_map(
    Path(game_id): Path<String>,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<Vec<ti4_host::protocol::view::BoardTileView>>, (StatusCode, String)> {
    let session = registry
        .get_game(&game_id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Game '{game_id}' not found")))?;

    Ok(Json(session.map_tiles()))
}

/// Handler for `GET /api/games/{game_id}/replay`: the game's history for copying out.
///
/// Any seated player of the game may fetch it. The body wraps the same JSON a `history.json`
/// holds (`history`) with the seed and seats needed to replay it; it includes every player's
/// decisions and the seed, so it is not a spectator view.
pub async fn get_replay(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let token = require_player_session(&headers)?.to_owned();
    tokio::task::spawn_blocking(move || {
        let session = registry
            .get_game(&game_id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Game '{game_id}' not found")))?;
        registry
            .authenticate_player_session(&game_id, &token)
            .map_err(lobby_error)?;
        let (history, seed, player_ids) = session.replay_export();
        let map_template = registry
            .store()
            .and_then(|store| store.load_player_init(&game_id).ok())
            .and_then(|init| init.map_template);
        Ok(Json(serde_json::json!({
            "format": "ti4-replay",
            "version": 1,
            "game_id": game_id,
            "seed": seed,
            "player_ids": player_ids,
            "map_template": map_template,
            "history": history,
        })))
    })
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("replay export failed: {error}"),
        )
    })?
}

/// Handler for `GET /api/games/{game_id}/snapshot`.
pub async fn get_snapshot(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<ServerMessage>, (StatusCode, String)> {
    // Checked where it always was (after the game lookup), so the error precedence is unchanged.
    let credential = player_session(&headers).map(|token| token.map(str::to_owned));
    // The registry and session locks are blocking mutexes; waiting on them on a runtime worker
    // stalls every other request scheduled there.
    tokio::task::spawn_blocking(move || {
        let session = registry
            .get_game(&game_id)
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Game '{game_id}' not found")))?;

        if session.error().is_some() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "Game session failed closed: {}",
                    session.error().unwrap_or_default()
                ),
            ));
        }

        Ok(Json(ServerMessage::InitialSnapshot(
            registry
                .player_snapshot(&game_id, credential?.as_deref(), &session)
                .map_err(lobby_error)?,
        )))
    })
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Snapshot worker failed: {error}"),
        )
    })?
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeHistoryRequest {
    pub expected_version: u64,
    pub action: String,
    pub event_id: Option<String>,
    pub cursor: Option<usize>,
}

/// Host-only authoritative rewind/redo. A changed session forces existing WS clients to
/// reconnect and fetch a replacement snapshot rather than applying stale delta messages.
pub async fn change_history(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<ChangeHistoryRequest>,
) -> Result<Json<ServerMessage>, (StatusCode, String)> {
    let token = require_player_session(&headers)?;
    let action = match (payload.action.as_str(), payload.event_id, payload.cursor) {
        ("undo", None, None) => HistoryAction::Undo,
        ("undo_batch", None, None) => HistoryAction::UndoBatch,
        ("undo_pipeline", None, None) => HistoryAction::UndoPipeline,
        ("redo", None, None) => HistoryAction::Redo,
        ("redo_batch", None, None) => HistoryAction::RedoBatch,
        ("redo_pipeline", None, None) => HistoryAction::RedoPipeline,
        ("restore", Some(event_id), None) => HistoryAction::Restore { event_id },
        ("restore_cursor", None, Some(cursor)) => HistoryAction::RestoreCursor { cursor },
        (action, event_id, cursor) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Invalid history action '{action}' (event_id {}, cursor {}): use undo, undo_batch, undo_pipeline, redo, redo_batch or redo_pipeline without arguments, restore with event_id, or restore_cursor with cursor",
                    if event_id.is_some() {
                        "given"
                    } else {
                        "absent"
                    },
                    if cursor.is_some() { "given" } else { "absent" },
                ),
            ));
        }
    };
    let token = token.to_owned();
    let snapshot = tokio::task::spawn_blocking(move || {
        registry.change_history(&game_id, &token, payload.expected_version, action)
    })
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("History worker failed: {error}"),
        )
    })?
    .map_err(|error| {
        let status = match error {
            HistoryError::NotFound => StatusCode::NOT_FOUND,
            HistoryError::Forbidden(_) => StatusCode::FORBIDDEN,
            HistoryError::InvalidTarget(_) => StatusCode::BAD_REQUEST,
            HistoryError::Conflict(_) => StatusCode::CONFLICT,
            HistoryError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, error.message())
    })?;
    Ok(Json(ServerMessage::InitialSnapshot(snapshot)))
}

/// Host-only, read-only dry run of removing or changing one past decision.
///
/// `POST /api/games/{game_id}/history/splice-preview`. Nothing in the game, its history or its
/// storage changes; the response says how many later decisions would still fit and where the
/// first one that does not is.
pub async fn splice_preview(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<ti4_host::protocol::splice::SplicePreviewRequest>,
) -> Result<Json<ti4_host::protocol::splice::SplicePreview>, (StatusCode, String)> {
    let token = require_player_session(&headers)?.to_owned();
    let preview = tokio::task::spawn_blocking(move || {
        registry.splice_preview(&game_id, &token, &payload.edit)
    })
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Splice preview worker failed: {error}"),
        )
    })?
    .map_err(|error| {
        let status = match error {
            HistoryError::NotFound => StatusCode::NOT_FOUND,
            HistoryError::Forbidden(_) => StatusCode::FORBIDDEN,
            HistoryError::InvalidTarget(_) => StatusCode::BAD_REQUEST,
            HistoryError::Conflict(_) => StatusCode::CONFLICT,
            HistoryError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, error.message())
    })?;
    Ok(Json(preview))
}

fn history_status_code(error: &HistoryError) -> StatusCode {
    match error {
        HistoryError::NotFound => StatusCode::NOT_FOUND,
        HistoryError::Forbidden(_) => StatusCode::FORBIDDEN,
        HistoryError::InvalidTarget(_) => StatusCode::BAD_REQUEST,
        HistoryError::Conflict(_) => StatusCode::CONFLICT,
        HistoryError::Storage(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Where a turn redo stands (`null` status when none is in flight). Any authenticated player.
///
/// `GET /api/games/{game_id}/turn-redo`.
pub async fn turn_redo_status(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
) -> Result<Json<ti4_host::protocol::turn_redo::TurnRedoStatusResponse>, (StatusCode, String)> {
    let token = require_player_session(&headers)?.to_owned();
    let status = tokio::task::spawn_blocking(move || registry.turn_redo_status(&game_id, &token))
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Turn redo worker failed: {error}"),
            )
        })?
        .map_err(|error| (history_status_code(&error), error.message()))?;
    Ok(Json(
        ti4_host::protocol::turn_redo::TurnRedoStatusResponse { status },
    ))
}

/// Turn redo: request (rewind my last turn), auto-play the round after the new turn, restore the
/// original timeline, or keep the new one.
///
/// `POST /api/games/{game_id}/turn-redo` with `{expected_version, action: ...}`. Every action but
/// `keep` answers with the replacement snapshot like the history endpoint, so clients resync;
/// `keep` answers `{"kept": true}`.
pub async fn turn_redo(
    Path(game_id): Path<String>,
    headers: HeaderMap,
    State(registry): State<Arc<GameRegistry>>,
    Json(payload): Json<ti4_host::protocol::turn_redo::TurnRedoRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use ti4_host::protocol::turn_redo::TurnRedoCommand;
    let token = require_player_session(&headers)?.to_owned();
    let version = payload.expected_version;
    let outcome = tokio::task::spawn_blocking(move || match payload.command {
        TurnRedoCommand::Request { seat, turns } => registry
            .turn_redo_request(&game_id, &token, version, seat.as_deref(), turns)
            .map(Some),
        TurnRedoCommand::Autoplay => registry
            .turn_redo_autoplay(&game_id, &token, version)
            .map(Some),
        TurnRedoCommand::Restore => registry
            .turn_redo_restore(&game_id, &token, version)
            .map(Some),
        TurnRedoCommand::Keep => registry.turn_redo_keep(&game_id, &token).map(|()| None),
    })
    .await
    .map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Turn redo worker failed: {error}"),
        )
    })?
    .map_err(|error| (history_status_code(&error), error.message()))?;
    let body = match outcome {
        Some(snapshot) => serde_json::to_value(ServerMessage::InitialSnapshot(snapshot))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        None => serde_json::json!({ "kept": true }),
    };
    Ok(Json(body))
}

#[cfg(test)]
mod template_request_tests {
    use super::*;

    fn request(count: usize, template: Option<&str>) -> Json<CreateGameRequest> {
        Json(CreateGameRequest {
            player_count: count,
            seed: Some(1),
            nickname: "Host".to_owned(),
            map_template: template.map(str::to_owned),
            map: None,
            start_preset: None,
            fill_seats: None,
            strategy_card_set: None,
            speaker_order: None,
            bot_delay_ms: None,
        })
    }

    fn preset_request(count: usize, preset: &str) -> Json<CreateGameRequest> {
        Json(CreateGameRequest {
            player_count: count,
            seed: Some(1),
            nickname: "Host".to_owned(),
            map_template: None,
            map: None,
            start_preset: Some(preset.to_owned()),
            fill_seats: None,
            strategy_card_set: None,
            speaker_order: None,
            bot_delay_ms: None,
        })
    }

    #[tokio::test]
    async fn fill_seats_starts_the_game_with_random_seats_and_one_human() {
        use crate::session::SeatController;
        // No grace: a seat that never connected could be taken over at once.
        let registry = Arc::new(GameRegistry::new().with_presence_grace(std::time::Duration::ZERO));
        let filled = |fill: &str| {
            let mut request = request(8, None);
            request.0.fill_seats = Some(fill.to_owned());
            request
        };
        let unknown = create_game(State(registry.clone()), filled("bots")).await;
        assert_eq!(unknown.unwrap_err().0, StatusCode::BAD_REQUEST);

        let created = create_game(State(registry.clone()), filled("random"))
            .await
            .expect("a started game")
            .0;
        assert!(matches!(
            created.lobby.phase,
            crate::session::registry::LobbyPhase::Running
        ));
        assert!(
            created
                .lobby
                .slots
                .iter()
                .all(|slot| slot.occupant.is_some())
        );
        let session = registry.get_game(&created.game_id).expect("the game runs");
        let (order, seats, _, seed) = session.lobby_details();
        assert_eq!((order.len(), seed), (8, Some(1)));
        // The seats are drawn from the seed: the slots of the running table are the seats.
        assert!(order.contains(&created.player.id));
        assert_eq!(
            created
                .lobby
                .slots
                .iter()
                .filter_map(|slot| slot.occupant.clone())
                .collect::<Vec<_>>(),
            order
        );
        for (seat, controller) in &seats {
            let expected = if *seat == created.player.id {
                SeatController::Human
            } else {
                SeatController::RANDOM
            };
            assert_eq!(*controller, expected, "{seat}");
        }
        // The credential of the response is the one of the human seat.
        assert_eq!(
            session.viewer_for_seat_token(&created.player_session),
            Some(ti4_host::protocol::status::ViewerRole::Player(
                created.player.id.clone()
            ))
        );
        // A seat that the server plays cannot be taken over.
        let random = order
            .iter()
            .find(|seat| **seat != created.player.id)
            .expect("a seat that the server plays")
            .clone();
        assert_eq!(
            registry
                .take_over_player(&created.game_id, &random, "Guest")
                .map(|_| ())
                .unwrap_err(),
            crate::session::registry::LobbyError::TakeoverUnavailable
        );
    }

    #[tokio::test]
    async fn an_unknown_start_preset_is_a_400_and_a_known_one_is_accepted() {
        let registry = Arc::new(GameRegistry::new());
        let unknown = create_game(State(registry.clone()), preset_request(3, "nope")).await;
        assert_eq!(unknown.unwrap_err().0, StatusCode::BAD_REQUEST);
        assert!(
            create_game(State(registry), preset_request(3, ti4_host::preset::COMBAT))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn an_unknown_or_mismatched_template_is_a_400() {
        let registry = Arc::new(GameRegistry::new());
        let unknown = create_game(State(registry.clone()), request(6, Some("nope"))).await;
        assert_eq!(unknown.unwrap_err().0, StatusCode::BAD_REQUEST);
        let mismatch = create_game(State(registry), request(4, Some("6pStandard"))).await;
        assert_eq!(mismatch.unwrap_err().0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn a_named_or_default_template_is_accepted() {
        let registry = Arc::new(GameRegistry::new());
        assert!(
            create_game(State(registry.clone()), request(6, Some("6pBeMyNeighbor")))
                .await
                .is_ok()
        );
        assert!(create_game(State(registry), request(6, None)).await.is_ok());
    }
}

/// A repeatable seed for the picture of variant `variant` of a card. Mixed with a per-process
/// secret so the numbers cannot be walked to learn anything about a real game's seed.
fn variant_seed(choice: &MapChoice, player_count: usize, variant: u32) -> u64 {
    static SALT: OnceLock<u64> = OnceLock::new();
    let salt = *SALT.get_or_init(rand::random);
    catalog::variant_seed_salted(salt, choice, player_count, variant)
}
