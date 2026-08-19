//! What does a PPO epoch cost, against the rollout that produced the batch?
//!
//! The whole case for PPO here rests on the answer. Simulation is essentially all of this
//! project's compute, and an epoch after the first re-simulates nothing: it re-reads recorded
//! feature vectors, re-scores them under the current weights, and accumulates. If an epoch is a
//! small fraction of a rollout, then K epochs cost far less than K batches and PPO buys gradient
//! steps at a large discount. If it is not, PPO is just a slower way to spend the same budget.

use std::collections::BTreeMap;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_engine::opening::DEFAULT_REQUIREMENT;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::ppo::{self, PpoStep};
use ti4_training::reward::Reward;
use ti4_training::rollout::{Horizon, play_rotated_batch};

const CHECKPOINT: &str = "out/run_pure_u5000.json";

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(CHECKPOINT).expect("checkpoint")).expect("json");
    let profiles: BTreeMap<FactionId, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");

    let reward = Reward::default();
    // 16 seeds x 6 rotations: one production training batch.
    let seeds: Vec<u64> = (0..16).collect();

    let started = Instant::now();
    let batch = play_rotated_batch(
        &store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::rounds(4),
        DEFAULT_REQUIREMENT,
    );
    let rollout = started.elapsed().as_secs_f64();
    println!("rollout of {} games: {rollout:.3} s", batch.len());

    for epochs in [1_usize, 2, 4, 8] {
        let mut working = profiles.clone();
        let started = Instant::now();
        let report = ppo::update(&mut working, &batch, &reward, PpoStep {
            learning_rate: 0.03,
            entropy: 0.05,
            gradient_clip: 1.0,
            clip: 0.2,
            epochs,
            positive_only: false,
            draft_entropy: 0.0,
        });
        let spent = started.elapsed().as_secs_f64();
        #[expect(clippy::cast_precision_loss, reason = "epoch counts are tiny")]
        let each = spent / epochs as f64;
        // What the same number of gradient steps would have cost as separate REINFORCE batches.
        #[expect(clippy::cast_precision_loss, reason = "epoch counts are tiny")]
        let reinforce = (rollout + each) * epochs as f64;
        println!(
            "K={epochs}: {spent:.3} s total, {each:.3} s/epoch, batch+K epochs = {:.3} s \
             vs {reinforce:.3} s for K REINFORCE batches ({:.2}x cheaper per step)",
            rollout + spent,
            reinforce / (rollout + spent)
        );
        // The clip fraction says whether the later epochs are still doing anything.
        for (index, epoch) in report.iter().enumerate() {
            let rows: Vec<f64> = epoch
                .values()
                .flat_map(|heads| heads.values().map(|(_, clip)| clip.clip_fraction))
                .collect();
            if rows.is_empty() {
                continue;
            }
            #[expect(clippy::cast_precision_loss, reason = "head counts are small")]
            let mean = rows.iter().sum::<f64>() / rows.len() as f64;
            let worst = rows.iter().copied().fold(0.0_f64, f64::max);
            println!("    epoch {index}: clip fraction mean {mean:.4} worst {worst:.4}");
        }
    }
}
