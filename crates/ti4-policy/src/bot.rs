//! A scored bot: routes every choice kind to a scorer, then samples (M08-004, M08-005b, M08-011).
//!
//! Ported from the oracle's `bots.py` `ScoredBot._raw_score`, `_worth_considering` and `_sample`.
//!
//! # What this bot can and cannot see
//!
//! Plain [`ti4_engine::choice::Decider::choose`] is intentionally blind: it scores from the
//! choice's kind, label, payload, and the content corpus. The game driver calls
//! [`ti4_engine::choice::Decider::choose_seeing`] with a public [`Observed`] view. That path adds
//! board-aware activation and movement components without lending the decider a state reference or
//! a hand.
//!
//! Concretely:
//!
//! - It always takes a scoring opportunity, because scoring is the only thing that wins.
//! - It prefers acting to passing, moving to standing still, and committing troops to declining —
//!   which is where a uniform-random table loses its games, not in choosing between good moves.
//! - It loses the cheapest unit to a hit, pays a bill in one exhaustion rather than two, and
//!   builds by value per resource.
//! - With an observation it values activation prizes, removes unreachable non-production
//!   activations from its own shortlist, and moves useful hulls rather than parading extras into
//!   an already-secured system.
//! - Cargo, landings, combat, and production remain M08-005c work; they deliberately retain their
//!   blind fallback until their public facts and decision-boundary tests are added.

use std::collections::BTreeMap;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed, SeatObservation};
use ti4_engine::production::Spend;
use ti4_model::content_types::{ContentType, POK, SourceSet};
use ti4_model::id::SystemId;

use crate::scoring::{Components, Decision};

#[cfg(test)]
std::thread_local! {
    static AUTHORED_SCORE_HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static AUTHORED_FILTER_HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn instrument_authored_score() {
    AUTHORED_SCORE_HITS.set(AUTHORED_SCORE_HITS.get() + 1);
}

#[cfg(test)]
fn instrument_authored_filter() {
    AUTHORED_FILTER_HITS.set(AUTHORED_FILTER_HITS.get() + 1);
}

#[cfg(test)]
pub(crate) fn authored_path_hits(reset: bool) -> (usize, usize) {
    let hits = (AUTHORED_SCORE_HITS.get(), AUTHORED_FILTER_HITS.get());
    if reset {
        AUTHORED_SCORE_HITS.set(0);
        AUTHORED_FILTER_HITS.set(0);
    }
    hits
}

/// How sharply the bot prefers its best option.
///
/// Sampled rather than taken outright, deliberately: an argmax bot is solvable, plays the same
/// game every time from a given position, and gives a learner one trajectory where it needs many.
pub const TEMPERATURE: f64 = 1.5;

/// A bot that scores each option, shortlists, and samples.
pub struct ScoredBot {
    rng: ChaCha8Rng,
    content: &'static ContentStore,
    sources: SourceSet,
    temperature: f64,
    /// Every decision this bot has made, for explanation and for training capture.
    pub decisions: Vec<Decision>,
    /// Whether to keep the decision log. Off in a batch of ten thousand games, where the log is
    /// larger than everything else put together.
    remember: bool,
}

/// A stable feature name per promissory note.
///
/// The ten notes with a printed worth get their own bucket, so the learner can price a Research
/// Agreement differently from a Ceasefire. Anything else lands in `note:other` -- visible, and
/// distinguishable from the `unknown_trade` bucket that used to swallow all of them.
fn note_feature(alias: &str) -> &'static str {
    match alias {
        "ra" => "note:ra",
        "an" => "note:an",
        "convoys" => "note:convoys",
        "ta" => "note:ta",
        "ce" => "note:ce",
        "ms" => "note:ms",
        "favor" => "note:favor",
        "war_funding" => "note:war_funding",
        "ps" => "note:ps",
        "cf" => "note:cf",
        _ => "note:other",
    }
}

impl ScoredBot {
    /// A bot with its own deterministic stream.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: ChaCha8Rng::seed_from_u64(seed),
            content: ContentStore::embedded(),
            sources: POK,
            temperature: TEMPERATURE,
            decisions: Vec::new(),
            remember: false,
        }
    }

    /// Keep a decision log. Off by default: a batch run does not want one.
    #[must_use]
    pub const fn remembering(mut self) -> Self {
        self.remember = true;
        self
    }

    /// Play at a different temperature. Near zero is argmax; large is near-uniform.
    #[must_use]
    pub const fn at_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    /// Play under a different content scope.
    #[must_use]
    pub const fn with_sources(mut self, sources: SourceSet) -> Self {
        self.sources = sources;
        self
    }

    /// Named components for one option. The dispatcher.
    ///
    /// An unrecognised kind scores empty rather than panicking or guessing: the option stays
    /// legal and rankable, ties break by sampling, and [`unscored_kinds`] can count what nobody
    /// has taught the bot to judge.
    #[must_use]
    pub fn raw_score(&self, choice: &Choice, option: &ChoiceOption) -> Components {
        #[cfg(test)]
        instrument_authored_score();
        match option.kind.as_str() {
            // Scoring is the only thing that wins a game, so it dominates every other reason.
            // When two legal objectives are available, their printed points decide which score
            // finishes the status window.
            "score" => self.score_objective(option),

            // Declining is worth something — a real option, often the right one — but it is the
            // baseline every other option is measured against, not a competitor for the top.
            "decline" => Components::of("baseline", 1.0),

            // Doing something beats passing. A table that passes its way through the action phase
            // is where a random bot loses its games.
            "action" => self.score_action(option),

            // The board judgements this bot cannot make. Scored above declining so the bot acts,
            // and flat so it does not pretend to prefer one system to another.
            "activate" => Components::of("act", 6.0),
            "move" => Components::of("advance", 5.0),
            "load" => Components::of("carry", 3.0),
            "commit" => Components::of("take_ground", 8.0),
            "place" => Components::of("deploy", 2.0),
            "retreat" | "retreat_to" => Components::of("withdraw", 2.0),

            "produce" => self.score_produce(option),
            "pay" => Self::score_pay(option),
            "spend" => Self::score_spend(choice, option),
            "offer" => Self::score_offer(option),
            "casualty" | "ground_casualty" => self.score_casualty(option),
            "sustain" => Components::of("absorb", 6.0),
            "research" => Components::of("technology", 6.0),
            "strategy" | "strategy_card" => self.score_strategy(option),
            "vote" | "vote_planet" => Components::of("influence", 1.0),
            "reaction" | "ability" | "component" => Components::of("use", 4.0),
            "discard" | "return" | "remove" => Components::of("give_up", -1.0),
            "pool" => Self::score_pool(choice, option),

            _ => Components::new(),
        }
    }

    /// Score an option from the same dispatcher as [`Self::raw_score`], adding public-board
    /// facts only for the tactical choices that need them.  Keeping the blind dispatcher as the
    /// base means a window that calls `choose` instead of `choose_seeing` has exactly the same
    /// kind coverage; it merely lacks position-sensitive components.
    #[must_use]
    fn seen_score(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        match option.kind.as_str() {
            "activate" => {
                let target = SystemId::new(&option.id);
                Components::of("act", 6.0).and(
                    "system_value",
                    crate::valuation::system_value(seen, &choice.player, &target),
                )
            }
            "move" => self.score_move_seen(choice, option, seen),
            "load" => self.score_load_seen(choice, option, seen),
            "commit" => self.score_commit_seen(choice, option, seen),
            "produce" => self.score_produce_seen(choice, option, seen),
            "pay" => Self::score_pay_seen(choice, option, seen),
            "research" => self.score_research_seen(choice, option, seen),
            "pool" => Self::score_pool_seen(choice, option, seen),
            _ => self.raw_score(choice, option),
        }
    }

    /// Public technology cards expose both their category and every player's existing face-up
    /// technologies.  Before objective planning exists, prefer a missing colour path or the
    /// next unit upgrade rather than letting legal research choices tie at random.
    #[must_use]
    fn score_research_seen(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        let Some(card) = self
            .content
            .get(ContentType::Technologies, &option.id)
            .filter(|card| card.in_sources(self.sources))
        else {
            return self.raw_score(choice, option);
        };
        let Some(seat) = seen.seat(&choice.player) else {
            return self.raw_score(choice, option);
        };
        let types = card.strings("types");
        if types.contains(&"UNITUPGRADE") {
            let held = f64::from(
                i32::try_from(
                    seat.technologies
                        .iter()
                        .filter(|technology| {
                            self.content
                                .get(ContentType::Technologies, technology.as_str())
                                .filter(|known| known.in_sources(self.sources))
                                .is_some_and(|known| {
                                    known.strings("types").contains(&"UNITUPGRADE")
                                })
                        })
                        .count(),
                )
                .expect("technology corpus count fits in i32"),
            );
            let score = Components::of("technology", 6.0)
                .and("unit_upgrade", 2.0)
                .and("upgrade_gap", 3.0 / (1.0 + held));
            return if Self::has_public_goal(seen, choice, &["develop", "revolutionize"])
                && held < 3.0
            {
                score.and("objective_upgrade", 6.0)
            } else {
                score
            };
        }
        let Some(colour) = types
            .into_iter()
            .find(|kind| matches!(*kind, "PROPULSION" | "BIOTIC" | "CYBERNETIC" | "WARFARE"))
        else {
            return Components::of("technology", 6.0);
        };
        let held = f64::from(
            i32::try_from(
                seat.technologies
                    .iter()
                    .filter(|technology| {
                        self.content
                            .get(ContentType::Technologies, technology.as_str())
                            .filter(|known| known.in_sources(self.sources))
                            .is_some_and(|known| known.strings("types").contains(&colour))
                    })
                    .count(),
            )
            .expect("technology corpus count fits in i32"),
        );
        let score = Components::of("technology", 6.0)
            .and("colour_path", 2.0)
            .and("colour_gap", 3.0 / (1.0 + held));
        if Self::has_public_goal(seen, choice, &["diversify", "master_science"]) {
            score.and(
                "objective_colour",
                if (held - 1.0).abs() < f64::EPSILON {
                    6.0
                } else {
                    2.0
                },
            )
        } else {
            score
        }
    }

    fn has_public_goal(seen: &Observed<'_>, choice: &Choice, aliases: &[&str]) -> bool {
        let scored = seen.scored_by(&choice.player);
        seen.revealed_objectives()
            .iter()
            .any(|goal| aliases.contains(&goal.as_str()) && !scored.contains(goal))
    }

    /// The nearest public single-kind purchase objective worth protecting this round.
    ///
    /// The oracle's full planner selects one active path and only protects a goal that is already
    /// plausible. This compact policy slice has no schedule, so it takes the smallest revealed
    /// unscored threshold that is at least half funded. It never consults secret objectives.
    fn public_purchase_reserve(seen: &Observed<'_>, choice: &Choice, kind: Spend) -> Option<i64> {
        let goals: &[(&str, i64)] = match kind {
            Spend::Resources => &[("monument", 8), ("golden_age", 16)],
            Spend::Influence => &[("sway_council", 8), ("manipulate_law", 16)],
        };
        let available = seen.available_spend(&choice.player, kind);
        let scored = seen.scored_by(&choice.player);
        goals
            .iter()
            .filter(|(alias, need)| {
                available >= (need + 1) / 2
                    && seen
                        .revealed_objectives()
                        .iter()
                        .any(|goal| goal.as_str() == *alias)
                    && !scored.contains(&ti4_model::id::ObjectiveId::new(*alias))
            })
            .map(|(_, need)| *need)
            .min()
    }

    /// The nearest public trade-good objective that is already plausible this round.
    fn public_trade_good_reserve(seen: &Observed<'_>, choice: &Choice) -> Option<i64> {
        let available = i64::from(seen.seat(&choice.player)?.trade_goods);
        let scored = seen.scored_by(&choice.player);
        [("trade_routes", 5), ("centralize_trade", 10)]
            .into_iter()
            .filter(|(alias, need)| {
                available >= (need + 1) / 2
                    && seen
                        .revealed_objectives()
                        .iter()
                        .any(|goal| goal.as_str() == *alias)
                    && !scored.contains(&ti4_model::id::ObjectiveId::new(*alias))
            })
            .map(|(_, need)| need)
            .min()
    }

    /// The public part of the oracle's `_score_move`: a hull is valuable when it has a job at
    /// the active system, and an idle reinforcement should lose to finishing movement.
    #[must_use]
    fn score_move_seen(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        let Some((origin, index)) = move_origin_and_index(&option.id) else {
            return self.raw_score(choice, option);
        };
        let Some(target) = seen.active_system() else {
            return self.raw_score(choice, option);
        };
        let origin = SystemId::new(origin);
        let types = ti4_content::units::catalogue(seen.content(), seen.sources());
        let source = seen.system(&origin);
        let Some(unit) = source
            .units
            .iter()
            .filter(|unit| {
                unit.owner == choice.player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
            })
            .nth(index)
        else {
            return self.raw_score(choice, option);
        };
        let Some(stats) = types.get(unit.type_id.as_str()) else {
            return self.raw_score(choice, option);
        };

        let destination = seen.system(target);
        let enemy_waiting = destination.units.iter().any(|other| {
            other.owner != choice.player
                && types
                    .get(other.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
        });
        let ours_waiting = destination.units.iter().any(|other| {
            other.owner == choice.player
                && types
                    .get(other.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
        });
        let riders = ground_riders(&source, &choice.player, &types);
        let carries_riders = stats.capacity() > 0 && riders > 0;
        let useful = enemy_waiting || !ours_waiting || carries_riders;

        let mut score = Components::of(
            "hull",
            crate::valuation::unit_value(seen.content(), seen.sources(), unit.type_id.as_str())
                * if useful { 1.0 } else { 0.2 },
        )
        .and(
            "destination_value",
            0.25 * crate::valuation::system_value(seen, &choice.player, target),
        );
        if carries_riders {
            let capacity = usize::try_from(stats.capacity()).unwrap_or(usize::MAX);
            let carried = i32::try_from(capacity.min(riders)).unwrap_or(i32::MAX);
            score = score.and("lift", 2.0 * f64::from(carried));
        }
        score
    }

    /// Cargo is useful when it fills a real transport role, especially before an activation with
    /// a public prize.  The option label carries the unit type because cargo ids are deliberately
    /// only stable indices into the window's private candidate list.
    #[must_use]
    fn score_load_seen(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        let unit_id = option
            .label
            .strip_prefix("load ")
            .and_then(|label| label.split_once(" from ").map(|(unit, _)| unit));
        let Some(unit_id) = unit_id else {
            return self.raw_score(choice, option);
        };
        // A point lookup, not a catalogue: building the whole map to answer one question was a
        // third of a game's running time across every site that did it.
        let Some(unit) = ti4_content::units::unit_type(seen.content(), unit_id, seen.sources())
        else {
            return self.raw_score(choice, option);
        };
        let mut score = if unit.is_ground_force() {
            Components::of("transport", 6.0)
        } else if unit.is_fighter() {
            Components::of("screen", 2.0)
        } else {
            return self.raw_score(choice, option);
        };
        if let Some(target) = seen.active_system() {
            score = score.and(
                "destination_value",
                0.5 * crate::valuation::system_value(seen, &choice.player, target),
            );
        }
        score
    }

    /// The public part of the oracle's `_score_commit`: a ground force lands to take a planet,
    /// not to make an already superior friendly garrison larger.
    #[must_use]
    fn score_commit_seen(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        let Some((index, planet)) = commit_index_and_planet(&option.id) else {
            return self.raw_score(choice, option);
        };
        let Some(target) = seen.active_system() else {
            return self.raw_score(choice, option);
        };
        let types = ti4_content::units::catalogue(seen.content(), seen.sources());
        let system = seen.system(target);
        let offered_troop = system
            .units
            .iter()
            .filter(|unit| {
                unit.owner == choice.player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ground_force)
            })
            .nth(index);
        if offered_troop.is_none() {
            return self.raw_score(choice, option);
        }
        let planet = ti4_model::id::PlanetId::new(planet);
        let (mine, defenders) = system.planet_units.get(&planet).map_or((0, 0), |units| {
            units.iter().fold((0, 0), |(mine, defenders), unit| {
                if !types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ground_force)
                {
                    return (mine, defenders);
                }
                if unit.owner == choice.player {
                    (mine + 1, defenders)
                } else {
                    (mine, defenders + 1)
                }
            })
        });
        if mine > defenders {
            return Components::of("already_held", 0.2);
        }
        Components::of("invade", 12.0).and(
            "planet_value",
            crate::valuation::planet_value(seen, &choice.player, &planet),
        )
    }

    /// The public-board part of the oracle's production scorer.  A transport is worth more when
    /// ground forces are visibly stranded on planets, because a fleet that cannot lift troops
    /// cannot turn its production into captured planets.
    #[must_use]
    fn score_produce_seen(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        let mut score = self.score_produce(option);
        let unit_id = option.id.strip_prefix("produce|").unwrap_or(&option.id);
        let Some(unit) = ti4_content::units::unit_type(seen.content(), unit_id, seen.sources())
        else {
            return score;
        };
        let stranded = crate::valuation::stranded_troops(seen, &choice.player);
        if unit.capacity() > 0 && stranded > 2 {
            let capacity = i32::try_from(unit.capacity()).unwrap_or(i32::MAX);
            score = score.and("lift_shortage", 5.0 * (f64::from(capacity) / 4.0).min(1.0));
        }
        score
    }

    /// Acting beats passing, and a strategic action beats an ordinary one because a strategy card
    /// left unexhausted is a card wasted for the round.
    fn score_action(&self, option: &ChoiceOption) -> Components {
        let _ = self;
        let id = option.id.as_str();
        if id.contains("pass") {
            // Passing ends this player's round. Positive, because passing with nothing worth
            // doing is correct, and below every real action, because it usually is not.
            return Components::of("pass", 0.5);
        }
        if id.contains("strategic") {
            return Components::of("act", 4.0).and("strategy_card", 6.0);
        }
        if id.contains("tactical") {
            return Components::of("act", 4.0).and("tempo", 4.0);
        }
        Components::of("act", 4.0)
    }

    /// Value per resource, so a cheap useful hull beats an expensive one the bot cannot afford to
    /// follow up.
    fn score_produce(&self, option: &ChoiceOption) -> Components {
        let unit = option.id.strip_prefix("produce|").unwrap_or(&option.id);
        let worth = crate::valuation::unit_value(self.content, self.sources, unit);
        let cost = option
            .payload
            .get("cost")
            .and_then(serde_json::Value::as_f64)
            .filter(|cost| *cost > 0.0)
            .unwrap_or(1.0);
        let made = option
            .payload
            .get("units")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(1.0);
        Components::of("build", 2.0).and("value_per_resource", 2.0 * worth * made / cost)
    }

    /// Settle the bill in one exhaustion where an offered planet can.
    ///
    /// The oracle's rule, and it is a policy preference rather than a legality one: paying with a
    /// smaller planet when a larger offered one settles the remainder forces an avoidable second
    /// exhaustion. Human deciders still see every option.
    fn score_pay(option: &ChoiceOption) -> Components {
        if option.id == "trade_good" {
            // Trade goods are flexible future resources. A planet that can settle the same bill
            // should be exhausted first, matching the oracle's explicit preservation penalty.
            return Components::of("spare_trade_good", -3.0);
        }
        let worth = option
            .payload
            .get("worth")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let owed = option
            .payload
            .get("owed")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let mut score = Components::of("pay", 1.0);
        if worth >= owed && owed > 0.0 {
            score = score.and("settles", 4.0);
        }
        // Among options that settle, the tightest fit wastes the least.
        score.and("overpayment", -0.5 * (worth - owed).max(0.0))
    }

    /// Preserve the next plausible public purchase-objective balance when payment metadata tells
    /// us whether this legal option spends resources or influence.
    fn score_pay_seen(choice: &Choice, option: &ChoiceOption, seen: &Observed<'_>) -> Components {
        let mut score = Self::score_pay(option);
        if option.id == "trade_good"
            && let Some(reserve) = Self::public_trade_good_reserve(seen, choice)
        {
            let held = seen.seat(&choice.player).map_or(0, |seat| seat.trade_goods);
            let shortfall = (reserve - (i64::from(held) - 1)).max(0);
            if shortfall > 0 {
                let capped_shortfall = i32::try_from(shortfall).unwrap_or(i32::MAX);
                score = score.and("trade_good_reserve", -2.0 * f64::from(capped_shortfall));
            }
        }
        let kind = match option
            .payload
            .get("kind")
            .and_then(serde_json::Value::as_str)
        {
            Some("resources") => Spend::Resources,
            Some("influence") => Spend::Influence,
            _ => return score,
        };
        let Some(reserve) = Self::public_purchase_reserve(seen, choice, kind) else {
            return score;
        };
        let worth = option
            .payload
            .get("worth")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let remaining = seen.available_spend(&choice.player, kind) - worth;
        let shortfall = (reserve - remaining).max(0);
        if shortfall == 0 {
            score
        } else {
            let capped_shortfall = i32::try_from(shortfall).unwrap_or(i32::MAX);
            score.and("objective_reserve", -2.0 * f64::from(capped_shortfall))
        }
    }

    /// An affordable Leadership purchase turns visible influence into an extra command token.
    /// The specific pool allocation remains the later `pool` choice; this is only the decision
    /// whether to buy at the offered three-influence rate.
    fn score_spend(choice: &Choice, option: &ChoiceOption) -> Components {
        if option.id == "buy" && choice.prompt.contains("command token") {
            return Components::of("command_token", 6.0).and("influence", -3.0);
        }
        Components::of("spend", 1.0)
    }

    /// Trade offers carry their complete public terms in a stable option id.  Score the net
    /// immediately usable value and recognise the one mutual-support exchange that gives each
    /// player a point. Accept/counter choices deliberately have no terms, so stay unscored.
    fn score_offer(option: &ChoiceOption) -> Components {
        if option.id == "ss" {
            return Components::of("support_exchange", 20.0);
        }
        if let Some(count) = option.id.strip_prefix("cc").and_then(parse_count) {
            return Components::of("commodity_conversion", f64::from(count));
        }
        if let Some((give, receive)) = option.id.strip_prefix("ct").and_then(parse_trade_pair) {
            return Components::of("trade_balance", f64::from(receive - give));
        }
        if let Some((give, receive)) = option.id.strip_prefix("tc").and_then(parse_trade_pair) {
            return Components::of("trade_balance", f64::from(receive - give));
        }
        if let Some((give, receive)) = parse_trade_pair(&option.id) {
            return Components::of("trade_balance", f64::from(receive - give));
        }
        if let Some((gift, _)) = option.id.strip_prefix('c').and_then(parse_trade_pair) {
            return Components::of("gift", -f64::from(gift));
        }
        // Promissory notes. The engine already prices both sides of the deal into the option's
        // payload -- `net` to us, `their_net` to them -- so a policy does not have to guess what a
        // note is worth, and the flat `unknown_trade` zero that used to land here made every note
        // deal identical to declining and to each other. Support kept its own strong score above,
        // which is why Support was the only note ever traded.
        //
        // `their_net` is included because a proposal only pays if it is accepted: an offer the
        // partner would refuse is worth nothing however good it looks from this chair. It is
        // clamped at zero so a generous deal is not scored as if the partner's gain were ours.
        if let Some(note) = option
            .payload
            .get("alias")
            .and_then(serde_json::Value::as_str)
        {
            let net = option
                .payload
                .get("net")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let theirs = option
                .payload
                .get("their_net")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            // Named per note, so the learner gets a stable feature per card rather than one
            // bucket holding every promissory note in the game. The names are `&'static str`
            // because a feature name is interned; an unknown alias falls into `note:other`
            // rather than being dropped, which keeps a new note visible instead of silent.
            return Components::of(note_feature(note), net).and("note_acceptable", theirs.min(0.0));
        }
        Components::of("unknown_trade", 0.0)
    }

    /// Every available objective is already legal and in this player's scoring window. Its
    /// identity is therefore safe to inspect even for a secret: the choice itself is private to
    /// its owner. Prefer the larger printed point award without reading unoffered secrets or
    /// inventing a schedule from future hidden cards.
    fn score_objective(&self, option: &ChoiceOption) -> Components {
        let alias = option.id.strip_prefix("score|").unwrap_or(&option.id);
        let points = [ContentType::PublicObjectives, ContentType::SecretObjectives]
            .into_iter()
            .find_map(|category| {
                self.content
                    .get(category, alias)
                    .filter(|record| record.in_sources(self.sources))
                    .and_then(|record| record.int("points"))
                    .and_then(|points| i32::try_from(points).ok())
            })
            .unwrap_or(1);
        Components::of("victory", 100.0 * f64::from(points))
    }

    /// Lose the cheapest thing. The one combat judgement available without the board: the label
    /// names the unit, and what a unit is worth needs the corpus, not the position.
    fn score_casualty(&self, option: &ChoiceOption) -> Components {
        let named = option
            .label
            .strip_prefix("destroy ")
            .unwrap_or(&option.label)
            .replace(" (damaged)", "");
        let worth = crate::valuation::unit_value(self.content, self.sources, named.trim());
        // A unit already damaged has spent its sustain, so losing it costs less than losing a
        // fresh one of the same type.
        let damaged = if option.label.contains("(damaged)") {
            0.5
        } else {
            1.0
        };
        Components::of("loss", -worth * damaged)
    }

    /// Strategy cards have printed, public economic roles.  The oracle's card preference is a
    /// better default than treating initiative as the entire choice; later objective planning
    /// may add demand-specific components without changing these baseline roles.
    fn score_strategy(&self, option: &ChoiceOption) -> Components {
        let initiative = option
            .payload
            .get("initiative")
            .and_then(serde_json::Value::as_f64);
        let name = self
            .content
            .get(ContentType::StrategyCards, &option.id)
            .filter(|card| card.in_sources(self.sources))
            .and_then(|card| card.text("name"));
        let preference = match name {
            Some("Imperial") => 9.0,
            Some("Technology") => 7.0,
            Some("Leadership") => 6.0,
            Some("Warfare") => 5.0,
            Some("Construction" | "Trade") => 4.0,
            Some("Politics") => 3.0,
            Some("Diplomacy") => 2.0,
            _ => 1.0,
        };
        let mut score = Components::of("card_preference", preference);
        if let Some(initiative) = initiative {
            score = score.and("initiative", -0.3 * initiative);
        }
        score
    }

    /// Command tokens: tactics move, fleet holds, strategy follows.
    fn score_pool(choice: &Choice, option: &ChoiceOption) -> Components {
        let _ = choice;
        match option.id.as_str() {
            id if id.contains("tactic") => Components::of("tokens", 3.0),
            id if id.contains("fleet") => Components::of("tokens", 2.0),
            _ => Components::of("tokens", 1.5),
        }
    }

    /// The value of a command token diminishes with the visible number already in that pool.
    /// Strategy-pool tokens are deliberately close to tactics when empty: they enable every
    /// secondary ability, instead of being starved by a static tactic-first ranking.
    fn score_pool_seen(choice: &Choice, option: &ChoiceOption, seen: &Observed<'_>) -> Components {
        let Some(seat) = seen.seat(&choice.player) else {
            return Self::score_pool(choice, option);
        };
        let (need, held) = match option.id.as_str() {
            "tactic_tokens" => (6.0, seat.tactic_tokens),
            "strategic_tokens" => (5.0, seat.strategic_tokens),
            "fleet_tokens" => (3.0, seat.fleet_tokens),
            _ => return Self::score_pool(choice, option),
        };
        Components::of("pool_need", need / (1.0 + f64::from(held.max(0))))
    }

    /// The options this bot will pick between, which is not always all of them.
    ///
    /// The oracle learned this the hard way: when every option scored zero the tie broke at
    /// random, and two factions sailed for a home system nothing of theirs could reach. Removing
    /// an option from consideration is different from outscoring it — the engine still offers it,
    /// because a human at the table may legitimately want it.
    fn worth_considering<'a>(
        &self,
        choice: &'a Choice,
        scores: &BTreeMap<String, Components>,
    ) -> Vec<&'a ChoiceOption> {
        #[cfg(test)]
        instrument_authored_filter();
        let _ = self;
        let all: Vec<&ChoiceOption> = choice.options.iter().collect();
        if all.len() <= 1 {
            return all;
        }
        // An option that dominates by an order of magnitude is not a preference to sample around.
        // Scoring, in practice: a bot that declines a victory point one time in twenty because the
        // softmax said so is not exploring, it is losing.
        let best = all
            .iter()
            .map(|option| scores.get(&option.id).map_or(0.0, Components::total))
            .fold(f64::NEG_INFINITY, f64::max);
        if best >= 50.0 {
            return all
                .into_iter()
                .filter(|option| {
                    scores.get(&option.id).map_or(0.0, Components::total) >= best - f64::EPSILON
                })
                .collect();
        }
        all
    }

    /// Apply the oracle's activation filter after the ordinary score shortlist.  A system that no
    /// ship can reach and where this player cannot produce remains legal; it is just not a policy
    /// candidate while another activation can actually do something.
    fn worth_considering_seen<'a>(
        &self,
        choice: &'a Choice,
        scores: &BTreeMap<String, Components>,
        seen: &Observed<'_>,
    ) -> Vec<&'a ChoiceOption> {
        let candidates = self.worth_considering(choice, scores);
        let useful: Vec<&ChoiceOption> = candidates
            .iter()
            .copied()
            .filter(|option| option.kind == "activate")
            .filter(|option| Self::activation_can_do_something(seen, &choice.player, &option.id))
            .collect();
        if useful.is_empty() {
            return candidates;
        }
        candidates
            .into_iter()
            .filter(|option| {
                option.kind != "activate"
                    || useful
                        .iter()
                        .any(|useful_option| useful_option.id == option.id)
            })
            .collect()
    }

    fn activation_can_do_something(
        seen: &Observed<'_>,
        player: &ti4_model::id::PlayerId,
        target: &str,
    ) -> bool {
        let Some(galaxy) = seen.galaxy() else {
            return false;
        };
        let target = SystemId::new(target);
        if galaxy.coord_of(target.as_str()).is_none() {
            return false;
        }
        let types = ti4_content::units::catalogue(seen.content(), seen.sources());
        let target_state = seen.system(&target);
        if target_state
            .units
            .iter()
            .chain(target_state.planet_units.values().flatten())
            .any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::has_production)
            })
        {
            return true;
        }

        seen.systems_with_units_of(player)
            .into_iter()
            .any(|origin| {
                let system = seen.system(origin);
                system.units.iter().any(|unit| {
                    let Some(kind) = types.get(unit.type_id.as_str()) else {
                        return false;
                    };
                    let Ok(move_value) = i32::try_from(kind.move_value()) else {
                        return false;
                    };
                    &unit.owner == player
                        && kind.is_ship()
                        && seen.can_reach(player, origin, &target, move_value)
                })
            })
    }

    /// Softmax over the shortlist.
    fn sample<'a>(
        &mut self,
        candidates: &[&'a ChoiceOption],
        scores: &BTreeMap<String, Components>,
    ) -> Option<&'a ChoiceOption> {
        match candidates {
            [] => None,
            [only] => Some(only),
            _ => {
                let totals: Vec<f64> = candidates
                    .iter()
                    .map(|option| scores.get(&option.id).map_or(0.0, Components::total))
                    .collect();
                let best = totals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let temperature = self.temperature.max(1e-6);
                let weights: Vec<f64> = totals
                    .iter()
                    .map(|total| ((total - best) / temperature).exp())
                    .collect();
                let sum: f64 = weights.iter().sum();
                if !sum.is_finite() || sum <= 0.0 {
                    return candidates.first().copied();
                }
                let mut roll = self.rng.random_range(0.0..sum);
                for (option, weight) in candidates.iter().zip(&weights) {
                    roll -= weight;
                    if roll <= 0.0 {
                        return Some(option);
                    }
                }
                candidates.last().copied()
            }
        }
    }

    fn choose_from_scores(
        &mut self,
        choice: &Choice,
        scores: BTreeMap<String, Components>,
        candidates: &[&ChoiceOption],
    ) -> Result<ChoiceOption, IllegalChoice> {
        let considered: Vec<String> = candidates
            .iter()
            .map(|option| option.id.clone())
            .collect::<Vec<String>>();
        let chosen =
            self.sample(candidates, &scores)
                .cloned()
                .ok_or_else(|| IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })?;

        if self.remember {
            self.decisions.push(Decision {
                player: choice.player.to_string(),
                prompt: choice.prompt.clone(),
                chosen: chosen.id.clone(),
                breakdown: scores,
                considered,
            });
        }
        Ok(chosen)
    }
}

fn move_origin_and_index(id: &str) -> Option<(&str, usize)> {
    let mut parts = id.split('|');
    (parts.next()? == "move").then_some(())?;
    let origin = parts.next()?;
    let index = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((origin, index))
}

fn commit_index_and_planet(id: &str) -> Option<(usize, &str)> {
    let mut parts = id.split('|');
    (parts.next()? == "commit").then_some(())?;
    let index = parts.next()?.parse().ok()?;
    let planet = parts.next()?;
    parts.next().is_none().then_some((index, planet))
}

fn parse_count(text: &str) -> Option<i32> {
    text.parse().ok().filter(|count: &i32| *count >= 0)
}

fn parse_trade_pair(text: &str) -> Option<(i32, i32)> {
    let (give, receive) = text.split_once(':')?;
    Some((parse_count(give)?, parse_count(receive)?))
}

fn ground_riders(
    system: &ti4_model::state::SystemState,
    player: &ti4_model::id::PlayerId,
    types: &std::collections::BTreeMap<&str, ti4_content::units::UnitType<'_>>,
) -> usize {
    system
        .planet_units
        .values()
        .flatten()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ground_force)
        })
        .count()
}

impl Decider for ScoredBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        let scores: BTreeMap<String, Components> = choice
            .options
            .iter()
            .map(|option| (option.id.clone(), self.raw_score(choice, option)))
            .collect();
        let candidates = self.worth_considering(choice, &scores);
        self.choose_from_scores(choice, scores, &candidates)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // `seen` derefs to the public position for the scorers below; held-secret progress is
        // not read here — ScoredBot scores publics only (M09-021's objective facts live in the
        // learned path).
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        let scores: BTreeMap<String, Components> = choice
            .options
            .iter()
            .map(|option| (option.id.clone(), self.seen_score(choice, option, seen)))
            .collect();
        let candidates = self.worth_considering_seen(choice, &scores, seen);
        self.choose_from_scores(choice, scores, &candidates)
    }
}

/// Choice kinds the engine raises that no scorer here judges.
///
/// A ledger, in the shape the rest of this project uses: a kind nobody scores is answered by an
/// unweighted sample, which is a decision made by nobody. Counting them is how the list shrinks.
#[must_use]
pub fn unscored_kinds() -> Vec<&'static str> {
    vec![
        // Acceptance/counter choices omit the proposed terms. Scoring one would require a
        // negotiation model rather than inventing a preference from an opaque prompt.
        "transaction",
        "open_transaction",
        "answer",
        // Explore and colour are cosmetic or forced in practice.
        "explore",
        "colour",
        // A tiebreak is by definition between options this bot could not separate.
        "tiebreak",
        "ready",
        "ready_technology",
        "order",
        "planet",
        "leader",
        "action_card",
        // The Heart of Ixth bends one die by 1: which die, and the sign, is a taste this
        // bot has no model of, so the sample stands in for the holder's judgment.
        "heart_ixth",
    ]
}

#[cfg(test)]
mod tests {

    /// A non-Support promissory note is scored by its own identity, not the zero fallback.
    ///
    /// The behavioural half of `plans/BUG_2026-08-29_PROMISSORY_NOTE_TRANSACTION_OFFERS.md`. The
    /// option being *present* was never the problem -- the engine has enumerated note sales for a
    /// while. It scored zero, which made it indistinguishable from declining and from every other
    /// note, so Support was the only note ever traded. This asserts selection, not enumeration.
    #[test]
    fn a_promissory_note_offer_outscores_an_equal_gift() {
        let mut note = ChoiceOption::labelled(
            "pnresearch_agreement:4".to_owned(),
            "offer",
            "sell a Research Agreement for 4 trade goods".to_owned(),
        );
        note.payload
            .insert("alias".to_owned(), serde_json::Value::from("ra"));
        note.payload
            .insert("net".to_owned(), serde_json::Value::from(3.0));
        note.payload
            .insert("their_net".to_owned(), serde_json::Value::from(1.0));

        let scored = ScoredBot::score_offer(&note);
        assert!(
            scored.total() > 0.0,
            "a note deal that gains three trade goods of value is worth taking: {scored:?}"
        );
        assert!(
            scored.parts().iter().any(|(name, _)| *name == "note:ra"),
            "and it is named by the note, not by a shared bucket: {scored:?}"
        );

        // A different note gets a different bucket, which is the half that stops them collapsing.
        let mut other = note.clone();
        other
            .payload
            .insert("alias".to_owned(), serde_json::Value::from("cf"));
        assert!(
            ScoredBot::score_offer(&other)
                .parts()
                .iter()
                .any(|(name, _)| *name == "note:cf"),
            "each note has its own feature"
        );

        // An offer the partner would refuse is discounted: a proposal only pays if accepted.
        let mut refused = note.clone();
        refused
            .payload
            .insert("their_net".to_owned(), serde_json::Value::from(-5.0));
        assert!(
            ScoredBot::score_offer(&refused).total() < scored.total(),
            "a deal the partner loses on is worth less than one they would take"
        );
    }
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::choice::{Observed, ask_private};
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    fn choice(kind: &str, ids: &[&str]) -> Choice {
        Choice::new(
            PlayerId::new("a"),
            "pick one",
            ids.iter()
                .map(|id| ChoiceOption::new(*id, kind))
                .collect::<Vec<ChoiceOption>>(),
        )
    }

    fn score_of(bot: &ScoredBot, choice: &Choice, index: usize) -> f64 {
        bot.raw_score(choice, &choice.options[index]).total()
    }

    fn public_target() -> String {
        ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| !system.is_anomaly() && !system.planets().is_empty())
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus contains an ordinary system with a planet")
    }

    fn watched_hub() -> (ti4_model::state::GameState, ti4_engine::fixtures::Hub) {
        let target = public_target();
        let hub = ti4_engine::fixtures::hub_with_centre(&target);
        (ti4_engine::fixtures::game(&["a", "b"]), hub)
    }

    fn secure_system(
        state: &mut ti4_model::state::GameState,
        system: &ti4_model::id::SystemId,
        player: &PlayerId,
    ) {
        for planet in
            ti4_content::galaxy::planets_in(ContentStore::embedded(), system.as_str(), POK)
        {
            state
                .system_mut(system)
                .set_control(ti4_model::id::PlanetId::new(planet.id()), player.clone());
        }
    }

    fn first_planet(system: &ti4_model::id::SystemId) -> ti4_model::id::PlanetId {
        ti4_content::galaxy::planets_in(ContentStore::embedded(), system.as_str(), POK)
            .into_iter()
            .next()
            .map(|planet| ti4_model::id::PlanetId::new(planet.id()))
            .expect("the target fixture contains a planet")
    }

    #[test]
    fn seeing_the_board_removes_unreachable_activations_from_the_shortlist() {
        // This is the oracle's `_worth_considering` rule, not legality: both systems remain
        // offered, but only the nearby one has a ship that can accomplish anything there.
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let origin = ti4_model::id::SystemId::new(hub.outer[0].clone());
        let target = ti4_model::id::SystemId::new(hub.centre.clone());
        let unreachable = ti4_model::id::SystemId::new(hub.across(&hub.outer[0]));
        ti4_engine::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        let offered = Choice::new(
            player.clone(),
            "activate a system",
            vec![
                ChoiceOption::new(target.to_string(), "activate"),
                ChoiceOption::new(unreachable.to_string(), "activate"),
            ],
        );
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &offered,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            target.to_string()
        );
        assert_eq!(
            bot.decisions[0].considered,
            vec![target.to_string()],
            "the useless activation is a human option but not a bot candidate"
        );
    }

    #[test]
    fn seeing_no_useful_activation_keeps_every_offered_system() {
        // A player with no movable ships and no dock must still answer the choice; filtering all
        // activations would turn a policy preference into an invented no-option state.
        let (state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let first = ti4_model::id::SystemId::new(hub.centre.clone());
        let second = ti4_model::id::SystemId::new(hub.outer[0].clone());
        let offered = Choice::new(
            player.clone(),
            "activate a system",
            vec![
                ChoiceOption::new(first.to_string(), "activate"),
                ChoiceOption::new(second.to_string(), "activate"),
            ],
        );
        let mut bot = ScoredBot::new(4).remembering();
        ask_private(
            &offered,
            &state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut bot,
        )
        .unwrap();

        assert_eq!(
            bot.decisions[0].considered,
            vec![first.to_string(), second.to_string()]
        );
    }

    #[test]
    fn seeing_the_board_moves_to_a_prize_but_finishes_an_idle_reinforcement() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let target = ti4_model::id::SystemId::new(hub.centre.clone());
        let origin = ti4_model::id::SystemId::new(hub.outer[0].clone());
        state.active_system = Some(target.clone());
        ti4_engine::fixtures::put(&mut state, &origin, "carrier", &player, 1);
        let advancing = Choice::new(
            player.clone(),
            "movement",
            vec![
                ChoiceOption::new(format!("move|{origin}|0"), "move"),
                ChoiceOption::decline(),
            ],
        );
        let mut bot = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            ask_private(
                &advancing,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .kind,
            "move",
            "a carrier establishes a position at an unclaimed prize"
        );

        ti4_engine::fixtures::put(&mut state, &target, "cruiser", &player, 1);
        secure_system(&mut state, &target, &player);
        let mut idle = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            ask_private(
                &advancing,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut idle
            )
            .unwrap()
            .id,
            "decline",
            "an idle carrier joining an already-secured system loses to finishing movement"
        );
    }

    #[test]
    fn seeing_the_board_loads_troops_for_a_prize_and_avoids_surplus_landings() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let target = ti4_model::id::SystemId::new(hub.centre.clone());
        let planet = first_planet(&target);
        state.active_system = Some(target.clone());
        ti4_engine::fixtures::put(&mut state, &target, "infantry", &player, 1);

        let cargo = Choice::new(
            player.clone(),
            "load which unit",
            vec![
                ChoiceOption::labelled("load|0", "load", "load infantry from space"),
                ChoiceOption::decline(),
            ],
        );
        let mut loader = ScoredBot::new(4).at_temperature(0.01).remembering();
        assert_eq!(
            ask_private(
                &cargo,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut loader,
            )
            .unwrap()
            .id,
            "load|0"
        );
        assert!(
            loader.decisions[0]
                .breakdown
                .get("load|0")
                .is_some_and(|score| score.parts().iter().any(|(name, _)| *name == "transport")),
            "the decision explains that the troop is cargo, not a flat action"
        );

        let landing = Choice::new(
            player.clone(),
            format!("commit ground forces in {target}"),
            vec![
                ChoiceOption::new(format!("commit|0|{planet}"), "commit"),
                ChoiceOption::decline(),
            ],
        );
        let mut invader = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            ask_private(
                &landing,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut invader,
            )
            .unwrap()
            .kind,
            "commit",
            "an uncontrolled planet is worth committing the first ground force"
        );

        ti4_engine::fixtures::put_on_planet(&mut state, &target, &planet, "infantry", &player, 2);
        let mut held = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            ask_private(
                &landing,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut held,
            )
            .unwrap()
            .id,
            "decline",
            "a superior friendly garrison does not need another troop"
        );
    }

    #[test]
    fn seeing_stranded_troops_prefers_lift_without_changing_blind_production() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let target = ti4_model::id::SystemId::new(hub.centre.clone());
        let planet = first_planet(&target);
        state.active_system = Some(target.clone());
        ti4_engine::fixtures::put_on_planet(&mut state, &target, &planet, "infantry", &player, 3);
        let production = Choice::new(
            player.clone(),
            "produce",
            vec![
                ChoiceOption::new("produce|carrier", "produce")
                    .with("cost", 4)
                    .with("units", 1),
                ChoiceOption::new("produce|cruiser", "produce")
                    .with("cost", 1)
                    .with("units", 1),
            ],
        );
        let mut blind = ScoredBot::new(4).at_temperature(0.01);
        assert!(
            score_of(&blind, &production, 1) > score_of(&blind, &production, 0),
            "the blind value-per-resource rule prefers the cruiser"
        );
        assert_eq!(blind.choose(&production).unwrap().id, "produce|cruiser");

        let mut seeing = ScoredBot::new(4).at_temperature(0.01).remembering();
        assert_eq!(
            ask_private(
                &production,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut seeing,
            )
            .unwrap()
            .id,
            "produce|carrier",
            "publicly stranded troops make lift the better production"
        );
        assert!(
            seeing.decisions[0]
                .breakdown
                .get("produce|carrier")
                .is_some_and(|score| score
                    .parts()
                    .iter()
                    .any(|(name, _)| *name == "lift_shortage"))
        );
    }

    #[test]
    fn a_victory_point_is_never_declined() {
        // The single most important thing this bot does. A softmax that declines a point one time
        // in twenty is not exploring, it is losing — so scoring is shortlisted alone, not merely
        // weighted highest.
        let mut bot = ScoredBot::new(1);
        let offer = Choice::new(
            PlayerId::new("a"),
            "score an objective",
            vec![
                ChoiceOption::new("score|expand_borders", "score"),
                ChoiceOption::decline(),
            ],
        );

        for _ in 0..50 {
            assert_eq!(bot.choose(&offer).unwrap().id, "score|expand_borders");
        }
    }

    #[test]
    fn a_two_point_objective_beats_a_one_point_objective() {
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();
        let objectives = choice("score", &["expand_borders", "master_science"]);

        assert_eq!(bot.choose(&objectives).unwrap().id, "master_science");
        assert!(
            (bot.decisions[0].breakdown["master_science"].total() - 200.0).abs() < f64::EPSILON,
            "the explanation preserves the printed two-point award"
        );
    }

    #[test]
    fn acting_beats_passing() {
        let bot = ScoredBot::new(1);
        let actions = choice("action", &["tactical", "strategic", "pass"]);
        assert!(score_of(&bot, &actions, 0) > score_of(&bot, &actions, 2));
        assert!(score_of(&bot, &actions, 1) > score_of(&bot, &actions, 2));
    }

    #[test]
    fn passing_is_still_worth_more_than_nothing() {
        // Passing with nothing worth doing is correct play, and a negative score would make the
        // bot prefer any legal absurdity to ending its round.
        let bot = ScoredBot::new(1);
        let actions = choice("action", &["pass"]);
        assert!(score_of(&bot, &actions, 0) > 0.0);
    }

    #[test]
    fn the_cheapest_unit_takes_the_hit() {
        let bot = ScoredBot::new(1);
        let hits = Choice::new(
            PlayerId::new("a"),
            "assign a hit",
            vec![
                ChoiceOption::labelled("destroy|0", "casualty", "destroy dreadnought"),
                ChoiceOption::labelled("destroy|1", "casualty", "destroy fighter"),
                ChoiceOption::labelled("destroy|2", "casualty", "destroy cruiser"),
            ],
        );

        let best = hits
            .options
            .iter()
            .max_by(|a, b| {
                bot.raw_score(&hits, a)
                    .total()
                    .partial_cmp(&bot.raw_score(&hits, b).total())
                    .unwrap()
            })
            .unwrap();
        assert_eq!(best.id, "destroy|1", "the fighter is the cheapest loss");
    }

    #[test]
    fn a_damaged_hull_is_a_cheaper_loss_than_a_fresh_one() {
        // It has already spent its sustain, so it is worth less than the same type undamaged.
        let bot = ScoredBot::new(1);
        let hits = Choice::new(
            PlayerId::new("a"),
            "assign a hit",
            vec![
                ChoiceOption::labelled("destroy|0", "casualty", "destroy dreadnought"),
                ChoiceOption::labelled("destroy|1", "casualty", "destroy dreadnought (damaged)"),
            ],
        );
        assert!(score_of(&bot, &hits, 1) > score_of(&bot, &hits, 0));
    }

    #[test]
    fn a_bill_is_settled_in_one_exhaustion_where_it_can_be() {
        let bot = ScoredBot::new(1);
        let bills = Choice::new(
            PlayerId::new("a"),
            "pay 3 resources",
            vec![
                ChoiceOption::new("pay|small", "pay")
                    .with("worth", 1)
                    .with("owed", 3),
                ChoiceOption::new("pay|exact", "pay")
                    .with("worth", 3)
                    .with("owed", 3),
                ChoiceOption::new("pay|huge", "pay")
                    .with("worth", 6)
                    .with("owed", 3),
            ],
        );

        assert!(
            score_of(&bot, &bills, 1) > score_of(&bot, &bills, 0),
            "settling beats a part payment"
        );
        assert!(
            score_of(&bot, &bills, 1) > score_of(&bot, &bills, 2),
            "and the tightest fit wastes the least"
        );
    }

    #[test]
    fn paying_with_a_trade_good_loses_to_an_exact_planet() {
        let bot = ScoredBot::new(1);
        let bills = Choice::new(
            PlayerId::new("a"),
            "pay 2 resources",
            vec![
                ChoiceOption::new("trade_good", "pay").with("worth", 1),
                ChoiceOption::new("pay|exact", "pay")
                    .with("worth", 2)
                    .with("owed", 2),
            ],
        );

        assert!(score_of(&bot, &bills, 1) > score_of(&bot, &bills, 0));
    }

    #[test]
    fn public_purchase_objective_preserves_the_smallest_resource_payment() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let (system, planet) = ti4_engine::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet, player.clone());
        state.player_mut(&player).unwrap().trade_goods = 3;
        state
            .revealed_objectives
            .push(ti4_model::id::ObjectiveId::new("monument"));
        let bills = Choice::new(
            player,
            "pay 5",
            vec![
                ChoiceOption::new("large", "pay")
                    .with("worth", 5)
                    .with("owed", 5)
                    .with("kind", "resources"),
                ChoiceOption::new("small", "pay")
                    .with("worth", 1)
                    .with("owed", 5)
                    .with("kind", "resources"),
            ],
        );
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &bills,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            "small",
            "the public Monument reserve outvalues settling an unrelated bill in one card"
        );
        assert!(
            bot.decisions[0].breakdown["small"]
                .parts()
                .iter()
                .any(|(name, _)| *name == "objective_reserve")
        );
    }

    #[test]
    fn unseen_purchase_objective_keeps_the_existing_settlement_preference() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let (system, planet) = ti4_engine::fixtures::a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet, player.clone());
        state.player_mut(&player).unwrap().trade_goods = 3;
        let bills = Choice::new(
            player,
            "pay 5",
            vec![
                ChoiceOption::new("large", "pay")
                    .with("worth", 5)
                    .with("owed", 5)
                    .with("kind", "resources"),
                ChoiceOption::new("small", "pay")
                    .with("worth", 1)
                    .with("owed", 5)
                    .with("kind", "resources"),
            ],
        );
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &bills,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            "large"
        );
        assert!(
            bot.decisions[0].breakdown["large"]
                .parts()
                .iter()
                .all(|(name, _)| *name != "objective_reserve")
        );
    }

    #[test]
    fn public_trade_good_objective_preserves_the_final_trade_good() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        state.player_mut(&player).unwrap().trade_goods = 5;
        state
            .revealed_objectives
            .push(ti4_model::id::ObjectiveId::new("trade_routes"));
        let bills = Choice::new(
            player,
            "pay 1",
            vec![
                ChoiceOption::new("trade_good", "pay")
                    .with("worth", 1)
                    .with("owed", 1)
                    .with("kind", "resources"),
                ChoiceOption::new("overpay", "pay")
                    .with("worth", 20)
                    .with("owed", 1)
                    .with("kind", "resources"),
            ],
        );
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &bills,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            "overpay",
            "a revealed Trade Routes threshold protects the final public trade good"
        );
        assert!(
            bot.decisions[0].breakdown["trade_good"]
                .parts()
                .iter()
                .any(|(name, _)| *name == "trade_good_reserve")
        );
    }

    #[test]
    fn affordable_token_spend_beats_declining() {
        let mut bot = ScoredBot::new(4).at_temperature(0.01);
        let purchase = Choice::new(
            PlayerId::new("a"),
            "spend 3 influence for a command token",
            vec![ChoiceOption::new("buy", "spend"), ChoiceOption::decline()],
        );

        assert_eq!(bot.choose(&purchase).unwrap().id, "buy");
    }

    #[test]
    fn trade_terms_prefer_conversion_and_support_over_a_gift() {
        let bot = ScoredBot::new(1);
        let offers = choice("offer", &["c2:0", "cc2", "ss"]);

        assert!(score_of(&bot, &offers, 1) > score_of(&bot, &offers, 0));
        assert!(score_of(&bot, &offers, 2) > score_of(&bot, &offers, 1));
    }

    #[test]
    fn public_research_prefers_a_missing_colour_path() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        state
            .player_mut(&player)
            .expect("fixture has player a")
            .technologies
            .insert(ti4_model::id::TechnologyId::new("amd"));
        let research = choice("research", &["gd", "nm"]);
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &research,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            "nm",
            "a second propulsion card loses to starting the visible biotic path"
        );
        assert!(
            bot.decisions[0].breakdown["nm"]
                .parts()
                .iter()
                .any(|(name, _)| *name == "colour_gap")
        );
    }

    #[test]
    fn public_colour_objective_finishes_an_existing_research_pair() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        state
            .player_mut(&player)
            .expect("fixture has player a")
            .technologies
            .insert(ti4_model::id::TechnologyId::new("amd"));
        state
            .revealed_objectives
            .push(ti4_model::id::ObjectiveId::new("diversify"));
        let research = choice("research", &["gd", "nm"]);
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &research,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            "gd",
            "the public pair objective makes the second propulsion technology the next step"
        );
        assert!(
            bot.decisions[0].breakdown["gd"]
                .parts()
                .iter()
                .any(|(name, _)| *name == "objective_colour")
        );
    }

    #[test]
    fn printed_strategy_roles_outweigh_initiative_alone() {
        let mut bot = ScoredBot::new(4).at_temperature(0.01);
        let cards = choice("strategy_card", &["base2", "pok1leadership"]);

        assert_eq!(
            bot.choose(&cards).unwrap().id,
            "pok1leadership",
            "Leadership's token economy beats Diplomacy's earlier initiative by its printed role"
        );
    }

    #[test]
    fn public_empty_strategy_pool_beats_a_crowded_tactic_pool() {
        let (mut state, hub) = watched_hub();
        let player = PlayerId::new("a");
        let seat = state.player_mut(&player).expect("fixture has player a");
        seat.tactic_tokens = 5;
        seat.strategic_tokens = 0;
        seat.fleet_tokens = 2;
        let pools = choice(
            "pool",
            &["tactic_tokens", "strategic_tokens", "fleet_tokens"],
        );
        let mut bot = ScoredBot::new(4).at_temperature(0.01).remembering();

        assert_eq!(
            ask_private(
                &pools,
                &state,
                ContentStore::embedded(),
                POK,
                Some(&hub.galaxy),
                &mut bot
            )
            .unwrap()
            .id,
            "strategic_tokens"
        );
        assert!(
            bot.decisions[0].breakdown["strategic_tokens"]
                .parts()
                .iter()
                .any(|(name, _)| *name == "pool_need")
        );
    }

    #[test]
    fn a_cheap_hull_beats_an_expensive_one_of_the_same_worth() {
        let bot = ScoredBot::new(1);
        let yard = Choice::new(
            PlayerId::new("a"),
            "produce",
            vec![
                ChoiceOption::labelled("produce|cruiser", "produce", "produce cruiser for 2")
                    .with("cost", 2)
                    .with("units", 1),
                ChoiceOption::labelled(
                    "produce|dreadnought",
                    "produce",
                    "produce dreadnought for 4",
                )
                .with("cost", 4)
                .with("units", 1),
                ChoiceOption::labelled("produce|fighter", "produce", "produce fighter for 1")
                    .with("cost", 1)
                    .with("units", 2),
            ],
        );

        // cruiser 2.0/2 = 1.0, dreadnought 4.0/4 = 1.0, fighters 0.5 x 2 / 1 = 1.0 — all equal on
        // value per resource, which is the point: the rule does not secretly prefer big hulls.
        let cruiser = score_of(&bot, &yard, 0);
        let dread = score_of(&bot, &yard, 1);
        assert!(
            (cruiser - dread).abs() < 0.01,
            "cruiser {cruiser} against dreadnought {dread}"
        );
    }

    #[test]
    fn an_unscored_kind_is_empty_rather_than_a_guess() {
        // The behaviour M08-004 asks for by name. An unknown kind must not panic, and must not
        // invent a preference — it falls through to an unweighted sample, and is counted.
        let bot = ScoredBot::new(1);
        let odd = choice("no_such_kind", &["a", "b"]);
        assert!(bot.raw_score(&odd, &odd.options[0]).is_empty());
    }

    #[test]
    fn every_unscored_kind_really_is_unscored() {
        // A ledger that drifts is worse than none: an entry here for a kind the dispatcher does
        // judge would report a gap that has already been closed.
        let bot = ScoredBot::new(1);
        for kind in unscored_kinds() {
            let raised = choice(kind, &["x"]);
            assert!(
                bot.raw_score(&raised, &raised.options[0]).is_empty(),
                "{kind} is listed as unscored but the dispatcher scores it"
            );
        }
    }

    #[test]
    fn the_same_seed_makes_the_same_choices() {
        let options = choice("move", &["a", "b", "c", "d"]);
        let once: Vec<String> = {
            let mut bot = ScoredBot::new(9);
            (0..20).map(|_| bot.choose(&options).unwrap().id).collect()
        };
        let twice: Vec<String> = {
            let mut bot = ScoredBot::new(9);
            (0..20).map(|_| bot.choose(&options).unwrap().id).collect()
        };
        assert_eq!(once, twice);
    }

    #[test]
    fn a_cold_bot_takes_its_best_option_and_a_hot_one_spreads() {
        let hits = Choice::new(
            PlayerId::new("a"),
            "assign a hit",
            vec![
                ChoiceOption::labelled("destroy|0", "casualty", "destroy dreadnought"),
                ChoiceOption::labelled("destroy|1", "casualty", "destroy fighter"),
            ],
        );

        let mut cold = ScoredBot::new(4).at_temperature(0.01);
        let picks: std::collections::BTreeSet<String> =
            (0..30).map(|_| cold.choose(&hits).unwrap().id).collect();
        assert_eq!(
            picks,
            ["destroy|1".to_owned()].into_iter().collect(),
            "at temperature zero it always loses the fighter"
        );

        let mut hot = ScoredBot::new(4).at_temperature(50.0);
        let spread: std::collections::BTreeSet<String> =
            (0..60).map(|_| hot.choose(&hits).unwrap().id).collect();
        assert_eq!(spread.len(), 2, "a hot bot explores");
    }

    #[test]
    fn a_bot_only_answers_with_an_option_it_was_offered() {
        // The boundary that stops a bot inventing a move. Checked here as well as in the table,
        // because a decider that returns a fabricated id is a bug this layer must not have.
        let mut bot = ScoredBot::new(3);
        let options = choice("move", &["a", "b"]);
        for _ in 0..40 {
            let answer = bot.choose(&options).unwrap();
            assert!(options.ids().contains(&answer.id.as_str()));
        }
    }

    #[test]
    fn an_empty_choice_is_refused_rather_than_answered() {
        let mut bot = ScoredBot::new(1);
        let nothing = Choice::new(PlayerId::new("a"), "pick one", Vec::new());
        assert!(bot.choose(&nothing).is_err());
    }

    #[test]
    fn a_remembering_bot_can_explain_what_it_did() {
        let mut bot = ScoredBot::new(2).remembering();
        let hits = Choice::new(
            PlayerId::new("a"),
            "assign a hit",
            vec![
                ChoiceOption::labelled("destroy|0", "casualty", "destroy dreadnought"),
                ChoiceOption::labelled("destroy|1", "casualty", "destroy fighter"),
            ],
        );
        bot.choose(&hits).unwrap();

        let decision = bot.decisions.first().expect("it was recorded");
        assert_eq!(decision.prompt, "assign a hit");
        assert_eq!(decision.breakdown.len(), 2, "both options were scored");
        assert!(decision.explain().contains("loss="));
    }

    #[test]
    fn a_bot_that_is_not_remembering_keeps_nothing() {
        // A batch of ten thousand games would otherwise hold more log than game.
        let mut bot = ScoredBot::new(2);
        let options = choice("move", &["a", "b"]);
        bot.choose(&options).unwrap();
        assert!(bot.decisions.is_empty());
    }

    // ── M08-018: nested scoring-window revalidation ────────────────────────────────
    //
    // Since occurrence-scoped secret scoring, one event can keep asking the same player:
    // unlimited action/agenda windows re-offer after each score, and combat pauses mid-fight
    // to open a OnePerPlayer window before resuming. These tests prove the bot answers every
    // nested offer legally, with no stale state and no duplicate decisions.

    use ti4_engine::choice::{Resolving, Table, Window};
    use ti4_engine::objectives::{EventScoreLimit, ScoringWindow};
    use ti4_engine::secrets::Timing;
    use ti4_model::state::{Feat, GameState};

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    /// A state where "a" holds the two action-timing secrets that both fire on one occurrence.
    fn two_action_secrets_in_one_occurrence() -> (GameState, ti4_model::state::FeatOccurrence) {
        let a = PlayerId::new("a");
        let mut state = ti4_engine::fixtures::game(&["a"]);
        state.player_mut(&a).unwrap().secret_objectives = vec![
            ti4_model::id::SecretObjectiveId::new("btv"),
            ti4_model::id::SecretObjectiveId::new("dtgs"),
        ];
        let occurrence = state.begin_feat_occurrence();
        state.record_event_feat(&a, Feat::WonInAnAnomaly, occurrence);
        state.record_event_feat(&a, Feat::DestroyedACapitalShip, occurrence);
        (state, occurrence)
    }

    /// Drive a scoring window against the bot until it closes. Returns the offers in order.
    fn drive_scoring_window(
        window: &mut ScoringWindow,
        state: &mut GameState,
        bot: &mut ScoredBot,
    ) -> Vec<Choice> {
        let mut offered = Vec::new();
        while let Some(choice) = window.pending_choice(state, content(), POK) {
            let answer = ask_private(&choice, state, content(), POK, None, bot).unwrap();
            window.resolve(state, content(), POK, answer).unwrap();
            offered.push(choice);
        }
        offered
    }

    #[test]
    fn an_unlimited_action_window_re_offers_the_scorer_until_nothing_is_left() {
        // 61.7: any number of secrets during an action turn. The window keeps the scorer in
        // place after each score, so the bot is asked again with a fresh offer — no cached
        // options from the first ask survive into the second.
        let (mut state, occurrence) = two_action_secrets_in_one_occurrence();
        let a = PlayerId::new("a");
        let mut bot = ScoredBot::new(7).remembering();
        let mut window = ScoringWindow::for_occurrence(
            std::slice::from_ref(&a),
            Timing::Action,
            occurrence,
            EventScoreLimit::AnyPerPlayer,
        );

        let offers = drive_scoring_window(&mut window, &mut state, &mut bot);

        assert_eq!(offers.len(), 2, "each eligible secret gets its own offer");
        for (index, choice) in offers.iter().enumerate() {
            assert_eq!(
                choice.player, a,
                "the unlimited window re-offers the same scorer"
            );
            assert_ne!(
                bot.decisions[index].chosen, "decline",
                "a victory point is never declined"
            );
        }
        let seat = state.player(&a).unwrap();
        assert!(
            seat.secret_objectives.is_empty(),
            "scored secrets leave the hand (61.18)"
        );
    }

    #[test]
    fn a_one_per_player_combat_window_offers_the_scorer_exactly_once() {
        // 61.7: one secret per combat occurrence. The cap — not eligibility — closes the window
        // after the first score, and the bot is never re-offered for that occurrence.
        let (mut state, occurrence) = two_action_secrets_in_one_occurrence();
        let a = PlayerId::new("a");
        // Cold on purpose: the argmax path through the nested window, not just sampling.
        let mut bot = ScoredBot::new(7).remembering().at_temperature(0.0);
        let mut window = ScoringWindow::for_occurrence(
            std::slice::from_ref(&a),
            Timing::Action,
            occurrence,
            EventScoreLimit::OnePerPlayer,
        );

        let offers = drive_scoring_window(&mut window, &mut state, &mut bot);

        assert_eq!(
            offers.len(),
            1,
            "the occurrence cap applies after the first secret"
        );
        assert_ne!(bot.decisions[0].chosen, "decline");
        let seat = state.player(&a).unwrap();
        assert_eq!(
            seat.secret_objectives.len(),
            1,
            "the second secret stays in hand"
        );
        assert!(
            state.scored_at_occurrence(&a, occurrence),
            "the cap is recorded on the state"
        );
    }

    #[test]
    fn an_agenda_window_scores_every_eligible_secret_and_closes() {
        // The agenda phase allows any number of secrets (61.7): one feat-based (dtd) and one
        // requirement-based (dp, three laws passed), both eligible on the same occurrence.
        let a = PlayerId::new("a");
        let mut state = ti4_engine::fixtures::game(&["a"]);
        state.player_mut(&a).unwrap().secret_objectives = vec![
            ti4_model::id::SecretObjectiveId::new("dtd"),
            ti4_model::id::SecretObjectiveId::new("dp"),
        ];
        for law in ["alpha", "beta", "gamma"] {
            state.laws.insert(law.to_string(), a.to_string());
        }
        let occurrence = state.begin_feat_occurrence();
        state.record_event_feat(&a, Feat::ElectedByAnAgenda, occurrence);

        let mut bot = ScoredBot::new(7).remembering();
        let mut window = ScoringWindow::for_occurrence(
            std::slice::from_ref(&a),
            Timing::Agenda,
            occurrence,
            EventScoreLimit::AnyPerPlayer,
        );

        let offers = drive_scoring_window(&mut window, &mut state, &mut bot);

        assert_eq!(
            offers.len(),
            2,
            "both agenda secrets are eligible on the same occurrence"
        );
        for decision in &bot.decisions {
            assert_ne!(decision.chosen, "decline");
        }
        let seat = state.player(&a).unwrap();
        assert!(seat.secret_objectives.is_empty());
    }

    #[test]
    fn a_player_with_no_eligible_secret_is_skipped_not_asked() {
        // A window with nothing to offer skips its player rather than asking a one-answer
        // question; the bot is never consulted and records no decision.
        let a = PlayerId::new("a");
        let mut state = ti4_engine::fixtures::game(&["a"]);
        state.player_mut(&a).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("ans")]; // status timing only
        let occurrence = state.begin_feat_occurrence();
        state.record_event_feat(&a, Feat::WonInAnAnomaly, occurrence);

        let bot = ScoredBot::new(7).remembering();
        let window = ScoringWindow::for_occurrence(
            std::slice::from_ref(&a),
            Timing::Action,
            occurrence,
            EventScoreLimit::AnyPerPlayer,
        );

        assert!(window.pending_choice(&state, content(), POK).is_none());
        assert!(bot.decisions.is_empty());
    }

    /// U1 (M08-018 review): the explanation layer must not name a secret the seat does not own.
    /// `Decision::explain()` prints every option id and component name, so if someone later names
    /// a component after what it scores — `format!("objective:{alias}")` is the tempting shape —
    /// `explain()` starts printing aliases. Today this holds structurally: component names are
    /// static
    /// literals, and scoring choices offer only the seat's own hand plus public objectives (whose
    /// aliases never collide with secret ones). Nothing pinned that; this test converts the argument
    /// into a guard — every token in `explain()` that resolves as a secret objective must be one
    /// seat owns (still holds or has scored).
    #[test]
    fn scoring_explanations_name_no_secret_the_seat_does_not_own() {
        let (mut state, occurrence) = two_action_secrets_in_one_occurrence();
        let a = PlayerId::new("a");
        let mut bot = ScoredBot::new(7).remembering();
        let mut window = ScoringWindow::for_occurrence(
            std::slice::from_ref(&a),
            Timing::Action,
            occurrence,
            EventScoreLimit::AnyPerPlayer,
        );

        drive_scoring_window(&mut window, &mut state, &mut bot);

        // Everything the seat may legitimately see: what it still holds plus what it scored.
        let owned = {
            let seat = state.player(&a).unwrap();
            let mut set: std::collections::BTreeSet<String> = seat
                .secret_objectives
                .iter()
                .map(|alias| alias.as_str().to_string())
                .collect();
            for objective in state.scored_by(&a) {
                if content()
                    .get(ContentType::SecretObjectives, objective.as_str())
                    .is_some()
                {
                    set.insert(objective.to_string());
                }
            }
            set
        };

        assert!(
            !bot.decisions.is_empty(),
            "the guard needs decisions to inspect"
        );
        for decision in &bot.decisions {
            let text = decision.explain();
            for token in text.split(|c: char| !c.is_ascii_alphanumeric()) {
                if content()
                    .get(ContentType::SecretObjectives, token)
                    .is_some()
                {
                    assert!(
                        owned.contains(token),
                        "explain() names the secret {token} the seat does not own:\n{text}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_bot_answers_a_retained_combat_window_after_its_scoring_pause() {
        // M07-022's pausing fixture, on a natural seed: round-1 AFB kills the fighter and fires
        // BarrageTookTheLastFighters on the attacker (seed 51 was probed for exactly this —
        // `Dice::from_faces` is engine-test-only, so the pause must come from the seeded stream).
        // The pause opens a OnePerPlayer scoring window; the bot must answer it mid-fight through
        // the table (validated) and then finish the retained combat legally.
        let players = [PlayerId::new("a"), PlayerId::new("b")];
        let mut state = ti4_engine::setup::start_game(content(), &players, POK, None).unwrap();
        let system = SystemId::new("18");
        ti4_engine::fixtures::put(&mut state, &system, "destroyer", &players[0], 1);
        ti4_engine::fixtures::put(&mut state, &system, "fighter", &players[1], 1);
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &players[1], 1);
        // The attacker holds the secret the barrage feat unlocks.
        state.player_mut(&players[0]).unwrap().secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("fwp")];

        let mut ask_table = Table::new();
        for (index, player) in players.iter().enumerate() {
            ask_table.seat(player.clone(), Box::new(ScoredBot::new(100 + index as u64)));
        }
        // The context keeps its own table (the M07-023 harness shape): one long-lived context
        // borrows it for the whole loop, so window-level asks go through a separate table.
        let mut inner = Table::new();
        let mut dice = ti4_engine::dice::Dice::new();
        let mut rng = ti4_engine::rng::GameRng::new(51);

        let mut window = ti4_engine::combat::CombatWindow::new(&state, content(), POK, &system);
        let mut ctx = Resolving {
            content: content(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut inner,
            timing: None,
        };
        window.settle_open(&mut state, &mut ctx).unwrap();

        let mut paused = false;
        for _ in 0..10_000 {
            if window.outcome().is_some() {
                break;
            }
            if let Some(choice) = window.pending_choice(&state, content(), POK) {
                let answer = ask_table
                    .ask_seeing(&choice, &Observed::new(&state, content(), POK, None))
                    .unwrap();
                window.resolve(&mut state, &mut ctx, answer).unwrap();
            } else if let Some(occurrence) = window.take_scoring_occurrence() {
                paused = true;
                // Exactly what Game does: service the occurrence's scoring window first.
                let mut scoring = ScoringWindow::for_occurrence(
                    &state.initiative_order(),
                    Timing::Action,
                    occurrence,
                    EventScoreLimit::OnePerPlayer,
                );
                while let Some(choice) = scoring.pending_choice(&state, content(), POK) {
                    let answer = ask_table
                        .ask_seeing(&choice, &Observed::new(&state, content(), POK, None))
                        .unwrap();
                    scoring.resolve(&mut state, content(), POK, answer).unwrap();
                }
            } else {
                window.settle_open(&mut state, &mut ctx).unwrap();
            }
        }

        assert!(
            window.outcome().is_some(),
            "the retained combat must finish"
        );
        assert!(paused, "seed 51 must actually pause for scoring");
        // The harness's context table must stay unasked: under POK these seats have no nested
        // ability offers, so any ask there would mean the fixture changed shape (M07-023 Q2).
        assert!(
            inner.log.records.is_empty(),
            "the context table must stay unasked"
        );
        let attacker = state.player(&players[0]).unwrap();
        assert!(
            attacker
                .event_feats
                .iter()
                .any(|(feat, _)| *feat == Feat::BarrageTookTheLastFighters),
            "the barrage feat fired"
        );
        assert!(
            attacker.secret_objectives.is_empty(),
            "the bot scored the paused secret"
        );
        assert!(attacker.victory_points > 0);
    }

    // ── M08-018: full-game legality and determinism across nested windows ───────────

    /// The campaign's seed range, kept clear of other suites' fixtures.
    const CAMPAIGN_SEED_BASE: u64 = 7_777;
    const CAMPAIGN_SEEDS: u64 = 10;
    const CAMPAIGN_ROTATIONS: usize = 3;

    /// Seeds verified to produce at least one mid-window scorer re-offer in real play.
    /// The event is rare (~3% of games), so the base ten-seed set alone does not
    /// guarantee it — without these, the non-vacuity clause below could fail on a tree
    /// where the mechanism works fine. Originally picked under M08-019 (canonical
    /// choice-option ordering); re-verified on 2026-09-02 under the Phase-9 tenth-batch
    /// engine (the Dark Energy Tap fixes shift campaign trajectories, and the original
    /// six seeds no longer trigger the event): a scan of the reserved range 7787-7999
    /// kept the seven seeds below, each verified to re-offer a scorer mid-window in
    /// rotation 0, and 7850 additionally in rotation 2.
    const NESTED_WINDOW_SEEDS: [u64; 7] = [7_793, 7_850, 7_864, 7_893, 7_907, 7_924, 7_992];

    /// What the campaign extracts from one run: the replay record and each seat's own secret
    /// aliases (what it still holds or has scored).
    type CampaignOutcome = (
        Vec<ti4_engine::choice::DecisionRecord>,
        BTreeMap<String, Vec<String>>,
    );

    /// Seat a six-player game on the roster with one scored bot per seat (the sim's accepted
    /// wiring), play it to the horizon, and return [`CampaignOutcome`].
    fn scored_game(seed: u64, rotation: usize) -> Result<CampaignOutcome, String> {
        let content = ContentStore::embedded();
        let players: Vec<PlayerId> = ["p1", "p2", "p3", "p4", "p5", "p6"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        // Rotate the roster assignment so different factions sit at each seat.
        let base = ti4_engine::seating::seat_in_scope(&players);
        let mut faction_list: Vec<ti4_model::id::FactionId> = base.values().cloned().collect();
        let width = faction_list.len();
        faction_list.rotate_left(rotation % width);
        let factions: BTreeMap<PlayerId, ti4_model::id::FactionId> = players
            .iter()
            .zip(faction_list)
            .map(|(player, faction)| (player.clone(), faction))
            .collect();

        let mut state = ti4_engine::setup::start_game_seeded(content, &players, POK, None, seed)
            .map_err(|error| error.to_string())?;
        for (player, faction) in &factions {
            state.player_mut(player).unwrap().faction = faction.clone();
        }
        let filler: Vec<String> = ti4_engine::seating::neutral_systems(content, 30, POK)
            .into_iter()
            .map(|system| system.to_string())
            .collect();
        let borrowed: Vec<&str> = filler.iter().map(String::as_str).collect();
        let galaxy = ti4_engine::seating::build_board(content, &factions, &borrowed, POK)
            .map_err(|error| error.to_string())?;
        for (player, faction) in &factions {
            ti4_engine::seating::deploy(&mut state, content, player, faction, POK)
                .map_err(|error| error.to_string())?;
        }

        let mut table = Table::new();
        for (index, player) in players.iter().enumerate() {
            let offset = u64::try_from(index).unwrap_or(0);
            table.seat(
                player.clone(),
                Box::new(ScoredBot::new(
                    seed.wrapping_mul(1_000_003).wrapping_add(offset),
                )),
            );
        }

        let mut game =
            ti4_engine::game::Game::with_table(state, content, table).with_galaxy(galaxy);
        game.run(5, 2_000_000).map_err(|error| error.to_string())?;
        let records = game.table.log.records.clone();

        // Which secret aliases each seat may have been offered, from the *end* state: everything it
        // still holds or has scored.
        //
        // This is not the whole set. The comment here used to say "a secret never changes hands
        // except by scoring (61.18), so this is exact", and that is false: 45.4 hands an unscored
        // secret back to the deck when a seat goes over its hand limit, so a card can be held,
        // offered, and then returned — leaving no trace in the final hand or the scored list. The
        // caller reads the return records to cover that.
        let allowed: BTreeMap<String, Vec<String>> = game
            .state
            .players
            .iter()
            .map(|seat| {
                let mut secrets: std::collections::BTreeSet<String> = seat
                    .secret_objectives
                    .iter()
                    .map(|alias| alias.as_str().to_string())
                    .collect();
                for objective in game.state.scored_by(&seat.id) {
                    if content
                        .get(ContentType::SecretObjectives, objective.as_str())
                        .is_some()
                    {
                        secrets.insert(objective.to_string());
                    }
                }
                (seat.id.to_string(), secrets.into_iter().collect())
            })
            .collect();
        Ok((records, allowed))
    }

    #[test]
    fn scored_games_stay_legal_and_deterministic_across_nested_windows() {
        // Every seat plays the authored bot through five rounds of real play: action-phase
        // scoring windows re-offer their scorer, combat pauses mid-fight for a OnePerPlayer
        // window, agendas vote and score. The table validates every answer, so an illegal choice
        // fails the run; two runs of one seed must replay identically.
        let content = ContentStore::embedded();
        let seat_names: std::collections::BTreeSet<String> = ["p1", "p2", "p3", "p4", "p5", "p6"]
            .iter()
            .map(ToString::to_string)
            .collect();

        let mut total_offers = 0;
        let mut re_offers = 0;
        let mut seeds: Vec<u64> = (0..CAMPAIGN_SEEDS)
            .map(|offset| CAMPAIGN_SEED_BASE + offset)
            .collect();
        seeds.extend(NESTED_WINDOW_SEEDS.iter().copied());
        for rotation in 0..CAMPAIGN_ROTATIONS {
            for seed in seeds.iter().copied() {
                let (first, allowed) = scored_game(seed, rotation)
                    .unwrap_or_else(|error| panic!("seed {seed} rotation {rotation}: {error}"));
                let (second, _) = scored_game(seed, rotation)
                    .expect("the second run of the same seed must also complete");

                assert_eq!(first.len(), second.len(), "seed {seed} rotation {rotation}");
                assert_eq!(
                    first, second,
                    "seed {seed} rotation {rotation}: identical replay record"
                );

                // Secrets handed back to the deck over the hand limit (45.4). The end-state
                // ledger cannot see these: the seat held the card, was offered it, then returned
                // it, so it appears in neither the final hand nor the scored list.
                let mut returned: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for record in &first {
                    if record.prompt == "return a secret objective to the deck" {
                        returned
                            .entry(record.player.to_string())
                            .or_default()
                            .extend(record.offered.iter().cloned());
                    }
                }
                for record in &first {
                    // Every answer came from a seated bot.
                    assert!(seat_names.contains(&record.player.to_string()));
                    // Secrets are offered only in scoring windows (the window offers the seat's
                    // own hand, nothing else). Alias spaces collide across categories — `sar` is
                    // both a secret and a warfare technology — so this check inspects scoring
                    // records only; within one there is no public/secret alias collision.
                    if record.prompt != "score an objective" {
                        continue;
                    }
                    let allowed = &allowed[&record.player.to_string()];
                    let handed_back = returned
                        .get(&record.player.to_string())
                        .map_or(&[][..], Vec::as_slice);
                    for offered in &record.offered {
                        if offered == "decline" {
                            continue;
                        }
                        let is_secret = content
                            .get(ContentType::SecretObjectives, offered)
                            .is_some();
                        assert!(
                            !is_secret
                                || allowed.contains(offered)
                                || handed_back.contains(offered),
                            "bot {} was offered the secret {offered} it never held",
                            record.player
                        );
                    }
                }

                // Non-vacuity: the campaign must actually exercise the nested structure — a
                // scorer re-offered by an unlimited window is two consecutive score offers to
                // the same seat.
                total_offers += first
                    .iter()
                    .filter(|record| record.prompt == "score an objective")
                    .count();
                for pair in first.iter().zip(first.iter().skip(1)) {
                    let (a, b) = pair;
                    if a.prompt == "score an objective"
                        && b.prompt == "score an objective"
                        && a.player == b.player
                    {
                        re_offers += 1;
                    }
                }
            }
        }

        assert!(
            total_offers > 0,
            "the campaign must actually score objectives"
        );
        assert!(
            re_offers > 0,
            "the campaign must actually re-offer a scorer mid-window"
        );
    }
}
