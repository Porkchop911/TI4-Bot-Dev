//! Does PPO's first epoch reproduce REINFORCE?
//!
//! It must. At epoch zero the current policy *is* the behaviour policy, so every importance
//! ratio is exactly one and the clipped surrogate's gradient collapses to the ordinary
//! policy-gradient one. If the two disagree here, PPO is not REINFORCE-plus-reuse -- it is a
//! different update with a bug in it, and every later comparison would be measuring the bug.
//!
//! The agreement is numerical rather than bit-level: REINFORCE defers the centring mean and so
//! accumulates `sum credit*dphi` and `sum dphi` separately, while PPO knows the mean up front and
//! weights each decision as it goes. Same value, different summation order.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::opening::DEFAULT_REQUIREMENT;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::gradient::{self, Statistics, Step};
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
    let seeds: Vec<u64> = (0..4).collect();
    let batch = play_rotated_batch(
        &store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::rounds(4),
        DEFAULT_REQUIREMENT,
    );
    println!("{} games retained", batch.len());

    // REINFORCE, exactly as the trainer does it.
    let mut reinforce = profiles.clone();
    let mut collected: BTreeMap<FactionId, BTreeMap<String, Statistics>> = BTreeMap::new();
    for rollout in &batch {
        if rollout.error.is_some() {
            continue;
        }
        for seat in &rollout.seats {
            let Some(profile) = profiles.get(&seat.faction) else {
                continue;
            };
            let rows = gradient::statistics(&seat.trajectory, &seat.episode, profile, &reward);
            let target = collected.entry(seat.faction.clone()).or_default();
            for (head, row) in rows {
                target.entry(head).or_default().merge(&row);
            }
        }
    }
    let step = Step {
        learning_rate: 0.03,
        entropy: 0.05,
        gradient_clip: 1.0,
    };
    for (faction, heads) in &collected {
        if let Some(profile) = reinforce.get_mut(faction) {
            gradient::apply(profile, heads, step);
        }
    }

    // PPO with one epoch and a clip wide enough that it cannot bind.
    let mut ppo_profiles = profiles.clone();
    ppo::update(
        &mut ppo_profiles,
        &batch,
        &reward,
        PpoStep {
            learning_rate: step.learning_rate,
            entropy: step.entropy,
            gradient_clip: step.gradient_clip,
            clip: 1e9,
            epochs: 1,
            positive_only: false,
        },
    );

    // Compare the weight *deltas*, not the weights: the weights are large and nearly equal, and
    // comparing them would hide a disagreement in the thing being tested.
    let (mut worst, mut worst_where, mut compared, mut nonzero) = (0.0_f64, String::new(), 0, 0);
    for (faction, base) in &profiles {
        let (Some(left), Some(right)) = (reinforce.get(faction), ppo_profiles.get(faction)) else {
            continue;
        };
        for head in base.learned.heads.keys() {
            let (Some(b), Some(l), Some(r)) = (base.head(head), left.head(head), right.head(head))
            else {
                continue;
            };
            for name in l.weights.keys().chain(r.weights.keys()) {
                let start = b.weights.get(name).copied().unwrap_or(0.0);
                let a = l.weights.get(name).copied().unwrap_or(0.0) - start;
                let c = r.weights.get(name).copied().unwrap_or(0.0) - start;
                compared += 1;
                if a.abs() > 1e-18 {
                    nonzero += 1;
                }
                let scale = a.abs().max(c.abs()).max(1e-30);
                let relative = (a - c).abs() / scale;
                if relative > worst {
                    worst = relative;
                    worst_where = format!("{faction}/{head}/{name}: {a:.6e} vs {c:.6e}");
                }
            }
        }
    }
    println!("{compared} weight deltas compared, {nonzero} of them nonzero");
    println!("worst relative disagreement: {worst:.3e}");
    if !worst_where.is_empty() {
        println!("  at {worst_where}");
    }
    println!(
        "{}",
        if worst < 1e-9 {
            "PARITY HELD -- PPO epoch 0 is REINFORCE"
        } else {
            "PARITY BROKEN"
        }
    );
}
