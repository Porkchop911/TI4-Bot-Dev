//! What words the accepting options on the `secondary` head actually carry.
//!
//! Every secondary table in this programme classified a follow by looking for a verb in the
//! option's feature words. Those words come from the option id and label only (features.rs:290),
//! never the prompt, so a card whose label does not contain the expected verb is invisible. This
//! dumps the raw word sets so the mapping can be built from what is there rather than guessed.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "out/pools/save52_e400_holdout.json.gz";

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let path = std::env::args()
        .skip(1)
        .find(|a| a.ends_with(".json"))
        .expect("checkpoint path");
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(4);
    let seeds: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(40);
    ti4_training::rollout::set_seat_scramble(true);

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let seed_block: Vec<u64> = (98_000_000..98_000_000 + seeds).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seed_block,
        Horizon::rounds(rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    // The full legal set of a secondary decision, as words, with how often it was offered and how
    // often each option in it was taken.
    let mut offered: BTreeMap<String, usize> = BTreeMap::new();
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                if step.head != "secondary" {
                    continue;
                }
                let words = |id: &String| -> String {
                    step.legal.get(id).map_or_else(String::new, |vector| {
                        let set: BTreeSet<String> = vector
                            .iter()
                            .filter_map(|(slot, _)| {
                                ti4_policy::intern::name_of(*slot)
                                    .strip_prefix("option:")
                                    .map(str::to_owned)
                            })
                            .collect();
                        set.into_iter().collect::<Vec<_>>().join("+")
                    })
                };
                let signature: Vec<String> = step.legal.keys().map(&words).collect();
                *offered.entry(signature.join("  |  ")).or_default() += 1;
                *taken.entry(words(&step.chosen)).or_default() += 1;
            }
        }
    }

    println!("{} games, secondary head\n", games.len());
    println!("LEGAL SETS OFFERED (option word-sets, joined)");
    let mut rows: Vec<_> = offered.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (signature, count) in rows.into_iter().take(20) {
        println!("  {count:>7}  {signature}");
    }

    println!("\nOPTIONS CHOSEN (word-set of the taken option)");
    let mut rows: Vec<_> = taken.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (signature, count) in rows.into_iter().take(25) {
        println!("  {count:>7}  {signature}");
    }
}
