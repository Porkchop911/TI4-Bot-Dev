//! Magen Defense Grid (`md`, the Codex 4 printing in the `techs_pok_c4` deck), both effects, from
//! rules version `RULES_TECH_EFFECTS`:
//!
//! 1. "When any player activates a system that contains 1 or more of your structures, place 1
//!    infantry from your reinforcements with each of those structures." Mandatory, so no question:
//!    one infantry on the planet of each structure the owner has there (a structure in a space area
//!    has no planet to hold a ground force and gets none). Hangs on `SYSTEM_ACTIVATED` ("when"),
//!    so the infantry are there before the activator's reactions and movement.
//! 2. "At the start of ground combat on a planet that contains 1 or more of your structures,
//!    produce 1 hit and assign it to 1 of your opponent's ground forces." Hangs on
//!    `GROUND_COMBAT_STARTED` (live invasions only; the synchronous test entry points emit
//!    nothing). The card's owner assigns the hit, so when the opponent has more than one kind of
//!    ground force there I ask the owner which (`magen_defense_hit`); the target's owner then
//!    decides on SUSTAIN DAMAGE as for any assigned hit (`assign_selected_ground_hit_in_timing`).
//!
//! The superseded printings `md_base` and `md_c1` are not in the deck and not implemented.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::action_cards::place_units_counted;
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingError};

/// The technology.
const TECH: &str = "md";

/// Whether the player's Magen Defense Grid is live in this game.
fn active(state: &GameState, player: &PlayerId) -> bool {
    state.rules_version >= ti4_model::state::RULES_TECH_EFFECTS
        && crate::technology::owns_technology(state, player, TECH)
}

/// The two timing abilities of one seat, registered for every seat (the card can be gained
/// mid-game); the conditions limit them to the holder.
pub(crate) fn abilities(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    vec![activation(owner_name, seat), combat_start(owner_name, seat)]
}

/// The planets of `system` on which `player` has structures, one entry per structure.
fn structure_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<PlanetId> {
    let types = catalogue(content, sources);
    let Some(board) = state.board.get(system) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (planet, units) in &board.planet_units {
        for unit in units {
            if &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_structure)
            {
                found.push(planet.clone());
            }
        }
    }
    found
}

/// Effect 1: infantry beside the structures when any player activates the system.
fn activation(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:md:SYSTEM_ACTIVATED:when"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            for planet in structure_planets(
                context.state,
                context.content,
                context.sources,
                &owner,
                &system,
            ) {
                place_units_counted(context, &owner, &system, Some(&planet), "infantry", 1);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        active(context.state, &condition_owner)
            && event.text("system").is_some_and(|system| {
                !structure_planets(
                    context.state,
                    context.content,
                    context.sources,
                    &condition_owner,
                    &SystemId::new(system),
                )
                .is_empty()
            })
    }))
}

/// The side of the combat that is not `owner`, if `owner` is a side.
fn opponent_in(event: &crate::event::Event, owner: &PlayerId) -> Option<PlayerId> {
    let attacker = event.text("attacker")?;
    let defender = event.text("defender")?;
    if attacker == owner.as_str() {
        Some(PlayerId::new(defender))
    } else if defender == owner.as_str() {
        Some(PlayerId::new(attacker))
    } else {
        None
    }
}

/// Effect 2: a hit on the opponent's ground forces as the ground combat starts.
fn combat_start(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:md_hit:GROUND_COMBAT_STARTED:after"),
        seat.clone(),
        "GROUND_COMBAT_STARTED",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            let (Some(system), Some(planet), Some(opponent)) = (
                event.text("system").map(SystemId::new),
                event.text("planet").map(PlanetId::new),
                opponent_in(event, &owner),
            ) else {
                return Ok(());
            };
            let types = catalogue(context.content, context.sources);
            let mut targets: Vec<Unit> = Vec::new();
            for unit in context
                .state
                .system_state(&system)
                .on_planet_of(&planet, &opponent)
            {
                let ground = crate::invasion::is_ground_force_here(
                    context.state,
                    context.content,
                    context.sources,
                    &system,
                    &planet,
                    unit,
                );
                // Identical units are one choice.
                if ground
                    && !targets.iter().any(|seen| {
                        seen.type_id == unit.type_id
                            && seen.sustained_damage == unit.sustained_damage
                            && seen.galvanized == unit.galvanized
                    })
                {
                    targets.push(unit.clone());
                }
            }
            let target = match targets.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let options = targets
                        .iter()
                        .enumerate()
                        .map(|(index, unit)| {
                            let sturdy = !unit.sustained_damage
                                && types
                                    .get(unit.type_id.as_str())
                                    .is_some_and(UnitType::sustain_damage);
                            ChoiceOption::labelled(
                                index.to_string(),
                                "magen_defense_hit",
                                format!(
                                    "assign the hit to {}'s {}{}",
                                    unit.owner,
                                    unit.type_id,
                                    if unit.sustained_damage {
                                        " (damaged)"
                                    } else {
                                        ""
                                    }
                                ),
                            )
                            .with("unit", unit.type_id.as_str())
                            .with("damaged", unit.sustained_damage)
                            .with("kills", !sturdy)
                            .with(
                                "cost",
                                types.get(unit.type_id.as_str()).map_or(0.0, UnitType::cost),
                            )
                        })
                        .collect();
                    let choice = Choice::new(
                        owner.clone(),
                        "Magen Defense Grid: assign the hit to one of the opponent's ground forces"
                            .to_owned(),
                        options,
                    )
                    .contextualized(DecisionContext::new(
                        owner.clone(),
                        DecisionSource::Content(TECH.to_owned()),
                        "magen_defense_hit",
                        context.state.phase,
                        context.state.round,
                    ));
                    let answer = context
                        .ask_seeing(&choice)
                        .map_err(TimingError::IllegalChoice)?;
                    let Some(unit) = answer
                        .id
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| targets.get(index))
                    else {
                        return Ok(());
                    };
                    unit.clone()
                }
            };
            crate::invasion::assign_selected_ground_hit_in_timing(
                resolver, context, &system, &planet, &target, TECH,
            )?;
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        active(context.state, &condition_owner)
            && opponent_in(event, &condition_owner).is_some()
            && event
                .text("system")
                .zip(event.text("planet"))
                .is_some_and(|(system, planet)| {
                    let planet = PlanetId::new(planet);
                    structure_planets(
                        context.state,
                        context.content,
                        context.sources,
                        &condition_owner,
                        &SystemId::new(system),
                    )
                    .contains(&planet)
                })
    }))
}
