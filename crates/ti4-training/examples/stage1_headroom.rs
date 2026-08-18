//! How much room is left at Stage 1, and how fast is a Stage-1 game?
//!
//! Before moving the algorithm arena to Stage 1 there are two things to know, and only one of them
//! is the speed. An arena discriminates between algorithms only if the task still has room to
//! improve on: if the incumbent already clears the opening bar on nearly every seat, then every
//! arm scores the same because the task is finished, and the arena measures nothing while looking
//! exactly like an arena that measured a null.
//!
//! So this reports, on a held-out panel:
//!
//! * per-faction clearance and shortfall for the Stage-1 champion, and for blank weights as the
//!   floor -- the gap between them is the band an algorithm comparison can move within;
//! * wall-clock per game at Stage 1 against Stage 2, which is the reason to want the move.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};
use ti4_training::stage1::{OpeningMetrics, evaluate_factions_on_pool};

const STAGE1: &str = "out/stage1_all_six.json";
const STAGE2: &str = "out/run_pure_u5000.json";
const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";
const TILE_OFFSET: u64 = 20_000_000;

fn load(path: &str, factions: &[FactionId]) -> BTreeMap<FactionId, Profile> {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
    let table = document
        .get("profiles")
        .or_else(|| document.get("accepted"))
        .unwrap_or(&document);
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(table.clone()).expect("profile table");
    factions
        .iter()
        .map(|faction| {
            let profile = loaded
                .get(faction.as_str())
                .cloned()
                .unwrap_or_else(|| blank_explicit_profile(faction.as_str()));
            (faction.clone(), profile)
        })
        .collect()
}

fn report(label: &str, metrics: &BTreeMap<FactionId, OpeningMetrics>) {
    println!("\n{label}");
    println!("faction       games  clearance  shortfall  planets  systems   units");
    println!("------------  -----  ---------  ---------  -------  -------  ------");
    for (faction, row) in metrics {
        println!(
            "{faction:<12}  {:>5}  {:>9.4}  {:>9.4}  {:>7.2}  {:>7.2}  {:>6.2}",
            row.seat_games,
            row.clearance,
            row.shortfall,
            row.planets_gained,
            row.systems,
            row.units_gained
        );
    }
    let count = f64::from(u32::try_from(metrics.len().max(1)).unwrap_or(1));
    println!(
        "  mean clearance {:.4}, mean shortfall {:.4}",
        metrics.values().map(|row| row.clearance).sum::<f64>() / count,
        metrics.values().map(|row| row.shortfall).sum::<f64>() / count
    );
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("map pool"));

    let trained = load(STAGE1, &factions);
    let blank: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
        .collect();

    // A held-out block, disjoint from anything training has seen.
    let seeds = 200_u64;
    let first = 98_000_000_u64;

    let champion = evaluate_factions_on_pool(
        store,
        &factions,
        &trained,
        FULL,
        first,
        seeds,
        Arc::clone(&pool),
        TILE_OFFSET,
    );
    let floor = evaluate_factions_on_pool(
        store,
        &factions,
        &blank,
        FULL,
        first,
        seeds,
        Arc::clone(&pool),
        TILE_OFFSET,
    );
    report("Stage-1 champion (out/stage1_all_six.json)", &champion);
    report("blank weights -- the floor", &floor);

    println!("\nthe band an arena could move within:");
    println!("faction       blank  champion  ceiling gap  band used");
    println!("------------  -----  --------  -----------  ---------");
    for (faction, row) in &champion {
        let low = floor.get(faction).map_or(0.0, |base| base.clearance);
        let gap = 1.0 - row.clearance;
        let band = row.clearance - low;
        println!(
            "{faction:<12}  {low:>5.3}  {:>8.4}  {gap:>11.4}  {band:>9.4}",
            row.clearance
        );
    }

    // Speed: the same games at both horizons, so the ratio is the horizon and nothing else.
    let block: Vec<u64> = (0..16).collect();
    for (label, horizon) in [
        ("Stage 1 (opening)", Horizon::opening()),
        ("Stage 2 (4 rounds)", Horizon::rounds(4)),
    ] {
        let started = Instant::now();
        let played = play_rotated_save54_pool_batch(
            store,
            &factions,
            &trained,
            FULL,
            &block,
            horizon,
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            Arc::clone(&pool),
            TILE_OFFSET,
        );
        let spent = started.elapsed().as_secs_f64();
        let decisions: usize = played
            .iter()
            .flat_map(|game| game.seats.iter())
            .map(|seat| seat.trajectory.len())
            .sum();
        println!(
            "{label:<20} {:>3} games in {spent:6.3} s  ({:.1} games/s, {decisions} decisions)",
            played.len(),
            f64::from(u32::try_from(played.len()).unwrap_or(1)) / spent
        );
    }
    let _ = STAGE2;
}
