//! Audit Support for the Throne in played games.
//!
//! ```text
//! cargo run --release -p ti4-training --example support_audit -- [--games 300] [--rounds 5]
//! ```
//!
//! Every activation, whatever drove it (a tactical action, Thunder's Edge Warfare's free tactical
//! action, Overrule performing Warfare), must return each Support for the Throne the activating
//! player holds whose owner has units in the activated system, take the holder's point with it,
//! and return no other. At the end of each game the VP ledger must agree with the notes still held.
//!
//! Seats are seeded random deciders over FULL content: they take strategic actions, trade notes and
//! activate systems far more evenly than a trained policy does, which is what coverage needs.

#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    reason = "a driver: one pass per step"
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::{Decider, SeededRandom};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_training::rollout::{OpeningMap, seated_faction, setup_game_with_decider_factory};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const MAX_STEPS: usize = 200_000;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn parsed<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value.parse().unwrap_or_else(|_| {
            eprintln!("{name} could not parse {value:?}");
            std::process::exit(2)
        })
    })
}

/// A seeded random seat that also counts every decision it is asked, by typed subtype.
struct Counting {
    inner: SeededRandom,
    asked: std::rc::Rc<std::cell::RefCell<BTreeMap<String, usize>>>,
}

impl Decider for Counting {
    fn choose(
        &mut self,
        choice: &ti4_engine::choice::Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        if let Some(context) = &choice.context {
            *self
                .asked
                .borrow_mut()
                .entry(context.subtype.clone())
                .or_default() += 1;
        }
        self.inner.choose(choice)
    }
}

#[derive(Default)]
struct Tally {
    games: usize,
    failed: usize,
    steps: usize,
    activations: usize,
    tactical: usize,
    warfare: usize,
    other: usize,
    /// A Support held by the activating player at the moment of activation.
    held_at_activation: usize,
    /// ... whose owner had units in the activated system: the note must go home.
    owner_present: usize,
    returned: usize,
    /// The note stayed although the owner was present.
    kept_wrongly: usize,
    /// The note went home although the owner was absent.
    returned_wrongly: usize,
    vp_not_taken: usize,
    ledger_mismatch: usize,
    new_holdings: usize,
    returned_events: usize,
    /// Games in which the Fracture came into play.
    fracture_games: usize,
    /// Activations of a system that held neutral ships.
    activations_into_neutral: usize,
    /// A seated player's ships still sharing a system with neutral ships after a tactical action:
    /// a space combat (neutral rule 2) that never happened.
    neutral_coexisting: usize,
    /// A seated player's unit inside a Fracture system while the Fracture is not in play.
    early_fracture_units: usize,
    /// Promissory notes handed to a player other than their owner, by alias.
    notes_lent: BTreeMap<String, usize>,
    /// Lent notes that came home outside a transaction -- i.e. were used -- by alias.
    notes_used: BTreeMap<String, usize>,
    /// Games in which some seat had Harrugh Gefhara (Hacan hero) unlocked.
    hacan_hero_games: usize,
    /// Times Harrugh Gefhara was purged (used).
    hacan_hero_used: usize,
    /// Decisions offered for the newly wired note and hero effects, by decision subtype.
    offers: BTreeMap<String, usize>,
    /// Agendas revealed across all games (Political Favor and Political Secret need them).
    agendas_revealed: usize,
    /// Correct returns, by the path that activated: coverage of Warfare and Overrule is the point.
    returned_by_path: BTreeMap<&'static str, usize>,
    examples: Vec<String>,
}

impl Tally {
    fn merge(mut self, other: Self) -> Self {
        self.games += other.games;
        self.failed += other.failed;
        self.steps += other.steps;
        self.activations += other.activations;
        self.tactical += other.tactical;
        self.warfare += other.warfare;
        self.other += other.other;
        self.held_at_activation += other.held_at_activation;
        self.owner_present += other.owner_present;
        self.returned += other.returned;
        self.kept_wrongly += other.kept_wrongly;
        self.returned_wrongly += other.returned_wrongly;
        self.vp_not_taken += other.vp_not_taken;
        self.ledger_mismatch += other.ledger_mismatch;
        self.new_holdings += other.new_holdings;
        self.returned_events += other.returned_events;
        self.fracture_games += other.fracture_games;
        self.activations_into_neutral += other.activations_into_neutral;
        self.neutral_coexisting += other.neutral_coexisting;
        self.early_fracture_units += other.early_fracture_units;
        for (alias, count) in other.notes_lent {
            *self.notes_lent.entry(alias).or_default() += count;
        }
        for (alias, count) in other.notes_used {
            *self.notes_used.entry(alias).or_default() += count;
        }
        self.hacan_hero_games += other.hacan_hero_games;
        self.hacan_hero_used += other.hacan_hero_used;
        for (subtype, count) in other.offers {
            *self.offers.entry(subtype).or_default() += count;
        }
        self.agendas_revealed += other.agendas_revealed;
        for (path, count) in other.returned_by_path {
            *self.returned_by_path.entry(path).or_default() += count;
        }
        self.examples.extend(other.examples);
        self
    }
}

fn audit(seed: u64, rounds: u32) -> Tally {
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let rotation = usize::try_from(seed % FACTIONS.len() as u64).unwrap_or(0);
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                seated_faction(&FACTIONS.map(FactionId::new), seed, rotation, index),
            )
        })
        .collect();
    let mut tally = Tally {
        games: 1,
        ..Tally::default()
    };
    // Every decision the seats are asked, by typed subtype: whether an effect is ever *offered* is
    // a different question from whether the random seats happened to take it.
    let asked: std::rc::Rc<std::cell::RefCell<BTreeMap<String, usize>>> =
        std::rc::Rc::new(std::cell::RefCell::new(BTreeMap::new()));
    let setup = setup_game_with_decider_factory(
        content,
        &players,
        &factions,
        FULL,
        seed,
        &OpeningMap::RustVaried,
        |_| {
            Ok(players
                .iter()
                .enumerate()
                .map(|(index, player)| {
                    let stream = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    (
                        player.clone(),
                        Box::new(Counting {
                            inner: SeededRandom::new(stream),
                            asked: std::rc::Rc::clone(&asked),
                        }) as Box<dyn Decider>,
                    )
                })
                .collect())
        },
    );
    let mut game = match setup {
        Ok(game) => game,
        Err(error) => {
            tally.failed = 1;
            tally
                .examples
                .push(format!("seed {seed}: setup failed: {error}"));
            return tally;
        }
    };

    let target = game.state.round.saturating_add(rounds);
    let types = ti4_content::units::catalogue(content, FULL);
    let is_ship = |unit: &ti4_model::units::Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(ti4_content::units::UnitType::is_ship)
    };
    // The last few steps' events and phase, attached to any violation so it can be diagnosed
    // without replaying the game.
    let mut recent: VecDeque<String> = VecDeque::new();
    // When each system first came to hold a seated player's ships beside neutral ships, and which
    // of those pairings have been reported already.
    let mut coexist_since: BTreeMap<ti4_model::id::SystemId, String> = BTreeMap::new();
    let mut reported: BTreeSet<(ti4_model::id::SystemId, String)> = BTreeSet::new();
    // Whether this game has already reported a seated unit inside a Fracture that is not in play.
    let mut early_fracture_reported = false;
    // Whether any seat had Harrugh Gefhara unlocked at some point in this game.
    let mut hacan_hero_seen = false;
    // `--inspect-step N`: dump the state around one step, for replaying a single failing seed.
    let inspect: Option<usize> = argument("--inspect-step").and_then(|value| value.parse().ok());
    let owners_in = |board: &BTreeMap<ti4_model::id::SystemId, ti4_model::state::SystemState>,
                     system: &Option<ti4_model::id::SystemId>| {
        system
            .as_ref()
            .and_then(|id| board.get(id))
            .map(|here| {
                here.units
                    .iter()
                    .chain(here.planet_units.values().flatten())
                    .map(|unit| format!("{}:{}", unit.owner, unit.type_id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    while !game.state.finished && game.state.round < target && tally.steps < MAX_STEPS {
        let seq = game.state.activation_seq;
        // The activator is whoever was acting when the step began. An action-card activation
        // (Overrule performing Warfare) ends the turn inside the same step, so the active player
        // afterwards is already the next seat -- reading it then blamed the wrong player.
        let active_before = game.state.active.clone();
        let system_before = game.state.active_system.clone();
        let holders = game.state.support_holders.clone();
        let points_before: BTreeMap<PlayerId, _> = players
            .iter()
            .filter_map(|p| {
                game.state
                    .player(p)
                    .map(|seat| (p.clone(), seat.victory_points))
            })
            .collect();
        let events_before = game.events.len();
        let notes_before = game.state.promissory_notes.clone();
        // The board as it stood when an activation in this step happened. A card played into the
        // activation window can move units in afterwards (Decoy Operation), and the Support trigger
        // reads the system at activation, not after the window.
        let board_before = game.state.board.clone();
        let hero = ti4_model::id::LeaderId::new("hacanhero");
        let hero_before: Vec<Option<ti4_model::state::LeaderStatus>> = players
            .iter()
            .map(|p| {
                game.state
                    .player(p)
                    .and_then(|seat| seat.leaders.get(&hero).copied())
            })
            .collect();
        let inspecting = inspect == Some(tally.steps + 1);
        let before_detail = inspecting.then(|| {
            (
                players
                    .iter()
                    .map(|p| {
                        (
                            p.clone(),
                            game.state
                                .player(p)
                                .map(|seat| seat.action_cards.clone())
                                .unwrap_or_default(),
                        )
                    })
                    .collect::<Vec<_>>(),
                game.state.active.clone(),
                game.state.board.clone(),
            )
        });

        let result = game.step();
        tally.steps += 1;
        if let Some(error) = result.error {
            tally.failed += 1;
            tally.examples.push(format!(
                "seed {seed}: step {} refused: {error:?}",
                tally.steps
            ));
            break;
        }
        let new_events = &game.events[events_before..];
        // Promissory note usage: a note that goes to a non-owner is lent; a lent note that comes
        // home in a step with no transaction was used.
        let traded = new_events
            .iter()
            .any(|event| event.starts_with("TRANSACTION"));
        for (note, holder) in &game.state.promissory_notes {
            let Some(owner) = ti4_engine::promissory::owner_of(note)
                .and_then(|name| ti4_engine::promissory::seat_of(&game.state, &name))
            else {
                continue;
            };
            let alias = ti4_engine::promissory::alias_of(note).to_owned();
            let before = notes_before.get(note);
            let was_lent = before.is_some_and(|previous| *previous != owner);
            let is_lent = *holder != owner;
            if is_lent && !was_lent {
                *tally.notes_lent.entry(alias).or_default() += 1;
            } else if was_lent && !is_lent && !traded {
                *tally.notes_used.entry(alias).or_default() += 1;
            }
        }
        for (index, p) in players.iter().enumerate() {
            let now = game
                .state
                .player(p)
                .and_then(|seat| seat.leaders.get(&hero).copied());
            if hero_before[index] == Some(ti4_model::state::LeaderStatus::Unlocked)
                && now == Some(ti4_model::state::LeaderStatus::Purged)
            {
                tally.hacan_hero_used += 1;
            }
            if now == Some(ti4_model::state::LeaderStatus::Unlocked) {
                hacan_hero_seen = true;
            }
        }
        if let Some((hands, active_before, board_before)) = &before_detail {
            eprintln!("== inspect step {} (seed {seed})", tally.steps);
            eprintln!("  events: {}", new_events.join(" | "));
            for (player, hand) in hands {
                let now = game
                    .state
                    .player(player)
                    .map(|seat| seat.action_cards.clone())
                    .unwrap_or_default();
                let played: Vec<_> = hand.iter().filter(|card| !now.contains(card)).collect();
                if !played.is_empty() {
                    eprintln!("  {player} hand lost {played:?}");
                }
            }
            eprintln!(
                "  active {active_before:?} -> {:?}; active_system {system_before:?} -> {:?}; activation_seq {seq} -> {}",
                game.state.active, game.state.active_system, game.state.activation_seq
            );
            eprintln!(
                "  support holders {holders:?} -> {:?}",
                game.state.support_holders
            );
            eprintln!(
                "  units in the new active system before {:?}",
                owners_in(board_before, &game.state.active_system)
            );
            eprintln!(
                "  units in the new active system after  {:?}",
                owners_in(&game.state.board, &game.state.active_system)
            );
        }
        recent.push_back(format!(
            "step {} {:?} active {:?} active_system {:?}: {}",
            tally.steps,
            game.state.phase,
            game.state.active,
            game.state.active_system,
            new_events.join(" | ")
        ));
        if recent.len() > 12 {
            recent.pop_front();
        }
        tally.returned_events += new_events
            .iter()
            .filter(|event| event.starts_with("SUPPORT_FOR_THE_THRONE_RETURNED:"))
            .count();
        tally.new_holdings += game
            .state
            .support_holders
            .iter()
            .filter(|(owner, holder)| holders.get(*owner) != Some(*holder))
            .count();

        // The Fracture is off the map until it comes into play: no seated player may have a unit
        // in one of its systems before then. Reported once per game, at the first step it happens.
        if !game.state.fracture_in_play && !early_fracture_reported {
            let intruders: Vec<String> = game
                .state
                .board
                .iter()
                .filter(|(id, _)| id.as_str().starts_with("fracture"))
                .flat_map(|(id, here)| {
                    here.units
                        .iter()
                        .chain(here.planet_units.values().flatten())
                        .filter(|unit| unit.owner.as_str() != "neutral")
                        .map(move |unit| format!("{id}:{}:{}", unit.owner, unit.type_id))
                })
                .collect();
            if !intruders.is_empty() {
                early_fracture_reported = true;
                tally.early_fracture_units += 1;
                tally.examples.push(format!(
                    "seed {seed} round {}: seated units in the Fracture before it is in play {intruders:?}; step {} {:?} active {:?} active_system {:?}: {}",
                    game.state.round,
                    tally.steps,
                    game.state.phase,
                    active_before,
                    game.state.active_system,
                    new_events.join(" | ")
                ));
            }
        }

        // Neutral rule 2: a seated player's ships may share a system with neutral ships only while
        // the tactical action that brought them together is still resolving. Track the step each
        // such pairing first appears at, and report it once if it outlives a tactical action.
        let mut coexisting_now: BTreeSet<ti4_model::id::SystemId> = BTreeSet::new();
        for (id, here) in &game.state.board {
            let neutral_ships = here
                .units
                .iter()
                .any(|unit| is_ship(unit) && unit.owner.as_str() == "neutral");
            let seated_ships = here
                .units
                .iter()
                .any(|unit| is_ship(unit) && unit.owner.as_str() != "neutral");
            if neutral_ships && seated_ships {
                coexisting_now.insert(id.clone());
            }
        }
        coexist_since.retain(|id, _| coexisting_now.contains(id));
        for id in &coexisting_now {
            coexist_since.entry(id.clone()).or_insert_with(|| {
                format!(
                    "step {} {:?} active {:?} active_system {:?} units {:?}: {}",
                    tally.steps,
                    game.state.phase,
                    active_before,
                    game.state.active_system,
                    owners_in(&game.state.board, &Some(id.clone())),
                    new_events.join(" | ")
                )
            });
        }
        if new_events
            .iter()
            .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        {
            for (id, since) in &coexist_since {
                if reported.insert((id.clone(), since.clone())) {
                    tally.neutral_coexisting += 1;
                    tally.examples.push(format!(
                        "seed {seed} round {}: {id} holds seated and neutral ships after a tactical action; together since {since}",
                        game.state.round
                    ));
                }
            }
        }

        let activated = game.state.activation_seq != seq
            || (game.state.active_system.is_some() && game.state.active_system != system_before);
        if !activated {
            continue;
        }
        let (Some(system), Some(activator)) =
            (game.state.active_system.clone(), active_before.clone())
        else {
            continue;
        };
        tally.activations += 1;
        if game
            .state
            .system_state(&system)
            .units
            .iter()
            .any(|unit| is_ship(unit) && unit.owner.as_str() == "neutral")
        {
            tally.activations_into_neutral += 1;
        }
        let path = if new_events
            .iter()
            .any(|event| event == "FREE_TACTICAL_ACTION")
        {
            tally.warfare += 1;
            "warfare"
        } else if new_events
            .iter()
            .any(|event| event.starts_with("SYSTEM_ACTIVATED:"))
        {
            tally.tactical += 1;
            "tactical"
        } else {
            tally.other += 1;
            "other"
        };

        let board = board_before.get(&system).cloned().unwrap_or_default();
        let mut present: BTreeSet<PlayerId> =
            board.units.iter().map(|unit| unit.owner.clone()).collect();
        present.extend(
            board
                .planet_units
                .values()
                .flatten()
                .map(|unit| unit.owner.clone()),
        );

        for (owner, holder) in &holders {
            if *holder != activator {
                continue;
            }
            tally.held_at_activation += 1;
            let still_held = game.state.support_holders.get(owner) == Some(holder);
            if present.contains(owner) {
                tally.owner_present += 1;
                if still_held {
                    tally.kept_wrongly += 1;
                    for line in &recent {
                        tally
                            .examples
                            .push(format!("    seed {seed} trace: {line}"));
                    }
                    tally.examples.push(format!(
                        "seed {seed} round {}: {activator} activated {system} ({path}) with {owner}'s units present and kept {owner}'s Support",
                        game.state.round
                    ));
                } else {
                    tally.returned += 1;
                    *tally.returned_by_path.entry(path).or_default() += 1;
                    let before = points_before.get(holder).copied();
                    let after = game.state.player(holder).map(|seat| seat.victory_points);
                    if let (Some(before), Some(after)) = (before, after)
                        && before > 0
                        && after != before - 1
                    {
                        tally.vp_not_taken += 1;
                        tally.examples.push(format!(
                            "seed {seed}: {holder} returned {owner}'s Support ({path}) but VP went {before} -> {after}"
                        ));
                    }
                }
            } else if !still_held {
                tally.returned_wrongly += 1;
                tally.examples.push(format!(
                    "seed {seed} round {}: {activator} activated {system} ({path}) without {owner}'s units and lost {owner}'s Support",
                    game.state.round
                ));
            }
        }
    }

    tally.fracture_games += usize::from(game.state.fracture_in_play);
    tally.hacan_hero_games += usize::from(hacan_hero_seen);
    tally.agendas_revealed += game
        .events
        .iter()
        .filter(|event| event.starts_with("AGENDA_REVEALED:"))
        .count();
    for (subtype, count) in asked.borrow().iter() {
        if matches!(
            subtype.as_str(),
            "favor" | "ps" | "war_funding" | "leader_hacanhero_free_production"
        ) {
            *tally.offers.entry(subtype.clone()).or_default() += count;
        }
    }
    for player in &players {
        let ledger: i64 = game
            .state
            .vp_ledger
            .iter()
            .filter(|(who, _, reason)| who == player && reason == "support_for_the_throne")
            .map(|(_, delta, _)| i64::from(*delta))
            .sum();
        let held = game
            .state
            .support_holders
            .values()
            .filter(|holder| *holder == player)
            .count();
        if ledger != i64::try_from(held).unwrap_or(-1) {
            tally.ledger_mismatch += 1;
            tally.examples.push(format!(
                "seed {seed}: {player} holds {held} Support(s) but the ledger credits {ledger}"
            ));
        }
    }
    tally
}

fn main() {
    let games: u64 = parsed("--games", 300);
    let rounds: u32 = parsed("--rounds", 5);
    let seed_base: u64 = parsed("--seed-base", 7_300_000_000);
    let started = std::time::Instant::now();
    let tally = (seed_base..seed_base + games)
        .into_par_iter()
        .map(|seed| audit(seed, rounds))
        .reduce(Tally::default, Tally::merge);

    println!("Support for the Throne audit");
    println!(
        "  games {} ({} failed), {} rounds each, {} steps, {:.0}s",
        tally.games,
        tally.failed,
        rounds,
        tally.steps,
        started.elapsed().as_secs_f64()
    );
    println!(
        "  activations {} (tactical {}, warfare {}, other {})",
        tally.activations, tally.tactical, tally.warfare, tally.other
    );
    println!(
        "  Supports received {}, returned events {}",
        tally.new_holdings, tally.returned_events
    );
    println!(
        "  held by the activator at activation {}; owner present {} -> returned {}, kept wrongly {}",
        tally.held_at_activation, tally.owner_present, tally.returned, tally.kept_wrongly
    );
    println!(
        "  returned although owner absent {}; point not taken on return {}; ledger mismatches {}",
        tally.returned_wrongly, tally.vp_not_taken, tally.ledger_mismatch
    );
    println!("  correct returns by path {:?}", tally.returned_by_path);
    println!(
        "  neutral units: Fracture in play in {} games, {} activations into neutral ships, {} systems left with seated and neutral ships after a tactical action",
        tally.fracture_games, tally.activations_into_neutral, tally.neutral_coexisting
    );
    println!(
        "  games with seated units inside the Fracture before it was in play: {}",
        tally.early_fracture_units
    );
    println!(
        "  promissory notes lent (to a non-owner), by alias: {:?}",
        tally.notes_lent
    );
    println!(
        "  promissory notes used (came home outside a transaction), by alias: {:?}",
        tally.notes_used
    );
    println!(
        "  Harrugh Gefhara (Hacan hero): unlocked in {} games, used {} times",
        tally.hacan_hero_games, tally.hacan_hero_used
    );
    println!(
        "  agendas revealed {}; offers of the newly wired effects, by decision subtype: {:?}",
        tally.agendas_revealed, tally.offers
    );
    for example in tally.examples.iter().take(15) {
        println!("  - {example}");
    }
    let violations = tally.kept_wrongly
        + tally.returned_wrongly
        + tally.vp_not_taken
        + tally.ledger_mismatch
        + tally.neutral_coexisting
        + tally.early_fracture_units;
    println!(
        "  verdict: {}",
        if violations == 0 { "PASS" } else { "FAIL" }
    );
}
