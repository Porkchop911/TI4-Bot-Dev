//! Which heads cannot see what, and how much of the decision surface that covers.
//!
//! One failure mode, applied everywhere: a linear softmax cannot read a feature that carries the
//! same name and value on every option of a choice. Such a feature adds one constant to every
//! logit and cancels exactly. So for each head this reports:
//!
//! * **blind decisions** -- every option carried an identical feature vector, so no weights can
//!   order them and the policy is sampling uniformly whatever it has learned;
//! * **distinct vectors per decision** -- the ceiling on how many different answers the head can
//!   give, against how many options it is asked to rank;
//! * **state visibility** -- whether any feature on the decision reflects the seat's own position
//!   rather than the option's identity.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::intern::name_of;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

#[derive(Default)]
struct Row {
    decisions: usize,
    blind: usize,
    options: usize,
    distinct: usize,
    with_state: usize,
    features: usize,
    ids: BTreeSet<String>,
}

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
    let seeds: Vec<u64> = (98_000_000..98_000_030).collect();
    let games = play_rotated_save54_pool_batch(
        store, &factions, &profiles, FULL, &seeds,
        Horizon::opening(), ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool), 20_000_000,
    );

    let mut heads: BTreeMap<String, Row> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            for step in &seat.trajectory {
                let row = heads.entry(step.head.clone()).or_default();
                row.decisions += 1;
                row.options += step.legal.len();
                let mut vectors: BTreeSet<String> = BTreeSet::new();
                let mut sees_state = false;
                for vector in step.legal.values() {
                    let mut names: Vec<String> = vector
                        .iter()
                        .map(|(slot, value)| format!("{}={value}", name_of(*slot)))
                        .collect();
                    names.sort();
                    row.features += names.len();
                    if names
                        .iter()
                        .any(|n| n.starts_with("state-kind:") || n.starts_with("state-option:"))
                    {
                        sees_state = true;
                    }
                    vectors.insert(names.join("|"));
                }
                if row.ids.len() < 40 {
                    for id in step.legal.keys() {
                        row.ids.insert(id.clone());
                    }
                }
                row.distinct += vectors.len();
                if vectors.len() <= 1 && step.legal.len() > 1 {
                    row.blind += 1;
                }
                if sees_state {
                    row.with_state += 1;
                }
            }
        }
    }

    let total: usize = heads.values().map(|r| r.decisions).sum();
    println!("{} games, {total} decisions\n", games.len());
    println!(
        "{:<12} {:>9} {:>7} {:>7} {:>9} {:>9} {:>8}",
        "head", "decisions", "share", "blind%", "opts/dec", "distinct", "sees state"
    );
    println!("{}", "-".repeat(68));
    let mut rows: Vec<(&String, &Row)> = heads.iter().collect();
    rows.sort_by_key(|(_, r)| std::cmp::Reverse(r.decisions));
    for (head, row) in rows {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let n = row.decisions as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let (blind, opts, distinct, state, tot) = (
            row.blind as f64,
            row.options as f64,
            row.distinct as f64,
            row.with_state as f64,
            total as f64,
        );
        println!(
            "{head:<12} {:>9} {:>6.1}% {:>6.1}% {:>9.1} {:>9.2} {:>7.1}%",
            row.decisions,
            100.0 * n / tot,
            100.0 * blind / n,
            opts / n,
            distinct / n,
            100.0 * state / n
        );
    }
    println!("
OPTION IDS of the heads that carry no state at all (sample):");
    for (head, row) in &heads {
        if row.with_state * 100 < row.decisions && row.decisions > 100 {
            let sample: Vec<&str> = row.ids.iter().map(String::as_str).take(6).collect();
            println!("  {head:<12} {}", sample.join(", "));
        }
    }

    println!("\nblind%   = every option had an identical vector; no weights can order them");
    println!("distinct = how many different options the head can actually tell apart");
    println!("sees state = any state-kind/state-option feature present on the decision");
}
