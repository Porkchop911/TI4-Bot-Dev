//! What an option would do, stated as fact (OBS-007a).
//!
//! A policy choosing between options can see what each one *is* — its id, its kind, its payload —
//! and not what it would *do*. "Produce a carrier" and "produce a destroyer" differ in cost, in
//! fleet supply, in capacity and in what the seat can afford afterwards, and none of that is in the
//! option. The consequence has to be re-derived by the network from the position, for every option,
//! every time.
//!
//! This module defines the type that carries it. It computes nothing: the deterministic helpers are
//! OBS-007b, the stochastic ones OBS-007c, and the per-class producers OBS-008.
//!
//! # Three rules the type enforces
//!
//! **Fail closed.** A preview that cannot be computed says so. There is no zero-valued default and
//! no "probably nothing": [`Outcome::Unknown`] carries a reason, and a reader that treats it as
//! "no change" is making that error visibly rather than inheriting it silently. This matters more
//! than it sounds — a shaping term fed a confident zero learns that the action is free.
//!
//! **Cannot mutate.** Every entry point takes `&GameState`. Previewing is a question, and a
//! question that can change the answer is not one. The borrow checker enforces this rather than a
//! convention anyone has to remember.
//!
//! **Bounded.** A preview is capped at [`MAX_DELTAS`] and says when it truncated. An option that
//! moves forty quantities produces a bounded summary that admits it is one, instead of a vector
//! whose width depends on the position.
//!
//! # Unknown is not Unavailable
//!
//! [`Outcome::Unavailable`] means the option cannot be taken — the engine would refuse it.
//! [`Outcome::Unknown`] means it can be taken and the consequence was not computable here. Folding
//! those together would teach a policy that anything unmodelled is illegal, which is both false and
//! self-reinforcing: it would stop choosing exactly the options nobody had got round to modelling.

use serde::{Deserialize, Serialize};

/// The most deltas one preview may carry.
///
/// Chosen so a preview stays a summary. The cap is part of the contract rather than an
/// implementation detail, because a consumer sizing a feature block needs to know it.
pub const MAX_DELTAS: usize = 12;

/// A quantity a preview can talk about.
///
/// A closed list. An open string would let each producer invent its own spelling of "resources",
/// and a consumer cannot align features it cannot name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quantity {
    Resources,
    Influence,
    TradeGoods,
    Commodities,
    VictoryPoints,
    TacticTokens,
    FleetTokens,
    StrategicTokens,
    /// Non-fighter ships this seat may keep in the system concerned (LRR 37).
    FleetSupplyHeadroom,
    /// Transport capacity free in the system concerned (LRR 16).
    CapacityFree,
    /// Units left in one use of PRODUCTION (LRR 68).
    ProductionRemaining,
    PlanetsControlled,
    ShipsInSystem,
    GroundForcesOnPlanet,
    /// Objectives this seat could score right now.
    ScoreableObjectives,
    ActionCardsHeld,
    TechnologiesOwned,
}

/// One quantity, before and after.
///
/// Both ends are carried rather than a single signed change, because "3 resources of 4" and
/// "3 resources of 30" are different decisions and a delta alone cannot tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delta {
    pub quantity: Quantity,
    pub before: i64,
    pub after: i64,
}

impl Delta {
    #[must_use]
    pub const fn new(quantity: Quantity, before: i64, after: i64) -> Self {
        Self {
            quantity,
            before,
            after,
        }
    }

    #[must_use]
    pub const fn change(&self) -> i64 {
        self.after - self.before
    }

    #[must_use]
    pub const fn is_change(&self) -> bool {
        self.before != self.after
    }
}

/// One publicly-known possible result of a random effect.
///
/// `weight` is a count of equally likely cases, not a float: a d10 hitting on 7 or better is
/// `weight 4` of `out_of 10`, which is exact, comparable and cannot drift. Producers that only know
/// the support and not the odds set every weight to one and say so through the total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chance {
    pub label: String,
    pub weight: u32,
    pub deltas: Vec<Delta>,
}

/// What taking the option would do.
///
/// `Serialize` but not `Deserialize`: the reasons are `&'static str`, which cannot be deserialised,
/// and that is the point. A reason is a compile-time literal chosen by the producer, so the set of
/// things a preview can say it does not know stays enumerable by reading the source, instead of
/// becoming free text invented at runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Outcome {
    /// Exactly this, every time.
    Certain { deltas: Vec<Delta> },
    /// One of these, with publicly known odds.
    ///
    /// `out_of` is the denominator, so a reader never has to sum the weights and hope. Cases whose
    /// odds are not public are represented by their support with equal weights.
    Chanced { cases: Vec<Chance>, out_of: u32 },
    /// The option is legal and the consequence was not computed here.
    Unknown { reason: &'static str },
    /// The engine would refuse the option.
    Unavailable { reason: &'static str },
}

/// A bounded, factual summary of what one option would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub outcome: Outcome,
    /// Whether the deltas were cut at [`MAX_DELTAS`].
    pub truncated: bool,
}

impl Preview {
    /// A certain outcome, truncated to the cap.
    #[must_use]
    pub fn certain(deltas: Vec<Delta>) -> Self {
        let truncated = deltas.len() > MAX_DELTAS;
        let mut deltas = deltas;
        // Sorted before truncating so the surviving entries do not depend on the order a producer
        // happened to push them: the same option must summarise identically however it was built.
        deltas.sort_by_key(|delta| delta.quantity);
        deltas.truncate(MAX_DELTAS);
        Self {
            outcome: Outcome::Certain { deltas },
            truncated,
        }
    }

    /// A random outcome. The denominator is the sum of the weights.
    ///
    /// Refused, as [`Outcome::Unknown`], when there are no cases or every weight is zero: a
    /// distribution over nothing is not a distribution, and inventing one here would put a
    /// confident falsehood where the honest answer is "not computed".
    #[must_use]
    pub fn chanced(cases: Vec<Chance>) -> Self {
        let out_of: u32 = cases.iter().map(|case| case.weight).sum();
        if cases.is_empty() || out_of == 0 {
            return Self::unknown("a distribution with no weight is not a distribution");
        }
        let truncated = cases.iter().any(|case| case.deltas.len() > MAX_DELTAS);
        let cases = cases
            .into_iter()
            .map(|mut case| {
                case.deltas.sort_by_key(|delta| delta.quantity);
                case.deltas.truncate(MAX_DELTAS);
                case
            })
            .collect();
        Self {
            outcome: Outcome::Chanced { cases, out_of },
            truncated,
        }
    }

    #[must_use]
    pub const fn unknown(reason: &'static str) -> Self {
        Self {
            outcome: Outcome::Unknown { reason },
            truncated: false,
        }
    }

    #[must_use]
    pub const fn unavailable(reason: &'static str) -> Self {
        Self {
            outcome: Outcome::Unavailable { reason },
            truncated: false,
        }
    }

    /// Whether this preview asserts anything about consequences.
    ///
    /// False for both `Unknown` and `Unavailable`. A consumer building features should read this
    /// before reading deltas, so that "not computed" cannot be mistaken for "computed as nothing".
    #[must_use]
    pub const fn is_informative(&self) -> bool {
        matches!(
            self.outcome,
            Outcome::Certain { .. } | Outcome::Chanced { .. }
        )
    }

    /// The deltas of a certain outcome, or the empty slice.
    ///
    /// Deliberately empty rather than `Option`, so a caller that ignores the distinction gets
    /// nothing rather than a plausible number. `is_informative` is how the distinction is made.
    #[must_use]
    pub fn certain_deltas(&self) -> &[Delta] {
        match &self.outcome {
            Outcome::Certain { deltas } => deltas,
            _ => &[],
        }
    }

    /// The expected change in one quantity, as a rational over the denominator.
    ///
    /// Returns `None` when the preview asserts nothing, so an unknown consequence cannot become a
    /// confident zero. Integer arithmetic throughout: the odds are counts and stay counts.
    #[must_use]
    pub fn expected(&self, quantity: Quantity) -> Option<(i64, u32)> {
        match &self.outcome {
            Outcome::Certain { deltas } => Some((
                deltas
                    .iter()
                    .filter(|delta| delta.quantity == quantity)
                    .map(Delta::change)
                    .sum(),
                1,
            )),
            Outcome::Chanced { cases, out_of } => {
                let weighted: i64 = cases
                    .iter()
                    .map(|case| {
                        let change: i64 = case
                            .deltas
                            .iter()
                            .filter(|delta| delta.quantity == quantity)
                            .map(Delta::change)
                            .sum();
                        change * i64::from(case.weight)
                    })
                    .sum();
                Some((weighted, *out_of))
            }
            Outcome::Unknown { .. } | Outcome::Unavailable { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_uncomputed_consequence_never_becomes_a_confident_zero() {
        // The failure this whole contract exists to prevent: a shaping term fed a zero learns the
        // action is free.
        let unknown = Preview::unknown("not modelled");
        assert!(!unknown.is_informative());
        assert_eq!(unknown.expected(Quantity::Resources), None);
        assert!(unknown.certain_deltas().is_empty());

        let certain = Preview::certain(vec![Delta::new(Quantity::Resources, 4, 4)]);
        assert!(certain.is_informative());
        assert_eq!(
            certain.expected(Quantity::Resources),
            Some((0, 1)),
            "a computed zero is a real answer and must be distinguishable from an absent one"
        );
    }

    #[test]
    fn unavailable_is_not_unknown() {
        let refused = Preview::unavailable("cannot afford");
        let unmodelled = Preview::unknown("not modelled");
        assert_ne!(refused.outcome, unmodelled.outcome);
        assert!(!refused.is_informative());
        assert!(!unmodelled.is_informative());
    }

    #[test]
    fn deltas_are_capped_and_say_so() {
        let many: Vec<Delta> = [
            Quantity::Resources,
            Quantity::Influence,
            Quantity::TradeGoods,
            Quantity::Commodities,
            Quantity::VictoryPoints,
            Quantity::TacticTokens,
            Quantity::FleetTokens,
            Quantity::StrategicTokens,
            Quantity::FleetSupplyHeadroom,
            Quantity::CapacityFree,
            Quantity::ProductionRemaining,
            Quantity::PlanetsControlled,
            Quantity::ShipsInSystem,
            Quantity::GroundForcesOnPlanet,
        ]
        .into_iter()
        .map(|quantity| Delta::new(quantity, 0, 1))
        .collect();
        assert!(many.len() > MAX_DELTAS);

        let preview = Preview::certain(many);
        assert!(preview.truncated, "a summary that cut something says so");
        assert_eq!(preview.certain_deltas().len(), MAX_DELTAS);
    }

    #[test]
    fn the_summary_does_not_depend_on_the_order_a_producer_built_it() {
        let one = Preview::certain(vec![
            Delta::new(Quantity::Influence, 2, 0),
            Delta::new(Quantity::Resources, 5, 3),
        ]);
        let other = Preview::certain(vec![
            Delta::new(Quantity::Resources, 5, 3),
            Delta::new(Quantity::Influence, 2, 0),
        ]);
        assert_eq!(one, other);
    }

    #[test]
    fn odds_are_counts_and_the_denominator_is_carried() {
        // A d10 hitting on 7 or better: four cases of ten, exactly, with no float anywhere.
        let preview = Preview::chanced(vec![
            Chance {
                label: "hit".to_owned(),
                weight: 4,
                deltas: vec![Delta::new(Quantity::ShipsInSystem, 3, 2)],
            },
            Chance {
                label: "miss".to_owned(),
                weight: 6,
                deltas: vec![Delta::new(Quantity::ShipsInSystem, 3, 3)],
            },
        ]);
        let Outcome::Chanced { out_of, .. } = preview.outcome else {
            panic!("chanced");
        };
        assert_eq!(out_of, 10, "the denominator is carried, not re-derived");
        assert_eq!(
            preview.expected(Quantity::ShipsInSystem),
            Some((-4, 10)),
            "four tenths of a ship, as a rational"
        );
    }

    #[test]
    fn a_distribution_over_nothing_is_refused_rather_than_invented() {
        assert!(!Preview::chanced(Vec::new()).is_informative());
        assert!(
            !Preview::chanced(vec![Chance {
                label: "impossible".to_owned(),
                weight: 0,
                deltas: Vec::new(),
            }])
            .is_informative(),
            "zero total weight is not a distribution"
        );
    }

    #[test]
    fn both_ends_are_carried_because_a_change_alone_is_ambiguous() {
        let scarce = Delta::new(Quantity::Resources, 4, 1);
        let plentiful = Delta::new(Quantity::Resources, 30, 27);
        assert_eq!(scarce.change(), plentiful.change());
        assert_ne!(
            scarce, plentiful,
            "spending three of four is not spending three of thirty"
        );
    }
}
