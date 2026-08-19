//! Per-faction report for a Stage-2 run: clearance, VP, round-1 secondaries, technologies.
//!
//! Spreads are min/max across the run's training seeds, not across games, because that is the
//! variation that has repeatedly exceeded the effects being measured — identical starting weights
//! have landed a faction anywhere from 0.34 to 0.98 depending only on the order games arrived in.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{
    Horizon, OpeningMap, audit_game, play_rotated_save54_pool_batch,
};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

/// Which strategy card a secondary belongs to, read off the option's own words.
///
/// The option ids are `yes`/`no`; what names the card is the verb the accepting option carries —
/// Trade replenishes, Warfare produces, Construction builds, and so on.
fn secondary_card(words: &BTreeSet<String>) -> Option<&'static str> {
    for (token, card) in [
        ("replenish", "Trade"),
        ("produce", "Warfare"),
        ("build", "Construction"),
        ("place", "Construction"),
        ("research", "Technology"),
        ("ready", "Diplomacy"),
        ("draw", "Politics/Imperial"),
        ("token", "Leadership"),
        ("influence", "Leadership"),
    ] {
        if words.contains(token) {
            return Some(card);
        }
    }
    None
}

fn faction_list() -> Vec<FactionId> {
    ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect()
}

fn load(path: &str, factions: &[FactionId]) -> Option<BTreeMap<FactionId, Profile>> {
    let bytes = std::fs::read(path).ok()?;
    let document: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).ok()?;
    Some(
        factions
            .iter()
            .filter_map(|f| loaded.get(f.as_str()).map(|p| (f.clone(), p.clone())))
            .collect(),
    )
}

fn main() {
    let store = ContentStore::embedded();
    let factions = faction_list();
    ti4_training::rollout::set_seat_scramble(true);
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/stage2_v2".to_owned());
    let panel: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(60);
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let map = OpeningMap::PythonPool {
        pool: Arc::clone(&pool),
        tile_seed_offset: 20_000_000,
    };
    let seeds: Vec<u64> = (98_000_000..98_000_000 + panel).collect();

    // Per training seed, per faction: clearance and VP. Spreads come from these.
    let mut clearance: BTreeMap<FactionId, Vec<f64>> = BTreeMap::new();
    let mut points: BTreeMap<FactionId, Vec<f64>> = BTreeMap::new();
    // Pooled across training seeds: what they do, rather than how well.
    let mut secondaries: BTreeMap<FactionId, BTreeMap<String, usize>> = BTreeMap::new();
    let mut technologies: BTreeMap<FactionId, BTreeMap<String, usize>> = BTreeMap::new();
    let mut starting: BTreeMap<FactionId, BTreeSet<String>> = BTreeMap::new();
    let mut runs = 0;

    for index in 0..8 {
        let path = format!("{dir}/s{index}.json");
        let Some(profiles) = load(&path, &factions) else {
            continue;
        };
        runs += 1;
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
        let mut cleared: BTreeMap<FactionId, (usize, usize, f64)> = BTreeMap::new();
        for game in &games {
            for seat in &game.seats {
                let row = cleared.entry(seat.faction.clone()).or_insert((0, 0, 0.0));
                row.0 += 1;
                row.1 += usize::from(seat.episode.cleared);
                #[expect(clippy::cast_precision_loss, reason = "VP are tiny")]
                let vp = seat.episode.final_progress.victory_points as f64;
                row.2 += vp;

                // Round-1 secondaries actually followed.
                for step in &seat.trajectory {
                    if step.head != "secondary"
                        || step.progress.round_number != 1
                        || step.chosen == "no"
                    {
                        continue;
                    }
                    let Some(vector) = step.legal.get(&step.chosen) else {
                        continue;
                    };
                    let words: BTreeSet<String> = vector
                        .iter()
                        .filter_map(|(slot, _)| {
                            ti4_policy::intern::name_of(*slot)
                                .strip_prefix("option:")
                                .map(str::to_owned)
                        })
                        .collect();
                    if let Some(card) = secondary_card(&words) {
                        *secondaries
                            .entry(seat.faction.clone())
                            .or_default()
                            .entry(card.to_owned())
                            .or_default() += 1;
                    }
                }
            }
        }
        for (faction, (games_played, wins, vp)) in cleared {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let n = games_played as f64;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let w = wins as f64;
            clearance.entry(faction.clone()).or_default().push(w / n);
            points.entry(faction).or_default().push(vp / n);
        }

        // Technologies need the end state, which the rollout path discards.
        for seed in seeds.iter().take(20) {
            let (_, state) = audit_game(
                store,
                &factions,
                &profiles,
                FULL,
                *seed,
                Horizon::rounds(4),
                &map,
            );
            for seat in &state.players {
                let faction = seat.faction.clone();
                let held: BTreeSet<String> =
                    seat.technologies.iter().map(ToString::to_string).collect();
                // Starting tech is a static faction property, so it is looked up rather than
                // inferred from what every game happens to hold — inferring it would misclassify
                // anything researched in every single game as a starting technology.
                let base = starting.entry(faction.clone()).or_insert_with(|| {
                    // Resolved the same way seating resolves them. The record holds printed
                    // names while the state holds resolved ids, so comparing the two directly
                    // matches nothing -- which is why Jol-Nar's starting Sarween Tools was
                    // reported as its most-acquired technology.
                    ti4_content::factions::get(store, faction.as_str())
                        .map(|record| {
                            record
                                .starting_tech()
                                .iter()
                                .filter_map(|name| {
                                    store.resolve_id(
                                        ti4_model::content_types::ContentType::Technologies,
                                        name,
                                        FULL,
                                    )
                                })
                                .map(ToString::to_string)
                                .collect()
                        })
                        .unwrap_or_default()
                });
                for tech in held.difference(&base.clone()) {
                    *technologies
                        .entry(faction.clone())
                        .or_default()
                        .entry(tech.clone())
                        .or_default() += 1;
                }
            }
        }
    }

    println!("{runs} training seeds, {panel}-seed panel, 4 rounds\n");
    println!(
        "{:<9} {:>8} {:>17}   {:>7} {:>17}",
        "faction", "clear", "[min, max]", "VP", "[min, max]"
    );
    println!("{}", "-".repeat(66));
    let mean = |v: &Vec<f64>| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let lo = |v: &Vec<f64>| v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = |v: &Vec<f64>| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut order: Vec<&FactionId> = clearance.keys().collect();
    order.sort_by(|a, b| mean(&points[*b]).total_cmp(&mean(&points[*a])));
    for faction in order {
        let (c, p) = (&clearance[faction], &points[faction]);
        println!(
            "{:<9} {:>8.4} [{:>6.4}, {:>6.4}]   {:>7.3} [{:>6.3}, {:>6.3}]",
            faction.as_str(),
            mean(c),
            lo(c),
            hi(c),
            mean(p),
            lo(p),
            hi(p)
        );
    }

    println!("\nROUND-1 SECONDARIES FOLLOWED (% of that faction's round-1 follows)");
    for (faction, counts) in &secondaries {
        let total: usize = counts.values().sum();
        let mut rows: Vec<(&String, &usize)> = counts.iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let share = |n: usize| 100.0 * n as f64 / total.max(1) as f64;
        let text: Vec<String> = rows
            .iter()
            .map(|(card, n)| format!("{card} {:.0}%", share(**n)))
            .collect();
        println!("  {:<9} {}", faction.as_str(), text.join("  "));
    }

    println!("\nTECHNOLOGIES ACQUIRED, excluding starting tech (share of seats holding it)");
    for faction in &factions {
        let Some(counts) = technologies.get(faction) else {
            continue;
        };
        let mut rows: Vec<(&String, &usize)> = counts.iter().collect();
        rows.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        let text: Vec<String> = rows
            .iter()
            .take(6)
            .map(|(tech, n)| {
                // Printed name, not the corpus abbreviation: `sr` is Sling Relay and `st` is
                // Sarween Tools, which are easy to read as each other at a glance.
                let name = store
                    .get(
                        ti4_model::content_types::ContentType::Technologies,
                        tech.as_str(),
                    )
                    .and_then(|record| record.text("name").map(ToOwned::to_owned))
                    .unwrap_or_else(|| (*tech).clone());
                format!("{name} ({n})")
            })
            .collect();
        println!(
            "  {:<9} {}",
            faction.as_str(),
            if text.is_empty() {
                "none".to_owned()
            } else {
                text.join("  ")
            }
        );
    }
}
