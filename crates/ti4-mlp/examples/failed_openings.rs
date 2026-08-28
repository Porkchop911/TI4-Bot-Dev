//! Print failed openings in full, one at a time, for a human to read.
//!
//! Every characterisation of these failures so far has been an aggregate, and the aggregates have
//! twice been misleading. "77.8% of failures miss planets" hid three unrelated faults — a faction
//! that never builds, one that never reaches a two-planet tile, one that reaches it and cannot
//! deliver. And "every targeting failure was a choice" rested on counting the planets *on* a tile,
//! which is not the same as being able to take them.
//!
//! At the operating temperature the survivors are few and close to deterministic, so they are a
//! finite list rather than a statistical tail. This prints them: what the seat had, where it went,
//! what else it was offered, where its forces ended up, and which part of the bar it missed.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::Choice;
use ti4_engine::choice::Decider;
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

/// One activation, with the whole menu it was chosen from.
struct Activation {
    chosen: SystemId,
    offered: Vec<SystemId>,
}

struct Watching {
    inner: Box<dyn Decider>,
    log: std::rc::Rc<std::cell::RefCell<Vec<Activation>>>,
}

impl Watching {
    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        if chosen.kind == ti4_engine::tactical::ACTIVATE_KIND {
            self.log.borrow_mut().push(Activation {
                chosen: SystemId::new(chosen.id.clone()),
                offered: choice
                    .options
                    .iter()
                    .filter(|option| option.kind == ti4_engine::tactical::ACTIVATE_KIND)
                    .map(|option| SystemId::new(option.id.clone()))
                    .collect(),
            });
        }
    }
}

impl Decider for Watching {
    fn choose(
        &mut self,
        choice: &Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose(choice)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let chosen = self.inner.choose_seeing(choice, seen)?;
        self.record(choice, &chosen);
        Ok(chosen)
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass over the sampled games; the play and what is printed about it belong together"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: failures belong to a specific policy"));
    let seeds: u64 = argument("--seeds").map_or(40, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(700_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    let temperature: f64 = argument("--temperature").map_or(0.25, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
    });
    let wanted: usize = argument("--show").map_or(20, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--show expects a positive integer"))
    });
    let only = argument("--faction");

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

    println!("failed openings from {bundle_path}");
    println!("temperature {temperature}, bar 3 planets / 3 systems / 2 capacity ships + 3 ground");

    let mut shown = 0usize;
    let mut scanned = 0usize;

    'outer: for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let logs: std::rc::Rc<
                std::cell::RefCell<
                    BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<Activation>>>>,
                >,
            > = std::rc::Rc::new(std::cell::RefCell::new(BTreeMap::new()));
            let seated_logs = std::rc::Rc::clone(&logs);

            let (_events, _picks, assignments, openings, final_state) =
                ti4_training::rollout::audit_game_with_deciders(
                    content,
                    &factions,
                    DEFAULT,
                    seed,
                    rotation,
                    ti4_training::rollout::Horizon {
                        rounds: 1,
                        steps: 200_000,
                    },
                    &ti4_training::rollout::OpeningMap::PythonPool {
                        pool: Arc::clone(&pool),
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
                                .ok_or_else(|| format!("{player} has no setup baseline"))?;
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
                            let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                            seated_logs
                                .borrow_mut()
                                .insert(player.clone(), std::rc::Rc::clone(&log));
                            deciders.insert(
                                player.clone(),
                                Box::new(Watching {
                                    inner: decider,
                                    log,
                                }),
                            );
                        }
                        Ok(deciders)
                    },
                )
                .unwrap_or_else(|error| refuse(&error));

            // A tile's planets, and how many of them this seat ended up holding.
            let planets_on = |system: &SystemId| -> usize {
                final_state
                    .board
                    .get(system)
                    .map_or(0, |state| state.planet_units.len())
            };
            let held_in = |system: &SystemId, player: &PlayerId| -> usize {
                final_state.board.get(system).map_or(0, |state| {
                    state
                        .planet_control
                        .values()
                        .filter(|owner| *owner == player)
                        .count()
                })
            };

            let recorded = logs.borrow();
            for (player, opening) in &openings {
                scanned += 1;
                if opening.cleared() {
                    continue;
                }
                let Some(faction) = assignments.get(player) else {
                    continue;
                };
                if only.as_ref().is_some_and(|want| want != faction.as_str()) {
                    continue;
                }

                shown += 1;
                println!();
                println!(
                    "--- {} on seed {seed}/{rotation} -----------------------------------------",
                    faction.as_str()
                );
                println!(
                    "  missed      {}{}{}",
                    if opening.planets_ok() {
                        String::new()
                    } else {
                        format!(
                            "planets ({} of {}) ",
                            opening.planets_gained, opening.requirement.planets_gained
                        )
                    },
                    if opening.systems_ok() {
                        String::new()
                    } else {
                        format!(
                            "systems ({} of {}) ",
                            opening.systems, opening.requirement.systems
                        )
                    },
                    if opening.units_ok() {
                        String::new()
                    } else {
                        format!(
                            "fleet ({} hulls, {} ground; needs {} and {})",
                            opening.capacity_ships,
                            opening.infantry,
                            opening.requirement.capacity_ships,
                            opening.requirement.infantry
                        )
                    }
                );

                let activations = recorded.get(player).map(|log| log.borrow());
                match activations {
                    Some(log) if !log.is_empty() => {
                        for (index, activation) in log.iter().enumerate() {
                            let menu: Vec<String> = activation
                                .offered
                                .iter()
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .map(|system| {
                                    format!("{}({}p)", system.as_str(), planets_on(system))
                                })
                                .collect();
                            println!(
                                "  activate {}  chose {} with {} planets, took {}",
                                index + 1,
                                activation.chosen.as_str(),
                                planets_on(&activation.chosen),
                                held_in(&activation.chosen, player)
                            );
                            println!("              offered {}", menu.join(" "));
                        }
                    }
                    _ => println!("  activate    none"),
                }

                // Where its forces finished, so a reader can see what was stranded.
                let mut ended: Vec<String> = Vec::new();
                for (system, state) in &final_state.board {
                    let ships = state.units_of(player).len();
                    let ground = state
                        .planet_units
                        .values()
                        .flatten()
                        .filter(|unit| &unit.owner == player)
                        .count();
                    if ships > 0 || ground > 0 {
                        ended.push(format!("{}({ships}s {ground}g)", system.as_str()));
                    }
                }
                println!("  ended       {}", ended.join(" "));

                if shown >= wanted {
                    break 'outer;
                }
            }
        }
    }

    println!();
    println!("  {shown} failures shown from {scanned} seat-games scanned");
}
