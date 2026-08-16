//! Production Stage-2 policy-gradient runner.
//!
//! Starts from blank profiles unless `--checkpoint` supplies a Stage-1 bootstrap. Stage 2 uses a
//! four-round horizon by default, configurable with `--rounds`, and the Stage-2 VP/objective reward.
//! Factions rotate through every physical seat on a shared varied map for each seed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ti4_content::ContentStore;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::archive::{Archive, Checkpoint};
use ti4_training::reward::Stage;
use ti4_training::rollout::{
    Horizon, play_rotated_batch_evaluation, play_rotated_save54_pool_batch_evaluation,
};
use ti4_training::stage1::{FactionPlan, FactionStart, train_factions};

fn number(name: &str, fallback: usize) -> usize {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn optional_number(name: &str) -> Option<usize> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
}

fn flag(name: &str) -> bool {
    std::env::args().any(|argument| argument == name)
}

/// The first seed of boundary `index`'s panel under per-boundary stepping.
///
/// With a step of zero every boundary re-measures the same fixed panel, which keeps old runs
/// comparable; with a positive step each boundary's block starts further along so adjacent
/// panels are disjoint and their gain estimates are statistically independent. A stepped panel
/// shares no source seeds with any earlier measurement, so paired evidence is only valid when
/// every table compared at that boundary (candidate **and** incumbent) was measured on the same
/// fresh block — see the champion re-measurement in the training loop.
fn first_seed_for_boundary(base: u64, step: u64, index: usize) -> u64 {
    base.wrapping_add(step.wrapping_mul(u64::try_from(index).unwrap_or(0)))
}

fn decimal(name: &str, fallback: f64) -> f64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn path_argument(name: &str) -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
}

fn same_existing_file(left: &Path, right: &Path) -> bool {
    std::fs::canonicalize(left)
        .and_then(|left| std::fs::canonicalize(right).map(|right| left == right))
        .unwrap_or_else(|_| left == right)
}

fn blank(factions: &[FactionId]) -> BTreeMap<FactionId, Profile> {
    factions
        .iter()
        .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
        .collect()
}

struct StartState {
    profiles: BTreeMap<FactionId, Profile>,
    accepted: BTreeMap<FactionId, Profile>,
    update: usize,
    history: Vec<serde_json::Value>,
    telemetry: Vec<serde_json::Value>,
    provenance: Option<String>,
    rounds: Option<u32>,
}

fn load_start(path: &Path, factions: &[FactionId]) -> Result<StartState, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let checksum = format!("{:x}", Sha256::digest(&bytes));
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse checkpoint: {error}"))?;
    let update = document
        .get("final_update")
        .or_else(|| document.get("update"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let table = document
        .get("learner_profiles")
        .or_else(|| document.get("profiles"))
        .or_else(|| document.get("accepted"))
        .unwrap_or(&document);
    let loaded: BTreeMap<String, Profile> = serde_json::from_value(table.clone())
        .map_err(|error| format!("read profile table: {error}"))?;
    let mut profiles = blank(factions);
    for faction in factions {
        if let Some(profile) = loaded.get(faction.as_str()) {
            profile
                .validate(Some(faction.as_str()))
                .map_err(|error| format!("{faction}: {error}"))?;
            if !profile.is_explicit() {
                return Err(format!(
                    "{faction}: schema {} is hashed; Stage 2 requires explicit profiles",
                    profile.schema
                ));
            }
            profiles.insert(faction.clone(), profile.clone());
        }
    }
    let mut accepted = profiles.clone();
    let accepted_table = document.get("accepted").or_else(|| {
        document
            .get("learner_profiles")
            .and_then(|_| document.get("profiles"))
    });
    if let Some(table) = accepted_table
        && let Ok(loaded) = serde_json::from_value::<BTreeMap<String, Profile>>(table.clone())
    {
        for faction in factions {
            if let Some(profile) = loaded.get(faction.as_str()) {
                profile
                    .validate(Some(faction.as_str()))
                    .map_err(|error| format!("accepted {faction}: {error}"))?;
                accepted.insert(faction.clone(), profile.clone());
            }
        }
    }
    let is_stage_two = document
        .get("stage")
        .is_some_and(|stage| stage == "Two" || stage == 2);
    let history = if is_stage_two {
        document
            .get("history")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let telemetry = if is_stage_two {
        document
            .get("training_telemetry")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let rounds = is_stage_two.then(|| {
        document
            .get("horizon")
            .and_then(|horizon| horizon.get("rounds"))
            .and_then(serde_json::Value::as_u64)
            .or_else(|| {
                document
                    .get("arguments")
                    .and_then(|arguments| arguments.get("rounds"))
                    .and_then(serde_json::Value::as_str)
                    .and_then(|rounds| rounds.parse().ok())
            })
            .and_then(|rounds| u32::try_from(rounds).ok())
            .unwrap_or(4)
    });
    Ok(StartState {
        profiles,
        accepted,
        update,
        history,
        telemetry,
        provenance: Some(format!("{}#sha256={checksum}", path.display())),
        rounds,
    })
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Metrics {
    games: usize,
    clearance: f64,
    victory_points: f64,
    /// Standard deviation of victory points across the panel's games.
    ///
    /// Carried because a mean without its spread cannot say whether a difference is real. The
    /// promotion gate divides by this; without it the gate compares a gain against a fixed
    /// threshold that may sit either side of the panel's own measurement error, and nobody can
    /// tell which.
    #[serde(default)]
    victory_points_stdev: f64,
    vp_margin: f64,
    won_or_tied: f64,
    scoreable: f64,
    planets: f64,
    systems: f64,
    units: f64,
    shortfall: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Evaluation {
    update: usize,
    /// First seed of this boundary's validation panel. `None` in checkpoints written before
    /// per-boundary stepping existed (all those boundaries used the run's fixed base seed).
    #[serde(default)]
    validation_first_seed: Option<u64>,
    elapsed_seconds: f64,
    candidate_metrics: BTreeMap<FactionId, Metrics>,
    accepted_metrics: BTreeMap<FactionId, Metrics>,
    confirmation_metrics: Option<BTreeMap<FactionId, Metrics>>,
    accepted: Vec<FactionId>,
    accepted_kind: Option<String>,
    validation_gain: GainEvidence,
    confirmation_gain: Option<GainEvidence>,
}

#[derive(Debug, Clone)]
struct PanelEvaluation {
    metrics: BTreeMap<FactionId, Metrics>,
    /// Sum of faction VP, averaged over all physical-seat rotations for each source seed.
    /// Keeping the source seed as the sample preserves the correlation shared by its rotations.
    table_vp_by_seed: BTreeMap<u64, f64>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct GainEvidence {
    gain: f64,
    standard_error: f64,
    samples: usize,
}

impl GainEvidence {
    fn paired(candidate: &PanelEvaluation, champion: &PanelEvaluation) -> Self {
        let differences: Vec<f64> = candidate
            .table_vp_by_seed
            .iter()
            .filter_map(|(seed, candidate)| {
                champion
                    .table_vp_by_seed
                    .get(seed)
                    .map(|champion| candidate - champion)
            })
            .collect();
        if differences.is_empty() {
            return Self::default();
        }
        let count = f64::from(u32::try_from(differences.len()).unwrap_or(u32::MAX));
        let gain = differences.iter().sum::<f64>() / count;
        let variance = if differences.len() < 2 {
            0.0
        } else {
            differences
                .iter()
                .map(|difference| (difference - gain).powi(2))
                .sum::<f64>()
                / (count - 1.0)
        };
        Self {
            gain,
            standard_error: variance.sqrt() / count.sqrt(),
            samples: differences.len(),
        }
    }

    fn beyond_noise(self, sigmas: f64) -> bool {
        if sigmas <= 0.0 {
            return true;
        }
        self.samples >= 2 && self.gain > sigmas * self.standard_error
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct FactionLearning {
    decisions: usize,
    movement: f64,
    mean_return_std: f64,
    max_return_std: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LearningBlock {
    from_update: usize,
    to_update: usize,
    updates: usize,
    elapsed_seconds: f64,
    decisions: usize,
    errors: usize,
    zero_movement_updates: usize,
    factions: BTreeMap<String, FactionLearning>,
}

fn learning_block(
    from_update: usize,
    elapsed_seconds: f64,
    generations: &[ti4_training::stage1::Generation],
) -> LearningBlock {
    #[derive(Default)]
    struct Accumulator {
        row: FactionLearning,
        return_observations: usize,
    }

    let mut factions: BTreeMap<String, Accumulator> = BTreeMap::new();
    for generation in generations {
        for (faction, heads) in &generation.telemetry {
            let row = factions.entry(faction.clone()).or_default();
            for telemetry in heads.values() {
                row.row.decisions += telemetry.actions;
                row.row.movement += telemetry.update_norm;
                row.row.mean_return_std += telemetry.return_std;
                row.row.max_return_std = row.row.max_return_std.max(telemetry.return_std);
                row.return_observations += 1;
            }
        }
    }
    let factions = factions
        .into_iter()
        .map(|(faction, mut accumulated)| {
            if accumulated.return_observations > 0 {
                let observations =
                    f64::from(u32::try_from(accumulated.return_observations).unwrap_or(u32::MAX));
                accumulated.row.mean_return_std /= observations;
            }
            (faction, accumulated.row)
        })
        .collect();
    LearningBlock {
        from_update,
        to_update: from_update + generations.len(),
        updates: generations.len(),
        elapsed_seconds,
        decisions: generations
            .iter()
            .map(|generation| generation.decisions)
            .sum(),
        errors: generations.iter().map(|generation| generation.errors).sum(),
        zero_movement_updates: generations
            .iter()
            .filter(|generation| generation.movement() <= f64::EPSILON)
            .count(),
        factions,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one accumulator per reported metric, read as the panel's definition"
)]
fn evaluate(
    plan: &FactionPlan,
    profiles: &BTreeMap<FactionId, Profile>,
    first_seed: u64,
    seeds: u64,
) -> Result<PanelEvaluation, String> {
    #[derive(Default)]
    struct Totals {
        games: usize,
        cleared: usize,
        victory_points: i64,
        victory_points_squares: i64,
        vp_margin: i64,
        won_or_tied: usize,
        scoreable: i64,
        planets: i64,
        systems: i64,
        units: i64,
        shortfall: f64,
    }
    let seed_block: Vec<u64> = (first_seed..first_seed + seeds).collect();
    // Evaluation-only rollouts: panels need final progress and opening measurements, not the
    // per-decision trajectories that training retains. Skipping recording keeps this phase from
    // allocating — and then serially freeing — gigabytes of feature vectors between panels.
    let rollouts = plan.map_pool.as_ref().map_or_else(
        || {
            play_rotated_batch_evaluation(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                &seed_block,
                Horizon::rounds(plan.rounds),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
            )
        },
        |pool| {
            play_rotated_save54_pool_batch_evaluation(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                &seed_block,
                Horizon::rounds(plan.rounds),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                Arc::clone(pool),
                plan.tile_seed_offset,
            )
        },
    );
    if let Some(error) = rollouts.iter().find_map(|rollout| rollout.error.as_deref()) {
        return Err(format!(
            "evaluation rollout failed; refusing to measure or promote a partial panel: {error}"
        ));
    }
    let mut totals: BTreeMap<FactionId, Totals> = BTreeMap::new();
    let mut seed_totals: BTreeMap<u64, (i64, usize)> = BTreeMap::new();
    for rollout in rollouts.iter().filter(|rollout| rollout.error.is_none()) {
        let table_points = rollout
            .seats
            .iter()
            .map(|seat| seat.episode.final_progress.victory_points)
            .sum::<i64>();
        let seed = seed_totals.entry(rollout.seed).or_default();
        seed.0 += table_points;
        seed.1 += 1;
        for seat in &rollout.seats {
            let progress = seat.episode.final_progress;
            let best_opponent = rollout
                .seats
                .iter()
                .filter(|other| other.faction != seat.faction)
                .map(|other| other.episode.final_progress.victory_points)
                .max()
                .unwrap_or(progress.victory_points);
            let row = totals.entry(seat.faction.clone()).or_default();
            row.games += 1;
            row.cleared += usize::from(seat.episode.cleared);
            row.victory_points += progress.victory_points;
            row.victory_points_squares += progress.victory_points * progress.victory_points;
            row.vp_margin += progress.victory_points - best_opponent;
            row.won_or_tied += usize::from(progress.victory_points >= best_opponent);
            row.scoreable += progress.scoreable_public + progress.scoreable_secret;
            row.planets += progress.planets_gained;
            row.systems += progress.systems;
            row.units += progress.units_gained;
            row.shortfall += seat.episode.shortfall;
        }
    }
    let metrics = totals
        .into_iter()
        .map(|(faction, row)| {
            let n = f64::from(u32::try_from(row.games.max(1)).unwrap_or(u32::MAX));
            (
                faction,
                Metrics {
                    games: row.games,
                    clearance: f64::from(u32::try_from(row.cleared).unwrap_or(u32::MAX)) / n,
                    victory_points: f64::from(
                        i32::try_from(row.victory_points).unwrap_or(i32::MAX),
                    ) / n,
                    victory_points_stdev: {
                        let mean =
                            f64::from(i32::try_from(row.victory_points).unwrap_or(i32::MAX)) / n;
                        let squares = f64::from(
                            i32::try_from(row.victory_points_squares).unwrap_or(i32::MAX),
                        ) / n;
                        (squares - mean * mean).max(0.0).sqrt()
                    },
                    vp_margin: f64::from(i32::try_from(row.vp_margin).unwrap_or(i32::MAX)) / n,
                    won_or_tied: f64::from(u32::try_from(row.won_or_tied).unwrap_or(u32::MAX)) / n,
                    scoreable: f64::from(i32::try_from(row.scoreable).unwrap_or(i32::MAX)) / n,
                    planets: f64::from(i32::try_from(row.planets).unwrap_or(i32::MAX)) / n,
                    systems: f64::from(i32::try_from(row.systems).unwrap_or(i32::MAX)) / n,
                    units: f64::from(i32::try_from(row.units).unwrap_or(i32::MAX)) / n,
                    shortfall: row.shortfall / n,
                },
            )
        })
        .collect();
    let table_vp_by_seed = seed_totals
        .into_iter()
        .map(|(seed, (points, games))| {
            let games = f64::from(u32::try_from(games.max(1)).unwrap_or(u32::MAX));
            (
                seed,
                f64::from(i32::try_from(points).unwrap_or(i32::MAX)) / games,
            )
        })
        .collect();
    Ok(PanelEvaluation {
        metrics,
        table_vp_by_seed,
    })
}

fn report(update: usize, metrics: &BTreeMap<FactionId, Metrics>) {
    println!("\nupdate {update}");
    println!(
        "faction       games  clearance      vp  margin    win  scoreable  planets  systems   units  short"
    );
    println!(
        "------------  -----  ---------  ------  ------  -----  ---------  -------  -------  ------  -----"
    );
    for (faction, row) in metrics {
        println!(
            "{faction:<12}  {:>5}  {:>9.3}  {:>6.2}  {:>+6.2}  {:>5.1}%  {:>9.2}  {:>7.2}  {:>7.2}  {:>6.2}  {:>5.2}",
            row.games,
            row.clearance,
            row.victory_points,
            row.vp_margin,
            100.0 * row.won_or_tied,
            row.scoreable,
            row.planets,
            row.systems,
            row.units,
            row.shortfall
        );
    }
}

fn report_gain(label: &str, evidence: GainEvidence, sigmas: f64) {
    println!(
        "{label}: aggregate gain={:+.3}, paired se={:.3}, detectable@{sigmas:.1}σ={:.3}, source seeds={}",
        evidence.gain,
        evidence.standard_error,
        sigmas * evidence.standard_error,
        evidence.samples
    );
}

/// Whether the candidate table should replace the champion.
///
/// Two bars, and the candidate must clear both. The first is the authored margin, which says how
/// much improvement is worth promoting for. The second is the panel's own measurement error, which
/// says how much improvement this panel could even see.
///
/// Noise is measured from candidate-minus-champion differences on identical source seeds. All
/// physical-seat rotations belonging to one source seed remain one statistical sample. This keeps
/// shared map, deal, and rotation effects paired instead of pretending they are independent.
/// Names every Stage-2 gate clause the candidate table violates, in check order.
///
/// The boolean gate below is unchanged; this exists so a run can say *which* clause refused a
/// boundary instead of only that it was refused. A stall that cannot name its own rejection reason
/// has to be reconstructed from aggregate metrics after the fact.
fn failed_stage_two_clauses(
    candidate: &BTreeMap<FactionId, Metrics>,
    champion: &BTreeMap<FactionId, Metrics>,
    evidence: GainEvidence,
    factions: &[FactionId],
    vp_margin: f64,
    max_faction_vp_regression: f64,
    max_faction_clearance_regression: f64,
    accept_sigmas: f64,
) -> Vec<String> {
    let mut failed = Vec::new();
    for faction in factions {
        if candidate[faction].clearance
            < champion[faction].clearance - max_faction_clearance_regression - 1e-12
        {
            failed.push(format!(
                "clearance veto {}: {} is more than {max_faction_clearance_regression:.4} below the champion's {:.4}",
                faction, candidate[faction].clearance, champion[faction].clearance
            ));
        }
    }
    for faction in factions {
        if candidate[faction].victory_points
            < champion[faction].victory_points - max_faction_vp_regression - 1e-12
        {
            failed.push(format!(
                "VP veto {}: {} is more than {max_faction_vp_regression:.4} below the champion's {:.4}",
                faction, candidate[faction].victory_points, champion[faction].victory_points
            ));
        }
    }
    let gain: f64 = factions
        .iter()
        .map(|faction| candidate[faction].victory_points - champion[faction].victory_points)
        .sum();
    let faction_count = f64::from(u32::try_from(factions.len()).unwrap_or(u32::MAX));
    if gain <= vp_margin * faction_count {
        failed.push(format!(
            "aggregate margin: total VP gain {gain:.4} is not above {} x {:.4}",
            factions.len(),
            vp_margin
        ));
    }
    if !evidence.beyond_noise(accept_sigmas) {
        failed.push(format!(
            "sigma evidence: paired gain {:.4} does not exceed {accept_sigmas:.1} x SE {:.4} over {} seeds",
            evidence.gain, evidence.standard_error, evidence.samples
        ));
    }
    failed
}

fn acceptable_stage_two_table(
    candidate: &BTreeMap<FactionId, Metrics>,
    champion: &BTreeMap<FactionId, Metrics>,
    evidence: GainEvidence,
    factions: &[FactionId],
    vp_margin: f64,
    max_faction_vp_regression: f64,
    max_faction_clearance_regression: f64,
    accept_sigmas: f64,
) -> bool {
    failed_stage_two_clauses(
        candidate,
        champion,
        evidence,
        factions,
        vp_margin,
        max_faction_vp_regression,
        max_faction_clearance_regression,
        accept_sigmas,
    )
    .is_empty()
}

fn report_learning(block: &LearningBlock) {
    println!(
        "learning {}..{}: {:.1}s, {} decisions, {} errors, {} zero-movement updates",
        block.from_update,
        block.to_update,
        block.elapsed_seconds,
        block.decisions,
        block.errors,
        block.zero_movement_updates
    );
    println!("faction       movement  mean-return-sd  max-return-sd  decisions");
    println!("------------  --------  --------------  -------------  ---------");
    for (faction, row) in &block.factions {
        println!(
            "{faction:<12}  {:>8.3}  {:>14.3}  {:>13.3}  {:>9}",
            row.movement, row.mean_return_std, row.max_return_std, row.decisions
        );
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "all persisted experiment state is explicit"
)]
fn checkpoint_document(
    update: usize,
    complete: bool,
    profiles: &BTreeMap<FactionId, Profile>,
    accepted: &BTreeMap<FactionId, Profile>,
    arguments: &BTreeMap<String, String>,
    provenance: Option<&str>,
    history: &[serde_json::Value],
    telemetry: &[serde_json::Value],
    rounds: u32,
) -> Checkpoint {
    let mut checkpoint = Checkpoint::new(
        "rust_stage2_policy_gradient".to_owned(),
        Stage::Two,
        Horizon::rounds(rounds),
        arguments.clone(),
    );
    checkpoint.resumed_from = provenance.map(str::to_owned);
    checkpoint.final_update = update;
    checkpoint.run_complete = complete;
    checkpoint.profiles = profiles
        .iter()
        .map(|(faction, profile)| (faction.to_string(), profile.clone()))
        .collect();
    checkpoint.accepted = accepted
        .iter()
        .map(|(faction, profile)| (faction.to_string(), profile.clone()))
        .collect();
    checkpoint.history = history.to_vec();
    checkpoint.training_telemetry = telemetry.to_vec();
    checkpoint
}

fn save(
    path: &Path,
    update: usize,
    complete: bool,
    profiles: &BTreeMap<FactionId, Profile>,
    accepted: &BTreeMap<FactionId, Profile>,
    arguments: &BTreeMap<String, String>,
    provenance: Option<&str>,
    history: &[serde_json::Value],
    telemetry: &[serde_json::Value],
    rounds: u32,
) -> Result<(), String> {
    let checkpoint = checkpoint_document(
        update, complete, profiles, accepted, arguments, provenance, history, telemetry, rounds,
    );
    Archive::at(
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    )
    .save(&checkpoint, path)
    .map_err(|error| format!("save {}: {error}", path.display()))
}

#[allow(
    clippy::too_many_lines,
    reason = "the executable keeps configuration, reporting, checkpointing and training visible"
)]
fn main() -> Result<(), String> {
    let updates = number("--updates", 1_000);
    let every = number("--every", 25).max(1);
    let train_seeds = u64::try_from(number("--train-seeds", 16)).unwrap_or(16);
    // Seed-stream overrides for differential comparisons against Python checkpoints, which stride
    // their training seeds by 10_000 from an explicit base. Unset keeps this engine's historical
    // stream (base per plan, stride one full batch), so existing runs stay comparable.
    let train_seed_base =
        optional_number("--train-seed-base").and_then(|value| u64::try_from(value).ok());
    let train_seed_stride =
        optional_number("--train-seed-stride").and_then(|value| u64::try_from(value).ok());
    let validation_seeds =
        u64::try_from(number("--validation-seeds", number("--eval-seeds", 32))).unwrap_or(32);
    let confirmation_seeds = u64::try_from(number("--confirmation-seeds", 32)).unwrap_or(32);
    let validation_first_seed =
        u64::try_from(number("--validation-first-seed", 96_000_000)).unwrap_or(96_000_000);
    let confirmation_first_seed =
        u64::try_from(number("--confirmation-first-seed", 97_000_000)).unwrap_or(97_000_000);
    // Per-boundary panel stepping: with a positive step each evaluation boundary starts its seed
    // block `step` further along, so consecutive panels are disjoint and cross-boundary trends use
    // independent samples. At zero (the default) every boundary re-measures the same fixed panel,
    // keeping historical runs comparable.
    let panel_step = u64::try_from(number("--panel-step", 0)).unwrap_or(0);
    let accept_vp_margin = decimal("--accept-vp-margin", 0.05);
    // Two standard errors is the usual bar: roughly "this would happen by chance about one panel
    // in twenty". Zero disables the check and restores the fixed-margin-only gate.
    let accept_sigmas = decimal("--accept-sigmas", 2.0);
    // The reference plan ships the Stage-1 step size; an experiment overrides it here and the value
    // is recorded in the checkpoint arguments so a run is reproducible from its own document.
    let learning_rate = decimal("--learning-rate", 0.03);
    // Same pattern as the learning rate: the reference plan ships the Stage-1 entropy bonus, and
    // an experiment overrides it here; the value is recorded in the checkpoint arguments.
    let entropy = decimal("--entropy", 0.01);
    // Terminal reward bonus for games that finish with at least three victory points. Zero keeps
    // the reference reward exactly; nonzero sharpens credit toward high-scoring games (Stage-2
    // plateau experiments).
    let high_vp_bonus = decimal("--high-vp-bonus", 0.0);
    // Clearance floor: uniform penalty per game whose opening did not clear, credited at the
    // final slot so every decision's return carries it (keeps learned play inside the gate's
    // per-faction clearance band). Zero keeps the reference reward exactly.
    let clearance_weight = decimal("--clearance-weight", 0.0);
    // Overlap consecutive updates' rollouts with the previous update's gradient apply so the
    // worker pool stays saturated across batch boundaries (bounded staleness of one update).
    // Off by default: the sequential reference path is byte-identical to earlier runs.
    let pipeline = flag("--pipeline");
    // Roll out this many consecutive updates' games in one shared parallel wave before applying
    // any of their gradients (bounded staleness of depth-1). One is the sequential reference;
    // larger values keep the pool saturated across update boundaries when game lengths vary.
    let rollout_depth = optional_number("--rollout-depth")
        .and_then(|depth| usize::try_from(depth).ok())
        .unwrap_or(1);
    if rollout_depth == 0 {
        return Err("--rollout-depth must be at least 1".to_owned());
    }
    if pipeline && rollout_depth > 1 {
        return Err(
            "--pipeline and --rollout-depth are alternative schedulers; use one or the other"
                .to_owned(),
        );
    }
    let max_faction_vp_regression = decimal("--max-faction-vp-regression", 0.15);
    let max_faction_clearance_regression = decimal("--max-faction-clearance-regression", 0.03);
    let checkpoint_path = path_argument("--checkpoint");
    let map_pool_path = path_argument("--map-pool");
    let output = path_argument("--out");
    if let (Some(checkpoint), Some(output)) = (&checkpoint_path, &output)
        && same_existing_file(checkpoint, output)
    {
        return Err(format!(
            "--checkpoint and --out resolve to the same file ({}); use a distinct output so the bootstrap remains immutable",
            checkpoint.display()
        ));
    }

    let requested_rounds =
        optional_number("--rounds").and_then(|rounds| u32::try_from(rounds).ok());
    let mut plan = FactionPlan::stage_two_reference();
    plan.step.learning_rate = learning_rate;
    plan.step.entropy = entropy;
    plan.high_vp_bonus = high_vp_bonus;
    plan.clearance_weight = clearance_weight;
    plan.pipeline = pipeline;
    plan.rollout_depth = rollout_depth;
    plan.train_seeds = train_seeds;
    if let Some(base) = train_seed_base {
        plan.seed = base;
    }
    plan.train_seed_stride = train_seed_stride.unwrap_or(plan.train_seeds);
    if let Some(path) = &map_pool_path {
        let pool = ti4_sim::MapPool::load(path)
            .map_err(|error| format!("load {}: {error}", path.display()))?;
        pool.validate_systems(ContentStore::embedded(), plan.sources)
            .map_err(|error| format!("validate {}: {error}", path.display()))?;
        if pool.home_slots() != plan.factions.len() {
            return Err(format!(
                "{} has {} home slots; Stage 2 has {} factions",
                path.display(),
                pool.home_slots(),
                plan.factions.len()
            ));
        }
        plan.map_pool = Some(Arc::new(pool));
    }
    let StartState {
        mut profiles,
        mut accepted,
        update: starting_update,
        mut history,
        telemetry: mut training_telemetry,
        provenance,
        rounds: resumed_rounds,
    } = checkpoint_path.as_deref().map_or_else(
        || {
            Ok(StartState {
                profiles: blank(&plan.factions),
                accepted: blank(&plan.factions),
                update: 0,
                history: Vec::new(),
                telemetry: Vec::new(),
                provenance: None,
                rounds: None,
            })
        },
        |path| load_start(path, &plan.factions),
    )?;
    plan.rounds = requested_rounds.or(resumed_rounds).unwrap_or(4);
    if plan.rounds < 2 {
        return Err("--rounds must be at least 2 for the Stage-2 VP reward".to_owned());
    }
    let mut arguments = BTreeMap::from([
        ("updates".to_owned(), updates.to_string()),
        ("every".to_owned(), every.to_string()),
        ("train_seeds".to_owned(), train_seeds.to_string()),
        ("validation_seeds".to_owned(), validation_seeds.to_string()),
        (
            "confirmation_seeds".to_owned(),
            confirmation_seeds.to_string(),
        ),
        ("panel_step".to_owned(), panel_step.to_string()),
        ("accept_vp_margin".to_owned(), accept_vp_margin.to_string()),
        ("accept_sigmas".to_owned(), accept_sigmas.to_string()),
        ("learning_rate".to_owned(), learning_rate.to_string()),
        ("entropy".to_owned(), entropy.to_string()),
        ("high_vp_bonus".to_owned(), high_vp_bonus.to_string()),
        ("clearance_weight".to_owned(), clearance_weight.to_string()),
        ("pipeline".to_owned(), pipeline.to_string()),
        ("rollout_depth".to_owned(), rollout_depth.to_string()),
        ("rounds".to_owned(), plan.rounds.to_string()),
        (
            "max_faction_vp_regression".to_owned(),
            max_faction_vp_regression.to_string(),
        ),
        (
            "max_faction_clearance_regression".to_owned(),
            max_faction_clearance_regression.to_string(),
        ),
        (
            "factions".to_owned(),
            "sol,letnev,xxcha,hacan,jolnar,l1z1x".to_owned(),
        ),
    ]);
    if let Some(path) = &checkpoint_path {
        arguments.insert("checkpoint".to_owned(), path.display().to_string());
    }
    if let Some(source) = &provenance {
        arguments.insert("bootstrap_provenance".to_owned(), source.clone());
    }
    if let Some(path) = &map_pool_path {
        arguments.insert("map_pool".to_owned(), path.display().to_string());
    }
    arguments.insert("train_seed_base".to_owned(), plan.seed.to_string());
    arguments.insert(
        "train_seed_stride".to_owned(),
        plan.train_seed_stride.to_string(),
    );

    println!("Stage-2 policy-gradient configuration");
    println!("  factions: sol,letnev,xxcha,hacan,jolnar,l1z1x");
    println!(
        "  stage: VP/objective reward, {}-round horizon",
        plan.rounds
    );
    println!(
        "  batch: {train_seeds} seeds x 6 rotations (training seed base {} stride {})",
        plan.seed, plan.train_seed_stride
    );
    println!(
        "  maps: {} and shared across rotations",
        map_pool_path.as_ref().map_or_else(
            || "Rust varied-map generator".to_owned(),
            |path| format!("Python-compatible pool {}", path.display())
        )
    );
    println!("  profiles: immutable shared schema-4 heads");
    println!(
        "  step: learning rate {learning_rate:.4} (entropy {}, clip {})",
        plan.step.entropy, plan.step.gradient_clip
    );
    if high_vp_bonus > 0.0 {
        println!("  reward: +{high_vp_bonus:.2} terminal bonus for finishing with >=3 VP");
    }
    if clearance_weight > 0.0 {
        println!(
            "  reward: -{clearance_weight:.2} per uncleared opening (full-game cost, final slot)"
        );
    }
    if pipeline {
        println!(
            "  scheduling: pipelined rollouts (staleness 1, pool kept busy across update boundaries)"
        );
    }
    if rollout_depth > 1 {
        println!(
            "  scheduling: rollout waves of {rollout_depth} updates (staleness {}, pool kept busy across update boundaries)",
            rollout_depth - 1
        );
    }
    println!("  meta teacher: none (no specified or validated artifact)");
    println!(
        "  promotion: {validation_seeds} validation + {confirmation_seeds} confirmation seeds; aggregate VP margin {accept_vp_margin:.2}; paired evidence {accept_sigmas:.1}σ; per-faction veto VP {max_faction_vp_regression:.2}, clearance {max_faction_clearance_regression:.2}"
    );
    if panel_step == 0 {
        println!("  panels: one fixed validation/confirmation panel at every boundary");
    } else {
        println!(
            "  panels: fresh per boundary (seed step {panel_step}); adjacent boundaries measure disjoint games; incumbent re-measured on each fresh panel to keep paired comparisons valid"
        );
    }
    println!("  execution: persistent Rayon pool + worker-side gradient statistics");
    println!(
        "  start: {}",
        checkpoint_path.as_ref().map_or("blank".to_owned(), |path| {
            format!("{} at update {starting_update}", path.display())
        })
    );

    let initial_candidate = evaluate(&plan, &profiles, validation_first_seed, validation_seeds)?;
    let mut accepted_panel = evaluate(&plan, &accepted, validation_first_seed, validation_seeds)?;
    let mut accepted_confirmation_panel = evaluate(
        &plan,
        &accepted,
        confirmation_first_seed,
        confirmation_seeds,
    )?;
    let initial_gain = GainEvidence::paired(&initial_candidate, &accepted_panel);
    report(starting_update, &initial_candidate.metrics);
    report_gain("bootstrap comparison", initial_gain, accept_sigmas);
    if flag("--eval-only") {
        println!(
            "\nevaluation only: {validation_seeds} validation seeds x 6 rotations (first seed {validation_first_seed}), no training"
        );
        println!("faction       games   cand_vp  acc_vp     d_vp   cand_clr acc_clr    d_clr");
        for faction in &plan.factions {
            let candidate = initial_candidate.metrics[faction];
            let accepted = accepted_panel.metrics[faction];
            println!(
                "{:12} {:>5}  {:>7.3}  {:>6.3}  {:+8.3}  {:>9.4} {:>7.4}  {:+8.4}",
                faction,
                candidate.games,
                candidate.victory_points,
                accepted.victory_points,
                candidate.victory_points - accepted.victory_points,
                candidate.clearance,
                accepted.clearance,
                candidate.clearance - accepted.clearance,
            );
        }
        report_gain("paired evidence", initial_gain, accept_sigmas);
        let failed = failed_stage_two_clauses(
            &initial_candidate.metrics,
            &accepted_panel.metrics,
            initial_gain,
            &plan.factions,
            accept_vp_margin,
            max_faction_vp_regression,
            max_faction_clearance_regression,
            accept_sigmas,
        );
        println!(
            "gate: {}",
            if failed.is_empty() {
                "PASS".to_owned()
            } else {
                format!("FAIL — {}", failed.join("; "))
            }
        );
        if let Some(path) = path_argument("--eval-out") {
            let report = serde_json::json!({
                "update": starting_update,
                "validation_seeds": validation_seeds,
                "validation_first_seed": validation_first_seed,
                "candidate_metrics": initial_candidate.metrics,
                "accepted_metrics": accepted_panel.metrics,
                "validation_gain": initial_gain,
                "failed_clauses": failed,
            });
            std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap())
                .map_err(|error| format!("write {}: {error}", path.display()))?;
            println!("wrote {}", path.display());
        }
        return Ok(());
    }
    history.push(
        serde_json::to_value(Evaluation {
            update: starting_update,
            validation_first_seed: Some(validation_first_seed),
            elapsed_seconds: 0.0,
            candidate_metrics: initial_candidate.metrics,
            accepted_metrics: accepted_panel.metrics.clone(),
            confirmation_metrics: Some(accepted_confirmation_panel.metrics.clone()),
            accepted: Vec::new(),
            accepted_kind: Some("bootstrap".to_owned()),
            validation_gain: initial_gain,
            confirmation_gain: None,
        })
        .map_err(|error| format!("serialize initial evaluation: {error}"))?,
    );
    // The bootstrap comparison above is boundary 0, measured on the base seed.
    let mut boundary_index = 1usize;
    let started = std::time::Instant::now();
    let mut done = 0usize;
    while done < updates {
        let count = every.min(updates - done);
        let block_started = std::time::Instant::now();
        let block_from = starting_update + done;
        plan.generations = count;
        plan.start = Some(FactionStart {
            profiles,
            generation: starting_update + done,
        });
        let run = train_factions(ContentStore::embedded(), &plan);
        if run
            .generations
            .iter()
            .any(|generation| generation.errors > 0)
        {
            return Err("a Stage-2 rollout failed; refusing to continue".to_owned());
        }
        let learning = learning_block(
            block_from,
            block_started.elapsed().as_secs_f64(),
            &run.generations,
        );
        report_learning(&learning);
        training_telemetry.push(
            serde_json::to_value(&learning)
                .map_err(|error| format!("serialize learning telemetry: {error}"))?,
        );
        profiles = run.profiles;
        done += count;
        let update = starting_update + done;
        // Boundary panels: the fixed historical panel when stepping is off, otherwise a fresh
        // disjoint block per boundary so cross-boundary trends use independent samples.
        let panel_validation_seed =
            first_seed_for_boundary(validation_first_seed, panel_step, boundary_index);
        let panel_confirmation_seed =
            first_seed_for_boundary(confirmation_first_seed, panel_step, boundary_index);
        boundary_index += 1;
        if panel_step > 0 {
            println!(
                "boundary {update}: fresh panels (validation first seed {panel_validation_seed}, confirmation first seed {panel_confirmation_seed})"
            );
            // Pairing is per source seed: a stepped candidate panel shares no seeds with the
            // champion's last measurement, so without this re-measurement every paired gain at
            // this boundary degenerates to zero samples. The oracle gets comparability for free
            // from its fixed per-run panels; stepping pays one extra incumbent evaluation per
            // boundary instead.
            accepted_panel = evaluate(&plan, &accepted, panel_validation_seed, validation_seeds)?;
            accepted_confirmation_panel = evaluate(
                &plan,
                &accepted,
                panel_confirmation_seed,
                confirmation_seeds,
            )?;
        }
        let candidate_panel = evaluate(&plan, &profiles, panel_validation_seed, validation_seeds)?;
        let validation_gain = GainEvidence::paired(&candidate_panel, &accepted_panel);
        report(update, &candidate_panel.metrics);
        report_gain("validation", validation_gain, accept_sigmas);
        let mut promoted = Vec::new();
        let mut accepted_kind = None;
        let mut assembled_confirmation = None;
        let mut assembled_confirmation_gain = None;
        // The clauses that refuse this boundary are named here so the checkpoint's history and the
        // console show *why* a candidate was rejected, not only that it was.
        let mut rejected_by: Vec<String> = failed_stage_two_clauses(
            &candidate_panel.metrics,
            &accepted_panel.metrics,
            validation_gain,
            &plan.factions,
            accept_vp_margin,
            max_faction_vp_regression,
            max_faction_clearance_regression,
            accept_sigmas,
        );
        if rejected_by.is_empty() {
            let confirmation = evaluate(
                &plan,
                &profiles,
                panel_confirmation_seed,
                confirmation_seeds,
            )?;
            let confirmation_gain =
                GainEvidence::paired(&confirmation, &accepted_confirmation_panel);
            report_gain("confirmation", confirmation_gain, accept_sigmas);
            let confirmation_failure = failed_stage_two_clauses(
                &confirmation.metrics,
                &accepted_confirmation_panel.metrics,
                confirmation_gain,
                &plan.factions,
                accept_vp_margin,
                max_faction_vp_regression,
                max_faction_clearance_regression,
                accept_sigmas,
            );
            let confirmed = confirmation_failure.is_empty();
            assembled_confirmation = Some(confirmation.metrics.clone());
            assembled_confirmation_gain = Some(confirmation_gain);
            if confirmed {
                accepted.clone_from(&profiles);
                accepted_panel.clone_from(&candidate_panel);
                accepted_confirmation_panel = confirmation;
                promoted.clone_from(&plan.factions);
                accepted_kind = Some("assembled".to_owned());
            } else {
                rejected_by = confirmation_failure;
            }
        }
        if promoted.is_empty() {
            for faction in &plan.factions {
                let mut isolated = accepted.clone();
                isolated.insert(faction.clone(), profiles[faction].clone());
                let primary = evaluate(&plan, &isolated, panel_validation_seed, validation_seeds)?;
                let primary_gain = GainEvidence::paired(&primary, &accepted_panel);
                let faction_improved = primary.metrics[faction].victory_points
                    > accepted_panel.metrics[faction].victory_points + 1e-12;
                if !faction_improved
                    || !acceptable_stage_two_table(
                        &primary.metrics,
                        &accepted_panel.metrics,
                        primary_gain,
                        &plan.factions,
                        accept_vp_margin,
                        max_faction_vp_regression,
                        max_faction_clearance_regression,
                        accept_sigmas,
                    )
                {
                    continue;
                }
                let confirmation = evaluate(
                    &plan,
                    &isolated,
                    panel_confirmation_seed,
                    confirmation_seeds,
                )?;
                let confirmation_gain =
                    GainEvidence::paired(&confirmation, &accepted_confirmation_panel);
                if acceptable_stage_two_table(
                    &confirmation.metrics,
                    &accepted_confirmation_panel.metrics,
                    confirmation_gain,
                    &plan.factions,
                    accept_vp_margin,
                    max_faction_vp_regression,
                    max_faction_clearance_regression,
                    accept_sigmas,
                ) {
                    accepted = isolated;
                    accepted_panel = primary;
                    accepted_confirmation_panel = confirmation;
                    promoted.push(faction.clone());
                }
            }
            if !promoted.is_empty() {
                accepted_kind = Some("isolated".to_owned());
            }
        }
        let promoted_label = promoted
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "promotion: {} ({})",
            if promoted_label.is_empty() {
                "none"
            } else {
                &promoted_label
            },
            accepted_kind.as_deref().unwrap_or("rejected")
        );
        if promoted.is_empty() {
            for reason in &rejected_by {
                println!("  rejected by gate clause: {reason}");
            }
        }
        history.push(
            serde_json::to_value(Evaluation {
                update,
                validation_first_seed: Some(panel_validation_seed),
                elapsed_seconds: started.elapsed().as_secs_f64(),
                candidate_metrics: candidate_panel.metrics,
                accepted_metrics: accepted_panel.metrics.clone(),
                confirmation_metrics: assembled_confirmation,
                accepted: promoted,
                accepted_kind,
                validation_gain,
                confirmation_gain: assembled_confirmation_gain,
            })
            .map_err(|error| format!("serialize evaluation: {error}"))?,
        );
        if let Some(path) = &output {
            save(
                path,
                update,
                done == updates,
                &profiles,
                &accepted,
                &arguments,
                provenance.as_deref(),
                &history,
                &training_telemetry,
                plan.rounds,
            )?;
            println!("checkpointed {} at update {update}", path.display());
        }
    }
    println!(
        "\n{updates} updates in {:.1}s ({:.3}s/update)",
        started.elapsed().as_secs_f64(),
        started.elapsed().as_secs_f64()
            / f64::from(u32::try_from(updates.max(1)).unwrap_or(u32::MAX))
    );
    Ok(())
}

#[cfg(test)]
mod tests {

    /// Metrics for a faction at a given mean VP and spread over `games` games.
    fn metrics_at(victory_points: f64, stdev: f64, games: usize) -> Metrics {
        Metrics {
            games,
            clearance: 0.8,
            victory_points,
            victory_points_stdev: stdev,
            vp_margin: -1.0,
            won_or_tied: 0.3,
            scoreable: 0.3,
            planets: 4.0,
            systems: 4.0,
            units: 6.0,
            shortfall: 0.3,
        }
    }

    fn table(values: &[(&str, f64)], stdev: f64, games: usize) -> BTreeMap<FactionId, Metrics> {
        values
            .iter()
            .map(|(faction, vp)| (FactionId::new(*faction), metrics_at(*vp, stdev, games)))
            .collect()
    }

    fn six() -> Vec<FactionId> {
        ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
            .into_iter()
            .map(FactionId::new)
            .collect()
    }

    #[test]
    fn noise_is_measured_from_paired_source_seed_differences() {
        let panel = |values: &[(u64, f64)]| PanelEvaluation {
            metrics: BTreeMap::new(),
            table_vp_by_seed: values.iter().copied().collect(),
        };
        // Absolute scores vary by 100 points across seeds, but candidate-minus-champion is exactly
        // one on each seed. Pairing correctly removes that shared seed difficulty.
        let candidate = panel(&[(1, 101.0), (2, 201.0), (3, 51.0)]);
        let champion = panel(&[(1, 100.0), (2, 200.0), (3, 50.0)]);
        let evidence = GainEvidence::paired(&candidate, &champion);
        assert!((evidence.gain - 1.0).abs() < 1e-12);
        assert!(evidence.standard_error.abs() < 1e-12);
        assert_eq!(evidence.samples, 3);
        assert!(evidence.beyond_noise(2.0));

        let varied = GainEvidence::paired(&panel(&[(1, 100.0), (2, 202.0), (3, 51.0)]), &champion);
        assert!((varied.gain - 1.0).abs() < 1e-12);
        assert!((varied.standard_error - 1.0 / 3.0_f64.sqrt()).abs() < 1e-12);

        let one_seed = GainEvidence::paired(&panel(&[(1, 11.0)]), &panel(&[(1, 10.0)]));
        assert!(
            !one_seed.beyond_noise(2.0),
            "one source seed is not evidence"
        );
    }

    #[test]
    fn a_gain_smaller_than_the_panels_own_error_is_refused() {
        // A point estimate can clear the authored margin while remaining smaller than two standard
        // errors of the paired source-seed differences. That is not promotion evidence yet.
        let factions = six();
        let names: Vec<(&str, f64)> = factions
            .iter()
            .map(|faction| (faction.as_str(), 2.00))
            .collect();
        let champion = table(&names, 1.6, 192);

        // Every faction up by 0.06: an aggregate gain of 0.36, which clears the fixed 0.30 bar.
        let improved: Vec<(&str, f64)> = factions
            .iter()
            .map(|faction| (faction.as_str(), 2.06))
            .collect();
        let candidate = table(&improved, 1.6, 192);

        assert!(
            acceptable_stage_two_table(
                &candidate,
                &champion,
                GainEvidence {
                    gain: 0.36,
                    standard_error: 0.20,
                    samples: 32
                },
                &factions,
                0.05,
                0.05,
                0.02,
                0.0
            ),
            "the fixed margin alone accepts it"
        );
        assert!(
            !acceptable_stage_two_table(
                &candidate,
                &champion,
                GainEvidence {
                    gain: 0.36,
                    standard_error: 0.20,
                    samples: 32
                },
                &factions,
                0.05,
                0.05,
                0.02,
                2.0
            ),
            "and the noise check refuses it, because this panel cannot see 0.06"
        );
    }

    #[test]
    fn a_gain_the_panel_can_actually_see_is_accepted() {
        // The other half: the check must not simply refuse everything, or it is a stopped clock.
        let factions = six();
        let champion: BTreeMap<FactionId, Metrics> = table(
            &factions
                .iter()
                .map(|f| (f.as_str(), 2.00))
                .collect::<Vec<_>>(),
            1.6,
            192,
        );
        let candidate: BTreeMap<FactionId, Metrics> = table(
            &factions
                .iter()
                .map(|f| (f.as_str(), 2.60))
                .collect::<Vec<_>>(),
            1.6,
            192,
        );
        assert!(acceptable_stage_two_table(
            &candidate,
            &champion,
            GainEvidence {
                gain: 3.60,
                standard_error: 0.20,
                samples: 32
            },
            &factions,
            0.05,
            0.05,
            0.02,
            2.0
        ));
    }

    #[test]
    fn a_larger_panel_can_see_a_smaller_gain() {
        // More independent source seeds lower the paired standard error. The same point estimate
        // can therefore become detectable without weakening the authored gain requirement.
        let factions = six();
        let names: Vec<(&str, f64)> = factions.iter().map(|f| (f.as_str(), 2.00)).collect();
        let better: Vec<(&str, f64)> = factions.iter().map(|f| (f.as_str(), 2.06)).collect();

        assert!(!acceptable_stage_two_table(
            &table(&better, 1.6, 192),
            &table(&names, 1.6, 192),
            GainEvidence {
                gain: 0.36,
                standard_error: 0.20,
                samples: 32
            },
            &factions,
            0.05,
            0.05,
            0.02,
            2.0
        ));
        assert!(acceptable_stage_two_table(
            &table(&better, 1.6, 40_000),
            &table(&names, 1.6, 40_000),
            GainEvidence {
                gain: 0.36,
                standard_error: 0.10,
                samples: 128
            },
            &factions,
            0.05,
            0.05,
            0.02,
            2.0
        ));
    }

    #[test]
    fn a_per_faction_regression_still_vetoes_however_large_the_aggregate_gain() {
        // The noise check is an extra bar, not a replacement. One seat going backwards must still
        // block a promotion that looks good in aggregate.
        let factions = six();
        let champion = table(
            &factions
                .iter()
                .map(|f| (f.as_str(), 2.00))
                .collect::<Vec<_>>(),
            1.6,
            192,
        );
        let mut candidate = table(
            &factions
                .iter()
                .map(|f| (f.as_str(), 3.00))
                .collect::<Vec<_>>(),
            1.6,
            192,
        );
        candidate.insert(FactionId::new("sol"), metrics_at(1.0, 1.6, 192));

        assert!(!acceptable_stage_two_table(
            &candidate,
            &champion,
            GainEvidence {
                gain: 5.0,
                standard_error: 0.10,
                samples: 32
            },
            &factions,
            0.05,
            0.05,
            0.02,
            2.0
        ));
    }

    #[test]
    fn zero_sigmas_restores_the_previous_gate_exactly() {
        // Opt-out rather than imposed: a run that wants the old behaviour can have it, and the
        // difference between the two is then a deliberate setting rather than a code change.
        let factions = six();
        let champion = table(
            &factions
                .iter()
                .map(|f| (f.as_str(), 2.00))
                .collect::<Vec<_>>(),
            1.6,
            192,
        );
        let candidate = table(
            &factions
                .iter()
                .map(|f| (f.as_str(), 2.06))
                .collect::<Vec<_>>(),
            1.6,
            192,
        );
        assert!(acceptable_stage_two_table(
            &candidate,
            &champion,
            GainEvidence::default(),
            &factions,
            0.05,
            0.05,
            0.02,
            0.0
        ));
    }
    use super::*;
    use ti4_training::gradient::Telemetry;
    use ti4_training::stage1::Generation;

    fn metric(victory_points: f64, clearance: f64) -> Metrics {
        Metrics {
            victory_points,
            clearance,
            ..Metrics::default()
        }
    }

    #[test]
    fn stage_two_promotion_requires_gain_and_respects_faction_vetoes() {
        let factions = [FactionId::new("sol"), FactionId::new("letnev")];
        let champion = BTreeMap::from([
            (factions[0].clone(), metric(2.0, 0.9)),
            (factions[1].clone(), metric(2.0, 0.9)),
        ]);
        let good = BTreeMap::from([
            (factions[0].clone(), metric(2.1, 0.9)),
            (factions[1].clone(), metric(2.1, 0.9)),
        ]);
        assert!(acceptable_stage_two_table(
            &good,
            &champion,
            GainEvidence::default(),
            &factions,
            0.05,
            0.15,
            0.03,
            0.0
        ));

        let sacrificed = BTreeMap::from([
            (factions[0].clone(), metric(2.5, 0.9)),
            (factions[1].clone(), metric(1.8, 0.9)),
        ]);
        assert!(!acceptable_stage_two_table(
            &sacrificed,
            &champion,
            GainEvidence::default(),
            &factions,
            0.05,
            0.15,
            0.03,
            0.0
        ));
    }

    #[test]
    fn the_gate_explains_which_clause_vetoes_a_candidate() {
        let factions = six();
        let names: Vec<(&str, f64)> = factions.iter().map(|f| (f.as_str(), 2.0)).collect();
        let champion = table(&names, 1.6, 192);

        // A candidate that fails every clause at once must name all of them: one clearance veto,
        // one VP veto, the aggregate margin, and the sigma evidence.
        let mut candidate = champion.clone();
        candidate.insert(
            FactionId::new("sol"),
            Metrics {
                victory_points: 1.70, // below the 0.15 VP tolerance
                clearance: 0.8,
                ..Metrics::default()
            },
        );
        candidate.insert(
            FactionId::new("letnev"),
            Metrics {
                victory_points: 2.0,
                clearance: 0.70, // below the 0.03 clearance tolerance
                ..Metrics::default()
            },
        );
        let failed = failed_stage_two_clauses(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.1,
                standard_error: 0.4,
                samples: 8,
            },
            &factions,
            0.05,
            0.15,
            0.03,
            2.0,
        );
        assert_eq!(
            failed.len(),
            4,
            "expected four distinct clause failures: {failed:?}"
        );
        assert!(
            failed
                .iter()
                .any(|line| line.starts_with("clearance veto letnev"))
        );
        assert!(failed.iter().any(|line| line.starts_with("VP veto sol")));
        assert!(
            failed
                .iter()
                .any(|line| line.starts_with("aggregate margin"))
        );
        assert!(failed.iter().any(|line| line.starts_with("sigma evidence")));

        // The same table with a clean pair of panels and no sigma requirement explains nothing,
        // which is how the wrapper keeps its boolean behavior.
        let clean: Vec<(&str, f64)> = factions.iter().map(|f| (f.as_str(), 2.06)).collect();
        let improved = table(&clean, 1.6, 192);
        assert!(
            failed_stage_two_clauses(
                &improved,
                &champion,
                GainEvidence::default(),
                &factions,
                0.05,
                0.15,
                0.03,
                0.0
            )
            .is_empty()
        );
    }

    #[test]
    fn learning_block_exposes_nonzero_gradient_signal() {
        let generation = Generation {
            index: 4,
            errors: 0,
            decisions: 12,
            telemetry: BTreeMap::from([(
                "sol".to_owned(),
                BTreeMap::from([(
                    "turn".to_owned(),
                    Telemetry {
                        actions: 12,
                        return_std: 0.75,
                        update_norm: 0.25,
                        ..Telemetry::default()
                    },
                )]),
            )]),
        };
        let block = learning_block(4, 1.5, &[generation]);
        assert_eq!(block.to_update, 5);
        assert_eq!(block.zero_movement_updates, 0);
        assert_eq!(block.factions["sol"].decisions, 12);
        assert!((block.factions["sol"].movement - 0.25).abs() < 1e-12);
        assert!((block.factions["sol"].mean_return_std - 0.75).abs() < 1e-12);
    }

    #[test]
    fn checkpoint_retains_full_history_telemetry_and_separate_champion() {
        let faction = FactionId::new("sol");
        let profiles =
            BTreeMap::from([(faction.clone(), blank_explicit_profile(faction.as_str()))]);
        let accepted = profiles.clone();
        let history = vec![
            serde_json::json!({"update": 0}),
            serde_json::json!({"update": 5}),
        ];
        let telemetry = vec![serde_json::json!({"from_update": 0, "to_update": 5})];
        let checkpoint = checkpoint_document(
            5,
            false,
            &profiles,
            &accepted,
            &BTreeMap::new(),
            Some("bootstrap.json#sha256=abc"),
            &history,
            &telemetry,
            4,
        );
        assert_eq!(checkpoint.history.len(), 2);
        assert_eq!(checkpoint.training_telemetry.len(), 1);
        assert_eq!(checkpoint.profiles.len(), 1);
        assert_eq!(checkpoint.accepted.len(), 1);
        assert_eq!(
            checkpoint.resumed_from.as_deref(),
            Some("bootstrap.json#sha256=abc")
        );
    }

    #[test]
    fn python_stage_two_resume_loads_learner_and_champion_from_their_distinct_fields() {
        let faction = FactionId::new("sol");
        let mut champion = blank_explicit_profile(faction.as_str());
        champion.name = "champion".to_owned();
        let mut learner = champion.clone();
        learner.name = "learner".to_owned();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ti4-stage2-resume-{}-{nonce}.json",
            std::process::id(),
        ));
        let document = serde_json::json!({
            "stage": 2,
            "horizon": {"rounds": 8, "steps": 1_000_000},
            "final_update": 17,
            "profiles": {"sol": champion},
            "learner_profiles": {"sol": learner},
            "history": [{"update": 17}],
        });
        std::fs::write(
            &path,
            serde_json::to_vec(&document).expect("serialize fixture"),
        )
        .expect("write fixture");
        let loaded = load_start(&path, std::slice::from_ref(&faction)).expect("load fixture");
        std::fs::remove_file(&path).expect("remove fixture");
        assert_eq!(loaded.update, 17);
        assert_eq!(loaded.rounds, Some(8));
        assert_eq!(loaded.profiles[&faction].name, "learner");
        assert_eq!(loaded.accepted[&faction].name, "champion");
        assert_eq!(loaded.history.len(), 1);
    }

    #[test]
    fn a_zero_panel_step_keeps_every_boundary_on_the_same_fixed_panel() {
        // Historical behavior: old checkpoints compared every candidate against the same panel,
        // so stepping must stay opt-in or resumed runs would silently change meaning.
        for index in 0..16usize {
            assert_eq!(first_seed_for_boundary(96_000_000, 0, index), 96_000_000);
        }
    }

    #[test]
    fn a_positive_panel_step_gives_adjacent_boundaries_disjoint_seed_blocks() {
        // The point of stepping: each boundary's gain estimate is drawn from fresh games, so a
        // trend across boundaries is not one noisy panel re-read at different weights.
        let firsts: Vec<u64> = (0..8)
            .map(|index| first_seed_for_boundary(96_000_000, 32, index))
            .collect();
        assert_eq!(firsts[0], 96_000_000);
        for pair in firsts.windows(2) {
            assert!(
                pair[1] - pair[0] >= 32,
                "blocks overlap: {} and {}",
                pair[0],
                pair[1]
            );
        }
    }
}
