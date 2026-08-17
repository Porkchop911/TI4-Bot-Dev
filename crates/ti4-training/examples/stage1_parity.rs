//! Stage-1 parity diagnostic and training curve.
//!
//! Unlike the older `stage1_curve`, this runner uses the representation and sampling unit that
//! produced the working Python result: schema-4 collision-free features, Letnev/Jol-Nar/Hacan,
//! 16 varied-map seeds per update, every faction in every physical seat, learning rate 0.03,
//! entropy 0.01 and fixed temperature 1.0. The Rust map generator and incomplete decision windows
//! remain separate parity gates; this runner exposes their effect rather than calling them equal.
//!
//! Evaluate the solved Python table before spending time training:
//!
//! ```text
//! cargo run -p ti4-training --example stage1_parity --release -- \
//!   --updates 0 --checkpoint D:\Projects\ti4-engine\out\stage1_pg_headsplit_20260810.json
//! ```
//!
//! Run from blank and report every 25 updates:
//!
//! ```text
//! cargo run -p ti4-training --example stage1_parity --release -- --updates 50 --every 25
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::reward::Stage;
use ti4_training::rollout::{
    Horizon, Rollout, play_rotated_batch, play_rotated_save54_batch, play_rotated_save54_pool_batch,
};
use ti4_training::stage1::{
    FactionPlan, FactionStart, OpeningMetrics, evaluate_factions, evaluate_factions_on_pool,
    train_factions,
};

fn number(name: &str, fallback: usize) -> usize {
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

fn string_argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn flag(name: &str) -> bool {
    std::env::args().any(|argument| argument == name)
}

fn load_profiles(path: &Path) -> Result<BTreeMap<FactionId, Profile>, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse checkpoint: {error}"))?;
    let table = document
        .get("profiles")
        .or_else(|| document.get("accepted"))
        .unwrap_or(&document);
    let profiles: BTreeMap<String, Profile> = serde_json::from_value(table.clone())
        .map_err(|error| format!("read profile table: {error}"))?;
    profiles
        .into_iter()
        .map(|(faction, profile)| {
            profile
                .validate(Some(&faction))
                .map_err(|error| format!("{faction}: {error}"))?;
            if !profile.is_explicit() {
                return Err(format!(
                    "{faction}: schema {} is hashed; this comparison requires schema 3, 4, or 5",
                    profile.schema
                ));
            }
            Ok((FactionId::new(faction), profile))
        })
        .collect()
}

fn blank(factions: &[FactionId]) -> BTreeMap<FactionId, Profile> {
    factions
        .iter()
        .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
        .collect()
}

fn evaluate_plan(
    plan: &FactionPlan,
    profiles: &BTreeMap<FactionId, Profile>,
    first_seed: u64,
    seeds: u64,
) -> BTreeMap<FactionId, OpeningMetrics> {
    plan.map_pool.as_ref().map_or_else(
        || {
            evaluate_factions(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                first_seed,
                seeds,
            )
        },
        |pool| {
            evaluate_factions_on_pool(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                first_seed,
                seeds,
                Arc::clone(pool),
                plan.tile_seed_offset,
            )
        },
    )
}

#[allow(
    clippy::cast_precision_loss,
    reason = "diagnostic panels are bounded and small"
)]
fn report_choice_counts(
    plan: &FactionPlan,
    profiles: &BTreeMap<FactionId, Profile>,
    first_seed: u64,
    seeds: u64,
) {
    let seed_values: Vec<u64> = (first_seed..first_seed + seeds).collect();
    let rollouts = plan.map_pool.as_ref().map_or_else(
        || {
            play_rotated_batch(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                &seed_values,
                Horizon::opening(),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
            )
        },
        |pool| {
            play_rotated_save54_pool_batch(
                ContentStore::embedded(),
                &plan.factions,
                profiles,
                plan.sources,
                &seed_values,
                Horizon::opening(),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                Arc::clone(pool),
                plan.tile_seed_offset,
            )
        },
    );
    println!("\nmean learned decisions per faction-game");
    println!("faction       turn  activation  production  payment");
    for faction in &plan.factions {
        let seats: Vec<_> = rollouts
            .iter()
            .flat_map(|rollout| &rollout.seats)
            .filter(|seat| &seat.faction == faction)
            .collect();
        let mean = |head: &str| {
            seats
                .iter()
                .flat_map(|seat| &seat.trajectory)
                .filter(|step| step.head == head)
                .count() as f64
                / seats.len() as f64
        };
        println!(
            "{faction:<12} {:>5.2}      {:>5.2}       {:>5.2}    {:>5.2}",
            mean("turn"),
            mean("activation"),
            mean("production"),
            mean("payment")
        );
    }
    report_component_pass_rates(&rollouts);
}

fn semantic_gate(
    plan: &FactionPlan,
    profiles: &BTreeMap<FactionId, Profile>,
) -> Result<(), String> {
    let rollouts = play_rotated_batch(
        ContentStore::embedded(),
        &plan.factions,
        profiles,
        plan.sources,
        &[91_000_000],
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
    );
    if rollouts.len() != plan.factions.len() {
        return Err(format!(
            "one seed produced {} rotations, expected {}",
            rollouts.len(),
            plan.factions.len()
        ));
    }
    if let Some(error) = rollouts.iter().find_map(|rollout| rollout.error.as_ref()) {
        return Err(format!("rollout failed: {error}"));
    }
    // Feature vectors are keyed by hash; this diagnostic reads names, so resolve them.
    let feature_names: std::collections::BTreeSet<String> = rollouts
        .iter()
        .flat_map(|rollout| &rollout.seats)
        .flat_map(|seat| &seat.trajectory)
        .flat_map(|step| step.legal.values())
        .flat_map(BTreeMap::keys)
        .map(|key| ti4_policy::intern::name_of(*key))
        .collect();
    for (label, prefixes) in [
        ("activation target", &["target:"][..]),
        ("movement origin/unit", &["origin:", "move-unit:"][..]),
        ("movement destination", &["destination:"][..]),
    ] {
        for prefix in prefixes {
            if !feature_names.iter().any(|name| name.starts_with(prefix)) {
                return Err(format!(
                    "no {label} feature with prefix {prefix} was recorded"
                ));
            }
        }
    }
    for faction in &plan.factions {
        let physical: std::collections::BTreeSet<_> = rollouts
            .iter()
            .flat_map(|rollout| &rollout.seats)
            .filter(|seat| &seat.faction == faction)
            .map(|seat| seat.player.to_string())
            .collect();
        if physical.len() != plan.factions.len() {
            return Err(format!(
                "{faction} occupied {} physical seats, expected {}",
                physical.len(),
                plan.factions.len()
            ));
        }
    }
    Ok(())
}

fn diagnose_profile_use(plan: &FactionPlan, profiles: &BTreeMap<FactionId, Profile>) {
    let rollouts = play_rotated_batch(
        ContentStore::embedded(),
        &plan.factions,
        profiles,
        plan.sources,
        &[91_000_001],
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
    );
    println!("\nprofile/choice diagnostic (one map, all rotations)");
    for faction in &plan.factions {
        let Some(profile) = profiles.get(faction) else {
            continue;
        };
        println!("  {faction}");
        let mut by_head: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
        let mut chosen: BTreeMap<(String, String), usize> = BTreeMap::new();
        for step in rollouts
            .iter()
            .flat_map(|rollout| &rollout.seats)
            .filter(|seat| &seat.faction == faction)
            .flat_map(|seat| &seat.trajectory)
        {
            let weights = profile
                .head(&step.head)
                .map(|head| &head.weights)
                .expect("validated head");
            let entry = by_head.entry(step.head.clone()).or_default();
            entry.0 += 1;
            entry.1 += step.features().len();
            entry.2 += step
                .features()
                .keys()
                .filter(|key| weights.contains_key(&ti4_policy::intern::name_of(**key)))
                .count();
            *chosen
                .entry((step.head.clone(), step.chosen.clone()))
                .or_default() += 1;
        }
        for head in [
            "strategy",
            "turn",
            "activation",
            "movement",
            "cargo",
            "landing",
            "production",
            "payment",
        ] {
            let (actions, emitted, matched) = by_head.get(head).copied().unwrap_or_default();
            if actions == 0 {
                continue;
            }
            let mut common: Vec<_> = chosen
                .iter()
                .filter(|((candidate, _), _)| candidate == head)
                .map(|((_, option), count)| (option, *count))
                .collect();
            common.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            let labels = common
                .into_iter()
                .take(3)
                .map(|(option, count)| format!("{option}×{count}"))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "    {head:<10} actions={actions:<4} weighted-feature-overlap={matched}/{emitted} choices=[{labels}]"
            );
        }
    }
}

fn trace_faction(plan: &FactionPlan, profiles: &BTreeMap<FactionId, Profile>, wanted: &str) {
    let rollouts = play_rotated_batch(
        ContentStore::embedded(),
        &plan.factions,
        profiles,
        plan.sources,
        &[91_000_001],
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
    );
    print_trace(&rollouts, wanted, "Rust varied map");
}

fn print_trace(rollouts: &[Rollout], wanted: &str, label: &str) {
    print_trace_matching(rollouts, wanted, label, false);
}

fn print_trace_matching(rollouts: &[Rollout], wanted: &str, label: &str, require_failure: bool) {
    let Some(seat) = rollouts
        .iter()
        .flat_map(|rollout| &rollout.seats)
        .find(|seat| {
            seat.faction.as_str() == wanted && (!require_failure || !seat.episode.cleared)
        })
    else {
        println!("\nno trace found for faction {wanted}");
        return;
    };
    println!(
        "\nordered trace for {wanted} on {label} in physical {}",
        seat.player
    );
    for (index, step) in seat.trajectory.iter().enumerate() {
        let chosen_probability = step.probabilities.get(&step.chosen).copied().unwrap_or(0.0);
        println!(
            "  {index:>3} {:<10} {:<28} p={chosen_probability:.3} legal={} progress={}/{}/{}",
            step.head,
            step.chosen,
            step.legal.len(),
            step.progress.planets_gained,
            step.progress.systems,
            step.progress.units_gained,
        );
        if step.head == "activation" {
            let mut alternatives: Vec<_> = step.probabilities.iter().collect();
            alternatives.sort_by(|a, b| b.1.total_cmp(a.1));
            let top = alternatives
                .into_iter()
                .take(5)
                .map(|(option, probability)| {
                    let reachable = step
                        .legal
                        .get(option)
                        .and_then(|features| {
                            ti4_policy::features::value_of(features, "target:reachable")
                        })
                        .unwrap_or(0.0);
                    format!("{option}:{probability:.3}/r{reachable:.0}")
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!("      activation alternatives {top}");
            if let (Some(probability), Some(features)) =
                (step.probabilities.get("26"), step.legal.get("26"))
            {
                let reachable =
                    ti4_policy::features::value_of(features, "target:reachable").unwrap_or(0.0);
                println!("      tile 26 p={probability:.3} reachable={reachable:.0}");
            }
        }
        if step.head == "movement" {
            let top = step
                .probabilities
                .iter()
                .map(|(option, probability)| {
                    let features = step.legal.get(option);
                    let unit = features
                        .and_then(|features| {
                            ti4_policy::features::names_of(features)
                                .into_iter()
                                .find_map(|name| {
                                    name.strip_prefix("payload:unit:").map(str::to_owned)
                                })
                        })
                        .unwrap_or_else(|| "done".to_owned());
                    let capacity = features
                        .and_then(|features| {
                            ti4_policy::features::value_of(features, "payload-number:capacity")
                        })
                        .unwrap_or(0.0);
                    format!("{option}:{probability:.3}/{unit}/c{capacity:.0}")
                })
                .collect::<Vec<_>>()
                .join(" ");
            println!("      movement alternatives {top}");
        }
        if step.head == "development" {
            let mut ranked = step.probabilities.iter().collect::<Vec<_>>();
            ranked.sort_by(|left, right| right.1.total_cmp(left.1));
            let top = ranked
                .into_iter()
                .take(8)
                .map(|(option, probability)| format!("{option}:{probability:.3}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!("      development alternatives {top}");
            if let Some(features) = step.legal.get("sr") {
                println!(
                    "      sr features {}",
                    ti4_policy::features::names_of(features).join(" ")
                );
            }
        }
    }
}

fn report(update: usize, rows: &BTreeMap<FactionId, OpeningMetrics>) {
    println!("\nupdate {update}");
    println!("faction       games  clearance  planets  systems   units  shortfall");
    println!("------------  -----  ---------  -------  -------  ------  ---------");
    for (faction, row) in rows {
        println!(
            "{faction:<12}  {:>5}    {:>6.3}    {:>5.2}    {:>5.2}   {:>5.2}     {:>5.3}",
            row.seat_games,
            row.clearance,
            row.planets_gained,
            row.systems,
            row.units_gained,
            row.shortfall,
        );
    }
}

#[allow(clippy::cast_precision_loss, reason = "diagnostic panels are small")]
fn report_component_pass_rates(rollouts: &[Rollout]) {
    #[derive(Default)]
    struct Passes {
        games: usize,
        planets: usize,
        systems: usize,
        units: usize,
    }
    let requirement = ti4_engine::opening::DEFAULT_REQUIREMENT;
    let mut totals: BTreeMap<FactionId, Passes> = BTreeMap::new();
    for seat in rollouts
        .iter()
        .filter(|rollout| rollout.error.is_none())
        .flat_map(|rollout| &rollout.seats)
    {
        let total = totals.entry(seat.faction.clone()).or_default();
        let progress = seat.episode.final_progress;
        total.games += 1;
        total.planets += usize::from(
            progress.planets_gained
                >= i64::try_from(requirement.planets_gained).unwrap_or(i64::MAX),
        );
        total.systems +=
            usize::from(progress.systems >= i64::try_from(requirement.systems).unwrap_or(i64::MAX));
        total.units += usize::from(
            progress.units_gained >= i64::try_from(requirement.units_gained).unwrap_or(i64::MAX),
        );
    }
    println!("component pass rates (individual bars, before conjunction)");
    println!("faction       planets  systems   units");
    for (faction, total) in totals {
        let n = total.games.max(1) as f64;
        println!(
            "{faction:<12}   {:>6.3}   {:>6.3}  {:>6.3}",
            total.planets as f64 / n,
            total.systems as f64 / n,
            total.units as f64 / n,
        );
    }
}

#[allow(clippy::cast_precision_loss, reason = "diagnostic panels are small")]
fn metrics_from_rollouts(rollouts: &[Rollout]) -> BTreeMap<FactionId, OpeningMetrics> {
    #[derive(Default)]
    struct Total {
        games: usize,
        cleared: usize,
        planets: i64,
        systems: i64,
        units: i64,
        shortfall: f64,
    }
    let mut totals: BTreeMap<FactionId, Total> = BTreeMap::new();
    for seat in rollouts
        .iter()
        .filter(|rollout| rollout.error.is_none())
        .flat_map(|rollout| &rollout.seats)
    {
        let total = totals.entry(seat.faction.clone()).or_default();
        total.games += 1;
        total.cleared += usize::from(seat.episode.cleared);
        total.planets += seat.episode.final_progress.planets_gained;
        total.systems += seat.episode.final_progress.systems;
        total.units += seat.episode.final_progress.units_gained;
        total.shortfall += seat.episode.shortfall;
    }
    totals
        .into_iter()
        .map(|(faction, total)| {
            let n = total.games.max(1) as f64;
            (
                faction,
                OpeningMetrics {
                    seat_games: total.games,
                    clearance: total.cleared as f64 / n,
                    planets_gained: total.planets as f64 / n,
                    systems: total.systems as f64 / n,
                    units_gained: total.units as f64 / n,
                    shortfall: total.shortfall / n,
                },
            )
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
struct SavedRun {
    schema_version: u32,
    trainer: String,
    update: usize,
    profiles: BTreeMap<FactionId, Profile>,
}

fn save_run(
    path: &Path,
    update: usize,
    profiles: &BTreeMap<FactionId, Profile>,
) -> Result<(), String> {
    let saved = SavedRun {
        schema_version: 1,
        trainer: "rust-stage1-parity".to_owned(),
        update,
        profiles: profiles.clone(),
    };
    let bytes =
        serde_json::to_vec_pretty(&saved).map_err(|error| format!("serialise output: {error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

#[allow(
    clippy::too_many_lines,
    reason = "the executable keeps its ordered comparison and gate sequence visible in one place"
)]
fn main() -> Result<(), String> {
    let updates = number("--updates", 50);
    let every = number("--every", 25).max(1);
    let evaluation_seeds = u64::try_from(number("--eval-seeds", 32)).unwrap_or(32);
    let evaluation_first_seed =
        u64::try_from(number("--eval-first-seed", 82_000_000)).unwrap_or(82_000_000);
    let mut plan = FactionPlan::python_reference();
    let map_pool_path = path_argument("--map-pool");
    if let Some(path) = &map_pool_path {
        let pool = ti4_sim::MapPool::load(path)
            .map_err(|error| format!("load {}: {error}", path.display()))?;
        pool.validate_systems(ContentStore::embedded(), plan.sources)
            .map_err(|error| format!("validate {}: {error}", path.display()))?;
        plan.map_pool = Some(Arc::new(pool));
    }
    let checkpoint = path_argument("--checkpoint");
    let output = path_argument("--out");
    let mut profiles = checkpoint
        .as_deref()
        .map(load_profiles)
        .transpose()?
        .unwrap_or_else(|| blank(&plan.factions));
    profiles.retain(|faction, _| plan.factions.contains(faction));
    if profiles.len() != plan.factions.len() {
        return Err("checkpoint does not contain exactly Letnev, Jol-Nar, and Hacan".to_owned());
    }

    println!("Stage-1 parity configuration");
    println!("  factions: letnev,jolnar,hacan");
    println!("  representation: schema 4 explicit named heads");
    println!("  batch: 16 seeds x 3 rotations = 48 games/update");
    if let Some(pool) = &plan.map_pool {
        println!(
            "  maps: Python-compatible Save-54 pool ({} arrangements, effort {}); shared across rotations",
            pool.len(),
            pool.effort()
        );
        println!("  tile seed: game seed + {}", plan.tile_seed_offset);
    } else {
        println!("  maps: Rust varied-map generator; same map shared across rotations");
        println!("  map parity: NOT CLAIMED without --map-pool");
    }
    println!("  learning rate / entropy / clip: 0.03 / 0.01 / 1.0");
    println!("  temperature: fixed 1.0");
    println!(
        "  reward: stage {:?}, clear 22, expansion 2, units 1",
        Stage::One
    );
    semantic_gate(&plan, &profiles)?;
    println!("  semantic gate: PASS (rotations + recorded structured board facts)");
    if flag("--diagnose") {
        diagnose_profile_use(&plan, &profiles);
    }
    if let Some(faction) = string_argument("--trace-faction") {
        trace_faction(&plan, &profiles, &faction);
    }

    let initial = evaluate_plan(&plan, &profiles, evaluation_first_seed, evaluation_seeds);
    report(0, &initial);
    if flag("--choice-counts") {
        report_choice_counts(&plan, &profiles, evaluation_first_seed, evaluation_seeds);
    }
    if flag("--save54-ablation") {
        let seeds: Vec<u64> =
            (evaluation_first_seed..evaluation_first_seed + evaluation_seeds).collect();
        let save54 = play_rotated_save54_batch(
            ContentStore::embedded(),
            &plan.factions,
            &profiles,
            plan.sources,
            &seeds,
            Horizon::opening(),
            ti4_engine::opening::DEFAULT_REQUIREMENT,
        );
        println!("\nSave-54 captured-geometry ablation");
        report(0, &metrics_from_rollouts(&save54));
        report_component_pass_rates(&save54);
        if let Some(faction) = string_argument("--trace-save54") {
            print_trace(&save54, &faction, "Save-54 captured geometry");
        }
        if let Some(faction) = string_argument("--trace-save54-failed") {
            print_trace_matching(&save54, &faction, "failed Save-54 captured geometry", true);
        }
    }
    if let Some(faction) = string_argument("--trace-pool-failed") {
        let pool = plan
            .map_pool
            .as_ref()
            .ok_or_else(|| "--trace-pool-failed requires --map-pool".to_owned())?;
        let seeds: Vec<u64> =
            (evaluation_first_seed..evaluation_first_seed + evaluation_seeds).collect();
        let pooled = play_rotated_save54_pool_batch(
            ContentStore::embedded(),
            &plan.factions,
            &profiles,
            plan.sources,
            &seeds,
            Horizon::opening(),
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            Arc::clone(pool),
            plan.tile_seed_offset,
        );
        print_trace_matching(&pooled, &faction, "Save-54 map pool", true);
    }
    let solved_gate_failed =
        checkpoint.is_some() && initial.values().any(|metrics| metrics.clearance < 0.80);
    if checkpoint.is_some() {
        println!(
            "\nsolved-profile transfer target: every faction >= 0.800 clearance \
             (Python reference: hacan 0.979, jolnar 0.979, letnev 0.938)"
        );
        println!(
            "solved-profile transfer gate: {}",
            if solved_gate_failed { "FAIL" } else { "PASS" }
        );
        if solved_gate_failed {
            println!(
                "the representation is now present, but Rust environment/choice parity is still \
                 insufficient; do not interpret a learning-speed comparison as an engine speedup"
            );
        }
    }

    let started = std::time::Instant::now();
    let mut done = 0usize;
    while done < updates {
        let count = every.min(updates - done);
        plan.generations = count;
        plan.start = Some(FactionStart {
            profiles,
            generation: done,
        });
        let run = train_factions(ContentStore::embedded(), &plan);
        if run
            .generations
            .iter()
            .any(|generation| generation.errors > 0)
        {
            return Err("a training rollout failed; refusing to report it as learning".to_owned());
        }
        profiles = run.profiles;
        done += count;
        report(
            done,
            &evaluate_plan(&plan, &profiles, evaluation_first_seed, evaluation_seeds),
        );
        if let Some(path) = &output {
            save_run(path, done, &profiles)?;
            println!("checkpointed {} at update {done}", path.display());
        }
    }
    println!(
        "\n{updates} updates in {:.1}s",
        started.elapsed().as_secs_f64()
    );

    if let Some(path) = &output {
        save_run(path, updates, &profiles)?;
        println!("saved {}", path.display());
    }
    if solved_gate_failed && !flag("--allow-solved-regression") {
        return Err(
            "solved Python weights failed the Rust transfer gate; pass --allow-solved-regression only for diagnostics"
                .to_owned(),
        );
    }
    Ok(())
}
