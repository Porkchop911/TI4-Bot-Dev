//! Two-step plans for Mecatol Rex: the Mecatol plan built from the prepared steps of
//! [`super::prep`].
//!
//! When the one-step plan ([`super::mecatol`]) is not likely (the seat cannot reach Mecatol Rex
//! with enough ground forces, or cannot win the fight), a prepared step on Mecatol Rex's system
//! (a build, a Warfare gather, a movement or combat technology, whichever the knobs
//! `bw_prep_*` allow) becomes the Mecatol plan, worth the custodians' point (while the token
//! stands) and 0.6 of Imperial's primary point. The plan names the prep's card in `steer.cards`
//! so the draft picks it, and the prep's systems and builds steer activation and production.

//! # Objectives and preps
//!
//! Serves the virtual `mecatol` plan only (no card alias; see [`super::mecatol`]). All four prep
//! kinds apply ([`KINDS`]).

use ti4_engine::choice::SeatObservation;

use super::{Cost, Plan, PlanCtx, Steer};
use crate::tuning;

/// The movement option of a gather step: carriers (most capacity) first, then the strongest
/// ships; `done` once nothing useful is left to move.
#[must_use]
pub fn gather_move(
    choice: &ti4_engine::choice::Choice,
    seen: &ti4_engine::choice::Observed<'_>,
) -> Option<usize> {
    let done = choice
        .options
        .iter()
        .position(ti4_engine::choice::ChoiceOption::is_decline)?;
    let best = choice
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == "move")
        .filter_map(|(i, o)| {
            let kind = crate::unit_type(seen, crate::payload_str(o, "unit")?)?;
            (kind.is_ship() && !kind.is_fighter()).then_some((
                i,
                kind.capacity(),
                kind.combat_dice(),
            ))
        })
        .max_by_key(|&(i, cap, dice)| (cap, dice, std::cmp::Reverse(i)))
        .map(|(i, _, _)| i);
    Some(best.unwrap_or(done))
}

/// The load option of a gather step: ground forces while any are offered.
#[must_use]
pub fn gather_load(
    choice: &ti4_engine::choice::Choice,
    seen: &ti4_engine::choice::Observed<'_>,
) -> Option<usize> {
    let done = choice
        .options
        .iter()
        .position(ti4_engine::choice::ChoiceOption::is_decline)?;
    let ground = choice.options.iter().position(|o| {
        o.kind == "load"
            && crate::payload_str(o, "unit")
                .and_then(|id| crate::unit_type(seen, id))
                .is_some_and(|t| t.is_ground_force())
    });
    Some(ground.unwrap_or(done))
}

/// The Mecatol plan from the best prepared step on Mecatol Rex's system, if the one-step plan is
/// not likely.
#[must_use]
pub fn plan(_seen: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
    let system = ctx.mecatol.system.clone()?;
    if ctx.mecatol.held {
        return None;
    }
    let one_step = ctx
        .steps
        .iter()
        .filter(|s| s.prep.is_none() && s.system == system && !s.planets.is_empty())
        .map(|s| s.success)
        .fold(0.0, f64::max);
    if one_step >= tuning::bw_likely() {
        return None;
    }
    let step = ctx
        .steps
        .iter()
        .filter(|s| s.system == system && s.prep.is_some())
        .filter(|s| s.planets.iter().any(|p| p.as_str() == crate::MECATOL))
        .filter(|s| {
            ctx.affordable(&[ctx
                .steps
                .iter()
                .position(|t| std::ptr::eq(t, *s))
                .unwrap_or(0)])
        })
        .max_by(|a, b| a.success.total_cmp(&b.success))?;
    let prep = step.prep.as_ref()?;
    let mut steer = Steer {
        systems: prep.systems.clone(),
        builds: prep.builds.clone(),
        cards: prep.cards.clone(),
        play: prep.play.clone(),
        gather: prep.gather.clone(),
        techs: prep.techs.clone(),
        ..Steer::default()
    };
    steer.systems.push(system);
    steer.cards.push("Imperial".to_owned());
    let tokens = prep.tokens + prep.strategic + 1;
    Some(Plan {
        objective: "mecatol".to_owned(),
        points: tuning::bw_mecatol_weight()
            * (if ctx.mecatol.custodians_removed {
                0.0
            } else {
                1.0
            } + 0.6),
        probability: step.success,
        cost: Cost {
            tokens,
            resources: prep.resources,
            activations: u32::try_from(prep.tokens + 1).unwrap_or(1),
        },
        steer,
        reason: format!("{}: then take Mecatol Rex", prep.label),
        payment: false,
        secret: false,
    })
}

/// The prep kinds a Mecatol two-step plan can use, with their knobs.
pub const KINDS: &[(&str, &str)] = &[
    ("build", "bw_prep_build"),
    ("warfare", "bw_prep_warfare"),
    ("tech_range", "bw_prep_tech_range"),
    ("tech_strength", "bw_prep_tech_strength"),
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_documented_prep_kind_has_its_knob() {
        for (_, knob) in super::KINDS {
            assert!(crate::tuning::KEYS.iter().any(|(k, _)| k == knob), "{knob}");
        }
    }
}
