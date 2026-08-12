//! Game and batch results (M10-001).
//!
//! Ported from the oracle's `engine/sim.py` `GameResult` and `Batch`.
//!
//! Two distinctions in here were named badly once in the oracle and are carried across
//! deliberately, because both made a run look better than it was:
//!
//! - **A winner is not a win.** [`GameResult::winner`] is whoever led when the game stopped, and
//!   a game that runs out of objectives still has a leader. Reporting that as a win made twenty
//!   games look decided when none of them were. [`GameResult::was_won`] is the real question.
//! - **Total points are not a score.** [`GameResult::total_points`] adds every seat together, so
//!   across six players a total of twelve is two each — in a game that ends at ten. Reported as
//!   "mean VP/game" it reads like a winning score.
//!
//! Failures are counted, never hidden: a game that errored is a result with an [`GameResult::error`],
//! and it stays in the batch so completion rate means something.

#![allow(
    clippy::cast_precision_loss,
    reason = "batch sizes and decision counts are far below 2^53, so these casts are exact"
)]

use std::collections::BTreeMap;

/// Why a game stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// Somebody reached the victory target (98.7).
    VictoryPoints,
    /// The objective deck ran out (61.15).
    ObjectivesExhausted,
    /// The horizon was reached with the game still going.
    HorizonReached,
    /// The game stopped because a step refused.
    Error,
}

impl Ending {
    /// The stable name used in reports.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::VictoryPoints => "victory_points",
            Self::ObjectivesExhausted => "objectives_exhausted",
            Self::HorizonReached => "horizon_reached",
            Self::Error => "error",
        }
    }
}

/// One finished game, reduced to what a batch cares about.
#[derive(Debug, Clone, PartialEq)]
pub struct GameResult {
    /// The seed this game was played from. With the same seed and the same engine, replaying it
    /// gives the same result — which is what makes a batch reproducible rather than merely large.
    pub seed: u64,
    /// Whether the game reached a natural end rather than the horizon or an error.
    pub finished: bool,
    /// Whoever led when the game stopped. **Not** necessarily a winner; see [`Self::was_won`].
    pub winner: Option<String>,
    /// Rounds played.
    pub rounds: u32,
    /// Final score per seat.
    pub victory_points: BTreeMap<String, i32>,
    /// How many times each event was emitted. The cheapest way to notice a subsystem that has
    /// gone quiet across a whole batch.
    pub events: BTreeMap<String, usize>,
    /// Decisions the table answered.
    pub decisions: usize,
    /// Wall time for this game.
    pub seconds: f64,
    /// Why it stopped.
    pub ended_because: Ending,
    /// The failure, if it failed. Counted, not hidden.
    pub error: Option<String>,
}

impl GameResult {
    /// Every seat's score added together — not the winner's.
    #[must_use]
    pub fn total_points(&self) -> i32 {
        self.victory_points.values().sum()
    }

    /// The best score in the game.
    #[must_use]
    pub fn top_score(&self) -> i32 {
        self.victory_points.values().copied().max().unwrap_or(0)
    }

    /// Whether anybody actually reached the target (98.7).
    #[must_use]
    pub fn was_won(&self, target: i32) -> bool {
        self.top_score() >= target
    }
}

/// A run of games, and the numbers that describe it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Batch {
    /// Every game, in a deterministic order — including the ones that failed.
    pub results: Vec<GameResult>,
}

impl Batch {
    /// Games that failed.
    #[must_use]
    pub fn errors(&self) -> Vec<&GameResult> {
        self.results
            .iter()
            .filter(|result| result.error.is_some())
            .collect()
    }

    /// Share of games that reached a natural end.
    #[must_use]
    pub fn completion_rate(&self) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }
        let finished = self.results.iter().filter(|r| r.finished).count();
        finished as f64 / self.results.len() as f64
    }

    /// How often each seat led at the end.
    #[must_use]
    pub fn wins(&self) -> BTreeMap<String, usize> {
        let mut counted: BTreeMap<String, usize> = BTreeMap::new();
        for winner in self.results.iter().filter_map(|r| r.winner.as_ref()) {
            *counted.entry(winner.clone()).or_default() += 1;
        }
        counted
    }

    /// Games somebody actually won, rather than merely led when time ran out.
    #[must_use]
    pub fn games_won(&self, target: i32) -> usize {
        self.results
            .iter()
            .filter(|result| result.was_won(target))
            .count()
    }

    /// Mean best score in a game — what a winner would need to be the target.
    #[must_use]
    pub fn mean_top_score(&self) -> f64 {
        self.mean(|result| f64::from(result.top_score()))
    }

    /// Mean of every seat's score added together.
    #[must_use]
    pub fn mean_points(&self) -> f64 {
        self.mean(|result| f64::from(result.total_points()))
    }

    /// Mean wall time per game.
    #[must_use]
    pub fn mean_seconds(&self) -> f64 {
        self.mean(|result| result.seconds)
    }

    /// Mean decisions per game.
    #[must_use]
    pub fn mean_decisions(&self) -> f64 {
        self.mean(|result| result.decisions as f64)
    }

    /// Seconds per decision across the whole batch.
    ///
    /// Totals divided by totals, never a mean of per-game rates: a game with three decisions
    /// would otherwise weigh as heavily as one with three hundred.
    #[must_use]
    pub fn seconds_per_decision(&self) -> f64 {
        let decisions: usize = self.results.iter().map(|result| result.decisions).sum();
        if decisions == 0 {
            return 0.0;
        }
        let seconds: f64 = self.results.iter().map(|result| result.seconds).sum();
        seconds / decisions as f64
    }

    /// How the games ended, counted by reason.
    #[must_use]
    pub fn endings(&self) -> BTreeMap<&'static str, usize> {
        let mut counted: BTreeMap<&'static str, usize> = BTreeMap::new();
        for result in &self.results {
            *counted.entry(result.ended_because.label()).or_default() += 1;
        }
        counted
    }

    /// Every event across the batch, counted.
    #[must_use]
    pub fn events(&self) -> BTreeMap<String, usize> {
        let mut combined: BTreeMap<String, usize> = BTreeMap::new();
        for result in &self.results {
            for (event, count) in &result.events {
                *combined.entry(event.clone()).or_default() += count;
            }
        }
        combined
    }

    /// Which of these events did not occur once, across the whole batch.
    ///
    /// The standing check, as a function: a subsystem that has stopped being reached shows up
    /// here long before anybody notices it in a game.
    #[must_use]
    pub fn never_happened<'a>(&self, expected: &[&'a str]) -> Vec<&'a str> {
        let seen = self.events();
        expected
            .iter()
            .filter(|event| !seen.contains_key(**event))
            .copied()
            .collect()
    }

    /// Gap between the most and least successful seat, as a fraction of games.
    ///
    /// A large spread means the seats are not interchangeable — which may be the map, the
    /// factions, or a bot that plays one of them badly.
    #[must_use]
    pub fn win_rate_spread(&self) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }
        let counts = self.wins();
        let seats: std::collections::BTreeSet<&String> = self
            .results
            .iter()
            .flat_map(|result| result.victory_points.keys())
            .collect();
        let rates: Vec<f64> = seats
            .into_iter()
            .map(|seat| counts.get(seat).copied().unwrap_or(0) as f64 / self.results.len() as f64)
            .collect();
        match (
            rates.iter().copied().fold(f64::MIN, f64::max),
            rates.iter().copied().fold(f64::MAX, f64::min),
        ) {
            (high, low) if !rates.is_empty() => high - low,
            _ => 0.0,
        }
    }

    fn mean(&self, of: impl Fn(&GameResult) -> f64) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }
        self.results.iter().map(of).sum::<f64>() / self.results.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(seed: u64, points: &[(&str, i32)]) -> GameResult {
        GameResult {
            seed,
            finished: true,
            winner: points
                .iter()
                .max_by_key(|(_, score)| *score)
                .map(|(seat, _)| (*seat).to_owned()),
            rounds: 6,
            victory_points: points
                .iter()
                .map(|(seat, score)| ((*seat).to_owned(), *score))
                .collect(),
            events: BTreeMap::new(),
            decisions: 100,
            seconds: 0.001,
            ended_because: Ending::VictoryPoints,
            error: None,
        }
    }

    #[test]
    fn leading_a_game_is_not_winning_it() {
        // The distinction that made twenty games look decided when none of them were.
        let led = result(1, &[("a", 4), ("b", 2)]);

        assert_eq!(led.winner.as_deref(), Some("a"), "a led");
        assert!(!led.was_won(10), "but nobody reached ten");

        let batch = Batch { results: vec![led] };
        assert_eq!(batch.wins().get("a"), Some(&1), "a is counted as leading");
        assert_eq!(batch.games_won(10), 0, "and no game was won");
    }

    #[test]
    fn total_points_are_the_table_not_the_winner() {
        // Six seats on two each totals twelve, in a game that ends at ten. Reported as a mean
        // score it reads like a comfortable win.
        let spread = result(
            1,
            &[("a", 2), ("b", 2), ("c", 2), ("d", 2), ("e", 2), ("f", 2)],
        );

        assert_eq!(spread.total_points(), 12);
        assert_eq!(spread.top_score(), 2, "nobody is anywhere near winning");
        assert!(!spread.was_won(10));
    }

    #[test]
    fn a_failed_game_stays_in_the_batch() {
        // Counted, not hidden: dropping failures makes the completion rate a measure of the
        // games that happened to work.
        let mut failed = result(2, &[("a", 0)]);
        failed.finished = false;
        failed.error = Some("step refused".to_owned());
        failed.ended_because = Ending::Error;

        let batch = Batch {
            results: vec![result(1, &[("a", 10)]), failed],
        };

        assert_eq!(batch.results.len(), 2);
        assert_eq!(batch.errors().len(), 1);
        assert!(
            (batch.completion_rate() - 0.5).abs() < f64::EPSILON,
            "one of two games completed"
        );
    }

    #[test]
    fn seconds_per_decision_weighs_by_decisions_not_by_game() {
        // A mean of per-game rates would let a three-decision game weigh as much as a
        // three-hundred-decision one.
        let mut quick = result(1, &[("a", 1)]);
        quick.decisions = 1;
        quick.seconds = 1.0;
        let mut long = result(2, &[("a", 1)]);
        long.decisions = 99;
        long.seconds = 1.0;

        let batch = Batch {
            results: vec![quick, long],
        };

        assert!(
            (batch.seconds_per_decision() - 0.02).abs() < 1e-9,
            "two seconds over a hundred decisions, got {}",
            batch.seconds_per_decision()
        );
    }

    #[test]
    fn an_event_nobody_emitted_is_reported() {
        let mut played = result(1, &[("a", 1)]);
        played.events.insert("COMBAT_ROUND".to_owned(), 3);
        let batch = Batch {
            results: vec![played],
        };

        assert_eq!(batch.events().get("COMBAT_ROUND"), Some(&3));
        assert_eq!(
            batch.never_happened(&["COMBAT_ROUND", "INVASION_BEGAN"]),
            vec!["INVASION_BEGAN"]
        );
    }

    #[test]
    fn an_empty_batch_reports_zero_rather_than_dividing_by_it() {
        let empty = Batch::default();
        assert!((empty.completion_rate() - 0.0).abs() < f64::EPSILON);
        assert!((empty.mean_seconds() - 0.0).abs() < f64::EPSILON);
        assert!((empty.seconds_per_decision() - 0.0).abs() < f64::EPSILON);
        assert!((empty.win_rate_spread() - 0.0).abs() < f64::EPSILON);
    }
}
