//! Can the policy tell one strategy card's secondary from another's?
//!
//! The `secondary` head is a yes/no, but "yes" means something completely different depending on
//! which card is on offer -- Leadership's secondary buys command tokens, Construction's places a
//! structure, Technology's buys research. If the feature vector for "yes" is the same whichever
//! card is being offered, then one weight vector must answer all of them identically and the head
//! is answering a question it cannot see.
//!
//! Prints the actual features of real secondary decisions, then counts how many distinct vectors
//! the head ever sees. Distinct vectors are the ceiling on how many different answers it can give.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::intern::name_of;
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
    ti4_training::rollout::set_seat_scramble(true);
    let head_name = std::env::args()
        .find(|a| a.starts_with("--head="))
        .map_or_else(|| "secondary".to_owned(), |a| a[7..].to_owned());
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/prod2/stage1_ppo_s0.json".to_owned());
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();
    let seeds: Vec<u64> = (98_000_000..98_000_020).collect();
    let games = play_rotated_save54_pool_batch(
        store, &factions, &profiles, FULL, &seeds,
        Horizon::opening(), ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool), 20_000_000,
    );

    // Four sample decisions, printed in full.
    let mut shown = 0;
    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                if step.head != head_name || shown >= 2 {
                    continue;
                }
                shown += 1;
                println!("--- secondary decision {shown} (seat {}, chose {})", seat.player, step.chosen);
                for (option, vector) in &step.legal {
                    let mut names: Vec<String> = vector
                        .iter()
                        .map(|(slot, value)| format!("{}={value}", name_of(*slot)))
                        .collect();
                    names.sort();
                    println!("  option {option:<5} {} features", names.len());
                    for name in &names {
                        println!("      {name}");
                    }
                }
            }
        }
    }

    // Do the crossed seat facts actually VARY at this head's decisions?
    //
    // Crossing a fact that never changes is worse than useless: it is perfectly collinear with the
    // option-identity feature it is crossed with, so it adds no information and multiplies the
    // gradient along that one direction by the sum of the squared fact values.
    let mut fact_values: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                if step.head != head_name {
                    continue;
                }
                for vector in step.legal.values() {
                    for (slot, value) in vector {
                        let name = name_of(*slot);
                        let Some(rest) = name
                            .strip_prefix("state-option:")
                            .or_else(|| name.strip_prefix("state-kind:"))
                        else {
                            continue;
                        };
                        let Some((_, fact)) = rest.split_once(':') else {
                            continue;
                        };
                        *fact_values
                            .entry(fact.to_owned())
                            .or_default()
                            .entry(format!("{value}"))
                            .or_default() += 1;
                    }
                }
            }
        }
    }
    println!("
crossed seat facts at every {head_name} decision, and how often each value appears:");
    for (fact, counts) in &fact_values {
        let distinct = counts.len();
        let mut shown: Vec<String> = counts
            .iter()
            .map(|(value, count)| format!("{value}x{count}"))
            .collect();
        shown.truncate(6);
        let flag = if distinct == 1 { "  <-- CONSTANT" } else { "" };
        println!("  {fact:<20} {distinct:>3} distinct  {}{flag}", shown.join(" "));
    }

    // How many distinct vectors does the head ever see?
    let mut distinct: BTreeSet<String> = BTreeSet::new();
    let mut distinct_yes: BTreeSet<String> = BTreeSet::new();
    let mut total = 0usize;
    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                if step.head != head_name {
                    continue;
                }
                total += 1;
                for (option, vector) in &step.legal {
                    let mut names: Vec<String> = vector
                        .iter()
                        .map(|(slot, value)| format!("{}={value}", name_of(*slot)))
                        .collect();
                    names.sort();
                    let key = names.join("|");
                    distinct.insert(key.clone());
                    if option == "yes" {
                        distinct_yes.insert(key);
                    }
                }
            }
        }
    }
    println!("\n{total} secondary decisions over {} games", games.len());
    println!("distinct option vectors seen by the head: {}", distinct.len());
    println!("distinct vectors for the 'yes' option    : {}", distinct_yes.len());
    println!(
        "\nIf 'yes' has only a handful of distinct vectors, the head cannot be answering\n\
         'is THIS secondary worth it' -- there are eight different secondaries."
    );
}
