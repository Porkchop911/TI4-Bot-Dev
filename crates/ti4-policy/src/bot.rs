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
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed};
use ti4_model::content_types::{ContentType, POK, SourceSet};
use ti4_model::id::SystemId;

use crate::scoring::{Components, Decision};

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
        match option.kind.as_str() {
            // Scoring is the only thing that wins a game, so it dominates every other reason.
            "score" => Components::of("victory", 100.0),

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
            "land" => Components::of("take_ground", 8.0),
            "place" => Components::of("deploy", 2.0),
            "retreat" | "retreat_to" => Components::of("withdraw", 2.0),

            "produce" => self.score_produce(option),
            "pay" => Self::score_pay(option),
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
            "land" => self.score_land_seen(choice, option, seen),
            "produce" => self.score_produce_seen(choice, option, seen),
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
            return Components::of("technology", 6.0)
                .and("unit_upgrade", 2.0)
                .and("upgrade_gap", 3.0 / (1.0 + held));
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
        Components::of("technology", 6.0)
            .and("colour_path", 2.0)
            .and("colour_gap", 3.0 / (1.0 + held))
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
        let types = ti4_content::units::catalogue(seen.content(), seen.sources());
        let Some(unit) = types.get(unit_id) else {
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
    fn score_land_seen(
        &self,
        choice: &Choice,
        option: &ChoiceOption,
        seen: &Observed<'_>,
    ) -> Components {
        let Some((index, planet)) = land_index_and_planet(&option.id) else {
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
        let types = ti4_content::units::catalogue(seen.content(), seen.sources());
        let Some(unit) = types.get(unit_id) else {
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

fn land_index_and_planet(id: &str) -> Option<(usize, &str)> {
    let mut parts = id.split('|');
    (parts.next()? == "land").then_some(())?;
    let index = parts.next()?.parse().ok()?;
    let planet = parts.next()?;
    parts.next().is_none().then_some((index, planet))
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
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
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
        // Raised inside a transaction; scoring one needs to know what the other seat is offering
        // and what this seat can spare, which is a negotiation model rather than a component.
        "transaction",
        "offer",
        "open_transaction",
        "answer",
        // Explore and colour are cosmetic or forced in practice.
        "explore",
        "colour",
        // A tiebreak is by definition between options this bot could not separate.
        "tiebreak",
        "spend",
        "ready",
        "ready_technology",
        "order",
        "planet",
        "leader",
        "action_card",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::choice::Observed;
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

    fn watched<'a>(
        state: &'a ti4_model::state::GameState,
        galaxy: &'a ti4_content::galaxy::Galaxy,
    ) -> Observed<'a> {
        Observed::new(state, ContentStore::embedded(), POK, Some(galaxy))
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
            bot.choose_seeing(&offered, &watched(&state, &hub.galaxy))
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
        bot.choose_seeing(&offered, &watched(&state, &hub.galaxy))
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
            bot.choose_seeing(&advancing, &watched(&state, &hub.galaxy))
                .unwrap()
                .kind,
            "move",
            "a carrier establishes a position at an unclaimed prize"
        );

        ti4_engine::fixtures::put(&mut state, &target, "cruiser", &player, 1);
        secure_system(&mut state, &target, &player);
        let mut idle = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            idle.choose_seeing(&advancing, &watched(&state, &hub.galaxy))
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
            loader
                .choose_seeing(&cargo, &watched(&state, &hub.galaxy))
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
            "commit ground forces",
            vec![
                ChoiceOption::new(format!("land|0|{planet}"), "land"),
                ChoiceOption::decline(),
            ],
        );
        let mut invader = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            invader
                .choose_seeing(&landing, &watched(&state, &hub.galaxy))
                .unwrap()
                .kind,
            "land",
            "an uncontrolled planet is worth committing the first ground force"
        );

        ti4_engine::fixtures::put_on_planet(&mut state, &target, &planet, "infantry", &player, 2);
        let mut held = ScoredBot::new(4).at_temperature(0.01);
        assert_eq!(
            held.choose_seeing(&landing, &watched(&state, &hub.galaxy))
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
            seeing
                .choose_seeing(&production, &watched(&state, &hub.galaxy))
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
            bot.choose_seeing(&research, &watched(&state, &hub.galaxy))
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
            bot.choose_seeing(&pools, &watched(&state, &hub.galaxy))
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
}
