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
    /// Each tested faction's own paired VP gain against its own champion (validation panel).
    #[serde(default)]
    faction_gains: BTreeMap<FactionId, GainEvidence>,
    /// Each tested faction's own paired clearance gain against its own champion (validation
    /// panel).
    #[serde(default)]
    faction_clearance_gains: BTreeMap<FactionId, GainEvidence>,
    /// Own gains for the factions that passed validation and were confirmed.
    #[serde(default)]
    confirmation_gains: Option<BTreeMap<FactionId, GainEvidence>>,
    /// Merit path ("vp" / "clearance") by which each promoted faction was accepted.
    #[serde(default)]
    promotion_paths: Option<BTreeMap<FactionId, String>>,
}

#[derive(Debug, Clone)]
struct PanelEvaluation {
    metrics: BTreeMap<FactionId, Metrics>,
    /// Each faction's own VP, per source seed (averaged over that seed's rotations). The
    /// per-faction gate pairs on this, so one head is judged against its own champion with the
    /// other five seats held fixed — never through a sum over other factions.
    faction_vp_by_seed: BTreeMap<FactionId, BTreeMap<u64, f64>>,
    /// Each faction's own clearance (cleared rotations / rotations), per source seed. Pairs the
    /// same way as VP so the clearance-merit path of the gate is measured on identical games.
    faction_clearance_by_seed: BTreeMap<FactionId, BTreeMap<u64, f64>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct GainEvidence {
    gain: f64,
    standard_error: f64,
    samples: usize,
}

impl GainEvidence {
    /// Paired gain for one faction's own VP against its own champion, per source seed.
    fn paired_faction(
        candidate: &PanelEvaluation,
        champion: &PanelEvaluation,
        faction: &FactionId,
    ) -> Self {
        match (
            candidate.faction_vp_by_seed.get(faction),
            champion.faction_vp_by_seed.get(faction),
        ) {
            (Some(candidate_table), Some(champion_table)) => {
                Self::pair_tables(candidate_table, champion_table)
            }
            _ => Self::default(),
        }
    }

    /// Paired gain for one faction's own clearance against its own champion, per source seed.
    fn paired_faction_clearance(
        candidate: &PanelEvaluation,
        champion: &PanelEvaluation,
        faction: &FactionId,
    ) -> Self {
        match (
            candidate.faction_clearance_by_seed.get(faction),
            champion.faction_clearance_by_seed.get(faction),
        ) {
            (Some(candidate_table), Some(champion_table)) => {
                Self::pair_tables(candidate_table, champion_table)
            }
            _ => Self::default(),
        }
    }

    fn pair_tables(candidate: &BTreeMap<u64, f64>, champion: &BTreeMap<u64, f64>) -> Self {
        let differences: Vec<f64> = candidate
            .iter()
            .filter_map(|(seed, own)| champion.get(seed).map(|champion| own - champion))
            .collect();
        Self::from_differences(&differences)
    }

    fn from_differences(differences: &[f64]) -> Self {
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
    /// The pooled trust-region reading, when the block was trained with PPO.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<ti4_training::ppo::ClipTelemetry>,
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

    // Pooled over the block's updates. Absent unless the update was PPO, because REINFORCE has
    // no ratio and therefore nothing to clip -- and a reported zero would wrongly read as "the
    // trust region never bound".
    let clipped: Vec<&ti4_training::ppo::ClipTelemetry> = generations
        .iter()
        .filter_map(|generation| generation.clip.as_ref())
        .collect();
    let clip = if clipped.is_empty() {
        None
    } else {
        let count = f64::from(u32::try_from(clipped.len()).unwrap_or(u32::MAX));
        Some(ti4_training::ppo::ClipTelemetry {
            clip_fraction: clipped.iter().map(|row| row.clip_fraction).sum::<f64>() / count,
            kl_mean: clipped.iter().map(|row| row.kl_mean).sum::<f64>() / count,
        })
    };
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
        clip,
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
    // Per faction, per source seed: total VP and rotation count, for own-merit pairing.
    let mut faction_seed_totals: BTreeMap<FactionId, BTreeMap<u64, (i64, usize)>> = BTreeMap::new();
    // Per faction, per source seed: cleared rotations and rotation count, for clearance pairing.
    let mut faction_clearance_totals: BTreeMap<FactionId, BTreeMap<u64, (usize, usize)>> =
        BTreeMap::new();
    for rollout in rollouts.iter().filter(|rollout| rollout.error.is_none()) {
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
            let per_seed = faction_seed_totals
                .entry(seat.faction.clone())
                .or_default()
                .entry(rollout.seed)
                .or_default();
            per_seed.0 += progress.victory_points;
            per_seed.1 += 1;
            let clr_seed = faction_clearance_totals
                .entry(seat.faction.clone())
                .or_default()
                .entry(rollout.seed)
                .or_default();
            clr_seed.0 += usize::from(seat.episode.cleared);
            clr_seed.1 += 1;
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
    let faction_vp_by_seed = faction_seed_totals
        .into_iter()
        .map(|(faction, per_seed)| {
            let table = per_seed
                .into_iter()
                .map(|(seed, (points, games))| {
                    let games = f64::from(u32::try_from(games.max(1)).unwrap_or(u32::MAX));
                    (
                        seed,
                        f64::from(i32::try_from(points).unwrap_or(i32::MAX)) / games,
                    )
                })
                .collect();
            (faction, table)
        })
        .collect();
    let faction_clearance_by_seed = faction_clearance_totals
        .into_iter()
        .map(|(faction, per_seed)| {
            let table = per_seed
                .into_iter()
                .map(|(seed, (cleared, games))| {
                    let games = f64::from(u32::try_from(games.max(1)).unwrap_or(u32::MAX));
                    (
                        seed,
                        f64::from(u32::try_from(cleared).unwrap_or(u32::MAX)) / games,
                    )
                })
                .collect();
            (faction, table)
        })
        .collect();
    Ok(PanelEvaluation {
        metrics,
        faction_vp_by_seed,
        faction_clearance_by_seed,
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

/// Per-faction gate verdict on one panel. Every clause references only the tested faction's own
/// metrics — no sum over other factions, so one head can never block or carry another. A head
/// passes when either merit path clears (on both validation and confirmation panels):
///   VP merit — its own paired VP gain exceeds `vp_margin` and `accept_sigmas` x its own SE,
///     with its own clearance within `max_faction_clearance_regression` of the champion's;
///   Clearance merit — its own paired clearance gain is at least `clearance_gain_bar`
///     ("large enough") and exceeds `accept_sigmas` x its own SE; this path accepts a bounded
///     VP regression up to `max_faction_vp_regression`. A large enough gain in clearance is
///     accepting VP regression. The bar keeps pushing until the champion's own clearance is
///     near 1.0, after which there is no room left for the path to fire.
fn own_verdict(
    candidate: &BTreeMap<FactionId, Metrics>,
    champion: &BTreeMap<FactionId, Metrics>,
    vp_evidence: GainEvidence,
    clr_evidence: GainEvidence,
    faction: &FactionId,
    vp_margin: f64,
    max_faction_clearance_regression: f64,
    clearance_gain_bar: f64,
    max_faction_vp_regression: f64,
    accept_sigmas: f64,
) -> (bool, String) {
    let cand = &candidate[faction];
    let champ = &champion[faction];

    // VP merit path.
    if vp_evidence.gain > vp_margin + 1e-12
        && vp_evidence.beyond_noise(accept_sigmas)
        && cand.clearance >= champ.clearance - max_faction_clearance_regression - 1e-12
    {
        return (
            true,
            format!(
                "VP merit: own gain {:+.4} > margin and {}σ; clearance {:.3}/{:.3}",
                vp_evidence.gain, accept_sigmas, cand.clearance, champ.clearance
            ),
        );
    }
    // Clearance merit path.
    if clr_evidence.gain >= clearance_gain_bar - 1e-12
        && clr_evidence.beyond_noise(accept_sigmas)
        && cand.victory_points >= champ.victory_points - max_faction_vp_regression - 1e-12
    {
        return (
            true,
            format!(
                "clearance merit: own gain {:+.4} >= bar and {}σ; VP {:+.4} within bound {:.3}/{:.3}",
                clr_evidence.gain,
                accept_sigmas,
                vp_evidence.gain,
                cand.victory_points,
                champ.victory_points
            ),
        );
    }

    // Both paths failed: name every clause that refused each of them.
    let mut vp_failed = Vec::new();
    if vp_evidence.gain <= vp_margin + 1e-12 {
        vp_failed.push(format!(
            "own VP margin: gain {:.4} is not above {vp_margin:.4}",
            vp_evidence.gain
        ));
    }
    if !vp_evidence.beyond_noise(accept_sigmas) {
        vp_failed.push(format!(
            "own sigma evidence: paired gain {:.4} does not exceed {accept_sigmas:.1} x SE {:.4} over {} seeds",
            vp_evidence.gain, vp_evidence.standard_error, vp_evidence.samples
        ));
    }
    if cand.clearance < champ.clearance - max_faction_clearance_regression - 1e-12 {
        vp_failed.push(format!(
            "own clearance guard: {:.4} is more than {max_faction_clearance_regression:.4} below the champion's {:.4}",
            cand.clearance, champ.clearance
        ));
    }
    let mut clr_failed = Vec::new();
    if clr_evidence.gain < clearance_gain_bar - 1e-12 {
        clr_failed.push(format!(
            "clearance merit bar: gain {:.4} is below {clearance_gain_bar:.4}",
            clr_evidence.gain
        ));
    }
    if !clr_evidence.beyond_noise(accept_sigmas) {
        clr_failed.push(format!(
            "clearance sigma evidence: paired gain {:.4} does not exceed {accept_sigmas:.1} x SE {:.4} over {} seeds",
            clr_evidence.gain, clr_evidence.standard_error, clr_evidence.samples
        ));
    }
    if cand.victory_points < champ.victory_points - max_faction_vp_regression - 1e-12 {
        clr_failed.push(format!(
            "VP regression bound: {:.4} is more than {max_faction_vp_regression:.4} below the champion's {:.4}",
            cand.victory_points, champ.victory_points
        ));
    }
    (
        false,
        format!(
            "VP merit failed [{}]; clearance merit failed [{}]",
            vp_failed.join("; "),
            clr_failed.join("; ")
        ),
    )
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
    if let Some(clip) = &block.clip {
        // Near zero the epochs are unconstrained and the batch is being used freely; approaching
        // one it has been exhausted and further epochs are spending compute for no gradient.
        println!(
            "  trust region: clip fraction {:.4}, approximate KL {:.5}",
            clip.clip_fraction, clip.kl_mean
        );
    }
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
    // Pure learning mode: no boundary evaluation, no gate, no promotion — only training and
    // per-block telemetry. The champion stays frozen for the whole run.
    let no_boundaries = flag("--no-boundaries");
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
    // Discount on the suffix-sum return. One is the undiscounted rule this trainer has always
    // used; below one, a decision is credited less for what happens far after it.
    let discount = decimal("--discount", 1.0);
    // Centre returns against their round's mean rather than one mean per head, which removes the
    // systematic difference between early decisions and late ones.
    let round_baseline = flag("--round-baseline");
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
    // PPO: take this many clipped-surrogate steps from each retained batch instead of one
    // REINFORCE step. One (the default) leaves the update as REINFORCE and retains nothing, so
    // the reference path is untouched unless the flag is given with a value above one.
    // Draw each seed's cyclic seating order at random rather than always rotating the same one.
    // The fixed rotation leaves draft precedence between any two factions at 16.7%-83.3% and never
    // changes which factions border each other; see `rollout::set_seat_scramble`.
    let scramble_seats = flag("--scramble-seats");
    ti4_training::rollout::set_seat_scramble(scramble_seats);
    let ppo_epochs = optional_number("--ppo-epochs").unwrap_or(1);
    let ppo_clip = decimal("--ppo-clip", 0.2);
    if ppo_epochs > 1 && ppo_clip <= 0.0 {
        return Err("--ppo-clip must be positive; the trust region is what makes reuse safe".to_owned());
    }
    if ppo_epochs > 1 && (pipeline || rollout_depth > 1) {
        return Err(
            "--ppo-epochs cannot be combined with --pipeline or --rollout-depth: both hand a batch              to weights that have since moved, which would make the importance ratio measure the              scheduler rather than the epochs"
                .to_owned(),
        );
    }
    let max_faction_vp_regression = decimal("--max-faction-vp-regression", 0.15);
    let max_faction_clearance_regression = decimal("--max-faction-clearance-regression", 0.03);
    // "Large enough" own clearance gain that accepts a bounded VP regression (clearance merit
    // path). Keeps pushing until the champion's own clearance is near 1.0.
    let clearance_gain_bar = decimal("--clearance-gain-bar", 0.03);
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
    plan.discount = discount;
    plan.round_baseline = round_baseline;
    plan.pipeline = pipeline;
    plan.rollout_depth = rollout_depth;
    if ppo_epochs > 1 {
        plan.ppo = Some(ti4_training::ppo::PpoStep {
            learning_rate: plan.step.learning_rate,
            entropy: plan.step.entropy,
            gradient_clip: plan.step.gradient_clip,
            clip: ppo_clip,
            epochs: ppo_epochs,
        });
    }
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
        ("no_boundaries".to_owned(), no_boundaries.to_string()),
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
        ("discount".to_owned(), discount.to_string()),
        ("round_baseline".to_owned(), round_baseline.to_string()),
        ("ppo_epochs".to_owned(), ppo_epochs.to_string()),
        ("scramble_seats".to_owned(), scramble_seats.to_string()),
        ("ppo_clip".to_owned(), ppo_clip.to_string()),
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
            "clearance_gain_bar".to_owned(),
            clearance_gain_bar.to_string(),
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
    if discount < 1.0 {
        println!("  reward: returns discounted at gamma {discount:.3}");
    }
    if round_baseline {
        println!("  baseline: per (head, round) rather than one mean per head");
    }
    println!(
        "  seating: {}",
        if scramble_seats {
            "per-seed random cyclic order (precedence and neighbours balanced)"
        } else {
            "fixed cyclic rotation -- precedence and neighbours NOT balanced"
        }
    );
    if ppo_epochs > 1 {
        println!(
            "  algorithm: PPO -- {ppo_epochs} clipped-surrogate epochs per retained batch, clip {ppo_clip:.2}"
        );
    } else {
        println!("  algorithm: REINFORCE -- one step per batch, trajectories reduced on the workers");
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
        "  promotion: per-faction own merit — each head vs its own champion with the other five seats fixed; VP path: own gain > {accept_vp_margin:.2} and > {accept_sigmas:.1}σ (own SE), clearance within {max_faction_clearance_regression:.2}; OR clearance path: own clearance gain >= {clearance_gain_bar:.2} and > {accept_sigmas:.1}σ, accepting VP regression up to {max_faction_vp_regression:.2}; validation + confirmation panels ({validation_seeds}+{confirmation_seeds} seeds)"
    );
    if no_boundaries {
        println!(
            "  boundaries: DISABLED (--no-boundaries) — pure learning; no evaluation, no gate, no promotion; champion frozen for the whole run"
        );
    } else if panel_step == 0 {
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
    let accepted_panel = evaluate(&plan, &accepted, validation_first_seed, validation_seeds)?;
    let accepted_confirmation_panel = evaluate(
        &plan,
        &accepted,
        confirmation_first_seed,
        confirmation_seeds,
    )?;
    report(starting_update, &initial_candidate.metrics);
    // Per-faction own gains: each head paired against its own champion on the same games.
    let mut bootstrap_gains: BTreeMap<FactionId, GainEvidence> = BTreeMap::new();
    let mut bootstrap_clearance_gains: BTreeMap<FactionId, GainEvidence> = BTreeMap::new();
    for faction in &plan.factions {
        let gain = GainEvidence::paired_faction(&initial_candidate, &accepted_panel, faction);
        let clr_gain =
            GainEvidence::paired_faction_clearance(&initial_candidate, &accepted_panel, faction);
        println!(
            "  {:12} own_vp {:+8.4} (SE {:.4})  own_clr {:+7.4} (SE {:.4}, n={})",
            faction,
            gain.gain,
            gain.standard_error,
            clr_gain.gain,
            clr_gain.standard_error,
            gain.samples
        );
        bootstrap_gains.insert(faction.clone(), gain);
        bootstrap_clearance_gains.insert(faction.clone(), clr_gain);
    }
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
        println!("gate (per-faction own merit, two paths):");
        for faction in &plan.factions {
            let gain = bootstrap_gains[faction];
            let clr_gain = bootstrap_clearance_gains[faction];
            let (passed, detail) = own_verdict(
                &initial_candidate.metrics,
                &accepted_panel.metrics,
                gain,
                clr_gain,
                faction,
                accept_vp_margin,
                max_faction_clearance_regression,
                clearance_gain_bar,
                max_faction_vp_regression,
                accept_sigmas,
            );
            println!(
                "  {:12} {} — {}",
                faction,
                if passed { "PASS" } else { "FAIL" },
                detail
            );
        }
        if let Some(path) = path_argument("--eval-out") {
            let report = serde_json::json!({
                "update": starting_update,
                "validation_seeds": validation_seeds,
                "validation_first_seed": validation_first_seed,
                "candidate_metrics": initial_candidate.metrics,
                "accepted_metrics": accepted_panel.metrics,
                "faction_gains": bootstrap_gains,
                "faction_clearance_gains": bootstrap_clearance_gains,
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
            faction_gains: bootstrap_gains.clone(),
            faction_clearance_gains: bootstrap_clearance_gains,
            confirmation_gains: None,
            promotion_paths: None,
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
        // --no-boundaries: pure learning mode — skip evaluation and the gate entirely;
        // telemetry above and checkpointing below still run every block.
        if !no_boundaries {
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
            }
            // Per-faction gate: each head is measured against its own champion with the other five
            // seats held at their champion heads, and judged on its own metrics only — no clause ever
            // sums over other factions, so one head can neither block nor carry another. All six are
            // tested against the same pre-boundary champion table so no promotion confounds a
            // sibling's evidence; promotions apply together afterwards. The champion panels are
            // re-measured every boundary (cheap after F15) so they are never stale, whether or not
            // stepping is on.
            let champion_panel =
                evaluate(&plan, &accepted, panel_validation_seed, validation_seeds)?;
            let champion_confirmation_panel = evaluate(
                &plan,
                &accepted,
                panel_confirmation_seed,
                confirmation_seeds,
            )?;
            report(update, &champion_panel.metrics);
            let mut promoted = Vec::new();
            let mut candidate_metrics: BTreeMap<FactionId, Metrics> = BTreeMap::new();
            let mut faction_gains: BTreeMap<FactionId, GainEvidence> = BTreeMap::new();
            let mut faction_clearance_gains: BTreeMap<FactionId, GainEvidence> = BTreeMap::new();
            let mut confirmation_rows: BTreeMap<FactionId, Metrics> = BTreeMap::new();
            let mut confirmation_gains_map: BTreeMap<FactionId, GainEvidence> = BTreeMap::new();
            let mut promotion_paths: BTreeMap<FactionId, String> = BTreeMap::new();
            for faction in &plan.factions {
                let mut isolated = accepted.clone();
                isolated.insert(faction.clone(), profiles[faction].clone());
                let primary = evaluate(&plan, &isolated, panel_validation_seed, validation_seeds)?;
                candidate_metrics.insert(faction.clone(), primary.metrics[faction]);
                let own_gain = GainEvidence::paired_faction(&primary, &champion_panel, faction);
                let own_clr_gain =
                    GainEvidence::paired_faction_clearance(&primary, &champion_panel, faction);
                faction_gains.insert(faction.clone(), own_gain);
                faction_clearance_gains.insert(faction.clone(), own_clr_gain);
                let (passed, detail) = own_verdict(
                    &primary.metrics,
                    &champion_panel.metrics,
                    own_gain,
                    own_clr_gain,
                    faction,
                    accept_vp_margin,
                    max_faction_clearance_regression,
                    clearance_gain_bar,
                    max_faction_vp_regression,
                    accept_sigmas,
                );
                if !passed {
                    println!(
                        "  {:12} cand_vp {:.3} acc_vp {:.3} own_vp {:+.4} (SE {:.4}) own_clr {:+.4} (SE {:.4}, n={}) rejected: {}",
                        faction,
                        primary.metrics[faction].victory_points,
                        champion_panel.metrics[faction].victory_points,
                        own_gain.gain,
                        own_gain.standard_error,
                        own_clr_gain.gain,
                        own_clr_gain.standard_error,
                        own_gain.samples,
                        detail
                    );
                    continue;
                }
                let confirmation = evaluate(
                    &plan,
                    &isolated,
                    panel_confirmation_seed,
                    confirmation_seeds,
                )?;
                let conf_own_gain = GainEvidence::paired_faction(
                    &confirmation,
                    &champion_confirmation_panel,
                    faction,
                );
                let conf_own_clr_gain = GainEvidence::paired_faction_clearance(
                    &confirmation,
                    &champion_confirmation_panel,
                    faction,
                );
                let (conf_passed, conf_detail) = own_verdict(
                    &confirmation.metrics,
                    &champion_confirmation_panel.metrics,
                    conf_own_gain,
                    conf_own_clr_gain,
                    faction,
                    accept_vp_margin,
                    max_faction_clearance_regression,
                    clearance_gain_bar,
                    max_faction_vp_regression,
                    accept_sigmas,
                );
                if !conf_passed {
                    println!(
                        "  {:12} cand_vp {:.3} acc_vp {:.3} own_vp {:+.4} (SE {:.4}) own_clr {:+.4} (SE {:.4}, n={}) confirmation rejected: {}",
                        faction,
                        primary.metrics[faction].victory_points,
                        champion_panel.metrics[faction].victory_points,
                        conf_own_gain.gain,
                        conf_own_gain.standard_error,
                        conf_own_clr_gain.gain,
                        conf_own_clr_gain.standard_error,
                        conf_own_gain.samples,
                        conf_detail
                    );
                    continue;
                }
                println!(
                    "  {:12} cand_vp {:.3} acc_vp {:.3} own_vp {:+.4} (SE {:.4}) own_clr {:+.4} (SE {:.4}, n={}) confirmed: {}",
                    faction,
                    primary.metrics[faction].victory_points,
                    champion_panel.metrics[faction].victory_points,
                    own_gain.gain,
                    own_gain.standard_error,
                    own_clr_gain.gain,
                    own_clr_gain.standard_error,
                    own_gain.samples,
                    detail
                );
                confirmation_rows.insert(faction.clone(), confirmation.metrics[faction]);
                confirmation_gains_map.insert(faction.clone(), conf_own_gain);
                promotion_paths.insert(
                    faction.clone(),
                    if detail.starts_with("VP") {
                        "vp".to_owned()
                    } else {
                        "clearance".to_owned()
                    },
                );
                promoted.push(faction.clone());
            }
            if !promoted.is_empty() {
                for faction in &promoted {
                    accepted.insert(faction.clone(), profiles[faction].clone());
                }
            }
            let accepted_kind = if promoted.is_empty() {
                None
            } else {
                Some("per_faction".to_owned())
            };
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
            history.push(
                serde_json::to_value(Evaluation {
                    update,
                    validation_first_seed: Some(panel_validation_seed),
                    elapsed_seconds: started.elapsed().as_secs_f64(),
                    candidate_metrics,
                    accepted_metrics: champion_panel.metrics.clone(),
                    confirmation_metrics: (!confirmation_rows.is_empty())
                        .then_some(confirmation_rows),
                    accepted: promoted,
                    accepted_kind,
                    faction_gains,
                    faction_clearance_gains,
                    confirmation_gains: (!confirmation_gains_map.is_empty())
                        .then_some(confirmation_gains_map),
                    promotion_paths: (!promotion_paths.is_empty()).then_some(promotion_paths),
                })
                .map_err(|error| format!("serialize evaluation: {error}"))?,
            );
        }
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
    use super::*;
    use ti4_training::gradient::Telemetry;
    use ti4_training::stage1::Generation;

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

    fn six() -> Vec<FactionId> {
        ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
            .into_iter()
            .map(FactionId::new)
            .collect()
    }

    #[test]
    fn own_noise_is_measured_from_paired_source_seed_differences() {
        let sol = FactionId::new("sol");
        let panel = |values: &[(u64, f64)]| PanelEvaluation {
            metrics: BTreeMap::new(),
            faction_vp_by_seed: BTreeMap::from([(sol.clone(), values.iter().copied().collect())]),
            faction_clearance_by_seed: BTreeMap::new(),
        };
        // Absolute scores vary by 100 points across seeds, but candidate-minus-champion is exactly
        // one on each seed. Pairing correctly removes that shared seed difficulty.
        let candidate = panel(&[(1, 101.0), (2, 201.0), (3, 51.0)]);
        let champion = panel(&[(1, 100.0), (2, 200.0), (3, 50.0)]);
        let evidence = GainEvidence::paired_faction(&candidate, &champion, &sol);
        assert!((evidence.gain - 1.0).abs() < 1e-12);
        assert!(evidence.standard_error.abs() < 1e-12);
        assert_eq!(evidence.samples, 3);
        assert!(evidence.beyond_noise(2.0));

        let varied = GainEvidence::paired_faction(
            &panel(&[(1, 100.0), (2, 202.0), (3, 51.0)]),
            &champion,
            &sol,
        );
        assert!((varied.gain - 1.0).abs() < 1e-12);
        assert!((varied.standard_error - 1.0 / 3.0_f64.sqrt()).abs() < 1e-12);

        let one_seed =
            GainEvidence::paired_faction(&panel(&[(1, 11.0)]), &panel(&[(1, 10.0)]), &sol);
        assert!(
            !one_seed.beyond_noise(2.0),
            "one source seed is not evidence"
        );
    }

    /// Test harness for the two-path verdict with fixed gate parameters (VP margin 0.05,
    /// clearance guard 0.03, clearance merit bar 0.03, VP regression bound 0.15). Returns the
    /// failure detail text; empty when the head passes.
    fn verdict_detail(
        candidate: &BTreeMap<FactionId, Metrics>,
        champion: &BTreeMap<FactionId, Metrics>,
        vp_evidence: GainEvidence,
        clr_evidence: GainEvidence,
        faction: &FactionId,
        accept_sigmas: f64,
    ) -> String {
        let (passed, detail) = own_verdict(
            candidate,
            champion,
            vp_evidence,
            clr_evidence,
            faction,
            0.05,
            0.03,
            0.03,
            0.15,
            accept_sigmas,
        );
        if passed { String::new() } else { detail }
    }

    #[test]
    fn vp_merit_requires_margin_and_visible_gain() {
        let sol = FactionId::new("sol");
        let champion = BTreeMap::from([(sol.clone(), metrics_at(2.0, 1.6, 192))]);
        let candidate = BTreeMap::from([(sol.clone(), metrics_at(2.06, 1.6, 192))]);

        // gain 0.06 > margin 0.05 but SE 0.20 makes 2σ = 0.40: refused on sigma evidence.
        let failed = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.06,
                standard_error: 0.20,
                samples: 32,
            },
            GainEvidence::default(),
            &sol,
            2.0,
        );
        assert!(failed.contains("own sigma evidence"), "{failed}");

        // The same point estimate on a panel that can see it passes via VP merit.
        let clean = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.06,
                standard_error: 0.01,
                samples: 32,
            },
            GainEvidence::default(),
            &sol,
            2.0,
        );
        assert!(clean.is_empty(), "{clean}");

        // A gain below the authored margin is refused even when perfectly visible.
        let small = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.04,
                standard_error: 0.01,
                samples: 32,
            },
            GainEvidence::default(),
            &sol,
            2.0,
        );
        assert!(small.contains("own VP margin"), "{small}");
    }

    #[test]
    fn a_large_clearance_gain_accepts_bounded_vp_regression() {
        // The core property of the clearance merit path: a large enough own clearance gain is
        // accepting a bounded VP regression.
        let sol = FactionId::new("sol");
        let champion = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.4,
                clearance: 0.83,
                ..Metrics::default()
            },
        )]);

        // VP down by 0.10 (within the 0.15 bound), clearance up by 0.10 and visible: passes via
        // clearance merit.
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.3,
                clearance: 0.93,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: -0.10,
                standard_error: 0.05,
                samples: 32,
            },
            GainEvidence {
                gain: 0.10,
                standard_error: 0.04,
                samples: 32,
            },
            &sol,
            2.0,
        );
        assert!(detail.is_empty(), "clearance merit should pass: {detail}");

        // The same clearance gain with a VP regression beyond the bound is refused.
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.1,
                clearance: 0.93,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: -0.30,
                standard_error: 0.05,
                samples: 32,
            },
            GainEvidence {
                gain: 0.10,
                standard_error: 0.04,
                samples: 32,
            },
            &sol,
            2.0,
        );
        assert!(detail.contains("VP regression bound"), "{detail}");
    }

    #[test]
    fn clearance_merit_requires_a_large_enough_visible_gain() {
        let sol = FactionId::new("sol");
        let champion = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.4,
                clearance: 0.83,
                ..Metrics::default()
            },
        )]);

        // Clearance up by only 0.01 (below the 0.03 bar) with VP down: neither path passes.
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.3,
                clearance: 0.84,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: -0.10,
                standard_error: 0.05,
                samples: 32,
            },
            GainEvidence {
                gain: 0.01,
                standard_error: 0.01,
                samples: 32,
            },
            &sol,
            2.0,
        );
        assert!(detail.contains("clearance merit bar"), "{detail}");

        // Clearance up by 0.10 but invisible in the panel's own noise: refused on sigma evidence.
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.3,
                clearance: 0.93,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: -0.10,
                standard_error: 0.05,
                samples: 32,
            },
            GainEvidence {
                gain: 0.10,
                standard_error: 0.40,
                samples: 32,
            },
            &sol,
            2.0,
        );
        assert!(detail.contains("clearance sigma evidence"), "{detail}");
    }

    #[test]
    fn vp_merit_blocked_by_own_clearance_guard() {
        // A strong visible VP gain still cannot promote a head whose own clearance falls below
        // the guard — unless the clearance merit path itself fires (it does not here: the
        // clearance gain is negative).
        let sol = FactionId::new("sol");
        let champion = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.0,
                clearance: 0.93,
                ..Metrics::default()
            },
        )]);
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.6,
                clearance: 0.85,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.6,
                standard_error: 0.1,
                samples: 32,
            },
            GainEvidence {
                gain: -0.08,
                standard_error: 0.04,
                samples: 32,
            },
            &sol,
            2.0,
        );
        assert!(detail.contains("own clearance guard"), "{detail}");

        // Within the guard: passes via VP merit.
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.6,
                clearance: 0.91,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.6,
                standard_error: 0.1,
                samples: 32,
            },
            GainEvidence {
                gain: -0.02,
                standard_error: 0.04,
                samples: 32,
            },
            &sol,
            2.0,
        );
        assert!(detail.is_empty(), "{detail}");
    }

    #[test]
    fn one_factions_regression_does_not_block_another_factions_promotion() {
        // The core property of the per-faction gate: under the old table gate, sol's collapse
        // vetoed letnev's real gain through the aggregate and per-faction clauses. Now each head
        // is judged on its own metrics only — sol's failure touches only sol.
        let factions = six();
        let champion: BTreeMap<FactionId, Metrics> = factions
            .iter()
            .map(|f| (f.clone(), metrics_at(2.0, 1.6, 192)))
            .collect();
        let mut candidate = champion.clone();
        candidate.insert(FactionId::new("sol"), metrics_at(0.5, 1.6, 192)); // sol collapses
        candidate.insert(
            FactionId::new("letnev"),
            metrics_at(2.8, 1.6, 192), // letnev improves for real
        );

        let letnev = FactionId::new("letnev");
        let sol = FactionId::new("sol");
        assert!(
            verdict_detail(
                &candidate,
                &champion,
                GainEvidence {
                    gain: 0.8,
                    standard_error: 0.1,
                    samples: 32
                },
                GainEvidence::default(),
                &letnev,
                2.0
            )
            .is_empty(),
            "letnev's own merit must not be touched by sol's collapse"
        );
        let sol_failed = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: -1.5,
                standard_error: 0.1,
                samples: 32,
            },
            GainEvidence::default(),
            &sol,
            2.0,
        );
        assert!(
            sol_failed.contains("own VP margin"),
            "sol's own regression must still refuse sol: {sol_failed}"
        );
    }

    #[test]
    fn zero_sigmas_disables_only_the_sigma_clauses() {
        // Opt-out rather than imposed: at 0σ the noise checks are off, but the authored margin,
        // clearance guard, and merit bar still apply.
        let sol = FactionId::new("sol");
        let champion = BTreeMap::from([(sol.clone(), metrics_at(2.0, 1.6, 192))]);
        let candidate = BTreeMap::from([(sol.clone(), metrics_at(2.06, 1.6, 192))]);
        assert!(
            verdict_detail(
                &candidate,
                &champion,
                GainEvidence {
                    gain: 0.06,
                    standard_error: 5.0,
                    samples: 32
                },
                GainEvidence::default(),
                &sol,
                0.0
            )
            .is_empty()
        );
    }

    #[test]
    fn the_verdict_names_every_refused_clause_on_both_paths() {
        // A head that fails everything must name clauses from both paths, and nothing about the
        // other five seats may appear in the explanation.
        let sol = FactionId::new("sol");
        let champion = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.0,
                clearance: 0.93,
                ..Metrics::default()
            },
        )]);
        let candidate = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 1.7,
                clearance: 0.85,
                ..Metrics::default()
            },
        )]);
        let detail = verdict_detail(
            &candidate,
            &champion,
            GainEvidence {
                gain: 0.1,
                standard_error: 0.4,
                samples: 8,
            },
            GainEvidence {
                gain: -0.08,
                standard_error: 0.3,
                samples: 8,
            },
            &sol,
            2.0,
        );
        assert!(detail.contains("own sigma evidence"), "{detail}");
        assert!(detail.contains("own clearance guard"), "{detail}");
        assert!(detail.contains("clearance merit bar"), "{detail}");
        assert!(
            detail.starts_with("VP merit failed [") && detail.contains("clearance merit failed ["),
            "{detail}"
        );

        // A clean head explains nothing.
        let improved = BTreeMap::from([(
            sol.clone(),
            Metrics {
                victory_points: 2.06,
                clearance: 0.93,
                ..Metrics::default()
            },
        )]);
        assert!(
            verdict_detail(
                &improved,
                &champion,
                GainEvidence {
                    gain: 0.06,
                    standard_error: 0.01,
                    samples: 8
                },
                GainEvidence::default(),
                &sol,
                2.0
            )
            .is_empty()
        );
    }

    #[test]
    fn paired_faction_pairs_only_the_tested_factions_own_seeds() {
        let sol = FactionId::new("sol");
        let mut candidate = PanelEvaluation {
            metrics: BTreeMap::new(),
            faction_vp_by_seed: BTreeMap::new(),
            faction_clearance_by_seed: BTreeMap::new(),
        };
        candidate
            .faction_vp_by_seed
            .insert(sol.clone(), [(1, 101.0), (2, 201.0)].into_iter().collect());
        let mut champion = PanelEvaluation {
            metrics: BTreeMap::new(),
            faction_vp_by_seed: BTreeMap::new(),
            faction_clearance_by_seed: BTreeMap::new(),
        };
        champion
            .faction_vp_by_seed
            .insert(sol.clone(), [(1, 100.0), (2, 200.0)].into_iter().collect());

        // Absolute scores vary by 100 across seeds; pairing removes the shared seed difficulty.
        let evidence = GainEvidence::paired_faction(&candidate, &champion, &sol);
        assert!((evidence.gain - 1.0).abs() < 1e-12);
        assert!(evidence.standard_error.abs() < 1e-12);
        assert_eq!(evidence.samples, 2);

        // A faction absent from one panel contributes nothing to the other's evidence.
        let missing = GainEvidence::paired_faction(&candidate, &champion, &FactionId::new("sol"));
        assert_eq!(missing.samples, 2);
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
