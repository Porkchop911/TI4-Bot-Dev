//! Does wrapping a seat's decider to watch it change what it does?
//!
//! The wasted-activation penalty needs a record of what each seat chose, and that record comes from
//! a `Watching` decider that wraps every seat in every PPO rollout. It delegates and records, so it
//! should be inert — but "should be" is not a measurement, and it is new code in the hot path of
//! every training run whose numbers are being compared against runs that predate it.
//!
//! So: play the same games twice, once with the wrapper and once without, and require the seats to
//! make **identical** decisions. The engine is deterministic given a seed and the bot samples from a
//! seeded stream, so this is an exact equality, not a tolerance.
//!
//! The comparison uses the bot's own PPO records rather than the wrapper's log, because those exist
//! on both passes and are produced by machinery the wrapper does not touch. Comparing the wrapper's
//! log against itself would prove nothing.
//!
//! Exits non-zero on any difference.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example wrapper_identity -- \
//!   --bundle out/checkpoints/mixed/epoch-14 --seeds 40
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 0;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// The wrapper under test. Identical to the one `ppo_update` installs.
struct Watching {
    inner: Box<dyn Decider>,
    log: Rc<RefCell<Vec<ti4_mlp::positive_corpus::Note>>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ChoiceOption) {
        if choice.options.len() < 2 {
            return;
        }
        let head = ti4_mlp::Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        self.log.borrow_mut().push(ti4_mlp::positive_corpus::Note {
            head: head.to_owned(),
            chosen: chosen.id.clone(),
            declined: chosen.is_decline(),
        });
    }
}

impl Decider for Watching {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let chosen = self.inner.choose(choice)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
}

/// One seat's decisions, as the bot itself recorded them.
type Line = Vec<(usize, usize, usize)>;

/// Play one game, optionally wrapping every seat, and return each seat's recorded line.
fn play(
    content: &'static ContentStore,
    factions: &[FactionId],
    pool: &Arc<ti4_sim::MapPool>,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    actor: &Rc<ti4_mlp::Actor>,
    seed: u64,
    rotation: usize,
    temperature: f64,
    wrap: bool,
) -> Result<(BTreeMap<String, Line>, BTreeMap<String, bool>), String> {
    let mut handles: BTreeMap<PlayerId, Rc<RefCell<Vec<ti4_mlp::bot::PpoRecord>>>> =
        BTreeMap::new();

    let (_e, _s, assignments, openings, _f) = ti4_training::rollout::audit_game_with_deciders(
        content,
        factions,
        DEFAULT,
        seed,
        rotation,
        ti4_training::rollout::Horizon {
            rounds: 1,
            steps: 200_000,
        },
        &ti4_training::rollout::OpeningMap::PythonPool {
            pool: Arc::clone(pool),
            tile_seed_offset: TILE_SEED_OFFSET,
        },
        |seated, baselines| {
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, (player, faction)) in seated.iter().enumerate() {
                let row = ti4_mlp::FactionRow::of(faction.as_str())
                    .map_err(|error| format!("{player}: {error}"))?;
                let baseline = baselines
                    .get(player)
                    .copied()
                    .ok_or_else(|| format!("{player} has no baseline"))?;
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let bot = ti4_mlp::bot::MlpBot::sharing(actor, vocabulary.clone(), row, stream)
                    .at_temperature(temperature)
                    .from_setup(baseline)
                    .recording_ppo(ti4_mlp::bundle::CriticMode::BatchMean);
                handles.insert(player.clone(), bot.ppo_records());
                let (decider, _status) = bot.seat();
                if wrap {
                    let log = Rc::new(RefCell::new(Vec::new()));
                    deciders.insert(
                        player.clone(),
                        Box::new(Watching {
                            inner: decider,
                            log,
                        }),
                    );
                } else {
                    deciders.insert(player.clone(), decider);
                }
            }
            Ok(deciders)
        },
    )?;

    let mut lines: BTreeMap<String, Line> = BTreeMap::new();
    let mut cleared: BTreeMap<String, bool> = BTreeMap::new();
    for (player, records) in &handles {
        let Some(faction) = assignments.get(player) else {
            return Err(format!("{player} has no faction"));
        };
        lines.insert(
            faction.to_string(),
            records
                .borrow()
                .iter()
                .map(|record| {
                    (
                        record.step.head,
                        record.step.options.len(),
                        record.step.chosen,
                    )
                })
                .collect(),
        );
    }
    for (player, opening) in &openings {
        if let Some(faction) = assignments.get(player) {
            cleared.insert(faction.to_string(), opening.cleared());
        }
    }
    Ok((lines, cleared))
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seeds: u64 = argument("--seeds").map_or(40, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds is a number"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(800_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base is a number"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new("out/pools/full_np8_12_train.json"),
                &[ti4_sim::artifacts::ArtifactRole::Train],
            )
            .unwrap_or_else(|error| refuse(&format!("train pool: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    // Both temperatures the sweep touched. Greedy is where the comparison was read; 2.5 is where
    // the training ran, and a wrapper that perturbed the RNG would show there first.
    let temperatures = [0.001_f64, 2.5];
    println!("wrapper identity for {bundle_path}");
    println!(
        "  {seeds} seeds x {} rotations x {} temperatures, decision-by-decision",
        FACTIONS.len(),
        temperatures.len()
    );
    println!();

    let jobs: Vec<(u64, usize, f64)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| {
            (0..FACTIONS.len())
                .flat_map(move |rotation| temperatures.map(move |t| (seed, rotation, t)))
        })
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);

    let harvest: Vec<Result<(usize, usize), String>> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = Rc::new(local);
            let mut compared = 0usize;
            let mut decisions = 0usize;
            for (seed, rotation, temperature) in chunk {
                let (bare, bare_cleared) = play(
                    content,
                    &factions,
                    &pool,
                    &vocabulary,
                    &local,
                    seed,
                    rotation,
                    temperature,
                    false,
                )?;
                let (wrapped, wrapped_cleared) = play(
                    content,
                    &factions,
                    &pool,
                    &vocabulary,
                    &local,
                    seed,
                    rotation,
                    temperature,
                    true,
                )?;
                if bare != wrapped {
                    for (faction, line) in &bare {
                        let other = wrapped.get(faction);
                        if other != Some(line) {
                            return Err(format!(
                                "{seed}/{rotation} T={temperature} {faction}: the wrapper changed \
                                 the line ({} decisions bare, {} wrapped)",
                                line.len(),
                                other.map_or(0, Vec::len)
                            ));
                        }
                    }
                }
                if bare_cleared != wrapped_cleared {
                    return Err(format!(
                        "{seed}/{rotation} T={temperature}: the wrapper changed an outcome"
                    ));
                }
                compared += bare.len();
                decisions += bare.values().map(Vec::len).sum::<usize>();
            }
            Ok((compared, decisions))
        })
        .collect();

    let mut seats = 0usize;
    let mut decisions = 0usize;
    for chunk in harvest {
        match chunk {
            Ok((s, d)) => {
                seats += s;
                decisions += d;
            }
            Err(error) => {
                eprintln!("\nDIFFERENT: {error}");
                std::process::exit(1);
            }
        }
    }

    println!("  {seats} seat-games, {decisions} decisions, identical in every one.");
    println!("  The wrapper is inert; numbers measured with it are comparable to numbers without.");
}
