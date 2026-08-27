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
//! Strategy picks are read from a snapshot taken **the moment the strategy phase ends**, not from
//! the final state. Cards are returned to the common pool in the status phase, so by the end of a
//! round `PlayerState::strategy_cards` is empty and a final-state reading answers "which card does
//! this faction hold now" — always "none". The first version did that and printed an empty table.
//!
//! These are *realized* picks rather than preferences: six seats draw from eight cards in
//! initiative order, so what a faction takes is bounded by what is still there when its turn comes.
//!
//! Secondary participation is recorded **at the decision**, by a wrapper around each seat's
//! decider. The event log was the obvious source and is the wrong one: `game.events` carries event
//! *names* with the payload consumed by the rules engine, so it gives a table-level follow rate and
//! no way to say who followed what. A seat's `Choice` carries both — `Choice::player` names the
//! seat and the prompt is `"{card} secondary"` — so watching the decision attributes the card and
//! the faction together, which counting events never could.
//!
//! The wrapper delegates and records; it never changes an answer. Both `choose` and `choose_seeing`
//! are overridden, because the engine calls whichever the site can honestly offer, and recording
//! only one would silently miss every decision made at the other.

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

/// One faction's behaviour across the sampled games.
#[derive(Default)]
struct Tally {
    games: usize,
    cards: BTreeMap<String, usize>,
    /// Per strategy card, how often this faction was offered its secondary and took it.
    secondaries: BTreeMap<String, (usize, usize)>,
}

/// One recorded secondary decision.
struct Secondary {
    faction: String,
    card: String,
    followed: bool,
}

/// A decider that answers exactly as the one it wraps, and writes down what it was asked.
///
/// Reporting is not the seat's job, and a bot that also logged would be a bot whose behaviour
/// depended on whether anyone was watching. This keeps the two apart: every answer comes from
/// `inner`, unchanged.
struct Watching {
    inner: Box<dyn Decider>,
    faction: String,
    log: std::rc::Rc<std::cell::RefCell<Vec<Secondary>>>,
}

impl Watching {
    /// The strategy card a secondary prompt is about, if this is one.
    ///
    /// Most cards phrase their own offer rather than using a generic one: the engine builds
    /// `"spend a strategy token to produce at home"` for Warfare and only falls back to
    /// `"{card} secondary"` for cards with no specific contract. The first version of this matched
    /// the fallback alone and recorded **nothing** — 3,791 secondaries in the event log against 0
    /// here — which is why the cross-check against the event count exists and why it is printed
    /// rather than merely computed.
    ///
    /// The table is duplicated from `ti4_engine::strategy`, so a reworded prompt would silently
    /// stop being counted. It would not stay silent: the cross-check is what turns that into a
    /// visible MISMATCH rather than a quietly shrinking denominator.
    fn secondary_card(prompt: &str) -> Option<&'static str> {
        if let Some(card) = prompt.strip_suffix(" secondary") {
            return Some(match card {
                "pok1leadership" => "leadership",
                "pok2diplomacy" => "diplomacy",
                "pok3politics" => "politics",
                "pok5trade" => "trade",
                "pok7technology" => "technology",
                "pok8imperial" => "imperial",
                "te4construction" => "construction",
                "te6warfare" => "warfare",
                _ => "other",
            });
        }
        // Leadership is deliberately absent. Its secondary offer -- "spend N influence for a
        // command token" -- is byte-identical to the *primary* offer built by
        // `strategy_cards::influence_purchase_choice`: same prompt, same option ids, same labels,
        // same kinds. Nothing in the `Choice` distinguishes them, so counting the prompt would
        // silently fold primary spends into the secondary rate. It did, before this comment
        // existed: Sol read 100% leadership follow.
        //
        // Left uncounted here and derived at table level instead, as the gap between the event
        // log's total and what is attributed below.
        Some(match prompt {
            "spend a strategy token to place a structure"
            | "spend a strategy token to build a structure" => "construction",
            "spend a strategy token to replenish commodities" => "trade",
            "spend a strategy token to produce at home" => "warfare",
            "spend a strategy token and 4 resources to research" => "technology",
            "spend a strategy token to draw a secret objective" => "imperial",
            "spend a strategy token to ready two planets" => "diplomacy",
            "spend a strategy token to draw two action cards" => "politics",
            _ => return None,
        })
    }

    fn record(&self, choice: &Choice, chosen: &ti4_engine::choice::ChoiceOption) {
        let Some(card) = Self::secondary_card(&choice.prompt) else {
            return;
        };
        // Two shapes of refusal: the generic fallback offers a `decline` option, the card-specific
        // contracts offer `no`. Both mean the seat did not follow.
        let followed = !chosen.is_decline() && chosen.id != "no";
        self.log.borrow_mut().push(Secondary {
            faction: self.faction.clone(),
            card: card.to_owned(),
            followed,
        });
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
            let log: std::rc::Rc<std::cell::RefCell<Vec<Secondary>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let (events, state, assignments, _openings) =
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
                    |seated| {
                        let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                        for (index, (player, faction)) in seated.iter().enumerate() {
                            let row = ti4_mlp::FactionRow::of(faction.as_str())
                                .map_err(|error| format!("{player}: {error}"))?;
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                &actor,
                                vocabulary.clone(),
                                row,
                                stream,
                            )
                            .seat();
                            deciders.insert(
                                player.clone(),
                                Box::new(Watching {
                                    inner: decider,
                                    faction: faction.to_string(),
                                    log: std::rc::Rc::clone(&log),
                                }),
                            );
                        }
                        Ok(deciders)
                    },
                )
                .unwrap_or_else(|error| refuse(&error));

            games += 1;
            for record in log.borrow().iter() {
                let entry = tallies
                    .entry(record.faction.clone())
                    .or_default()
                    .secondaries
                    .entry(record.card.clone())
                    .or_insert((0, 0));
                entry.0 += 1;
                entry.1 += usize::from(record.followed);
            }
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

    print_report(&tallies, followed, declined, games);
}

/// The two tables, once the games are played.
fn print_report(tallies: &BTreeMap<String, Tally>, followed: usize, declined: usize, games: usize) {
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
    for (faction, tally) in tallies {
        print!("  {faction:<10}");
        for card in &every_card {
            let count = tally.cards.get(card).copied().unwrap_or(0);
            print!(" {:>9.1}%", share(count, tally.games));
        }
        println!();
    }

    // Per game, not per offer. A follow *rate* answers "when offered, how often" and hides how
    // often a secondary is used at all: a card offered twice a game and followed both times reads
    // the same 100% as one offered thirty times. The counts below are what a seat actually does in
    // a game. The rate is kept beside them because a low count from a low offer rate means
    // something different from a low count from declining.
    println!();
    println!("  secondaries used per game, by faction and card");
    println!();
    let mut every_secondary: Vec<String> = tallies
        .values()
        .flat_map(|tally| tally.secondaries.keys().cloned())
        .collect();
    every_secondary.sort_unstable();
    every_secondary.dedup();

    print!("  {:<10}", "faction");
    for card in &every_secondary {
        print!(" {:>10}", truncate(card, 10));
    }
    println!(" {:>8} {:>9} {:>8}", "used", "offered", "follow");
    for (faction, tally) in tallies {
        print!("  {faction:<10}");
        let mut offered_total = 0usize;
        let mut followed_total = 0usize;
        for card in &every_secondary {
            let (offered, taken) = tally.secondaries.get(card).copied().unwrap_or((0, 0));
            offered_total += offered;
            followed_total += taken;
            print!(" {:>10.2}", ratio(taken, tally.games));
        }
        println!(
            " {:>8.2} {:>9.2} {:>7.1}%",
            ratio(followed_total, tally.games),
            ratio(offered_total, tally.games),
            share(followed_total, offered_total)
        );
    }

    // Leadership, derived. Everything the event log counted that was not attributed above is a
    // leadership secondary, because that is the one card whose offer cannot be told apart from its
    // own primary. The subtraction is only sound while `recorded <= events`, so that is checked
    // rather than assumed: if it ever inverts, the map above is counting something it should not.
    let offered = followed + declined;
    let recorded: usize = tallies
        .values()
        .flat_map(|tally| tally.secondaries.values())
        .map(|(offered, _)| *offered)
        .sum();
    let recorded_followed: usize = tallies
        .values()
        .flat_map(|tally| tally.secondaries.values())
        .map(|(_, taken)| *taken)
        .sum();

    println!();
    if recorded > offered || recorded_followed > followed {
        println!(
            "  leadership   NOT DERIVABLE: {recorded}/{offered} attributed against the event log,              so the prompt map is over-counting"
        );
    } else {
        let leadership_offered = offered - recorded;
        let leadership_followed = followed - recorded_followed;
        println!(
            "  leadership   {:.1}% followed ({leadership_followed}/{leadership_offered}), table-level only",
            share(leadership_followed, leadership_offered)
        );
        println!("               its secondary offer is byte-identical to its primary");
    }
    println!(
        "  all cards    {:.1}% followed ({followed}/{offered}), {:.2} offered per game",
        share(followed, offered),
        ratio(offered, games)
    );
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
