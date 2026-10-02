//! Stateless, loopback-only MLP advice and battle odds for one redacted game snapshot.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing::post};
use serde::{Deserialize, Serialize};
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
use ti4_training::battle_arena::{self, GroundSide, Side};

const MAX_OPTIONS: usize = 512;
const MAX_PLACEMENTS: usize = 256;
const MAX_OFF_MAP_SYSTEMS: usize = 64;
const MAX_SOURCES: usize = 7;
const DEFAULT_TEMPERATURE: f64 = 0.25;
const MAX_BODY_BYTES: usize = 1_048_576;
const DEFAULT_SIMULATIONS: usize = 2000;
const MAX_SIMULATIONS: usize = 50_000;

/// Units count input, accepting either `{"unit_id": count}` or `[["unit_id", count]]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UnitCounts {
    Map(BTreeMap<String, usize>),
    List(Vec<(String, usize)>),
}

impl UnitCounts {
    #[must_use]
    pub fn into_entries(self) -> Vec<(String, usize)> {
        match self {
            Self::Map(map) => map.into_iter().collect(),
            Self::List(list) => list,
        }
    }
}

/// A participant in space combat, specified either by a player ID or custom side parameters.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ParticipantInput {
    Player(PlayerId),
    Side(SideInput),
}

/// Fleet configuration for one participant in space combat.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideInput {
    /// Player ID whose fleet/guns at the combat system should be included.
    #[serde(default)]
    pub player: Option<PlayerId>,
    /// Faction identifier (e.g. "sol", "letnev", "jolnar"). If omitted and `player` is given, read from state.
    #[serde(default)]
    pub faction: Option<String>,
    /// Ships to field as `{"unit_id": count}` or `[["unit_id", count]]`. Added to units from state.
    #[serde(default)]
    pub units: Option<UnitCounts>,
    /// Ships that start damaged as `{"unit_id": count}` or `[["unit_id", count]]`.
    #[serde(default)]
    pub damaged: Option<UnitCounts>,
    /// Units firing space cannon before combat as `{"unit_id": count}` or `[["unit_id", count]]`.
    #[serde(default)]
    pub guns: Option<UnitCounts>,
}

/// A request to evaluate space combat using `battle_arena` rollouts.
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

/// Outcome statistics from `battle_arena` rollouts.
#[derive(Debug, Clone, Serialize)]
pub struct BattleResponse {
    pub simulations: usize,
    pub attacker_win_rate: f64,
    pub defender_win_rate: f64,
    pub mutual_destruction_rate: f64,
    pub unresolved_rate: f64,
    pub average_rounds: f64,
    pub attacker_expected_survivors: BTreeMap<String, f64>,
    pub defender_expected_survivors: BTreeMap<String, f64>,
    pub attacker_fielded: BTreeMap<String, usize>,
    pub defender_fielded: BTreeMap<String, usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundOddsSide {
    pub faction: String,
    pub units: BTreeMap<String, usize>,
    #[serde(default)]
    pub damaged: BTreeMap<String, usize>,
    #[serde(default)]
    pub guns: BTreeMap<String, usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundOddsRequest {
    pub attacker: GroundOddsSide,
    pub defender: GroundOddsSide,
    /// Legal bombarding ships that fire Harrow after rounds (never before landing).
    #[serde(default)]
    pub harrow: BTreeMap<String, usize>,
    #[serde(default)]
    pub simulations: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct GroundOddsResponse {
    pub simulations: usize,
    pub attacker_win_rate: f64,
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

    /// Evaluate space combat odds using `battle_arena` rollouts.
    ///
    /// # Errors
    ///
    /// Returns an [`ApiError`] if parameters, factions, or unit IDs are invalid.
    #[allow(clippy::cast_precision_loss)]
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

        let (attacker_side, attacker_fielded) = resolve_side(
            &request.attacker,
            request.state.as_ref(),
            request.system.as_ref(),
            galaxy.as_ref(),
            self.inner.content,
            sources,
        )?;

        let (defender_side, defender_fielded) = resolve_side(
            &request.defender,
            request.state.as_ref(),
            request.system.as_ref(),
            galaxy.as_ref(),
            self.inner.content,
            sources,
        )?;

        let simulations = match request.simulations {
            Some(0) => return Err(ApiError::bad_request("simulations must be at least 1")),
            Some(n) if n > MAX_SIMULATIONS => {
                return Err(ApiError::bad_request(format!(
                    "simulations cannot exceed {MAX_SIMULATIONS}"
                )));
            }
            Some(n) => n,
            None => DEFAULT_SIMULATIONS,
        };

        let attacker_cannon = request.attacker_cannon.unwrap_or(true);
        let in_progress = request.in_progress.unwrap_or(false);
        let base_seed = request.seed.unwrap_or(0);

        let mut attacker_wins = 0usize;
        let mut defender_wins = 0usize;
        let mut mutual_destruction = 0usize;
        let mut unresolved = 0usize;
        let mut total_rounds = 0u64;

        let mut attacker_survivors = vec![0u64; attacker_side.names().len()];
        let mut defender_survivors = vec![0u64; defender_side.names().len()];

        for i in 0..simulations {
            let seed = base_seed.wrapping_add(i as u64);
            let outcome = if in_progress {
                battle_arena::fight_in_progress(&attacker_side, &defender_side, seed)
            } else {
                battle_arena::fight_outcome(&attacker_side, &defender_side, seed, attacker_cannon)
            };

            if outcome.unresolved {
                unresolved += 1;
            }
            match outcome.winner {
                Some("a") => attacker_wins += 1,
                Some("b") => defender_wins += 1,
                _ => mutual_destruction += 1,
            }
            total_rounds += u64::from(outcome.rounds);

            for (idx, &count) in outcome.attacker_left.iter().enumerate() {
                attacker_survivors[idx] += count as u64;
            }
            for (idx, &count) in outcome.defender_left.iter().enumerate() {
                defender_survivors[idx] += count as u64;
            }
        }

        let sim_f64 = simulations as f64;
        let mut attacker_expected_survivors = BTreeMap::new();
        for (idx, name) in attacker_side.names().iter().enumerate() {
            attacker_expected_survivors
                .insert(name.clone(), attacker_survivors[idx] as f64 / sim_f64);
        }
        let mut defender_expected_survivors = BTreeMap::new();
        for (idx, name) in defender_side.names().iter().enumerate() {
            defender_expected_survivors
                .insert(name.clone(), defender_survivors[idx] as f64 / sim_f64);
        }

        Ok(BattleResponse {
            simulations,
            attacker_win_rate: attacker_wins as f64 / sim_f64,
            defender_win_rate: defender_wins as f64 / sim_f64,
            mutual_destruction_rate: mutual_destruction as f64 / sim_f64,
            unresolved_rate: unresolved as f64 / sim_f64,
            average_rounds: total_rounds as f64 / sim_f64,
            attacker_expected_survivors,
            defender_expected_survivors,
            attacker_fielded,
            defender_fielded,
        })
    }

    pub fn ground_odds(&self, request: &GroundOddsRequest) -> Result<GroundOddsResponse, ApiError> {
        let n = request.simulations.unwrap_or(DEFAULT_SIMULATIONS);
        if n == 0 || n > MAX_SIMULATIONS {
            return Err(ApiError::bad_request(
                "simulations must be between 1 and 50000",
            ));
        }
        if !request.attacker.guns.is_empty() {
            return Err(ApiError::bad_request(
                "only the defender has standing defense guns",
            ));
        }
        if request.harrow.len() > MAX_PLACEMENTS
            || request.harrow.values().any(|count| *count > MAX_PLACEMENTS)
            || request.harrow.values().sum::<usize>() > MAX_PLACEMENTS
        {
            return Err(ApiError::bad_request("too many Harrow units"));
        }
        for id in request.harrow.keys() {
            if !ti4_content::units::unit_type(self.inner.content, id, POK)
                .is_some_and(|unit| unit.has_bombardment() && unit.bombard_dice() > 0)
            {
                return Err(ApiError::bad_request(format!("invalid Harrow unit: {id}")));
            }
        }
        let resolve = |side: &GroundOddsSide| -> Result<GroundSide, ApiError> {
            if side.units.len() > MAX_PLACEMENTS
                || side.guns.len() > MAX_PLACEMENTS
                || side.units.values().any(|count| *count > MAX_PLACEMENTS)
                || side.guns.values().any(|count| *count > MAX_PLACEMENTS)
                || side.units.values().sum::<usize>() > MAX_PLACEMENTS
                || side.guns.values().sum::<usize>() > MAX_PLACEMENTS
            {
                return Err(ApiError::bad_request("too many ground units"));
            }
            for (id, count) in &side.units {
                if *count > 0
                    && !ti4_content::units::unit_type(self.inner.content, id, POK)
                        .is_some_and(|unit| unit.is_ground_force())
                {
                    return Err(ApiError::bad_request(format!("invalid ground force: {id}")));
                }
                if side.damaged.get(id).copied().unwrap_or(0) > *count {
                    return Err(ApiError::bad_request("damaged count exceeds force count"));
                }
            }
            if side.damaged.keys().any(|id| !side.units.contains_key(id)) {
                return Err(ApiError::bad_request("damaged unit is not fielded"));
            }
            for id in side.guns.keys() {
                if !ti4_content::units::unit_type(self.inner.content, id, POK)
                    .is_some_and(|unit| unit.space_cannon_hits_on().is_some())
                {
                    return Err(ApiError::bad_request(format!("invalid defense gun: {id}")));
                }
            }
            let forces: Vec<_> = side.units.iter().map(|(id, n)| (id.clone(), *n)).collect();
            let damaged: Vec<_> = side
                .damaged
                .iter()
                .map(|(id, n)| (id.clone(), *n))
                .collect();
            let guns: Vec<_> = side.guns.iter().map(|(id, n)| (id.clone(), *n)).collect();
            Ok(
                GroundSide::of(self.inner.content, &forces, &damaged, &side.faction)
                    .with_defense_guns(self.inner.content, &guns),
            )
        };
        let harrow: Vec<_> = request
            .harrow
            .iter()
            .map(|(id, count)| (id.clone(), *count))
            .collect();
        let attacker =
            resolve(&request.attacker)?.with_bombardment(self.inner.content, &harrow, false, true);
        let defender = resolve(&request.defender)?;
        let wins = (0..n)
            .filter(|seed| {
                battle_arena::ground_fight(&attacker, &defender, *seed as u64).winner == Some("a")
            })
            .count();
        Ok(GroundOddsResponse {
            simulations: n,
            attacker_win_rate: wins as f64 / n as f64,
        })
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

#[allow(clippy::too_many_lines)]
fn resolve_side(
    participant: &ParticipantInput,
    state: Option<&GameState>,
    system: Option<&SystemId>,
    galaxy: Option<&Galaxy>,
    content: &ContentStore,
    sources: SourceSet,
) -> Result<(Side, BTreeMap<String, usize>), ApiError> {
    let (player_id, explicit_faction, extra_units, extra_damaged, extra_guns) = match participant {
        ParticipantInput::Player(pid) => (Some(pid), None, Vec::new(), Vec::new(), Vec::new()),
        ParticipantInput::Side(side) => (
            side.player.as_ref(),
            side.faction.as_deref(),
            side.units
                .clone()
                .map(UnitCounts::into_entries)
                .unwrap_or_default(),
            side.damaged
                .clone()
                .map(UnitCounts::into_entries)
                .unwrap_or_default(),
            side.guns
                .clone()
                .map(UnitCounts::into_entries)
                .unwrap_or_default(),
        ),
    };

    let mut fleet: BTreeMap<String, usize> = BTreeMap::new();
    let mut damaged: BTreeMap<String, usize> = BTreeMap::new();
    let mut guns: BTreeMap<String, usize> = BTreeMap::new();

    let faction: String = if let Some(pid) = player_id {
        let game_state = state
            .ok_or_else(|| ApiError::bad_request("state is required when player is specified"))?;
        let seat = game_state
            .player(pid)
            .ok_or_else(|| ApiError::bad_request(format!("player {pid} not found in state")))?;
        let player_faction = explicit_faction.unwrap_or(seat.faction.as_str());

        if let Some(sys) = system {
            let board = game_state.system_state(sys);
            for unit in &board.units {
                if &unit.owner == pid
                    && let Some(kind) =
                        ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                    && kind.is_ship()
                {
                    *fleet.entry(unit.type_id.to_string()).or_default() += 1;
                    if unit.sustained_damage {
                        *damaged.entry(unit.type_id.to_string()).or_default() += 1;
                    }
                }
            }
            for unit in board.planet_units.values().flatten() {
                if &unit.owner == pid
                    && let Some(kind) =
                        ti4_content::units::unit_type(content, unit.type_id.as_str(), sources)
                    && kind.has_space_cannon()
                {
                    *guns.entry(unit.type_id.to_string()).or_default() += 1;
                }
            }
            if let Some(g) = galaxy {
                for adjacent_id in g.adjacent(sys.as_str()) {
                    let adj_sys = SystemId::new(adjacent_id);
                    let adj_board = game_state.system_state(&adj_sys);
                    let adj_units = adj_board
                        .planet_units
                        .values()
                        .flatten()
                        .chain(&adj_board.units);
                    for unit in adj_units {
                        if &unit.owner == pid {
                            let reaches = matches!(
                                unit.type_id.as_str(),
                                "pds2" | "xxcha_mech" | "xxcha_flagship"
                            ) || ti4_content::units::unit_type(
                                content,
                                unit.type_id.as_str(),
                                sources,
                            )
                            .and_then(|k| k.record().text("ability"))
                            .is_some_and(|ability| {
                                let a = ability.to_ascii_lowercase();
                                a.contains("space cannon against ships that are")
                                    && a.contains("adjacent")
                            });
                            if reaches {
                                *guns.entry(unit.type_id.to_string()).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
        player_faction.to_owned()
    } else {
        explicit_faction.unwrap_or("generic").to_owned()
    };

    for (id, count) in extra_units {
        *fleet.entry(id).or_default() += count;
    }
    for (id, count) in extra_damaged {
        *damaged.entry(id).or_default() += count;
    }
    for (id, count) in extra_guns {
        *guns.entry(id).or_default() += count;
    }

    // Validate unit IDs
    for (id, &count) in &fleet {
        if count > 0 && ti4_content::units::unit_type(content, id, sources).is_none() {
            return Err(ApiError::bad_request(format!(
                "unknown unit id in fleet: {id}"
            )));
        }
    }
    for (id, &count) in &damaged {
        if count > 0 && ti4_content::units::unit_type(content, id, sources).is_none() {
            return Err(ApiError::bad_request(format!(
                "unknown unit id in damaged: {id}"
            )));
        }
    }
    for (id, &count) in &guns {
        if count > 0 && ti4_content::units::unit_type(content, id, sources).is_none() {
            return Err(ApiError::bad_request(format!(
                "unknown unit id in guns: {id}"
            )));
        }
    }

    let fleet_vec: Vec<(String, usize)> =
        fleet.clone().into_iter().filter(|(_, n)| *n > 0).collect();
    let damaged_vec: Vec<(String, usize)> = damaged.into_iter().filter(|(_, n)| *n > 0).collect();
    let guns_vec: Vec<(String, usize)> = guns.into_iter().filter(|(_, n)| *n > 0).collect();

    let side =
        Side::of(content, &fleet_vec, &damaged_vec, &faction, true).with_guns(content, &guns_vec);

    Ok((side, fleet))
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

    #[test]
    fn direct_fleet_battle_simulation_computes_odds() {
        let advisor = Advisor::battle_only();
        let request = BattleRequest {
            state: None,
            galaxy_layout: None,
            system: None,
            attacker: ParticipantInput::Side(SideInput {
                player: None,
                faction: Some("sol".to_owned()),
                units: Some(UnitCounts::List(vec![
                    ("carrier".to_owned(), 1),
                    ("fighter".to_owned(), 4),
                ])),
                damaged: None,
                guns: None,
            }),
            defender: ParticipantInput::Side(SideInput {
                player: None,
                faction: Some("letnev".to_owned()),
                units: Some(UnitCounts::Map(
                    [("dreadnought".to_owned(), 1)].into_iter().collect(),
                )),
                damaged: None,
                guns: None,
            }),
            simulations: Some(100),
            attacker_cannon: None,
            in_progress: None,
            seed: Some(42),
        };

        let response = advisor.battle(&request).expect("battle succeeds");
        assert_eq!(response.simulations, 100);
        let total_rate = response.attacker_win_rate
            + response.defender_win_rate
            + response.mutual_destruction_rate;
        assert!((total_rate - 1.0).abs() < 1e-6);
        assert_eq!(response.attacker_fielded.get("carrier"), Some(&1));
        assert_eq!(response.attacker_fielded.get("fighter"), Some(&4));
        assert_eq!(response.defender_fielded.get("dreadnought"), Some(&1));
        assert!(response.average_rounds > 0.0);
    }

    #[test]
    fn battle_from_game_state_snapshot_resolves_fleets() {
        let advisor = Advisor::battle_only();
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        let sys = SystemId::new("18");
        let a_player = PlayerId::new("a");
        let b_player = PlayerId::new("b");

        state
            .system_mut(&sys)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("cruiser"),
                a_player.clone(),
            ));
        state
            .system_mut(&sys)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("cruiser"),
                a_player.clone(),
            ));
        state
            .system_mut(&sys)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("destroyer"),
                b_player.clone(),
            ));

        let request = BattleRequest {
            state: Some(state),
            galaxy_layout: None,
            system: Some(sys),
            attacker: ParticipantInput::Player(a_player),
            defender: ParticipantInput::Player(b_player),
            simulations: Some(50),
            attacker_cannon: None,
            in_progress: None,
            seed: Some(123),
        };

        let response = advisor.battle(&request).expect("state battle succeeds");
        assert_eq!(response.simulations, 50);
        assert_eq!(response.attacker_fielded.get("cruiser"), Some(&2));
        assert_eq!(response.defender_fielded.get("destroyer"), Some(&1));
        assert!(response.attacker_win_rate > response.defender_win_rate);
    }

    #[test]
    fn battle_validation_rejects_invalid_inputs() {
        let advisor = Advisor::battle_only();

        // 0 simulations rejected
        let req_zero_sims = BattleRequest {
            state: None,
            galaxy_layout: None,
            system: None,
            attacker: ParticipantInput::Side(SideInput {
                faction: Some("sol".to_owned()),
                units: Some(UnitCounts::List(vec![("fighter".to_owned(), 1)])),
                ..Default::default()
            }),
            defender: ParticipantInput::Side(SideInput {
                faction: Some("letnev".to_owned()),
                units: Some(UnitCounts::List(vec![("fighter".to_owned(), 1)])),
                ..Default::default()
            }),
            simulations: Some(0),
            attacker_cannon: None,
            in_progress: None,
            seed: None,
        };
        assert!(advisor.battle(&req_zero_sims).is_err());

        // Simulations exceeding MAX_SIMULATIONS rejected
        let mut req_too_many = req_zero_sims.clone();
        req_too_many.simulations = Some(MAX_SIMULATIONS + 1);
        assert!(advisor.battle(&req_too_many).is_err());

        // Unknown unit ID rejected
        let mut req_bad_unit = req_zero_sims.clone();
        req_bad_unit.simulations = Some(10);
        req_bad_unit.attacker = ParticipantInput::Side(SideInput {
            faction: Some("sol".to_owned()),
            units: Some(UnitCounts::List(vec![("nonexistent_ship".to_owned(), 1)])),
            ..Default::default()
        });
        assert!(advisor.battle(&req_bad_unit).is_err());

        // Player specified without state rejected
        let mut req_no_state = req_zero_sims;
        req_no_state.simulations = Some(10);
        req_no_state.attacker = ParticipantInput::Player(PlayerId::new("missing_player"));
        assert!(advisor.battle(&req_no_state).is_err());
    }
}
