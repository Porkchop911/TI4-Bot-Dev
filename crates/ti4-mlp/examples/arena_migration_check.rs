//! Check that an arena migration left play unchanged, and what the battle facts cost.
//!
//! The source and the migrated bundle play the same seeded, near-greedy six-seat tables. The new
//! input rows are zero, so every offered option set and every choice must match. The migrated run
//! also reports how many battle facts it emitted (the extra assigned feature occurrences) and the
//! wall-time ratio.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example arena_migration_check -- \
//!   --source <checkpoint> --arena <checkpoint-arena> --seeds 3 --rounds 4
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;
use ti4_training::rollout::{
    OpeningMap, SimulationCapabilities, seated_faction,
    setup_game_with_capabilities_and_decider_factory,
};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn number<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} expects a number, got {value}")))
    })
}

fn refuse(message: &str) -> ! {
    eprintln!("REFUSED: {message}");
    std::process::exit(2)
}

type Trace = Vec<(String, Vec<String>, String)>;

struct Played {
    trace: Trace,
    seconds: f64,
    assigned: usize,
    movement: usize,
}

fn play(
    content: &'static ContentStore,
    actor: &Rc<ti4_mlp::Actor>,
    vocabulary: &Vocabulary,
    seed: u64,
    rounds: u32,
) -> Result<Played, String> {
    let factions = FACTIONS.map(FactionId::new);
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let seated: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.clone(), seated_faction(&factions, seed, 0, index)))
        .collect();
    let mut statuses = Vec::new();
    let mut game = setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &seated,
        DEFAULT,
        seed,
        &OpeningMap::RustVaried,
        SimulationCapabilities { diplomacy: true },
        |baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let (decider, status) =
                    ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                        .at_temperature(0.001)
                        .from_setup(baseline)
                        .seat();
                statuses.push(status);
                deciders.insert(player.clone(), decider);
            }
            Ok(deciders)
        },
    )?;
    let target = game.state.round + rounds;
    let started = Instant::now();
    let mut steps = 0_usize;
    while game.state.round < target && !game.state.finished {
        if let Some(error) = game.step().error {
            return Err(format!("seed {seed}, step {steps}: {error:?}"));
        }
        steps += 1;
        if steps >= 80_000 {
            return Err(format!("seed {seed} did not finish {rounds} rounds"));
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    let records = &game.table.log.records;
    Ok(Played {
        trace: records
            .iter()
            .map(|r| (r.player.to_string(), r.offered.clone(), r.chosen.clone()))
            .collect(),
        seconds,
        assigned: statuses
            .iter()
            .map(|status| status.counters().assigned.load(Ordering::Relaxed))
            .sum(),
        movement: records
            .iter()
            .filter(|r| r.offered.iter().any(|id| id == "done_moving"))
            .count(),
    })
}

fn load(path: &str) -> (Rc<ti4_mlp::Actor>, Vocabulary) {
    let loaded = ti4_mlp::bundle::read(Path::new(path))
        .unwrap_or_else(|error| refuse(&format!("{path}: {error}")));
    (Rc::new(loaded.actor), loaded.vocabulary)
}

#[expect(clippy::cast_precision_loss, reason = "counts are small")]
fn main() {
    let source = argument("--source").unwrap_or_else(|| refuse("--source is required"));
    let arena = argument("--arena").unwrap_or_else(|| refuse("--arena is required"));
    let seeds: u64 = number("--seeds", 3);
    let seed_base: u64 = number("--seed-base", 917_000_000);
    let rounds: u32 = number("--rounds", 4);
    ti4_tensor::configure_deterministic(20_260_917)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));
    let content = ContentStore::embedded();
    let (plain, plain_vocabulary) = load(&source);
    let (armed, armed_vocabulary) = load(&arena);
    if plain.battle_predictor().is_some() || armed.battle_predictor().is_none() {
        refuse("--source must be a plain bundle and --arena an arena-capable one");
    }

    let mut identical = true;
    let (mut t_plain, mut t_arena, mut decisions, mut facts, mut movement) = (0.0, 0.0, 0, 0, 0);
    println!("  seed        decisions  movement  battle facts  plain s  arena s");
    for seed in seed_base..seed_base + seeds {
        let before = play(content, &plain, &plain_vocabulary, seed, rounds)
            .unwrap_or_else(|error| refuse(&error));
        let after = play(content, &armed, &armed_vocabulary, seed, rounds)
            .unwrap_or_else(|error| refuse(&error));
        if let Some(index) = before
            .trace
            .iter()
            .zip(&after.trace)
            .position(|(a, b)| a != b)
        {
            identical = false;
            println!(
                "  {seed}  MIGRATION CHANGED PLAY at decision {index}: {:?} vs {:?}",
                before.trace[index], after.trace[index]
            );
        } else if before.trace.len() != after.trace.len() {
            identical = false;
            println!("  {seed}  MIGRATION CHANGED PLAY: trace lengths differ");
        }
        let extra = after.assigned.saturating_sub(before.assigned);
        println!(
            "  {seed}  {:>9}  {:>8}  {:>12}  {:>7.2}  {:>7.2}",
            after.trace.len(),
            after.movement,
            extra,
            before.seconds,
            after.seconds
        );
        t_plain += before.seconds;
        t_arena += after.seconds;
        decisions += after.trace.len();
        facts += extra;
        movement += after.movement;
    }
    println!(
        "\n  identity: {}",
        if identical { "IDENTICAL" } else { "DIFFERS" }
    );
    println!(
        "  {decisions} decisions, {movement} movement decisions, {facts} battle facts emitted; wall time x{:.3}",
        t_arena / t_plain.max(f64::EPSILON)
    );
    if !identical {
        std::process::exit(1);
    }
}
