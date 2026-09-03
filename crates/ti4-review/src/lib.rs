//! Native, omniscient learned-game review sessions.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::too_many_lines
)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_engine::game::Game;
use ti4_mlp::bot::{InferenceStatus, MlpBot};
use ti4_mlp::{Actor, FactionRow, SparseOption};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId, SystemId};
use ti4_model::state::{GameState, Phase};
use ti4_policy::features::names_of;
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::{Profile, decision_head};
use ti4_policy::progress::Baseline;
use ti4_policy::vocabulary::Vocabulary;
use ti4_sim::MapPool;
use ti4_training::rollout::{OpeningMap, setup_game_with_decider_factory};

pub mod gui;

pub const SESSION_SCHEMA: &str = "ti4-review-session";
pub const SESSION_VERSION: u32 = 2;
pub const TILE_SEED_OFFSET: u64 = 20_000_000;
pub const MAX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_SESSION_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_HTML_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_FRAMES: usize = 1_000_001;
pub const MAX_COMMAND_STEPS: usize = 2_000_000;
pub const MAX_RUN_COUNT: usize = 1_000_000;
pub const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

#[must_use]
pub const fn default_sampling_temperature() -> f64 {
    1.0
}

#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}")]
    Invalid(String),
    #[error("session exceeds its {MAX_SESSION_BYTES}-byte limit")]
    SessionTooLarge,
    #[error("HTML export exceeds its {MAX_HTML_BYTES}-byte limit")]
    HtmlTooLarge,
}

pub type Result<T> = std::result::Result<T, ReviewError>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileTable {
    #[default]
    Learner,
    Accepted,
}

impl ProfileTable {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Learner => "Learner",
            Self::Accepted => "Accepted champion",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SimulationConfig {
    pub checkpoint: PathBuf,
    pub map_pool: PathBuf,
    pub seed: u64,
    pub rotation: usize,
    pub table: ProfileTable,
    pub temperature: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionManifest {
    pub checkpoint_path: String,
    pub checkpoint_sha256: String,
    pub map_pool_path: String,
    pub map_pool_sha256: String,
    pub seed: u64,
    pub tile_seed: u64,
    pub rotation: usize,
    pub profile_table: ProfileTable,
    #[serde(default = "default_sampling_temperature")]
    pub temperature: f64,
    #[serde(default)]
    pub policy: PolicySummary,
    pub factions: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicySummary {
    pub format: String,
    pub schema: Option<u64>,
    pub name: Option<String>,
    pub source: Option<String>,
    pub git_commit: Option<String>,
    pub update: Option<u64>,
    pub dimensions: Option<String>,
    pub heads: Vec<String>,
    pub factions: Vec<String>,
    pub profiles: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanetMeta {
    pub id: String,
    pub label: String,
    pub resources: i64,
    pub influence: i64,
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub tech_specialties: Vec<String>,
    #[serde(default)]
    pub legendary: bool,
    #[serde(default)]
    pub space_station: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoardTile {
    pub system: String,
    pub label: String,
    pub q: i32,
    pub r: i32,
    pub hyperlane: bool,
    pub planets: Vec<PlanetMeta>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeatureContribution {
    pub name: String,
    pub value: f64,
    /// Linear-policy weight. Nonlinear MLP inputs have no single fixed weight.
    pub weight: Option<f64>,
    /// Exact linear value × weight. Absent for nonlinear MLP inputs.
    pub contribution: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionDetail {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub score: Option<f64>,
    pub probability: Option<f64>,
    pub features: Vec<FeatureContribution>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionDetail {
    pub sequence: usize,
    pub player: String,
    pub faction: String,
    pub prompt: String,
    pub path: String,
    pub requested_head: String,
    pub resolved_head: String,
    pub temperature: Option<f64>,
    pub chosen: Option<String>,
    pub options: Vec<OptionDetail>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActionSummary {
    pub actor: String,
    pub faction: String,
    pub headline: String,
    pub start_frame: usize,
    pub end_frame: usize,
    pub details: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReviewFrame {
    pub index: usize,
    pub engine_step: usize,
    pub decision_count: usize,
    pub action_count: usize,
    pub round: u32,
    pub phase: Phase,
    pub active: Option<String>,
    pub resolved_choice: bool,
    pub action_completed: bool,
    pub finished: bool,
    pub error: Option<String>,
    pub new_events: Vec<String>,
    pub decisions: Vec<DecisionDetail>,
    #[serde(default)]
    pub action_summary: Option<ActionSummary>,
    pub state: GameState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionOutcome {
    InProgress,
    Completed,
    EngineFailed { error: String },
    SafetyLimit { steps: usize },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewSession {
    pub schema: String,
    pub version: u32,
    pub manifest: SessionManifest,
    pub board: Vec<BoardTile>,
    #[serde(default)]
    pub planet_catalog: Vec<PlanetMeta>,
    pub frames: Vec<ReviewFrame>,
    pub outcome: SessionOutcome,
}

impl ReviewSession {
    pub fn validate(&self) -> Result<()> {
        if self.schema != SESSION_SCHEMA || self.version != SESSION_VERSION {
            return Err(ReviewError::Invalid(format!(
                "unsupported review session {} v{}",
                self.schema, self.version
            )));
        }
        if self.frames.is_empty() || self.frames.len() > MAX_FRAMES {
            return Err(ReviewError::Invalid("invalid frame count".to_owned()));
        }
        let mut session_factions = self.manifest.factions.clone();
        session_factions.sort();
        let mut standard_factions = FACTIONS.map(str::to_owned).to_vec();
        standard_factions.sort();
        if session_factions != standard_factions {
            return Err(ReviewError::Invalid(
                "session does not carry the standard six-faction lineup".to_owned(),
            ));
        }
        if !self.manifest.temperature.is_finite() || self.manifest.temperature <= 0.0 {
            return Err(ReviewError::Invalid(
                "session carries an invalid sampling temperature".to_owned(),
            ));
        }
        for (index, frame) in self.frames.iter().enumerate() {
            if frame.index != index {
                return Err(ReviewError::Invalid(format!(
                    "frame {} has non-contiguous index {}",
                    index, frame.index
                )));
            }
            if index == 0 && frame.engine_step != 0 {
                return Err(ReviewError::Invalid(
                    "initial frame is not engine step zero".to_owned(),
                ));
            }
            if index > 0 && frame.engine_step != self.frames[index - 1].engine_step + 1 {
                return Err(ReviewError::Invalid(format!(
                    "frame {index} has a broken engine-step sequence"
                )));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn latest(&self) -> &ReviewFrame {
        self.frames.last().expect("validated sessions have a frame")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvanceUnit {
    Step,
    Decision,
    Action,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdvanceReport {
    pub steps: usize,
    pub decisions: usize,
    pub actions: usize,
    pub reached_target: bool,
}

struct TraceBot {
    inner: LearnedBot,
    faction: String,
    log: Rc<RefCell<Vec<DecisionDetail>>>,
}

impl TraceBot {
    fn option_rows(choice: &Choice) -> Vec<OptionDetail> {
        choice
            .options
            .iter()
            .map(|option| OptionDetail {
                id: option.id.clone(),
                kind: option.kind.clone(),
                label: option.display().to_owned(),
                score: None,
                probability: None,
                features: Vec::new(),
            })
            .collect()
    }

    fn push(
        &self,
        choice: &Choice,
        path: &str,
        requested_head: &str,
        resolved_head: &str,
        temperature: Option<f64>,
        chosen: &std::result::Result<ChoiceOption, IllegalChoice>,
        options: Vec<OptionDetail>,
    ) {
        let sequence = self.log.borrow().len();
        self.log.borrow_mut().push(DecisionDetail {
            sequence,
            player: choice.player.to_string(),
            faction: self.faction.clone(),
            prompt: choice.prompt.clone(),
            path: path.to_owned(),
            requested_head: requested_head.to_owned(),
            resolved_head: resolved_head.to_owned(),
            temperature,
            chosen: chosen.as_ref().ok().map(|option| option.id.clone()),
            options,
        });
    }
}

impl Decider for TraceBot {
    fn choose(&mut self, choice: &Choice) -> std::result::Result<ChoiceOption, IllegalChoice> {
        let options = Self::option_rows(choice);
        let picked = self.inner.choose(choice);
        self.push(choice, "blind", "other", "other", None, &picked, options);
        picked
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> std::result::Result<ChoiceOption, IllegalChoice> {
        let (features, probabilities) =
            self.inner
                .consider(seen.observed(), choice, &seen.held_secret_progress());
        let requested = decision_head(choice);
        let resolved = self.inner.profile().resolved_head(requested).to_owned();
        let temperature = self
            .inner
            .profile()
            .head(&resolved)
            .map(|head| head.temperature);
        let mut options = Self::option_rows(choice);
        for row in &mut options {
            let Some(vector) = features.get(&row.id) else {
                continue;
            };
            row.score = Some(self.inner.profile().score_vector(&resolved, vector));
            row.probability = probabilities.get(&row.id).copied();
            let names = names_of(vector);
            let head = self.inner.profile().head(&resolved);
            row.features = names
                .into_iter()
                .zip(vector.values().copied())
                .map(|(name, value)| {
                    let weight = head
                        .and_then(|head| head.weights.get(&name))
                        .copied()
                        .unwrap_or(0.0);
                    FeatureContribution {
                        name,
                        value,
                        weight: Some(weight),
                        contribution: Some(value * weight),
                    }
                })
                .collect();
            row.features.sort_by(|left, right| {
                right
                    .contribution
                    .unwrap_or_default()
                    .abs()
                    .total_cmp(&left.contribution.unwrap_or_default().abs())
                    .then_with(|| left.name.cmp(&right.name))
            });
        }
        let picked = self.inner.choose_seeing(choice, seen);
        self.push(
            choice,
            "seeing",
            requested,
            &resolved,
            temperature,
            &picked,
            options,
        );
        picked
    }
}

struct MlpTraceBot {
    inner: Box<dyn Decider>,
    actor: Rc<Actor>,
    vocabulary: Vocabulary,
    row: FactionRow,
    baseline: Baseline,
    faction: String,
    temperature: f64,
    log: Rc<RefCell<Vec<DecisionDetail>>>,
}

impl MlpTraceBot {
    fn push(
        &self,
        choice: &Choice,
        path: &str,
        head: &str,
        chosen: &std::result::Result<ChoiceOption, IllegalChoice>,
        options: Vec<OptionDetail>,
    ) {
        let sequence = self.log.borrow().len();
        self.log.borrow_mut().push(DecisionDetail {
            sequence,
            player: choice.player.to_string(),
            faction: self.faction.clone(),
            prompt: choice.prompt.clone(),
            path: path.to_owned(),
            requested_head: head.to_owned(),
            resolved_head: head.to_owned(),
            temperature: Some(self.temperature),
            chosen: chosen.as_ref().ok().map(|option| option.id.clone()),
            options,
        });
    }
}

impl Decider for MlpTraceBot {
    fn choose(&mut self, choice: &Choice) -> std::result::Result<ChoiceOption, IllegalChoice> {
        let options = TraceBot::option_rows(choice);
        let picked = self.inner.choose(choice);
        self.push(choice, "blind", "other", &picked, options);
        picked
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> std::result::Result<ChoiceOption, IllegalChoice> {
        let vectors = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            &choice.player,
            &seen.held_secret_progress(),
            self.baseline,
        );
        let sparse: Vec<SparseOption> = vectors
            .iter()
            .map(|vector| SparseOption {
                columns: vector
                    .keys()
                    .map(|key| {
                        i64::try_from(self.vocabulary.column_of_key(*key)).unwrap_or_default()
                    })
                    .collect(),
                values: vector.values().map(|value| *value as f32).collect(),
            })
            .collect();
        let head = Actor::resolve_head(decision_head(choice));
        let scores = self.actor.logits(&sparse, head, self.row).ok();
        let probabilities = self
            .actor
            .probabilities(&sparse, head, self.row, self.temperature)
            .ok();
        let mut options = TraceBot::option_rows(choice);
        for (index, option) in options.iter_mut().enumerate() {
            option.score = scores
                .as_ref()
                .map(|scores| scores.double_value(&[i64::try_from(index).unwrap_or_default()]));
            option.probability = probabilities
                .as_ref()
                .and_then(|probabilities| probabilities.get(index).copied());
            if let Some(vector) = vectors.get(index) {
                option.features = names_of(vector)
                    .into_iter()
                    .zip(vector.values().copied())
                    .map(|(name, value)| FeatureContribution {
                        name,
                        value,
                        weight: None,
                        contribution: None,
                    })
                    .collect();
                option
                    .features
                    .sort_by(|left, right| left.name.cmp(&right.name));
            }
        }
        let picked = self.inner.choose_seeing(choice, seen);
        self.push(choice, "seeing-mlp", head, &picked, options);
        picked
    }
}

enum LoadedPolicy {
    Linear(BTreeMap<String, Profile>),
    Mlp {
        actor: Rc<Actor>,
        vocabulary: Vocabulary,
    },
}

pub struct LiveReview {
    pub session: ReviewSession,
    game: Game<'static>,
    decisions: Rc<RefCell<Vec<DecisionDetail>>>,
    captured_decisions: usize,
    captured_events: usize,
    engine_steps: usize,
    action_count: usize,
    action: Option<ActionCapture>,
    _mlp_statuses: Vec<InferenceStatus>,
}

struct ActionCapture {
    actor: PlayerId,
    faction: String,
    start_frame: usize,
    start_state: GameState,
    last_state: GameState,
    transitions: Vec<StateTransition>,
    events: Vec<String>,
    decisions: Vec<DecisionDetail>,
    activated_systems: BTreeSet<String>,
}

struct StateTransition {
    before: GameState,
    after: GameState,
    events: Vec<String>,
}

impl LiveReview {
    pub fn start(config: &SimulationConfig) -> Result<Self> {
        if config.rotation >= FACTIONS.len() {
            return Err(ReviewError::Invalid(
                "rotation must be 0 through 5".to_owned(),
            ));
        }
        if !config.temperature.is_finite() || config.temperature <= 0.0 {
            return Err(ReviewError::Invalid(
                "temperature must be a finite number greater than zero".to_owned(),
            ));
        }
        let (checkpoint_path, checkpoint_bytes, policy, policy_summary) =
            load_policy(&config.checkpoint, config.table)?;
        let pool_bytes = read_bounded(&config.map_pool)?;
        let pool = MapPool::load_verified(&config.map_pool, &pool_bytes)
            .map_err(|error| ReviewError::Invalid(format!("map pool: {error}")))?;
        let content = ContentStore::embedded();
        pool.validate_systems(content, FULL)
            .map_err(|error| ReviewError::Invalid(format!("map pool content: {error}")))?;

        let players: Vec<PlayerId> = (0..FACTIONS.len())
            .map(|index| PlayerId::new(format!("seat{index}")))
            .collect();
        let faction_order = seeded_faction_order(config.seed);
        let factions: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .enumerate()
            .map(|(index, player)| {
                (
                    player.clone(),
                    FactionId::new(faction_order[(index + config.rotation) % FACTIONS.len()]),
                )
            })
            .collect();
        let decisions = Rc::new(RefCell::new(Vec::new()));
        let decision_sink = Rc::clone(&decisions);
        let decider_players = players.clone();
        let decider_factions = factions.clone();
        let mlp_status_sink = Rc::new(RefCell::new(Vec::new()));
        let status_sink = Rc::clone(&mlp_status_sink);
        let temperature = config.temperature;
        let map = OpeningMap::PythonPool {
            pool: Arc::new(pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        };
        let game = setup_game_with_decider_factory(
            content,
            &players,
            &factions,
            FULL,
            config.seed,
            &map,
            move |baselines| {
                let mut table: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, player) in decider_players.iter().enumerate() {
                    let faction = decider_factions
                        .get(player)
                        .expect("complete fixed seating")
                        .to_string();
                    let stream = config
                        .seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(index as u64);
                    let baseline = baselines.get(player).copied().unwrap_or_default();
                    let decider: Box<dyn Decider> = match &policy {
                        LoadedPolicy::Linear(profiles) => {
                            let mut profile = profiles
                                .get(&faction)
                                .expect("profiles validated before setup")
                                .clone();
                            for head in profile.learned.heads.values_mut() {
                                head.temperature = temperature;
                            }
                            let bot = LearnedBot::from_shared(Arc::new(profile), stream)
                                .from_setup(baseline);
                            Box::new(TraceBot {
                                inner: bot,
                                faction,
                                log: Rc::clone(&decision_sink),
                            })
                        }
                        LoadedPolicy::Mlp { actor, vocabulary } => {
                            let row = FactionRow::of(&faction)
                                .map_err(|error| format!("MLP faction: {error}"))?;
                            let bot = MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                                .at_temperature(temperature)
                                .from_setup(baseline);
                            let (inner, status) = bot.seat();
                            status_sink.borrow_mut().push(status);
                            Box::new(MlpTraceBot {
                                inner,
                                actor: Rc::clone(actor),
                                vocabulary: vocabulary.clone(),
                                row,
                                baseline,
                                faction,
                                temperature,
                                log: Rc::clone(&decision_sink),
                            })
                        }
                    };
                    table.insert(player.clone(), decider);
                }
                Ok(table)
            },
        )
        .map_err(|error| ReviewError::Invalid(format!("game setup: {error}")))?;

        let board = board_metadata(content, game.galaxy().expect("setup installs galaxy"));
        let planet_catalog = planet_catalog(content);
        let manifest = SessionManifest {
            checkpoint_path: checkpoint_path.display().to_string(),
            checkpoint_sha256: sha256(&checkpoint_bytes),
            map_pool_path: config.map_pool.display().to_string(),
            map_pool_sha256: sha256(&pool_bytes),
            seed: config.seed,
            tile_seed: config.seed.wrapping_add(TILE_SEED_OFFSET),
            rotation: config.rotation,
            profile_table: config.table,
            temperature: config.temperature,
            policy: policy_summary,
            factions: players
                .iter()
                .map(|player| factions[player].to_string())
                .collect(),
        };
        let initial = ReviewFrame {
            index: 0,
            engine_step: 0,
            decision_count: 0,
            action_count: 0,
            round: game.state.round,
            phase: game.state.phase,
            active: game.state.active.as_ref().map(ToString::to_string),
            resolved_choice: false,
            action_completed: false,
            finished: game.state.finished,
            error: None,
            new_events: game.events.clone(),
            decisions: Vec::new(),
            action_summary: None,
            state: game.state.clone(),
        };
        Ok(Self {
            session: ReviewSession {
                schema: SESSION_SCHEMA.to_owned(),
                version: SESSION_VERSION,
                manifest,
                board,
                planet_catalog,
                frames: vec![initial],
                outcome: SessionOutcome::InProgress,
            },
            captured_events: game.events.len(),
            game,
            decisions,
            captured_decisions: 0,
            engine_steps: 0,
            action_count: 0,
            action: None,
            _mlp_statuses: mlp_status_sink.borrow_mut().drain(..).collect(),
        })
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self.session.outcome,
            SessionOutcome::Completed | SessionOutcome::EngineFailed { .. }
        )
    }

    #[must_use]
    pub fn step_once(&mut self) -> &ReviewFrame {
        if self.is_terminal() || self.session.frames.len() >= MAX_FRAMES {
            if self.session.frames.len() >= MAX_FRAMES {
                self.session.outcome = SessionOutcome::SafetyLimit {
                    steps: self.engine_steps,
                };
            }
            return self.session.latest();
        }
        self.begin_action_capture();
        let result = self.game.step();
        self.engine_steps += 1;
        let decisions = {
            let log = self.decisions.borrow();
            let found = log[self.captured_decisions..].to_vec();
            self.captured_decisions = log.len();
            found
        };
        let new_events = self.game.events[self.captured_events..].to_vec();
        self.captured_events = self.game.events.len();
        if let Some(action) = &mut self.action {
            let before = std::mem::replace(&mut action.last_state, self.game.state.clone());
            action.transitions.push(StateTransition {
                before,
                after: self.game.state.clone(),
                events: new_events.clone(),
            });
            action.events.extend(new_events.iter().cloned());
            action.decisions.extend(decisions.iter().cloned());
            if let Some(system) = &self.game.state.active_system {
                action.activated_systems.insert(system.to_string());
            }
        }
        let action_finished = self.action.as_ref().is_some_and(|action| {
            action_period_ended(&action.actor, &self.game.state, result.finished)
        });
        let action_summary = action_finished.then(|| {
            let action = self.action.take().expect("checked above");
            summarize_action(
                action,
                &self.game.state,
                &self.session.board,
                self.session.frames.len(),
            )
        });
        let action_completed = action_summary.is_some();
        if action_completed {
            self.action_count += 1;
        }
        let error = result.error.as_ref().map(ToString::to_string);
        if let Some(error) = &error {
            self.session.outcome = SessionOutcome::EngineFailed {
                error: error.clone(),
            };
        } else if result.finished {
            self.session.outcome = SessionOutcome::Completed;
        }
        let frame = ReviewFrame {
            index: self.session.frames.len(),
            engine_step: self.engine_steps,
            decision_count: self.game.table.log.len(),
            action_count: self.action_count,
            round: self.game.state.round,
            phase: result.phase,
            active: result.active.as_ref().map(ToString::to_string),
            resolved_choice: result.resolved_choice || !decisions.is_empty(),
            action_completed,
            finished: result.finished,
            error,
            new_events,
            decisions,
            action_summary,
            state: self.game.state.clone(),
        };
        self.session.frames.push(frame);
        self.session.latest()
    }

    fn begin_action_capture(&mut self) {
        if self.action.is_some() || self.game.state.phase != Phase::Action {
            return;
        }
        let Some(actor) = self.game.state.active.clone() else {
            return;
        };
        let faction = self
            .game
            .state
            .player(&actor)
            .map_or_else(|| "unknown".to_owned(), |player| player.faction.to_string());
        self.action = Some(ActionCapture {
            actor,
            faction,
            start_frame: self.session.frames.len().saturating_sub(1),
            start_state: self.game.state.clone(),
            last_state: self.game.state.clone(),
            transitions: Vec::new(),
            events: Vec::new(),
            decisions: Vec::new(),
            activated_systems: BTreeSet::new(),
        });
    }

    pub fn advance(&mut self, unit: AdvanceUnit, count: usize) -> AdvanceReport {
        let wanted = count.min(MAX_RUN_COUNT);
        let mut report = AdvanceReport {
            steps: 0,
            decisions: 0,
            actions: 0,
            reached_target: wanted == 0,
        };
        while !report.reached_target && report.steps < MAX_COMMAND_STEPS && !self.is_terminal() {
            let frame = self.step_once().clone();
            report.steps += 1;
            report.decisions += frame.decisions.len();
            report.actions += usize::from(frame.action_completed);
            report.reached_target = match unit {
                AdvanceUnit::Step => report.steps >= wanted,
                AdvanceUnit::Decision => report.decisions >= wanted,
                AdvanceUnit::Action => report.actions >= wanted,
            };
        }
        if !report.reached_target && !self.is_terminal() {
            self.session.outcome = SessionOutcome::SafetyLimit {
                steps: report.steps,
            };
        }
        report
    }

    pub fn advance_to_next_round(&mut self) -> AdvanceReport {
        let start = self.game.state.round;
        self.advance_until(|review| review.game.state.round > start)
    }

    pub fn advance_to_end(&mut self) -> AdvanceReport {
        self.advance_until(Self::is_terminal)
    }

    fn advance_until(&mut self, predicate: impl Fn(&Self) -> bool) -> AdvanceReport {
        let mut report = AdvanceReport {
            steps: 0,
            decisions: 0,
            actions: 0,
            reached_target: predicate(self),
        };
        while !report.reached_target && report.steps < MAX_COMMAND_STEPS && !self.is_terminal() {
            let frame = self.step_once().clone();
            report.steps += 1;
            report.decisions += frame.decisions.len();
            report.actions += usize::from(frame.action_completed);
            report.reached_target = predicate(self);
        }
        if !report.reached_target && !self.is_terminal() {
            self.session.outcome = SessionOutcome::SafetyLimit {
                steps: report.steps,
            };
        }
        report
    }
}

fn seeded_faction_order(seed: u64) -> [&'static str; FACTIONS.len()] {
    // Keep seating independent from the map and policy RNG streams while making it reproducible
    // from the review's public seed. Rotation remains a cyclic physical-seat counterbalance over
    // this seed-specific permutation.
    let mut order = FACTIONS;
    let mut state = seed ^ 0x4641_4354_494f_4e53;
    for upper in (1..order.len()).rev() {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut random = state;
        random = (random ^ (random >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        random = (random ^ (random >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        random ^= random >> 31;
        let other = usize::try_from(random % (upper as u64 + 1)).expect("index fits usize");
        order.swap(upper, other);
    }
    order
}

fn action_period_ended(actor: &PlayerId, state: &GameState, finished: bool) -> bool {
    state.phase != Phase::Action || state.active.as_ref() != Some(actor) || finished
}

fn summarize_action(
    action: ActionCapture,
    end: &GameState,
    board: &[BoardTile],
    end_frame: usize,
) -> ActionSummary {
    let chosen_actions: Vec<String> = action
        .decisions
        .iter()
        .filter(|decision| decision.requested_head == "turn")
        .filter_map(selected_option)
        .filter(|option| option.kind != ti4_engine::transactions::OPEN_KIND)
        .map(|option| option.label.clone())
        .collect();
    let system_names: Vec<String> = action
        .activated_systems
        .iter()
        .map(|system| system_label(board, system))
        .collect();
    let headline = if action.events.iter().any(|event| event == "PLAYER_PASSED") {
        format!("{} ({}) passed", action.actor, action.faction)
    } else if action
        .events
        .iter()
        .any(|event| event == "STRATEGIC_ACTION_CANCELLED")
    {
        format!(
            "{} ({}) had their strategic action cancelled",
            action.actor, action.faction
        )
    } else if action
        .events
        .iter()
        .any(|event| event == "TACTICAL_ACTION_BEGAN" || event == "FREE_TACTICAL_ACTION")
    {
        system_names.first().map_or_else(
            || {
                format!(
                    "{} ({}) took a tactical action",
                    action.actor, action.faction
                )
            },
            |system| {
                format!(
                    "{} ({}) took a tactical action in {system}",
                    action.actor, action.faction
                )
            },
        )
    } else if action
        .events
        .iter()
        .any(|event| event == "STRATEGIC_ACTION_BEGAN")
    {
        format!(
            "{} ({}) took a strategic action",
            action.actor, action.faction
        )
    } else if chosen_actions.is_empty() {
        format!(
            "{} ({}) completed their active turn",
            action.actor, action.faction
        )
    } else {
        format!(
            "{} ({}) — {}",
            action.actor,
            action.faction,
            chosen_actions.join("; then ")
        )
    };

    let mut details = Vec::new();
    if !chosen_actions.is_empty() {
        details.push(format!(
            "Action choice{}: {}",
            if chosen_actions.len() == 1 {
                ""
            } else {
                "s (same active-player period)"
            },
            chosen_actions.join(" → ")
        ));
    }
    for system in &system_names {
        details.push(format!("Activated {system}"));
    }
    if action.transitions.is_empty() {
        append_unit_changes(
            &mut details,
            &action.start_state,
            end,
            board,
            &action.actor,
            false,
        );
        append_control_changes(&mut details, &action.start_state, end, board);
    } else {
        for transition in &action.transitions {
            let movement = transition.events.iter().any(|event| {
                matches!(
                    event.as_str(),
                    "SHIP_MOVED" | "UNITS_COMMITTED" | "RETREAT_DECLARED"
                )
            });
            append_unit_changes(
                &mut details,
                &transition.before,
                &transition.after,
                board,
                &action.actor,
                movement,
            );
            append_control_changes(&mut details, &transition.before, &transition.after, board);
        }
    }
    append_transactions(&mut details, &action.decisions, &action.events);
    append_notable_decisions(&mut details, &action.decisions);
    append_event_outcomes(&mut details, &action.events);
    for event in &action.events {
        if let Some(objective) = event.strip_prefix("OBJECTIVE_SCORED:") {
            details.push(format!("Scored objective {objective}"));
        }
    }
    if details.is_empty() {
        details.push("No lasting public-state change was recorded.".to_owned());
    }
    ActionSummary {
        actor: action.actor.to_string(),
        faction: action.faction,
        headline,
        start_frame: action.start_frame,
        end_frame,
        details,
    }
}

fn append_notable_decisions(details: &mut Vec<String>, decisions: &[DecisionDetail]) {
    for decision in decisions {
        let Some(option) = selected_option(decision) else {
            continue;
        };
        if decision.requested_head == "turn"
            || decision.prompt.starts_with("transaction with ")
            || decision.prompt.contains(" -- accept?")
        {
            continue;
        }
        if matches!(option.kind.as_str(), "decline" | "pass") {
            continue;
        }
        details.push(format!(
            "{}: {} → {}",
            decision.player, decision.prompt, option.label
        ));
    }
}

fn append_event_outcomes(details: &mut Vec<String>, events: &[String]) {
    const OUTCOMES: [(&str, &str); 12] = [
        (
            "STRATEGIC_ACTION_CANCELLED",
            "Strategic action was cancelled",
        ),
        ("COMPONENT_ACTION_FAILED", "Component action failed"),
        ("COMPONENT_ACTION_RESOLVED", "Component action resolved"),
        (
            "ACTION_CARD_UNRESOLVED",
            "An action card was cancelled or left unresolved",
        ),
        ("RETREAT_DECLARED", "A retreat was declared"),
        ("SPACE_COMBAT_WON", "Space combat was won"),
        ("SPACE_COMBAT_RESOLVED", "Space combat resolved"),
        ("INVASION_BEGAN", "Invasion began"),
        ("FRONTIER_EXPLORED", "A frontier token was explored"),
        ("TURN_RETAINED", "The active player retained the turn"),
        ("TURN_SKIPPED", "A player's turn was skipped"),
        (
            "TURN_ENDED_BY_MINISTER_OF_PEACE",
            "The turn was ended by Minister of Peace",
        ),
    ];
    for (event, label) in OUTCOMES {
        let count = events
            .iter()
            .filter(|candidate| candidate.as_str() == event)
            .count();
        if count == 1 {
            details.push(label.to_owned());
        } else if count > 1 {
            details.push(format!("{label} ×{count}"));
        }
    }
}

fn selected_option(decision: &DecisionDetail) -> Option<&OptionDetail> {
    let chosen = decision.chosen.as_deref()?;
    decision.options.iter().find(|option| option.id == chosen)
}

fn system_label(board: &[BoardTile], system: &str) -> String {
    board
        .iter()
        .find(|tile| tile.system == system)
        .map_or_else(|| format!("system {system}"), |tile| tile.label.clone())
}

fn location_label(board: &[BoardTile], location: &str) -> String {
    if let Some(system) = location.strip_prefix("space:") {
        return format!("{} space", system_label(board, system));
    }
    if let Some(planet) = location.strip_prefix("planet:") {
        return board
            .iter()
            .flat_map(|tile| &tile.planets)
            .find(|candidate| candidate.id == planet)
            .map_or_else(
                || format!("planet {planet}"),
                |candidate| candidate.label.clone(),
            );
    }
    location.to_owned()
}

fn unit_kind(unit: &ti4_model::units::Unit) -> String {
    ti4_content::units::unit_type(ContentStore::embedded(), unit.type_id.as_str(), FULL)
        .map_or_else(
            || unit.type_id.to_string(),
            |kind| kind.base_type().to_owned(),
        )
}

fn unit_locations(state: &GameState) -> BTreeMap<(String, String, String), i32> {
    let mut counts = BTreeMap::new();
    for (system, system_state) in &state.board {
        for unit in &system_state.units {
            *counts
                .entry((
                    unit.owner.to_string(),
                    unit_kind(unit),
                    format!("space:{system}"),
                ))
                .or_default() += 1;
        }
        for (planet, units) in &system_state.planet_units {
            for unit in units {
                *counts
                    .entry((
                        unit.owner.to_string(),
                        unit_kind(unit),
                        format!("planet:{planet}"),
                    ))
                    .or_default() += 1;
            }
        }
    }
    counts
}

fn append_unit_changes(
    details: &mut Vec<String>,
    start: &GameState,
    end: &GameState,
    board: &[BoardTile],
    actor: &PlayerId,
    movement: bool,
) {
    let before = unit_locations(start);
    let after = unit_locations(end);
    let identities: BTreeSet<(String, String)> = before
        .keys()
        .chain(after.keys())
        .map(|(owner, kind, _)| (owner.clone(), kind.clone()))
        .collect();
    for (owner, kind) in identities {
        let locations: BTreeSet<String> = before
            .keys()
            .chain(after.keys())
            .filter(|(candidate, candidate_kind, _)| candidate == &owner && candidate_kind == &kind)
            .map(|(_, _, location)| location.clone())
            .collect();
        let mut departures = Vec::new();
        let mut arrivals = Vec::new();
        for location in locations {
            let key = (owner.clone(), kind.clone(), location.clone());
            let delta = after.get(&key).copied().unwrap_or_default()
                - before.get(&key).copied().unwrap_or_default();
            if delta < 0 {
                departures.push((location, -delta));
            } else if delta > 0 {
                arrivals.push((location, delta));
            }
        }
        if movement {
            for (from, remaining) in &mut departures {
                for (to, available) in &mut arrivals {
                    let moved = (*remaining).min(*available);
                    if moved == 0 {
                        continue;
                    }
                    let owner_label = if owner == actor.as_str() {
                        String::new()
                    } else {
                        format!("{owner}: ")
                    };
                    details.push(format!(
                        "{owner_label}Moved {moved} {kind} from {} to {}",
                        location_label(board, from),
                        location_label(board, to)
                    ));
                    *remaining -= moved;
                    *available -= moved;
                }
            }
        }
        for (location, count) in departures.into_iter().filter(|(_, count)| *count > 0) {
            details.push(format!(
                "{owner}: lost or removed {count} {kind} at {}",
                location_label(board, &location)
            ));
        }
        for (location, count) in arrivals.into_iter().filter(|(_, count)| *count > 0) {
            details.push(format!(
                "{owner}: added {count} {kind} at {}",
                location_label(board, &location)
            ));
        }
    }
}

fn append_control_changes(
    details: &mut Vec<String>,
    start: &GameState,
    end: &GameState,
    board: &[BoardTile],
) {
    for tile in board {
        for planet in &tile.planets {
            let before = start
                .board
                .get(&SystemId::new(&tile.system))
                .and_then(|state| state.planet_control.get(planet.id.as_str()));
            let after = end
                .board
                .get(&SystemId::new(&tile.system))
                .and_then(|state| state.planet_control.get(planet.id.as_str()));
            if before != after {
                details.push(match (before, after) {
                    (Some(old), Some(new)) => {
                        format!("{new} took {} from {old}", planet.label)
                    }
                    (None, Some(new)) => format!("{new} took control of {}", planet.label),
                    (Some(old), None) => format!("{old} lost control of {}", planet.label),
                    (None, None) => continue,
                });
            }
        }
    }
}

fn append_transactions(details: &mut Vec<String>, decisions: &[DecisionDetail], events: &[String]) {
    for decision in decisions {
        let Some(option) = selected_option(decision) else {
            continue;
        };
        if decision.prompt.starts_with("transaction with ") {
            if option.id == "decline" {
                details.push(format!(
                    "{} opened {}, but made no offer",
                    decision.player, decision.prompt
                ));
            } else {
                details.push(format!(
                    "Transaction offer by {}: “{}” ({})",
                    decision.player, option.label, decision.prompt
                ));
            }
        } else if decision.prompt.contains(" -- accept?") {
            details.push(format!(
                "Transaction response: {} chose “{}” to {}",
                decision.player,
                option.label,
                decision.prompt.trim_end_matches(" -- accept?")
            ));
        }
    }
    let completed = events
        .iter()
        .filter(|event| matches!(event.as_str(), "TRANSACTION" | "TRANSACTION_RESOLVED"))
        .count();
    let refused = events
        .iter()
        .filter(|event| *event == "TRANSACTION_REFUSED")
        .count();
    let rejected = events
        .iter()
        .filter(|event| *event == "TRANSACTION_REJECTED")
        .count();
    let abandoned = events
        .iter()
        .filter(|event| *event == "TRANSACTION_ABANDONED")
        .count();
    if completed + refused + rejected + abandoned > 0 {
        details.push(format!(
            "Transaction outcomes: {completed} completed, {refused} refused, {rejected} illegal/rejected, {abandoned} abandoned"
        ));
    }
}

fn load_policy(
    path: &Path,
    selection: ProfileTable,
) -> Result<(PathBuf, Vec<u8>, LoadedPolicy, PolicySummary)> {
    let bundle_directory = if path.is_dir() {
        Some(path.to_path_buf())
    } else if matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("manifest.json" | "slots.json")
    ) && path
        .parent()
        .is_some_and(|parent| parent.join("manifest.json").is_file())
    {
        path.parent().map(Path::to_path_buf)
    } else {
        None
    };

    if let Some(directory) = bundle_directory {
        ti4_tensor::configure_deterministic(20_260_821)
            .map_err(|error| ReviewError::Invalid(format!("MLP runtime: {error}")))?;
        let loaded = ti4_mlp::bundle::read(&directory)
            .map_err(|error| ReviewError::Invalid(format!("MLP checkpoint bundle: {error}")))?;
        let manifest_path = directory.join("manifest.json");
        let manifest_bytes = read_bounded(&manifest_path)?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| ReviewError::Invalid(format!("MLP manifest JSON: {error}")))?;
        let summary = mlp_policy_summary(&manifest);
        return Ok((
            directory,
            manifest_bytes,
            LoadedPolicy::Mlp {
                actor: Rc::new(loaded.actor),
                vocabulary: loaded.vocabulary,
            },
            summary,
        ));
    }

    let bytes = read_bounded(path)?;
    let profiles = load_profiles(&bytes, selection).map_err(|error| {
        if path.file_name().and_then(|name| name.to_str()) == Some("slots.json") {
            ReviewError::Invalid(
                "slots.json is only one MLP bundle component; select it beside a valid manifest.json"
                    .to_owned(),
            )
        } else {
            error
        }
    })?;
    let summary = linear_policy_summary(&profiles, selection);
    Ok((
        path.to_path_buf(),
        bytes,
        LoadedPolicy::Linear(profiles),
        summary,
    ))
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn mlp_policy_summary(manifest: &Value) -> PolicySummary {
    let trunk = manifest.get("trunk");
    let dimensions = [
        manifest
            .get("slot_count")
            .and_then(Value::as_u64)
            .map(|count| format!("{count} occupied slots")),
        manifest
            .get("slot_capacity")
            .and_then(Value::as_u64)
            .map(|count| format!("{count} slot capacity")),
        manifest
            .get("embed_dim")
            .and_then(Value::as_u64)
            .map(|width| format!("embedding {width}")),
        trunk
            .and_then(|trunk| trunk.get("width"))
            .and_then(Value::as_u64)
            .map(|width| format!("trunk width {width}")),
        trunk
            .and_then(|trunk| trunk.get("depth"))
            .and_then(Value::as_u64)
            .map(|depth| format!("depth {depth}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    PolicySummary {
        format: "MLP inference bundle".to_owned(),
        schema: manifest.get("schema").and_then(Value::as_u64),
        name: None,
        source: manifest
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_owned),
        git_commit: manifest
            .get("git_commit")
            .and_then(Value::as_str)
            .map(str::to_owned),
        update: manifest.get("update").and_then(Value::as_u64),
        dimensions: (!dimensions.is_empty()).then_some(dimensions),
        heads: strings(manifest.get("heads")),
        factions: strings(manifest.get("factions")),
        profiles: FACTIONS
            .iter()
            .map(|faction| format!("{faction} · faction-conditioned row"))
            .collect(),
    }
}

fn linear_policy_summary(
    profiles: &BTreeMap<String, Profile>,
    selection: ProfileTable,
) -> PolicySummary {
    let listed = FACTIONS
        .iter()
        .filter_map(|faction| profiles.get(*faction).map(|profile| (*faction, profile)))
        .map(|(faction, profile)| {
            format!(
                "{faction}: {} · schema {} · {} heads · {} features/head",
                profile.name,
                profile.schema,
                profile.learned.heads.len(),
                profile.dimensions()
            )
        })
        .collect();
    PolicySummary {
        format: "Linear profile table".to_owned(),
        name: Some(selection.label().to_owned()),
        factions: FACTIONS.map(str::to_owned).to_vec(),
        profiles: listed,
        ..PolicySummary::default()
    }
}

fn load_profiles(bytes: &[u8], selection: ProfileTable) -> Result<BTreeMap<String, Profile>> {
    let document: Value = serde_json::from_slice(bytes)
        .map_err(|error| ReviewError::Invalid(format!("checkpoint JSON: {error}")))?;
    let table = match selection {
        ProfileTable::Learner => document
            .get("learner_profiles")
            .or_else(|| document.get("profiles"))
            .unwrap_or(&document),
        ProfileTable::Accepted => document.get("accepted").ok_or_else(|| {
            ReviewError::Invalid("checkpoint has no accepted champion table".to_owned())
        })?,
    };
    let profiles: BTreeMap<String, Profile> = serde_json::from_value(table.clone())
        .map_err(|error| ReviewError::Invalid(format!("profile table: {error}")))?;
    for faction in FACTIONS {
        let profile = profiles
            .get(faction)
            .ok_or_else(|| ReviewError::Invalid(format!("profile table has no {faction}")))?;
        profile
            .validate(Some(faction))
            .map_err(|error| ReviewError::Invalid(format!("{faction} profile: {error}")))?;
        if !profile.is_explicit() {
            return Err(ReviewError::Invalid(format!(
                "{faction} profile uses hashed schema {}; reviewer requires explicit profiles",
                profile.schema
            )));
        }
    }
    Ok(profiles)
}

fn board_metadata(content: &ContentStore, galaxy: &ti4_content::galaxy::Galaxy) -> Vec<BoardTile> {
    let mut board: Vec<BoardTile> = galaxy
        .system_ids()
        .into_iter()
        .filter_map(|id| {
            let coord = galaxy.coord_of(id)?;
            let system = ti4_content::galaxy::system(content, id, FULL)?;
            let planets = system
                .planets()
                .into_iter()
                .filter_map(|planet_id| ti4_content::galaxy::planet(content, planet_id, FULL))
                .map(|planet| PlanetMeta {
                    id: planet.id().to_owned(),
                    label: planet.name().unwrap_or(planet.id()).to_owned(),
                    resources: planet.resources(),
                    influence: planet.influence(),
                    traits: planet.traits().into_iter().map(str::to_owned).collect(),
                    tech_specialties: planet
                        .tech_specialties()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    legendary: planet.is_legendary(),
                    space_station: planet.is_space_station(),
                })
                .collect();
            Some(BoardTile {
                system: id.to_owned(),
                label: system.name().unwrap_or(id).to_owned(),
                q: coord.q,
                r: coord.r,
                hyperlane: system.is_hyperlane(),
                planets,
            })
        })
        .collect();
    board.sort_by_key(|tile| (tile.q, tile.r));
    board
}

fn planet_catalog(content: &ContentStore) -> Vec<PlanetMeta> {
    ti4_content::galaxy::all_planets(content, FULL)
        .into_values()
        .map(|planet| PlanetMeta {
            id: planet.id().to_owned(),
            label: planet.name().unwrap_or(planet.id()).to_owned(),
            resources: planet.resources(),
            influence: planet.influence(),
            traits: planet.traits().into_iter().map(str::to_owned).collect(),
            tech_specialties: planet
                .tech_specialties()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            legendary: planet.is_legendary(),
            space_station: planet.is_space_station(),
        })
        .collect()
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|source| ReviewError::Read {
        path: path.to_owned(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(ReviewError::Invalid(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(ReviewError::Invalid(format!(
            "{} exceeds the input size limit",
            path.display()
        )));
    }
    fs::read(path).map_err(|source| ReviewError::Read {
        path: path.to_owned(),
        source,
    })
}

fn sha256(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub fn save_session(path: &Path, session: &ReviewSession) -> Result<()> {
    session.validate()?;
    let bytes = serde_json::to_vec(session)
        .map_err(|error| ReviewError::Invalid(format!("serialize session: {error}")))?;
    if bytes.len() > MAX_SESSION_BYTES {
        return Err(ReviewError::SessionTooLarge);
    }
    replace_file(path, &bytes)
}

pub fn load_session(path: &Path) -> Result<ReviewSession> {
    let bytes = read_bounded(path)?;
    let mut session: ReviewSession = serde_json::from_slice(&bytes)
        .map_err(|error| ReviewError::Invalid(format!("review session JSON: {error}")))?;
    session.validate()?;
    populate_action_summaries(&mut session);
    Ok(session)
}

fn populate_action_summaries(session: &mut ReviewSession) {
    if session
        .frames
        .iter()
        .any(|frame| frame.action_summary.is_some())
    {
        return;
    }
    let board = session.board.clone();
    let mut action: Option<ActionCapture> = None;
    let mut action_count = 0;
    for index in 0..session.frames.len() {
        let frame = &session.frames[index];
        if action.is_none()
            && frame.state.phase == Phase::Action
            && let Some(actor) = frame.state.active.clone()
        {
            let faction = frame
                .state
                .player(&actor)
                .map_or_else(|| "unknown".to_owned(), |player| player.faction.to_string());
            action = Some(ActionCapture {
                actor,
                faction,
                start_frame: index,
                start_state: frame.state.clone(),
                last_state: frame.state.clone(),
                transitions: Vec::new(),
                events: Vec::new(),
                decisions: Vec::new(),
                activated_systems: BTreeSet::new(),
            });
        } else if let Some(capture) = &mut action {
            let before = std::mem::replace(&mut capture.last_state, frame.state.clone());
            capture.transitions.push(StateTransition {
                before,
                after: frame.state.clone(),
                events: frame.new_events.clone(),
            });
            capture.events.extend(frame.new_events.iter().cloned());
            capture.decisions.extend(frame.decisions.iter().cloned());
            if let Some(system) = &frame.state.active_system {
                capture.activated_systems.insert(system.to_string());
            }
        }
        let finished = action.as_ref().is_some_and(|capture| {
            index > capture.start_frame
                && action_period_ended(&capture.actor, &frame.state, frame.finished)
        });
        let summary = finished.then(|| {
            summarize_action(
                action.take().expect("checked above"),
                &session.frames[index].state,
                &board,
                index,
            )
        });
        if summary.is_some() {
            action_count += 1;
        }
        let frame = &mut session.frames[index];
        frame.action_completed = summary.is_some();
        frame.action_summary = summary;
        frame.action_count = action_count;
    }
}

fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| ReviewError::Write {
            path: parent.to_owned(),
            source,
        })?;
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let temp = path.with_extension(format!("{extension}.tmp"));
    let backup = path.with_extension(format!("{extension}.bak"));
    fs::write(&temp, bytes).map_err(|source| ReviewError::Write {
        path: temp.clone(),
        source,
    })?;
    if backup.exists() {
        fs::remove_file(&backup).map_err(|source| ReviewError::Write {
            path: backup.clone(),
            source,
        })?;
    }
    let had_old = path.exists();
    if had_old {
        fs::rename(path, &backup).map_err(|source| ReviewError::Write {
            path: path.to_owned(),
            source,
        })?;
    }
    if let Err(source) = fs::rename(&temp, path) {
        if had_old {
            let _ = fs::rename(&backup, path);
        }
        return Err(ReviewError::Write {
            path: path.to_owned(),
            source,
        });
    }
    if had_old {
        fs::remove_file(&backup).map_err(|source| ReviewError::Write {
            path: backup,
            source,
        })?;
    }
    Ok(())
}

pub fn render_html(session: &ReviewSession) -> Result<String> {
    session.validate()?;
    let data = serde_json::to_string(session)
        .map_err(|error| ReviewError::Invalid(format!("serialize HTML data: {error}")))?
        .replace('<', "\\u003c");
    let content = ContentStore::embedded();
    let objective_meta: BTreeMap<String, Value> = session
        .frames
        .iter()
        .flat_map(|frame| &frame.state.revealed_objectives)
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|objective| {
            let record = content.get(
                ti4_model::content_types::ContentType::PublicObjectives,
                &objective,
            );
            let meta = serde_json::json!({
                "name": record.as_ref().and_then(|record| record.text("name")).unwrap_or(&objective),
                "text": record.as_ref().and_then(|record| record.text("text")).unwrap_or(""),
                "points": record.as_ref().and_then(|record| record.int("points")).unwrap_or(0),
            });
            (objective, meta)
        })
        .collect();
    let objective_meta = serde_json::to_string(&objective_meta)
        .map_err(|error| ReviewError::Invalid(format!("serialize objective metadata: {error}")))?
        .replace('<', "\\u003c");
    let template = r#"<!doctype html><html><head><meta charset="utf-8"><title>TI4 Review</title><style>
body{margin:0;background:#09111e;color:#e9f0fb;font:14px system-ui}header{padding:12px 18px;background:#111e31;position:sticky;top:0;z-index:2}
main{display:grid;grid-template-columns:2fr 1fr;gap:12px;padding:12px}section{background:#101b2c;border:1px solid #29415f;border-radius:8px;padding:12px}
#board{width:100%;height:720px}.tile{fill:#162b43;stroke:#7098bd;stroke-width:2}.hyper{fill:#342555}svg text{fill:#fff;text-anchor:middle;font-size:11px}.legend{font-size:12px;color:#afbdd0;margin:6px}
button,input{background:#1c304a;color:#fff;border:1px solid #5b7da1;border-radius:5px;padding:6px}pre{white-space:pre-wrap;word-break:break-word;max-height:500px;overflow:auto}
.player{border-left:7px solid var(--pc);background:#0b1625;padding:8px;margin:8px 0;border-radius:6px}.player h4{margin:0 0 6px}.stats{display:flex;flex-wrap:wrap;gap:5px}.stat,.chip{background:#1a2b42;border-radius:5px;padding:3px 6px}.sheet{margin-top:6px}.sheet b{color:var(--pc)}.chips{display:flex;flex-wrap:wrap;gap:4px;margin:3px 0 7px}.chip{box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--pc) 55%,transparent)}.objective,.action{background:#0b1625;border:1px solid #29415f;border-radius:6px;padding:7px;margin:5px 0}.objective small,.action small{color:#afbdd0}
</style></head><body><header><button onclick="move(-1)">Previous</button> <input id="frame" type="range" min="0" max="0" value="0" oninput="show(+this.value)"> <button onclick="move(1)">Next</button> <b id="where"></b></header>
<main><section><div class="legend">Thick outer edge = exclusive space control. Thin inner edge = planet control and is split when ownership is mixed. Planet fill = planet owner. Planet labels: resources/influence · C/H/I trait · B/G/R/Y specialty · ★ legendary · S station · × destroyed. Gray units are neutral; red slash = damaged; yellow ring = galvanized.</div><svg id="board" viewBox="-600 -500 1200 1000"></svg></section><section><h3>Current policy profiles</h3><div id="policy"></div><h3>Open objectives</h3><div id="objectives"></div><h3>Latest completed action</h3><div id="action"></div><h3>Table state</h3><div id="table-state"></div><h3>Player sheets</h3><div id="players"></div><h3>Decision</h3><pre id="decision"></pre><h3>Events</h3><pre id="events"></pre></section></main>
<script>const session=__SESSION_DATA__,objectiveMeta=__OBJECTIVE_META__;const slider=document.querySelector('#frame');slider.max=session.frames.length-1;let at=0;
const colors=['#e04242','#428eeb','#f2c638','#36b874','#ad67e0','#ee7e31'];
const esc=s=>String(s).replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]));
const colorOf=id=>/^seat\d+$/.test(String(id))?colors[(+String(id).slice(4))%6]:'#a6aeb8';
const list=x=>Array.from(x||[],String);const objList=x=>Object.entries(x||{}).map(([k,v])=>`${k} ×${v}`);
function chips(icon,title,values){values=list(values);return `<div class="sheet"><b>${icon} ${esc(title)} · ${values.length}</b><div class="chips">${values.length?values.map(v=>`<span class="chip">${esc(v)}</span>`).join(''):'<span class="chip">None</span>'}</div></div>`}
function planetsIn(t,f){const found=[...(t.planets||[])];for(const [id,system] of Object.entries(f.state.placed_planets||{}))if(system===t.system&&!found.some(p=>p.id===id)){const p=(session.planet_catalog||[]).find(candidate=>candidate.id===id);if(p)found.push(p)}return found}
function controlledPlanets(f,p){const held=[];for(const t of session.board){const s=f.state.board[t.system];if(!s)continue;for(const planet of planetsIn(t,f))if(s.planet_control?.[planet.id]===p.id){const attachments=list(f.state.planet_attachments?.[planet.id]).length;held.push(`${planet.label} ${planet.resources}/${planet.influence}${f.state.exhausted_planets.includes(planet.id)?' · exhausted':''}${attachments?` · ${attachments} attachment(s)`:''}`)}}return held}
function notesOf(f,p){const notes=Object.entries(f.state.promissory_notes||{}).filter(([,holder])=>holder===p.id).map(([note])=>`${note}${list(f.state.promissory_faceup).includes(note)?' · faceup':''}`);for(const[owner,holder]of Object.entries(f.state.support_holders||{}))if(holder===p.id)notes.push(`Support for the Throne:${owner} · faceup`);return [...new Set(notes)].sort()}
function playerCard(p,f){const c=colorOf(p.id);const scored=list(f.state.scored_objectives?.[p.id]);const strategy=list(p.strategy_cards).map(x=>p.exhausted_strategy_cards.includes(x)?`${x} · used`:x);const tech=list(p.technologies).map(x=>p.exhausted_technologies.includes(x)?`${x} · exhausted`:x);const relics=list(p.relics).map(x=>list(p.exhausted_relics).includes(x)?`${x} · exhausted`:x);return `<article class="player" style="--pc:${c}"><h4>● ${esc(p.id)} · ${esc(p.faction)} · ${p.victory_points} VP</h4><div class="stats"><span class="stat">◆ TG ${p.trade_goods}</span><span class="stat">◇ Com ${p.commodities}</span><span class="stat">▲ T ${p.tactic_tokens}</span><span class="stat">⬟ F ${p.fleet_tokens}</span><span class="stat">● S ${p.strategic_tokens}</span><span class="stat">${p.passed?'PASSED':'ACTIVE'}</span></div>${chips('◆','Strategy cards',strategy)}${chips('●','Planets',controlledPlanets(f,p))}${chips('⚙','Technologies',tech)}${chips('✓','Scored objectives',scored)}${chips('?','Secret objectives',p.secret_objectives)}${chips('▣','Action cards',p.action_cards)}${chips('✦','Relics / fragments',[...relics,...objList(p.relic_fragments)])}${chips('◈','Exploration cards in play',p.exploration_cards)}${chips('✉','Promissory notes',notesOf(f,p))}${chips('♟','Leaders',Object.entries(p.leaders||{}).map(([k,v])=>`${k} · ${v}`))}${chips('⌁','Plots',p.plots)}</article>`}
function tableState(f){const unclaimed=list(f.state.unclaimed_strategy_cards).map(card=>{const goods=f.state.strategy_card_goods?.[card]||0;return goods?`${card} · ${goods} TG`:card});return `<div class="stats"><span class="stat">♛ Speaker ${esc(f.state.speaker)}</span><span class="stat">◎ Custodians ${f.state.custodians_removed?'removed':'present'}</span><span class="stat">▣ Action discard ${list(f.state.discarded_action_cards).length}</span></div>${chips('◆','Unclaimed strategy cards',unclaimed)}${chips('⚖','Laws in play',Object.entries(f.state.laws||{}).map(([law,outcome])=>`${law} · ${outcome}`))}${chips('↯','Discarded action cards',f.state.discarded_action_cards)}`}
function policySummary(){const p=session.manifest.policy||{},format=p.format||'Legacy review · profile details unavailable',schema=p.schema==null?'':` · schema ${p.schema}`,source=p.source?`<div>Source: ${esc(p.source)}</div>`:'',commit=p.git_commit?`<small>Engine/training commit: ${esc(p.git_commit)}</small><br>`:'',update=p.update==null?'':`<small>Training update: ${p.update}</small><br>`,dimensions=p.dimensions?`<small>${esc(p.dimensions)}</small>`:'',mode=p.format==='MLP inference bundle'?'shared actor with faction rows':session.manifest.profile_table;return `<div class="action"><b>${esc(format)}${schema}</b><br><small>${esc(session.manifest.checkpoint_path)} · ${esc(mode)} · temperature ${session.manifest.temperature}</small>${source}${commit}${update}${dimensions}${chips('◫','Decision heads',p.heads)}${chips('♙','Available faction rows',p.factions)}${chips('ƒ','Loaded profiles',p.profiles)}</div>`}
function objectives(f){return list(f.state.revealed_objectives).map(id=>{const m=objectiveMeta[id]||{name:id,text:'',points:0},scored=Object.entries(f.state.scored_objectives||{}).filter(([,v])=>list(v).includes(id)).map(([p])=>p);return `<div class="objective"><b>${esc(m.name)} · ${m.points} VP</b><br><small>${esc(id)} · scored by ${esc(scored.join(', ')||'nobody')}</small><div>${esc(m.text)}</div></div>`}).join('')||'None revealed yet.'}
function actionSummary(i){for(let n=i;n>=0;n--){const a=session.frames[n].action_summary;if(a)return `<div class="action"><b>${esc(a.headline)}</b><br><small>frames ${a.start_frame}–${a.end_frame} · active-player period</small>${list(a.details).map(d=>`<div>• ${esc(d)}</div>`).join('')}</div>`}return 'No action-phase turn has completed yet.'}
function move(n){show(Math.max(0,Math.min(session.frames.length-1,at+n)))}
function show(i){at=i;slider.value=i;const f=session.frames[i];document.querySelector('#where').textContent=` frame ${i} · step ${f.engine_step} · round ${f.round} · ${f.phase}`;drawDynamic(f);document.querySelector('#policy').innerHTML=policySummary();document.querySelector('#objectives').innerHTML=objectives(f);document.querySelector('#action').innerHTML=actionSummary(i);document.querySelector('#table-state').innerHTML=tableState(f);document.querySelector('#players').innerHTML=f.state.players.map(p=>playerCard(p,f)).join('');document.querySelector('#decision').textContent=f.decisions.length?JSON.stringify(f.decisions,null,2):'No policy decision on this engine step.';document.querySelector('#events').textContent=f.new_events.join('\n')||'—'}
const ns='http://www.w3.org/2000/svg';function el(tag,attrs={},text=''){const n=document.createElementNS(ns,tag);for(const[k,v]of Object.entries(attrs))n.setAttribute(k,v);if(text)n.textContent=text;return n}
function kind(id){id=String(id).toLowerCase();for(const k of ['war_sun','space_dock','dreadnought','destroyer','flagship','carrier','cruiser','fighter','infantry','mech','pds'])if(id.includes(k))return k;return id}
function unit(svg,x,y,u,count){const c=colorOf(u.owner),k=kind(u.type_id),s=7;let n;if(k==='fighter')n=el('polygon',{points:`${x},${y-s} ${x-s},${y+s} ${x+s},${y+s}`,fill:c});else if(k==='destroyer')n=el('polygon',{points:`${x},${y-s} ${x+s},${y} ${x},${y+s} ${x-s},${y}`,fill:c});else if(k==='carrier'||k==='space_dock')n=el('rect',{x:x-s*1.4,y:y-s*.65,width:s*2.8,height:s*1.3,rx:2,fill:c});else if(k==='cruiser'||k==='pds')n=el('rect',{x:x-s,y:y-s,width:s*2,height:s*2,fill:c});else if(k==='dreadnought'||k==='mech')n=el('polygon',{points:Array.from({length:k==='mech'?5:6},(_,i)=>{const a=Math.PI*2*i/(k==='mech'?5:6)-Math.PI/2;return `${x+s*Math.cos(a)},${y+s*Math.sin(a)}`}).join(' '),fill:c});else n=el('circle',{cx:x,cy:y,r:k==='war_sun'?s*1.4:s,fill:c});n.setAttribute('stroke','#07101a');n.setAttribute('stroke-width','2');svg.appendChild(n);if(u.galvanized)svg.appendChild(el('circle',{cx:x,cy:y,r:s*1.7,fill:'none',stroke:'#ffd84d','stroke-width':2}));if(u.sustained_damage)svg.appendChild(el('line',{x1:x-s,y1:y+s,x2:x+s,y2:y-s,stroke:'#ff3030','stroke-width':3}));svg.appendChild(el('text',{x,y:y+17,'font-size':8},`${k.slice(0,2)}×${count}`))}
function draw(f){const svg=document.querySelector('#board');svg.innerHTML='';for(const t of session.board){const x=150*(t.q+t.r/2),y=130*t.r,pts=[];for(let k=0;k<6;k++){const a=Math.PI/6+Math.PI/3*k;pts.push(`${x+72*Math.cos(a)},${y+72*Math.sin(a)}`)}const state=f.state.board[t.system]||{units:[],planet_control:{},planet_units:{},command_tokens:[]};const groundKinds=['infantry','mech','pds','space_dock'];const owners=[...new Set(state.units.filter(u=>!groundKinds.includes(kind(u.type_id))).map(u=>u.owner))];const tile=el('polygon',{points:pts.join(' '),class:t.hyperlane?'tile hyper':'tile'});if(owners.length===1){tile.setAttribute('stroke',colorOf(owners[0]));tile.setAttribute('stroke-width','8')}svg.appendChild(tile);svg.appendChild(el('text',{x,y:y-53},t.label));const groups=new Map;for(const u of state.units){const key=[u.owner,kind(u.type_id),u.sustained_damage,u.galvanized].join('|');if(!groups.has(key))groups.set(key,{u,count:0});groups.get(key).count++}let gi=0;for(const {u,count} of groups.values()){unit(svg,x+(gi%5-2)*24,y-28+Math.floor(gi/5)*25,u,count);gi++}const pc=t.planets.length;for(let pi=0;pi<pc;pi++){const p=t.planets[pi],px=x+(pc===1?0:(pi-(pc-1)/2)*45),py=y+34,owner=state.planet_control?.[p.id],c=owner?colorOf(owner):'#5b6069';svg.appendChild(el('circle',{cx:px,cy:py,r:20,fill:c,stroke:owner?c:'#89919e','stroke-width':3}));svg.appendChild(el('text',{x:px,y:py-3,'font-size':9},`${p.resources}/${p.influence}`));const traits=(p.traits||[]).map(v=>v[0]).join(''),tech=(p.tech_specialties||[]).map(v=>({propulsion:'B',biotic:'G',warfare:'R',cybernetic:'Y'}[v.toLowerCase()]||'T')).join('');svg.appendChild(el('text',{x:px,y:py+9,'font-size':8},`${traits}${traits&&tech?'·':''}${tech}`));svg.appendChild(el('text',{x:px,y:py+31,'font-size':8},`${p.legendary?'★':''}${p.label}`));const ground=state.planet_units?.[p.id]||[];const gg=new Map;for(const u of ground){const key=[u.owner,kind(u.type_id),u.sustained_damage,u.galvanized].join('|');if(!gg.has(key))gg.set(key,{u,count:0});gg.get(key).count++}let ui=0;for(const {u,count} of gg.values()){unit(svg,px-10+ui*20,py-17,u,count);ui++}}for(let ci=0;ci<(state.command_tokens||[]).length;ci++)svg.appendChild(el('circle',{cx:x-52+ci*13,cy:y+55,r:5,fill:colorOf(state.command_tokens[ci]),stroke:'#fff'}))}}
function drawDynamic(f){
 const svg=document.querySelector('#board');svg.innerHTML='';
 for(const t of session.board){
  const x=150*(t.q+t.r/2),y=130*t.r,pts=[];for(let k=0;k<6;k++){const a=Math.PI/6+Math.PI/3*k;pts.push(`${x+72*Math.cos(a)},${y+72*Math.sin(a)}`)}
  const state=f.state.board[t.system]||{units:[],planet_control:{},planet_units:{},command_tokens:[],purged_planets:[],coexisting:{}},purgedSystem=list(f.state.purged_systems).includes(t.system),planets=planetsIn(t,f);
  const groundKinds=['infantry','mech','pds','space_dock'],owners=[...new Set(state.units.filter(u=>!groundKinds.includes(kind(u.type_id))).map(u=>u.owner))];
  const tile=el('polygon',{points:pts.join(' '),class:t.hyperlane?'tile hyper':'tile'});if(purgedSystem)tile.setAttribute('fill','#18181c');if(owners.length===1){tile.setAttribute('stroke',colorOf(owners[0]));tile.setAttribute('stroke-width','8')}svg.appendChild(tile);const planetOwners=[...new Set(planets.filter(p=>!list(state.purged_planets).includes(p.id)).map(p=>state.planet_control?.[p.id]).filter(Boolean))];const inner=pts.map(point=>{const[a,b]=point.split(',').map(Number);return `${x+(a-x)*.92},${y+(b-y)*.92}`});if(planetOwners.length===1)svg.appendChild(el('polygon',{points:inner.join(' '),fill:'none',stroke:colorOf(planetOwners[0]),'stroke-width':3}));else if(planetOwners.length>1)for(let edge=0;edge<inner.length;edge++){const[a,b]=inner[edge].split(','),[c,d]=inner[(edge+1)%inner.length].split(',');svg.appendChild(el('line',{x1:a,y1:b,x2:c,y2:d,stroke:colorOf(planetOwners[edge%planetOwners.length]),'stroke-width':4}))}svg.appendChild(el('text',{x,y:y-53},`${t.label}${purgedSystem?' · PURGED':''}`));
  const groups=new Map;for(const u of state.units){const key=[u.owner,kind(u.type_id),u.sustained_damage,u.galvanized].join('|');if(!groups.has(key))groups.set(key,{u,count:0});groups.get(key).count++}let gi=0;for(const {u,count} of groups.values()){unit(svg,x+(gi%5-2)*24,y-28+Math.floor(gi/5)*25,u,count);gi++}
  const tokens=[];if(list(f.state.frontier_tokens).includes(t.system))tokens.push('Frontier');for(const[k,s]of Object.entries(f.state.wormhole_tokens||{}))if(s===t.system)tokens.push(`${k} WH`);if(f.state.ion_storm?.[0]===t.system)tokens.push(`Ion ${f.state.ion_storm[1]}`);if(list(f.state.ingress_tokens).includes(t.system))tokens.push('Ingress');if(list(f.state.breach_tokens).includes(t.system))tokens.push('Breach');if(f.state.thunders_edge_system===t.system)tokens.push("Thunder's Edge");if(tokens.length)svg.appendChild(el('text',{x,y:y+59,'font-size':8,fill:'#81dded'},tokens.join(' · ')));
  const pc=planets.length;for(let pi=0;pi<pc;pi++){const p=planets[pi],px=x+(pc===1?0:(pi-(pc-1)/2)*45),py=y+34,owner=state.planet_control?.[p.id],purged=list(state.purged_planets).includes(p.id),c=purged?'#26262a':owner?colorOf(owner):'#5b6069';svg.appendChild(el('circle',{cx:px,cy:py,r:20,fill:c,stroke:owner&&!purged?c:'#89919e','stroke-width':3}));svg.appendChild(el('text',{x:px,y:py-3,'font-size':9},`${p.resources}/${p.influence}`));const traits=(p.traits||[]).map(v=>v[0]).join(''),tech=(p.tech_specialties||[]).map(v=>({propulsion:'B',biotic:'G',warfare:'R',cybernetic:'Y'}[v.toLowerCase()]||'T')).join('');svg.appendChild(el('text',{x:px,y:py+9,'font-size':8},`${traits}${traits&&tech?'·':''}${tech}`));const prefix=purged?'× ':p.space_station?'S ':p.legendary?'★':'';svg.appendChild(el('text',{x:px,y:py+31,'font-size':8},`${prefix}${p.label}`));const attachments=list(f.state.planet_attachments?.[p.id]);if(attachments.length)svg.appendChild(el('text',{x:px+18,y:py-17,'font-size':8,fill:'#ffd84d'},`+${attachments.length}`));const coexist=list(state.coexisting?.[p.id]);for(let ci=0;ci<coexist.length;ci++){const a=Math.PI*2*ci/coexist.length;svg.appendChild(el('circle',{cx:px+25*Math.cos(a),cy:py+25*Math.sin(a),r:4,fill:colorOf(coexist[ci]),stroke:'#fff'}))}const ground=state.planet_units?.[p.id]||[];const gg=new Map;for(const u of ground){const key=[u.owner,kind(u.type_id),u.sustained_damage,u.galvanized].join('|');if(!gg.has(key))gg.set(key,{u,count:0});gg.get(key).count++}let ui=0;for(const {u,count} of gg.values()){unit(svg,px-10+ui*20,py-17,u,count);ui++}}
  for(let ci=0;ci<(state.command_tokens||[]).length;ci++)svg.appendChild(el('circle',{cx:x-52+ci*13,cy:y+55,r:5,fill:colorOf(state.command_tokens[ci]),stroke:'#fff'}));
 }
}
show(0);</script></body></html>"#;
    let html = template
        .replace("__SESSION_DATA__", &data)
        .replace("__OBJECTIVE_META__", &objective_meta);
    if html.len() > MAX_HTML_BYTES {
        return Err(ReviewError::HtmlTooLarge);
    }
    Ok(html)
}

pub fn export_html(path: &Path, session: &ReviewSession) -> Result<()> {
    replace_file(path, render_html(session)?.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_session() -> ReviewSession {
        let players = (0..6)
            .map(|index| PlayerId::new(format!("seat{index}")))
            .collect::<Vec<_>>();
        let state = GameState::new(&players, &[], BTreeMap::new(), None, 1);
        let frame = ReviewFrame {
            index: 0,
            engine_step: 0,
            decision_count: 0,
            action_count: 0,
            round: 1,
            phase: Phase::Strategy,
            active: None,
            resolved_choice: false,
            action_completed: false,
            finished: false,
            error: None,
            new_events: vec![],
            decisions: vec![],
            action_summary: None,
            state,
        };
        ReviewSession {
            schema: SESSION_SCHEMA.to_owned(),
            version: SESSION_VERSION,
            manifest: SessionManifest {
                checkpoint_path: "checkpoint.json".to_owned(),
                checkpoint_sha256: "0".repeat(64),
                map_pool_path: "pool.json.gz".to_owned(),
                map_pool_sha256: "1".repeat(64),
                seed: 42,
                tile_seed: 20_000_042,
                rotation: 0,
                profile_table: ProfileTable::Learner,
                temperature: default_sampling_temperature(),
                policy: PolicySummary::default(),
                factions: FACTIONS.map(str::to_owned).to_vec(),
            },
            board: vec![],
            planet_catalog: vec![],
            frames: vec![frame],
            outcome: SessionOutcome::InProgress,
        }
    }

    #[test]
    fn session_round_trips_and_remains_incomplete() {
        let session = fixture_session();
        session.validate().unwrap();
        let json = serde_json::to_vec(&session).unwrap();
        let read: ReviewSession = serde_json::from_slice(&json).unwrap();
        assert_eq!(read, session);
        assert_eq!(read.outcome, SessionOutcome::InProgress);
    }

    #[test]
    fn old_sessions_default_the_sampling_temperature() {
        let mut value = serde_json::to_value(fixture_session()).unwrap();
        value["manifest"]
            .as_object_mut()
            .unwrap()
            .remove("temperature");

        let restored: ReviewSession = serde_json::from_value(value).unwrap();
        assert!(
            (restored.manifest.temperature - default_sampling_temperature()).abs() <= f64::EPSILON
        );
        restored.validate().unwrap();
    }

    #[test]
    fn old_sessions_default_the_dynamic_planet_catalog() {
        let mut value = serde_json::to_value(fixture_session()).unwrap();
        value.as_object_mut().unwrap().remove("planet_catalog");

        let restored: ReviewSession = serde_json::from_value(value).unwrap();
        assert!(restored.planet_catalog.is_empty());
        restored.validate().unwrap();
    }

    #[test]
    fn old_sessions_default_missing_policy_details() {
        let mut value = serde_json::to_value(fixture_session()).unwrap();
        value["manifest"].as_object_mut().unwrap().remove("policy");

        let restored: ReviewSession = serde_json::from_value(value).unwrap();
        assert_eq!(restored.manifest.policy, PolicySummary::default());
        restored.validate().unwrap();
    }

    #[test]
    fn current_schema_six_profile_metadata_is_displayable() {
        let summary = mlp_policy_summary(&serde_json::json!({
            "schema": 6,
            "source": "M10-034 PPO, 500 updates",
            "git_commit": "dbdaa61c2104fc307e330e124e69d11edc9fd0ab",
            "update": 52436,
            "slot_count": 11147,
            "slot_capacity": 16384,
            "embed_dim": 16,
            "heads": ["strategy", "turn", "other"],
            "factions": ["hacan", "jolnar", "sol"],
            "trunk": { "width": 256, "depth": 2 }
        }));

        assert_eq!(summary.format, "MLP inference bundle");
        assert_eq!(summary.schema, Some(6));
        assert_eq!(summary.update, Some(52_436));
        assert_eq!(summary.heads, ["strategy", "turn", "other"]);
        assert_eq!(summary.factions, ["hacan", "jolnar", "sol"]);
        assert_eq!(summary.profiles.len(), FACTIONS.len());
        assert!(
            summary
                .profiles
                .iter()
                .any(|profile| profile.starts_with("sol"))
        );
        assert!(
            summary
                .dimensions
                .as_deref()
                .unwrap()
                .contains("11147 occupied")
        );
        assert!(
            summary
                .dimensions
                .as_deref()
                .unwrap()
                .contains("trunk width 256")
        );
    }

    #[test]
    fn the_planet_catalog_contains_placed_planets_and_space_stations() {
        let catalog = planet_catalog(ContentStore::embedded());
        assert!(catalog.iter().any(|planet| planet.id == "mirage"));
        assert!(catalog.iter().any(|planet| planet.space_station));
    }

    #[test]
    fn a_non_positive_sampling_temperature_is_refused_before_input_loading() {
        let config = SimulationConfig {
            checkpoint: PathBuf::from("does-not-exist.json"),
            map_pool: PathBuf::from("does-not-exist.json.gz"),
            seed: 42,
            rotation: 0,
            table: ProfileTable::Learner,
            temperature: 0.0,
        };

        let error = match LiveReview::start(&config) {
            Ok(_) => panic!("zero temperature was accepted"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("temperature"), "{error}");
    }

    #[test]
    fn old_frames_without_action_summaries_still_load() {
        let session = fixture_session();
        let mut value = serde_json::to_value(session).unwrap();
        value["frames"][0]
            .as_object_mut()
            .unwrap()
            .remove("action_summary");
        let restored: ReviewSession = serde_json::from_value(value).unwrap();
        assert!(restored.frames[0].action_summary.is_none());
    }

    #[test]
    fn an_action_period_ends_on_active_player_handoff_not_on_a_retained_turn() {
        let mut state = fixture_session().frames[0].state.clone();
        state.phase = Phase::Action;
        let actor = state.players[0].id.clone();
        let next = state.players[1].id.clone();
        state.active = Some(actor.clone());
        state.turn_seq = 7;
        assert!(!action_period_ended(&actor, &state, false));
        state.turn_seq = 8;
        assert!(
            !action_period_ended(&actor, &state, false),
            "a retained turn remains the same active-player period"
        );
        state.active = Some(next);
        assert!(action_period_ended(&actor, &state, false));
    }

    #[test]
    fn an_old_replay_reconstructs_full_action_periods_on_load() {
        let mut session = fixture_session();
        let actor = session.frames[0].state.players[0].id.clone();
        let next = session.frames[0].state.players[1].id.clone();
        session.frames[0].phase = Phase::Action;
        session.frames[0].active = Some(actor.to_string());
        session.frames[0].state.phase = Phase::Action;
        session.frames[0].state.active = Some(actor.clone());
        session.frames[0].state.turn_seq = 7;
        let mut nested = session.frames[0].clone();
        nested.index = 1;
        nested.engine_step = 1;
        nested.new_events = vec!["TRANSACTION_OPENED".to_owned()];
        let mut handed_off = nested.clone();
        handed_off.index = 2;
        handed_off.engine_step = 2;
        handed_off.active = Some(next.to_string());
        handed_off.state.active = Some(next);
        handed_off.state.turn_seq = 8;
        session.frames.extend([nested, handed_off]);

        populate_action_summaries(&mut session);

        assert!(!session.frames[1].action_completed);
        assert!(session.frames[2].action_completed);
        assert_eq!(session.frames[2].action_count, 1);
        assert_eq!(
            session.frames[2]
                .action_summary
                .as_ref()
                .map(|summary| summary.actor.as_str()),
            Some(actor.as_str())
        );
    }

    #[test]
    fn unrelated_removal_and_creation_are_not_invented_as_movement() {
        let actor = PlayerId::new("seat0");
        let mut before = fixture_session().frames[0].state.clone();
        let mut after = before.clone();
        let from = SystemId::new("18");
        let to = SystemId::new("19");
        before
            .system_mut(&from)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("carrier"),
                actor.clone(),
            ));
        after
            .system_mut(&to)
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("carrier"),
                actor.clone(),
            ));
        let mut details = Vec::new();

        append_unit_changes(&mut details, &before, &after, &[], &actor, false);

        assert!(details.iter().all(|detail| !detail.contains("Moved")));
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("lost or removed"))
        );
        assert!(details.iter().any(|detail| detail.contains("added")));
    }

    #[test]
    fn final_engine_outcomes_are_explained() {
        let mut details = Vec::new();
        append_event_outcomes(
            &mut details,
            &[
                "STRATEGIC_ACTION_CANCELLED".to_owned(),
                "TURN_RETAINED".to_owned(),
                "COMPONENT_ACTION_FAILED".to_owned(),
            ],
        );
        assert!(details.iter().any(|detail| detail.contains("cancelled")));
        assert!(details.iter().any(|detail| detail.contains("retained")));
        assert!(details.iter().any(|detail| detail.contains("failed")));
    }

    #[test]
    fn broken_frame_sequence_is_refused() {
        let mut session = fixture_session();
        let mut second = session.frames[0].clone();
        second.index = 2;
        second.engine_step = 1;
        session.frames.push(second);
        assert!(session.validate().is_err());
    }

    #[test]
    fn html_is_self_contained_and_marks_replay_data() {
        let html = render_html(&fixture_session()).unwrap();
        assert!(html.contains("<svg id=\"board\""));
        assert!(html.contains("Thick outer edge = exclusive space control"));
        assert!(html.contains("function playerCard"));
        assert!(html.contains("function drawDynamic"));
        assert!(html.contains("function tableState"));
        assert!(html.contains("function policySummary"));
        assert!(html.contains("Current policy profiles"));
        assert!(html.contains("Promissory notes"));
        assert!(html.contains("Gray units are neutral"));
        assert!(html.contains("function unit(svg"));
        assert!(html.contains("Open objectives"));
        assert!(html.contains("Latest completed action"));
        assert!(html.contains("const a=Math.PI/6+Math.PI/3*k"));
        assert!(html.contains(SESSION_SCHEMA));
        assert!(!html.contains("<script src="));
        assert!(!html.contains("<link rel="));
        assert!(!html.contains("fetch("));
    }

    #[test]
    fn old_planet_metadata_defaults_new_visual_fields() {
        let planet: PlanetMeta = serde_json::from_value(serde_json::json!({
            "id": "jord",
            "label": "Jord",
            "resources": 4,
            "influence": 2
        }))
        .unwrap();
        assert!(planet.traits.is_empty());
        assert!(planet.tech_specialties.is_empty());
        assert!(!planet.legendary);
        assert!(!planet.space_station);
    }
}
