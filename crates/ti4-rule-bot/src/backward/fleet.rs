//! Fleet and ship placement goals: ships or units in N systems (planetless, on the rim, notable,
//! next to Mecatol Rex, an anomaly or a rival's home, with a rival's dock, in the nexus).
//!
//! Backward: the goal counts systems holding a ship of the seat. The cheapest route is the fewest
//! activations (at most three) that put a ship in a system where the seat has none, judged by the
//! engine's requirement functions on the imagined systems ([`PlanCtx::route`]). A step whose
//! ship would have to leave a system with only that ship counts less ([`super::Step::spare_ship`]).
//! Prepared steps ([`super::prep`]) apply to every alias below: a build adds a ship to send, a
//! Warfare gather and the technology preps bring ships into range.
//!
//! Not planned here (production objectives, left to [`crate::planner::Bias`]): `raise_fleet`,
//! `command_armada` (`fleet_in_one_system`), `engineer_marvel` (`capital_ship_systems`),
//! `supremacy` (`capital_ships_rival_or_mecatol`).
//!
//! # Objectives (alias, name, points; family token)
//!
//! Public:
//!
//! | alias | name | VP | family |
//! |---|---|---|---|
//! | `deep_space` | Explore Deep Space | 1 | `planetless_systems` |
//! | `vast_territories` | Patrol Vast Territories | 2 | `planetless_systems` |
//! | `outer_rim` | Populate the Outer Rim | 1 | `on_the_rim` |
//! | `control_borderlands` | Control the Borderlands | 2 | `on_the_rim` |
//! | `make_history` | Make History | 1 | `notable_systems` |
//! | `become_legend` | Become a Legend | 2 | `notable_systems` |
//! | `intimidate` | Intimidate Council | 1 | `ships_adjacent_to_mecatol` |
//!
//! Secret (1 VP each):
//!
//! | alias | name | family |
//! |---|---|---|
//! | `ctr` | Control the Region | `ships_in_systems` |
//! | `lsc` | Learn the Secrets of the Cosmos | `ship_systems_beside_anomaly` |
//! | `te` | Threaten Enemies | `ship_systems_beside_rival_home` |
//! | `csl` | Cut Supply Lines | `rival_dock_systems_with_ships` |
//! | `ose` | Occupy the Seat of the Empire | `mecatol_ships_while_controlling` |
//! | `fc` | Foster Cohesion | `neighbours` |
//! | `dfat` | Defy Space and Time | `units_in_nexus` |
//! | `btgk` | Become the Gatekeeper | `wormhole_kinds` |

use ti4_engine::choice::SeatObservation;

use super::{ObjectivePlanner, Plan, PlanCtx};

/// The fleet group.
pub struct Fleet;

impl ObjectivePlanner for Fleet {
    fn objective_ids(&self) -> &[&str] {
        &[
            "ships_in_systems",
            "planetless_systems",
            "on_the_rim",
            "notable_systems",
            "ships_adjacent_to_mecatol",
            "ship_systems_beside_anomaly",
            "ship_systems_beside_rival_home",
            "rival_dock_systems_with_ships",
            "mecatol_ships_while_controlling",
            "neighbours",
            "units_in_nexus",
            "wormhole_kinds",
        ]
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        ctx.route_plan(obs, 3, |step| step.ship, "put a ship in")
    }
}

/// The aliases documented above.
pub const ALIASES: &[&str] = &[
    "deep_space",
    "vast_territories",
    "outer_rim",
    "control_borderlands",
    "make_history",
    "become_legend",
    "intimidate",
    "ctr",
    "lsc",
    "te",
    "csl",
    "ose",
    "fc",
    "dfat",
    "btgk",
];

#[cfg(test)]
mod tests {
    #[test]
    fn the_documented_aliases_are_the_ones_the_planner_matches() {
        let matched = super::super::tests::matched_aliases(&super::Fleet);
        let documented: std::collections::BTreeSet<String> =
            super::ALIASES.iter().map(|a| (*a).to_owned()).collect();
        assert_eq!(matched, documented);
    }
}
