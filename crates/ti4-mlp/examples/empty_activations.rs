//! What actually happens after each activation (M10 behaviour diagnostic).
//!
//! Activating a system costs a command token from a pool of three, and the token stays there for
//! the rest of the round. An activation that moves nothing in and produces nothing has spent that
//! token for no board change. It is not illegal, and occasionally it is even deliberate, but a
//! policy doing it often is spending the resource that limits how many systems it can take.
//!
//! Two distinct faults hide behind "activated and nothing happened", and the aggregates this
//! project has trusted before could not tell them apart:
//!
//! - **unreachable**: the movement step offered no ship at all, so nothing *could* move in. The
//!   seat picked a destination outside its own reach.
//! - **declined**: ships were offered and the seat chose to finish movement anyway.
//!
//! The first is a targeting failure, the second a valuation failure, and they want different
//! fixes. This counts them separately, per faction, and reports what share of activations ended
//! with no unit moved and nothing produced.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::Choice;
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

/// One activation and everything the seat did while it was the active system.
#[derive(Default, Clone)]
struct Activation {
    /// Ships the movement step ever offered. Zero means nothing could reach it.
    moves_offered: usize,
    /// Ships actually moved in.
    moves_taken: usize,
    /// Production options ever offered here.
    produce_offered: usize,
    /// Units actually produced.
    produce_taken: usize,
    /// Ground forces committed to a planet.
    commits_taken: usize,
}

impl Activation {
    /// Nothing moved in, nothing was built, nothing landed.
    const fn empty(&self) -> bool {
        self.moves_taken == 0 && self.produce_taken == 0 && self.commits_taken == 0
    }
    /// No ship was ever on the menu: the seat could not have moved in even if it wanted to.
    const fn unreachable(&self) -> bool {
        self.moves_offered == 0
    }
}

/// Totals for one faction.
#[derive(Default, Clone)]
struct Tally {
    activations: usize,
    empty: usize,
    unreachable: usize,
    /// Empty despite ships being offered.
    declined: usize,
    /// Nothing moved in, but something was produced or committed.
    built_only: usize,
    /// Activations that were offered production at some point.
    could_produce: usize,
    games: usize,
}

struct Watching {
    inner: Box<dyn Decider>,
    log: std::rc::Rc<std::cell::RefCell<Vec<Activation>>>,
}

impl Watching {
    /// Fold one answered choice into the activation it belongs to.
    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        let mut log = self.log.borrow_mut();

        if chosen.kind == ti4_engine::tactical::ACTIVATE_KIND {
            log.push(Activation::default());
            return;
        }
        // Everything else belongs to the activation in progress. Choices before the first
        // activation of a game (strategy selection and so on) have nowhere to go, and are not part
        // of what this measures.
        let Some(current) = log.last_mut() else {
            return;
        };

        let offered = |kind: &str| choice.options.iter().filter(|o| o.kind == kind).count();

        // "Ever offered", not "offered now": the movement step asks repeatedly, and a seat that has
        // already moved its only carrier is then correctly offered nothing.
        current.moves_offered = current
            .moves_offered
            .max(offered(ti4_engine::tactical::MOVE_KIND));
        current.produce_offered = current
            .produce_offered
            .max(offered(ti4_engine::production::PRODUCE_KIND));

        if chosen.kind == ti4_engine::tactical::MOVE_KIND {
            current.moves_taken += 1;
        } else if chosen.kind == ti4_engine::production::PRODUCE_KIND {
            current.produce_taken += 1;
        } else if chosen.kind == ti4_engine::invasion::COMMIT_KIND {
            current.commits_taken += 1;
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
    reason = "one pass over the sampled games; setup and the table it produces belong together"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: behaviour belongs to a specific policy"));
    let seeds: u64 = argument("--seeds").map_or(60, |value| {
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
    let rounds: u32 = argument("--rounds").map_or(1, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--rounds expects a positive integer"))
    });

    ti4_tensor::configure_deterministic(20_260_828)
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

    println!("activation outcomes for {bundle_path}");
    println!("temperature {temperature}, {rounds} round(s), seeds {seed_base}..+{seeds}");

    // Split by outcome. The aggregate alone cannot say whether an unreachable activation costs
    // anything: if cleared and failed openings waste tokens at the same rate, the waste is
    // ugly but not the thing holding clearance down, and fixing it would buy nothing.
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_outcome: BTreeMap<bool, Tally> = BTreeMap::new();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let logs: std::rc::Rc<
                std::cell::RefCell<
                    BTreeMap<PlayerId, std::rc::Rc<std::cell::RefCell<Vec<Activation>>>>,
                >,
            > = std::rc::Rc::new(std::cell::RefCell::new(BTreeMap::new()));
            let seated_logs = std::rc::Rc::clone(&logs);

            let (_events, _picks, assignments, openings, _final_state) =
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

            let recorded = logs.borrow();
            for (player, faction) in &assignments {
                let Some(log) = recorded.get(player) else {
                    continue;
                };
                let cleared = openings
                    .get(player)
                    .is_some_and(ti4_engine::opening::Opening::cleared);
                let mut fold = |entry: &mut Tally| {
                    entry.games += 1;
                    for activation in log.borrow().iter() {
                        entry.activations += 1;
                        if activation.produce_offered > 0 {
                            entry.could_produce += 1;
                        }
                        if activation.empty() {
                            entry.empty += 1;
                            if activation.unreachable() {
                                entry.unreachable += 1;
                            } else {
                                entry.declined += 1;
                            }
                        } else if activation.moves_taken == 0 {
                            entry.built_only += 1;
                        }
                    }
                };
                fold(tallies.entry(faction.to_string()).or_default());
                fold(by_outcome.entry(cleared).or_default());
            }
        }
    }

    let share = |part: usize, whole: usize| -> String {
        if whole == 0 {
            return "    --".to_owned();
        }
        #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
        let value = 100.0 * part as f64 / whole as f64;
        format!("{value:5.1}%")
    };

    println!();
    println!("  activations that changed nothing, split by whether anything could have moved in");
    println!();
    println!(
        "  faction      games   activations  per game     empty  unreachable   declined  built only"
    );
    let mut totals = Tally::default();
    for (faction, tally) in &tallies {
        totals.games += tally.games;
        totals.activations += tally.activations;
        totals.empty += tally.empty;
        totals.unreachable += tally.unreachable;
        totals.declined += tally.declined;
        totals.built_only += tally.built_only;
        totals.could_produce += tally.could_produce;
        #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
        let per_game = tally.activations as f64 / tally.games.max(1) as f64;
        println!(
            "  {:<10} {:>6} {:>13} {:>9.2}    {}       {}     {}      {}",
            faction,
            tally.games,
            tally.activations,
            per_game,
            share(tally.empty, tally.activations),
            share(tally.unreachable, tally.activations),
            share(tally.declined, tally.activations),
            share(tally.built_only, tally.activations),
        );
    }
    println!("  {:-<94}", "");
    #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
    let per_game = totals.activations as f64 / totals.games.max(1) as f64;
    println!(
        "  {:<10} {:>6} {:>13} {:>9.2}    {}       {}     {}      {}",
        "table",
        totals.games,
        totals.activations,
        per_game,
        share(totals.empty, totals.activations),
        share(totals.unreachable, totals.activations),
        share(totals.declined, totals.activations),
        share(totals.built_only, totals.activations),
    );
    println!();
    println!("  the same split by whether that seat's opening cleared");
    println!();
    println!(
        "  opening      seats   activations  per game     empty  unreachable   declined  built only"
    );
    for (cleared, tally) in [
        (true, by_outcome.get(&true)),
        (false, by_outcome.get(&false)),
    ]
    .into_iter()
    .filter_map(|(flag, tally)| tally.map(|tally| (flag, tally)))
    {
        #[expect(clippy::cast_precision_loss, reason = "counts are far below 2^53")]
        let per_game = tally.activations as f64 / tally.games.max(1) as f64;
        println!(
            "  {:<10} {:>6} {:>13} {:>9.2}    {}       {}     {}      {}",
            if cleared { "cleared" } else { "failed" },
            tally.games,
            tally.activations,
            per_game,
            share(tally.empty, tally.activations),
            share(tally.unreachable, tally.activations),
            share(tally.declined, tally.activations),
            share(tally.built_only, tally.activations),
        );
    }
    println!();
    println!(
        "  production was on the menu in {} of activations",
        share(totals.could_produce, totals.activations)
    );
}
