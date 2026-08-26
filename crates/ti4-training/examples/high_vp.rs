//! What happens in the games where a seat actually scores well?
//!
//! The VP distribution has a thin tail — a fraction of a percent of seats reach 7 or 8 by round
//! four, which is the pace a competent player holds routinely. Those seats are the only evidence
//! in the whole corpus of what good play looks like inside this engine, so it is worth reading
//! what they did rather than only how much they scored.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, OpeningMap, audit_game};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    ti4_training::rollout::set_seat_scramble(true);
    let path = std::env::args()
        .find(|a| a.ends_with(".json"))
        .unwrap_or_else(|| "out/stage2_clear/C2-s0.json".to_owned());
    let games: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--games=").and_then(|v| v.parse().ok()))
        .unwrap_or(200);
    let bar: i32 = std::env::args()
        .find_map(|a| a.strip_prefix("--at-least=").and_then(|v| v.parse().ok()))
        .unwrap_or(6);
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .filter_map(|f| loaded.get(f.as_str()).map(|p| (f.clone(), p.clone())))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let map = OpeningMap::PythonPool {
        pool,
        tile_seed_offset: 20_000_000,
    };

    let mut shown = 0usize;
    let mut seats = 0usize;
    // What the high scorers scored, against what everyone scored.
    let mut top_objectives: BTreeMap<String, usize> = BTreeMap::new();
    let mut all_objectives: BTreeMap<String, usize> = BTreeMap::new();
    let mut top_custodians = 0usize;
    let mut top_relics = 0usize;
    let mut top_tech = 0.0;
    let mut top_planets = 0.0;
    let mut tops = 0usize;

    for seed in 98_000_000..98_000_000 + games {
        let (_, state) = audit_game(
            store,
            &factions,
            &profiles,
            FULL,
            seed,
            Horizon::rounds(4),
            &map,
        );
        for seat in &state.players {
            seats += 1;
            let scored = state
                .scored_objectives
                .get(&seat.id)
                .cloned()
                .unwrap_or_default();
            for objective in &scored {
                *all_objectives.entry(objective.to_string()).or_default() += 1;
            }
            if seat.victory_points < bar {
                continue;
            }
            tops += 1;
            for objective in &scored {
                *top_objectives.entry(objective.to_string()).or_default() += 1;
            }
            top_relics += usize::from(!seat.relics.is_empty());
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let techs = seat.technologies.len() as f64;
            top_tech += techs;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let planets = state
                .board
                .values()
                .flat_map(|system| system.planet_control.values())
                .filter(|owner| **owner == seat.id)
                .count() as f64;
            top_planets += planets;
            if state.custodians_removed {
                top_custodians += 1;
            }
            if shown < 6 {
                shown += 1;
                println!(
                    "--- seed {seed}: {} scored {} VP",
                    seat.faction, seat.victory_points
                );
                println!(
                    "      objectives: {}",
                    scored
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                println!(
                    "      {planets:.0} planets, {} technologies, {} relics, cards {:?}",
                    seat.technologies.len(),
                    seat.relics.len(),
                    seat.strategy_cards
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let n = tops.max(1) as f64;
    println!(
        "\n{tops} seats of {seats} reached {bar}+ VP ({:.1}%)",
        100.0 * n / seats.max(1) as f64
    );
    println!(
        "  they averaged {:.1} planets, {:.1} technologies; {} held a relic; {} were in games where the custodians came off",
        top_planets / n,
        top_tech / n,
        top_relics,
        top_custodians
    );
    println!("\nOBJECTIVES the high scorers took (count among them / count overall):");
    let mut rows: Vec<(&String, &usize)> = top_objectives.iter().collect();
    rows.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (objective, count) in rows.iter().take(14) {
        let overall = all_objectives.get(*objective).copied().unwrap_or(0);
        println!("  {objective:<26} {count:>4} / {overall}");
    }
}
