//! What a contact actually produces, under training conditions.
//!
//! The reviewer measurement that motivated "86% of contacts produce nothing" was one round at
//! temperature 0.5 with a session recording, and it conflated three different things. Astra's
//! correction is the shape of this example: a contact that offered **nothing admissible** is dead
//! weight, a contact whose menu held only signals is a different case, and a non-empty menu the
//! policy declined is a choice, not waste. Only the first is safe to suppress.
//!
//! So this replays the training regime -- the same bundle, temperature, horizon, pool and seating
//! the trainer uses -- and classifies every contact from the decision stream. Nothing is written to
//! disk: the counters live in the deciders, so a four-round game costs no session artifact (a
//! reviewer session of one round is already ~120 MB).
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example contact_funnel -- \
//!   --bundle out/ppo-diplomacy-my-run/checkpoints/checkpoint-212544 \
//!   --map-pool out/pools/full_np8_12_train.json \
//!   --seeds 4 --rounds 4 --temperature 2.5
//! ```
//!
//! Seeds are seed *blocks*: each plays all six rotations, as the trainer does, so the counts are
//! comparable with an update's workload.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, Decider};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 7_000_000;

const CONTACT_PREFIX: &str = "component|diplomacy|";
const BUNDLE_PREFIX: &str = "diplomacy|";
const SIGNAL_PREFIX: &str = "diplomacy|signal|";
const NO_OFFER: &str = "diplomacy|decline";

/// Every count the funnel needs, accumulated across seats and games.
#[derive(Default, Debug)]
struct Funnel {
    /// Action decisions that listed at least one contact, and contacts listed in them.
    action_decisions_offering_contacts: usize,
    contact_options_offered: usize,
    contacts_opened: usize,
    /// The menu a contact opened with, in Astra's three categories.
    menu_empty: usize,
    menu_signals_only: usize,
    menu_has_bundles: usize,
    /// What the proposer did with a menu that had bundles.
    declined_nonempty_menu: usize,
    proposed_bundle: usize,
    sent_signal: usize,
    /// Responses to a proposal.
    responses: usize,
    accepted: usize,
    countered: usize,
    refused: usize,
    /// Menu sizes, to show what the option count actually is in training.
    bundles_seen: usize,
    signals_seen: usize,
    largest_menu: usize,
}

impl Funnel {
    fn merge(&mut self, other: &Self) {
        self.action_decisions_offering_contacts += other.action_decisions_offering_contacts;
        self.contact_options_offered += other.contact_options_offered;
        self.contacts_opened += other.contacts_opened;
        self.menu_empty += other.menu_empty;
        self.menu_signals_only += other.menu_signals_only;
        self.menu_has_bundles += other.menu_has_bundles;
        self.declined_nonempty_menu += other.declined_nonempty_menu;
        self.proposed_bundle += other.proposed_bundle;
        self.sent_signal += other.sent_signal;
        self.responses += other.responses;
        self.accepted += other.accepted;
        self.countered += other.countered;
        self.refused += other.refused;
        self.bundles_seen += other.bundles_seen;
        self.signals_seen += other.signals_seen;
        self.largest_menu = self.largest_menu.max(other.largest_menu);
    }
}

/// Wraps a seat's decider and classifies its diplomacy decisions, like the trainer's `Watching`.
struct Counting {
    inner: Box<dyn Decider>,
    funnel: Rc<RefCell<Funnel>>,
}

impl Counting {
    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        let mut funnel = self.funnel.borrow_mut();
        let subtype = choice
            .context
            .as_ref()
            .map_or("", |context| context.subtype.as_str());

        let contacts = choice
            .options
            .iter()
            .filter(|option| option.id.starts_with(CONTACT_PREFIX))
            .count();
        if contacts > 0 {
            funnel.action_decisions_offering_contacts += 1;
            funnel.contact_options_offered += contacts;
        }

        match subtype {
            "diplomacy_offer" => {
                // The opening menu of a contact: bundles, signals, and "make no offer".
                let signals = choice
                    .options
                    .iter()
                    .filter(|option| option.id.starts_with(SIGNAL_PREFIX))
                    .count();
                let bundles = choice
                    .options
                    .iter()
                    .filter(|option| {
                        option.id.starts_with(BUNDLE_PREFIX)
                            && !option.id.starts_with(SIGNAL_PREFIX)
                            && option.id != NO_OFFER
                    })
                    .count();
                funnel.contacts_opened += 1;
                funnel.bundles_seen += bundles;
                funnel.signals_seen += signals;
                funnel.largest_menu = funnel.largest_menu.max(choice.options.len());
                if bundles == 0 && signals == 0 {
                    funnel.menu_empty += 1;
                } else if bundles == 0 {
                    funnel.menu_signals_only += 1;
                } else {
                    funnel.menu_has_bundles += 1;
                }
                if chosen.id == NO_OFFER {
                    if bundles > 0 {
                        funnel.declined_nonempty_menu += 1;
                    }
                } else if chosen.id.starts_with(SIGNAL_PREFIX) {
                    funnel.sent_signal += 1;
                } else {
                    funnel.proposed_bundle += 1;
                }
            }
            "diplomacy_response" => {
                funnel.responses += 1;
                if chosen.id == "diplomacy|accept" {
                    funnel.accepted += 1;
                } else if chosen.id == "diplomacy|decline" {
                    funnel.refused += 1;
                } else {
                    funnel.countered += 1;
                }
            }
            _ => {}
        }
    }
}

impl Decider for Counting {
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

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2)
}

fn number<T: std::str::FromStr>(name: &str, fallback: T) -> T {
    argument(name).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} expects a number, got {value:?}")))
    })
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let pool_path = argument("--map-pool").unwrap_or_else(|| refuse("--map-pool is required"));
    let seeds: u64 = number("--seeds", 4);
    let seed_base: u64 = number("--seed-base", 1_261_000_000);
    let rounds: u32 = number("--rounds", 4);
    let temperature: f64 = number("--temperature", 2.5);

    ti4_tensor::configure_deterministic(20_260_916)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = bundle.vocabulary;
    // `MlpBot::sharing` takes a shared handle: one read-only actor for every seat, as the
    // trainer's CPU workers do.
    let actor = Rc::new(bundle.actor.inference_copy());
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            std::fs::read(&pool_path)
                .unwrap_or_else(|error| refuse(&format!("reading {pool_path}: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..6)
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let factions: [FactionId; 6] = FACTIONS.map(FactionId::new);

    println!("contact funnel, training regime");
    println!("  bundle       {bundle_path}");
    println!("  maps         {pool_path}");
    println!(
        "  games        {seeds} seed blocks x {} rotations = {}",
        FACTIONS.len(),
        seeds as usize * FACTIONS.len()
    );
    println!("  rounds       {rounds}   temperature {temperature}");
    println!();

    let started = std::time::Instant::now();
    let mut totals = Funnel::default();
    let mut games = 0_usize;
    let mut errors = 0_usize;
    let mut truncated = 0_usize;

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let seated: BTreeMap<PlayerId, FactionId> = players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    (
                        player.clone(),
                        ti4_training::rollout::seated_faction(&factions, seed, rotation, index),
                    )
                })
                .collect();
            let counters: Rc<RefCell<Funnel>> = Rc::new(RefCell::new(Funnel::default()));
            let (rollout, _) =
                ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                    content,
                    &players,
                    &seated,
                    DEFAULT,
                    seed,
                    ti4_training::rollout::Horizon {
                        rounds,
                        steps: 10_000,
                    },
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                    &ti4_training::rollout::OpeningMap::PythonPool {
                        pool: Arc::clone(&pool),
                        tile_seed_offset: TILE_SEED_OFFSET,
                    },
                    ti4_training::rollout::SimulationCapabilities { diplomacy: true },
                    false,
                    |baselines| {
                        let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                        for (index, player) in players.iter().enumerate() {
                            let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                                .map_err(|error| format!("{player}: {error}"))?;
                            let baseline = baselines
                                .get(player)
                                .copied()
                                .ok_or_else(|| format!("{player} has no setup baseline"))?;
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            let bot =
                                ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream)
                                    .at_temperature(temperature)
                                    .from_setup(baseline);
                            let (decider, _status) = bot.seat();
                            deciders.insert(
                                player.clone(),
                                Box::new(Counting {
                                    inner: decider,
                                    funnel: Rc::clone(&counters),
                                }),
                            );
                        }
                        Ok(deciders)
                    },
                );
            games += 1;
            if let Some(error) = &rollout.error {
                errors += 1;
                eprintln!("  seed {seed} rotation {rotation}: {error}");
            }
            totals.merge(&counters.borrow());
        }
    }
    let _ = &mut truncated;

    let percent = |part: usize, whole: usize| {
        if whole == 0 {
            0.0
        } else {
            100.0 * part as f64 / whole as f64
        }
    };
    let opened = totals.contacts_opened;

    println!("  games {games}, errors {errors}, truncated {truncated}");
    println!("  measured in {:.1}s\n", started.elapsed().as_secs_f64());
    println!("  contact options offered in action decisions   {}", totals.contact_options_offered);
    println!(
        "  action decisions listing a contact            {}",
        totals.action_decisions_offering_contacts
    );
    println!("  contacts opened                               {opened}");
    println!();
    println!("  menu at the opened contact");
    println!(
        "    nothing admissible (no bundles, no signals) {:>6}  {:>5.1}%   <- safe to suppress",
        totals.menu_empty,
        percent(totals.menu_empty, opened)
    );
    println!(
        "    signals only                                {:>6}  {:>5.1}%",
        totals.menu_signals_only,
        percent(totals.menu_signals_only, opened)
    );
    println!(
        "    bundles available                           {:>6}  {:>5.1}%",
        totals.menu_has_bundles,
        percent(totals.menu_has_bundles, opened)
    );
    println!();
    println!("  what the proposer did");
    println!(
        "    proposed a deal                             {:>6}  {:>5.1}%",
        totals.proposed_bundle,
        percent(totals.proposed_bundle, opened)
    );
    println!(
        "    sent a signal                               {:>6}  {:>5.1}%",
        totals.sent_signal,
        percent(totals.sent_signal, opened)
    );
    println!(
        "    declined a menu that had deals              {:>6}  {:>5.1}%   <- a choice, not waste",
        totals.declined_nonempty_menu,
        percent(totals.declined_nonempty_menu, opened)
    );
    println!();
    println!("  responses {}  accepted {}  countered {}  refused {}",
        totals.responses, totals.accepted, totals.countered, totals.refused);
    println!(
        "  menu sizes: {:.1} bundles and {:.1} signals per opened contact, largest menu {}",
        totals.bundles_seen as f64 / opened.max(1) as f64,
        totals.signals_seen as f64 / opened.max(1) as f64,
        totals.largest_menu
    );
}
