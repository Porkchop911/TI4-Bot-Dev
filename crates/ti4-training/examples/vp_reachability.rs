//! Which victory-point sources are actually reachable in simulated play?
//!
//! An award site existing in the engine is not the same as a seat ever being offered the decision
//! that triggers it. This plays long games and tallies the prompts and options seen, so a source
//! that is implemented but never surfaced shows up as a zero.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

/// The VP sources humans actually use, from 5,881 recorded games, with the words that would
/// appear in a prompt or option if the decision were ever offered.
const SOURCES: [(&str, &[&str]); 9] = [
    ("public objective", &["public objective", "score a public"]),
    ("secret objective", &["secret"]),
    ("custodians / Mecatol", &["custodian", "mecatol"]),
    ("Support for the Throne (option id \"ss\")", &["ss"]),
    ("Shard of the Throne", &["shard"]),
    ("Crown of Emphidia", &["emphidia", "crown"]),
    ("Imperial Rider", &["rider"]),
    (
        "agenda VP (Mutiny/Seed/Censure)",
        &["mutiny", "seed of an empire", "censure"],
    ),
    ("Imperial (strategy card)", &["imperial"]),
];

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    ti4_training::rollout::set_seat_scramble(true);
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(8);
    let path = std::env::args()
        .find(|a| a.ends_with(".json"))
        .unwrap_or_else(|| "out/stage2_ppo/s0.json".to_owned());
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();

    let seeds: Vec<u64> = (98_000_000..98_000_040).collect();
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

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut heads: BTreeMap<String, usize> = BTreeMap::new();
    let mut vp = 0.0;
    let mut seats = 0usize;
    let mut max_round = 0u32;
    for game in &games {
        for seat in &game.seats {
            seats += 1;
            #[expect(clippy::cast_precision_loss, reason = "VP are tiny")]
            let points = seat.episode.final_progress.victory_points as f64;
            vp += points;
            for step in &seat.trajectory {
                max_round = max_round.max(step.progress.round_number);
                *heads.entry(step.head.clone()).or_default() += 1;
                seen.insert(step.head.to_lowercase());
                for option in step.legal.keys() {
                    seen.insert(option.to_lowercase());
                    if option == "ss" {
                        *heads
                            .entry("[offered:support-swap]".to_owned())
                            .or_default() += 1;
                    }
                }
                if step.chosen == "ss" {
                    *heads.entry("[CHOSEN:support-swap]".to_owned()).or_default() += 1;
                }
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let n = seats as f64;
    println!(
        "{} games at {rounds} rounds (reached round {max_round}), {seats} seats, mean {:.2} VP\n",
        games.len(),
        vp / n
    );
    println!("VP SOURCE REACHABILITY -- does any decision ever mention it?");
    for (label, needles) in SOURCES {
        let hit = seen
            .iter()
            .any(|text| needles.iter().any(|needle| text.contains(needle)));
        println!(
            "  {:<34} {}",
            label,
            if hit { "reachable" } else { "NEVER OFFERED" }
        );
    }
    println!("\nheads exercised: {}", heads.len());
    let mut rows: Vec<(&String, &usize)> = heads.iter().collect();
    rows.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    println!(
        "  {}",
        rows.iter()
            .map(|(h, c)| format!("{h}:{c}"))
            .collect::<Vec<_>>()
            .join("  ")
    );
}
