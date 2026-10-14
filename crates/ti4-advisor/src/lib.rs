//! Stateless, loopback-only MLP advice and battle odds for one redacted game snapshot.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post};
use serde::{Deserialize, Serialize};
use ti4_calc::odds::{self, CalcError, SpaceOptions};
pub use ti4_calc::odds::{
    BattleResponse, GroundOddsRequest, GroundOddsResponse, GroundOddsSide, ParticipantInput,
    SideInput, UnitCounts,
};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::laws;
use ti4_mlp::{Actor, CriticInput, FactionRow, SparseOption};
use ti4_model::content_types::{POK, Source, SourceSet};
use ti4_model::hex::Hex;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_policy::critic::{CriticFeatures, critic_vector};
use ti4_policy::progress::Baseline;
use ti4_policy::vocabulary::Vocabulary;
use ti4_server::map::GalaxyLayout;

const MAX_OPTIONS: usize = 512;
const MAX_PLACEMENTS: usize = odds::MAX_PLACEMENTS;
const MAX_OFF_MAP_SYSTEMS: usize = 64;
const MAX_SOURCES: usize = 7;
const DEFAULT_TEMPERATURE: f64 = 0.25;
const MAX_BODY_BYTES: usize = 1_048_576;

/// A request to evaluate space combat using `ti4_calc` rollouts.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattleRequest {
    /// An optional [`GameState`] snapshot.
    #[serde(default)]
    pub state: Option<GameState>,
    /// Versioned layout used to reconstruct galaxy adjacency for deep space cannon.
    #[serde(default)]
    pub galaxy_layout: Option<GalaxyLayout>,
    /// System ID where combat takes place.
    #[serde(default)]
    pub system: Option<SystemId>,
    /// Attacker specification (player ID or fleet description).
    pub attacker: ParticipantInput,
    /// Defender specification (player ID or fleet description).
    pub defender: ParticipantInput,
    /// Number of rollout simulations (defaults to 2000, capped at 50000).
    #[serde(default)]
    pub simulations: Option<usize>,
    /// Whether attacker's space cannon fires before combat (defaults to true).
    #[serde(default)]
    pub attacker_cannon: Option<bool>,
    /// Whether combat is already in progress (bypasses space cannon and round-1 AFB).
    #[serde(default)]
    pub in_progress: Option<bool>,
    /// Optional seed for deterministic simulations.
    #[serde(default)]
    pub seed: Option<u64>,
}

/// A parsed request to evaluate a single legal engine choice.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    pub state: GameState,
    pub galaxy_layout: GalaxyLayout,
    pub player: PlayerId,
    pub choice: Choice,
    #[serde(default)]
    pub temperature: Option<f64>,
}

/// Advice for one option in the request's original stable order.
#[derive(Debug, Serialize)]
pub struct OptionAdvice {
    pub option_id: String,
    pub probability: f64,
    pub logit: f64,
}

/// MLP output for one choice. `value` is the raw critic output, not a win probability.
#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub head: String,
    pub value: f64,
    pub options: Vec<OptionAdvice>,
}

#[derive(Debug)]
pub struct ApiError(pub String);

impl ApiError {
    #[must_use]
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<CalcError> for ApiError {
    fn from(error: CalcError) -> Self {
        Self(error.0)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": self.0 })),
        )
            .into_response()
    }
}

/// Preloaded model and content for a stateless evaluator.
#[derive(Clone)]
pub struct Advisor {
    inner: Arc<Inner>,
}

struct Inner {
    content: &'static ContentStore,
    model: Mutex<Option<Model>>,
}

struct Model {
    actor: Actor,
    vocabulary: Vocabulary,
}

impl Advisor {
    /// Load a fully validated bundle before accepting requests.
    ///
    /// # Errors
    ///
    /// Returns an error message if the checkpoint cannot be read or validated.
    pub fn load(checkpoint: &std::path::Path) -> Result<Self, String> {
        let loaded = ti4_mlp::bundle::read(checkpoint).map_err(|error| error.to_string())?;
        Ok(Self::from_parts(loaded.actor, loaded.vocabulary))
    }

    fn from_parts(actor: Actor, vocabulary: Vocabulary) -> Self {
        Self {
            inner: Arc::new(Inner {
                content: ContentStore::embedded(),
                model: Mutex::new(Some(Model { actor, vocabulary })),
            }),
        }
    }

    /// Advisor instance initialized for battle calculations without requiring an MLP checkpoint.
    #[must_use]
    pub fn battle_only() -> Self {
        Self {
            inner: Arc::new(Inner {
                content: ContentStore::embedded(),
                model: Mutex::new(None),
            }),
        }
    }

    /// Build the loopback HTTP application. The request body limit bounds malformed JSON work.
    #[must_use]
    pub fn router(self) -> Router {
        Router::new()
            .route("/evaluate", post(evaluate))
            .route("/battle", post(battle))
            .route("/battle_odds", post(battle))
            .route("/ground_odds", post(ground_odds))
            .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(self)
    }

    fn evaluate(&self, request: &EvaluateRequest) -> Result<EvaluateResponse, ApiError> {
        let sources = validate_request(request)?;
        let mut galaxy = reconstruct_galaxy(self.inner.content, &request.galaxy_layout, sources)?;
        laws::apply_to_galaxy(&request.state, &mut galaxy);
        let player = request
            .state
            .player(&request.player)
            .ok_or_else(|| ApiError::bad_request("player is not present in state"))?;
        let row = FactionRow::of(player.faction.as_str())
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        let temperature = request.temperature.unwrap_or(DEFAULT_TEMPERATURE);
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(ApiError::bad_request(
                "temperature must be finite and positive",
            ));
        }

        let model_guard = self
            .inner
            .model
            .lock()
            .map_err(|_| ApiError::bad_request("model lock poisoned"))?;
        let model = model_guard
            .as_ref()
            .ok_or_else(|| ApiError::bad_request("model checkpoint is not loaded"))?;
        let mut decider = Evaluator {
            actor: &model.actor,
            vocabulary: &model.vocabulary,
            row,
            temperature,
            result: None,
        };
        ti4_engine::choice::ask_private(
            &request.choice,
            &request.state,
            self.inner.content,
            sources,
            Some(&galaxy),
            &mut decider,
        )
        .map_err(|error| ApiError::bad_request(format!("cannot evaluate choice: {error}")))?;
        decider
            .result
            .ok_or_else(|| ApiError::bad_request("evaluation produced no result"))
    }

    /// Evaluate space combat odds using `ti4_calc` rollouts.
    ///
    /// # Errors
    ///
    /// Returns an [`ApiError`] if parameters, factions, or unit IDs are invalid.
    pub fn battle(&self, request: &BattleRequest) -> Result<BattleResponse, ApiError> {
        let sources = if let Some(ref layout) = request.galaxy_layout {
            if layout.active_sources.is_empty() || layout.active_sources.len() > MAX_SOURCES {
                return Err(ApiError::bad_request(
                    "active_sources must contain between 1 and 7 sources",
                ));
            }
            parse_sources(&layout.active_sources)?
        } else {
            POK
        };

        let galaxy = if let Some(ref layout) = request.galaxy_layout {
            Some(reconstruct_galaxy(self.inner.content, layout, sources)?)
        } else {
            None
        };

        let (attacker_side, attacker_fielded) = odds::resolve_side(
            &request.attacker,
            request.state.as_ref(),
            request.system.as_ref(),
            galaxy.as_ref(),
            self.inner.content,
            sources,
        )?;

        let (defender_side, defender_fielded) = odds::resolve_side(
            &request.defender,
            request.state.as_ref(),
            request.system.as_ref(),
            galaxy.as_ref(),
            self.inner.content,
            sources,
        )?;

        Ok(odds::space_odds(
            (attacker_side, attacker_fielded),
            (defender_side, defender_fielded),
            SpaceOptions {
                simulations: request.simulations,
                attacker_cannon: request.attacker_cannon,
                in_progress: request.in_progress,
                seed: request.seed,
            },
        )?)
    }

    /// Evaluate ground combat odds using `ti4_calc` rollouts.
    ///
    /// # Errors
    ///
    /// Returns an [`ApiError`] if counts, unit IDs, or the number of simulations are invalid.
    pub fn ground_odds(&self, request: &GroundOddsRequest) -> Result<GroundOddsResponse, ApiError> {
        Ok(odds::ground_odds(self.inner.content, request)?)
    }
}

async fn evaluate(
    State(advisor): State<Advisor>,
    Json(request): Json<EvaluateRequest>,
) -> Result<Json<EvaluateResponse>, ApiError> {
    advisor.evaluate(&request).map(Json)
}

async fn battle(
    State(advisor): State<Advisor>,
    Json(request): Json<BattleRequest>,
) -> Result<Json<BattleResponse>, ApiError> {
    advisor.battle(&request).map(Json)
}

async fn ground_odds(
    State(advisor): State<Advisor>,
    Json(request): Json<GroundOddsRequest>,
) -> Result<Json<GroundOddsResponse>, ApiError> {
    advisor.ground_odds(&request).map(Json)
}

struct Evaluator<'a> {
    actor: &'a Actor,
    vocabulary: &'a Vocabulary,
    row: FactionRow,
    temperature: f64,
    result: Option<EvaluateResponse>,
}

impl Decider for Evaluator<'_> {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        choice
            .options
            .first()
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.result = Some(score_choice(
            self.actor,
            self.vocabulary,
            self.row,
            self.temperature,
            choice,
            seen,
        )?);
        self.choose(choice)
    }
}

fn score_choice(
    actor: &Actor,
    vocabulary: &Vocabulary,
    row: FactionRow,
    temperature: f64,
    choice: &Choice,
    seen: &SeatObservation<'_>,
) -> Result<EvaluateResponse, IllegalChoice> {
    let held = seen.held_secret_progress();
    let vectors = ti4_policy::projection::mlp_choice_features(
        seen.observed(),
        choice,
        &choice.player,
        &held,
        Baseline::default(),
    );
    let options: Result<Vec<_>, _> = vectors
        .iter()
        .map(|vector| sparse(vector, vocabulary))
        .collect();
    let options = options.map_err(|reason| refused(choice, reason))?;
    let requested = ti4_policy::learned::decision_head(choice);
    let head = actor.resolve_layout_head(requested);
    let logits = actor
        .logits(&options, head, row)
        .map_err(|error| refused(choice, error.to_string()))?;
    let logits = ti4_tensor::to_vec(&logits).map_err(|error| refused(choice, error.to_string()))?;
    if logits.iter().any(|logit| !logit.is_finite()) {
        return Err(refused(
            choice,
            "model produced a non-finite logit".to_owned(),
        ));
    }
    let probabilities = actor
        .probabilities(&options, head, row, temperature)
        .map_err(|error| refused(choice, error.to_string()))?;
    let critic = CriticInput::new(&critic_vector(seen, CriticFeatures::full()), vocabulary);
    let value = actor
        .value(&critic, row)
        .map_err(|error| refused(choice, error.to_string()))?;
    if logits.len() != choice.options.len() || probabilities.len() != choice.options.len() {
        return Err(refused(
            choice,
            "model output length differs from choice options".to_owned(),
        ));
    }
    Ok(EvaluateResponse {
        head: head.to_owned(),
        value,
        options: choice
            .options
            .iter()
            .zip(logits)
            .zip(probabilities)
            .map(|((option, logit), probability)| OptionAdvice {
                option_id: option.id.clone(),
                probability,
                logit: f64::from(logit),
            })
            .collect(),
    })
}

fn sparse(
    vector: &ti4_policy::features::FeatureVector,
    vocabulary: &Vocabulary,
) -> Result<SparseOption, String> {
    let mut columns = Vec::with_capacity(vector.len());
    let mut values = Vec::with_capacity(vector.len());
    for (key, value) in vector {
        if !value.is_finite() {
            return Err("a projected feature is not finite".to_owned());
        }
        columns.push(
            i64::try_from(vocabulary.resolve_key(*key).0)
                .map_err(|_| "feature column does not fit i64")?,
        );
        let value = *value as f32;
        if !value.is_finite() {
            return Err("a projected feature does not fit f32".to_owned());
        }
        values.push(value);
    }
    Ok(SparseOption { columns, values })
}

fn refused(choice: &Choice, reason: String) -> IllegalChoice {
    IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason,
    }
}

fn validate_request(request: &EvaluateRequest) -> Result<SourceSet, ApiError> {
    if request.player != request.choice.player {
        return Err(ApiError::bad_request("player must match choice.player"));
    }
    if request.choice.options.is_empty() || request.choice.options.len() > MAX_OPTIONS {
        return Err(ApiError::bad_request(
            "choice must have 1 through 512 options",
        ));
    }
    if request
        .choice
        .options
        .iter()
        .map(|option| &option.id)
        .collect::<BTreeSet<_>>()
        .len()
        != request.choice.options.len()
    {
        return Err(ApiError::bad_request("choice option ids must be unique"));
    }
    if request.galaxy_layout.version != 1 {
        return Err(ApiError::bad_request("unsupported galaxy layout version"));
    }
    if request.galaxy_layout.placements.len() > MAX_PLACEMENTS
        || request.galaxy_layout.off_map_system_ids.len() > MAX_OFF_MAP_SYSTEMS
        || request.galaxy_layout.active_sources.len() > MAX_SOURCES
    {
        return Err(ApiError::bad_request(
            "galaxy layout exceeds service bounds",
        ));
    }
    parse_sources(&request.galaxy_layout.active_sources)
}

fn parse_sources(names: &[String]) -> Result<SourceSet, ApiError> {
    let mut sources = SourceSet::empty();
    for name in names {
        let source = name
            .parse::<Source>()
            .map_err(|_| ApiError::bad_request(format!("unknown source {name:?}")))?;
        if !sources.insert(source) {
            return Err(ApiError::bad_request(format!("duplicate source {name:?}")));
        }
    }
    if sources.is_empty() {
        return Err(ApiError::bad_request("active_sources cannot be empty"));
    }
    Ok(sources)
}

fn reconstruct_galaxy(
    content: &ContentStore,
    layout: &GalaxyLayout,
    sources: SourceSet,
) -> Result<Galaxy, ApiError> {
    let mut placements = Vec::with_capacity(layout.placements.len());
    let mut system_ids = BTreeSet::new();
    let mut coordinates = BTreeSet::new();
    for placement in &layout.placements {
        if !system_ids.insert(placement.system_id.as_str()) {
            return Err(ApiError::bad_request("galaxy layout repeats a system id"));
        }
        if !coordinates.insert((placement.q, placement.r)) {
            return Err(ApiError::bad_request("galaxy layout repeats a coordinate"));
        }
        placements.push((
            placement.system_id.as_str(),
            Hex::new(placement.q, placement.r),
        ));
    }
    let mut galaxy = Galaxy::placed(content, &placements, sources)
        .map_err(|error| ApiError::bad_request(format!("invalid galaxy layout: {error}")))?;
    let mut off_map_ids = BTreeSet::new();
    for system_id in &layout.off_map_system_ids {
        if !off_map_ids.insert(system_id.as_str()) || system_ids.contains(system_id.as_str()) {
            return Err(ApiError::bad_request(
                "off-map system ids must be unique and unplaced",
            ));
        }
        galaxy
            .place_off_map(content, system_id, sources)
            .map_err(|error| ApiError::bad_request(format!("invalid off-map system: {error}")))?;
    }
    Ok(galaxy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

    fn layout() -> GalaxyLayout {
        GalaxyLayout {
            version: 1,
            active_sources: vec!["base".to_owned(), "pok".to_owned()],
            placements: vec![
                ti4_server::map::GalaxyPlacement {
                    system_id: "18".to_owned(),
                    q: 0,
                    r: 0,
                },
                ti4_server::map::GalaxyPlacement {
                    system_id: "39".to_owned(),
                    q: 2,
                    r: 0,
                },
            ],
            off_map_system_ids: vec!["82b".to_owned()],
        }
    }

    #[test]
    fn layout_reconstruction_preserves_main_and_off_map_topology() {
        let content = ContentStore::embedded();
        let rebuilt = reconstruct_galaxy(content, &layout(), POK).expect("valid layout");
        assert_eq!(rebuilt.coord_of("39"), Some(Hex::new(2, 0)));
        assert!(rebuilt.are_adjacent("82b", "39"));
    }

    #[test]
    fn unknown_or_duplicate_sources_are_rejected() {
        assert!(parse_sources(&["base".to_owned(), "unknown".to_owned()]).is_err());
        assert!(parse_sources(&["base".to_owned(), "base".to_owned()]).is_err());
    }

    #[test]
    fn duplicate_layout_systems_are_rejected() {
        let content = ContentStore::embedded();
        let mut invalid = layout();
        invalid.placements.push(ti4_server::map::GalaxyPlacement {
            system_id: "18".to_owned(),
            q: 1,
            r: 0,
        });
        assert!(reconstruct_galaxy(content, &invalid, POK).is_err());
    }

    #[test]
    fn duplicate_layout_coordinates_are_rejected() {
        let content = ContentStore::embedded();
        let mut invalid = layout();
        invalid.placements[1].q = 0;
        invalid.placements[1].r = 0;
        assert!(reconstruct_galaxy(content, &invalid, POK).is_err());
    }

    #[test]
    fn request_player_and_choice_owner_must_match() {
        let request = EvaluateRequest {
            state: ti4_engine::fixtures::game(&["a"]),
            galaxy_layout: layout(),
            player: PlayerId::new("a"),
            choice: Choice::new(PlayerId::new("b"), "test", vec![ChoiceOption::decline()]),
            temperature: None,
        };
        assert!(validate_request(&request).is_err());
    }
}
