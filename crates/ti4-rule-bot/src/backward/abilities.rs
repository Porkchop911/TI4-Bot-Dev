//! Backward planning for the combat secrets that need a unit ability or a particular fleet at
//! the fight: Unveil Flagship (win a space combat in a system with your flagship, which must
//! survive), Betray a Friend (win a combat against a player whose promissory note you hold),
//! Fight with Precision (anti-fighter barrage destroys the last of a player's fighters) and Make
//! an Example of Their World (bombardment destroys the last ground forces on a planet).
//!
//! Like [`super::combat`] each is a one-step plan over [`PlanCtx::steps`] (plain or prepared):
//! the system whose activation stages the event, the fight's success, and a factor for the event
//! itself: the flagship comes along and survives (0.75), the player's note is held (1), the
//! barrage or bombardment dice kill every fighter or defender (a binomial tail over the dice of
//! the units that are there or can come, no planetary shield for bombardment).
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `uf` | Unveil Flagship | secret | 1 | none (event) |
//! | `baf` | Betray a Friend | secret | 1 | none (event) |
//! | `fwp` | Fight with Precision | secret | 1 | none (event) |
//! | `mew` | Make an Example of Their World | secret | 1 | none (event) |

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::units::Unit;

use super::combat::{hostile_ships, others_in, stage_plan};
use super::{ObjectivePlanner, Plan, PlanCtx, Step};
use crate::objectives::Goal;
use crate::{is_ship, unit_type};

/// The secrets this group plans.
pub const IDS: &[&str] = &["uf", "baf", "fwp", "mew"];
/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &["uf", "baf", "fwp", "mew"];

/// The flagship comes along and is still there when the combat ends.
pub const FLAGSHIP_SURVIVES: f64 = 0.75;

/// The held event secrets of the group, as goals.
#[must_use]
pub fn event_goals(seen: &SeatObservation<'_>) -> Vec<Goal> {
    super::combat::event_goals_of(seen, IDS)
}

/// The chance that at least `need` of `dice` dice hit when each hits with probability `p`.
#[must_use]
pub fn at_least(dice: usize, p: f64, need: usize) -> f64 {
    if need == 0 {
        return 1.0;
    }
    if need > dice {
        return 0.0;
    }
    // P(X >= need) = 1 - sum_{k<need} C(n,k) p^k (1-p)^(n-k)
    let mut below = 0.0;
    let mut coeff = 1.0_f64;
    for k in 0..need {
        if k > 0 {
            #[expect(clippy::cast_precision_loss, reason = "a handful of dice")]
            {
                coeff *= (dice - k + 1) as f64 / k as f64;
            }
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_possible_wrap,
            reason = "a handful of dice"
        )]
        {
            below += coeff * p.powi(k as i32) * (1.0 - p).powi((dice - k) as i32);
        }
    }
    (1.0 - below).clamp(0.0, 1.0)
}

/// Chance that a die with "hits on `k`" hits.
#[must_use]
pub fn die(hits_on: i64) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "single digits")]
    let p = (11 - hits_on).clamp(0, 10) as f64 / 10.0;
    p
}

/// The seat's units in `system` and the ones that could move there.
fn own_units(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Vec<Unit> {
    let mut out: Vec<Unit> = seen
        .system(system)
        .units
        .iter()
        .filter(|u| &u.owner == me)
        .cloned()
        .collect();
    out.extend(seen.movable_into(me, system).into_iter().map(|m| m.unit));
    out
}

fn flagship_present(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> bool {
    own_units(seen, me, system)
        .iter()
        .any(|u| u.type_id.as_str().contains("flagship"))
}

/// Owners of the rival promissory notes the seat holds faceup.
fn note_issuers(seen: &Observed<'_>, me: &PlayerId) -> Vec<PlayerId> {
    seen.promissory_notes()
        .into_iter()
        .filter(|(_, holder)| holder == me)
        .filter_map(|(id, _)| id.rsplit(':').next().map(PlayerId::new))
        .filter(|owner| owner != me)
        .collect()
}

fn fwp_factor(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Option<f64> {
    let hostile: Vec<Unit> = seen
        .system(system)
        .units
        .iter()
        .filter(|u| &u.owner != me && is_ship(seen, u))
        .cloned()
        .collect();
    let fighters = hostile
        .iter()
        .filter(|u| unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.is_fighter()))
        .count();
    // Only fighters may stand there: a barrage kills nothing else.
    if fighters == 0 || fighters != hostile.len() {
        return None;
    }
    let mut chance = 1.0;
    let mut dice = 0usize;
    let mut best = 0.0_f64;
    for unit in own_units(seen, me, system) {
        let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
            continue;
        };
        if let Some(hits_on) = kind.afb_hits_on() {
            dice += usize::try_from(kind.afb_dice()).unwrap_or(0);
            best = best.max(die(hits_on));
        }
    }
    if dice == 0 {
        return None;
    }
    chance *= at_least(dice, best, fighters);
    (chance > 0.0).then_some(chance)
}

fn mew_factor(seen: &Observed<'_>, me: &PlayerId, system: &SystemId) -> Option<f64> {
    let board = seen.system(system);
    let mut dice = 0usize;
    let mut hit = 0.0_f64;
    for unit in own_units(seen, me, system) {
        let Some(kind) = unit_type(seen, unit.type_id.as_str()) else {
            continue;
        };
        if let Some(hits_on) = kind.bombard_hits_on() {
            dice += usize::try_from(kind.bombard_dice()).unwrap_or(0);
            hit = hit.max(die(hits_on));
        }
    }
    if dice == 0 {
        return None;
    }
    // The planet with the fewest ground forces of a rival and no planetary shield.
    board
        .planet_units
        .values()
        .filter_map(|units| {
            let rival: Vec<&Unit> = units.iter().filter(|u| &u.owner != me).collect();
            let ground = rival
                .iter()
                .filter(|u| {
                    unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.is_ground_force())
                })
                .count();
            let shielded = rival
                .iter()
                .any(|u| unit_type(seen, u.type_id.as_str()).is_some_and(|t| t.planetary_shield()));
            (ground > 0 && !shielded).then(|| at_least(dice, hit, ground))
        })
        .max_by(f64::total_cmp)
        .filter(|c| *c > 0.0)
}

/// The factor by which `step` stages the secret `alias`, if it does.
fn factor(seen: &Observed<'_>, me: &PlayerId, alias: &str, step: &Step) -> Option<f64> {
    let system = &step.system;
    match alias {
        "uf" => (hostile_ships(seen, me, system) && flagship_present(seen, me, system))
            .then_some(FLAGSHIP_SURVIVES),
        "baf" => {
            let issuers = note_issuers(seen, me);
            others_in(seen, me, system)
                .iter()
                .any(|p| issuers.contains(p))
                .then_some(1.0)
        }
        "fwp" => fwp_factor(seen, me, system),
        "mew" => mew_factor(seen, me, system),
        _ => None,
    }
}

/// The abilities group.
pub struct Abilities;

impl ObjectivePlanner for Abilities {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        stage_plan(obs, ctx, factor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_tails() {
        assert!((at_least(3, 0.5, 0) - 1.0).abs() < 1e-12);
        assert!(at_least(2, 0.5, 3).abs() < 1e-12);
        assert!((at_least(1, 0.6, 1) - 0.6).abs() < 1e-12);
        // two dice at 0.5, both: 0.25; at least one: 0.75
        assert!((at_least(2, 0.5, 2) - 0.25).abs() < 1e-12);
        assert!((at_least(2, 0.5, 1) - 0.75).abs() < 1e-12);
        assert!((die(5) - 0.6).abs() < 1e-12 && (die(9) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Abilities),
            crate::backward::coverage::sorted(COVERS)
        );
    }

    use ti4_engine::fixtures::{game, hub_with_centre, plain_systems, put};
    use ti4_model::id::SecretObjectiveId;
    use ti4_model::state::Phase;

    #[test]
    fn fwp_plans_a_barrage_against_a_lone_fighter_screen() {
        let empty = plain_systems(200)
            .into_iter()
            .find(|id| {
                ti4_content::galaxy::planets_in(
                    ti4_content::ContentStore::embedded(),
                    id,
                    ti4_model::POK,
                )
                .is_empty()
            })
            .expect("an empty system");
        let hub = hub_with_centre(&empty);
        let mut knobs = crate::tuning::Tuning::default();
        knobs.bw_combat_abilities = true;
        let plans_of = |s: &_, g: Option<&_>| {
            crate::tuning::with(knobs, || {
                crate::backward::coverage::plans_of(&Abilities, s, g)
            })
        };
        let mut state = game(&["a", "b"]);
        state.round = 3;
        state.phase = Phase::Action;
        let start = SystemId::new(hub.outer[0].clone());
        put(&mut state, &start, "destroyer", &PlayerId::new("a"), 3);
        {
            let seat = state.player_mut(&PlayerId::new("a")).expect("seat");
            seat.secret_objectives = vec![SecretObjectiveId::new("fwp")];
            seat.tactic_tokens = 3;
        }
        let centre = SystemId::new(hub.centre.clone());
        // Nothing to barrage: no plan.
        assert!(plans_of(&state, Some(&hub.galaxy)).is_empty());
        put(&mut state, &centre, "fighter", &PlayerId::new("b"), 2);
        let found = plans_of(&state, Some(&hub.galaxy));
        assert_eq!(found.len(), 1);
        assert!(found[0].probability > 0.3 && found[0].steer.systems.contains(&centre));
        // A cruiser beside the fighters: the barrage cannot be the last of the non-fighters.
        put(&mut state, &centre, "cruiser", &PlayerId::new("b"), 1);
        assert!(plans_of(&state, Some(&hub.galaxy)).is_empty());
    }
}
