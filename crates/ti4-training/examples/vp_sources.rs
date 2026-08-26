//! Where do the bots' victory points actually come from?
//!
//! Table VP is the number every Stage-2 comparison turns on, and it has been reported without ever
//! saying which objectives produced it. `record_score` keeps what each seat scored, so this reads
//! it back and names them.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    ti4_training::rollout::set_seat_scramble(true);
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/stage2_ppo/s0.json".to_owned());
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();

    let seeds: Vec<u64> = (98_000_000..98_000_100).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::rounds(4),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    // Progress is snapshotted at every decision, so VP, and what was scoreable when, can be read
    // back by round. Objective *names* are not retained on the episode -- only counts -- so this
    // reports when and how much rather than which card.
    let mut by_round: BTreeMap<u32, (f64, f64, f64, usize)> = BTreeMap::new();
    let mut per_faction: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    let mut seats = 0usize;
    let mut vp_total = 0.0;
    let mut zero_vp = 0usize;
    let mut vp_hist: BTreeMap<i64, usize> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            seats += 1;
            let final_vp = seat.episode.final_progress.victory_points;
            #[expect(clippy::cast_precision_loss, reason = "VP are tiny")]
            let vp = final_vp as f64;
            vp_total += vp;
            zero_vp += usize::from(final_vp == 0);
            *vp_hist.entry(final_vp).or_default() += 1;
            let row = per_faction
                .entry(seat.faction.to_string())
                .or_insert((0.0, 0.0, 0));
            row.0 += vp;
            row.1 += seat.episode.shortfall;
            row.2 += 1;
            // Last snapshot of each round, so the VP is the round's closing total.
            let mut last: BTreeMap<u32, &ti4_policy::progress::Progress> = BTreeMap::new();
            for step in &seat.episode.steps {
                last.insert(step.round_number, step);
            }
            for (round, step) in last {
                #[expect(clippy::cast_precision_loss, reason = "counts are tiny")]
                let entry = by_round.entry(round).or_insert((0.0, 0.0, 0.0, 0));
                entry.0 += step.victory_points as f64;
                entry.1 += step.scoreable_public as f64;
                entry.2 += step.scoreable_secret as f64;
                entry.3 += 1;
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let n = seats as f64;
    println!(
        "{} games, {seats} seats
",
        games.len()
    );
    println!(
        "mean {:.3} VP per seat over four rounds
",
        vp_total / n
    );

    println!("VP DISTRIBUTION across seats:");
    for (vp, count) in &vp_hist {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let share = 100.0 * *count as f64 / n;
        println!(
            "  {vp} VP  {share:>5.1}%  {}",
            "#".repeat((share / 2.0) as usize)
        );
    }

    println!(
        "
BY ROUND -- closing VP, and what was scoreable at the time:"
    );
    println!(
        "{:<7} {:>10} {:>18} {:>18}",
        "round", "mean VP", "scoreable public", "scoreable secret"
    );
    for (round, (vp, pub_, sec, count)) in &by_round {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let c = *count as f64;
        println!(
            "{round:<7} {:>10.3} {:>18.2} {:>18.2}",
            vp / c,
            pub_ / c,
            sec / c
        );
    }

    println!(
        "
BY FACTION:"
    );
    println!("{:<9} {:>9} {:>12}", "faction", "mean VP", "shortfall");
    let mut rows: Vec<(&String, &(f64, f64, usize))> = per_faction.iter().collect();
    rows.sort_by(|a, b| (b.1.0 / b.1.2 as f64).total_cmp(&(a.1.0 / a.1.2 as f64)));
    for (faction, (vp, short, count)) in rows {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let c = *count as f64;
        println!("{faction:<9} {:>9.3} {:>12.3}", vp / c, short / c);
    }
}
