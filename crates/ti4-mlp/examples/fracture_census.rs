//! How often the Fracture enters play in real games, and whether its systems are then offered for
//! activation and chosen.
//!
//! Three different things can make "the bot never activates a Fracture tile" true, and they want
//! different fixes:
//!
//! - **never in play**: nobody gained a breakthrough, or nobody rolled the Fracture in. There is
//!   nothing on the board to activate, and no engine or policy change will produce one.
//! - **in play, never offered**: the tiles are on the board but activation does not list them.
//!   That is an engine bug -- the one `d151134` fixed, where activation enumerated the printed
//!   galaxy and never the systems the Fracture adds.
//! - **offered, never chosen**: the engine does its job and the policy declines. A valuation
//!   question, not a rules one.
//!
//! This counts all three per game from the final state and a decider wrapper, so the answer is
//! read off real play rather than inferred from a unit test.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::{ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId, SystemId};

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

/// Activation choices in one game, across every seat.
#[derive(Default, Clone, Copy)]
struct Counts {
    /// Choices that offered at least one system to activate.
    activation_choices: usize,
    /// Of those, choices that listed at least one Fracture system.
    offered: usize,
    /// Activations whose chosen system was a Fracture system.
    chosen: usize,
}

struct Watching {
    inner: Box<dyn Decider>,
    counts: Rc<RefCell<Counts>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ChoiceOption) {
        let activate = ti4_engine::tactical::ACTIVATE_KIND;
        if !choice.options.iter().any(|option| option.kind == activate) {
            return;
        }
        let content = ContentStore::embedded();
        let fracture = |id: &str| {
            ti4_engine::fracture::is_fracture_system(content, DEFAULT, &SystemId::new(id))
        };
        let mut counts = self.counts.borrow_mut();
        counts.activation_choices += 1;
        if choice
            .options
            .iter()
            .any(|option| option.kind == activate && fracture(&option.id))
        {
            counts.offered += 1;
        }
        if chosen.kind == activate && fracture(&chosen.id) {
            counts.chosen += 1;
        }
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

/// One game's outcome.
struct GameStat {
    fracture_in_play: bool,
    slices_claimed: usize,
    breakthroughs: usize,
    counts: Counts,
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let parse = |name: &str, default: u64| -> u64 {
        argument(name).map_or(default, |value| {
            value
                .parse()
                .unwrap_or_else(|_| refuse(&format!("{name} expects an unsigned integer")))
        })
    };
    let seeds = parse("--seeds", 100);
    let seed_base = parse("--seed-base", 910_001_000);
    let rounds = u32::try_from(parse("--rounds", 4)).unwrap_or(4);
    let temperature: f64 = argument("--temperature").map_or(0.01, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
    });

    ti4_tensor::configure_deterministic(20_260_911)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;

    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let content = ContentStore::embedded();
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("fracture census for {bundle_path}");
    println!("  temperature {temperature}, {rounds} round(s), seeds {seed_base}..+{seeds} x 6 rotations");

    // One owned actor per worker: `tch::Tensor` is `Send` but not `Sync`.
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);
    let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| {
            (
                loaded
                    .actor
                    .inference_copy()
                    .to_device(ti4_tensor::Device::Cpu),
                chunk.to_vec(),
            )
        })
        .collect();

    let stats: Vec<GameStat> = chunks
        .into_par_iter()
        .map(|(actor, chunk)| -> Vec<GameStat> {
            let actor = Rc::new(actor);
            chunk
                .into_iter()
                .map(|(seed, rotation)| {
                    let counts = Rc::new(RefCell::new(Counts::default()));
                    let seat_counts = Rc::clone(&counts);
                    let (_events, _start, _assignments, _openings, final_state) =
                        ti4_training::rollout::audit_game_with_deciders(
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
                            |seated, baselines| {
                                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                    BTreeMap::new();
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
                                    let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                        &actor,
                                        vocabulary.clone(),
                                        row,
                                        stream,
                                    )
                                    .from_setup(baseline)
                                    .at_temperature(temperature)
                                    .seat();
                                    deciders.insert(
                                        player.clone(),
                                        Box::new(Watching {
                                            inner: decider,
                                            counts: Rc::clone(&seat_counts),
                                        }),
                                    );
                                }
                                Ok(deciders)
                            },
                        )
                        .unwrap_or_else(|error| refuse(&error));
                    let counts = *counts.borrow();
                    GameStat {
                        fracture_in_play: final_state.fracture_in_play,
                        slices_claimed: final_state.expedition_slices.len(),
                        breakthroughs: final_state
                            .players
                            .iter()
                            .filter(|seat| seat.breakthrough.is_some())
                            .count(),
                        counts,
                    }
                })
                .collect()
        })
        .collect::<Vec<Vec<GameStat>>>()
        .into_iter()
        .flatten()
        .collect();

    let games = stats.len();
    let pct = |part: usize, whole: usize| {
        if whole == 0 {
            "   --".to_owned()
        } else {
            #[allow(clippy::cast_precision_loss, reason = "counts are small")]
            let share = 100.0 * part as f64 / whole as f64;
            format!("{share:5.1}%")
        }
    };
    let with_slice = stats.iter().filter(|s| s.slices_claimed > 0).count();
    let with_breakthrough = stats.iter().filter(|s| s.breakthroughs > 0).count();
    let in_play: Vec<&GameStat> = stats.iter().filter(|s| s.fracture_in_play).collect();
    let sum = |set: &[&GameStat], f: fn(&Counts) -> usize| set.iter().map(|s| f(&s.counts)).sum::<usize>();
    let all: Vec<&GameStat> = stats.iter().collect();

    println!();
    println!("  games                                   {games}");
    println!("  an expedition slice was claimed         {with_slice:6}  {}", pct(with_slice, games));
    println!("  a seat holds a breakthrough             {with_breakthrough:6}  {}", pct(with_breakthrough, games));
    println!("  the Fracture was in play at the end     {:6}  {}", in_play.len(), pct(in_play.len(), games));
    println!();
    println!("  in games where the Fracture was in play:");
    let choices = sum(&in_play, |c| c.activation_choices);
    let offered = sum(&in_play, |c| c.offered);
    let chosen = sum(&in_play, |c| c.chosen);
    println!("    activation choices                    {choices}");
    println!("    ...listing a Fracture system          {offered:6}  {}", pct(offered, choices));
    println!("    ...where a Fracture system was chosen {chosen:6}  {}", pct(chosen, offered));
    println!();
    println!(
        "  across all games: {} activation choices, {} listed a Fracture system, {} chose one",
        sum(&all, |c| c.activation_choices),
        sum(&all, |c| c.offered),
        sum(&all, |c| c.chosen)
    );
}
