//! Production Stage-2 policy-gradient runner.
//!
//! Starts from blank profiles unless `--checkpoint` supplies a Stage-1 bootstrap. Stage 2 always
//! uses four-round rollouts and the Stage-2 VP/objective reward. Factions rotate through every
//! physical seat on a shared varied map for each seed.

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
use ti4_training::rollout::{Horizon, play_rotated_batch, play_rotated_save54_pool_batch};
use ti4_training::stage1::{FactionPlan, FactionStart, train_factions};

fn number(name: &str, fallback: usize) -> usize {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
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
    Ok(StartState {
        profiles,
        accepted,
        update,
        history,
        telemetry,
        provenance: Some(format!("{}#sha256={checksum}", path.display())),
    })
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Metrics {
    games: usize,
    clearance: f64,
    victory_points: f64,
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
    elapsed_seconds: f64,
    candidate_metrics: BTreeMap<FactionId, Metrics>,
    accepted_metrics: BTreeMap<FactionId, Metrics>,
    confirmation_metrics: Option<BTreeMap<FactionId, Metrics>>,
    accepted: Vec<FactionId>,
    accepted_kind: Option<String>,
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

fn evaluate(
    plan: &FactionPlan,
    profiles: &BTreeMap<FactionId, Profile>,
    first_seed: u64,
    seeds: u64,
) -> Result<BTreeMap<FactionId, Metrics>, String> {
    #[derive(Default)]
    struct Totals {
        games: usize,
        cleared: usize,
        victory_points: i64,
        vp_margin: i64,
        won_or_tied: usize,
        scoreable: i64,
        planets: i64,
        systems: i64,
        units: i64,
        shortfall: f64,
    }
    let seed_block: Vec<u64> = (first_seed..first_seed + seeds).collect();
    let rollouts = plan.map_pool.as_ref().map_or_else(
        || {
            play_rotated_batch(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                &seed_block,
                Horizon::short(),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
            )
        },
        |pool| {
            play_rotated_save54_pool_batch(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                &seed_block,
                Horizon::short(),
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
            row.vp_margin += progress.victory_points - best_opponent;
            row.won_or_tied += usize::from(progress.victory_points >= best_opponent);
            row.scoreable += progress.scoreable_public + progress.scoreable_secret;
            row.planets += progress.planets_gained;
            row.systems += progress.systems;
            row.units += progress.units_gained;
            row.shortfall += seat.episode.shortfall;
        }
    }
    Ok(totals
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
        .collect())
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

fn acceptable_stage_two_table(
    candidate: &BTreeMap<FactionId, Metrics>,
    champion: &BTreeMap<FactionId, Metrics>,
    factions: &[FactionId],
    vp_margin: f64,
    max_faction_vp_regression: f64,
    max_faction_clearance_regression: f64,
) -> bool {
    if factions.iter().any(|faction| {
        candidate[faction].clearance
            < champion[faction].clearance - max_faction_clearance_regression - 1e-12
    }) || factions.iter().any(|faction| {
        candidate[faction].victory_points
            < champion[faction].victory_points - max_faction_vp_regression - 1e-12
    }) {
        return false;
    }
    let gain: f64 = factions
        .iter()
        .map(|faction| candidate[faction].victory_points - champion[faction].victory_points)
        .sum();
    let faction_count = f64::from(u32::try_from(factions.len()).unwrap_or(u32::MAX));
    gain > vp_margin * faction_count
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
) -> Checkpoint {
    let mut checkpoint = Checkpoint::new(
        "rust_stage2_policy_gradient".to_owned(),
        Stage::Two,
        Horizon::short(),
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
) -> Result<(), String> {
    let checkpoint = checkpoint_document(
        update, complete, profiles, accepted, arguments, provenance, history, telemetry,
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
    let validation_seeds =
        u64::try_from(number("--validation-seeds", number("--eval-seeds", 32))).unwrap_or(32);
    let confirmation_seeds = u64::try_from(number("--confirmation-seeds", 32)).unwrap_or(32);
    let validation_first_seed =
        u64::try_from(number("--validation-first-seed", 96_000_000)).unwrap_or(96_000_000);
    let confirmation_first_seed =
        u64::try_from(number("--confirmation-first-seed", 97_000_000)).unwrap_or(97_000_000);
    let accept_vp_margin = decimal("--accept-vp-margin", 0.05);
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

    let mut plan = FactionPlan::stage_two_reference();
    plan.train_seeds = train_seeds;
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
    } = checkpoint_path.as_deref().map_or_else(
        || {
            Ok(StartState {
                profiles: blank(&plan.factions),
                accepted: blank(&plan.factions),
                update: 0,
                history: Vec::new(),
                telemetry: Vec::new(),
                provenance: None,
            })
        },
        |path| load_start(path, &plan.factions),
    )?;
    let mut arguments = BTreeMap::from([
        ("updates".to_owned(), updates.to_string()),
        ("every".to_owned(), every.to_string()),
        ("train_seeds".to_owned(), train_seeds.to_string()),
        ("validation_seeds".to_owned(), validation_seeds.to_string()),
        (
            "confirmation_seeds".to_owned(),
            confirmation_seeds.to_string(),
        ),
        ("accept_vp_margin".to_owned(), accept_vp_margin.to_string()),
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

    println!("Stage-2 policy-gradient configuration");
    println!("  factions: sol,letnev,xxcha,hacan,jolnar,l1z1x");
    println!("  stage: VP/objective reward, four-round horizon");
    println!("  batch: {train_seeds} seeds x 6 rotations");
    println!(
        "  maps: {} and shared across rotations",
        map_pool_path.as_ref().map_or_else(
            || "Rust varied-map generator".to_owned(),
            |path| format!("Python-compatible pool {}", path.display())
        )
    );
    println!("  profiles: immutable shared schema-4 heads");
    println!("  meta teacher: none (no specified or validated artifact)");
    println!(
        "  promotion: {validation_seeds} validation + {confirmation_seeds} confirmation seeds; aggregate VP margin {accept_vp_margin:.2}; per-faction veto VP {max_faction_vp_regression:.2}, clearance {max_faction_clearance_regression:.2}"
    );
    println!("  execution: persistent Rayon pool + worker-side gradient statistics");
    println!(
        "  start: {}",
        checkpoint_path.as_ref().map_or("blank".to_owned(), |path| {
            format!("{} at update {starting_update}", path.display())
        })
    );

    let initial_candidate = evaluate(&plan, &profiles, validation_first_seed, validation_seeds)?;
    let mut accepted_metrics = evaluate(&plan, &accepted, validation_first_seed, validation_seeds)?;
    let mut accepted_confirmation = evaluate(
        &plan,
        &accepted,
        confirmation_first_seed,
        confirmation_seeds,
    )?;
    report(starting_update, &initial_candidate);
    history.push(
        serde_json::to_value(Evaluation {
            update: starting_update,
            elapsed_seconds: 0.0,
            candidate_metrics: initial_candidate,
            accepted_metrics: accepted_metrics.clone(),
            confirmation_metrics: Some(accepted_confirmation.clone()),
            accepted: Vec::new(),
            accepted_kind: Some("bootstrap".to_owned()),
        })
        .map_err(|error| format!("serialize initial evaluation: {error}"))?,
    );
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
        let candidate_metrics =
            evaluate(&plan, &profiles, validation_first_seed, validation_seeds)?;
        report(update, &candidate_metrics);
        let mut promoted = Vec::new();
        let mut accepted_kind = None;
        let mut assembled_confirmation = None;
        if acceptable_stage_two_table(
            &candidate_metrics,
            &accepted_metrics,
            &plan.factions,
            accept_vp_margin,
            max_faction_vp_regression,
            max_faction_clearance_regression,
        ) {
            let confirmation = evaluate(
                &plan,
                &profiles,
                confirmation_first_seed,
                confirmation_seeds,
            )?;
            let confirmed = acceptable_stage_two_table(
                &confirmation,
                &accepted_confirmation,
                &plan.factions,
                accept_vp_margin,
                max_faction_vp_regression,
                max_faction_clearance_regression,
            );
            assembled_confirmation = Some(confirmation.clone());
            if confirmed {
                accepted.clone_from(&profiles);
                accepted_metrics.clone_from(&candidate_metrics);
                accepted_confirmation = confirmation;
                promoted.clone_from(&plan.factions);
                accepted_kind = Some("assembled".to_owned());
            }
        }
        if promoted.is_empty() {
            for faction in &plan.factions {
                let mut isolated = accepted.clone();
                isolated.insert(faction.clone(), profiles[faction].clone());
                let primary = evaluate(&plan, &isolated, validation_first_seed, validation_seeds)?;
                let faction_improved = primary[faction].victory_points
                    > accepted_metrics[faction].victory_points + 1e-12;
                if !faction_improved
                    || !acceptable_stage_two_table(
                        &primary,
                        &accepted_metrics,
                        &plan.factions,
                        accept_vp_margin,
                        max_faction_vp_regression,
                        max_faction_clearance_regression,
                    )
                {
                    continue;
                }
                let confirmation = evaluate(
                    &plan,
                    &isolated,
                    confirmation_first_seed,
                    confirmation_seeds,
                )?;
                if acceptable_stage_two_table(
                    &confirmation,
                    &accepted_confirmation,
                    &plan.factions,
                    accept_vp_margin,
                    max_faction_vp_regression,
                    max_faction_clearance_regression,
                ) {
                    accepted = isolated;
                    accepted_metrics = primary;
                    accepted_confirmation = confirmation;
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
        history.push(
            serde_json::to_value(Evaluation {
                update,
                elapsed_seconds: started.elapsed().as_secs_f64(),
                candidate_metrics,
                accepted_metrics: accepted_metrics.clone(),
                confirmation_metrics: assembled_confirmation,
                accepted: promoted,
                accepted_kind,
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
            &good, &champion, &factions, 0.05, 0.15, 0.03
        ));

        let sacrificed = BTreeMap::from([
            (factions[0].clone(), metric(2.5, 0.9)),
            (factions[1].clone(), metric(1.8, 0.9)),
        ]);
        assert!(!acceptable_stage_two_table(
            &sacrificed,
            &champion,
            &factions,
            0.05,
            0.15,
            0.03
        ));
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
        assert_eq!(loaded.profiles[&faction].name, "learner");
        assert_eq!(loaded.accepted[&faction].name, "champion");
        assert_eq!(loaded.history.len(), 1);
    }
}
