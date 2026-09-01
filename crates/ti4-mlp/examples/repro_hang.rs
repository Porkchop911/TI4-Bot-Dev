//! Replay one MLP self-play seed that failed to progress, and report where it stuck.
//!
//! `cargo run --release -p ti4-mlp --example repro_hang -- --bundle out/checkpoints/run-024/checkpoint-5792 --seed 650003462 --rotation 2`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let bundle_path = argument("--bundle").expect("--bundle");
    let seed: u64 = argument("--seed").and_then(|v| v.parse().ok()).unwrap_or(650_003_462);
    let rotation: usize = argument("--rotation").and_then(|v| v.parse().ok()).unwrap_or(2);
    let steps: usize = argument("--steps").and_then(|v| v.parse().ok()).unwrap_or(10_000);

    ti4_tensor::configure_deterministic(20_260_826).expect("deterministic backend");

    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new("out/pools/full_np8_12_train.json"),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .expect("pool");
    let pool = std::sync::Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes)).expect("pool parse"),
    );

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path)).expect("bundle");
    let vocabulary = loaded.vocabulary;
    let actor = std::rc::Rc::new(loaded.actor);

    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|n| PlayerId::new(*n))
        .collect();
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (player.clone(), FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]))
        })
        .collect();

    let sweep: u64 = argument("--sweep").and_then(|v| v.parse().ok()).unwrap_or(1);
    let mut failures = 0;
    for offset in 0..sweep {
    let seed = seed + offset;
    let rollout = ti4_training::rollout::play_with_decider_factory(
        ContentStore::embedded(),
        &players,
        &seated,
        DEFAULT,
        seed,
        ti4_training::rollout::Horizon { rounds: 1, steps },
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: std::sync::Arc::clone(&pool),
            tile_seed_offset: 20_000_000,
        },
        |_baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn ti4_engine::choice::Decider>> =
                BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let row = ti4_mlp::FactionRow::of(seated[player].as_str()).expect("row");
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let bot =
                    ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream);
                let (decider, _status) = bot.seat();
                deciders.insert(player.clone(), decider);
            }
            Ok::<_, String>(deciders)
        },
    );

    if let Some(error) = &rollout.error {
        failures += 1;
        println!("seed {seed} rotation {rotation}: {error}");
    }
    }
    println!("swept {sweep} seeds, {failures} failed");
}
