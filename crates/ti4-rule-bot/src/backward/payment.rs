//! Backward planning for the objectives that are bought: "spend N resources / influence / trade
//! goods / command tokens" (Erect a Monument, Found a Golden Age, Sway the Council, Manipulate
//! Galactic Law, Negotiate Trade Routes, Centralize Galactic Trade, Lead From the Front,
//! Galvanize the People, Amass Wealth, Hold Vast Reserves).
//!
//! These are the objectives the rule bot scores most (93% of the seats for the stage I ones)
//! without any steering, because the price is paid from the whole empire and needs no board
//! state. The planner therefore models them honestly as cheap: a bought objective that the seat
//! can pay now is reachable with probability close to one, and the only steer is to **keep the
//! price unspent until the status phase** (the [`Reserve`]); one that is a little short may be
//! helped by a strategy card (Trade for trade goods, Leadership for tokens). Each plan carries
//! the reserve of its own price; the weighing between plans (and the payment discount) is the
//! framework's.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `monument` | Erect a Monument | public | 1 | `cost_spend_resources` |
//! | `sway_council` | Sway the Council | public | 1 | `cost_spend_influence` |
//! | `trade_routes` | Negotiate Trade Routes | public | 1 | `cost_trade_goods` |
//! | `lead` | Lead From the Front | public | 1 | `cost_tokens` |
//! | `amass_wealth` | Amass Wealth | public | 1 | `cost_all_three` |
//! | `golden_age` | Found a Golden Age | public | 2 | `cost_spend_resources` |
//! | `manipulate_law` | Manipulate Galactic Law | public | 2 | `cost_spend_influence` |
//! | `centralize_trade` | Centralize Galactic Trade | public | 2 | `cost_trade_goods` |
//! | `galvanize` | Galvanize the People | public | 2 | `cost_tokens` |
//! | `vast_reserves` | Hold Vast Reserves | public | 2 | `cost_all_three` |

use ti4_engine::choice::SeatObservation;
use ti4_engine::objectives::Cost as Price;
use ti4_model::id::ObjectiveId;

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer};
use crate::objectives::{Goal, Reserve};

/// The bought objectives of the corpus.
pub const IDS: &[&str] = &[
    "monument",
    "golden_age",
    "sway_council",
    "manipulate_law",
    "trade_routes",
    "centralize_trade",
    "lead",
    "galvanize",
    "amass_wealth",
    "vast_reserves",
];

/// Probability that a price payable now is still payable in the status phase: the planets and
/// goods are only at risk from forced payments (production beyond the reserve, a secondary).
pub const HELD_P: f64 = 0.95;
/// A strategy card that closes a gap of at most this many price units (trade goods or tokens).
pub const CARD_REACH: f64 = 3.0;
/// Probability the card closes the gap once drafted (others may take it first).
pub const CARD_P: f64 = 0.45;

/// The plan of one bought goal, or `None` when it is no bought objective, or a gap that no card
/// closes (planet spends are left to the default logic).
#[must_use]
pub fn plan_for(goal: &Goal) -> Option<Plan> {
    if !goal.bought || goal.secret {
        return None;
    }
    let price = ti4_engine::objectives::cost_of(&ObjectiveId::new(&goal.alias))?;
    let tokens = 0;
    // Paying is not a cost of the plan's route: it only holds the price back (the reserve), so
    // the plan charges nothing beyond a note of what it keeps.
    let cost = Cost {
        tokens,
        resources: 0,
        activations: 0,
    };
    let reserve_note = Reserve::for_cost(price);
    let (probability, cards, reason) = if goal.satisfied {
        (
            HELD_P,
            Vec::new(),
            format!(
                "{} is payable now: keep {reserve_note:?} until the status phase",
                goal.alias
            ),
        )
    } else {
        let gap = goal.gap();
        let card = match price {
            Price::TradeGoods(_) | Price::AllThree(_) if gap <= CARD_REACH => "Trade",
            Price::Tokens(_) if gap <= CARD_REACH => "Leadership",
            _ => return None,
        };
        (
            CARD_P,
            vec![card.to_owned()],
            format!("{} is {gap:.0} short: {card} would close it", goal.alias),
        )
    };
    Some(Plan {
        objective: goal.alias.clone(),
        points: goal.points,
        probability,
        cost,
        steer: Steer {
            cards,
            ..Steer::default()
        },
        reason,
        payment: true,
        secret: false,
    })
}

/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &[
    "monument",
    "sway_council",
    "trade_routes",
    "lead",
    "amass_wealth",
    "golden_age",
    "manipulate_law",
    "centralize_trade",
    "galvanize",
    "vast_reserves",
];

/// The payment group.
pub struct Payment;

impl ObjectivePlanner for Payment {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, _obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        plan_for(&ctx.goal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(alias: &str, points: f64, have: f64, threshold: f64) -> Goal {
        Goal {
            alias: alias.to_owned(),
            secret: false,
            points,
            bought: true,
            have,
            threshold,
            satisfied: have >= threshold,
            family: String::new(),
        }
    }

    #[test]
    fn payable_now_is_reachable_and_reserves_its_price() {
        let p = plan_for(&goal("monument", 1.0, 8.0, 8.0)).unwrap();
        assert!(p.probability > 0.9 && p.payment);
    }

    #[test]
    fn a_small_goods_or_token_gap_is_helped_by_a_card() {
        let p = plan_for(&goal("trade_routes", 1.0, 3.0, 5.0)).unwrap();
        assert_eq!(p.steer.cards[0], "Trade");
        assert!(p.probability < 0.9);
        let p = plan_for(&goal("lead", 1.0, 1.0, 3.0)).unwrap();
        assert_eq!(p.steer.cards[0], "Leadership");
    }

    #[test]
    fn a_planet_spend_gap_or_a_big_gap_is_not_planned() {
        // Resources and influence are not fetched by a card: fallback.
        assert!(plan_for(&goal("sway_council", 1.0, 3.0, 8.0)).is_none());
        assert!(plan_for(&goal("centralize_trade", 2.0, 2.0, 10.0)).is_none());
    }

    #[test]
    fn requirement_objectives_are_not_bought() {
        let mut g = goal("corner", 1.0, 4.0, 4.0);
        g.bought = false;
        assert!(plan_for(&g).is_none());
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Payment),
            crate::backward::coverage::sorted(COVERS)
        );
    }
}
