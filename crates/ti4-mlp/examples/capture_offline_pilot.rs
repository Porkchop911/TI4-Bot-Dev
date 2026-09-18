//! Capture a heterogeneous, seat-authorized, training-ready self-play pilot corpus.
//!
//! The primary decision shard stores the exact canonical actor/critic feature vectors used by the
//! MLP boundary, the engine-ordered legal actions, and the chosen action index. It deliberately
//! does not store a good/bad label or pretend that the behavior policy's probability is needed for
//! behavior cloning. Raw progress and terminal outcomes remain available for downstream ranking.
//!
//! # Parallel capture
//!
//! A game is independent by construction: its seed, faction draw, policy assignment and every RNG
//! stream are pure functions of the game index, so games play on one thread per logical processor
//! (rayon's global pool; `--workers N` pins a dedicated pool of exactly N instead). Each worker
//! chunk owns deep inference copies of both actors — `tch::Tensor` is `Send` but not `Sync`, so no
//! actor crosses a thread boundary, and inference never mutates the weights — and writes its own
//! per-game zstd frames to staging. At game end — while the records are still in memory — each
//! game is checked against the retention rule (`RETENTION_RULE`): retained games pass a
//! loss-alignment gate and then write their parts into the folder of their reason bucket (see
//! `bucket_for`: high-VP games go to `good/`, low-VP slogs to `bad/`, the seeded 5% control to
//! `random/`, engine failures to `failed/`); discarded games write nothing at all. The main thread
//! concatenates each bucket's retained frames in game order and proves every published shard is
//! byte-identical to those validated frames via a running sha256. Each training bucket folder is a
//! self-contained corpus (its own shards plus a scoped manifest), so downstream tooling can consume
//! any quality class directly. The shards are therefore byte-identical for a given seed base at any
//! worker count, which the package evidence proves by diffing `--workers 1` against the default.
//!
//! # Publishing killed runs
//!
//! `--publish-staging <dir>` publishes whatever complete games a killed run left in its staging
//! directory: every part pair is fully decoded and validated, truncated kill artifacts are
//! excluded with a report while decodable-but-inconsistent parts refuse the publish, and the
//! manifest's provenance (seed base, rounds, checkpoint manifests) is derived from the staged
//! metadata itself. The same assembly path as a completed run builds the shards, gates them
//! byte-exactly, and renames staging to the corpus directory.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::progress::{Baseline, Progress};
use ti4_policy::vocabulary::Vocabulary;

const SCHEMA: &str = "ti4-offline-selfplay-v3";
const OBSERVATION_SCHEMA: &str = "seat-authorized-canonical-mlp-v3";
const DECISIONS_FILE: &str = "decisions.jsonl.zst";
const GAMES_FILE: &str = "games.jsonl.zst";
const DIPLOMACY_FILE: &str = "diplomacy.jsonl.zst";
/// Published bucket folders, one per retention-reason class (see `bucket_for`). The three training
/// buckets always exist; `failed` appears only when a game actually fails.
const BUCKET_GOOD: &str = "good";
const BUCKET_BAD: &str = "bad";
const BUCKET_RANDOM: &str = "random";
const BUCKET_FAILED: &str = "failed";
const MANIFEST_FILE: &str = "manifest.json";
const DEFAULT_GAMES: usize = 12;
const DEFAULT_ROUNDS: u32 = 4;
const DEFAULT_SEED_BASE: u64 = 1_026_091_300;
const TILE_SEED_OFFSET: u64 = 0;
const IN_SCOPE_FACTIONS: [&str; 6] = ["jolnar", "letnev", "sol", "xxcha", "hacan", "l1z1x"];

/// Streaming retention rule (agreed with codex, 2026-09-13): a game is written to disk iff any
/// faction reaches `STANDOUT_VP`, or the table total reaches `STRONG_TABLE_VP`, or it falls
/// below `WEAK_TABLE_VP`; games in between are kept with probability 5% as a random control. Every
/// condition is decidable at game end while the records are still in memory, so discarded games
/// never touch disk. The coin is seeded from the game seed — a pure function of the game index —
/// so retention is reproducible at any worker count.
const RETENTION_RULE: &str = "vp-threshold-v1";
const STANDOUT_VP: i32 = 6; // inclusive
const STRONG_TABLE_VP: i32 = 24; // table total, inclusive
const WEAK_TABLE_VP: i32 = 10; // table total, exclusive
const RANDOM_CONTROL_NUMERATOR: u32 = 5;
const RANDOM_CONTROL_DENOMINATOR: u32 = 100;
const RETENTION_COIN_SALT: u64 = 0x5EED_5A17;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SparseFeature {
    name: String,
    column: usize,
    value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegalAction {
    index: usize,
    id: String,
    kind: String,
    label: String,
    payload: BTreeMap<String, serde_json::Value>,
    actor_features: Vec<SparseFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicSeatSnapshot {
    seat: String,
    faction: String,
    victory_points: i32,
    trade_goods: i32,
    commodities: i32,
    tactic_tokens: i32,
    fleet_tokens: i32,
    strategic_tokens: i32,
    strategy_cards: Vec<String>,
    exhausted_strategy_cards: Vec<String>,
    technologies: Vec<String>,
    exhausted_technologies: Vec<String>,
    action_cards_held: usize,
    secret_objectives_held: usize,
    passed: bool,
    scored_objectives: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObjectiveProgress {
    alias: String,
    family_token: String,
    have: f64,
    threshold: f64,
    satisfied: bool,
    stage: Option<u8>,
}

impl From<ti4_engine::objectives::CardProgress> for ObjectiveProgress {
    fn from(value: ti4_engine::objectives::CardProgress) -> Self {
        Self {
            alias: value.alias,
            family_token: value.family_token,
            have: value.have,
            threshold: value.threshold,
            satisfied: value.satisfied,
            stage: value.stage,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SeatAuthorizedObservation {
    round: u32,
    phase: String,
    active_player: Option<String>,
    active_system: Option<String>,
    pending_step: Option<String>,
    speaker: String,
    initiative_order: Vec<String>,
    revealed_public_objectives: Vec<String>,
    revealed_public_objective_progress: Vec<ObjectiveProgress>,
    public_seats: Vec<PublicSeatSnapshot>,
    public_board: serde_json::Value,
    laws: BTreeMap<String, String>,
    faceup_promissory_notes: BTreeMap<String, String>,
    held_action_cards: Vec<String>,
    held_secret_objectives: Vec<String>,
    held_secret_progress: Vec<ObjectiveProgress>,
    held_promissory_notes: Vec<String>,
    #[serde(default)]
    diplomacy_relationships: Vec<DiplomacyRelationshipObservation>,
    #[serde(default)]
    active_diplomacy_deals: Vec<ti4_model::Deal>,
    #[serde(default)]
    recent_diplomacy_signals: Vec<ti4_model::Signal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiplomacyRelationshipObservation {
    observer: String,
    subject: String,
    relationship: ti4_model::Relationship,
    recent_attack: bool,
    recent_breach: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CapturedDecision {
    game_id: String,
    seat: String,
    faction: String,
    policy_id: String,
    policy_rng_seed: u64,
    seat_decision_index: usize,
    head: String,
    prompt: String,
    context: Option<ti4_engine::decision_context::DecisionContext>,
    observation: SeatAuthorizedObservation,
    progress: Progress,
    critic_features: Vec<SparseFeature>,
    legal_actions: Vec<LegalAction>,
    chosen_action_index: usize,
    chosen_action_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyMetadata {
    policy_id: String,
    family: String,
    checkpoint: Option<String>,
    checkpoint_manifest_sha256: Option<String>,
    temperature: Option<f64>,
    bias: Option<String>,
    source_profile: Option<String>,
    rng_seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SeatMetadata {
    seat: String,
    faction: String,
    policy: PolicyMetadata,
    final_progress: Progress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapPlacement {
    system: String,
    q: i32,
    r: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GameMetadata {
    game_id: String,
    game_index: usize,
    game_seed: u64,
    tile_seed_offset: u64,
    rounds_requested: u32,
    completed: bool,
    error: Option<String>,
    map_placements: Vec<MapPlacement>,
    seats: Vec<SeatMetadata>,
    decision_count: usize,
    #[serde(default)]
    diplomacy_enabled: bool,
    #[serde(default)]
    diplomacy_deal_count: usize,
    #[serde(default)]
    diplomacy_signal_count: usize,
    #[serde(default)]
    diplomacy_telemetry: DiplomacyTelemetry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DiplomacyTelemetry {
    opportunities: usize,
    offers: usize,
    signals: usize,
    accepts: usize,
    declines: usize,
    counters: usize,
    fulfilled: usize,
    broken: usize,
    expired: usize,
    diplomacy_decisions: usize,
    legal_options: usize,
    max_initial_candidates: usize,
    max_counter_candidates: usize,
    active_deal_high_water: usize,
    final_relationships: Vec<ti4_model::Relationship>,
}

fn diplomacy_telemetry<'a>(
    state: &ti4_model::state::GameState,
    decisions: impl Iterator<Item = &'a CapturedDecision>,
) -> DiplomacyTelemetry {
    let mut telemetry = DiplomacyTelemetry::default();
    for decision in decisions.filter(|decision| decision.head == "diplomacy") {
        telemetry.opportunities += 1;
        telemetry.diplomacy_decisions += 1;
        telemetry.legal_options += decision.legal_actions.len();
        let initial = decision
            .legal_actions
            .iter()
            .filter(|option| option.kind == "diplomacy_offer")
            .count();
        let counters = decision
            .legal_actions
            .iter()
            .filter(|option| option.kind == "diplomacy_counter")
            .count();
        telemetry.max_initial_candidates = telemetry.max_initial_candidates.max(initial);
        telemetry.max_counter_candidates = telemetry.max_counter_candidates.max(counters);
    }
    let mut active = BTreeSet::new();
    for entry in &state.diplomacy.journal {
        match &entry.event {
            ti4_model::DiplomacyEvent::Offered { deal_id, .. } => {
                telemetry.offers += 1;
                active.insert(*deal_id);
                telemetry.active_deal_high_water =
                    telemetry.active_deal_high_water.max(active.len());
            }
            ti4_model::DiplomacyEvent::Countered { .. } => telemetry.counters += 1,
            ti4_model::DiplomacyEvent::Accepted { .. } => telemetry.accepts += 1,
            ti4_model::DiplomacyEvent::Declined { deal_id, .. } => {
                telemetry.declines += 1;
                active.remove(deal_id);
            }
            ti4_model::DiplomacyEvent::DealSettled { deal_id, status } => {
                active.remove(deal_id);
                match status {
                    ti4_model::DealStatus::Fulfilled => telemetry.fulfilled += 1,
                    ti4_model::DealStatus::Broken => telemetry.broken += 1,
                    ti4_model::DealStatus::Expired => telemetry.expired += 1,
                    _ => {}
                }
            }
            ti4_model::DiplomacyEvent::SignalEmitted { .. } => telemetry.signals += 1,
            ti4_model::DiplomacyEvent::ImmediateApplied { .. }
            | ti4_model::DiplomacyEvent::PromiseSettled { .. }
            | ti4_model::DiplomacyEvent::SignalJudged { .. } => {}
        }
    }
    telemetry.final_relationships = state
        .diplomacy
        .relationships
        .values()
        .flat_map(|row| row.values().copied())
        .collect();
    telemetry
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    schema: String,
    observation_schema: String,
    #[serde(default)]
    diplomacy_enabled: bool,
    created_utc: String,
    engine_git_commit: String,
    engine_worktree_dirty: bool,
    /// Commit and binary that actually generated the per-game parts. For a normal completed run
    /// these equal the publishing process; recovery supplies the captured generation provenance.
    generation_executable_sha256: Option<String>,
    publisher_git_commit: String,
    /// Games present in the shards (retained ones); `games_played` is everything that was played.
    games: usize,
    rounds: u32,
    /// The worker count of the run that produced this corpus; null when publishing a killed
    /// run's staging where it is no longer known.
    workers: Option<usize>,
    retention_rule: String,
    games_played: usize,
    /// Original requested count when a killed run published fewer games than were planned.
    games_planned: Option<usize>,
    games_retained: usize,
    retention_breakdown: BTreeMap<String, usize>,
    seed_base: u64,
    factions: Vec<String>,
    policy_families: Vec<String>,
    map_pool: String,
    map_pool_sha256: String,
    vocabulary_slots_sha256: String,
    behavior_probabilities_recorded: bool,
    forced_decisions_retained: bool,
    storage_encoding: String,
    /// "mixed" (default 11-kind cycle) or "single" (`--single` checkpoint at its temperatures).
    policy_mode: String,
    checkpoint_manifests: BTreeMap<String, String>,
    records: BTreeMap<String, usize>,
    /// Shard sha256 keyed by path relative to the corpus root (e.g. `good/decisions.jsonl.zst`).
    shards: BTreeMap<String, String>,
    /// Per-bucket game and decision counts for every published folder.
    buckets: BTreeMap<String, BucketStats>,
}

struct RecordingDecider {
    inner: Box<dyn Decider>,
    game_id: String,
    seat: String,
    faction: String,
    policy_id: String,
    policy_rng_seed: u64,
    baseline: Baseline,
    vocabulary: Vocabulary,
    records: Rc<RefCell<Vec<CapturedDecision>>>,
}

impl RecordingDecider {
    fn features(&self, vector: &ti4_policy::features::FeatureVector) -> Vec<SparseFeature> {
        vector
            .iter()
            .map(|(key, value)| SparseFeature {
                name: ti4_policy::intern::name_of(*key),
                column: self.vocabulary.column_of_key(*key),
                value: *value,
            })
            .collect()
    }

    fn observation(&self, seen: &SeatObservation<'_>) -> SeatAuthorizedObservation {
        let player = seen.bound_seat();
        let public_seats = seen
            .players()
            .into_iter()
            .filter_map(|id| {
                let seat = seen.seat(id)?;
                Some(PublicSeatSnapshot {
                    seat: id.to_string(),
                    faction: seat.faction.to_string(),
                    victory_points: seat.victory_points,
                    trade_goods: seat.trade_goods,
                    commodities: seat.commodities,
                    tactic_tokens: seat.tactic_tokens,
                    fleet_tokens: seat.fleet_tokens,
                    strategic_tokens: seat.strategic_tokens,
                    strategy_cards: seat
                        .strategy_cards
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    exhausted_strategy_cards: seat
                        .exhausted_strategy_cards
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    technologies: seat.technologies.iter().map(ToString::to_string).collect(),
                    exhausted_technologies: seat
                        .exhausted_technologies
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    action_cards_held: seat.action_cards_held,
                    secret_objectives_held: seat.secret_objectives_held,
                    passed: seat.passed,
                    scored_objectives: seen.scored_by(id).iter().map(ToString::to_string).collect(),
                })
            })
            .collect();
        SeatAuthorizedObservation {
            round: seen.round(),
            phase: format!("{:?}", seen.phase()),
            active_player: seen.active_player().map(ToString::to_string),
            active_system: seen.active_system().map(ToString::to_string),
            pending_step: seen.pending_step().map(str::to_owned),
            speaker: seen.speaker().to_string(),
            initiative_order: seen
                .initiative_order()
                .iter()
                .map(ToString::to_string)
                .collect(),
            revealed_public_objectives: seen
                .revealed_objectives()
                .iter()
                .map(ToString::to_string)
                .collect(),
            revealed_public_objective_progress: seen
                .revealed_objective_progress(player)
                .into_iter()
                .map(Into::into)
                .collect(),
            public_seats,
            public_board: serde_json::to_value(seen.board()).unwrap_or(serde_json::Value::Null),
            laws: seen.laws().clone(),
            faceup_promissory_notes: seen
                .promissory_notes()
                .into_iter()
                .map(|(note, holder)| (note, holder.to_string()))
                .collect(),
            held_action_cards: seen
                .held_action_cards()
                .iter()
                .map(ToString::to_string)
                .collect(),
            held_secret_objectives: seen
                .held_secrets()
                .iter()
                .map(ToString::to_string)
                .collect(),
            held_secret_progress: seen
                .held_secret_progress()
                .into_iter()
                .map(Into::into)
                .collect(),
            held_promissory_notes: seen.held_promissory_notes(),
            diplomacy_relationships: diplomacy_relationships(seen),
            active_diplomacy_deals: public_diplomacy_deals(seen),
            recent_diplomacy_signals: public_diplomacy_signals(seen),
        }
    }
}

fn diplomacy_relationships(seen: &SeatObservation<'_>) -> Vec<DiplomacyRelationshipObservation> {
    let players = seen.players();
    let mut out = Vec::new();
    for observer in &players {
        for subject in &players {
            if observer == subject {
                continue;
            }
            out.push(DiplomacyRelationshipObservation {
                observer: observer.to_string(),
                subject: subject.to_string(),
                relationship: seen.diplomacy_relationship(observer, subject),
                recent_attack: seen.recent_diplomacy_attack(observer, subject),
                recent_breach: seen.recent_diplomacy_breach(observer, subject),
            });
        }
    }
    out
}

fn public_diplomacy_deals(seen: &SeatObservation<'_>) -> Vec<ti4_model::Deal> {
    let mut deals = BTreeMap::new();
    for player in seen.players() {
        for deal in seen.active_diplomacy_deals(player) {
            deals.entry(deal.id).or_insert_with(|| deal.clone());
        }
    }
    deals.into_values().collect()
}

fn public_diplomacy_signals(seen: &SeatObservation<'_>) -> Vec<ti4_model::Signal> {
    let mut signals = BTreeMap::new();
    for player in seen.players() {
        for signal in seen.recent_diplomacy_signals(player) {
            signals.entry(signal.id).or_insert_with(|| signal.clone());
        }
    }
    signals.into_values().collect()
}

impl Decider for RecordingDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let held = seen.held_secret_progress();
        let vectors = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            &choice.player,
            &held,
            self.baseline,
        );
        let critic =
            ti4_policy::critic::critic_vector(seen, ti4_policy::critic::CriticFeatures::full());
        let observation = self.observation(seen);
        let progress = ti4_policy::progress::measure(seen, &choice.player, self.baseline);
        let answer = self.inner.choose_seeing(choice, seen)?;
        let chosen_action_index = choice
            .options
            .iter()
            .position(|option| option.id == answer.id)
            .ok_or_else(|| IllegalChoice::NotOffered {
                player: choice.player.clone(),
                chosen: answer.id.clone(),
                offered: choice.ids().into_iter().map(str::to_owned).collect(),
            })?;
        let legal_actions = choice
            .options
            .iter()
            .zip(vectors.iter())
            .enumerate()
            .map(|(index, (option, vector))| LegalAction {
                index,
                id: option.id.clone(),
                kind: option.kind.clone(),
                label: option.label.clone(),
                payload: option.payload.clone(),
                actor_features: self.features(vector),
            })
            .collect();
        let mut records = self.records.borrow_mut();
        let seat_decision_index = records.len();
        records.push(CapturedDecision {
            game_id: self.game_id.clone(),
            seat: self.seat.clone(),
            faction: self.faction.clone(),
            policy_id: self.policy_id.clone(),
            policy_rng_seed: self.policy_rng_seed,
            seat_decision_index,
            head: ti4_mlp::capture_head(ti4_policy::learned::decision_head(choice)).to_owned(),
            prompt: choice.prompt.clone(),
            context: choice.context.clone(),
            observation,
            progress,
            critic_features: self.features(critic.facts()),
            legal_actions,
            chosen_action_index,
            chosen_action_id: answer.id.clone(),
        });
        Ok(answer)
    }
}

#[derive(Debug, Clone, Copy)]
enum Bias {
    Aggressive,
    Defensive,
    Economy,
    Objective,
}

impl Bias {
    const fn name(self) -> &'static str {
        match self {
            Self::Aggressive => "aggressive",
            Self::Defensive => "defensive",
            Self::Economy => "economy_first",
            Self::Objective => "objective_first",
        }
    }

    fn matches(self, choice: &Choice, option: &ChoiceOption) -> bool {
        let kind = option.kind.as_str();
        let id = option.id.to_ascii_lowercase();
        let prompt = choice.prompt.to_ascii_lowercase();
        match self {
            Self::Aggressive => {
                matches!(
                    kind,
                    "activate" | "move" | "commit" | "bombard" | "space_cannon" | "action"
                ) || id.contains("warfare")
            }
            Self::Defensive => matches!(
                kind,
                "produce" | "place" | "sustain" | "retreat" | "retreat_to" | "repair"
            ),
            Self::Economy => {
                matches!(
                    kind,
                    "trade" | "offer" | "transaction" | "produce" | "refresh" | "spend"
                ) || id.contains("trade")
                    || id.contains("econom")
            }
            Self::Objective => {
                matches!(kind, "score" | "research" | "commit")
                    || id.contains("imperial")
                    || id.contains("objective")
                    || prompt.contains("score")
            }
        }
    }
}

struct BiasedBot {
    bias: Bias,
    strength: f64,
    rng: ChaCha8Rng,
    fallback: ti4_policy::bot::ScoredBot,
}

impl BiasedBot {
    fn new(bias: Bias, seed: u64) -> Self {
        Self {
            bias,
            strength: 0.8,
            rng: ChaCha8Rng::seed_from_u64(seed ^ 0xB1A5_ED00),
            fallback: ti4_policy::bot::ScoredBot::new(seed),
        }
    }

    fn narrowed(&mut self, choice: &Choice) -> Option<Choice> {
        let preferred: Vec<ChoiceOption> = choice
            .options
            .iter()
            .filter(|option| self.bias.matches(choice, option))
            .cloned()
            .collect();
        if preferred.is_empty() || !self.rng.random_bool(self.strength) {
            return None;
        }
        Some(Choice {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            options: preferred,
            context: choice.context.clone(),
        })
    }
}

impl Decider for BiasedBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if let Some(narrowed) = self.narrowed(choice) {
            self.fallback.choose(&narrowed)
        } else {
            self.fallback.choose(choice)
        }
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if let Some(narrowed) = self.narrowed(choice) {
            self.fallback.choose_seeing(&narrowed, seen)
        } else {
            self.fallback.choose_seeing(choice, seen)
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PolicyKind {
    CurrentGreedy,
    CurrentStandard,
    CurrentHot,
    OlderGreedy,
    OlderHot,
    Heuristic,
    Evolutionary,
    Biased(Bias),
}

const POLICY_CYCLE: [PolicyKind; 11] = [
    PolicyKind::CurrentGreedy,
    PolicyKind::CurrentStandard,
    PolicyKind::CurrentHot,
    PolicyKind::OlderGreedy,
    PolicyKind::OlderHot,
    PolicyKind::Heuristic,
    PolicyKind::Evolutionary,
    PolicyKind::Biased(Bias::Aggressive),
    PolicyKind::Biased(Bias::Defensive),
    PolicyKind::Biased(Bias::Economy),
    PolicyKind::Biased(Bias::Objective),
];

/// Read-only data every worker may share across threads. The evolutionary profiles are immutable
/// after loading (the production rollout workers already share them the same way), and the paths
/// and digests only feed the manifest.
#[derive(Clone)]
struct SharedAssets {
    evolutionary: BTreeMap<String, Arc<ti4_policy::learned::Profile>>,
    current_path: String,
    current_manifest_sha: String,
    older_path: String,
    older_manifest_sha: String,
    evolutionary_path: String,
    evolutionary_sha: String,
    /// Single-checkpoint mode (`--single`): every seat uses the current checkpoint at one of
    /// these temperatures instead of the mixed 11-kind cycle. `None` in default mixed mode.
    single_temps: Option<Vec<f64>>,
}

/// What one worker chunk owns: deep copies of both actors (made on the main thread and moved in)
/// plus cloned vocabularies. `tch::Tensor` is `Send` but not `Sync`, so an actor never crosses a
/// thread boundary by shared reference; every game that chunk plays shares its private copies
/// through `Rc`. Inference never mutates the weights, so the copies stay exact for the whole run.
/// The number of chunks is bounded by the worker count, so tensor memory stays O(workers) rather
/// than O(games).
struct LocalAssets {
    current_actor: Rc<ti4_mlp::Actor>,
    current_vocabulary: Vocabulary,
    older_actor: Rc<ti4_mlp::Actor>,
    older_vocabulary: Vocabulary,
}

/// Owned deep copies of both actors and their vocabularies. Moved by value into one worker thread,
/// where it is wrapped in `Rc` — an actor crosses a thread boundary only as an owned value, never
/// as a shared reference (`tch::Tensor` is `Send` but not `Sync`).
struct WorkerActors {
    current_actor: ti4_mlp::Actor,
    current_vocabulary: Vocabulary,
    older_actor: ti4_mlp::Actor,
    older_vocabulary: Vocabulary,
}

/// The master copies on the main thread, used only as the source for the per-chunk inference
/// copies and never played with directly.
struct MasterAssets {
    current_actor: ti4_mlp::Actor,
    current_vocabulary: Vocabulary,
    older_actor: ti4_mlp::Actor,
    older_vocabulary: Vocabulary,
    shared: SharedAssets,
}

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

/// Parse the comma-separated temperature list for `--single`. Every value must be finite and > 0:
/// the MLP decider refuses anything else, and an empty list would make seat assignment divide by
/// zero.
fn parse_temperatures(raw: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let value: f64 = part
            .trim()
            .parse()
            .map_err(|_| format!("bad temperature {part:?}"))?;
        if !value.is_finite() || value <= 0.0 {
            return Err(format!("temperatures must be finite and > 0, got {value}"));
        }
        out.push(value);
    }
    if out.is_empty() {
        return Err("--temperatures must name at least one temperature".to_owned());
    }
    Ok(out)
}

fn next_policy_offset(offset: usize, seats: usize, cycle_len: usize, single_mode: bool) -> usize {
    let stride = if single_mode { 1 } else { seats };
    (offset + stride) % cycle_len
}

fn refuse(reason: impl std::fmt::Display) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

/// Streaming sha256: a published shard can be tens of gigabytes, so the file is hashed in 1 MiB
/// chunks rather than read whole into memory.
fn file_sha(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("opening {}: {error}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("reading {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn git(command: &str) -> String {
    std::process::Command::new("git")
        .args(command.split_whitespace())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn load_assets(
    current: String,
    older: String,
    evolutionary: String,
) -> Result<MasterAssets, String> {
    let current_manifest_sha = file_sha(&Path::new(&current).join("manifest.json"))?;
    let older_manifest_sha = file_sha(&Path::new(&older).join("manifest.json"))?;
    let current_loaded = ti4_mlp::bundle::read(Path::new(&current))
        .map_err(|error| format!("reading current bundle: {error}"))?;
    let older_loaded = ti4_mlp::bundle::read(Path::new(&older))
        .map_err(|error| format!("reading older bundle: {error}"))?;
    let current_slots = file_sha(&Path::new(&current).join("slots.json"))?;
    let older_slots = file_sha(&Path::new(&older).join("slots.json"))?;
    if current_slots != older_slots {
        return Err("pilot checkpoints use different vocabularies".to_owned());
    }
    let stored_evolutionary =
        std::fs::read(&evolutionary).map_err(|error| format!("reading {evolutionary}: {error}"))?;
    let evolutionary_bytes = if Path::new(&evolutionary)
        .extension()
        .is_some_and(|extension| extension == "zst")
    {
        zstd::stream::decode_all(std::io::Cursor::new(stored_evolutionary))
            .map_err(|error| format!("decompressing {evolutionary}: {error}"))?
    } else {
        stored_evolutionary
    };
    let evolutionary_sha = sha256(&evolutionary_bytes);
    let evolutionary_profiles =
        ti4_training::vocabulary_corpus::champion_profiles(&evolutionary_bytes, &evolutionary)
            .map_err(|error| format!("reading evolutionary profiles: {error}"))?
            .into_iter()
            .map(|(faction, profile)| (faction, Arc::new(profile)))
            .collect();
    Ok(MasterAssets {
        current_actor: current_loaded.actor,
        current_vocabulary: current_loaded.vocabulary,
        older_actor: older_loaded.actor,
        older_vocabulary: older_loaded.vocabulary,
        shared: SharedAssets {
            evolutionary: evolutionary_profiles,
            current_path: current,
            current_manifest_sha,
            older_path: older,
            older_manifest_sha,
            evolutionary_path: evolutionary,
            evolutionary_sha,
            single_temps: None,
        },
    })
}

/// Single-checkpoint loading for `--single`: one bundle, no cross-vocabulary check (there is
/// nothing to compare against). The checkpoint lands in the "current" slot; the unused older and
/// evolutionary slots are filled with inert copies so the per-chunk deep-copy machinery stays
/// uniform between modes.
fn load_single(path: &str, temps: Vec<f64>) -> Result<MasterAssets, String> {
    let manifest_sha = file_sha(&Path::new(path).join("manifest.json"))?;
    let loaded = ti4_mlp::bundle::read(Path::new(path))
        .map_err(|error| format!("reading single bundle: {error}"))?;
    // The copy is made before the move below: `inference_copy` borrows, and the master actor
    // itself becomes the current slot.
    let older = loaded.actor.inference_copy();
    Ok(MasterAssets {
        current_actor: loaded.actor,
        current_vocabulary: loaded.vocabulary.clone(),
        older_actor: older,
        older_vocabulary: loaded.vocabulary,
        shared: SharedAssets {
            evolutionary: BTreeMap::new(),
            current_path: path.to_owned(),
            current_manifest_sha: manifest_sha,
            older_path: String::new(),
            older_manifest_sha: String::new(),
            evolutionary_path: String::new(),
            evolutionary_sha: String::new(),
            single_temps: Some(temps),
        },
    })
}

/// One seat in single-checkpoint mode: the `--single` checkpoint at one of its temperatures.
fn policy_single(
    local: &LocalAssets,
    shared: &SharedAssets,
    faction: &str,
    baseline: Baseline,
    seed: u64,
    temperature: f64,
) -> Result<(Box<dyn Decider>, PolicyMetadata), String> {
    let row = ti4_mlp::FactionRow::of(faction).map_err(|error| error.to_string())?;
    let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
        &local.current_actor,
        local.current_vocabulary.clone(),
        row,
        seed,
    )
    .at_temperature(temperature)
    .from_setup(baseline)
    .seat();
    Ok((
        decider,
        PolicyMetadata {
            policy_id: format!("single_mlp_t{:03}", (temperature * 100.0).round() as u32),
            family: "mlp".to_owned(),
            checkpoint: Some(shared.current_path.clone()),
            checkpoint_manifest_sha256: Some(shared.current_manifest_sha.clone()),
            temperature: Some(temperature),
            bias: None,
            source_profile: None,
            rng_seed: seed,
        },
    ))
}

fn policy(
    kind: PolicyKind,
    local: &LocalAssets,
    shared: &SharedAssets,
    faction: &str,
    baseline: Baseline,
    seed: u64,
    fallback_profile_index: usize,
) -> Result<(Box<dyn Decider>, PolicyMetadata), String> {
    let mlp = |actor: &Rc<ti4_mlp::Actor>,
               vocabulary: Vocabulary,
               temperature: f64,
               id: &str,
               path: &str,
               digest: &str|
     -> Result<(Box<dyn Decider>, PolicyMetadata), String> {
        let row = ti4_mlp::FactionRow::of(faction).map_err(|error| error.to_string())?;
        let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(actor, vocabulary, row, seed)
            .at_temperature(temperature)
            .from_setup(baseline)
            .seat();
        Ok((
            decider,
            PolicyMetadata {
                policy_id: id.to_owned(),
                family: "mlp".to_owned(),
                checkpoint: Some(path.to_owned()),
                checkpoint_manifest_sha256: Some(digest.to_owned()),
                temperature: Some(temperature),
                bias: None,
                source_profile: None,
                rng_seed: seed,
            },
        ))
    };
    match kind {
        PolicyKind::CurrentGreedy => mlp(
            &local.current_actor,
            local.current_vocabulary.clone(),
            0.25,
            "current_mlp_t025",
            &shared.current_path,
            &shared.current_manifest_sha,
        ),
        PolicyKind::CurrentStandard => mlp(
            &local.current_actor,
            local.current_vocabulary.clone(),
            1.0,
            "current_mlp_t100",
            &shared.current_path,
            &shared.current_manifest_sha,
        ),
        PolicyKind::CurrentHot => mlp(
            &local.current_actor,
            local.current_vocabulary.clone(),
            2.5,
            "current_mlp_t250",
            &shared.current_path,
            &shared.current_manifest_sha,
        ),
        PolicyKind::OlderGreedy => mlp(
            &local.older_actor,
            local.older_vocabulary.clone(),
            0.25,
            "older_mlp_t025",
            &shared.older_path,
            &shared.older_manifest_sha,
        ),
        PolicyKind::OlderHot => mlp(
            &local.older_actor,
            local.older_vocabulary.clone(),
            2.5,
            "older_mlp_t250",
            &shared.older_path,
            &shared.older_manifest_sha,
        ),
        PolicyKind::Heuristic => Ok((
            Box::new(ti4_policy::bot::ScoredBot::new(seed)),
            PolicyMetadata {
                policy_id: "authored_heuristic".to_owned(),
                family: "heuristic".to_owned(),
                checkpoint: None,
                checkpoint_manifest_sha256: None,
                temperature: Some(ti4_policy::bot::TEMPERATURE),
                bias: None,
                source_profile: None,
                rng_seed: seed,
            },
        )),
        PolicyKind::Evolutionary => {
            let selected = shared
                .evolutionary
                .get(faction)
                .map(|profile| (faction.to_owned(), Arc::clone(profile)))
                .or_else(|| {
                    shared
                        .evolutionary
                        .iter()
                        .nth(fallback_profile_index % shared.evolutionary.len().max(1))
                        .map(|(name, profile)| (name.clone(), Arc::clone(profile)))
                })
                .ok_or_else(|| "evolutionary checkpoint contains no profiles".to_owned())?;
            Ok((
                Box::new(
                    ti4_policy::inference::LearnedBot::from_shared(selected.1, seed)
                        .from_setup(baseline),
                ),
                PolicyMetadata {
                    policy_id: "evolutionary_linear".to_owned(),
                    family: "evolutionary_linear".to_owned(),
                    checkpoint: Some(shared.evolutionary_path.clone()),
                    checkpoint_manifest_sha256: Some(shared.evolutionary_sha.clone()),
                    temperature: Some(1.0),
                    bias: None,
                    source_profile: Some(selected.0),
                    rng_seed: seed,
                },
            ))
        }
        PolicyKind::Biased(bias) => Ok((
            Box::new(BiasedBot::new(bias, seed)),
            PolicyMetadata {
                policy_id: format!("heuristic_{}", bias.name()),
                family: "strategically_biased_heuristic".to_owned(),
                checkpoint: None,
                checkpoint_manifest_sha256: None,
                temperature: Some(ti4_policy::bot::TEMPERATURE),
                bias: Some(bias.name().to_owned()),
                source_profile: None,
                rng_seed: seed,
            },
        )),
    }
}

struct JsonlZstdWriter {
    encoder: zstd::stream::write::Encoder<'static, BufWriter<std::fs::File>>,
    count: usize,
}

impl JsonlZstdWriter {
    fn create(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::create(path)
            .map_err(|error| format!("creating {}: {error}", path.display()))?;
        let encoder = zstd::stream::write::Encoder::new(BufWriter::new(file), 9)
            .map_err(|error| format!("opening zstd stream: {error}"))?;
        Ok(Self { encoder, count: 0 })
    }

    fn write<T: Serialize>(&mut self, value: &T) -> Result<(), String> {
        serde_json::to_writer(&mut self.encoder, value)
            .map_err(|error| format!("serializing record: {error}"))?;
        self.encoder
            .write_all(b"\n")
            .map_err(|error| format!("writing record: {error}"))?;
        self.count += 1;
        Ok(())
    }

    fn finish(self) -> Result<usize, String> {
        self.encoder
            .finish()
            .map_err(|error| format!("finishing zstd stream: {error}"))?
            .flush()
            .map_err(|error| format!("flushing zstd stream: {error}"))?;
        Ok(self.count)
    }
}

/// The loss-alignment predicate for one captured decision: the chosen index must be in range,
/// match the chosen action's id, and the legal actions must be numbered from zero — otherwise a
/// training loss computed on this record would silently train on the wrong move.
fn is_loss_aligned(
    legal_actions: &[LegalAction],
    chosen_action_index: usize,
    chosen_action_id: &str,
) -> bool {
    !legal_actions.is_empty()
        && chosen_action_index < legal_actions.len()
        && legal_actions[chosen_action_index].id == chosen_action_id
        && legal_actions
            .iter()
            .enumerate()
            .all(|(index, option)| option.index == index)
}

/// Gate applied to every record of a retained game *before* anything is written: the predicate is
/// a property of the data, not of serialization, so checking it in memory catches exactly what the
/// old end-of-run shard re-parse caught — and an unaligned record can never reach disk.
fn validate_decision(decision: &CapturedDecision) -> Result<(), String> {
    if is_loss_aligned(
        &decision.legal_actions,
        decision.chosen_action_index,
        &decision.chosen_action_id,
    ) {
        Ok(())
    } else {
        Err("not loss-aligned".to_owned())
    }
}

/// Why a game was written to disk (or that it failed and was retained for visibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetentionReason {
    Standout,
    StrongTable,
    WeakTable,
    RandomControl,
    FailedGame,
}

impl RetentionReason {
    fn as_str(self) -> &'static str {
        match self {
            RetentionReason::Standout => "standout",
            RetentionReason::StrongTable => "strong_table",
            RetentionReason::WeakTable => "weak_table",
            RetentionReason::RandomControl => "random_control",
            RetentionReason::FailedGame => "failed_game",
        }
    }
}

/// The published folder for one retention reason: high-VP games are `good`, low-VP slogs are
/// `bad`, the seeded 5% control is `random`, and engine failures stay visible in `failed` without
/// contaminating any training bucket.
fn bucket_for(reason: RetentionReason) -> &'static str {
    match reason {
        RetentionReason::Standout | RetentionReason::StrongTable => BUCKET_GOOD,
        RetentionReason::WeakTable => BUCKET_BAD,
        RetentionReason::RandomControl => BUCKET_RANDOM,
        RetentionReason::FailedGame => BUCKET_FAILED,
    }
}

/// The retention decision for one finished game (see `RETENTION_RULE`). Pure function of the end
/// state and the game seed, so it is identical at any worker count.
fn decide_retention(table_vp: i32, max_faction_vp: i32, game_seed: u64) -> Option<RetentionReason> {
    if max_faction_vp >= STANDOUT_VP {
        return Some(RetentionReason::Standout);
    }
    if table_vp >= STRONG_TABLE_VP {
        return Some(RetentionReason::StrongTable);
    }
    if table_vp < WEAK_TABLE_VP {
        return Some(RetentionReason::WeakTable);
    }
    let mut coin = ChaCha8Rng::seed_from_u64(game_seed ^ RETENTION_COIN_SALT);
    (coin.random_range(0..RANDOM_CONTROL_DENOMINATOR) < RANDOM_CONTROL_NUMERATOR)
        .then_some(RetentionReason::RandomControl)
}

/// Everything that determines one game before it is played. Precomputed on the main thread so the
/// faction deck and policy offset advance exactly as they did in the sequential pilot, whatever
/// order the workers finish their games in.
#[derive(Clone)]
struct GamePlan {
    game_index: usize,
    game_seed: u64,
    game_id: String,
    seated: BTreeMap<PlayerId, FactionId>,
    policy_offset: usize,
}

/// What one finished (or failed) game reports back. The records themselves never cross the thread
/// boundary: each worker writes its own per-game zstd frames and the main thread concatenates them
/// in game order.
/// Per-game completion status lives in the games shard; only manifest-level aggregates come back.
/// What one finished (or failed) game reports back. The records themselves never cross the thread
/// boundary: each worker writes its own per-game zstd frames and the main thread concatenates them
/// in game order. Discarded games report zero written decisions but keep their recorded count for
/// the retention sidecar log.
#[derive(Debug, Clone)]
struct GameOutcome {
    game_index: usize,
    game_seed: u64,
    /// Decisions written to the shard (0 for discarded games).
    decision_count: usize,
    /// Decisions this game actually produced, retained or not — provenance only.
    recorded_decisions: usize,
    policy_families: BTreeSet<String>,
    retained: bool,
    retention_reason: Option<RetentionReason>,
    table_vp: i32,
    max_faction_vp: i32,
    diplomacy_deal_count: usize,
}

/// One line of the published `retention.jsonl` sidecar: provenance for every played game, whether
/// it made it into the shards or not.
#[derive(Serialize)]
struct RetentionRecord {
    game_index: usize,
    game_seed: u64,
    table_vp: i32,
    max_faction_vp: i32,
    recorded_decisions: usize,
    retained: bool,
    reason: Option<String>,
}

/// Per-bucket counts published in the manifests: how many games and decisions each reason folder
/// holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BucketStats {
    games: usize,
    decisions: usize,
    #[serde(default)]
    diplomacy_deals: usize,
}

/// Read-only context shared by every worker thread. `ContentStore`, the map pool and the opening
/// map are already shared across rollout workers in production, so this adds no new aliasing.
struct PlayContext<'a> {
    content: &'a ContentStore,
    players: &'a [PlayerId],
    map: &'a ti4_training::rollout::OpeningMap,
    staging: &'a Path,
    rounds: u32,
    total_games: usize,
    diplomacy_enabled: bool,
}

fn decisions_part(game_index: usize) -> String {
    format!("decisions-{game_index:06}.jsonl.zst")
}

fn games_part(game_index: usize) -> String {
    format!("games-{game_index:06}.jsonl.zst")
}

fn diplomacy_part(game_index: usize) -> String {
    format!("diplomacy-{game_index:06}.jsonl.zst")
}

/// Join the per-game zstd frames into one shard, in game order. A zstd stream may legally contain
/// several concatenated frames (each part is exactly one), so plain concatenation stays a valid
/// stream that any decoder walks transparently; keeping each worker's output on disk means no record
/// crosses a thread boundary or accumulates in one heap. An empty bucket still gets a valid
/// zero-record zstd frame, so every published folder has the same shape and decodes cleanly.
/// Returns the sha256 of the exact bytes written, so the caller can prove the published shard is
/// byte-identical to the validated frames.
/// `keep_parts` leaves the per-game frames in place for a caller that must stay retryable (a
/// killed run's staging is the only copy of those games); otherwise each part is removed as it is
/// consumed.
fn concatenate_parts(
    parts: &[PathBuf],
    final_path: &Path,
    keep_parts: bool,
) -> Result<String, String> {
    let file = std::fs::File::create(final_path)
        .map_err(|error| format!("creating {}: {error}", final_path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut hasher = sha2::Sha256::new();
    if parts.is_empty() {
        let frame = zstd::stream::encode_all(&b""[..], 9)
            .map_err(|error| format!("encoding empty shard: {error}"))?;
        hasher.update(&frame);
        writer
            .write_all(&frame)
            .map_err(|error| format!("writing {}: {error}", final_path.display()))?;
    } else {
        for part in parts {
            let frame = std::fs::read(part)
                .map_err(|error| format!("reading {}: {error}", part.display()))?;
            hasher.update(&frame);
            writer
                .write_all(&frame)
                .map_err(|error| format!("writing {}: {error}", final_path.display()))?;
            if !keep_parts {
                std::fs::remove_file(part)
                    .map_err(|error| format!("removing {}: {error}", part.display()))?;
            }
        }
    }
    writer
        .flush()
        .map_err(|error| format!("flushing {}: {error}", final_path.display()))?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Play one planned game on this thread and write its two per-game frames. The body is the former
/// sequential loop iteration, unchanged in every decision-relevant way: same decider factory,
/// same step budget, same record order (seats in seat0..seat5 order), same metadata.
#[expect(
    clippy::too_many_lines,
    reason = "one game's capture is one linear pass: setup, run, records, frames"
)]
fn play_game(
    plan: &GamePlan,
    local: &LocalAssets,
    shared: &SharedAssets,
    ctx: &PlayContext<'_>,
) -> Result<GameOutcome, String> {
    let handles: Rc<RefCell<BTreeMap<PlayerId, Rc<RefCell<Vec<CapturedDecision>>>>>> =
        Rc::new(RefCell::new(BTreeMap::new()));
    let policies: Rc<RefCell<BTreeMap<PlayerId, PolicyMetadata>>> =
        Rc::new(RefCell::new(BTreeMap::new()));
    let baselines: Rc<RefCell<BTreeMap<PlayerId, Baseline>>> =
        Rc::new(RefCell::new(BTreeMap::new()));
    let handles_in = Rc::clone(&handles);
    let policies_in = Rc::clone(&policies);
    let baselines_in = Rc::clone(&baselines);

    let mut game = ti4_training::rollout::setup_game_with_capabilities_and_decider_factory(
        ctx.content,
        ctx.players,
        &plan.seated,
        DEFAULT,
        plan.game_seed,
        ctx.map,
        ti4_training::rollout::SimulationCapabilities {
            diplomacy: ctx.diplomacy_enabled,
        },
        |baselines| {
            let mut deciders = BTreeMap::new();
            for (seat_index, player) in ctx.players.iter().enumerate() {
                let faction = plan.seated[player].as_str();
                let policy_seed = plan
                    .game_seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(seat_index).unwrap_or(0));
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("missing baseline for {player}"))?;
                baselines_in.borrow_mut().insert(player.clone(), baseline);
                let (inner, metadata) = if let Some(temps) = &shared.single_temps {
                    // Single-checkpoint mode: the temperature cycle replaces the 11-kind policy
                    // cycle; the offset arithmetic is otherwise identical to mixed mode.
                    let index = (plan.policy_offset + seat_index) % temps.len();
                    policy_single(local, shared, faction, baseline, policy_seed, temps[index])?
                } else {
                    let policy_kind =
                        POLICY_CYCLE[(plan.policy_offset + seat_index) % POLICY_CYCLE.len()];
                    policy(
                        policy_kind,
                        local,
                        shared,
                        faction,
                        baseline,
                        policy_seed,
                        plan.game_index + seat_index,
                    )?
                };
                let records = Rc::new(RefCell::new(Vec::new()));
                handles_in
                    .borrow_mut()
                    .insert(player.clone(), Rc::clone(&records));
                policies_in
                    .borrow_mut()
                    .insert(player.clone(), metadata.clone());
                deciders.insert(
                    player.clone(),
                    Box::new(RecordingDecider {
                        inner,
                        game_id: plan.game_id.clone(),
                        seat: player.to_string(),
                        faction: faction.to_owned(),
                        policy_id: metadata.policy_id,
                        policy_rng_seed: policy_seed,
                        baseline,
                        vocabulary: local.current_vocabulary.clone(),
                        records,
                    }) as Box<dyn Decider>,
                );
            }
            Ok(deciders)
        },
    )?;
    let run_error = game
        .run(ctx.rounds, 125_000usize.saturating_mul(ctx.rounds as usize))
        .err();
    let completed = run_error.is_none();
    let error = run_error.map(|value| value.to_string());
    let diplomacy_records = ti4_engine::diplomacy::export_log_records(&game.state, &plan.game_id)
        .map_err(|error| format!("exporting diplomacy log: {error}"))?;
    let diplomacy_signal_count = game
        .state
        .diplomacy
        .journal
        .iter()
        .filter(|entry| matches!(entry.event, ti4_model::DiplomacyEvent::SignalEmitted { .. }))
        .count();
    let map_placements = game
        .galaxy()
        .map(|galaxy| {
            galaxy
                .system_ids()
                .into_iter()
                .filter_map(|system| {
                    galaxy.coord_of(system).map(|hex| MapPlacement {
                        system: system.to_owned(),
                        q: hex.q,
                        r: hex.r,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let seen = ti4_engine::choice::Observed::new(&game.state, ctx.content, DEFAULT, game.galaxy());

    let mut policy_families = BTreeSet::new();
    let mut seat_metadata = Vec::new();
    for player in ctx.players {
        let policy = policies.borrow()[player].clone();
        policy_families.insert(policy.family.clone());
        let baseline = baselines.borrow().get(player).copied().unwrap_or_default();
        let final_progress = ti4_policy::progress::measure(&seen, player, baseline);
        seat_metadata.push(SeatMetadata {
            seat: player.to_string(),
            faction: plan.seated[player].to_string(),
            policy,
            final_progress,
        });
    }

    // ---- end-state retention, decided while the records are still in memory -------------------
    let mut table_vp = 0i32;
    let mut max_faction_vp = 0i32;
    for player in ctx.players {
        if let Some(seat) = game.state.player(player) {
            table_vp += seat.victory_points;
            max_faction_vp = max_faction_vp.max(seat.victory_points);
        }
    }
    let recorded_decisions: usize = handles
        .borrow()
        .values()
        .map(|records| records.borrow().len())
        .sum();
    let mut telemetry_decisions = Vec::with_capacity(recorded_decisions);
    for records in handles.borrow().values() {
        telemetry_decisions.extend(records.borrow().iter().cloned());
    }
    let diplomacy_telemetry = diplomacy_telemetry(&game.state, telemetry_decisions.iter());
    if diplomacy_telemetry.max_initial_candidates > 24
        || diplomacy_telemetry.max_counter_candidates > 8
    {
        return Err(format!(
            "diplomacy candidate cap exceeded: initial {}, counter {}",
            diplomacy_telemetry.max_initial_candidates, diplomacy_telemetry.max_counter_candidates
        ));
    }

    // Failed games are always retained so engine failures stay visible in the corpus; finished
    // games go through the retention rule (see `RETENTION_RULE`). A discarded game writes no
    // frames at all: its records die here, on this thread.
    let retention_reason = if completed {
        decide_retention(table_vp, max_faction_vp, plan.game_seed)
    } else {
        Some(RetentionReason::FailedGame)
    };
    let Some(reason) = retention_reason else {
        println!(
            "  game {}/{total} {game_id}: {recorded_decisions} decisions (discarded: table {table_vp}, max faction {max_faction_vp})",
            plan.game_index + 1,
            total = ctx.total_games,
            game_id = plan.game_id,
        );
        return Ok(GameOutcome {
            game_index: plan.game_index,
            game_seed: plan.game_seed,
            decision_count: 0,
            recorded_decisions,
            policy_families,
            retained: false,
            retention_reason: None,
            table_vp,
            max_faction_vp,
            diplomacy_deal_count: 0,
        });
    };

    // Loss-alignment gate on every record that is about to be written (see `validate_decision`).
    let mut line = 0usize;
    for player in ctx.players {
        for decision in handles.borrow()[player].borrow().iter() {
            validate_decision(decision).map_err(|_| {
                format!(
                    "game {} decision {}: not loss-aligned",
                    plan.game_index, line
                )
            })?;
            line += 1;
        }
    }

    // Retained games land in their reason's bucket folder (see `bucket_for`). The three training
    // buckets exist since staging setup; a failure creates its own lazily.
    let bucket_dir = ctx.staging.join(bucket_for(reason));
    std::fs::create_dir_all(&bucket_dir)
        .map_err(|error| format!("creating {}: {error}", bucket_dir.display()))?;

    let mut decisions_writer =
        JsonlZstdWriter::create(&bucket_dir.join(decisions_part(plan.game_index)))?;
    for player in ctx.players {
        for decision in handles.borrow()[player].borrow().iter() {
            decisions_writer.write(decision)?;
        }
    }
    let decision_count = decisions_writer.finish()?;

    let mut diplomacy_writer =
        JsonlZstdWriter::create(&bucket_dir.join(diplomacy_part(plan.game_index)))?;
    for record in &diplomacy_records {
        diplomacy_writer.write(record)?;
    }
    let diplomacy_deal_count = diplomacy_writer.finish()?;

    let mut games_writer = JsonlZstdWriter::create(&bucket_dir.join(games_part(plan.game_index)))?;
    games_writer.write(&GameMetadata {
        game_id: plan.game_id.clone(),
        game_index: plan.game_index,
        game_seed: plan.game_seed,
        tile_seed_offset: TILE_SEED_OFFSET,
        rounds_requested: ctx.rounds,
        completed,
        error,
        map_placements,
        seats: seat_metadata,
        decision_count,
        diplomacy_enabled: ctx.diplomacy_enabled,
        diplomacy_deal_count,
        diplomacy_signal_count,
        diplomacy_telemetry,
    })?;
    games_writer.finish()?;

    println!(
        "  game {}/{total} {game_id}: {decision_count} decisions (retained: {reason})",
        plan.game_index + 1,
        total = ctx.total_games,
        game_id = plan.game_id,
        reason = reason.as_str(),
    );
    Ok(GameOutcome {
        game_index: plan.game_index,
        game_seed: plan.game_seed,
        decision_count,
        recorded_decisions,
        policy_families,
        retained: true,
        retention_reason: Some(reason),
        table_vp,
        max_faction_vp,
        diplomacy_deal_count,
    })
}

fn main() {
    if let Err(error) = run() {
        refuse(error);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the pilot's bounded orchestration is linear"
)]
fn run() -> Result<(), String> {
    // `--publish-staging` publishes a killed run's staging directory instead of playing games. It
    // needs no tensor backend, map pool parsing or policy loading: everything comes from disk.
    if let Some(staging) = argument("--publish-staging") {
        let Some(checkpoint) = argument("--checkpoint") else {
            return Err("--publish-staging requires --checkpoint".to_owned());
        };
        let Some(games_arg) = argument("--games-played") else {
            return Err("--publish-staging requires --games-played".to_owned());
        };
        let games_played = games_arg
            .parse::<usize>()
            .map_err(|_| "--games-played must be a positive integer".to_owned())?;
        if games_played == 0 {
            return Err("--games-played must be non-zero".to_owned());
        }
        let games_planned = argument("--games-planned")
            .map(|value| value.parse::<usize>())
            .transpose()
            .map_err(|_| "--games-planned must be a positive integer".to_owned())?;
        if games_planned == Some(0) || games_planned.is_some_and(|planned| planned < games_played) {
            return Err("--games-planned must be non-zero and at least --games-played".to_owned());
        }
        let generation_commit = argument("--generation-git-commit")
            .ok_or_else(|| "--publish-staging requires --generation-git-commit".to_owned())?;
        let generator_sha = argument("--generator-sha256")
            .ok_or_else(|| "--publish-staging requires --generator-sha256".to_owned())?;
        if generator_sha.len() != 64 || !generator_sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("--generator-sha256 must be a 64-digit SHA-256".to_owned());
        }
        let generation_dirty = argument("--generation-worktree-dirty")
            .unwrap_or_else(|| "false".to_owned())
            .parse::<bool>()
            .map_err(|_| "--generation-worktree-dirty must be true or false".to_owned())?;
        let policy_mode = argument("--policy-mode").ok_or_else(|| {
            "--publish-staging requires --policy-mode (single or mixed)".to_owned()
        })?;
        if policy_mode != "single" && policy_mode != "mixed" {
            return Err("--policy-mode must be single or mixed".to_owned());
        }
        let workers = match argument("--workers") {
            None => None,
            Some(value) => Some(
                value
                    .parse::<usize>()
                    .map_err(|_| "--workers must be a positive integer".to_owned())?,
            ),
        };
        if workers == Some(0) {
            return Err("--workers must be non-zero".to_owned());
        }
        let map_pool =
            argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
        let map_pool_sha = argument("--map-pool-sha256");
        return publish_staging_run(
            &staging,
            argument("--out").as_deref(),
            &checkpoint,
            games_played,
            games_planned,
            workers,
            map_pool,
            map_pool_sha,
            &policy_mode,
            generation_commit,
            generation_dirty,
            generator_sha.to_ascii_lowercase(),
        );
    }

    let output =
        PathBuf::from(argument("--out").unwrap_or_else(|| "E:/ti4-corpus/pilot-v1".to_owned()));
    let games = argument("--games")
        .map_or(Ok(DEFAULT_GAMES), |value| value.parse::<usize>())
        .map_err(|_| "--games must be a positive integer".to_owned())?;
    let rounds = argument("--rounds")
        .map_or(Ok(DEFAULT_ROUNDS), |value| value.parse::<u32>())
        .map_err(|_| "--rounds must be a positive integer".to_owned())?;
    let diplomacy_enabled = argument("--diplomacy")
        .map_or(Ok(false), |value| value.parse::<bool>())
        .map_err(|_| "--diplomacy must be true or false".to_owned())?;
    let seed_base = argument("--seed-base")
        .map_or(Ok(DEFAULT_SEED_BASE), |value| value.parse::<u64>())
        .map_err(|_| "--seed-base must be an integer".to_owned())?;
    if games == 0 || rounds == 0 {
        return Err("--games and --rounds must be non-zero".to_owned());
    }
    if output.exists() {
        return Err(format!(
            "{} already exists; corpora are immutable",
            output.display()
        ));
    }
    let single = argument("--single");
    if single.is_some()
        && (argument("--current").is_some()
            || argument("--older").is_some()
            || argument("--evolutionary").is_some())
    {
        return Err("--single cannot be combined with --current/--older/--evolutionary".to_owned());
    }
    let current = argument("--current").unwrap_or_else(|| {
        "out/blank-shaped-4layers/shaped-r1bonus3-20260912/checkpoint-318956".to_owned()
    });
    let older = argument("--older").unwrap_or_else(|| {
        "out/blank-shaped-4layers/continue-20260912/checkpoint-236556".to_owned()
    });
    let evolutionary = argument("--evolutionary")
        .unwrap_or_else(|| "fixtures/mlp-baselines/final10000.zst".to_owned());
    // Phase timing: setup (backend, assets, plans, actor copies) / games (the parallel part) /
    // finish (frame assembly + validation read-back). Wall-clock diagnostics only; nothing here
    // feeds a decision or a shard byte.
    let setup_started = std::time::Instant::now();
    let tensor_seed = i64::try_from(seed_base)
        .map_err(|_| "--seed-base must fit a signed 64-bit tensor seed".to_owned())?;
    ti4_tensor::configure_deterministic(tensor_seed)
        .map_err(|error| format!("configuring tensor backend: {error}"))?;
    let master = match &single {
        Some(path) => {
            let temps = parse_temperatures(
                argument("--temperatures")
                    .as_deref()
                    .unwrap_or("0.25,1.0,2.5"),
            )?;
            load_single(path, temps)?
        }
        None => load_assets(current, older, evolutionary)?,
    };
    if diplomacy_enabled
        && (master.current_actor.head_layout() != ti4_mlp::HeadLayout::Diplomacy
            || master.older_actor.head_layout() != ti4_mlp::HeadLayout::Diplomacy)
    {
        return Err(
            "--diplomacy=true requires schema-9/10 MLP checkpoints with the diplomacy head"
                .to_owned(),
        );
    }

    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .map_err(|error| format!("map pool {pool_path}: {error}"))?;
    let pool_sha = sha256(&pool_bytes);
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .map_err(|error| format!("parsing map pool: {error}"))?,
    );
    let slots_sha = file_sha(&Path::new(&master.shared.current_path).join("slots.json"))?;
    let staging = output.with_extension(format!("staging-{}", std::process::id()));
    if staging.exists() {
        return Err(format!(
            "staging directory {} already exists",
            staging.display()
        ));
    }
    std::fs::create_dir_all(&staging)
        .map_err(|error| format!("creating {}: {error}", staging.display()))?;
    // The three training bucket folders always exist so every published corpus has the same shape;
    // `failed` is created lazily by a worker only when a game actually fails.
    for bucket in [BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM] {
        std::fs::create_dir_all(staging.join(bucket))
            .map_err(|error| format!("creating {}: {error}", staging.join(bucket).display()))?;
    }

    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let map = ti4_training::rollout::OpeningMap::PythonPool {
        pool: Arc::clone(&pool),
        tile_seed_offset: TILE_SEED_OFFSET,
    };

    // ---- plan every game before any of them is played ------------------------------------------
    // Plan on the main thread so every worker receives deterministic inputs independent of finish
    // order. Faction seating uses the same game seed and shared permutation contract as training.
    let mut plans: Vec<GamePlan> = Vec::with_capacity(games);
    let mut policy_offset = 0usize;
    let faction_roster = IN_SCOPE_FACTIONS.map(FactionId::new);
    for game_index in 0..games {
        let game_seed = seed_base.wrapping_add(u64::try_from(game_index).unwrap_or(0));
        let seated: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .enumerate()
            .map(|(seat, player)| {
                (
                    player.clone(),
                    ti4_training::rollout::seated_faction(&faction_roster, game_seed, 0, seat),
                )
            })
            .collect();
        plans.push(GamePlan {
            game_index,
            game_seed,
            game_id: format!("pilot-{game_seed:010}-{game_index:04}"),
            seated,
            policy_offset,
        });
        // Single-checkpoint mode cycles over its temperature list instead of the 11 kinds.
        let cycle_len = master
            .shared
            .single_temps
            .as_ref()
            .map_or(POLICY_CYCLE.len(), Vec::len);
        // Advancing six seats through three temperatures by six never moves at all. Rotate single
        // mode by one game so every seat sees every temperature; retain the coprime 6/11 stride
        // used by the heterogeneous policy cycle.
        policy_offset = next_policy_offset(
            policy_offset,
            players.len(),
            cycle_len,
            master.shared.single_temps.is_some(),
        );
    }

    // ---- workers -------------------------------------------------------------------------------
    let default_workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
        .max(1);
    let workers_requested = match argument("--workers") {
        Some(text) => Some(
            text.parse::<usize>()
                .map_err(|_| "--workers must be a positive integer".to_owned())?,
        ),
        None => None,
    };
    if workers_requested == Some(0) {
        return Err("--workers must be non-zero".to_owned());
    }
    let workers = workers_requested.unwrap_or(default_workers);

    println!(
        "offline self-play pilot: {games} games x {rounds} rounds on {workers} worker(s) -> {}",
        output.display()
    );

    // Each chunk owns deep copies of both actors; the number of chunks is bounded by the worker
    // count, so tensor memory stays O(workers), not O(games). The copies are made here on the main
    // thread as *owned* actors and wrapped in `Rc` only inside the worker closure: `tch::Tensor`
    // is `Send` but not `Sync`, so an actor may cross a thread boundary by value, never by shared
    // reference (the same seam `build_positive_corpus` uses).
    let per_worker = plans.len().div_ceil(workers).max(1);
    let jobs: Vec<(WorkerActors, Vec<GamePlan>)> = plans
        .chunks(per_worker)
        .map(|chunk| {
            (
                WorkerActors {
                    current_actor: master.current_actor.inference_copy(),
                    current_vocabulary: master.current_vocabulary.clone(),
                    older_actor: master.older_actor.inference_copy(),
                    older_vocabulary: master.older_vocabulary.clone(),
                },
                chunk.to_vec(),
            )
        })
        .collect();

    let ctx = PlayContext {
        content,
        players: &players,
        map: &map,
        staging: &staging,
        rounds,
        total_games: games,
        diplomacy_enabled,
    };
    println!(
        "  setup in {:.1}s ({} worker chunk(s), {} deep actor copies)",
        setup_started.elapsed().as_secs_f64(),
        jobs.len(),
        jobs.len() * 2
    );
    let play_started = std::time::Instant::now();
    // Worker errors carry the failing game's index so the run reports one deterministic failure
    // (the smallest index) whatever order the chunks finished in.
    let execute = || {
        jobs.into_par_iter()
            .map(|(actors, chunk)| {
                let local = LocalAssets {
                    current_actor: Rc::new(actors.current_actor),
                    current_vocabulary: actors.current_vocabulary,
                    older_actor: Rc::new(actors.older_actor),
                    older_vocabulary: actors.older_vocabulary,
                };
                let mut outcomes = Vec::with_capacity(chunk.len());
                for plan in &chunk {
                    match play_game(plan, &local, &master.shared, &ctx) {
                        Ok(outcome) => outcomes.push(outcome),
                        Err(message) => return Err((plan.game_index, message)),
                    }
                }
                Ok(outcomes)
            })
            .collect::<Vec<Result<Vec<_>, (usize, String)>>>()
    };

    // Any game that fails refuses the whole run, exactly as the sequential pilot did: a corpus
    // with a silently missing game would be worse than no corpus.
    let harvest: Vec<Result<Vec<GameOutcome>, (usize, String)>> = if workers_requested.is_none() {
        // No flag: rayon's global pool already runs one thread per logical processor, which is the
        // fastest this machine can schedule. A dedicated pool would only duplicate it.
        execute()
    } else {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .map_err(|error| format!("building worker pool: {error}"))?
            .install(execute)
    };

    if let Some((failed_game, message)) = harvest
        .iter()
        .filter_map(|chunk| chunk.as_ref().err().cloned())
        .min_by_key(|(game_index, _)| *game_index)
    {
        return Err(format!("game {failed_game}: {message}"));
    }

    let mut outcomes: Vec<GameOutcome> = Vec::with_capacity(games);
    for chunk in harvest {
        outcomes.extend(chunk.expect("failure checked above"));
    }
    // Chunks partition the plans contiguously and each worker keeps game order inside its chunk, so
    // flattening must land in game order. Verify rather than assume.
    for (position, outcome) in outcomes.iter().enumerate() {
        if outcome.game_index != position {
            return Err(format!(
                "worker results are out of order at game {position}: got {}",
                outcome.game_index
            ));
        }
    }
    println!("  games in {:.1}s", play_started.elapsed().as_secs_f64());
    // Single-checkpoint mode has no older or evolutionary assets; the empty paths would be
    // misleading entries.
    let mut checkpoint_manifests = BTreeMap::from([(
        master.shared.current_path.clone(),
        master.shared.current_manifest_sha.clone(),
    )]);
    if master.shared.single_temps.is_none() {
        checkpoint_manifests.insert(
            master.shared.older_path.clone(),
            master.shared.older_manifest_sha.clone(),
        );
        checkpoint_manifests.insert(
            master.shared.evolutionary_path.clone(),
            master.shared.evolutionary_sha.clone(),
        );
    }
    let params = PublishParams {
        games_played: games,
        games_planned: None,
        seed_base,
        rounds,
        workers: Some(workers),
        map_pool: pool_path,
        pool_sha,
        slots_sha,
        checkpoint_manifests,
        policy_mode: if master.shared.single_temps.is_some() {
            "single"
        } else {
            "mixed"
        },
        engine_git_commit: git("rev-parse HEAD"),
        engine_worktree_dirty: !git("status --porcelain").is_empty(),
        diplomacy_enabled,
        generation_executable_sha256: std::env::current_exe()
            .ok()
            .and_then(|path| file_sha(&path).ok()),
    };
    assemble_and_publish(&outcomes, &staging, &staging, &output, &params)?;
    Ok(())
}

/// Everything the assembly step needs that is not on disk: run parameters and provenance. A
/// completed run fills these from its in-memory state; publishing a killed run's staging derives
/// or takes them as arguments (`publish_staging_run`).
struct PublishParams {
    /// Games that actually completed, including games discarded by the retention filter.
    games_played: usize,
    games_planned: Option<usize>,
    seed_base: u64,
    rounds: u32,
    /// Unknown for a killed run's staging unless passed explicitly.
    workers: Option<usize>,
    map_pool: String,
    pool_sha: String,
    slots_sha: String,
    checkpoint_manifests: BTreeMap<String, String>,
    policy_mode: &'static str,
    engine_git_commit: String,
    engine_worktree_dirty: bool,
    diplomacy_enabled: bool,
    generation_executable_sha256: Option<String>,
}

/// Assemble per-bucket shards from staged per-game frames, gate them byte-exactly, write the
/// retention sidecar and manifests, and rename staging to the corpus directory. Shared by a
/// completed run and `--publish-staging` for a killed one; outcomes must be in game order.
#[expect(
    clippy::too_many_lines,
    reason = "the publish step is one linear pass: shards, gates, sidecars, manifests"
)]
fn assemble_and_publish(
    outcomes: &[GameOutcome],
    parts_root: &Path,
    publish_staging: &Path,
    output: &Path,
    params: &PublishParams,
) -> Result<(), String> {
    let finish_started = std::time::Instant::now();
    let preserve_source_parts = parts_root != publish_staging;

    // The manifest describes the corpus, so policy families come from retained games only.
    let mut policy_families = BTreeSet::new();
    for outcome in outcomes {
        if outcome.retained {
            policy_families.extend(outcome.policy_families.iter().cloned());
        }
    }

    // ---- assemble the per-bucket shards in game order -------------------------------------------
    // Only retained games have frames; outcomes are already in game order, so filtering keeps it.
    let decision_count: usize = outcomes.iter().map(|outcome| outcome.decision_count).sum();
    let diplomacy_count: usize = outcomes
        .iter()
        .map(|outcome| outcome.diplomacy_deal_count)
        .sum();
    let games_retained = outcomes.iter().filter(|outcome| outcome.retained).count();

    // The three training buckets always exist; `failed` only when a game actually failed.
    let published_buckets: &[&str] = if parts_root.join(BUCKET_FAILED).exists() {
        &[BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM, BUCKET_FAILED]
    } else {
        &[BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM]
    };
    let mut shards: BTreeMap<String, String> = BTreeMap::new();
    let mut bucket_stats: BTreeMap<String, BucketStats> = BTreeMap::new();
    for bucket in published_buckets.iter().copied() {
        // A normal run creates the training buckets at staging setup; a killed run's staging has
        // them too, but create_dir_all keeps this total over any directory shape.
        std::fs::create_dir_all(publish_staging.join(bucket)).map_err(|error| {
            format!(
                "creating {}: {error}",
                publish_staging.join(bucket).display()
            )
        })?;
        let in_bucket = |outcome: &GameOutcome| {
            outcome.retained
                && outcome
                    .retention_reason
                    .is_some_and(|reason| bucket_for(reason) == bucket)
        };
        let decision_parts: Vec<PathBuf> = outcomes
            .iter()
            .filter(|outcome| in_bucket(outcome))
            .map(|outcome| {
                parts_root
                    .join(bucket)
                    .join(decisions_part(outcome.game_index))
            })
            .collect();
        let game_parts: Vec<PathBuf> = outcomes
            .iter()
            .filter(|outcome| in_bucket(outcome))
            .map(|outcome| parts_root.join(bucket).join(games_part(outcome.game_index)))
            .collect();
        let diplomacy_parts: Vec<PathBuf> = outcomes
            .iter()
            .filter(|outcome| in_bucket(outcome))
            .map(|outcome| {
                parts_root
                    .join(bucket)
                    .join(diplomacy_part(outcome.game_index))
            })
            .collect();

        let decisions_path = publish_staging.join(bucket).join(DECISIONS_FILE);
        let games_path = publish_staging.join(bucket).join(GAMES_FILE);
        let diplomacy_path = publish_staging.join(bucket).join(DIPLOMACY_FILE);
        let expected_decisions_sha =
            concatenate_parts(&decision_parts, &decisions_path, preserve_source_parts)?;
        let expected_games_sha =
            concatenate_parts(&game_parts, &games_path, preserve_source_parts)?;
        let expected_diplomacy_sha =
            concatenate_parts(&diplomacy_parts, &diplomacy_path, preserve_source_parts)?;

        // Byte-exactness: each published shard must hold exactly the frames whose records passed
        // the loss-alignment gate before writing. A mismatch means corruption between write and
        // publish, which refuses the run instead of publishing a corpus nobody can trust.
        let decisions_sha = file_sha(&decisions_path)?;
        if decisions_sha != expected_decisions_sha {
            return Err(format!(
                "{bucket}/{DECISIONS_FILE} does not match its validated frames"
            ));
        }
        let games_sha = file_sha(&games_path)?;
        if games_sha != expected_games_sha {
            return Err(format!(
                "{bucket}/{GAMES_FILE} does not match its validated frames"
            ));
        }
        let diplomacy_sha = file_sha(&diplomacy_path)?;
        if diplomacy_sha != expected_diplomacy_sha {
            return Err(format!(
                "{bucket}/{DIPLOMACY_FILE} does not match its validated frames"
            ));
        }

        shards.insert(format!("{bucket}/{DECISIONS_FILE}"), decisions_sha);
        shards.insert(format!("{bucket}/{GAMES_FILE}"), games_sha);
        shards.insert(format!("{bucket}/{DIPLOMACY_FILE}"), diplomacy_sha);
        bucket_stats.insert(
            bucket.to_owned(),
            BucketStats {
                games: outcomes.iter().filter(|outcome| in_bucket(outcome)).count(),
                decisions: outcomes
                    .iter()
                    .filter(|outcome| in_bucket(outcome))
                    .map(|outcome| outcome.decision_count)
                    .sum(),
                diplomacy_deals: outcomes
                    .iter()
                    .filter(|outcome| in_bucket(outcome))
                    .map(|outcome| outcome.diplomacy_deal_count)
                    .sum(),
            },
        );
    }

    // Retention sidecar: one line per known outcome (a completed run knows every played game, a
    // killed run's staging holds retained games only) and calibration data for future threshold
    // tuning. Written in game order, so deterministic.
    let retention_lines: Vec<String> = outcomes
        .iter()
        .map(|outcome| {
            serde_json::to_string(&RetentionRecord {
                game_index: outcome.game_index,
                game_seed: outcome.game_seed,
                table_vp: outcome.table_vp,
                max_faction_vp: outcome.max_faction_vp,
                recorded_decisions: outcome.recorded_decisions,
                retained: outcome.retained,
                reason: outcome
                    .retention_reason
                    .map(|reason| reason.as_str().to_owned()),
            })
            .expect("retention record serializes")
        })
        .collect();
    let mut retention_log = retention_lines.join("\n");
    retention_log.push('\n');
    std::fs::write(publish_staging.join("retention.jsonl"), retention_log)
        .map_err(|error| format!("writing retention log: {error}"))?;

    println!(
        "  assembly and integrity check in {:.1}s",
        finish_started.elapsed().as_secs_f64()
    );
    let manifest = Manifest {
        schema: SCHEMA.to_owned(),
        observation_schema: OBSERVATION_SCHEMA.to_owned(),
        diplomacy_enabled: params.diplomacy_enabled,
        created_utc: chrono::Utc::now().to_rfc3339(),
        engine_git_commit: params.engine_git_commit.clone(),
        engine_worktree_dirty: params.engine_worktree_dirty,
        generation_executable_sha256: params.generation_executable_sha256.clone(),
        publisher_git_commit: git("rev-parse HEAD"),
        games: games_retained,
        rounds: params.rounds,
        workers: params.workers,
        retention_rule: RETENTION_RULE.to_owned(),
        games_played: params.games_played,
        games_planned: params.games_planned,
        games_retained,
        retention_breakdown: outcomes.iter().fold(BTreeMap::new(), |mut map, outcome| {
            if let Some(reason) = outcome.retention_reason {
                *map.entry(reason.as_str().to_owned()).or_insert(0) += 1;
            }
            map
        }),
        seed_base: params.seed_base,
        factions: IN_SCOPE_FACTIONS
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        policy_families: policy_families.into_iter().collect(),
        map_pool: params.map_pool.clone(),
        map_pool_sha256: params.pool_sha.clone(),
        vocabulary_slots_sha256: params.slots_sha.clone(),
        behavior_probabilities_recorded: false,
        forced_decisions_retained: true,
        storage_encoding: "zstd".to_owned(),
        policy_mode: params.policy_mode.to_owned(),
        checkpoint_manifests: params.checkpoint_manifests.clone(),
        records: BTreeMap::from([
            ("games".to_owned(), games_retained),
            ("decisions".to_owned(), decision_count),
            ("diplomacy_deals".to_owned(), diplomacy_count),
        ]),
        shards,
        buckets: bucket_stats.clone(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serializing manifest: {error}"))?;
    std::fs::write(publish_staging.join(MANIFEST_FILE), manifest_bytes)
        .map_err(|error| format!("writing manifest: {error}"))?;

    // Each training bucket folder is a self-contained corpus: its own shards plus this scoped
    // manifest, so downstream tooling can consume any quality class directly. `failed` stays a
    // visibility artifact without a manifest — its partial decisions are never training data.
    for (bucket, stats) in &bucket_stats {
        if *bucket == BUCKET_FAILED {
            continue;
        }
        let scoped = Manifest {
            games: stats.games,
            games_retained: stats.games,
            retention_breakdown: outcomes.iter().fold(BTreeMap::new(), |mut map, outcome| {
                if let Some(reason) = outcome.retention_reason
                    && bucket_for(reason) == *bucket
                {
                    *map.entry(reason.as_str().to_owned()).or_insert(0) += 1;
                }
                map
            }),
            policy_families: outcomes
                .iter()
                .filter(|outcome| {
                    outcome.retained
                        && outcome
                            .retention_reason
                            .is_some_and(|reason| bucket_for(reason) == *bucket)
                })
                .flat_map(|outcome| outcome.policy_families.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            records: BTreeMap::from([
                ("games".to_owned(), stats.games),
                ("decisions".to_owned(), stats.decisions),
                ("diplomacy_deals".to_owned(), stats.diplomacy_deals),
            ]),
            shards: BTreeMap::from([
                (
                    DECISIONS_FILE.to_owned(),
                    manifest.shards[&format!("{bucket}/{DECISIONS_FILE}")].clone(),
                ),
                (
                    GAMES_FILE.to_owned(),
                    manifest.shards[&format!("{bucket}/{GAMES_FILE}")].clone(),
                ),
                (
                    DIPLOMACY_FILE.to_owned(),
                    manifest.shards[&format!("{bucket}/{DIPLOMACY_FILE}")].clone(),
                ),
            ]),
            ..manifest.clone()
        };
        let scoped_bytes = serde_json::to_vec_pretty(&scoped)
            .map_err(|error| format!("serializing {bucket} manifest: {error}"))?;
        std::fs::write(
            publish_staging.join(bucket).join(MANIFEST_FILE),
            scoped_bytes,
        )
        .map_err(|error| format!("writing {bucket} manifest: {error}"))?;
    }

    std::fs::rename(publish_staging, output)
        .map_err(|error| format!("publishing {}: {error}", publish_staging.display()))?;
    println!(
        "published {games_retained}/{} games (retention {rule}) / {decision_count} decisions -> {path}",
        params.games_played,
        rule = RETENTION_RULE,
        path = output.display(),
    );
    Ok(())
}

/// One complete game recovered from a killed run's staging: its outcome plus the metadata needed
/// to derive run-level provenance (rounds, checkpoint manifests).
struct StagedGame {
    outcome: GameOutcome,
    metadata: GameMetadata,
}

/// A per-game decision, game, or diplomacy part name -> its numeric game index.
fn part_index(name: &str) -> Option<usize> {
    let stem = name.strip_suffix(".jsonl.zst")?;
    let (prefix, digits) = stem.split_once('-')?;
    if prefix != "decisions" && prefix != "games" && prefix != "diplomacy" {
        return None;
    }
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse::<usize>().ok()
}

/// True for per-game part names (the only files a publish may delete from bucket folders).
#[cfg(test)]
fn is_part_name(name: &str) -> bool {
    part_index(name).is_some()
}

/// Fully decode one zstd part. A truncated frame — the kill artifact of a game that was mid-write
/// when the run died — surfaces as an error the caller maps to "incomplete".
fn decode_part(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("opening {}: {error}", path.display()))?;
    let mut decoder = zstd::stream::read::Decoder::new(std::io::BufReader::new(file))
        .map_err(|error| format!("decoding {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut decoder, &mut bytes)
        .map_err(|error| format!("decoding {}: {error}", path.display()))?;
    Ok(bytes)
}

/// Why a staged part pair cannot be trusted. Incomplete parts are the expected kill artifact and
/// exclude their game with a report; corrupt parts are not an explainable kill artifact and refuse
/// the publish rather than risk a corpus nobody can trust.
enum PartProblem {
    Incomplete(String),
    Corrupt(String),
}

fn validate_part_pair(
    index: usize,
    decisions_path: &Path,
    games_path: &Path,
    diplomacy_path: &Path,
) -> Result<GameMetadata, PartProblem> {
    let games_bytes = decode_part(games_path).map_err(PartProblem::Incomplete)?;
    let text = String::from_utf8(games_bytes)
        .map_err(|_| PartProblem::Corrupt("games part is not UTF-8".to_owned()))?;
    // Exactly one line: the game's metadata record.
    let mut lines = text.lines();
    let first = lines
        .next()
        .ok_or_else(|| PartProblem::Incomplete("empty games part".to_owned()))?;
    if lines.next().is_some() {
        return Err(PartProblem::Corrupt(
            "games part holds more than one record".to_owned(),
        ));
    }
    let metadata: GameMetadata = serde_json::from_str(first)
        .map_err(|error| PartProblem::Corrupt(format!("parsing games part: {error}")))?;
    if metadata.game_index != index {
        return Err(PartProblem::Corrupt(format!(
            "games part for game {index} claims game {}",
            metadata.game_index
        )));
    }
    let decisions_bytes = decode_part(decisions_path).map_err(PartProblem::Incomplete)?;
    let seats: BTreeMap<&str, &SeatMetadata> = metadata
        .seats
        .iter()
        .map(|seat| (seat.seat.as_str(), seat))
        .collect();
    if seats.len() != metadata.seats.len() {
        return Err(PartProblem::Corrupt(
            "game metadata contains duplicate seat ids".to_owned(),
        ));
    }
    let mut next_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut decision_count = 0usize;
    for (line_index, line) in BufReader::new(decisions_bytes.as_slice())
        .lines()
        .enumerate()
    {
        let line = line.map_err(|error| {
            PartProblem::Corrupt(format!("reading decision record {line_index}: {error}"))
        })?;
        let decision: CapturedDecision = serde_json::from_str(&line).map_err(|error| {
            PartProblem::Corrupt(format!("parsing decision record {line_index}: {error}"))
        })?;
        validate_decision(&decision).map_err(|error| {
            PartProblem::Corrupt(format!("decision record {line_index}: {error}"))
        })?;
        if decision.game_id != metadata.game_id {
            return Err(PartProblem::Corrupt(format!(
                "decision record {line_index} claims game {} instead of {}",
                decision.game_id, metadata.game_id
            )));
        }
        let seat = seats.get(decision.seat.as_str()).ok_or_else(|| {
            PartProblem::Corrupt(format!(
                "decision record {line_index} names unknown seat {}",
                decision.seat
            ))
        })?;
        if decision.faction != seat.faction
            || decision.policy_id != seat.policy.policy_id
            || decision.policy_rng_seed != seat.policy.rng_seed
        {
            return Err(PartProblem::Corrupt(format!(
                "decision record {line_index} disagrees with metadata for seat {}",
                decision.seat
            )));
        }
        let expected = next_index.entry(decision.seat.clone()).or_default();
        if decision.seat_decision_index != *expected {
            return Err(PartProblem::Corrupt(format!(
                "decision record {line_index} has seat index {}, expected {}",
                decision.seat_decision_index, *expected
            )));
        }
        *expected += 1;
        decision_count += 1;
    }
    if decision_count != metadata.decision_count {
        return Err(PartProblem::Corrupt(format!(
            "decisions part holds {decision_count} records, metadata claims {}",
            metadata.decision_count
        )));
    }
    let diplomacy_bytes = decode_part(diplomacy_path).map_err(PartProblem::Incomplete)?;
    let mut deal_ids = BTreeSet::new();
    let mut diplomacy_count = 0usize;
    for (line_index, line) in BufReader::new(diplomacy_bytes.as_slice())
        .lines()
        .enumerate()
    {
        let line = line.map_err(|error| {
            PartProblem::Corrupt(format!("reading diplomacy record {line_index}: {error}"))
        })?;
        let record: ti4_model::DiplomacyLogRecord =
            serde_json::from_str(&line).map_err(|error| {
                PartProblem::Corrupt(format!("parsing diplomacy record {line_index}: {error}"))
            })?;
        if record.schema != ti4_engine::diplomacy::log::DIPLOMACY_LOG_SCHEMA_V2
            || record.game_id != metadata.game_id
            || !deal_ids.insert(record.deal_id)
        {
            return Err(PartProblem::Corrupt(format!(
                "diplomacy record {line_index} has a bad schema, game id, or duplicate deal id"
            )));
        }
        diplomacy_count += 1;
    }
    if diplomacy_count != metadata.diplomacy_deal_count {
        return Err(PartProblem::Corrupt(format!(
            "diplomacy part holds {diplomacy_count} records, metadata claims {}",
            metadata.diplomacy_deal_count
        )));
    }
    Ok(metadata)
}

/// Rebuild one game's outcome from its staged parts. The retention rule is a pure function of the
/// VPs and the seed, so re-deriving it must land in exactly the bucket the parts sit in — any
/// other result means inconsistent staging data.
fn reconstruct(index: usize, bucket: &str, metadata: GameMetadata) -> Result<StagedGame, String> {
    let table_vp = metadata
        .seats
        .iter()
        .map(|seat| seat.final_progress.victory_points)
        .sum::<i64>();
    let max_faction_vp = metadata
        .seats
        .iter()
        .map(|seat| seat.final_progress.victory_points)
        .max()
        .unwrap_or(0);
    let table_vp =
        i32::try_from(table_vp).map_err(|_| format!("game {index}: table VP out of range"))?;
    let max_faction_vp = i32::try_from(max_faction_vp)
        .map_err(|_| format!("game {index}: faction VP out of range"))?;
    let reason = if bucket == BUCKET_FAILED {
        if metadata.completed || metadata.error.is_none() {
            return Err(format!(
                "game {index}: failed bucket metadata does not describe a failed game"
            ));
        }
        RetentionReason::FailedGame
    } else {
        decide_retention(table_vp, max_faction_vp, metadata.game_seed).ok_or_else(|| {
            format!("game {index}: VPs no longer satisfy retention (inconsistent staging)")
        })?
    };
    if bucket_for(reason) != bucket {
        return Err(format!(
            "game {index} sits in {} but its VPs select {}",
            bucket,
            reason.as_str()
        ));
    }
    let policy_families: BTreeSet<String> = metadata
        .seats
        .iter()
        .map(|seat| seat.policy.family.clone())
        .collect();
    Ok(StagedGame {
        outcome: GameOutcome {
            game_index: index,
            game_seed: metadata.game_seed,
            decision_count: metadata.decision_count,
            recorded_decisions: metadata.decision_count,
            policy_families,
            retained: true,
            retention_reason: Some(reason),
            table_vp,
            max_faction_vp,
            diplomacy_deal_count: metadata.diplomacy_deal_count,
        },
        metadata,
    })
}

/// Scan a killed run's staging directory for complete per-game part pairs. Truncated parts (the
/// kill artifact) exclude their game with a report; decodable-but-inconsistent parts refuse the
/// publish. Results are keyed by (bucket, index), so worker completion order cannot affect them.
fn scan_staging(staging: &Path) -> Result<Vec<StagedGame>, String> {
    const BUCKETS: [&str; 4] = [BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM, BUCKET_FAILED];
    let mut pairs: Vec<(usize, usize, PathBuf, PathBuf, PathBuf)> = Vec::new();
    let mut seen_indexes: BTreeMap<usize, &'static str> = BTreeMap::new();
    for (ord, bucket) in BUCKETS.into_iter().enumerate() {
        let dir = staging.join(bucket);
        if !dir.is_dir() {
            continue;
        }
        let mut by_index: BTreeMap<usize, (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>)> =
            BTreeMap::new();
        for entry in std::fs::read_dir(&dir)
            .map_err(|error| format!("reading {}: {error}", dir.display()))?
        {
            let path = entry.map_err(|e| e.to_string())?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(index) = part_index(name) else {
                continue;
            };
            let slot = by_index.entry(index).or_default();
            if name.starts_with("decisions-") {
                slot.0 = Some(path);
            } else if name.starts_with("games-") {
                slot.1 = Some(path);
            } else {
                slot.2 = Some(path);
            }
        }
        for (index, (decisions, games, diplomacy)) in by_index {
            if let Some(previous) = seen_indexes.insert(index, bucket) {
                return Err(format!(
                    "game {index} appears in both {previous} and {bucket}"
                ));
            }
            match (decisions, games, diplomacy) {
                (Some(decisions), Some(games), Some(diplomacy)) => {
                    pairs.push((ord, index, decisions, games, diplomacy));
                }
                _ => println!("  excluding game {index} from {bucket}: incomplete part pair"),
            }
        }
    }

    // Validate in parallel: parts are independent.
    let checked = pairs
        .par_iter()
        .map(|(ord, index, decisions_path, games_path, diplomacy_path)| {
            match validate_part_pair(*index, decisions_path, games_path, diplomacy_path) {
                Ok(metadata) => Ok((*ord, *index, metadata)),
                Err(problem) => Err((*ord, *index, problem)),
            }
        })
        .collect::<Vec<Result<(usize, usize, GameMetadata), (usize, usize, PartProblem)>>>();

    let mut staged: Vec<StagedGame> = Vec::new();
    for result in checked {
        match result {
            Ok((ord, index, metadata)) => staged.push(reconstruct(index, BUCKETS[ord], metadata)?),
            Err((ord, index, problem)) => match problem {
                PartProblem::Incomplete(message) => {
                    println!("  excluding game {index} from {}: {message}", BUCKETS[ord]);
                }
                PartProblem::Corrupt(message) => {
                    return Err(format!("game {index} in {}: {message}", BUCKETS[ord]));
                }
            },
        }
    }
    staged.sort_by_key(|game| game.outcome.game_index);
    Ok(staged)
}

/// Publish a killed run's staging directory into a valid corpus from whatever complete games it
/// holds. The manifest is derived from what is on disk; only the planned game count, the training
/// checkpoint (vocabulary provenance), and optionally the worker count come from arguments.
#[expect(
    clippy::too_many_lines,
    reason = "publishing a killed run is one linear pass: args, scan, derive, assemble"
)]
fn publish_staging_run(
    staging_arg: &str,
    out_arg: Option<&str>,
    checkpoint_arg: &str,
    games_played: usize,
    games_planned: Option<usize>,
    workers: Option<usize>,
    map_pool: String,
    expected_pool_sha: Option<String>,
    policy_mode: &str,
    generation_commit: String,
    generation_dirty: bool,
    generator_sha: String,
) -> Result<(), String> {
    let staging = PathBuf::from(staging_arg);
    if !staging.is_dir() {
        return Err(format!("{} is not a directory", staging.display()));
    }
    let output = if let Some(path) = out_arg {
        PathBuf::from(path)
    } else {
        // `corpus.staging-12345` -> `corpus`: the name the run itself would have published.
        let name = staging
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let Some(stripped) = name.split_once(".staging-").map(|(head, _)| head) else {
            return Err(format!(
                "{} has no .staging- suffix; pass --out explicitly",
                staging.display()
            ));
        };
        staging.with_file_name(stripped)
    };
    if output.exists() {
        return Err(format!(
            "{} already exists; corpora are immutable",
            output.display()
        ));
    }
    let checkpoint = PathBuf::from(checkpoint_arg);
    let slots_sha = file_sha(&checkpoint.join("slots.json"))?;

    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        Path::new(&map_pool),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .map_err(|error| format!("map pool {map_pool}: {error}"))?;
    let pool_sha = sha256(&pool_bytes);
    if let Some(expected) = expected_pool_sha
        && !pool_sha.eq_ignore_ascii_case(&expected)
    {
        return Err(format!(
            "map pool digest {pool_sha} does not match generation digest {expected}"
        ));
    }

    println!(
        "publishing killed staging {} -> {}",
        staging.display(),
        output.display()
    );
    let staged = if let Some(worker_count) = workers {
        rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|index| format!("partial-publish-{index}"))
            .build()
            .map_err(|error| format!("building {worker_count}-worker publish pool: {error}"))?
            .install(|| scan_staging(&staging))?
    } else {
        scan_staging(&staging)?
    };
    if staged.is_empty() {
        return Err(format!("no complete games found in {}", staging.display()));
    }
    // Parallel workers complete non-contiguous index ranges, so a killed run may legitimately
    // retain a high game index even though fewer total games completed. Only the cardinality is
    // bounded by the actual completed count.
    if staged.len() > games_played {
        return Err(format!(
            "recovered {} retained games, more than --games-played {games_played}",
            staged.len()
        ));
    }

    // Every game's seed minus its index is the run's seed base; all must agree.
    let mut seed_base: Option<u64> = None;
    for game in &staged {
        let derived = game
            .metadata
            .game_seed
            .wrapping_sub(game.outcome.game_index as u64);
        match seed_base {
            Some(base) if base != derived => {
                return Err(format!(
                    "game {}: seed {} does not fit the run's seed base",
                    game.outcome.game_index, game.metadata.game_seed
                ));
            }
            _ => seed_base = Some(derived),
        }
    }
    let seed_base = seed_base.expect("staged is non-empty");

    // All games of one run request the same rounds.
    let mut rounds: Option<u32> = None;
    for game in &staged {
        match rounds {
            Some(value) if value != game.metadata.rounds_requested => {
                return Err(format!(
                    "game {}: rounds {} disagree with {}",
                    game.outcome.game_index, game.metadata.rounds_requested, value
                ));
            }
            _ => rounds = Some(game.metadata.rounds_requested),
        }
    }
    let rounds = rounds.expect("staged is non-empty");

    // Checkpoint provenance straight from the seats' metadata. A path may never silently acquire
    // two digests across games.
    let mut checkpoint_manifests: BTreeMap<String, String> = BTreeMap::new();
    for game in &staged {
        for seat in &game.metadata.seats {
            if let (Some(path), Some(sha)) = (
                &seat.policy.checkpoint,
                &seat.policy.checkpoint_manifest_sha256,
            ) {
                if let Some(previous) = checkpoint_manifests.insert(path.clone(), sha.clone())
                    && previous != *sha
                {
                    return Err(format!(
                        "checkpoint {path} has conflicting manifest digests"
                    ));
                }
            }
        }
    }
    let policy_mode: &'static str = match policy_mode {
        "single" => {
            if checkpoint_manifests.len() != 1
                || staged
                    .iter()
                    .flat_map(|game| &game.metadata.seats)
                    .any(|seat| {
                        seat.policy.family != "mlp"
                            || !seat.policy.policy_id.starts_with("single_mlp_t")
                    })
            {
                return Err("staged policy metadata does not match --policy-mode single".to_owned());
            }
            "single"
        }
        "mixed" => "mixed",
        _ => unreachable!("validated by caller"),
    };

    let publish_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{} has no usable final component", output.display()))?;
    let publish_staging =
        output.with_file_name(format!("{publish_name}.publishing-{}", std::process::id()));
    if publish_staging.exists() {
        return Err(format!(
            "{} already exists from an earlier attempt; preserve or remove it before retrying",
            publish_staging.display()
        ));
    }
    std::fs::create_dir(&publish_staging)
        .map_err(|error| format!("creating {}: {error}", publish_staging.display()))?;

    let params = PublishParams {
        games_played,
        games_planned,
        seed_base,
        rounds,
        workers,
        map_pool,
        pool_sha,
        slots_sha,
        checkpoint_manifests,
        policy_mode,
        engine_git_commit: generation_commit,
        engine_worktree_dirty: generation_dirty,
        diplomacy_enabled: staged
            .first()
            .is_some_and(|game| game.metadata.diplomacy_enabled),
        generation_executable_sha256: Some(generator_sha),
    };
    let outcomes: Vec<GameOutcome> = staged.iter().map(|game| game.outcome.clone()).collect();
    assemble_and_publish(&outcomes, &staging, &publish_staging, &output, &params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legal(ids: &[&str]) -> Vec<LegalAction> {
        ids.iter()
            .enumerate()
            .map(|(index, id)| LegalAction {
                index,
                id: (*id).to_owned(),
                kind: "test".to_owned(),
                label: (*id).to_owned(),
                payload: BTreeMap::new(),
                actor_features: Vec::new(),
            })
            .collect()
    }

    #[test]
    fn aligned_decision_passes() {
        assert!(is_loss_aligned(&legal(&["a", "b", "c"]), 1, "b"));
    }

    #[test]
    fn out_of_range_index_fails() {
        assert!(!is_loss_aligned(&legal(&["a", "b"]), 2, "b"));
    }

    #[test]
    fn single_mode_temperatures_parse_and_refuse_bad_values() {
        let ok = parse_temperatures("0.25,1.0,2.5").expect("the default trio parses");
        assert_eq!(ok, vec![0.25, 1.0, 2.5]);
        assert_eq!(parse_temperatures("7").unwrap(), vec![7.0]);
        assert!(parse_temperatures("").is_err());
        assert!(parse_temperatures(",").is_err());
        assert!(parse_temperatures("0").is_err());
        assert!(parse_temperatures("-1").is_err());
        assert!(parse_temperatures("nan").is_err());
        assert!(parse_temperatures("inf").is_err());
        assert!(parse_temperatures("x").is_err());
    }

    #[test]
    fn single_mode_rotates_every_seat_across_every_temperature() {
        let mut offset = 0;
        let mut seen = vec![BTreeSet::new(); 6];
        for _ in 0..3 {
            for (seat, temperatures) in seen.iter_mut().enumerate() {
                temperatures.insert((offset + seat) % 3);
            }
            offset = next_policy_offset(offset, 6, 3, true);
        }
        assert!(seen.iter().all(|temperatures| temperatures.len() == 3));
    }

    #[test]
    fn chosen_id_mismatch_fails() {
        assert!(!is_loss_aligned(&legal(&["a", "b"]), 0, "b"));
    }

    #[test]
    fn unnumbered_legal_actions_fail() {
        let mut options = legal(&["a", "b"]);
        options[1].index = 7;
        assert!(!is_loss_aligned(&options, 0, "a"));
    }

    #[test]
    fn empty_legal_actions_fail() {
        assert!(!is_loss_aligned(&[], 0, "a"));
    }

    #[test]
    fn standout_faction_is_retained() {
        assert_eq!(decide_retention(12, 7, 42), Some(RetentionReason::Standout));
    }

    #[test]
    fn exactly_six_vp_is_a_standout() {
        assert_eq!(decide_retention(15, 6, 42), Some(RetentionReason::Standout));
    }

    #[test]
    fn strong_table_is_retained() {
        assert_eq!(
            decide_retention(24, 5, 42),
            Some(RetentionReason::StrongTable)
        );
    }

    #[test]
    fn weak_table_is_retained() {
        assert_eq!(decide_retention(9, 3, 42), Some(RetentionReason::WeakTable));
    }

    #[test]
    fn table_vp_of_ten_is_not_weak() {
        assert_ne!(
            decide_retention(10, 5, 42),
            Some(RetentionReason::WeakTable)
        );
    }

    #[test]
    fn buckets_map_reasons_to_folders() {
        assert_eq!(bucket_for(RetentionReason::Standout), BUCKET_GOOD);
        assert_eq!(bucket_for(RetentionReason::StrongTable), BUCKET_GOOD);
        assert_eq!(bucket_for(RetentionReason::WeakTable), BUCKET_BAD);
        assert_eq!(bucket_for(RetentionReason::RandomControl), BUCKET_RANDOM);
        assert_eq!(bucket_for(RetentionReason::FailedGame), BUCKET_FAILED);
    }

    #[test]
    fn random_control_coin_is_deterministic_per_seed() {
        for seed in [0u64, 1, 7, 999_999, u64::MAX] {
            assert_eq!(decide_retention(15, 5, seed), decide_retention(15, 5, seed));
        }
    }

    #[test]
    fn random_control_keeps_about_five_percent() {
        let kept = (0..20_000u64)
            .filter(|seed| decide_retention(15, 5, *seed) == Some(RetentionReason::RandomControl))
            .count();
        let rate = kept as f64 / 20_000.0;
        assert!((0.03..=0.07).contains(&rate), "random control rate {rate}");
    }

    // ---- partial publish (killed staging) -------------------------------------------------------

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ti4-capture-partial-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn synthetic_seat(name: &str, checkpoint: Option<&str>, vp: i64) -> SeatMetadata {
        SeatMetadata {
            seat: name.to_owned(),
            faction: "sol".to_owned(),
            policy: PolicyMetadata {
                policy_id: "single_mlp_t100".to_owned(),
                family: "mlp".to_owned(),
                checkpoint: checkpoint.map(str::to_owned),
                checkpoint_manifest_sha256: checkpoint.map(|_| "ab".repeat(32)),
                temperature: Some(1.0),
                bias: None,
                source_profile: None,
                rng_seed: 7,
            },
            final_progress: Progress {
                victory_points: vp,
                ..Default::default()
            },
        }
    }

    /// Real staging setup creates the three training buckets up front; synthetic staging must
    /// match that shape.
    fn real_staging_shape(staging: &Path) {
        for bucket in [BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM] {
            std::fs::create_dir_all(staging.join(bucket)).expect("bucket dir");
        }
    }

    fn write_synthetic_game(
        staging: &Path,
        bucket: &str,
        index: usize,
        seed_base: u64,
        vps: &[i64],
        decisions: usize,
    ) {
        let game_seed = seed_base.wrapping_add(index as u64);
        let dir = staging.join(bucket);
        std::fs::create_dir_all(&dir).expect("bucket dir");
        let mut decisions_writer =
            JsonlZstdWriter::create(&dir.join(decisions_part(index))).expect("decisions part");
        for decision_index in 0..decisions {
            decisions_writer
                .write(&CapturedDecision {
                    game_id: format!("game-{index}"),
                    seat: "seat0".to_owned(),
                    faction: "sol".to_owned(),
                    policy_id: "single_mlp_t100".to_owned(),
                    policy_rng_seed: 7,
                    seat_decision_index: decision_index,
                    head: "test".to_owned(),
                    prompt: "test".to_owned(),
                    context: None,
                    observation: SeatAuthorizedObservation {
                        round: 1,
                        phase: "action".to_owned(),
                        active_player: Some("seat0".to_owned()),
                        active_system: None,
                        pending_step: None,
                        speaker: "seat0".to_owned(),
                        initiative_order: vec!["seat0".to_owned()],
                        revealed_public_objectives: Vec::new(),
                        revealed_public_objective_progress: Vec::new(),
                        public_seats: Vec::new(),
                        public_board: serde_json::Value::Null,
                        laws: BTreeMap::new(),
                        faceup_promissory_notes: BTreeMap::new(),
                        held_action_cards: Vec::new(),
                        held_secret_objectives: Vec::new(),
                        held_secret_progress: Vec::new(),
                        held_promissory_notes: Vec::new(),
                        diplomacy_relationships: Vec::new(),
                        active_diplomacy_deals: Vec::new(),
                        recent_diplomacy_signals: Vec::new(),
                    },
                    progress: Progress::default(),
                    critic_features: Vec::new(),
                    legal_actions: legal(&["pass"]),
                    chosen_action_index: 0,
                    chosen_action_id: "pass".to_owned(),
                })
                .expect("decision record");
        }
        decisions_writer.finish().expect("decisions finish");
        JsonlZstdWriter::create(&dir.join(diplomacy_part(index)))
            .expect("diplomacy part")
            .finish()
            .expect("diplomacy finish");
        let seats = vps
            .iter()
            .enumerate()
            .map(|(i, vp)| synthetic_seat(&format!("seat{i}"), Some("ckpt-a"), *vp))
            .collect::<Vec<_>>();
        let mut games_writer =
            JsonlZstdWriter::create(&dir.join(games_part(index))).expect("games part");
        games_writer
            .write(&GameMetadata {
                game_id: format!("game-{index}"),
                game_index: index,
                game_seed,
                tile_seed_offset: TILE_SEED_OFFSET,
                rounds_requested: 4,
                completed: true,
                error: None,
                map_placements: Vec::new(),
                seats,
                decision_count: decisions,
                diplomacy_enabled: false,
                diplomacy_deal_count: 0,
                diplomacy_signal_count: 0,
                diplomacy_telemetry: DiplomacyTelemetry::default(),
            })
            .expect("games record");
        games_writer.finish().expect("games finish");
    }

    fn partial_params() -> PublishParams {
        PublishParams {
            games_played: 4,
            games_planned: Some(10),
            seed_base: 1_000,
            rounds: 4,
            workers: Some(2),
            map_pool: "out/pools/full_np8_12_train.json".to_owned(),
            pool_sha: "poolsha".to_owned(),
            slots_sha: "slotsha".to_owned(),
            checkpoint_manifests: BTreeMap::from([("ckpt-a".to_owned(), "ab".repeat(32))]),
            policy_mode: "single",
            engine_git_commit: "generation-commit".to_owned(),
            engine_worktree_dirty: true,
            diplomacy_enabled: false,
            generation_executable_sha256: Some("cd".repeat(32)),
        }
    }

    #[test]
    fn the_partial_scan_reconstructs_outcomes_from_staging_parts() {
        let staging = temp_dir("scan");
        // max 7 >= 6 -> standout; table 27 >= 24 with max < 6 -> strong_table; table 9 < 10 -> weak.
        write_synthetic_game(&staging, BUCKET_GOOD, 0, 1_000, &[7, 3, 2, 1, 1, 0], 4);
        write_synthetic_game(&staging, BUCKET_GOOD, 1, 1_000, &[5, 5, 5, 5, 4, 3], 6);
        write_synthetic_game(&staging, BUCKET_BAD, 2, 1_000, &[3, 2, 2, 1, 1, 0], 3);
        let staged = scan_staging(&staging).expect("scan");
        assert_eq!(staged.len(), 3);
        assert_eq!(
            staged[0].outcome.retention_reason,
            Some(RetentionReason::Standout)
        );
        assert_eq!(
            staged[1].outcome.retention_reason,
            Some(RetentionReason::StrongTable)
        );
        assert_eq!(
            staged[2].outcome.retention_reason,
            Some(RetentionReason::WeakTable)
        );
        assert_eq!(staged[0].outcome.table_vp, 14);
        assert_eq!(staged[1].outcome.max_faction_vp, 5);
        assert_eq!(staged[2].outcome.decision_count, 3);
        let _ = std::fs::remove_dir_all(&staging);
    }

    #[test]
    fn the_partial_scan_excludes_truncated_parts() {
        let staging = temp_dir("trunc");
        write_synthetic_game(&staging, BUCKET_GOOD, 0, 1_000, &[7, 3, 2, 1, 1, 0], 4);
        write_synthetic_game(&staging, BUCKET_BAD, 1, 1_000, &[3, 2, 2, 1, 1, 0], 3);
        // Simulate a kill mid-write: chop the second game's decisions frame.
        let part = staging.join(BUCKET_BAD).join(decisions_part(1));
        let bytes = std::fs::read(&part).expect("read");
        std::fs::write(&part, &bytes[..bytes.len() - 8]).expect("truncate");
        let staged = scan_staging(&staging).expect("scan");
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].outcome.game_index, 0);
        let _ = std::fs::remove_dir_all(&staging);
    }

    #[test]
    fn the_partial_scan_refuses_inconsistent_parts() {
        let staging = temp_dir("corrupt");
        // Metadata claims six decisions but only three were written.
        write_synthetic_game(&staging, BUCKET_GOOD, 0, 1_000, &[7, 3, 2, 1, 1, 0], 6);
        let part = staging.join(BUCKET_GOOD).join(decisions_part(0));
        std::fs::remove_file(&part).expect("remove");
        let mut writer = JsonlZstdWriter::create(&part).expect("rewrite");
        for _ in 0..3 {
            writer
                .write(&serde_json::json!({"synthetic": true}))
                .unwrap();
        }
        writer.finish().unwrap();
        assert!(scan_staging(&staging).is_err());
        let _ = std::fs::remove_dir_all(&staging);
    }

    #[test]
    fn the_partial_scan_refuses_duplicate_game_indexes_across_buckets() {
        let staging = temp_dir("duplicate-index");
        write_synthetic_game(&staging, BUCKET_GOOD, 0, 1_000, &[7, 3, 2, 1, 1, 0], 2);
        write_synthetic_game(&staging, BUCKET_BAD, 0, 1_000, &[3, 2, 2, 1, 1, 0], 2);
        let error = scan_staging(&staging).err().expect("duplicate must fail");
        assert!(error.contains("appears in both"), "{error}");
        let _ = std::fs::remove_dir_all(&staging);
    }

    #[test]
    fn the_partial_publish_writes_a_trainable_corpus() {
        let staging = temp_dir("publish");
        real_staging_shape(&staging);
        write_synthetic_game(&staging, BUCKET_GOOD, 0, 1_000, &[7, 3, 2, 1, 1, 0], 4);
        write_synthetic_game(&staging, BUCKET_BAD, 1, 1_000, &[3, 2, 2, 1, 1, 0], 3);
        let output = staging.with_file_name("published-corpus");
        let publish_staging = staging.with_file_name("published-corpus.building");
        std::fs::create_dir(&publish_staging).expect("publication staging");
        let staged = scan_staging(&staging).expect("scan");
        let outcomes: Vec<GameOutcome> = staged.iter().map(|game| game.outcome.clone()).collect();
        assemble_and_publish(
            &outcomes,
            &staging,
            &publish_staging,
            &output,
            &partial_params(),
        )
        .expect("publish");

        assert!(staging.exists(), "source staging remains retryable");
        let manifest: Manifest =
            serde_json::from_slice(&std::fs::read(output.join(MANIFEST_FILE)).expect("manifest"))
                .expect("parse");
        assert_eq!(manifest.games, 2);
        assert_eq!(manifest.records["games"], 2);
        assert_eq!(manifest.records["decisions"], 7);
        assert_eq!(manifest.workers, Some(2));
        assert_eq!(manifest.games_played, 4);
        assert_eq!(manifest.games_planned, Some(10));
        assert_eq!(manifest.engine_git_commit, "generation-commit");
        assert_eq!(manifest.publisher_git_commit, git("rev-parse HEAD"));
        assert_eq!(manifest.policy_mode, "single");
        // Three buckets x three shards; the empty random bucket still publishes valid frames.
        assert_eq!(manifest.shards.len(), 9);
        for (name, sha) in &manifest.shards {
            let actual = file_sha(&output.join(name)).expect("shard hash");
            assert_eq!(&actual, sha, "{name}");
        }
        // Retention sidecar: one line per recovered game, in game order.
        let lines: Vec<serde_json::Value> = std::fs::read_to_string(output.join("retention.jsonl"))
            .expect("retention")
            .lines()
            .map(|line| serde_json::from_str(line).expect("record"))
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["reason"], "standout");
        assert_eq!(lines[1]["reason"], "weak_table");
        // Scoped manifests for the training buckets only.
        assert!(output.join(BUCKET_GOOD).join(MANIFEST_FILE).exists());
        assert!(output.join(BUCKET_BAD).join(MANIFEST_FILE).exists());
        assert!(output.join(BUCKET_RANDOM).join(MANIFEST_FILE).exists());
        // Published output has no parts; the killed run's source frames remain untouched.
        for bucket in [BUCKET_GOOD, BUCKET_BAD] {
            let leftovers: Vec<String> = std::fs::read_dir(output.join(bucket))
                .expect("bucket")
                .map(|entry| {
                    entry
                        .expect("entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            assert!(
                leftovers.iter().all(|name| !is_part_name(name)),
                "{leftovers:?}"
            );
        }
        assert!(staging.join(BUCKET_GOOD).join(decisions_part(0)).exists());
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(&output);
    }
}
