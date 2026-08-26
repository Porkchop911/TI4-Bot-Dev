//! Does a faction need its preferred card to clear, or will any card do?
//!
//! The tables show sharp per-faction preferences, which invites the reading that each faction
//! *needs* its card. That does not follow: a preference can be a small edge the policy takes when
//! it is free, while the objective is reachable by other routes. The cleanest test is to condition
//! clearance on the card actually taken.
//!
//! The other route is the secondary. Every strategy card offers one to the players who did not
//! take it, so a seat participates in five secondaries per round and holds one primary. If those
//! are what clear the opening, then the primary matters much less than the preference tables make
//! it look -- and the `secondary` head is worth far more attention than it has had.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const LABEL: [(&str, &str); 8] = [
    ("pok1leadership", "Lead"),
    ("pok2diplomacy", "Dipl"),
    ("pok3politics", "Poli"),
    ("te4construction", "Cons"),
    ("pok5trade", "Trad"),
    ("te6warfare", "Warf"),
    ("pok7technology", "Tech"),
    ("pok8imperial", "Impe"),
];

fn label(card: &str) -> &'static str {
    LABEL
        .iter()
        .find(|(id, _)| card.starts_with(&id[..7.min(id.len())]))
        .map_or("????", |(_, short)| *short)
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(
            "out/pools/save52_e400_holdout.json.gz",
        ))
        .expect("pool"),
    );
    ti4_training::rollout::set_seat_scramble(true);

    let paths: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a.ends_with(".json"))
        .collect();
    let seeds: Vec<u64> = (98_000_000..98_000_150).collect();
    let mut games = Vec::new();
    for path in &paths {
        let document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("read")).expect("parse");
        let loaded: BTreeMap<String, Profile> =
            serde_json::from_value(document["profiles"].clone()).expect("profiles");
        let profiles: BTreeMap<FactionId, Profile> = factions
            .iter()
            .map(|f| (f.clone(), loaded[f.as_str()].clone()))
            .collect();
        games.extend(play_rotated_save54_pool_batch(
            store,
            &factions,
            &profiles,
            FULL,
            &seeds,
            Horizon::opening(),
            ti4_engine::opening::DEFAULT_REQUIREMENT,
            Arc::clone(&pool),
            20_000_000,
        ));
    }

    // clearance keyed by (faction, card taken)
    let mut by_card: BTreeMap<(String, &str), (usize, usize)> = BTreeMap::new();
    // what the secondary head is even choosing between
    let mut secondary_options: BTreeMap<String, usize> = BTreeMap::new();
    // clearance against how many secondaries this seat opted into
    let mut by_uptake: BTreeMap<usize, (usize, usize)> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            let mut card = "none";
            let mut taken = 0usize;
            for step in &seat.trajectory {
                match step.head.as_str() {
                    "strategy" => card = label(&step.chosen),
                    "secondary" => {
                        *secondary_options.entry(step.chosen.clone()).or_default() += 1;
                        if !step.chosen.contains("decline") && !step.chosen.contains("pass") {
                            taken += 1;
                        }
                    }
                    _ => {}
                }
            }
            let cleared = usize::from(seat.episode.cleared);
            let row = by_card
                .entry((seat.faction.to_string(), card))
                .or_insert((0, 0));
            row.0 += 1;
            row.1 += cleared;
            let up = by_uptake.entry(taken.min(5)).or_insert((0, 0));
            up.0 += 1;
            up.1 += cleared;
        }
    }

    println!("{} games\n", games.len());
    println!("CLEARANCE BY THE CARD ACTUALLY TAKEN (n in brackets; blank = fewer than 25 samples)");
    print!("{:<10}", "faction");
    for (_, short) in LABEL {
        print!("{short:>14}");
    }
    println!();
    for faction in &factions {
        print!("{:<10}", faction.as_str());
        for (_, short) in LABEL {
            let (n, cleared) = by_card
                .get(&(faction.to_string(), short))
                .copied()
                .unwrap_or((0, 0));
            if n < 25 {
                print!("{:>14}", "-");
            } else {
                #[expect(clippy::cast_precision_loss, reason = "small counts")]
                let rate = cleared as f64 / n as f64;
                print!("{:>9.3}{:>5}", rate, format!("({n})"));
            }
        }
        println!();
    }

    println!("\nSECONDARY head: what it chooses between");
    for (option, count) in secondary_options.iter().take(12) {
        println!("  {option:<40} {count:>8}");
    }

    println!("\nCLEARANCE vs HOW MANY SECONDARIES THE SEAT OPTED INTO");
    println!("{:<12} {:>8} {:>10}", "secondaries", "seats", "clearance");
    for (taken, (n, cleared)) in &by_uptake {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let rate = *cleared as f64 / *n as f64;
        println!("{taken:<12} {n:>8} {rate:>10.3}");
    }
}
