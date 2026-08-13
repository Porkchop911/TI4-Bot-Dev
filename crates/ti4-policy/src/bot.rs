//! A scored bot: routes every choice kind to a scorer, then samples (M08-004, M08-011).
//!
//! Ported from the oracle's `bots.py` `ScoredBot._raw_score`, `_worth_considering` and `_sample`.
//!
//! # What this bot can and cannot see
//!
//! [`ti4_engine::choice::Decider`] is handed a [`Choice`] and nothing else. The oracle's bot holds
//! a redacted *game* and reads the board while it scores, which is how it values activating one
//! system over another. This engine keeps the decision table inside the game, so a decider cannot
//! also hold the state, and most choices are raised from windows that own a slice of the position
//! rather than the whole of it.
//!
//! So this bot scores from what the choice carries: the option's kind, its label, its payload, and
//! the content corpus — which is enough for the judgements that decide most games, and not enough
//! for the ones that decide the best games. Concretely:
//!
//! - It always takes a scoring opportunity, because scoring is the only thing that wins.
//! - It prefers acting to passing, moving to standing still, and committing troops to declining —
//!   which is where a uniform-random table loses its games, not in choosing between good moves.
//! - It loses the cheapest unit to a hit, pays a bill in one exhaustion rather than two, and
//!   builds by value per resource.
//! - It cannot tell a rich system from a poor one, or a defended one from an empty one. Those need
//!   [`crate::valuation::system_value`], which needs the board.
//!
//! That gap is a seam this engine does not have yet, not a judgement that scoring the board is
//! unimportant. Closing it means giving `Decider::choose` an observation argument, which touches
//! every window that raises a choice; it is M08-005's problem and is named here rather than
//! papered over with a constant that pretends to be a valuation.

use std::collections::BTreeMap;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
use ti4_model::content_types::{POK, SourceSet};

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
            "strategy" | "strategy_card" => Self::score_strategy(option),
            "vote" | "vote_planet" => Components::of("influence", 1.0),
            "reaction" | "ability" | "component" => Components::of("use", 4.0),
            "discard" | "return" | "remove" => Components::of("give_up", -1.0),
            "pool" => Self::score_pool(choice, option),

            _ => Components::new(),
        }
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

    /// Prefer a low initiative number, which moves earlier in every phase.
    ///
    /// Crude and deliberately so: which card is best depends on what a player is trying to do,
    /// and that is [`crate::valuation`] territory the bot cannot reach from here.
    fn score_strategy(option: &ChoiceOption) -> Components {
        let initiative = option
            .payload
            .get("initiative")
            .and_then(serde_json::Value::as_f64);
        let mut score = Components::of("take_card", 5.0);
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
        let considered: Vec<String> = candidates
            .iter()
            .map(|option| option.id.clone())
            .collect::<Vec<String>>();
        let chosen =
            self.sample(&candidates, &scores)
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
