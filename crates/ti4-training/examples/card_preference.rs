//! Which strategy card does each faction take, and from which seat?
//!
//! Strategy selection runs in speaker order -- seat0 first, seat5 last -- so a seat's choices are
//! constrained by what the seats before it left. Reading preference by faction alone would blend a
//! genuine liking for a card with simply being early enough to get it, which is why this reports
//! both, and reports what was still available where that can be recovered.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new("out/pools/save52_e400_holdout.json.gz"))
            .expect("pool"),
    );
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/prod/stage1_ppo_s0.json".to_owned());
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();

    let seeds: Vec<u64> = (98_000_000..98_000_150).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    let mut cross: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    let mut by_faction: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut by_seat: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut offered: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                if step.head != "strategy" {
                    continue;
                }
                *by_faction
                    .entry(seat.faction.to_string())
                    .or_default()
                    .entry(step.chosen.clone())
                    .or_default() += 1;
                *by_seat
                    .entry(seat.player.to_string())
                    .or_default()
                    .entry(step.chosen.clone())
                    .or_default() += 1;
                *cross
                    .entry((seat.faction.to_string(), seat.player.to_string()))
                    .or_default()
                    .entry(step.chosen.clone())
                    .or_default() += 1;
                for option in step.legal.keys() {
                    *offered
                        .entry(seat.player.to_string())
                        .or_default()
                        .entry(option.clone())
                        .or_default() += 1;
                }
            }
        }
    }

    let mut cards: Vec<String> = by_faction
        .values()
        .flat_map(|row| row.keys().cloned())
        .collect();
    cards.sort();
    cards.dedup();
    println!("checkpoint: {path}");
    println!("{} games, strategy-phase picks on held-out boards\n", games.len());

    let show = |title: &str, table: &BTreeMap<String, BTreeMap<String, usize>>| {
        println!("{title}");
        print!("{:<9}", "");
        for card in &cards {
            print!("{:>10}", card.chars().take(9).collect::<String>());
        }
        println!("{:>8}", "picks");
        for (key, row) in table {
            let total: usize = row.values().sum();
            print!("{key:<9}");
            for card in &cards {
                let count = row.get(card).copied().unwrap_or(0);
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let share = if total == 0 {
                    0.0
                } else {
                    100.0 * count as f64 / total as f64
                };
                print!("{share:>9.1}%");
            }
            println!("{total:>8}");
        }
        println!();
    };
    show("PICKED, by faction (% of that faction's picks)", &by_faction);
    // Faction x seat, for the factions whose first choice is contested.
    println!("FACTION x SEAT: what each faction takes from each seat (% of that cell)");
    print!("{:<17}", "faction / seat");
    for card in &cards {
        print!("{:>10}", card.chars().take(9).collect::<String>());
    }
    println!();
    for ((faction, seat), row) in &cross {
        let total: usize = row.values().sum();
        print!("{:<9}{:<8}", faction, seat);
        for card in &cards {
            let count = row.get(card).copied().unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let share = if total == 0 { 0.0 } else { 100.0 * count as f64 / total as f64 };
            print!("{share:>9.1}%");
        }
        println!();
    }
    println!();
    show("PICKED, by seat (% of that seat's picks)", &by_seat);
    // What each seat was even offered: seat0 chooses from all six, seat5 from whatever is left.
    println!("AVAILABILITY: % of that seat's picks where the card was still on the table");
    print!("{:<9}", "");
    for card in &cards {
        print!("{:>10}", card.chars().take(9).collect::<String>());
    }
    println!();
    for (seat, row) in &offered {
        let picks: usize = by_seat.get(seat).map_or(0, |r| r.values().sum());
        print!("{seat:<9}");
        for card in &cards {
            let count = row.get(card).copied().unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let share = if picks == 0 {
                0.0
            } else {
                100.0 * count as f64 / picks as f64
            };
            print!("{share:>9.1}%");
        }
        println!();
    }
}
