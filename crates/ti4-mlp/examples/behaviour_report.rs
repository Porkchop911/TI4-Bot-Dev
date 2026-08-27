//! What the policy actually does: strategy card picks and secondary participation.
//!
//! The training driver reports outcomes — clearance, victory points — because those are what the
//! reward is made of. They say nothing about *how* a seat got there. This asks the two behavioural
//! questions that keep coming up: which strategy card does each faction take, and how often does it
//! follow someone else's secondary.
//!
//! Inference is CPU-only under §7.1, so this needs no GPU and can run beside a training job.
//!
//! # What is and is not attributable
//!
//! Strategy picks are read from the **final state**: `PlayerState::strategy_cards` holds what each
//! seat took, and the seat-to-faction assignment turns that into a per-faction tally.
//!
//! Secondary participation is read from the **event log**, which carries event *names* and nothing
//! else — `game.events` is a `Vec<String>` of types, with the payload consumed by the rules engine
//! rather than retained. So follow/decline is a table-level rate, not a per-faction one. Reporting
//! it per faction would require the engine to retain event payloads; inventing an attribution here
//! would be worse than saying so.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// One faction's behaviour across the sampled games.
#[derive(Default)]
struct Tally {
    games: usize,
    cards: BTreeMap<String, usize>,
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| {
        refuse("--bundle is required: the report describes a specific checkpoint")
    });
    let seeds: u64 = argument("--seeds").map_or(200, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(690_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    let rounds: u32 = argument("--rounds").map_or(1, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--rounds expects a positive integer"))
    });

    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = std::rc::Rc::new(
        loaded
            .actor
            .inference_copy()
            .to_device(ti4_tensor::Device::Cpu),
    );

    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    let content = ContentStore::embedded();
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("MLP behaviour report");
    println!("  bundle      {bundle_path}");
    println!(
        "  sample      {seeds} seeds x {} rotations, {rounds} round(s)",
        FACTIONS.len()
    );

    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    let mut followed = 0usize;
    let mut declined = 0usize;
    let mut games = 0usize;

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let (events, state, assignments) = ti4_training::rollout::audit_game_with_deciders(
                content,
                &factions,
                DEFAULT,
                seed,
                rotation,
                ti4_training::rollout::Horizon {
                    rounds,
                    steps: 200_000,
                },
                &ti4_training::rollout::OpeningMap::PythonPool {
                    pool: Arc::clone(&pool),
                    tile_seed_offset: TILE_SEED_OFFSET,
                },
                |seated| {
                    let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                    for (index, (player, faction)) in seated.iter().enumerate() {
                        let row = ti4_mlp::FactionRow::of(faction.as_str())
                            .map_err(|error| format!("{player}: {error}"))?;
                        let stream = seed
                            .wrapping_mul(1_000_003)
                            .wrapping_add(u64::try_from(index).unwrap_or(0));
                        let (decider, _status) =
                            ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream)
                                .seat();
                        deciders.insert(player.clone(), decider);
                    }
                    Ok(deciders)
                },
            )
            .unwrap_or_else(|error| refuse(&error));

            games += 1;
            for event in &events {
                match event.as_str() {
                    "STRATEGY_SECONDARY_FOLLOWED" => followed += 1,
                    "STRATEGY_SECONDARY_DECLINED" => declined += 1,
                    _ => {}
                }
            }
            for player in &state.players {
                let Some(faction) = assignments.get(&player.id) else {
                    continue;
                };
                let tally = tallies.entry(faction.to_string()).or_default();
                tally.games += 1;
                for card in &player.strategy_cards {
                    *tally.cards.entry(card.to_string()).or_default() += 1;
                }
            }
        }
    }

    if games == 0 {
        refuse("no games were played");
    }

    // Every card that appeared anywhere, so the table has stable columns.
    let mut every_card: Vec<String> = tallies
        .values()
        .flat_map(|tally| tally.cards.keys().cloned())
        .collect();
    every_card.sort_unstable();
    every_card.dedup();

    println!("\n  strategy card picks, share of that faction's games\n");
    print!("  {:<10}", "faction");
    for card in &every_card {
        print!(" {:>10}", truncate(card, 10));
    }
    println!();
    for (faction, tally) in &tallies {
        print!("  {faction:<10}");
        for card in &every_card {
            let count = tally.cards.get(card).copied().unwrap_or(0);
            print!(" {:>9.1}%", share(count, tally.games));
        }
        println!();
    }

    println!("\n  secondaries (table-level; the event log carries no player)\n");
    let offered = followed + declined;
    println!("    offered   {offered}");
    println!(
        "    followed  {followed} ({:.1}%)",
        share(followed, offered)
    );
    println!(
        "    declined  {declined} ({:.1}%)",
        share(declined, offered)
    );
    println!("    per game  {:.2} offered", ratio(offered, games));
}

fn truncate(text: &str, width: usize) -> String {
    text.chars().take(width).collect()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts are exact in f64 far beyond any sample size"
)]
fn share(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64 * 100.0
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts are exact in f64 far beyond any sample size"
)]
fn ratio(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64
}
