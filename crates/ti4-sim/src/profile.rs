//! Bounded profile with raw samples (M09-019b).
//!
//! The first real component breakdown of per-decision cost on the post-rules tree. MLP plan §2
//! quotes ~450 µs/decision *inferred from an aggregate training log*; this module measures the
//! three components — engine stepping, feature extraction, model scoring — under the fixed
//! M00-012 protocol (`plans/evidence/M00-012{,a,b,c,d,e}.md`) instead of inferring them.
//!
//! # Protocol conformance (predeclared in `plans/M09-019b_BOUNDED_PROFILE_FEATURE_INVENTORY.md`)
//!
//! - 10 unmeasured warmup iterations per workload, each passing its semantic gate, then a
//!   five-second idle before timed samples begin (M00-012a).
//! - 30 timed samples per workload, none discarded; monotonic elapsed nanoseconds only
//!   (M00-012b); fresh game/workload state per iteration; seeds from the manifest constants
//!   below, never ambient entropy.
//! - Single worker; no power-plan, priority, or affinity change by the runner (M00-012c).
//! - Variance thresholds fixed in advance from M00-012e for the single-core game / policy
//!   scoring classes: stdev/mean ≤ 5% and (p95 − p50)/median ≤ 10%. A failed threshold triggers
//!   one fresh repeat run; both reports are then retained and the result marked `unstable`.
//! - One M00-012d schema report per workload, raw samples included, written to gitignored
//!   `out/profiles/`; sha256 + summary committed in evidence.
//!
//! # The semantic gate is the honest part
//!
//! A timing number means nothing if the workload did not run the shape it was asked for:
//!
//! - **W1 (engine)** plays one *complete* game per sample: on this engine every full game ends
//!   by objective-deck exhaustion at round 9, so completion is the workload's natural shape. A
//!   sample that errors or hits the safety bounds ran less work and fails the gate.
//! - **W2/W3 (feature/model)** replay to one fixed production-head choice; the captured position
//!   must be identical across every iteration (same step index, same option ids), which makes
//!   replay determinism part of the gate rather than an assumption.
//!
//! # No optimization bundled
//!
//! This module is measurement code only. It changes no engine, feature-construction, or
//! inference behavior to improve a number (the row says "no optimization bundled").

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, Observed, SeededRandom, Table as ChoiceTable};
use ti4_engine::game::Game;
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::{DEFAULT, SourceSet};
use ti4_model::id::PlayerId;
use ti4_policy::features::{FeatureVector, explicit_choice_features};
use ti4_policy::inference::probabilities;
use ti4_policy::learned::{Profile, decision_head};

use crate::baseline::{Champions, R6_CHECKPOINT_SHA_PREFIX};
use crate::benchmark::{Host, Sample, SemanticGate, Statistics};
use crate::maps::MapPool;
use crate::run::Table as SimTable;

/// Unmeasured warmup iterations before timed samples (M00-012a).
pub const WARMUP_ITERATIONS: usize = 10;
/// Timed samples per workload (M00-012b).
pub const TIMED_SAMPLES: usize = 30;
/// Idle period between warmup and the first timed sample (M00-012a).
pub const IDLE_BEFORE_TIMING: Duration = Duration::from_secs(5);

/// W1 seed base: sample i runs on `W1_SEED_BASE + i` for i in 0..40. Distinct from the M08-021
/// behavioral suite (`812_xxx`) and the M09-019a panel (`919_001..=919_030`).
pub const W1_SEED_BASE: u64 = 919_501;
/// The one fixed position for all W2/W3 iterations (game seed = tile seed).
pub const W2W3_FIXTURE_SEED: u64 = 919_601;

/// W1 safety step bound per sample. A complete game on this engine ends by objective-deck
/// exhaustion long before it (measured: every seed in the manifest completes at round 9);
/// reaching the bound means the workload did not run its shape.
pub const W1_HORIZON_STEPS: usize = 20_000;
/// W1 safety round cap (the default horizon's).
pub const W1_ROUND_CAP: u32 = 50;
/// Safety bound for W2/W3 fixture replay; exceeding it fails the semantic gate.
pub const REPLAY_STEP_BOUND: usize = 5_000;

/// The holdout pool (Validation role), relative to the repository root — same board process as
/// the accepted M09-019a panel. The final-role pool is deliberately not used (parent non-goal).
pub const POOL_PATH: &str = "out/pools/full_np8_12_holdout.json";
/// sha256 prefix of the holdout pool (MLP plan §10 artifact manifest).
pub const POOL_SHA_PREFIX: &str = "aba33c81aa04cefb";
/// The r6 checkpoint envelope, relative to the repository root; loaded through [`Champions`],
/// which validates every profile.
pub const CHECKPOINT_PATH: &str = "out/stage2_r6/final10000.json";
/// Where raw reports land, relative to the repository root (gitignored).
pub const REPORT_DIR: &str = "out/profiles";

/// Predeclared variance thresholds (M00-012e, single-core game / policy scoring class).
pub const MAX_STDEV_PCT: f64 = 5.0;
/// Predeclared spread threshold: (p95 − p50)/median as a percentage of the median.
pub const MAX_SPREAD_PCT: f64 = 10.0;

/// The head whose choice is the W2/W3 fixture: a first-class head with a large, payload-rich
/// option set — representative of the expensive end of per-decision policy work.
const FIXTURE_HEAD: &str = "production";
/// A production menu with fewer options than this is not the shape the workload was asked for.
const MIN_FIXTURE_OPTIONS: usize = 3;

// ---------------------------------------------------------------------------
// Report schema (M00-012d, plus per-sample units of work)
// ---------------------------------------------------------------------------

/// One workload's benchmark report, in the M00-012d schema with one declared extension:
/// `units_per_sample` / `total_units` / `nanos_per_unit`, so the raw samples carry their
/// normaliser (W1: resolved choices; W2/W3: options) and a per-unit figure is recomputable from
/// the report alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileReport {
    /// Schema version, fixed by the protocol.
    pub schema_version: String,
    /// Which benchmark this is (`m09_019b_w1_engine`, `..._w2_feature`, `..._w3_model`).
    pub benchmark_id: String,
    /// Always `rust` here; there is no paired implementation (parity is not an acceptance
    /// criterion and no speedup claim is made).
    pub implementation: String,
    /// No oracle commit applies.
    pub oracle_commit: Option<String>,
    /// The Rust commit at run time, when it could be read.
    pub rust_commit: Option<String>,
    /// Where it ran (the accepted protocol reader; changes nothing about the host).
    pub host: Host,
    /// What it ran.
    pub workload: WorkloadBlock,
    /// Warmup iterations completed before timing began.
    pub warmup_iterations: usize,
    /// Every timed sample in nanoseconds, undiscarded.
    pub samples_ns: Vec<u128>,
    /// The unit of work each sample did (W1: resolved choices; W2/W3: options extracted/scored).
    pub units_per_sample: Vec<usize>,
    /// Total units across the timed samples — the normaliser, checkable against the raw data.
    pub total_units: usize,
    /// Mean nanoseconds per unit of work (the derived figure evidence quotes).
    pub nanos_per_unit: f64,
    /// Summary over all raw samples (the accepted M00-012 math).
    pub statistics_ns: StatisticsNs,
    /// The predeclared variance verdict.
    pub variance: VarianceBlock,
    /// Audit metadata; excluded from hash/equality comparisons per the protocol.
    pub captured_at_utc: String,
}

/// What a workload ran (M00-012d `workload` block).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadBlock {
    /// The fixture or plan this measured.
    pub fixture_id: String,
    /// The manifest seed (W1's base; the single W2/W3 fixture seed).
    pub seed: u64,
    /// Workers used — always one for these single-core workloads.
    pub workers: usize,
    /// Whether every sample did the work it was asked for.
    pub semantic_gate: SemanticGate,
}

/// Summary statistics in the M00-012d field names (computed by [`Statistics::over`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StatisticsNs {
    /// How many samples.
    pub count: usize,
    /// Arithmetic mean.
    pub mean: f64,
    /// Median, also p50.
    pub median: f64,
    /// Standard deviation (the protocol module's convention — variance over all samples).
    pub stdev: f64,
    /// Fastest sample.
    pub min: u128,
    /// Slowest sample.
    pub max: u128,
    /// 50th percentile (nearest rank).
    pub p50: f64,
    /// 95th percentile (nearest rank).
    pub p95: f64,
    /// 99th percentile (nearest rank).
    pub p99: f64,
}

/// The predeclared variance verdict for one workload.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VarianceBlock {
    /// stdev/mean as a percentage of the mean.
    pub stdev_pct: f64,
    /// (p95 − p50)/median as a percentage of the median.
    pub p95_minus_p50_pct: f64,
    /// Whether both predeclared thresholds held.
    pub accepted: bool,
}

impl ProfileReport {
    /// Assemble one workload's report from its raw samples and their units of work.
    ///
    /// A sample reaching this point has already passed its semantic gate inside the runner; a
    /// failed one aborts the campaign before any report is assembled (M00-012b). An empty set
    /// still fails closed on the gate field.
    #[must_use]
    pub fn assemble(
        benchmark_id: &str,
        fixture_id: String,
        seed: u64,
        samples: &[(u128, usize)],
        rust_commit: Option<String>,
    ) -> Self {
        let protocol_samples: Vec<Sample> = samples
            .iter()
            .enumerate()
            .map(|(index, (nanos, units))| Sample {
                pair: index,
                seed,
                nanos: *nanos,
                games: 0,
                decisions: *units,
                gate: SemanticGate::Pass,
            })
            .collect();
        let statistics = Statistics::over(&protocol_samples);
        let samples_ns: Vec<u128> = samples.iter().map(|(nanos, _)| *nanos).collect();
        let units_per_sample: Vec<usize> = samples.iter().map(|(_, units)| *units).collect();
        let total_units: usize = units_per_sample.iter().sum();

        let total_ns: u128 = samples.iter().map(|(nanos, _)| *nanos).sum();
        let gate = if samples.is_empty() {
            SemanticGate::Fail
        } else {
            SemanticGate::Pass
        };
        let stdev_pct = if statistics.mean_nanos > 0.0 {
            statistics.stdev_nanos / statistics.mean_nanos * 100.0
        } else {
            f64::INFINITY
        };
        let spread_pct = if statistics.median_nanos > 0.0 {
            (statistics.p95_nanos - statistics.median_nanos) / statistics.median_nanos * 100.0
        } else {
            f64::INFINITY
        };
        Self {
            schema_version: "1.0.0".to_owned(),
            benchmark_id: benchmark_id.to_owned(),
            implementation: "rust".to_owned(),
            oracle_commit: None,
            rust_commit,
            host: Host::observed(),
            workload: WorkloadBlock {
                fixture_id,
                seed,
                workers: 1,
                semantic_gate: gate,
            },
            warmup_iterations: WARMUP_ITERATIONS,
            samples_ns,
            units_per_sample,
            total_units,
            nanos_per_unit: if total_units == 0 {
                0.0
            } else {
                as_float_u128(total_ns) / as_float(total_units)
            },
            statistics_ns: StatisticsNs {
                count: statistics.samples,
                mean: statistics.mean_nanos,
                median: statistics.median_nanos,
                stdev: statistics.stdev_nanos,
                min: statistics.min_nanos,
                max: statistics.max_nanos,
                p50: statistics.median_nanos,
                p95: statistics.p95_nanos,
                p99: statistics.p99_nanos,
            },
            variance: VarianceBlock {
                stdev_pct,
                p95_minus_p50_pct: spread_pct,
                accepted: stdev_pct <= MAX_STDEV_PCT && spread_pct <= MAX_SPREAD_PCT,
            },
            captured_at_utc: Utc::now().to_rfc3339(),
        }
    }
}

/// Nanoseconds as a float (exact below 2^53 ≈ 104 days).
#[expect(
    clippy::cast_precision_loss,
    reason = "nanosecond counts are far below 2^53"
)]
fn as_float_u128(value: u128) -> f64 {
    u64::try_from(value).unwrap_or(u64::MAX) as f64
}

/// A count as a float.
#[expect(clippy::cast_precision_loss, reason = "unit counts are small")]
fn as_float(value: usize) -> f64 {
    value as f64
}

// ---------------------------------------------------------------------------
// Workload builders
// ---------------------------------------------------------------------------

/// The six seats, in the stable order every panel uses.
fn players() -> Vec<PlayerId> {
    (1..=6)
        .map(|index| PlayerId::new(format!("p{index}")))
        .collect()
}

/// Build one fresh game on a pool board with pure-random seats — the shared setup of all three
/// workloads. The tile seed is the game seed itself, matching `play_learned`'s board process.
fn build_game<'c>(
    content: &'c ContentStore,
    sources: SourceSet,
    pool: &MapPool,
    seed: u64,
) -> Result<Game<'c>, String> {
    let table = SimTable::seated(content, &players(), sources);
    let mut state = start_game_seeded(content, &table.players, sources, None, seed)
        .map_err(|error| format!("setup: {error}"))?;

    for (player, faction) in &table.factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }

    // Home systems in assignment order: the pool places them into its home slots.
    let mut homes: Vec<String> = Vec::with_capacity(table.factions.len());
    for faction in table.factions.values() {
        match ti4_content::factions::get(content, faction.as_str())
            .and_then(|record| record.home_system())
            .map(str::to_owned)
        {
            Some(home) => homes.push(home),
            None => return Err(format!("faction {faction} has no home system")),
        }
    }
    let borrowed: Vec<&str> = homes.iter().map(String::as_str).collect();
    let galaxy = pool
        .galaxy(content, sources, seed, &borrowed)
        .map_err(|error| format!("pool: {error}"))?;

    for (player, faction) in &table.factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, sources)
            .map_err(|error| format!("deploy: {error}"))?;
    }

    let deciders = ChoiceTable::with_default(Box::new(SeededRandom::new(seed)));
    Ok(Game::with_table(state, content, deciders).with_galaxy(galaxy))
}

/// Replay exactly `target_steps` steps from a fresh game and return the live position with the
/// choice offered at that point. Fresh state per call — this is what makes every W2/W3 sample an
/// independent iteration rather than thirty reads of one cached position.
fn position_at<'c>(
    content: &'c ContentStore,
    sources: SourceSet,
    pool: &MapPool,
    seed: u64,
    target_steps: usize,
) -> Result<(Game<'c>, Choice), String> {
    let mut game = build_game(content, sources, pool, seed)?;
    for step in 0..target_steps {
        let result = game.step();
        if result.finished || result.error.is_some() {
            return Err(format!(
                "replay died at step {} of {target_steps} (finished={}, error={:?})",
                step, result.finished, result.error
            ));
        }
    }
    let choice = game
        .legal_options()
        .ok_or_else(|| format!("no choice offered after {target_steps} steps"))?;
    Ok((game, choice))
}

/// The W2/W3 fixture: the first production-head choice with a real menu, reached by replay.
struct Fixture {
    /// Steps executed before this choice is offered.
    step_index: usize,
    /// The option ids in engine order — the identity the semantic gate checks across iterations.
    option_ids: Vec<String>,
}

/// Find (once) where the fixture position is. Fails closed if replay cannot reach a production
/// head inside [`REPLAY_STEP_BOUND`] steps.
fn capture_fixture(
    content: &ContentStore,
    sources: SourceSet,
    pool: &MapPool,
) -> Result<Fixture, String> {
    let mut game = build_game(content, sources, pool, W2W3_FIXTURE_SEED)?;
    let mut steps = 0usize;
    loop {
        if let Some(choice) = game.legal_options()
            && decision_head(&choice) == FIXTURE_HEAD
            && choice.options.len() >= MIN_FIXTURE_OPTIONS
        {
            return Ok(Fixture {
                step_index: steps,
                option_ids: choice
                    .options
                    .iter()
                    .map(|option| option.id.clone())
                    .collect(),
            });
        }
        let result = game.step();
        steps += 1;
        if result.finished || result.error.is_some() || steps > REPLAY_STEP_BOUND {
            return Err(format!(
                "fixture replay failed at step {} (finished={}, error={:?}, bound={REPLAY_STEP_BOUND})",
                steps, result.finished, result.error
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// The three workloads — each returns (elapsed nanoseconds, units of work), or None on a
// semantic-gate failure.
// ---------------------------------------------------------------------------

/// W1 — engine. One complete game from setup to natural termination; units are the resolved
/// choices (the normaliser comparable to the pre-rules per-decision aggregate).
///
/// Why "complete" rather than a fixed step budget: on this engine every full game ends by
/// objective-deck exhaustion at round 9 regardless of play style (see `w1_ending_diagnostic`), so
/// "play one complete game" is the workload's natural shape, and per-decision normalisation keeps
/// samples comparable even when games differ in step count. `None`: the game did not complete —
/// an engine error, a round-cap or safety-step-bound hit means it ran a different shape of work.
fn w1_sample(
    content: &ContentStore,
    sources: SourceSet,
    pool: &MapPool,
    seed: u64,
) -> Option<(u128, usize)> {
    let mut game = build_game(content, sources, pool, seed).ok()?;
    let started = Instant::now();
    let outcome = game.run(W1_ROUND_CAP, W1_HORIZON_STEPS);
    let elapsed = started.elapsed().as_nanos();
    match outcome {
        Ok(state) if state.finished => Some((elapsed, game.table.log.records.len())),
        _ => None, // engine error, round-capped, or safety-bound: not the shape asked for.
    }
}

/// W2 — feature extraction at the fixture position; units are the option vectors extracted.
/// `None`: replay determinism broke (position mismatch) or the extraction came back empty.
fn w2_sample(
    content: &ContentStore,
    sources: SourceSet,
    pool: &MapPool,
    fixture: &Fixture,
) -> Option<(u128, usize)> {
    let (game, choice) = position_at(
        content,
        sources,
        pool,
        W2W3_FIXTURE_SEED,
        fixture.step_index,
    )
    .ok()?;
    if choice
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect::<Vec<_>>()
        != fixture.option_ids
    {
        return None; // this is not the position the run was asked for.
    }
    let seen = Observed::new(&game.state, content, sources, None);
    let started = Instant::now();
    let vectors = explicit_choice_features(&seen, &choice, &choice.player);
    let elapsed = started.elapsed().as_nanos();
    if vectors.iter().all(FeatureVector::is_empty) {
        return None; // an empty extraction is not the work that was asked for.
    }
    Some((elapsed, choice.options.len()))
}

/// W3 — model scoring at the fixture position: head resolution + per-option score + softmax,
/// exactly the live inference path's arithmetic on precomputed feature vectors (extraction cost
/// is W2's number; W2 + W3 together approximate one `consider()`). Units are options scored.
/// `None`: position mismatch, missing champion, non-finite scores, or a softmax that does not
/// close to unit mass.
fn w3_sample(
    content: &ContentStore,
    sources: SourceSet,
    pool: &MapPool,
    fixture: &Fixture,
    champions: &Champions,
) -> Option<(u128, usize)> {
    let (game, choice) = position_at(
        content,
        sources,
        pool,
        W2W3_FIXTURE_SEED,
        fixture.step_index,
    )
    .ok()?;
    if choice
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect::<Vec<_>>()
        != fixture.option_ids
    {
        return None;
    }
    let seat = game.state.player(&choice.player)?;
    let profile: &Profile = champions.profiles.get(seat.faction.as_str())?.as_ref();

    // Feature vectors are the input to this workload, not part of its timed region.
    let seen = Observed::new(&game.state, content, sources, None);
    let vectors = explicit_choice_features(&seen, &choice, &choice.player);

    let started = Instant::now();
    let head = profile.resolved_head(decision_head(&choice));
    let mut scores: BTreeMap<String, f64> = BTreeMap::new();
    for (option, vector) in choice.options.iter().zip(vectors.iter()) {
        scores.insert(option.id.clone(), profile.score_vector(head, vector));
    }
    let temperature = profile.head(head).map_or(1.0, |head| head.temperature);
    let chances = probabilities(&scores, temperature);
    let elapsed = started.elapsed().as_nanos();

    if !scores.values().all(|score| score.is_finite()) {
        return None;
    }
    let mass: f64 = chances.values().sum();
    if (mass - 1.0).abs() > 1e-6 {
        return None; // the softmax did not close: the arithmetic ran a different shape.
    }
    Some((elapsed, choice.options.len()))
}

// ---------------------------------------------------------------------------
// Campaign runner
// ---------------------------------------------------------------------------

/// Why the campaign cannot run or must abort.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("read artifact: {0}")]
    Io(#[from] std::io::Error),
    #[error("pool checksum mismatch: expected prefix {expected}, found {found}")]
    PoolChecksumMismatch { expected: String, found: String },
    #[error("{0}")]
    Workload(String),
    #[error("input artifact changed during the campaign: {details} (refusing to report)")]
    InputOverwritten { details: String },
}

/// The inputs' identity before and after the campaign — the non-overwrite proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputProof {
    /// sha256 of the pool file before the campaign.
    pub pool_before: [u8; 32],
    /// sha256 of the pool file after the campaign (must equal `pool_before`).
    pub pool_after: [u8; 32],
    /// sha256 of the checkpoint envelope as loaded (verified against its manifest prefix).
    pub checkpoint: String,
}

/// The repository root, resolved from the crate's manifest location so the campaign works no
/// matter which directory cargo happens to run a test from (tests: package dir; examples:
/// workspace root).
///
/// # Panics
/// Panics if the crate is not at `crates/ti4-sim` inside the workspace — a build-time layout
/// defect, not a runtime condition.
#[must_use]
pub fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|crates| crates.parent())
        .map(Path::to_path_buf)
        .expect("workspace layout: crates/ti4-sim")
}

/// Run all three workloads end to end and write one M00-012d report per workload into
/// `root/out/profiles`.
///
/// # Errors
/// [`ProfileError`] on any checksum mismatch, semantic-gate failure in a warmup or timed sample,
/// or detected input mutation. A failed **timed** sample aborts the whole campaign (M00-012b) —
/// there is no partial report.
pub fn run_campaign(root: &Path) -> Result<(Vec<ProfileReport>, InputProof), ProfileError> {
    let pool_path = root.join(POOL_PATH);
    let checkpoint_path = root.join(CHECKPOINT_PATH);
    let out_dir = root.join(REPORT_DIR);

    let pool_bytes = fs::read(&pool_path)?;
    let pool_sha = Sha256::digest(&pool_bytes);
    if !hex(pool_sha).starts_with(POOL_SHA_PREFIX) {
        return Err(ProfileError::PoolChecksumMismatch {
            expected: POOL_SHA_PREFIX.to_owned(),
            found: hex(pool_sha),
        });
    }
    let pool = MapPool::load(&pool_path)
        .map_err(|error| ProfileError::Workload(format!("pool load: {error}")))?;

    // The champions are loaded through the fail-closed panel loader: checksum + per-faction
    // validation before any measurement.
    let champions = Champions::load_checkpoint_accepted(&checkpoint_path, R6_CHECKPOINT_SHA_PREFIX)
        .map_err(|error| ProfileError::Workload(format!("checkpoint: {error}")))?;

    let content = ContentStore::embedded();
    // The project's runtime scope (DEFAULT = FULL). `SourceSet::default()` is the *empty* set —
    // an EnumSet default, not a scope — and would fail setup with no strategy card set.
    let sources = DEFAULT;
    let rust_commit = current_commit();

    fs::create_dir_all(&out_dir)?;

    // W2/W3 share one fixture position, found once before any timing begins.
    let fixture = capture_fixture(content, sources, &pool).map_err(ProfileError::Workload)?;

    let mut reports = Vec::new();

    // W1 — engine: fresh game per iteration on its own manifest seed.
    reports.push(run_workload(
        &out_dir,
        "m09_019b_w1_engine",
        "holdout-pool random seats, one complete game per sample".to_owned(),
        W1_SEED_BASE,
        "W1 warmup failed its semantic gate (a sample did not complete)",
        rust_commit.clone(),
        |i| w1_sample(content, sources, &pool, W1_SEED_BASE + i as u64),
    )?);

    // W2 — feature extraction at the shared fixture position.
    reports.push(run_workload(
        &out_dir,
        "m09_019b_w2_feature",
        format!(
            "fixture production choice at step {} of seed {W2W3_FIXTURE_SEED}; whole-choice explicit extraction",
            fixture.step_index
        ),
        W2W3_FIXTURE_SEED,
        "W2 warmup failed its semantic gate (position mismatch or empty extraction)",
        rust_commit.clone(),
        |_i| w2_sample(content, sources, &pool, &fixture),
    )?);

    // W3 — model scoring at the shared fixture position.
    reports.push(run_workload(
        &out_dir,
        "m09_019b_w3_model",
        format!(
            "fixture production choice at step {} of seed {W2W3_FIXTURE_SEED}; head + per-option score + softmax",
            fixture.step_index
        ),
        W2W3_FIXTURE_SEED,
        "W3 warmup failed its semantic gate (position mismatch or non-closed softmax)",
        rust_commit.clone(),
        |_i| w3_sample(content, sources, &pool, &fixture, &champions),
    )?);

    // Non-overwrite proof: the pool must be byte-identical after the campaign.
    let pool_after = Sha256::digest(&fs::read(&pool_path)?);
    if pool_after != pool_sha {
        return Err(ProfileError::InputOverwritten {
            details: format!(
                "pool sha256 changed from {} to {}",
                hex(pool_sha),
                hex(pool_after)
            ),
        });
    }

    Ok((
        reports,
        InputProof {
            pool_before: pool_sha.into(),
            pool_after: pool_after.into(),
            checkpoint: champions.source_sha256.clone(),
        },
    ))
}

/// Run one workload's 10 warmups (each must pass), the five-second idle, and 30 timed samples.
/// Returns `(warmup_all_passed, timed_samples)`; a failed **timed** sample aborts with an error —
/// M00-012b invalidates the whole run on any failure, so there is no such thing as a partial set.
fn sample_workload<F>(mut workload: F) -> Result<(bool, Vec<(u128, usize)>), ProfileError>
where
    F: FnMut(usize) -> Option<(u128, usize)>,
{
    let mut warmup_ok = true;
    for i in 0..WARMUP_ITERATIONS {
        if workload(i).is_none() {
            warmup_ok = false;
        }
    }

    std::thread::sleep(IDLE_BEFORE_TIMING);

    let mut samples = Vec::with_capacity(TIMED_SAMPLES);
    for i in 0..TIMED_SAMPLES {
        match workload(WARMUP_ITERATIONS + i) {
            Some(sample) => samples.push(sample),
            None => {
                return Err(ProfileError::Workload(format!(
                    "timed sample {} failed its semantic gate; the run is invalid (M00-012b)",
                    WARMUP_ITERATIONS + i
                )));
            }
        }
    }
    Ok((warmup_ok, samples))
}

/// Write one report as pretty JSON under `out_dir/<benchmark_id>.json`.
fn write_report(out_dir: &Path, report: &ProfileReport) -> Result<(), ProfileError> {
    let path = out_dir.join(format!("{}.json", report.benchmark_id));
    fs::write(
        path,
        serde_json::to_string_pretty(report).expect("report serializes"),
    )?;
    Ok(())
}

/// Run one workload end to end — warmups + timed samples — then assemble and write its report.
/// A failed **timed** sample propagates as an error (M00-012b: no partial reports); a failed
/// warmup is reported through `gate_failure` so the caller's message names the workload.
fn run_workload<F>(
    out_dir: &Path,
    benchmark_id: &str,
    fixture_id: String,
    seed: u64,
    gate_failure: &'static str,
    rust_commit: Option<String>,
    workload: F,
) -> Result<ProfileReport, ProfileError>
where
    F: FnMut(usize) -> Option<(u128, usize)>,
{
    let (warmup_ok, samples) = sample_workload(workload)?;
    if !warmup_ok {
        return Err(ProfileError::Workload(gate_failure.to_owned()));
    }
    let report = ProfileReport::assemble(benchmark_id, fixture_id, seed, &samples, rust_commit);
    write_report(out_dir, &report)?;
    Ok(report)
}

/// The current HEAD, when git can answer; audit metadata only.
fn current_commit() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}

/// sha256 as lowercase hex.
fn hex(digest: impl AsRef<[u8]>) -> String {
    let mut out = String::with_capacity(64);
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The full campaign. Gated so the default suite stays fast; evidence records the exact
    /// invocation (`TI4_M09_019B_PROFILE=1 cargo test -p ti4-sim m09_019b_profile_campaign`).
    #[test]
    fn m09_019b_profile_campaign() {
        if std::env::var_os("TI4_M09_019B_PROFILE").is_none() {
            eprintln!("skipping M09-019b profile campaign (set TI4_M09_019B_PROFILE=1 to run)");
            return;
        }
        let root = repo_root();
        let (reports, proof) = run_campaign(&root).expect("campaign failed");
        for report in &reports {
            println!(
                "{}: mean={:.0} ns median={:.0} ns stdev%={:.2} spread%={:.2} accepted={} gate={:?} units/sample≈{}",
                report.benchmark_id,
                report.statistics_ns.mean,
                report.statistics_ns.median,
                report.variance.stdev_pct,
                report.variance.p95_minus_p50_pct,
                report.variance.accepted,
                report.workload.semantic_gate,
                report.total_units / TIMED_SAMPLES,
            );
        }
        println!(
            "non-overwrite: pool before={} after={} checkpoint={}",
            hex(proof.pool_before),
            hex(proof.pool_after),
            proof.checkpoint
        );
    }

    #[expect(
        clippy::float_cmp,
        reason = "exact comparisons are semantically correct: a zero-spread set computes exactly 0.0"
    )]
    #[test]
    fn variance_verdict_uses_the_predeclared_thresholds() {
        // Zero spread passes (the protocol says "at most").
        let flat = [(100u128, 4usize); TIMED_SAMPLES];
        let at = ProfileReport::assemble("t", "f".into(), 1, &flat, None);
        assert!(at.variance.accepted, "zero spread must pass");

        // A set whose stdev/mean far exceeds 5% must be rejected: 29 samples at 100 ns and one
        // at 400 ns give mean = 110, population stdev ≈ 53.9 → ~48.9%.
        let mut samples = vec![(100u128, 4usize); TIMED_SAMPLES - 1];
        samples.push((400, 4));
        let wide = ProfileReport::assemble("t", "f".into(), 1, &samples, None);
        assert!(
            !wide.variance.accepted,
            "a ~49% stdev/mean must fail the 5% threshold"
        );
        assert!(wide.variance.stdev_pct > MAX_STDEV_PCT);

        // The fields are computed as declared on a passing set.
        let tight = ProfileReport::assemble("t", "f".into(), 1, &flat, None);
        assert_eq!(tight.variance.stdev_pct, 0.0);
        assert_eq!(tight.variance.p95_minus_p50_pct, 0.0);
    }

    #[expect(
        clippy::float_cmp,
        reason = "percentile values are u128s below 2^53 converted losslessly; exact equality is the assertion"
    )]
    #[test]
    fn report_schema_round_trips_with_all_protocol_fields() {
        let samples: Vec<(u128, usize)> = (0..TIMED_SAMPLES)
            .map(|i| (1_000 + i as u128 * 7, 5))
            .collect();
        let report = ProfileReport::assemble(
            "m09_019b_test",
            "fixture".into(),
            42,
            &samples,
            Some("abc".into()),
        );
        assert_eq!(report.schema_version, "1.0.0");
        assert_eq!(report.implementation, "rust");
        assert!(report.oracle_commit.is_none());
        assert_eq!(report.rust_commit.as_deref(), Some("abc"));
        assert_eq!(report.workload.workers, 1);
        assert_eq!(report.warmup_iterations, WARMUP_ITERATIONS);
        let expected_ns: Vec<u128> = samples.iter().map(|(nanos, _)| *nanos).collect();
        assert_eq!(report.samples_ns, expected_ns);
        assert_eq!(report.units_per_sample, vec![5; TIMED_SAMPLES]);
        assert_eq!(report.total_units, 5 * TIMED_SAMPLES);
        // nanos_per_unit = total ns / total units.
        let total_ns: u128 = expected_ns.iter().sum();
        assert!(
            (report.nanos_per_unit - as_float_u128(total_ns) / as_float(report.total_units)).abs()
                < 1e-9
        );

        // Nearest-rank percentiles on 30 ascending samples: p50 = 15th (index 14), p95 = 29th.
        assert_eq!(report.statistics_ns.p50, as_float_u128(expected_ns[14]));
        assert_eq!(report.statistics_ns.p95, as_float_u128(expected_ns[28]));

        // The raw report must survive a round trip: that is what makes the committed sha256 an
        // audit anchor rather than a promise.
        let json = serde_json::to_string(&report).expect("serializes");
        let back: ProfileReport = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, report);
    }

    #[test]
    fn empty_sample_set_fails_closed() {
        let none: &[(u128, usize)] = &[];
        let report = ProfileReport::assemble("t", "f".into(), 1, none, None);
        assert_eq!(report.workload.semantic_gate, SemanticGate::Fail);
        assert!(!report.variance.accepted);
        assert_eq!(report.total_units, 0);
    }

    /// Diagnostic (env-gated): how does each W1 seed's game end under the safety bounds?
    /// `RunError::StepLimit` = hit the bound without completing; Ok(finished) = completed.
    #[test]
    fn w1_ending_diagnostic() {
        if std::env::var_os("TI4_M09_019B_PROFILE").is_none() {
            eprintln!("skipping W1 ending diagnostic (set TI4_M09_019B_PROFILE=1 to run)");
            return;
        }
        let root = repo_root();
        let pool_path = root.join(POOL_PATH);
        let pool = MapPool::load(&pool_path).expect("pool loads");
        let content = ContentStore::embedded();
        for i in 0..WARMUP_ITERATIONS + TIMED_SAMPLES {
            let seed = W1_SEED_BASE + i as u64;
            let mut game = build_game(content, DEFAULT, &pool, seed).expect("builds");
            match game.run(W1_ROUND_CAP, W1_HORIZON_STEPS) {
                Err(ti4_engine::game::RunError::StepLimit {
                    max_steps, round, ..
                }) => println!(
                    "seed {seed}: StepLimit at budget {max_steps} (round {round}) — full shape"
                ),
                Err(error) => println!("seed {seed}: engine error: {error}"),
                Ok(state) => println!(
                    "seed {seed}: ended early — finished={} round={}",
                    state.finished, state.round
                ),
            }
        }
    }

    /// The fixture capture is deterministic: two independent captures from the same inputs find
    /// the same position. (Runs against the real pool; cheap — one replay each.)
    #[test]
    fn fixture_capture_is_deterministic() {
        let content = ContentStore::embedded();
        let sources = DEFAULT;
        let pool_path = repo_root().join(POOL_PATH);
        let bytes = fs::read(&pool_path).expect("pool exists in this checkout");
        assert!(hex(Sha256::digest(&bytes)).starts_with(POOL_SHA_PREFIX));
        let pool = MapPool::load(&pool_path).expect("pool loads");
        let a = capture_fixture(content, sources, &pool).expect("fixture reachable");
        let b = capture_fixture(content, sources, &pool).expect("fixture reachable");
        assert_eq!(a.step_index, b.step_index);
        assert_eq!(a.option_ids, b.option_ids);
        assert!(a.option_ids.len() >= MIN_FIXTURE_OPTIONS);
    }
}
