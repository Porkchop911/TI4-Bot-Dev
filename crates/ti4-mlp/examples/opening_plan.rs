//! Does the policy follow the opening plan, and if not, which step does it miss?
//!
//! The clearance report says *which component of the bar* came up short. That is not the same as
//! which decision went wrong, and the difference matters: "77.8% of failures missed planets" is
//! consistent with a dozen different mistakes.
//!
//! The plan for a seat that already holds two capacity ships and three ground forces is short:
//!
//! 1. activate a **two-planet** system and carry at least two ground forces there;
//! 2. activate a **different one-planet** system and carry at least one;
//! 3. build something — `units_gained` is measured against setup, so landing infantry that already
//!    existed does not satisfy it.
//!
//! Three planets across three systems, plus a unit. This measures adherence to that shape, per
//! faction, split by whether the seat cleared.
//!
//! # What is measured, and how
//!
//! Activations are recorded at the decision, by a wrapper that delegates and writes down what it
//! was asked — an activation option's id *is* its destination system. Planet counts come from the
//! board, because how many planets a tile has is a property of the tile and not of the game.
//!
//! The unit step needs no instrumentation: `Opening::units_ok` already reports it.
//!
//! Sending both capacity ships to one system is the concentration failure measured in
//! F-M10-034-C2, and it appears here as a seat that activated only one tile, or two tiles that
//! cannot supply three planets between them.

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

/// A decider that answers exactly as the one it wraps, and records which systems it activated.
struct Watching {
    inner: Box<dyn Decider>,
    log: std::rc::Rc<std::cell::RefCell<Vec<Activation>>>,
}

/// One activation: where the seat went, and everywhere it could have gone.
///
/// The offered set is what separates "chose badly" from "had nothing better". A seat that took a
/// one-planet tile when a two-planet tile was on the menu made a mistake; one that took the best
/// tile available did not, and no amount of training fixes the second.
struct Activation {
    chosen: SystemId,
    offered: Vec<SystemId>,
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

/// How one faction's seats followed the plan.
#[derive(Default)]
struct Adherence {
    seats: usize,
    /// Seats that activated no system, one, or two or more.
    activated: BTreeMap<usize, usize>,
    /// Reached a tile with at least two planets.
    reached_a_double: usize,
    /// Reached two distinct tiles carrying at least three planets between them — the shape the
    /// plan needs, whether or not the seat converted it.
    reached_three_planets: usize,
    /// Built something.
    built: usize,
    /// All three: two tiles worth three planets, and a unit.
    followed: usize,
    /// Seats whose activations could have carried three planets had they taken the best tiles on
    /// offer -- so the shortfall was a choice.
    could_have: usize,
    /// Seats for which even the best offered tiles could not carry three planets.
    nothing_better: usize,
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
    clippy::too_many_lines,
    reason = "one pass over the sampled games; the plan checks belong with the play that produces them"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: adherence belongs to a specific policy"));
    let seeds: u64 = argument("--seeds").map_or(150, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(700_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    let temperature: f64 = argument("--temperature").map_or(1.0, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
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

    println!("opening plan adherence");
    println!("  bundle      {bundle_path}");
    println!(
        "  sample      {seeds} seeds x {} rotations, one round, temperature {temperature}",
        FACTIONS.len()
    );
    println!(
        "  plan        a 2-planet tile with 2+ ground, a different 1-planet tile, and a build"
    );

    let mut cleared: BTreeMap<String, Adherence> = BTreeMap::new();
    let mut failed: BTreeMap<String, Adherence> = BTreeMap::new();

    for seed in seed_base..seed_base + seeds {
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

            // How many planets each tile carries. A property of the tile, so the end-of-round board
            // reports it as faithfully as the start would.
            let planets_on = |system: &SystemId| -> usize {
                final_state
                    .board
                    .get(system)
                    .map_or(0, |state| state.planet_units.len())
            };

            let recorded = logs.borrow();
            for (player, opening) in &openings {
                let Some(faction) = assignments.get(player) else {
                    continue;
                };
                let log = recorded.get(player);
                let activations: Vec<SystemId> = log
                    .map(|log| log.borrow().iter().map(|a| a.chosen.clone()).collect())
                    .unwrap_or_default();
                let distinct: BTreeSet<&SystemId> = activations.iter().collect();

                // The best two tiles this seat was ever offered, across its activations. If those
                // cannot carry three planets between them, no choice at these decisions could have
                // reached the bar, and the failure is not a targeting mistake.
                let mut best: Vec<usize> = log
                    .map(|log| {
                        let borrowed = log.borrow();
                        let mut menu: BTreeSet<SystemId> = BTreeSet::new();
                        for activation in borrowed.iter() {
                            menu.extend(activation.offered.iter().cloned());
                        }
                        menu.iter().map(|system| planets_on(system)).collect()
                    })
                    .unwrap_or_default();
                best.sort_unstable_by(|a: &usize, b: &usize| b.cmp(a));
                let attainable = best.len() >= 2 && best.iter().take(2).sum::<usize>() >= 3;

                let mut sizes: Vec<usize> =
                    distinct.iter().map(|system| planets_on(system)).collect();
                sizes.sort_unstable_by(|a, b| b.cmp(a));
                let reached_a_double = sizes.first().copied().unwrap_or(0) >= 2;
                // Two distinct tiles carrying three planets between them: the shape the plan needs.
                let reached_three = distinct.len() >= 2 && sizes.iter().take(2).sum::<usize>() >= 3;
                let built = opening.units_ok();

                let table = if opening.cleared() {
                    &mut cleared
                } else {
                    &mut failed
                };
                let tally = table.entry(faction.to_string()).or_default();
                tally.seats += 1;
                *tally.activated.entry(distinct.len().min(2)).or_default() += 1;
                tally.reached_a_double += usize::from(reached_a_double);
                tally.reached_three_planets += usize::from(reached_three);
                tally.built += usize::from(built);
                tally.followed += usize::from(reached_three && built);
                if !reached_three {
                    if attainable {
                        tally.could_have += 1;
                    } else {
                        tally.nothing_better += 1;
                    }
                }
            }
        }
    }

    for (label, table) in [("cleared", &cleared), ("failed", &failed)] {
        println!();
        println!("  seats that {label}");
        println!();
        println!(
            "  {:<10} {:>7} {:>9} {:>10} {:>12} {:>10} {:>8} {:>12} {:>13}",
            "faction",
            "seats",
            "1 tile",
            "2+ tiles",
            "hit a 2-tile",
            "3 planets",
            "built",
            "could have",
            "nothing left"
        );
        for (faction, tally) in table {
            println!(
                "  {:<10} {:>7} {:>8.1}% {:>9.1}% {:>11.1}% {:>9.1}% {:>9.1}% {:>12.1}% {:>11.1}%",
                faction,
                tally.seats,
                share(tally.activated.get(&1).copied().unwrap_or(0), tally.seats),
                share(tally.activated.get(&2).copied().unwrap_or(0), tally.seats),
                share(tally.reached_a_double, tally.seats),
                share(tally.reached_three_planets, tally.seats),
                share(tally.built, tally.seats),
                share(tally.could_have, tally.seats),
                share(tally.nothing_better, tally.seats),
            );
        }
    }

    println!();
    println!(
        "  \"3 planets\" is two distinct activated tiles carrying three planets between them --\n  \
         the shape the plan needs, whether or not the seat converted it."
    );
}
