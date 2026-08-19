//! What features does each decision head actually see?
//!
//! Written after asserting from memory that the cargo head has no facts about what it is loading
//! or where it is going. That should be checked rather than remembered, and the same question is
//! worth answering for every head at once.
//!
//! Families are grouped by name shape: `target:planet-count` and `target:resources` are two
//! members of the `target:` family, while `option:carrier` and `option:build` are two members of
//! `option:`. The families are what the representation offers; the members are how finely.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::intern::name_of;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

/// The family a feature name belongs to.
///
/// Structured families carry their own second component (`target:own-distance` is a fact about the
/// target, not a family of its own), while token families collapse (`option:carrier` is one of
/// very many `option:` members and listing each would drown the shape).
fn family(name: &str) -> String {
    const STRUCTURED: [&str; 6] = [
        "target:",
        "route:",
        "landing:",
        "invasion:",
        "pay:",
        "payload-number:",
    ];
    if STRUCTURED.iter().any(|prefix| name.starts_with(prefix)) {
        return name.to_owned();
    }
    match name.split_once(':') {
        Some((head, _)) => format!("{head}:*"),
        None => name.to_owned(),
    }
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
    let only = std::env::args().find_map(|a| a.strip_prefix("--head=").map(ToOwned::to_owned));
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .filter_map(|f| loaded.get(f.as_str()).map(|p| (f.clone(), p.clone())))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let seeds: Vec<u64> = (98_000_000..98_000_012).collect();
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

    // head -> family -> (times seen, distinct members)
    let mut heads: BTreeMap<String, BTreeMap<String, (usize, BTreeSet<String>)>> = BTreeMap::new();
    let mut decisions: BTreeMap<String, usize> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                if only.as_ref().is_some_and(|want| &step.head != want) {
                    continue;
                }
                *decisions.entry(step.head.clone()).or_default() += 1;
                let table = heads.entry(step.head.clone()).or_default();
                for vector in step.legal.values() {
                    for (slot, _) in vector {
                        let name = name_of(*slot);
                        let entry = table.entry(family(&name)).or_default();
                        entry.0 += 1;
                        if entry.1.len() < 64 {
                            entry.1.insert(name);
                        }
                    }
                }
            }
        }
    }

    for (head, families) in &heads {
        let n = decisions.get(head).copied().unwrap_or(0);
        println!("\n=== {head}  ({n} decisions) ===");
        let mut rows: Vec<(&String, &(usize, BTreeSet<String>))> = families.iter().collect();
        rows.sort_by_key(|(_, (count, _))| std::cmp::Reverse(*count));
        for (name, (count, members)) in rows {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let per = *count as f64 / n.max(1) as f64;
            if name.ends_with(":*") {
                println!("  {name:<34} {per:>7.1} per decision   {} distinct", members.len());
            } else {
                println!("  {name:<34} {per:>7.1} per decision");
            }
        }
    }
}
