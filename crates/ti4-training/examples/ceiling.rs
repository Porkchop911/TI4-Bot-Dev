//! What the authored bot scores on the Stage-2 evaluation panel.
//!
//! A learned policy that has stopped improving tells you nothing on its own about whether it has
//! converged to the best available play or merely to the best its gradient could find. This plays
//! the identical panel — same seeds, same rotations, same map pool, same four-round horizon — with
//! the authored bot in every seat, so the plateau can be read against a reference.
//!
//! `cargo run -p ti4-training --example ceiling --release -- --map-pool <path> [--seeds 32]`

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_training::rollout::{Horizon, play_rotated_pool_batch_authored};

fn number(name: &str, fallback: u64) -> u64 {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

/// One faction's totals over the panel.
#[derive(Default)]
struct Row {
    games: usize,
    cleared: usize,
    vp: i64,
    margin: i64,
    won: usize,
    planets: i64,
    systems: i64,
    units: i64,
}

/// Print the panel, and return the means.
fn report(rows: &BTreeMap<String, Row>) {
    println!("faction       games  clearance      vp  margin    win  planets  systems   units");
    println!("------------  -----  ---------  ------  ------  -----  -------  -------  ------");
    let (mut tv, mut tc, mut tw) = (0.0, 0.0, 0.0);
    for (faction, row) in rows {
        let n = f64::from(u32::try_from(row.games.max(1)).unwrap_or(1));
        let f = |v: i64| f64::from(i32::try_from(v).unwrap_or(0));
        let count = |v: usize| f64::from(u32::try_from(v).unwrap_or(0));
        let (vp, cl, win) = (f(row.vp) / n, count(row.cleared) / n, count(row.won) / n);
        tv += vp;
        tc += cl;
        tw += win;
        println!(
            "{faction:<12}  {:>5}  {cl:>9.3}  {vp:>6.2}  {:>6.2}  {:>4.1}%  {:>7.2}  {:>7.2}  {:>6.2}",
            row.games,
            f(row.margin) / n,
            100.0 * win,
            f(row.planets) / n,
            f(row.systems) / n,
            f(row.units) / n
        );
    }
    let k = f64::from(u32::try_from(rows.len().max(1)).unwrap_or(1));
    println!(
        "
MEAN          -      {:>9.3}  {:>6.2}       -  {:>4.1}%",
        tc / k,
        tv / k,
        100.0 * tw / k
    );
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let pool_path = args
        .iter()
        .position(|a| a == "--map-pool")
        .and_then(|i| args.get(i + 1))
        .ok_or("--map-pool is required so the panel matches the trainer's")?;
    let seeds = number("--seeds", 32);
    let first = number("--first-seed", 96_000_000);

    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|f| FactionId::new(*f))
        .collect();
    let pool = ti4_sim::MapPool::load(std::path::Path::new(pool_path))
        .map_err(|e| format!("load {pool_path}: {e}"))?;
    pool.validate_systems(ContentStore::embedded(), FULL)
        .map_err(|e| format!("validate {pool_path}: {e}"))?;

    let seed_block: Vec<u64> = (first..first + seeds).collect();
    let started = std::time::Instant::now();
    let rollouts = play_rotated_pool_batch_authored(
        ContentStore::embedded(),
        &factions,
        FULL,
        &seed_block,
        Horizon::short(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::new(pool),
        0,
    );
    if let Some(error) = rollouts.iter().find_map(|r| r.error.as_deref()) {
        return Err(format!("panel failed: {error}"));
    }

    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for rollout in &rollouts {
        let best: BTreeMap<&str, i64> = rollout
            .seats
            .iter()
            .map(|seat| {
                let rival = rollout
                    .seats
                    .iter()
                    .filter(|other| other.player != seat.player)
                    .map(|other| other.episode.final_progress.victory_points)
                    .max()
                    .unwrap_or(0);
                (seat.faction.as_str(), rival)
            })
            .collect();
        for seat in &rollout.seats {
            let p = seat.episode.final_progress;
            let rival = best.get(seat.faction.as_str()).copied().unwrap_or(0);
            let row = rows.entry(seat.faction.to_string()).or_default();
            row.games += 1;
            row.cleared += usize::from(seat.episode.cleared);
            row.vp += p.victory_points;
            row.margin += p.victory_points - rival;
            row.won += usize::from(p.victory_points >= rival);
            row.planets += p.planets_gained;
            row.systems += p.systems;
            row.units += p.units_gained;
        }
    }

    println!(
        "authored bot on the Stage-2 panel: {} games/faction, 4-round horizon, {:.1}s",
        seeds * u64::try_from(factions.len()).unwrap_or(6),
        started.elapsed().as_secs_f64()
    );
    report(&rows);
    Ok(())
}
