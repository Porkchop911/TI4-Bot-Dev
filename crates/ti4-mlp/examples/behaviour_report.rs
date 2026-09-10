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
//! Strategy picks are recorded **at the decision**, like the secondaries below. Two earlier
//! versions read them from the state instead and both answered a different question than the one
//! asked. Reading the *final* state printed an empty table: cards return to the common pool in the
//! status phase, so `PlayerState::strategy_cards` is empty by the end of a round and a final
//! reading says "which card does this faction hold now" — always "none". Reading the snapshot
//! `audit_game_with_deciders` returns fixed that, but that snapshot is taken the first time the
//! strategy phase ends and never again, so at `--rounds 4` it silently reported **round one only**
//! — every faction's shares summing to exactly 100% when four cards per game were actually played.
//! Watching the draft decision is the only source that sees every round.
//!
//! The monoculture columns exist because that second bug was invisible without them: `/game` must
//! equal the round count, and `top`/`mono` restate the per-game concentration on the exact terms
//! `reward::returns` charges `--strategy-diversity-weight` on, so the penalty can be seen to bind.
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
use ti4_model::content_types::{ContentType, DEFAULT};
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

/// The prompt `ti4_engine::draft::strategy_options` builds for the primary pick.
///
/// Duplicated from the engine for the same reason [`Watching::secondary_card`]'s table is: a
/// reworded prompt would silently stop being counted. Unlike the secondaries there is no event
/// cross-check to catch that, so the picks-per-game column below is printed as the guard -- it
/// should equal the round count, and a zero there means this string went stale.
const STRATEGY_DRAFT_PROMPT: &str = "choose a strategy card";

/// The monoculture share `reward::returns` starts charging above, and the minimum plays it needs.
const MONOCULTURE_SHARE: f64 = 0.8;
const MONOCULTURE_MINIMUM_PLAYS: usize = 3;
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
    /// Every strategy card this faction picked, across every round.
    picks: usize,
    /// Games in which this faction played at least three cards and its most-played card was more
    /// than 80% of them -- the exact condition `reward::returns` charges the monoculture penalty
    /// on, counted here so the term can be seen to bind or not.
    monoculture_games: usize,
    /// Games with at least three plays, the denominator [`Tally::monoculture_games`] belongs over.
    judgeable_games: usize,
    /// Summed per-game share of this faction's most-played card, over [`Tally::judgeable_games`].
    concentration: f64,
    /// Per strategy card, how often this faction was offered its secondary and took it.
    secondaries: BTreeMap<String, (usize, usize)>,
    technologies: BTreeMap<String, usize>,
    /// Summed end-of-game fleet value in per-mille resources, over [`Tally::games`].
    fleet_permille: i64,
}

/// One strategy card taken as a primary, in one round of one game.
///
/// Recorded at the decision rather than read back from the state. The state carries only the round
/// this pick belongs to -- cards return to the common pool every status phase -- so a snapshot can
/// answer for one round and no more.
struct Pick {
    faction: String,
    card: String,
}

/// One recorded secondary decision.
struct Secondary {
    faction: String,
    card: String,
    followed: bool,
}

/// One technology actually selected in one reproducible game.
struct Research {
    seed: u64,
    rotation: usize,
    round: u32,
    faction: String,
    technology: String,
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
    research: std::rc::Rc<std::cell::RefCell<Vec<Research>>>,
    picks: std::rc::Rc<std::cell::RefCell<Vec<Pick>>>,
    seed: u64,
    rotation: usize,
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
        // The strategy-card draft, from `draft::strategy_options`. Recorded here rather than read
        // from the state snapshot `audit_game_with_deciders` returns: that snapshot is taken the
        // first time the strategy phase ends and never again, so it answers for round one however
        // many rounds the game ran. The option id is the corpus card id, which is what the columns
        // below are keyed on.
        if choice.prompt == STRATEGY_DRAFT_PROMPT {
            self.picks.borrow_mut().push(Pick {
                faction: self.faction.clone(),
                card: chosen.id.clone(),
            });
            return;
        }
        if ti4_content::ContentStore::embedded()
            .get(ContentType::Technologies, &chosen.id)
            .is_some()
        {
            self.research.borrow_mut().push(Research {
                seed: self.seed,
                rotation: self.rotation,
                round: choice.context.as_ref().map_or(0, |context| context.round),
                faction: self.faction.clone(),
                technology: chosen.id.clone(),
            });
        }
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

#[expect(
    clippy::too_many_lines,
    reason = "one pass over the sampled games; splitting the tally out would separate the counting from the definition of what is being counted"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| {
        refuse("--bundle is required: the report describes a specific checkpoint")
    });
    // Greedy by default. The report describes what the policy *does*, and what it does when
    // evaluated is take its argmax; reading it at 1.0 describes a distribution nobody plays.
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
            .unwrap_or_else(|| refuse("--temperature expects a positive number"))
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
    let mut research_examples = Vec::new();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let log: std::rc::Rc<std::cell::RefCell<Vec<Secondary>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let research: std::rc::Rc<std::cell::RefCell<Vec<Research>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let picks: std::rc::Rc<std::cell::RefCell<Vec<Pick>>> =
                std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let (events, _round_one, assignments, _openings, final_state) =
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
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            // Seated with its own setup baseline. Without it the bot reports
                            // absolute holdings through features trained as gains since setup, and
                            // the evaluation measures a different model than the one under test.
                            let baseline = baselines
                                .get(player)
                                .copied()
                                .ok_or_else(|| format!("{player} has no setup baseline"))?;
                            let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                &actor,
                                vocabulary.clone(),
                                row,
                                stream,
                            )
                            .at_temperature(temperature)
                            .from_setup(baseline)
                            .seat();
                            deciders.insert(
                                player.clone(),
                                Box::new(Watching {
                                    inner: decider,
                                    faction: faction.to_string(),
                                    log: std::rc::Rc::clone(&log),
                                    research: std::rc::Rc::clone(&research),
                                    picks: std::rc::Rc::clone(&picks),
                                    seed,
                                    rotation,
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
            for record in research.borrow_mut().drain(..) {
                *tallies
                    .entry(record.faction.clone())
                    .or_default()
                    .technologies
                    .entry(record.technology.clone())
                    .or_default() += 1;
                if content
                    .get(ContentType::Technologies, &record.technology)
                    .is_some_and(|technology| {
                        technology
                            .strings("types")
                            .iter()
                            .any(|kind| kind.eq_ignore_ascii_case("UNITUPGRADE"))
                    })
                {
                    research_examples.push(record);
                }
            }
            for event in &events {
                match event.as_str() {
                    "STRATEGY_SECONDARY_FOLLOWED" => followed += 1,
                    "STRATEGY_SECONDARY_DECLINED" => declined += 1,
                    _ => {}
                }
            }
            // Picks, from the decisions rather than from `state` -- see `Watching::record`. Tallied
            // per game so the monoculture share can be computed the way the reward computes it:
            // per seat, per game, over that game's plays.
            let mut per_game: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
            for pick in picks.borrow_mut().drain(..) {
                *per_game
                    .entry(pick.faction)
                    .or_default()
                    .entry(pick.card)
                    .or_default() += 1;
            }
            for (faction, cards) in per_game {
                let tally = tallies.entry(faction).or_default();
                let total: usize = cards.values().sum();
                let most = cards.values().copied().max().unwrap_or(0);
                for (card, count) in cards {
                    *tally.cards.entry(card).or_default() += count;
                }
                tally.picks += total;
                if total >= MONOCULTURE_MINIMUM_PLAYS {
                    #[expect(clippy::cast_precision_loss, reason = "card counts are single digits")]
                    let share = (most as f64) / (total as f64);
                    tally.judgeable_games += 1;
                    tally.concentration += share;
                    if share > MONOCULTURE_SHARE {
                        tally.monoculture_games += 1;
                    }
                }
            }

            // Fleet value at the end of the game, from the final state rather than the round-one
            // pick snapshot. `Observed` is the same accessor the reward's `--fleet-weight` term
            // reads, so this column and that coefficient cannot drift apart.
            let seen = ti4_engine::choice::Observed::new(&final_state, content, DEFAULT, None);
            for player in &final_state.players {
                let Some(faction) = assignments.get(&player.id) else {
                    continue;
                };
                let tally = tallies.entry(faction.to_string()).or_default();
                tally.games += 1;
                tally.fleet_permille += seen.fleet_value_permille(&player.id);
            }
        }
    }

    if games == 0 {
        refuse("no games were played");
    }

    print_report(&tallies, followed, declined, games, &research_examples);
}

/// The two tables, once the games are played.
/// Which strategy card each faction takes, over every round it played.
///
/// Share of that faction's *picks*, not of its games. A seat takes one card per round, so at four
/// rounds a per-game share would sum to 400%; the version of this that read a round-one snapshot
/// summed to exactly 100% and looked correct while answering a narrower question.
fn print_picks(tallies: &BTreeMap<String, Tally>) {
    // Every card that appeared anywhere, so the table has stable columns.
    let mut every_card: Vec<String> = tallies
        .values()
        .flat_map(|tally| tally.cards.keys().cloned())
        .collect();
    every_card.sort_unstable();
    every_card.dedup();

    println!("\n  strategy card picks, share of that faction's picks across all rounds\n");
    print!("  {:<10}", "faction");
    for card in &every_card {
        print!(" {:>10}", truncate(card, 10));
    }
    println!(" {:>7} {:>7} {:>8}", "/game", "top", "mono");
    for (faction, tally) in tallies {
        print!("  {faction:<10}");
        for card in &every_card {
            let count = tally.cards.get(card).copied().unwrap_or(0);
            print!(" {:>9.1}%", share(count, tally.picks));
        }
        // `/game` is the staleness guard named on STRATEGY_DRAFT_PROMPT: it should equal the round
        // count. `top` is the mean per-game share of the most-played card and `mono` the fraction
        // of judgeable games above the penalty's threshold -- together, whether the monoculture
        // term actually binds for this faction.
        #[expect(
            clippy::cast_precision_loss,
            reason = "game counts are far below f64's exact-integer range"
        )]
        let concentration = if tally.judgeable_games == 0 {
            0.0
        } else {
            tally.concentration / tally.judgeable_games as f64
        };
        println!(
            " {:>7.2} {:>6.1}% {:>7.1}%",
            ratio(tally.picks, tally.games),
            concentration * 100.0,
            share(tally.monoculture_games, tally.judgeable_games)
        );
    }
}

fn print_report(
    tallies: &BTreeMap<String, Tally>,
    followed: usize,
    declined: usize,
    games: usize,
    research_examples: &[Research],
) {
    print_picks(tallies);

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

    // Per faction, not just the table total below. The aggregate cannot say whether one seat is
    // researching for everyone, which is exactly the question `--tech-weight` raises.
    println!();
    println!("  technologies researched and end-of-game fleet, per faction");
    println!();
    println!(
        "  {:<10} {:>9} {:>10} {:>12}",
        "faction", "techs", "techs/game", "fleet value"
    );
    for (faction, tally) in tallies {
        println!(
            "  {faction:<10} {:>9} {:>10.2} {:>12.2}",
            tally.technologies.values().sum::<usize>(),
            ratio(tally.technologies.values().sum::<usize>(), tally.games),
            fleet_resources(tally.fleet_permille, tally.games)
        );
    }

    println!();
    println!("  unit-upgrade research");
    let researched: usize = tallies
        .values()
        .flat_map(|tally| tally.technologies.values())
        .sum();
    println!(
        "  selected {} unit upgrade(s) among {researched} technology choices",
        research_examples.len()
    );
    for example in research_examples.iter().take(12) {
        println!(
            "  seed {} rotation {} round {}: {} researched {}",
            example.seed, example.rotation, example.round, example.faction, example.technology
        );
    }
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

/// Mean fleet value per game, converted out of the per-mille units the accessor reports in.
#[expect(
    clippy::cast_precision_loss,
    reason = "per-mille fleet sums are exact in f64 far beyond any sample size"
)]
fn fleet_resources(permille: i64, games: usize) -> f64 {
    if games == 0 {
        return 0.0;
    }
    permille as f64 / 1000.0 / games as f64
}
