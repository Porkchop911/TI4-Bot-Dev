//! Named score components, and the record of one scored decision (M08-003).
//!
//! Ported from the oracle's `bots.py` `Decision` and its `dict[str, float]` component maps.
//!
//! A score is a sum of *named* parts rather than a single number. That is not decoration: a bot
//! that plays badly is debugged by asking which component was wrong, and a component nobody named
//! cannot be answered for. The same named parts are what a learned policy is later allowed to
//! reweight, so folding them into one opaque total would close that door too.
//!
//! Order is insertion order, not alphabetical. A breakdown reads as the argument the bot made,
//! and an argument has an order.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A score, kept as the parts it was built from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Components {
    parts: Vec<(&'static str, f64)>,
}

impl Components {
    /// An empty score. Totals zero, which is the right value for "nothing to say about this".
    #[must_use]
    pub const fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// A score of one named part.
    #[must_use]
    pub fn of(name: &'static str, value: f64) -> Self {
        Self {
            parts: vec![(name, value)],
        }
    }

    /// Add a named part. Adding the same name twice keeps both, because two reasons for the same
    /// judgement are two reasons.
    #[must_use]
    pub fn and(mut self, name: &'static str, value: f64) -> Self {
        self.parts.push((name, value));
        self
    }

    /// The parts, in the order they were argued.
    #[must_use]
    pub fn parts(&self) -> &[(&'static str, f64)] {
        &self.parts
    }

    /// The sum of the parts, which is the score.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.parts.iter().map(|(_, value)| value).sum()
    }

    /// Whether this score was argued at all, as opposed to defaulting to nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// One line naming every part and its value.
    #[must_use]
    pub fn explain(&self) -> String {
        if self.parts.is_empty() {
            return "(no components)".to_owned();
        }
        self.parts
            .iter()
            .map(|(name, value)| format!("{name}={value:+.2}"))
            .collect::<Vec<String>>()
            .join(" ")
    }
}

/// One scored choice, kept so play can be explained and tuned.
///
/// The breakdown is held per option rather than only for the winner: "why this one" is not
/// answerable without "and what the others were worth".
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// Who was asked.
    pub player: String,
    /// What they were asked.
    pub prompt: String,
    /// The option id taken.
    pub chosen: String,
    /// Every offered option's components, by option id.
    pub breakdown: BTreeMap<String, Components>,
    /// The shortlist the choice was actually sampled from.
    ///
    /// Separate from `breakdown` because an option can be scored and still not be considered —
    /// and a log that showed only the survivors would hide the filter that did the work.
    pub considered: Vec<String>,
}

impl Decision {
    /// Totals by option id.
    #[must_use]
    pub fn scores(&self) -> BTreeMap<&str, f64> {
        self.breakdown
            .iter()
            .map(|(id, parts)| (id.as_str(), parts.total()))
            .collect()
    }

    /// Every option, its total and its parts, best first — the decision as an argument.
    #[must_use]
    pub fn explain(&self) -> String {
        let mut rows: Vec<(&String, &Components)> = self.breakdown.iter().collect();
        rows.sort_by(|(a_id, a), (b_id, b)| {
            b.total()
                .partial_cmp(&a.total())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a_id.cmp(b_id))
        });
        let mut out = format!("{} — {}\n", self.player, self.prompt);
        for (id, parts) in rows {
            let mark = if *id == self.chosen { "*" } else { " " };
            let listed = if self.considered.iter().any(|c| c == id) {
                ""
            } else {
                " (not considered)"
            };
            let _ = writeln!(
                out,
                "{mark} {:>8.2}  {id}{listed}  [{}]",
                parts.total(),
                parts.explain()
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_is_the_sum_of_its_parts() {
        let score = Components::of("prize", 4.0).and("risk", -1.5);
        assert!((score.total() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn an_unargued_score_is_zero_rather_than_a_panic() {
        // Reached whenever a choice kind has no registered scorer. Zero and *named as empty* is
        // the safe answer: the option stays legal and rankable, and the gap is visible.
        let score = Components::new();
        assert!(score.is_empty());
        assert!(score.total().abs() < f64::EPSILON);
        assert_eq!(score.explain(), "(no components)");
    }

    #[test]
    fn two_reasons_for_the_same_judgement_are_both_kept() {
        // Not a map: overwriting would silently drop one argument and change the total.
        let score = Components::of("planet", 2.0).and("planet", 3.0);
        assert_eq!(score.parts().len(), 2);
        assert!((score.total() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_parts_stay_in_the_order_they_were_argued() {
        let score = Components::of("first", 1.0)
            .and("second", 2.0)
            .and("third", 3.0);
        let names: Vec<&str> = score.parts().iter().map(|(name, _)| *name).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn a_decision_explains_the_losers_as_well_as_the_winner() {
        // "Why this one" is not answerable without "and what the others were worth".
        let decision = Decision {
            player: "a".to_owned(),
            prompt: "assign a hit".to_owned(),
            chosen: "destroy|1".to_owned(),
            breakdown: [
                ("destroy|0".to_owned(), Components::of("loss", -4.0)),
                ("destroy|1".to_owned(), Components::of("loss", -0.5)),
                ("destroy|2".to_owned(), Components::of("loss", -2.0)),
            ]
            .into_iter()
            .collect(),
            considered: vec!["destroy|1".to_owned(), "destroy|2".to_owned()],
        };

        let text = decision.explain();
        assert!(text.contains("destroy|0"), "the rejected option is shown");
        assert!(
            text.contains("(not considered)"),
            "and the filter is visible: {text}"
        );
        let chosen_line = text
            .lines()
            .find(|line| line.starts_with('*'))
            .expect("the chosen option is marked");
        assert!(chosen_line.contains("destroy|1"));

        let scores = decision.scores();
        assert!((scores["destroy|0"] + 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_explanation_orders_by_score_not_by_id() {
        let decision = Decision {
            player: "a".to_owned(),
            prompt: "pick".to_owned(),
            chosen: "z".to_owned(),
            breakdown: [
                ("a".to_owned(), Components::of("x", 1.0)),
                ("z".to_owned(), Components::of("x", 9.0)),
            ]
            .into_iter()
            .collect(),
            considered: vec!["a".to_owned(), "z".to_owned()],
        };

        let text = decision.explain();
        let listed: Vec<&str> = text
            .lines()
            .skip(1)
            .filter_map(|line| {
                line.split_whitespace()
                    .find(|word| *word == "a" || *word == "z")
            })
            .collect();
        assert_eq!(listed, vec!["z", "a"], "best first");
    }
}
