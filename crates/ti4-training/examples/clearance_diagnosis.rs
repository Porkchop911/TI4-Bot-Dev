//! Why does a seat fail to clear, when 100% is achievable?
//!
//! Clearance sits at 0.75–0.89 per faction and the bar is reachable on every board with the right
//! primary or secondary — Construction or Warfare primary, or the Warfare secondary, is enough
//! even for the hardest faction. So a failure is a decision, not a constraint, and the useful
//! question is which decision.
//!
//! Three things separate the possibilities:
//!
//! * **how far short** a failing seat was — missing by a fraction of a planet is a different
//!   failure from missing by two;
//! * **clearance conditional on the primary taken**, so a card that reliably clears shows up;
//! * **clearance conditional on following the Warfare or Construction secondary in round one**,
//!   which is the route available to a seat that did not take the primary.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

const CARDS: [(&str, &str); 8] = [
    ("pok1leadership", "Lead"),
    ("pok2diplomacy", "Dipl"),
    ("pok3politics", "Poli"),
    ("te4construction", "Cons"),
    ("pok5trade", "Trad"),
    ("te6warfare", "Warf"),
    ("pok7technology", "Tech"),
    ("pok8imperial", "Impe"),
];

fn card_label(id: &str) -> &'static str {
    CARDS
        .iter()
        .find(|(alias, _)| id.starts_with(&alias[..7.min(alias.len())]))
        .map_or("????", |(_, short)| *short)
}

/// Which secondary an accepting option belongs to, from the verb it carries.
fn secondary_of(words: &BTreeSet<String>) -> Option<&'static str> {
    // Built from what the options actually carry, not from the card text. Option features come
    // from the option id and label only (features.rs:290), never the prompt, so Technology --
    // whose label is bare "spend" -- was invisible to a mapping that looked for "research".
    // Leadership carries "influence" alongside "spend" and must be tested first.
    for (token, card) in [
        ("produce", "Warf"),
        ("build", "Cons"),
        ("place", "Cons"),
        ("replenish", "Trad"),
        ("ready", "Dipl"),
        // Imperial and Politics both label their accepting option "draw" and carry no word that
        // separates them; they share a column.
        ("draw", "Draw"),
        ("influence", "Lead"),
        ("spend", "Tech"),
    ] {
        if words.contains(token) {
            return Some(card);
        }
    }
    None
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    ti4_training::rollout::set_seat_scramble(true);
    let path = std::env::args()
        .find(|a| a.ends_with(".json"))
        .unwrap_or_else(|| "out/stage2_clear/C1-s0.json".to_owned());
    let panel: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(120);
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .filter_map(|f| loaded.get(f.as_str()).map(|p| (f.clone(), p.clone())))
        .collect();
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(4);
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let seeds: Vec<u64> = (98_000_000..98_000_000 + panel).collect();

    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::rounds(rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    // (cleared, total) keyed several ways.
    let mut shortfalls: BTreeMap<FactionId, Vec<f64>> = BTreeMap::new();
    let mut by_primary: BTreeMap<(FactionId, &'static str), (usize, usize)> = BTreeMap::new();
    let mut by_route: BTreeMap<(FactionId, &'static str), (usize, usize)> = BTreeMap::new();
    let mut overall: BTreeMap<FactionId, (usize, usize)> = BTreeMap::new();
    // How often each faction opted into each card's secondary in round one.
    let mut by_secondary: BTreeMap<(FactionId, &'static str), usize> = BTreeMap::new();
    let mut seats_seen: BTreeMap<FactionId, usize> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            let cleared = usize::from(seat.episode.cleared);
            let row = overall.entry(seat.faction.clone()).or_insert((0, 0));
            row.0 += 1;
            row.1 += cleared;
            if seat.episode.cleared {
                // Only failures carry a shortfall worth reading.
            } else {
                shortfalls
                    .entry(seat.faction.clone())
                    .or_default()
                    .push(seat.episode.shortfall);
            }

            let mut primary = "none";
            let mut followed: BTreeSet<&'static str> = BTreeSet::new();
            for step in &seat.trajectory {
                match step.head.as_str() {
                    "strategy" => primary = card_label(&step.chosen),
                    "secondary" if step.progress.round_number == 1 && step.chosen != "no" => {
                        if let Some(vector) = step.legal.get(&step.chosen) {
                            let words: BTreeSet<String> = vector
                                .iter()
                                .filter_map(|(slot, _)| {
                                    ti4_policy::intern::name_of(*slot)
                                        .strip_prefix("option:")
                                        .map(str::to_owned)
                                })
                                .collect();
                            if let Some(card) = secondary_of(&words) {
                                followed.insert(card);
                            }
                        }
                    }
                    _ => {}
                }
            }
            let cell = by_primary
                .entry((seat.faction.clone(), primary))
                .or_insert((0, 0));
            cell.0 += 1;
            cell.1 += cleared;

            // The routes the rules make sufficient: the two primaries, or the Warfare secondary.
            let route = if primary == "Cons" || primary == "Warf" {
                "took Cons/Warf primary"
            } else if followed.contains("Warf") {
                "followed Warf secondary"
            } else if followed.contains("Cons") {
                "followed Cons secondary"
            } else {
                "neither"
            };
            let cell = by_route.entry((seat.faction.clone(), route)).or_insert((0, 0));
            cell.0 += 1;
            cell.1 += cleared;

            *seats_seen.entry(seat.faction.clone()).or_default() += 1;
            for card in &followed {
                *by_secondary.entry((seat.faction.clone(), *card)).or_default() += 1;
            }
        }
    }

    println!("{} games, checkpoint {path}\n", games.len());

    println!("HOW FAR SHORT the failures were (shortfall, only seats that missed):");
    println!("{:<9} {:>7} {:>8} {:>8} {:>8} {:>8}", "faction", "misses", "mean", "<=0.5", "<=1.0", ">2.0");
    for (faction, values) in &shortfalls {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let n = values.len().max(1) as f64;
        let mean = values.iter().sum::<f64>() / n;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let near = 100.0 * values.iter().filter(|v| **v <= 0.5).count() as f64 / n;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let one = 100.0 * values.iter().filter(|v| **v <= 1.0).count() as f64 / n;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let far = 100.0 * values.iter().filter(|v| **v > 2.0).count() as f64 / n;
        println!(
            "{:<9} {:>7} {mean:>8.2} {near:>7.0}% {one:>7.0}% {far:>7.0}%",
            faction.as_str(),
            values.len()
        );
    }

    println!("\nCLEARANCE BY THE ROUTE TAKEN (n in brackets; the rules make these sufficient)");
    let routes = [
        "took Cons/Warf primary",
        "followed Warf secondary",
        "followed Cons secondary",
        "neither",
    ];
    print!("{:<9}", "faction");
    for route in routes {
        print!("{:>26}", route);
    }
    println!();
    for faction in &factions {
        print!("{:<9}", faction.as_str());
        for route in routes {
            let (n, cleared) = by_route
                .get(&(faction.clone(), route))
                .copied()
                .unwrap_or((0, 0));
            if n < 15 {
                print!("{:>26}", "-");
            } else {
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let rate = cleared as f64 / n as f64;
                print!("{:>20.3}{:>6}", rate, format!("({n})"));
            }
        }
        println!();
    }

    println!("
ROUND-1 SECONDARIES FOLLOWED (% of that faction's seats)");
    let secondaries = ["Lead", "Dipl", "Cons", "Trad", "Warf", "Tech", "Draw"];
    print!("{:<9}", "faction");
    for card in secondaries {
        print!("{card:>9}");
    }
    println!();
    for faction in &factions {
        print!("{:<9}", faction.as_str());
        let seats = seats_seen.get(faction).copied().unwrap_or(0).max(1);
        for card in secondaries {
            let n = by_secondary.get(&(faction.clone(), card)).copied().unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = 100.0 * n as f64 / seats as f64;
            print!("{rate:>8.0}%");
        }
        println!();
    }

    println!("\nCLEARANCE BY PRIMARY TAKEN");
    print!("{:<9}", "faction");
    for (_, short) in CARDS {
        print!("{short:>13}");
    }
    println!();
    for faction in &factions {
        print!("{:<9}", faction.as_str());
        for (_, short) in CARDS {
            let (n, cleared) = by_primary
                .get(&(faction.clone(), short))
                .copied()
                .unwrap_or((0, 0));
            if n < 15 {
                print!("{:>13}", "-");
            } else {
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let rate = cleared as f64 / n as f64;
                print!("{:>8.3}{:>5}", rate, format!("({n})"));
            }
        }
        println!();
    }
}
