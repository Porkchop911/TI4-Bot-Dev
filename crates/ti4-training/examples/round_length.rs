//! Does removing trained opponents shorten the round?
//!
//! Replacing five seats with blank policies made every faction's clearance *fall*, which reads as
//! "opponents help". But there is a confound that would produce the same reading for a duller
//! reason: the action phase runs until every player passes, so opponents that pass early end the
//! round early and cost the remaining seat its later turns. That is not cooperation, it is a
//! shorter game.
//!
//! Decisions taken per seat separates them. If the trained seat takes roughly as many decisions in
//! both conditions, the round length is not the explanation and the interaction is real.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

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
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read("out/prod/stage1_ppo_s0.json").expect("read"))
            .expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let trained: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();
    let blank: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), blank_explicit_profile(f.as_str())))
        .collect();

    let seeds: Vec<u64> = (98_000_000..98_000_040).collect();
    let target = FactionId::new("letnev"); // the largest clearance drop of the six

    for (label, profiles) in [
        ("all six trained", trained.clone()),
        ("letnev trained, five blank", {
            let mut mixed = blank.clone();
            mixed.insert(target.clone(), trained[&target].clone());
            mixed
        }),
    ] {
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
        let (mut target_steps, mut target_seats, mut all_steps, mut all_seats) = (0, 0, 0, 0);
        for game in &games {
            for seat in &game.seats {
                all_steps += seat.trajectory.len();
                all_seats += 1;
                if seat.faction == target {
                    target_steps += seat.trajectory.len();
                    target_seats += 1;
                }
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let (ts, tn) = (target_steps as f64, target_seats.max(1) as f64);
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let (as_, an) = (all_steps as f64, all_seats.max(1) as f64);
        println!(
            "{label:<28} letnev decisions/seat {:>7.1}   table decisions/seat {:>7.1}",
            ts / tn,
            as_ / an
        );
    }
}
