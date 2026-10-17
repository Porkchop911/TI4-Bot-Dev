//! Backward planning for the combat secrets, which the engine scores the moment the event happens
//! and for which it reports no progress (so they are not among the seat's goals unless
//! [`event_goals`] adds them): Darken the Skies (win a combat in a rival's home system), Spark a
//! Rebellion (win a combat against the points leader), Destroy Their Greatest Ship (destroy a
//! rival's flagship or war sun), Brave the Void (win a combat in an anomaly) and Demonstrate Your
//! Power (three or more non-fighter ships in the system when a space combat ends).
//!
//! Each is a one-step plan over the activations the framework already knows ([`PlanCtx::steps`]):
//! the system that would stage the event, with the probability that the fight is won and a factor
//! for the event itself (a flagship has to actually die, the combat has to be a space combat).
//! The cost is the tactic token of the activation. Secrets of the group whose event needs
//! something the planner cannot see (a promissory note held, a flagship that survives, space
//! cannon or bombardment) are left to the default logic.
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `dts` | Darken the Skies | secret | 1 | none (event) |
//! | `sar` | Spark a Rebellion | secret | 1 | none (event) |
//! | `dtgs` | Destroy Their Greatest Ship | secret | 1 | none (event) |
//! | `btv` | Brave the Void | secret | 1 | none (event) |
//! | `dyp` | Demonstrate Your Power | secret | 1 | none (event) |

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_model::id::{PlayerId, SystemId};

use super::{ObjectivePlanner, Plan, PlanCtx, Route, Step};
use crate::objectives::Goal;
use crate::{home_system, is_ship};

/// The secrets this group plans.
pub const IDS: &[&str] = &["dts", "sar", "dtgs", "btv", "dyp"];

/// The chance that the fight that stages the event also does what the secret asks of it, for a
/// flagship or war sun that has to die (sustain damage and the owner's choice of casualties).
pub const CAPITAL_KILL: f64 = 0.6;
/// The same for Demonstrate Your Power's three ships (some may be lost in the fight).
pub const THREE_SHIPS: f64 = 0.8;

/// The held event secrets of the group, as goals (one point each, not yet met), for the
/// framework to plan: the engine gives them no progress, so they are not in
/// [`crate::objectives::goals`].
#[must_use]
pub fn event_goals(seen: &SeatObservation<'_>) -> Vec<Goal> {
    event_goals_of(seen, IDS)
}

/// The held secrets among `ids` as event goals (see [`event_goals`]).
#[must_use]
pub fn event_goals_of(seen: &SeatObservation<'_>, ids: &[&str]) -> Vec<Goal> {
    seen.held_secrets()
        .iter()
        .map(ti4_model::id::SecretObjectiveId::as_str)
        .filter(|alias| ids.contains(alias))
        .map(|alias| Goal {
            alias: alias.to_owned(),
            secret: true,
            points: 1.0,
            bought: false,
            have: 0.0,
            threshold: 1.0,
            satisfied: false,
            family: "event".to_owned(),
        })
        .collect()
}

pub(crate) fn others_in(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Vec<PlayerId> {
    let board = seen.system(system);
    let mut out: Vec<PlayerId> = board
        .units
        .iter()
        .chain(board.planet_units.values().flatten())
        .filter(|u| &u.owner != me)
        .map(|u| u.owner.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The unique rival with the most points, if there is one and it is ahead of the seat.
fn leader(seen: &Observed<'_>, me: &PlayerId) -> Option<PlayerId> {
    let mine = seen.seat(me)?.victory_points;
    let mut rivals: Vec<(i32, PlayerId)> = seen
        .players()
        .into_iter()
        .filter(|p| *p != me)
        .filter_map(|p| Some((seen.seat(p)?.victory_points, p.clone())))
        .collect();
    rivals.sort_by(|a, b| b.0.cmp(&a.0));
    match rivals.as_slice() {
        [(top, who), rest @ ..] if *top > mine && rest.iter().all(|(v, _)| v < top) => {
            Some(who.clone())
        }
        _ => None,
    }
}

pub(crate) fn hostile_ships(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    seen.system(system)
        .units
        .iter()
        .any(|u| &u.owner != me && is_ship(seen, u))
}

/// The factor by which `step` stages the secret `alias`, if it does.
fn factor(seen: &Observed<'_>, me: &PlayerId, alias: &str, step: &Step) -> Option<f64> {
    let system = &step.system;
    let others = others_in(seen, me, system);
    if others.is_empty() {
        return None; // no fight there
    }
    match alias {
        "dts" => {
            let in_home = others
                .iter()
                .any(|p| home_system(seen, p).as_ref() == Some(system));
            in_home.then_some(1.0)
        }
        "sar" => {
            let lead = leader(seen, me)?;
            others.contains(&lead).then_some(1.0)
        }
        "dtgs" => seen
            .system(system)
            .units
            .iter()
            .any(|u| {
                &u.owner != me && {
                    let id = u.type_id.as_str();
                    id.contains("flagship") || id.contains("warsun") || id.contains("war_sun")
                }
            })
            .then_some(CAPITAL_KILL),
        "btv" => {
            let anomaly =
                ti4_content::galaxy::system(seen.content(), system.as_str(), seen.sources())
                    .is_some_and(|record| record.is_anomaly());
            (anomaly && hostile_ships(seen, me, system)).then_some(1.0)
        }
        "dyp" => {
            if !hostile_ships(seen, me, system) {
                return None;
            }
            let present = seen
                .system(system)
                .units
                .iter()
                .filter(|u| &u.owner == me && is_ship(seen, u) && !is_fighter(seen, u))
                .count();
            let coming = seen
                .movable_into(me, system)
                .iter()
                .filter(|m| is_ship(seen, &m.unit) && !is_fighter(seen, &m.unit))
                .count();
            (present + coming >= 3).then_some(THREE_SHIPS)
        }
        _ => None,
    }
}

fn is_fighter(seen: &Observed<'_>, unit: &ti4_model::units::Unit) -> bool {
    crate::unit_type(seen, unit.type_id.as_str()).is_some_and(|t| t.is_fighter())
}

/// The best staging among `options` (success of the step, factor of the event): the highest
/// product, the earlier on a tie.
#[must_use]
pub fn best_option(options: &[(f64, f64)]) -> Option<usize> {
    options
        .iter()
        .enumerate()
        .max_by(|(ia, a), (ib, b)| (a.0 * a.1).total_cmp(&(b.0 * b.1)).then(ib.cmp(ia)))
        .map(|(i, _)| i)
}

/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &["dts", "sar", "dtgs", "btv", "dyp"];

/// The combat group.
pub struct Combat;

impl ObjectivePlanner for Combat {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        stage_plan(obs, ctx, factor)
    }
}

/// The one-step plan of an event secret: the step (plain or prepared) whose system stages the
/// event with the best `success x factor`, where `factor` says whether and how well a step
/// stages the goal's alias.
pub fn stage_plan(
    obs: &SeatObservation<'_>,
    ctx: &PlanCtx,
    factor: impl Fn(&Observed<'_>, &PlayerId, &str, &Step) -> Option<f64>,
) -> Option<Plan> {
    let observed = obs.observed();
    let alias = ctx.goal.alias.as_str();
    // A prepared step (build or Warfare move first) also needs its tactic tokens.
    let staged: Vec<(usize, f64, f64)> = ctx
        .steps
        .iter()
        .enumerate()
        .filter(|(_, step)| ctx.tactic_tokens >= 1 + step.prep.as_ref().map_or(0, |p| p.tokens))
        .filter_map(|(i, step)| {
            factor(observed, &ctx.me, alias, step).map(|f| (i, step.success, f))
        })
        .collect();
    let pairs: Vec<(f64, f64)> = staged.iter().map(|&(_, s, f)| (s, f)).collect();
    let (i, success, f) = staged[best_option(&pairs)?];
    let route = Route {
        steps: vec![i],
        probability: success * f,
    };
    let system = &ctx.steps[i].system;
    Some(ctx.plan_of(&route, format!("{alias}: fight in {system}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
    use ti4_engine::fixtures::{a_system_where, game, hub_with_centre, put};
    use ti4_model::POK;
    use ti4_model::state::Phase;

    fn me() -> PlayerId {
        PlayerId::new("a")
    }

    /// Plans the group makes for the seat's goals, through the engine's private ask.
    struct Probe(Vec<Plan>);
    impl Decider for Probe {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            Ok(choice.options[0].clone())
        }
        fn choose_seeing(
            &mut self,
            choice: &Choice,
            seen: &SeatObservation<'_>,
        ) -> Result<ChoiceOption, IllegalChoice> {
            let mut ctx = PlanCtx::new(seen, &me());
            for goal in ctx.goals.clone() {
                ctx.goal = goal;
                if let Some(plan) = Combat.plan(seen, &ctx) {
                    self.0.push(plan);
                }
            }
            Ok(choice.options[0].clone())
        }
    }

    fn plans(state: &ti4_model::state::GameState, hub: &ti4_engine::fixtures::Hub) -> Vec<Plan> {
        let mut probe = Probe(Vec::new());
        let choice = Choice::new(me(), "x", vec![ChoiceOption::labelled("o", "o", "o")]);
        ti4_engine::choice::ask_private(
            &choice,
            state,
            ContentStore::embedded(),
            POK,
            Some(&hub.galaxy),
            &mut probe,
        )
        .expect("legal");
        probe.0
    }

    #[test]
    fn brave_the_void_plans_the_anomaly_with_a_beatable_fleet_in_it() {
        let nebula = a_system_where("nebula");
        let hub = hub_with_centre(&nebula);
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.phase = Phase::Action;
        let start = SystemId::new(hub.outer[0].clone());
        put(&mut state, &start, "dreadnought", &me(), 3);
        let centre = SystemId::new(hub.centre.clone());
        {
            let seat = state.player_mut(&me()).expect("seat");
            seat.secret_objectives.clear();
            seat.secret_objectives
                .push(ti4_model::id::SecretObjectiveId::new("btv"));
        }
        // Nobody there: no combat, no plan.
        assert!(plans(&state, &hub).is_empty());
        put(&mut state, &centre, "destroyer", &PlayerId::new("b"), 1);
        let found = plans(&state, &hub);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].steer.systems, vec![centre]);
        assert!(found[0].probability > 0.5 && found[0].secret);
        // A secret the group does not know is not planned.
        state.player_mut(&me()).expect("seat").secret_objectives =
            vec![ti4_model::id::SecretObjectiveId::new("pe")];
        assert!(plans(&state, &hub).is_empty());
    }

    #[test]
    fn the_likeliest_staging_wins_and_nothing_is_none() {
        assert_eq!(best_option(&[]), None);
        assert_eq!(best_option(&[(0.9, 0.6), (0.7, 1.0)]), Some(1));
        assert_eq!(best_option(&[(0.7, 1.0), (0.7, 1.0)]), Some(0));
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Combat),
            crate::backward::coverage::sorted(COVERS)
        );
    }
}
