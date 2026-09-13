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

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use rand::{Rng, SeedableRng, seq::SliceRandom};
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

const SCHEMA: &str = "ti4-offline-selfplay-v1";
const OBSERVATION_SCHEMA: &str = "seat-authorized-canonical-mlp-v1";
const DECISIONS_FILE: &str = "decisions.jsonl.zst";
const GAMES_FILE: &str = "games.jsonl.zst";
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    schema: String,
    observation_schema: String,
    created_utc: String,
    engine_git_commit: String,
    engine_worktree_dirty: bool,
    /// Games present in the shards (retained ones); `games_played` is everything that was played.
    games: usize,
    rounds: u32,
    workers: usize,
    retention_rule: String,
    games_played: usize,
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
        }
    }
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
            head: ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice))
                .to_owned(),
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
        },
    })
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
        Ok(Self {
            encoder,
            count: 0,
        })
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
}

fn decisions_part(game_index: usize) -> String {
    format!("decisions-{game_index:06}.jsonl.zst")
}

fn games_part(game_index: usize) -> String {
    format!("games-{game_index:06}.jsonl.zst")
}

/// Join the per-game zstd frames into one shard, in game order. A zstd stream may legally contain
/// several concatenated frames (each part is exactly one), so plain concatenation stays a valid
/// stream that any decoder walks transparently; keeping each worker's output on disk means no record
/// crosses a thread boundary or accumulates in one heap. An empty bucket still gets a valid
/// zero-record zstd frame, so every published folder has the same shape and decodes cleanly.
/// Returns the sha256 of the exact bytes written, so the caller can prove the published shard is
/// byte-identical to the validated frames.
fn concatenate_parts(parts: &[PathBuf], final_path: &Path) -> Result<String, String> {
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
            std::fs::remove_file(part)
                .map_err(|error| format!("removing {}: {error}", part.display()))?;
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

    let mut game = ti4_training::rollout::setup_game_with_decider_factory(
        ctx.content,
        ctx.players,
        &plan.seated,
        DEFAULT,
        plan.game_seed,
        ctx.map,
        |baselines| {
            let mut deciders = BTreeMap::new();
            for (seat_index, player) in ctx.players.iter().enumerate() {
                let faction = plan.seated[player].as_str();
                let policy_kind =
                    POLICY_CYCLE[(plan.policy_offset + seat_index) % POLICY_CYCLE.len()];
                let policy_seed = plan
                    .game_seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(seat_index).unwrap_or(0));
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("missing baseline for {player}"))?;
                baselines_in.borrow_mut().insert(player.clone(), baseline);
                let (inner, metadata) = policy(
                    policy_kind,
                    local,
                    shared,
                    faction,
                    baseline,
                    policy_seed,
                    plan.game_index + seat_index,
                )?;
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

    let mut decisions_writer = JsonlZstdWriter::create(
        &bucket_dir.join(decisions_part(plan.game_index)),
    )?;
    for player in ctx.players {
        for decision in handles.borrow()[player].borrow().iter() {
            decisions_writer.write(decision)?;
        }
    }
    let decision_count = decisions_writer.finish()?;

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
    let output =
        PathBuf::from(argument("--out").unwrap_or_else(|| "E:/ti4-corpus/pilot-v1".to_owned()));
    let games = argument("--games")
        .map_or(Ok(DEFAULT_GAMES), |value| value.parse::<usize>())
        .map_err(|_| "--games must be a positive integer".to_owned())?;
    let rounds = argument("--rounds")
        .map_or(Ok(DEFAULT_ROUNDS), |value| value.parse::<u32>())
        .map_err(|_| "--rounds must be a positive integer".to_owned())?;
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
    let master = load_assets(current, older, evolutionary)?;

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
    // The faction deck and policy offset are a sequential state machine over the game index. Running
    // it here on the main thread guarantees each worker receives exactly the inputs the original
    // sequential loop would have given that game, whatever order the workers finish in.
    let mut plans: Vec<GamePlan> = Vec::with_capacity(games);
    let mut faction_deck: Vec<String> = Vec::new();
    let mut faction_cycle = 0u64;
    let mut policy_offset = 0usize;
    for game_index in 0..games {
        while faction_deck.len() < players.len() {
            let mut cycle: Vec<String> = IN_SCOPE_FACTIONS
                .iter()
                .map(|faction| (*faction).to_owned())
                .collect();
            cycle.shuffle(&mut ChaCha8Rng::seed_from_u64(
                seed_base ^ 0xFAC7_10A0 ^ faction_cycle,
            ));
            faction_deck.extend(cycle);
            faction_cycle += 1;
        }
        let selected: Vec<String> = faction_deck.drain(..players.len()).collect();
        let seated: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .zip(&selected)
            .map(|(player, faction)| (player.clone(), FactionId::new(faction)))
            .collect();
        let game_seed = seed_base.wrapping_add(u64::try_from(game_index).unwrap_or(0));
        plans.push(GamePlan {
            game_index,
            game_seed,
            game_id: format!("pilot-{game_seed:010}-{game_index:04}"),
            seated,
            policy_offset,
        });
        policy_offset = (policy_offset + players.len()) % POLICY_CYCLE.len();
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
    let finish_started = std::time::Instant::now();

    // The manifest describes the corpus, so policy families come from retained games only.
    let mut policy_families = BTreeSet::new();
    for outcome in &outcomes {
        if outcome.retained {
            policy_families.extend(outcome.policy_families.iter().cloned());
        }
    }

    // ---- assemble the per-bucket shards in game order -------------------------------------------
    // Only retained games have frames; outcomes are already in game order, so filtering keeps it.
    let decision_count: usize = outcomes.iter().map(|outcome| outcome.decision_count).sum();
    let games_retained = outcomes.iter().filter(|outcome| outcome.retained).count();

    // The three training buckets always exist; `failed` only when a game actually failed.
    let published_buckets: &[&str] = if staging.join(BUCKET_FAILED).exists() {
        &[BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM, BUCKET_FAILED]
    } else {
        &[BUCKET_GOOD, BUCKET_BAD, BUCKET_RANDOM]
    };
    let mut shards: BTreeMap<String, String> = BTreeMap::new();
    let mut bucket_stats: BTreeMap<String, BucketStats> = BTreeMap::new();
    for bucket in published_buckets.iter().copied() {
        let in_bucket = |outcome: &GameOutcome| {
            outcome.retained
                && outcome
                    .retention_reason
                    .is_some_and(|reason| bucket_for(reason) == bucket)
        };
        let decision_parts: Vec<PathBuf> = outcomes
            .iter()
            .filter(|outcome| in_bucket(outcome))
            .map(|outcome| staging.join(bucket).join(decisions_part(outcome.game_index)))
            .collect();
        let game_parts: Vec<PathBuf> = outcomes
            .iter()
            .filter(|outcome| in_bucket(outcome))
            .map(|outcome| staging.join(bucket).join(games_part(outcome.game_index)))
            .collect();

        let decisions_path = staging.join(bucket).join(DECISIONS_FILE);
        let games_path = staging.join(bucket).join(GAMES_FILE);
        let expected_decisions_sha = concatenate_parts(&decision_parts, &decisions_path)?;
        let expected_games_sha = concatenate_parts(&game_parts, &games_path)?;

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
            return Err(format!("{bucket}/{GAMES_FILE} does not match its validated frames"));
        }

        shards.insert(format!("{bucket}/{DECISIONS_FILE}"), decisions_sha);
        shards.insert(format!("{bucket}/{GAMES_FILE}"), games_sha);
        bucket_stats.insert(
            bucket.to_owned(),
            BucketStats {
                games: outcomes.iter().filter(|outcome| in_bucket(outcome)).count(),
                decisions: outcomes
                    .iter()
                    .filter(|outcome| in_bucket(outcome))
                    .map(|outcome| outcome.decision_count)
                    .sum(),
            },
        );
    }

    // Retention sidecar: one line per played game (retained or not) — provenance for the corpus
    // and calibration data for future threshold tuning. Written in game order, so deterministic.
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
    std::fs::write(staging.join("retention.jsonl"), retention_log)
        .map_err(|error| format!("writing retention log: {error}"))?;

    println!(
        "  assembly and integrity check in {:.1}s",
        finish_started.elapsed().as_secs_f64()
    );
    let checkpoint_manifests = BTreeMap::from([
        (
            master.shared.current_path.clone(),
            master.shared.current_manifest_sha.clone(),
        ),
        (
            master.shared.older_path.clone(),
            master.shared.older_manifest_sha.clone(),
        ),
        (
            master.shared.evolutionary_path.clone(),
            master.shared.evolutionary_sha.clone(),
        ),
    ]);
    let manifest = Manifest {
        schema: SCHEMA.to_owned(),
        observation_schema: OBSERVATION_SCHEMA.to_owned(),
        created_utc: chrono::Utc::now().to_rfc3339(),
        engine_git_commit: git("rev-parse HEAD"),
        engine_worktree_dirty: !git("status --porcelain").is_empty(),
        games: games_retained,
        rounds,
        workers,
        retention_rule: RETENTION_RULE.to_owned(),
        games_played: games,
        games_retained,
        retention_breakdown: outcomes.iter().fold(BTreeMap::new(), |mut map, outcome| {
            if let Some(reason) = outcome.retention_reason {
                *map.entry(reason.as_str().to_owned()).or_insert(0) += 1;
            }
            map
        }),
        seed_base,
        factions: IN_SCOPE_FACTIONS
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        policy_families: policy_families.into_iter().collect(),
        map_pool: pool_path,
        map_pool_sha256: pool_sha,
        vocabulary_slots_sha256: slots_sha,
        behavior_probabilities_recorded: false,
        forced_decisions_retained: true,
        storage_encoding: "zstd".to_owned(),
        checkpoint_manifests,
        records: BTreeMap::from([
            ("games".to_owned(), games_retained),
            ("decisions".to_owned(), decision_count),
        ]),
        shards,
        buckets: bucket_stats.clone(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("serializing manifest: {error}"))?;
    std::fs::write(staging.join(MANIFEST_FILE), manifest_bytes)
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
                if let Some(reason) = outcome.retention_reason && bucket_for(reason) == *bucket {
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
            ]),
            ..manifest.clone()
        };
        let scoped_bytes = serde_json::to_vec_pretty(&scoped)
            .map_err(|error| format!("serializing {bucket} manifest: {error}"))?;
        std::fs::write(staging.join(bucket).join(MANIFEST_FILE), scoped_bytes)
            .map_err(|error| format!("writing {bucket} manifest: {error}"))?;
    }

    std::fs::rename(&staging, &output)
        .map_err(|error| format!("publishing {}: {error}", output.display()))?;
    println!(
        "published {games_retained}/{games} games (retention {rule}) / {decision_count} decisions -> {path}",
        rule = RETENTION_RULE,
        path = output.display(),
    );
    Ok(())
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
}
