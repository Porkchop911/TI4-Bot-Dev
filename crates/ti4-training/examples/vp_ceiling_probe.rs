//! VP ceiling probe for Stage-2 sustainability questions.
//!
//! Plays a fixed panel (seeds x 6 rotations, Python-compatible map pool) with the profiles from a
//! checkpoint and reports each faction's final-VP distribution: mean / p50 / p90 / max plus how
//! many games reached at least 3 VP. Answers "is an average of 3 VP even reachable under this
//! horizon?" before spending training wall time on it.
use std::cmp::min;
use std::collections::BTreeMap;
use std::path::PathBuf;

use ti4_content::ContentStore;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};
use ti4_training::stage1::FactionPlan;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == name {
            return args.next();
        }
    }
    None
}

fn path_argument(name: &str) -> Option<PathBuf> {
    argument(name).map(PathBuf::from)
}

fn number(name: &str, default: i64) -> i64 {
    argument(name)
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let checkpoint = path_argument("--checkpoint").expect("usage: --checkpoint <path>");
    let pool_path = path_argument("--map-pool").expect("usage: --map-pool <path>");
    let seeds = u64::try_from(number("--seeds", 32)).unwrap_or(32);
    // Which profile table to play: the frozen champion ("accepted") or the active learner
    // ("profiles"). Both tables can be probed on identical seeds for paired comparison.
    let table_name = argument("--table").unwrap_or_else(|| "accepted".to_owned());

    let content = ContentStore::embedded();
    let plan = FactionPlan::stage_two_reference();
    let pool = ti4_sim::MapPool::load(&pool_path)
        .unwrap_or_else(|error| panic!("load {}: {error}", pool_path.display()));
    pool.validate_systems(content, plan.sources)
        .unwrap_or_else(|error| panic!("validate {}: {error}", pool_path.display()));

    let bytes =
        std::fs::read(&checkpoint).unwrap_or_else(|error| panic!("read checkpoint: {error}"));
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).unwrap_or_else(|error| panic!("parse checkpoint: {error}"));
    // Select the requested profile table (default: accepted/champion).
    let table = document
        .get(table_name.as_str())
        .unwrap_or_else(|| panic!("checkpoint has no '{table_name}' table"));
    let loaded: BTreeMap<String, Profile> = serde_json::from_value(table.clone())
        .unwrap_or_else(|error| panic!("read profile table: {error}"));

    let mut profiles = BTreeMap::new();
    for faction in &plan.factions {
        let profile = loaded
            .get(faction.as_str())
            .unwrap_or_else(|| panic!("{} missing from checkpoint", faction.as_str()));
        profile
            .validate(Some(faction.as_str()))
            .unwrap_or_else(|error| panic!("{faction}: {error}"));
        assert!(
            profile.is_explicit(),
            "{faction}: schema {} is hashed; probe requires explicit profiles",
            profile.schema
        );
        profiles.insert(faction.clone(), profile.clone());
    }

    let seed_block: Vec<u64> = (90_000_000..90_000_000 + seeds).collect();
    let rollouts = play_rotated_save54_pool_batch(
        content,
        &plan.factions,
        &profiles,
        plan.sources,
        &seed_block,
        Horizon::rounds(plan.rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        std::sync::Arc::new(pool),
        0,
    );

    let mut by_faction: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut cleared_by_faction: BTreeMap<String, i64> = BTreeMap::new();
    for rollout in &rollouts {
        if rollout.error.is_some() {
            continue;
        }
        for seat in &rollout.seats {
            by_faction
                .entry(seat.faction.as_str().to_owned())
                .or_default()
                .push(seat.episode.final_progress.victory_points);
            if seat.episode.cleared {
                *cleared_by_faction
                    .entry(seat.faction.as_str().to_owned())
                    .or_insert(0) += 1;
            }
        }
    }

    println!(
        "VP ceiling probe: {} seeds x {} rotations, {} rounds horizon",
        seeds,
        plan.factions.len(),
        plan.rounds
    );
    println!(
        "{:<8} {:>5} {:>7} {:>6} {:>6} {:>6} {:>6} {:>7} {:>9}",
        "faction", "n", "mean", "min", "max", "p50", "p90", "clearance", "games>=3"
    );
    for faction in &plan.factions {
        let values = by_faction
            .get(faction.as_str())
            .cloned()
            .unwrap_or_default();
        if values.is_empty() {
            println!("{:<8} {:>5}", faction.as_str(), 0);
            continue;
        }
        let mut sorted = values.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        let mean: f64 = sorted.iter().sum::<i64>() as f64 / n as f64;
        let min_vp = sorted[0];
        let p50 = sorted[n / 2];
        let p90 = sorted[min((n * 9) / 10, n - 1)];
        let max = sorted[n - 1];
        let above3 = values.iter().filter(|v| **v >= 3).count();
        let cleared = cleared_by_faction
            .get(faction.as_str())
            .copied()
            .unwrap_or(0);
        println!(
            "{:<8} {:>5} {:>7.2} {:>6} {:>6} {:>6} {:>6} {:>7.3} {:>9}",
            faction.as_str(),
            n,
            mean,
            min_vp,
            max,
            p50,
            p90,
            cleared as f64 / n as f64,
            above3
        );
    }
}
