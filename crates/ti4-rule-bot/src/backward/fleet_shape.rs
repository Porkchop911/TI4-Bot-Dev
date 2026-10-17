//! Fleet shape and ground piles: objectives about one big fleet, a flagship or war sun in the
//! right place, and many ground forces on one planet.
//!
//! Backward from the requirement:
//!
//! - **Fleet in one system** (`raise_fleet`, `command_armada`): the system S where the most
//!   non-fighter ships can be together after one activation of S (the ships already there plus
//!   those that can reach it, [`ti4_engine::choice::Observed::movable_into`]); the movement hook
//!   ([`super::Steer::gather`]) then moves all of them. With [`tuning::bw_prep_warfare`] a gather
//!   step beside S widens the reach; with [`tuning::bw_prep_build`] the home dock builds the
//!   cheapest ships that are missing (fleet supply and production slots permitting).
//! - **Flagship or war sun on the board** (`engineer_marvel`): build the flagship at home
//!   ([`tuning::bw_prep_build`] sources: dock activation, Construction, Warfare).
//! - **Flagship or war sun in a rival's home or Mecatol Rex** (`supremacy`): activate that
//!   system with the capital ship among the ships that can reach it, or build it and then act.
//! - **Ground forces on one planet** (`otf`): the ground forces the carriers bring to a planet
//!   to take; with [`tuning::bw_prep_warfare`] after a gather step. The commit hook piles them
//!   on that planet.
//!
//! # Objectives (alias, name, points; family token)
//!
//! Public:
//!
//! | alias | name | VP | family |
//! |---|---|---|---|
//! | `raise_fleet` | Raise a Fleet | 1 | `fleet_in_one_system` |
//! | `command_armada` | Command an Armada | 2 | `fleet_in_one_system` |
//! | `engineer_marvel` | Engineer a Marvel | 1 | `capital_ship_systems` |
//! | `supremacy` | Achieve Supremacy | 2 | `capital_ships_rival_or_mecatol` |
//!
//! Secret (1 VP each):
//!
//! | alias | name | family |
//! |---|---|---|
//! | `otf` | Occupy the Fringe | `ground_forces_on_one_planet` |
//!
//! Prep kinds: build for the fleet and the flagship goals, Warfare gather for the fleet and the
//! ground pile; the technology preps do not apply (no fight or reach to improve beyond the
//! activation itself).

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_model::id::{PlayerId, SystemId};

use super::prep::{self, PrepInfo, PrepKind, SysCase};
use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer};
use crate::tuning;

/// The aliases documented above.
pub const ALIASES: &[&str] = &[
    "raise_fleet",
    "command_armada",
    "engineer_marvel",
    "supremacy",
    "otf",
];

/// The fleet-shape group.
pub struct FleetShape;

impl ObjectivePlanner for FleetShape {
    fn objective_ids(&self) -> &[&str] {
        &[
            "fleet_in_one_system",
            "capital_ship_systems",
            "capital_ships_rival_or_mecatol",
            "ground_forces_on_one_planet",
        ]
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        let seen = obs.observed();
        let threshold = usize::try_from(ctx.goal.threshold.max(0.0) as i64).unwrap_or(0);
        match ctx.goal.family.as_str() {
            "fleet_in_one_system" => fleet_in_one_system(seen, ctx, threshold),
            "capital_ship_systems" => engineer(seen, ctx),
            "capital_ships_rival_or_mecatol" => supremacy(seen, ctx),
            "ground_forces_on_one_planet" => ground_pile(seen, ctx, threshold),
            _ => None,
        }
    }
}

fn plan_of(ctx: &PlanCtx, probability: f64, cost: Cost, steer: Steer, reason: String) -> Plan {
    Plan {
        objective: ctx.goal.alias.clone(),
        points: ctx.goal.points,
        probability,
        cost,
        steer,
        reason,
        payment: false,
        secret: ctx.goal.secret,
    }
}

fn non_fighter_ships(
    seen: &Observed<'_>,
    units: &[ti4_model::units::Unit],
    me: &PlayerId,
) -> usize {
    units
        .iter()
        .filter(|u| &u.owner == me)
        .filter(|u| {
            crate::unit_type(seen, u.type_id.as_str())
                .is_some_and(|t| t.is_ship() && !t.is_fighter())
        })
        .count()
}

/// Non-fighter ships in `system` once it is activated: those there and those that can reach it.
fn fleet_after(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> usize {
    let here = non_fighter_ships(seen, &seen.system(system).units, me);
    let coming = seen
        .movable_into(me, system)
        .iter()
        .filter(|m| {
            crate::unit_type(seen, m.unit.type_id.as_str())
                .is_some_and(|t| t.is_ship() && !t.is_fighter())
        })
        .count();
    here + coming
}

fn cheapest_ship<'a>(seen: &Observed<'a>) -> Option<ti4_content::units::UnitType<'a>> {
    ["destroyer", "cruiser", "carrier"]
        .into_iter()
        .filter_map(|id| crate::unit_type(seen, id))
        .min_by(|a, b| a.cost().total_cmp(&b.cost()))
}

fn fleet_in_one_system(seen: &Observed<'_>, ctx: &PlanCtx, n: usize) -> Option<Plan> {
    let galaxy = seen.galaxy()?;
    let me = &ctx.me;
    let tokened = seen.systems_with_token(me);
    let mut best: Option<(f64, SystemId, usize)> = None;
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        if tokened.contains(&system) || ctx.tactic_tokens < 1 {
            continue;
        }
        let count = fleet_after(seen, me, &system);
        if count < n {
            continue;
        }
        let case = SysCase::new(seen, me, &system);
        let fight = if case.hostile {
            let m = case.matchup_of(seen, &case.own);
            if m < case.margin {
                continue;
            }
            1.0 / (1.0 + (-2.0 * m).exp())
        } else {
            1.0
        };
        let p = 0.9 * fight;
        if best.as_ref().is_none_or(|b| p > b.0) {
            best = Some((p, system, count));
        }
    }
    if let Some((p, system, count)) = best {
        return Some(plan_of(
            ctx,
            p,
            Cost {
                tokens: 1,
                resources: 0,
                activations: 1,
            },
            Steer {
                systems: vec![system.clone()],
                gather: vec![system.clone()],
                ..Steer::default()
            },
            format!("gather {count} ships in {system}"),
        ));
    }
    // Build the missing ships at home.
    if !tuning::bw_prep_build() {
        return None;
    }
    let home = crate::home_system(seen, me)?;
    let ship = cheapest_ship(seen)?;
    let slots = prep::home_slots(seen, me, &home);
    let (source, tokens, strategic, odds) = prep::sources(seen, me, ctx)
        .into_iter()
        .max_by(|a, b| a.3.total_cmp(&b.3).then(b.1.cmp(&a.1)))?;
    let dock = matches!(
        source,
        prep::Source::Dock | prep::Source::DockWithWarfare | prep::Source::WarfarePrimary
    );
    let have = non_fighter_ships(seen, &seen.system(&home).units, me) + if dock { 0 } else { 0 };
    let coming = if dock {
        fleet_after(seen, me, &home) - have
    } else {
        0
    };
    let missing = n.saturating_sub(have + coming);
    let on_board: usize = seen
        .board()
        .values()
        .map(|b| non_fighter_ships(seen, &b.units, me))
        .sum();
    let room = usize::try_from(seen.seat(me)?.fleet_tokens)
        .unwrap_or(0)
        .saturating_sub(on_board);
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let cost = missing as f64 * ship.cost();
    if missing == 0
        || i64::try_from(missing).ok()? > slots
        || missing > room
        || cost > ctx.resources as f64
    {
        return None;
    }
    let card = match source {
        prep::Source::Construction => Some("Construction"),
        prep::Source::WarfarePrimary | prep::Source::DockWithWarfare => Some("Warfare"),
        _ => None,
    };
    let mut systems = Vec::new();
    if dock {
        systems.push(home.clone());
    }
    Some(plan_of(
        ctx,
        odds * 0.9,
        Cost {
            tokens: tokens + strategic,
            resources: cost.ceil() as i64,
            activations: u32::try_from(tokens).unwrap_or(0),
        },
        Steer {
            systems,
            builds: vec![ship.id().to_owned()],
            cards: card.map(str::to_owned).into_iter().collect(),
            play: card.map(str::to_owned).into_iter().collect(),
            gather: if dock { vec![home] } else { Vec::new() },
            ..Steer::default()
        },
        format!("build + fleet: {missing} x {} at home", ship.id()),
    ))
}

/// The seat's flagship type id.
fn flagship_id(seen: &Observed<'_>, me: &PlayerId) -> Option<String> {
    let faction = crate::faction(seen, me);
    ti4_content::units::catalogue(seen.content(), seen.sources())
        .values()
        .find(|t| t.base_type() == "flagship" && t.faction() == Some(faction.as_str()))
        .map(|t| t.id().to_owned())
}

/// Build a flagship at home: (units, cost, slots ok, prep info).
fn build_flagship(seen: &Observed<'_>, ctx: &PlanCtx) -> Option<(String, f64, PrepInfo, f64)> {
    if !tuning::bw_prep_build() {
        return None;
    }
    let me = &ctx.me;
    let id = flagship_id(seen, me)?;
    let kind = crate::unit_type(seen, &id)?;
    let home = crate::home_system(seen, me)?;
    let on_board: usize = seen
        .board()
        .values()
        .map(|b| non_fighter_ships(seen, &b.units, me))
        .sum();
    if prep::home_slots(seen, me, &home) < 1
        || kind.cost() > ctx.resources as f64
        || on_board >= usize::try_from(seen.seat(me)?.fleet_tokens).unwrap_or(0)
    {
        return None;
    }
    let (source, tokens, strategic, odds) = prep::sources(seen, me, ctx)
        .into_iter()
        .max_by(|a, b| a.3.total_cmp(&b.3).then(b.1.cmp(&a.1)))?;
    let dock = matches!(
        source,
        prep::Source::Dock | prep::Source::DockWithWarfare | prep::Source::WarfarePrimary
    );
    let card = match source {
        prep::Source::Construction => Some("Construction".to_owned()),
        prep::Source::WarfarePrimary | prep::Source::DockWithWarfare => Some("Warfare".to_owned()),
        _ => None,
    };
    Some((
        id.clone(),
        kind.cost(),
        PrepInfo {
            kind: PrepKind::Build,
            label: format!("build + flagship ({id})"),
            tokens,
            strategic,
            resources: kind.cost().ceil() as i64,
            systems: if dock { vec![home] } else { Vec::new() },
            gather: Vec::new(),
            builds: vec![id],
            cards: card.clone().into_iter().collect(),
            play: card.into_iter().collect(),
            techs: Vec::new(),
            odds,
        },
        odds,
    ))
}

fn engineer(seen: &Observed<'_>, ctx: &PlanCtx) -> Option<Plan> {
    let (_, _, prep, odds) = build_flagship(seen, ctx)?;
    Some(plan_of(
        ctx,
        odds * 0.9,
        Cost {
            tokens: prep.tokens + prep.strategic,
            resources: prep.resources,
            activations: u32::try_from(prep.tokens).unwrap_or(0),
        },
        Steer {
            systems: prep.systems,
            builds: prep.builds,
            cards: prep.cards,
            play: prep.play,
            ..Steer::default()
        },
        prep.label,
    ))
}

fn supremacy(seen: &Observed<'_>, ctx: &PlanCtx) -> Option<Plan> {
    let galaxy = seen.galaxy()?;
    let me = &ctx.me;
    let tokened = seen.systems_with_token(me);
    let mut targets: Vec<SystemId> = Vec::new();
    if let Some(mr) = &ctx.mecatol.system {
        targets.push(mr.clone());
    }
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        let rival_home = ti4_content::galaxy::planets_in(seen.content(), id, seen.sources())
            .iter()
            .any(|p| {
                p.homeworld_of()
                    .is_some_and(|f| f != crate::faction(seen, me))
            });
        if rival_home {
            targets.push(system);
        }
    }
    let is_capital = |t: &str| {
        crate::unit_type(seen, t).is_some_and(|k| matches!(k.base_type(), "flagship" | "warsun"))
    };
    // Plain: a capital ship among those that can reach the target.
    let mut best: Option<(f64, SystemId)> = None;
    for target in &targets {
        if tokened.contains(target) || ctx.tactic_tokens < 1 {
            continue;
        }
        let movers = seen
            .movable_into(me, target)
            .iter()
            .any(|m| is_capital(m.unit.type_id.as_str()));
        if !movers {
            continue;
        }
        let case = SysCase::new(seen, me, target);
        let m = case.matchup_of(seen, &case.own);
        if m < case.margin {
            continue;
        }
        let p = 0.9
            * if case.hostile {
                1.0 / (1.0 + (-2.0 * m).exp())
            } else {
                1.0
            };
        if best.as_ref().is_none_or(|b| p > b.0) {
            best = Some((p, target.clone()));
        }
    }
    if let Some((p, target)) = best {
        return Some(plan_of(
            ctx,
            p,
            Cost {
                tokens: 1,
                resources: 0,
                activations: 1,
            },
            Steer {
                systems: vec![target.clone()],
                ..Steer::default()
            },
            format!("move the capital ship into {target}"),
        ));
    }
    // Build it, then act.
    let (id, _, prep, odds) = build_flagship(seen, ctx)?;
    let home = crate::home_system(seen, me)?;
    let kind = crate::unit_type(seen, &id)?;
    let range = i32::try_from(kind.move_value()).unwrap_or(1);
    let (target, success) = targets
        .iter()
        .filter(|t| !tokened.contains(*t) && (*t == &home || seen.can_reach(me, &home, t, range)))
        .filter_map(|t| {
            let case = SysCase::new(seen, me, t);
            let own = prep::with_added(&case.own, &id, 1);
            case.outcome(seen, case.ground, &own, true)
                .map(|o| (t.clone(), o.success.max(0.5)))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let tokens = prep.tokens + prep.strategic + 1;
    let mut systems = prep.systems.clone();
    systems.push(target.clone());
    if prep.tokens + 1 > ctx.tactic_tokens {
        return None;
    }
    Some(plan_of(
        ctx,
        odds * success * 0.9,
        Cost {
            tokens,
            resources: prep.resources,
            activations: u32::try_from(prep.tokens + 1).unwrap_or(1),
        },
        Steer {
            systems,
            builds: prep.builds,
            cards: prep.cards,
            play: prep.play,
            ..Steer::default()
        },
        format!("build + act: flagship {id}, then {target}"),
    ))
}

fn ground_pile(seen: &Observed<'_>, ctx: &PlanCtx, n: usize) -> Option<Plan> {
    let galaxy = seen.galaxy()?;
    let me = &ctx.me;
    let tokened = seen.systems_with_token(me);
    let mut best: Option<(f64, SystemId, Option<SystemId>, usize)> = None;
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        if tokened.contains(&system) || ctx.tactic_tokens < 1 {
            continue;
        }
        let case = SysCase::new(seen, me, &system);
        if case.targets.is_empty()
            || (case.hostile && case.matchup_of(seen, &case.own) < case.margin)
        {
            continue;
        }
        let defenders: usize = case
            .targets
            .iter()
            .map(|(_, _, need)| need.saturating_sub(1))
            .min()
            .unwrap_or(0);
        if case.ground >= n + defenders && case.ships {
            let p = 0.8;
            if best.as_ref().is_none_or(|b| p > b.0) {
                best = Some((p, system.clone(), None, case.ground));
            }
            continue;
        }
        if tuning::bw_prep_warfare() && ctx.tactic_tokens >= 2 {
            for x in galaxy.adjacent(id) {
                let x = SystemId::new(x);
                if tokened.contains(&x) || crate::has_hostile_ships(seen, me, &x) {
                    continue;
                }
                let (ground, own) = prep::gathered(seen, me, &case, &x);
                if ground >= n + defenders && !own.is_empty() {
                    let p = 0.8 * 0.85 * 0.9;
                    if best.as_ref().is_none_or(|b| p > b.0) {
                        best = Some((p, system.clone(), Some(x), ground));
                    }
                }
            }
        }
    }
    let (p, system, gather, ground) = best?;
    let mut systems = Vec::new();
    let mut gather_systems = Vec::new();
    let activations = if let Some(x) = gather {
        systems.push(x.clone());
        gather_systems.push(x);
        2
    } else {
        1
    };
    systems.push(system.clone());
    Some(plan_of(
        ctx,
        p,
        Cost {
            tokens: activations,
            resources: 0,
            activations: u32::try_from(activations).unwrap_or(1),
        },
        Steer {
            systems,
            gather: gather_systems,
            pile: vec![system.clone()],
            ..Steer::default()
        },
        format!("pile {ground} ground forces on a planet of {system}"),
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_documented_aliases_are_the_ones_the_planner_matches() {
        let matched = super::super::tests::matched_aliases(&super::FleetShape);
        let documented: std::collections::BTreeSet<String> =
            super::ALIASES.iter().map(|a| (*a).to_owned()).collect();
        assert_eq!(matched, documented);
    }
}
