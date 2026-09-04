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
    /// Sol's per-use Bellum Gloriosum allowance for fighters and ground forces.
    ///
    /// This is a production-limit exception, not transport capacity (LRR 68.1a).
    ProductionFreeCapacity,
    PlanetsControlled,
    ShipsInSystem,
    GroundForcesOnPlanet,
    /// Objectives this seat could score right now.
    ScoreableObjectives,
    ActionCardsHeld,
    TechnologiesOwned,
    /// Hits one roll (or one die) produces, before any casualty is assigned (LRR 78.13's
    /// hits-on-N-or-higher convention). What a hit count implies downstream — a destroyed unit,
    /// a cancelled hit — is producer-specific; this names only the count itself (OBS-008b2).
    Hits,
    /// Votes committed to the outcome a seat is backing (LRR 8.10-8.11). Distinct from
    /// `ConstraintKind::Votes`, which bounds how many an ongoing multi-pick ask may still spend;
    /// this is the exact running total a single option would reach (OBS-008f2).
    Votes,
    /// Secret objectives this seat holds, scored or not (LRR 45). Mirrors `ActionCardsHeld`'s
    /// role for the other over-the-limit hand: a return or a discard moves this by exactly one
    /// (OBS-008h3).
    SecretObjectivesHeld,
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

/// Exact deterministic previews, built from the same rules helpers the engine pays with (OBS-007b).
///
/// Analytic, not simulated. A preview that cloned the state and applied the change would agree with
/// application by construction and prove nothing; these compute the answer independently, so the
/// agreement tests below are a real check on both.
pub mod deterministic {
    use ti4_content::ContentStore;
    use ti4_model::content_types::SourceSet;
    use ti4_model::id::PlayerId;
    use ti4_model::state::GameState;

    use super::{Delta, Preview, Quantity};
    use crate::production::Spend;

    const fn pool_of(kind: Spend) -> Quantity {
        match kind {
            Spend::Resources => Quantity::Resources,
            Spend::Influence => Quantity::Influence,
        }
    }

    /// What paying `cost` would do to this seat's spendable pools.
    ///
    /// The pool falls by what the chosen plan is *worth*, not by the cost. Those differ whenever a
    /// face overpays: exhausting a four-influence planet against a three-influence bill removes
    /// four from what remains spendable, and reporting a fall of three would describe a position
    /// the seat is not in. That gap is the same one that billed seven influence for two command
    /// tokens.
    ///
    /// Unaffordable is `Unavailable`, never a zero-change `Certain`: the engine would refuse it.
    #[must_use]
    pub fn spend(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
        cost: i64,
        kind: Spend,
    ) -> Preview {
        let pool = crate::production::available(state, content, sources, player, kind);
        let goods = state
            .player(player)
            .map_or(0, |seat| i64::from(seat.trade_goods));
        if cost <= 0 {
            return Preview::certain(vec![
                Delta::new(pool_of(kind), pool, pool),
                Delta::new(Quantity::TradeGoods, goods, goods),
            ]);
        }
        let plans = crate::payment::plans(state, content, sources, player, cost, kind);
        let Some(plan) = plans.first() else {
            return Preview::unavailable("no plan pays this cost");
        };
        let worth = plan.worth(content, sources, kind);
        Preview::certain(vec![
            Delta::new(pool_of(kind), pool, pool - worth),
            Delta::new(
                Quantity::TradeGoods,
                goods,
                goods - i64::from(plan.trade_goods),
            ),
        ])
    }
}

#[cfg(test)]
mod obs007b_deterministic {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    use super::*;
    use crate::production::Spend;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn after(preview: &Preview, quantity: Quantity) -> i64 {
        preview
            .certain_deltas()
            .iter()
            .find(|delta| delta.quantity == quantity)
            .map(|delta| delta.after)
            .expect("the preview names this quantity")
    }

    /// The preview and the payment agree, and neither is derived from the other.
    ///
    /// `spend` computes the pool afterwards from the plan's worth. This applies the same plan for
    /// real and re-measures. Agreement is therefore a claim about two independent computations,
    /// which is the only version of this test worth writing: a preview that cloned the state and
    /// applied the change would agree by construction and check nothing.
    #[test]
    fn a_preview_agrees_with_actually_paying() {
        let content = ContentStore::embedded();
        for (cost, kind) in [
            (1_i64, Spend::Resources),
            (3, Spend::Resources),
            (1, Spend::Influence),
            (3, Spend::Influence),
        ] {
            let mut state = crate::fixtures::game(&["a", "b"]);
            state.player_mut(&player()).unwrap().trade_goods = 4;

            let preview = deterministic::spend(&state, content, POK, &player(), cost, kind);
            if !preview.is_informative() {
                continue; // unaffordable in this fixture; covered separately
            }
            let pool_kind = match kind {
                Spend::Resources => Quantity::Resources,
                Spend::Influence => Quantity::Influence,
            };
            let predicted_pool = after(&preview, pool_kind);
            let predicted_goods = after(&preview, Quantity::TradeGoods);

            let plan = crate::payment::plans(&state, content, POK, &player(), cost, kind)
                .into_iter()
                .next()
                .expect("informative preview implies a plan");
            assert!(crate::payment::apply(&mut state, &player(), &plan));

            assert_eq!(
                crate::production::available(&state, content, POK, &player(), kind),
                predicted_pool,
                "the pool after paying {cost} of {kind:?} is what the preview said"
            );
            assert_eq!(
                i64::from(state.player(&player()).unwrap().trade_goods),
                predicted_goods,
                "and so are the trade goods"
            );
        }
    }

    /// An unaffordable cost is refused, not described as free.
    #[test]
    fn what_cannot_be_paid_is_unavailable_rather_than_a_zero_delta() {
        let content = ContentStore::embedded();
        let state = crate::fixtures::game(&["a", "b"]);
        let preview =
            deterministic::spend(&state, content, POK, &player(), 9_999, Spend::Resources);
        assert!(!preview.is_informative());
        assert!(
            matches!(preview.outcome, Outcome::Unavailable { .. }),
            "the engine would refuse it, so it is Unavailable and not Unknown"
        );
        assert_eq!(preview.expected(Quantity::Resources), None);
    }

    /// A payment that cannot go through changes nothing.
    ///
    /// `payment::apply` validates the whole plan before it mutates anything, so a plan naming an
    /// already-exhausted planet leaves the position exactly as it was. Asserted rather than
    /// assumed: a half-applied payment would take the trade goods and leave the bill unpaid.
    #[test]
    fn a_refused_payment_is_atomic() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().trade_goods = 4;
        let plans = crate::payment::plans(&state, content, POK, &player(), 3, Spend::Resources);
        let Some(plan) = plans.into_iter().find(|plan| !plan.planets.is_empty()) else {
            return; // no planet-paying plan in this fixture
        };

        for planet in &plan.planets {
            state.exhaust_planet(planet.clone());
        }
        let goods_before = state.player(&player()).unwrap().trade_goods;
        let exhausted_before = state.exhausted_planets.clone();

        assert!(
            !crate::payment::apply(&mut state, &player(), &plan),
            "a plan naming an exhausted planet cannot be paid"
        );
        assert_eq!(
            state.player(&player()).unwrap().trade_goods,
            goods_before,
            "and takes nothing on the way out"
        );
        assert_eq!(state.exhausted_planets, exhausted_before);
    }

    /// Paying nothing is a computed no-change, not an absence of information.
    #[test]
    fn a_zero_cost_is_certain_and_not_unknown() {
        let content = ContentStore::embedded();
        let state = crate::fixtures::game(&["a", "b"]);
        let preview = deterministic::spend(&state, content, POK, &player(), 0, Spend::Resources);
        assert!(preview.is_informative());
        assert_eq!(preview.expected(Quantity::Resources), Some((0, 1)));
    }
}

#[cfg(test)]
mod obs007b_alternate_faces {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;

    use super::*;
    use crate::production::Spend;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    /// The agreement must hold where a planet has a second face, not only where it has one.
    ///
    /// `production::available` counts a planet at its largest face, and Archon's Gift adds the
    /// other kind's printed value as an alternate. `Plan::worth` reads the printed value from
    /// content alone and has no way to see a breakthrough the seat holds. If those diverge, a
    /// preview built from `worth` would report a pool that the position does not have — so this is
    /// the case the ordinary fixture cannot reach, asserted directly.
    #[test]
    fn a_second_face_does_not_break_the_agreement() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.player_mut(&player()).unwrap().trade_goods = 0;
        state.player_mut(&player()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("xxchabt"));

        for (cost, kind) in [(1_i64, Spend::Resources), (2, Spend::Influence)] {
            let mut trial = state.clone();
            let preview = deterministic::spend(&trial, content, POK, &player(), cost, kind);
            if !preview.is_informative() {
                continue;
            }
            let pool_kind = match kind {
                Spend::Resources => Quantity::Resources,
                Spend::Influence => Quantity::Influence,
            };
            let predicted = preview
                .certain_deltas()
                .iter()
                .find(|delta| delta.quantity == pool_kind)
                .map(|delta| delta.after)
                .expect("the preview names the pool");

            let plan = crate::payment::plans(&trial, content, POK, &player(), cost, kind)
                .into_iter()
                .next()
                .expect("informative preview implies a plan");
            assert!(crate::payment::apply(&mut trial, &player(), &plan));

            assert_eq!(
                crate::production::available(&trial, content, POK, &player(), kind),
                predicted,
                "with an alternate face in play, paying {cost} of {kind:?} must still land where \
                 the preview said"
            );
        }
    }
}

/// Exact public dice-mechanic distributions, computed rather than sampled (OBS-007c).
///
/// The stochastic counterpart to [`deterministic`]: shared math a per-class producer (OBS-008b's
/// combat, invasion, and reroll steps) can build a [`Preview::chanced`] from, rather than each
/// re-deriving the same binomial arithmetic. This module never rolls a die and never consumes
/// [`crate::rng::GameRng`] — it describes the distribution a roll *would* have, from public rules
/// alone.
pub mod stochastic {
    use super::{Chance, Delta, Preview};

    /// The largest dice pool this module represents as an exact distribution.
    ///
    /// [`Chance::weight`] is `u32` (an OBS-007a decision, not reopened here), and `10^9` is the
    /// largest power of ten that fits. A caller with more dice gets [`Preview::unknown`], never a
    /// distribution silently rescaled to fit: approximating a rules-exact quantity is exactly what
    /// this module's callers must not do.
    pub const MAX_DICE: u32 = 9;

    /// The exact distribution of hit counts from `dice` ten-sided dice, each hitting on `hit_on` or
    /// higher (LRR 78.13) — the same threshold convention [`crate::dice::Roll::hits_on`] and
    /// [`crate::dice::Roll::hits`] already use. `deltas_for(hits)` maps one hit count to the deltas
    /// that result; what a hit count *means* is producer-specific, the odds are not.
    ///
    /// `hit_on <= 1` always hits and `hit_on > 10` never can (a d10 has no eleventh face); both
    /// collapse to [`Preview::certain`] rather than a one-case distribution, since there is no
    /// chance involved. So does `dice == 0`.
    ///
    /// # Panics
    /// Never in practice: every weight is bounded by `10^dice <= 10^MAX_DICE`, which fits `u32`
    /// by construction once `dice > MAX_DICE` has already returned early.
    #[must_use]
    pub fn hit_count_preview(
        dice: u32,
        hit_on: u32,
        deltas_for: impl Fn(u32) -> Vec<Delta>,
    ) -> Preview {
        if dice == 0 {
            return Preview::certain(deltas_for(0));
        }
        if hit_on <= 1 {
            return Preview::certain(deltas_for(dice));
        }
        if hit_on > 10 {
            return Preview::certain(deltas_for(0));
        }
        if dice > MAX_DICE {
            return Preview::unknown("dice pool exceeds exact representable precision");
        }
        let successes = u128::from(11 - hit_on);
        let misses = 10 - successes;
        let cases: Vec<Chance> = (0..=dice)
            .map(|hits| {
                let weight = binomial(dice, hits) * successes.pow(hits) * misses.pow(dice - hits);
                Chance {
                    label: format!("{hits}-hits"),
                    weight: u32::try_from(weight).expect("bounded by MAX_DICE"),
                    deltas: deltas_for(hits),
                }
            })
            .collect();
        Preview::chanced(cases)
    }

    /// `n` choose `k`, exact. Only ever called with `n <= MAX_DICE`, so the running product never
    /// approaches `u128`'s range; the incremental multiply-then-divide keeps every intermediate
    /// value an exact integer (the standard identity for Pascal's-triangle-by-multiplication).
    fn binomial(n: u32, k: u32) -> u128 {
        let k = k.min(n - k);
        let mut result: u128 = 1;
        for i in 0..k {
            result = result * u128::from(n - i) / u128::from(i + 1);
        }
        result
    }
}

#[cfg(test)]
mod obs007c_stochastic {
    use super::stochastic::{MAX_DICE, hit_count_preview};
    use super::{Delta, Outcome, Quantity};

    /// A fixed one-quantity delta per hit count, for tests that only care about the distribution
    /// shape rather than what a hit means.
    fn marker(hits: u32) -> Vec<Delta> {
        vec![Delta::new(Quantity::ShipsInSystem, 0, i64::from(hits))]
    }

    #[test]
    fn hit_count_distribution_matches_hand_computed_binomial_odds() {
        // Two dice, hitting on 6+: 5/10 per die. P(0)=1/4, P(1)=1/2, P(2)=1/4 -- 25/50/25 of 100.
        let preview = hit_count_preview(2, 6, marker);
        let Outcome::Chanced { cases, out_of } = preview.outcome else {
            panic!("expected a chanced outcome");
        };
        assert_eq!(out_of, 100);
        let weight_of = |label: &str| {
            cases
                .iter()
                .find(|case| case.label == label)
                .map_or_else(|| panic!("no case labelled {label}"), |case| case.weight)
        };
        assert_eq!(weight_of("0-hits"), 25);
        assert_eq!(weight_of("1-hits"), 50);
        assert_eq!(weight_of("2-hits"), 25);
    }

    #[test]
    fn hit_on_at_or_below_one_is_certain_not_a_one_case_distribution() {
        let preview = hit_count_preview(3, 1, marker);
        assert!(matches!(preview.outcome, Outcome::Certain { .. }));
        assert_eq!(preview.certain_deltas(), marker(3).as_slice());

        let preview = hit_count_preview(3, 0, marker);
        assert!(matches!(preview.outcome, Outcome::Certain { .. }));
    }

    #[test]
    fn hit_on_above_ten_is_certain_at_zero_hits() {
        let preview = hit_count_preview(4, 11, marker);
        assert!(matches!(preview.outcome, Outcome::Certain { .. }));
        assert_eq!(preview.certain_deltas(), marker(0).as_slice());
    }

    #[test]
    fn zero_dice_is_certain_at_zero_hits_for_any_threshold() {
        for hit_on in [1, 6, 11] {
            let preview = hit_count_preview(0, hit_on, marker);
            assert!(matches!(preview.outcome, Outcome::Certain { .. }));
            assert_eq!(preview.certain_deltas(), marker(0).as_slice());
        }
    }

    #[test]
    fn nine_dice_is_the_largest_exact_pool_and_ten_is_refused() {
        let preview = hit_count_preview(MAX_DICE, 6, marker);
        assert!(preview.is_informative(), "nine dice is still exact");

        let preview = hit_count_preview(MAX_DICE + 1, 6, marker);
        assert!(
            !preview.is_informative(),
            "ten dice must not be silently rescaled to fit u32"
        );
        assert!(matches!(preview.outcome, Outcome::Unknown { .. }));
    }

    #[test]
    fn the_expected_hit_count_is_the_textbook_binomial_mean() {
        // Three dice hitting on 7+ (4/10 per die): expected hits is 3 * 4/10 = 12/10, which does
        // not reduce to a whole number -- proving the rational survives rather than being rounded.
        let preview = hit_count_preview(3, 7, marker);
        let (numerator, denominator) = preview
            .expected(Quantity::ShipsInSystem)
            .expect("a chanced preview is informative");
        assert_eq!(
            (numerator, denominator),
            (1200, 1000),
            "3 dice * 4/10 per die, as an exact rational over the full 10^3 denominator"
        );
        assert!(
            (f64_from_i64(numerator) / f64::from(denominator)) > 1.0,
            "sanity: 1.2 expected hits"
        );
    }

    fn f64_from_i64(value: i64) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "test-only sanity check on a value far below f64's exact-integer range"
        )]
        let value = value as f64;
        value
    }
}
