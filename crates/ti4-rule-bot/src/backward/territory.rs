//! Territory goals: control N planets, planets of a trait or with specialties, outside home,
//! attached planets, legendary planets, and what the planets are worth together.
//!
//! Backward: the goal is met when `threshold` planets of the right kind are controlled. The
//! cheapest route is the fewest activations (at most three, within the tactic tokens) whose
//! landings take enough of them, judged by the engine's own requirement functions on the
//! imagined planets ([`PlanCtx::route`]). Only steps that take a planet qualify; the probability
//! is the product of the steps' fight and landing odds. Nothing within three activations: no
//! plan (the default rules play). Prepared steps ([`super::prep`]: build, Warfare gather,
//! technology range or strength) are seen through the same route, so every alias below also
//! gets the four prep kinds.
//!
//! # Objectives (alias, name, points; family token)
//!
//! Public:
//!
//! | alias | name | VP | family |
//! |---|---|---|---|
//! | `expand_borders` | Expand Borders | 1 | `non_home` |
//! | `subdue` | Subdue the Galaxy | 2 | `non_home` |
//! | `corner` | Corner the Market | 1 | `same_trait` |
//! | `unify_colonies` | Unify the Colonies | 2 | `same_trait` |
//! | `research_outposts` | Found Research Outposts | 1 | `tech_specialties` |
//! | `brain_trust` | Form Galactic Brain Trust | 2 | `tech_specialties` |
//! | `lost_outposts` | Discover Lost Outposts | 1 | `attached_planets` |
//! | `ancient_monuments` | Reclaim Ancient Monuments | 2 | `attached_planets` |
//! | `conquer` | Conquer the Weak | 2 | `rival_home_planets` |
//! | `distant_lands` | Rule Distant Lands | 2 | `distinct_rival_home_reaches` |
//! | `push_boundaries` | Push Boundaries | 1 | `weaker_neighbours` |
//!
//! Secret (1 VP each):
//!
//! | alias | name | family |
//! |---|---|---|
//! | `faa` | Forge an Alliance | `planets_of_trait_cultural` |
//! | `mrm` | Mine Rare Metals | `planets_of_trait_hazardous` |
//! | `mp` | Monopolize Production | `planets_of_trait_industrial` |
//! | `sai` | Seize an Icon | `legendary_planets` |
//! | `syc` | Stake Your Claim | `shared_planet_systems` |
//! | `eh` | Establish Hegemony | `combined_value_influence` |
//! | `hrm` | Hoard Raw Materials | `combined_value_resources` |

use ti4_engine::choice::SeatObservation;

use super::{ObjectivePlanner, Plan, PlanCtx};

/// The territory group.
pub struct Territory;

impl ObjectivePlanner for Territory {
    fn objective_ids(&self) -> &[&str] {
        &[
            "non_home",
            "same_trait",
            "tech_specialties",
            "attached_planets",
            "rival_home_planets",
            "distinct_rival_home_reaches",
            "weaker_neighbours",
            "planets_of_trait_cultural",
            "planets_of_trait_hazardous",
            "planets_of_trait_industrial",
            "legendary_planets",
            "shared_planet_systems",
            "combined_value_influence",
            "combined_value_resources",
        ]
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        // A bought-cost goal never reaches here; a goal already within one planet is cheapest.
        ctx.route_plan(obs, 3, |step| !step.planets.is_empty(), "take planets")
    }
}

/// The aliases documented above.
pub const ALIASES: &[&str] = &[
    "expand_borders",
    "subdue",
    "corner",
    "unify_colonies",
    "research_outposts",
    "brain_trust",
    "lost_outposts",
    "ancient_monuments",
    "conquer",
    "distant_lands",
    "push_boundaries",
    "faa",
    "mrm",
    "mp",
    "sai",
    "syc",
    "eh",
    "hrm",
];

#[cfg(test)]
mod tests {
    #[test]
    fn the_documented_aliases_are_the_ones_the_planner_matches() {
        let matched = super::super::tests::matched_aliases(&super::Territory);
        let documented: std::collections::BTreeSet<String> =
            super::ALIASES.iter().map(|a| (*a).to_owned()).collect();
        assert_eq!(matched, documented);
    }
}
