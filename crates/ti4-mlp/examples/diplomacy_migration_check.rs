//! Check a real checkpoint's in-memory v10→v11 migration and measure structured diplomacy's cost.
//!
//! Nothing is written. The checkpoint is loaded twice: once as it is, and once migrated in memory
//! (vocabulary v10→v11, the input rows it addresses, and the appended diplomacy head).
//!
//! 1. **Identity.** Both play the same seeded, greedy six-seat tables with diplomacy off. Every
//!    offered option set and every choice must match, or the migration changed the policy.
//! 2. **Cost.** The migrated actor plays the same tables with diplomacy on. Decisions, engine steps,
//!    wall time, and state and journal size are reported beside the diplomacy-off run.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example diplomacy_migration_check -- \
//!   --bundle out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280 \
//!   --seeds 3 --rounds 3
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
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

/// One decision as the table recorded it: who chose, what was offered, what was taken.
type Trace = Vec<(String, Vec<String>, String)>;

struct Played {
    trace: Trace,
    steps: usize,
    seconds: f64,
    diplomacy_decisions: usize,
    contacts: usize,
    max_diplomacy_options: usize,
    state_bytes: usize,
    journal_events: usize,
    journal_bytes: usize,
}

#[expect(clippy::too_many_arguments, reason = "one call site, all required")]
fn play(
    content: &'static ContentStore,
    actor: &Rc<ti4_mlp::Actor>,
    vocabulary: &Vocabulary,
    seed: u64,
    rounds: u32,
    diplomacy: bool,
    max_steps: usize,
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
    let mut game = setup_game_with_capabilities_and_decider_factory(
        content,
        &players,
        &seated,
        DEFAULT,
        seed,
        &OpeningMap::RustVaried,
        SimulationCapabilities { diplomacy },
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
                let (decider, _status) =
                    ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                        .at_temperature(0.001)
                        .from_setup(baseline)
                        .seat();
                deciders.insert(player.clone(), decider);
            }
            Ok(deciders)
        },
    )?;

    let target = game.state.round + rounds;
    let started = Instant::now();
    let mut steps = 0_usize;
    while game.state.round < target && !game.state.finished {
        let result = game.step();
        if let Some(error) = result.error {
            return Err(format!("seed {seed}, step {steps}: {error:?}"));
        }
        steps += 1;
        if steps >= max_steps {
            return Err(format!(
                "seed {seed} did not finish {rounds} rounds within {max_steps} steps"
            ));
        }
    }
    let seconds = started.elapsed().as_secs_f64();

    let records = &game.table.log.records;
    // The negotiation window's own subtypes only: the Diplomacy strategy card also asks questions
    // whose subtypes start with "diplomacy_".
    let is_diplomacy = |record: &&ti4_engine::choice::DecisionRecord| {
        record.context.as_ref().is_some_and(|context| {
            matches!(
                context.subtype.as_str(),
                "diplomacy_offer" | "diplomacy_response"
            )
        })
    };
    Ok(Played {
        trace: records
            .iter()
            .map(|record| {
                (
                    record.player.to_string(),
                    record.offered.clone(),
                    record.chosen.clone(),
                )
            })
            .collect(),
        steps,
        seconds,
        diplomacy_decisions: records.iter().filter(is_diplomacy).count(),
        contacts: records
            .iter()
            .filter(|record| record.chosen.starts_with("component|diplomacy|"))
            .count(),
        max_diplomacy_options: records
            .iter()
            .filter(is_diplomacy)
            .map(|record| record.offered.len())
            .max()
            .unwrap_or(0),
        state_bytes: serde_json::to_vec(&game.state).map_or(0, |bytes| bytes.len()),
        journal_events: game.state.diplomacy.journal.len(),
        journal_bytes: serde_json::to_vec(&game.state.diplomacy.journal)
            .map_or(0, |bytes| bytes.len()),
    })
}

fn load(path: &Path, migrate: bool) -> (Rc<ti4_mlp::Actor>, Vocabulary) {
    let loaded = ti4_mlp::bundle::read(path)
        .unwrap_or_else(|error| refuse(&format!("{}: {error}", path.display())));
    if !migrate {
        return (Rc::new(loaded.actor), loaded.vocabulary);
    }
    let migrated = ti4_mlp::bundle::migrate_v10_to_v11(loaded)
        .unwrap_or_else(|error| refuse(&format!("migration: {error}")));
    let ti4_mlp::bundle::Loaded {
        actor, vocabulary, ..
    } = migrated;
    (Rc::new(actor.with_diplomacy_head()), vocabulary)
}

/// Where two traces first differ, if they do.
fn divergence(left: &Trace, right: &Trace) -> Option<String> {
    if let Some(index) = left.iter().zip(right).position(|(a, b)| a != b) {
        return Some(format!(
            "decision {index}: original {:?} vs migrated {:?}",
            left[index], right[index]
        ));
    }
    (left.len() != right.len()).then(|| {
        format!(
            "length: original {} vs migrated {} decisions",
            left.len(),
            right.len()
        )
    })
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts and byte sizes are small"
)]
fn main() {
    let bundle = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seeds: u64 = number("--seeds", 3);
    let seed_base: u64 = number("--seed-base", 910_000_000);
    let rounds: u32 = number("--rounds", 3);
    let max_steps: usize = number("--max-steps", 60_000);
    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));
    let content = ContentStore::embedded();
    let path = Path::new(&bundle);
    let (original, original_vocabulary) = load(path, false);
    let (migrated, migrated_vocabulary) = load(path, true);
    println!(
        "bundle {bundle}\n  original: {} heads, capacity {}; migrated: {} heads, capacity {}",
        original.head_names().len(),
        original.capacity(),
        migrated.head_names().len(),
        migrated.capacity()
    );

    let mut identical = true;
    let mut totals = [(0_usize, 0_usize, 0.0_f64, 0_usize); 2];
    println!(
        "\n  seed         mode        decisions  steps  seconds  dipl.dec  contacts  max-opts  state KB  journal ev  journal KB"
    );
    for seed in seed_base..seed_base + seeds {
        let run = |actor, vocabulary, diplomacy| {
            play(
                content, actor, vocabulary, seed, rounds, diplomacy, max_steps,
            )
            .unwrap_or_else(|error| refuse(&error))
        };
        let before = run(&original, &original_vocabulary, false);
        let after = run(&migrated, &migrated_vocabulary, false);
        if let Some(difference) = divergence(&before.trace, &after.trace) {
            identical = false;
            println!("  {seed}  MIGRATION CHANGED PLAY: {difference}");
        }
        let on = run(&migrated, &migrated_vocabulary, true);
        for (index, (mode, played)) in [("diplomacy off", &after), ("diplomacy on", &on)]
            .into_iter()
            .enumerate()
        {
            println!(
                "  {seed}  {mode:<13} {:>9}  {:>5}  {:>7.2}  {:>8}  {:>8}  {:>8}  {:>8.1}  {:>10}  {:>10.1}",
                played.trace.len(),
                played.steps,
                played.seconds,
                played.diplomacy_decisions,
                played.contacts,
                played.max_diplomacy_options,
                played.state_bytes as f64 / 1024.0,
                played.journal_events,
                played.journal_bytes as f64 / 1024.0,
            );
            let total = &mut totals[index];
            total.0 += played.trace.len();
            total.1 += played.steps;
            total.2 += played.seconds;
            total.3 += played.state_bytes;
        }
    }

    let [off, on] = totals;
    println!(
        "\n  identity (diplomacy off, original vs migrated): {}",
        if identical { "IDENTICAL" } else { "DIFFERS" }
    );
    println!(
        "  diplomacy on vs off: decisions x{:.2}, steps x{:.2}, wall time x{:.2}, seconds/decision x{:.2}, final state x{:.2}",
        on.0 as f64 / off.0.max(1) as f64,
        on.1 as f64 / off.1.max(1) as f64,
        on.2 / off.2.max(f64::EPSILON),
        (on.2 / on.0.max(1) as f64) / (off.2 / off.0.max(1) as f64).max(f64::EPSILON),
        on.3 as f64 / off.3.max(1) as f64,
    );
    if !identical {
        std::process::exit(1);
    }
}
