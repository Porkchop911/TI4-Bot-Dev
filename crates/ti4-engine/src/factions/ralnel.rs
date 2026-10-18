//! The Ral Nel Consortium (`ralnel`, Thunder's Edge). See `plans/evidence/BF-ralnel.md`.
//!
//! Split for size: this file holds Survival Instinct, Miniaturization, the Nano-Link Permit, the
//! Linkship space-cannon loan, the Last Dispatch's retreat, and the Director Nel hero's un-pass;
//! `ralnel_cards.rs` holds Nanomachines, Kan Kip Rel, the Alarum, and Data Skimmer. The Watchful
//! Ojz commander is the shared borrowed-commander effect (`borrowed_commanders.rs`), claimed here.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Survival Instinct: "After a player activates a system that contains your ships: you may move
//!   up to 2 of your ships into the active system from adjacent systems that do not contain your
//!   command tokens." (These ships can transport fighters and ground forces.)
//! * Miniaturization: "Your structures can be transported by any ship; this does not require or
//!   count against capacity. While your structures are in the space area, they cannot use their
//!   unit abilities." Window: "At the end of your tactical actions: you may place your structures
//!   that are in space areas onto planets you control in their respective systems."
//! * Nano-Link Permit: "After you activate a system: You may move your structures from adjacent
//!   systems that do not contain your command tokens onto planets you control in the active
//!   system. Then, return this card to the Ral Nel player."
//! * Linkship I / II: "This unit can use the SPACE CANNON ability of one of your structures in its
//!   space area; each structure (I) / each linkship (II) can trigger the same structure."
//! * Last Dispatch (flagship): "When this unit retreats, you may destroy 1 ship in the active system
//!   that does not have SUSTAIN DAMAGE."
//! * Director Nel (hero): "After the last player passes: You may choose to no longer be passed; if
//!   you do, gain 2 command tokens, draw 1 action card, and purge this card."
//!
//! Shared rights: Ral Nel's commander and the faction's flagship text reach the other factions
//! through `promissory::has_commander_ability` and `factions::flagship_has_text` (Nekro Valefar Z).

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::UnitType;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_movement::MovementHooks;
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias.
pub const FACTION: &str = "ralnel";
/// Survival Instinct.
pub const SURVIVAL: &str = "survivalinstinct";
/// Miniaturization.
pub const MINIATURIZATION: &str = "miniaturization";
/// Linkship I.
pub const DESTROYER: &str = "ralnel_destroyer";
/// Linkship II.
pub const DESTROYER2: &str = "ralnel_destroyer2";
/// Last Dispatch.
pub const FLAGSHIP: &str = "ralnel_flagship";
/// Alarum.
pub const MECH: &str = "ralnel_mech";
/// Kan Kip Rel.
pub const AGENT: &str = "ralnelagent";
/// Watchful Ojz.
pub const COMMANDER: &str = "ralnelcommander";
/// Director Nel.
pub const HERO: &str = "ralnelhero";

/// The Nano-Link Permit's note id (`promissory::note_id`).
pub const NANOLINK_NOTE: &str = "nanolink:ralnel";

/// Survival Instinct moves at most this many ships.
const SURVIVAL_MOVES: usize = 2;

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[SURVIVAL, MINIATURIZATION],
    technologies: &[super::ralnel_cards::NANOMACHINES, "linkship2"],
    units: &[FLAGSHIP, MECH, DESTROYER, DESTROYER2],
    promissory: &[super::ralnel_cards::NANOLINK],
    leaders: &[AGENT, COMMANDER, HERO],
    breakthroughs: &[super::ralnel_cards::BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        component_actions: Some(super::ralnel_cards::component_actions),
        perform_component: Some(super::ralnel_cards::perform_component),
        leader_action: Some(super::ralnel_cards::leader_action),
        use_leader: Some(super::ralnel_cards::use_leader),
        movement: MovementHooks {
            free_cargo: Some(free_cargo),
            ..MovementHooks::NONE
        },
        ..Hooks::NONE
    },
};

/// Whether `player` plays the Ral Nel Consortium.
#[must_use]
pub fn is_ralnel(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

// -- the one place the faction asks ---------------------------------------------------------------

pub(crate) fn decision(
    state: &GameState,
    who: &PlayerId,
    card: &str,
    subtype: &str,
) -> DecisionContext {
    DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

/// Put one question to `who`. Every Ral Nel choice made inside a timing window or a component
/// action is asked here.
pub(crate) fn ask(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    card: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(decision(
        context.state,
        who,
        card,
        subtype,
    ));
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

// -- Survival Instinct ----------------------------------------------------------------------------

/// The ships `owner` could move into `active` now: ships in an adjacent system that holds no
/// command token of `owner`, as `(origin, index in the origin's units, unit)`, in board order.
fn survival_candidates(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    owner: &PlayerId,
    active: &SystemId,
) -> Vec<(SystemId, usize, Unit)> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found = Vec::new();
    for origin in crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, owner)
        .neighbours(active.as_str())
    {
        let origin = SystemId::new(origin);
        let board = state.system_state(&origin);
        if board.command_tokens.contains(owner) {
            continue;
        }
        for (index, unit) in board.units.iter().enumerate() {
            if &unit.owner == owner
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ship)
            {
                found.push((origin.clone(), index, unit.clone()));
            }
        }
    }
    found
}

fn survival_ship_id(origin: &SystemId, index: usize) -> String {
    format!("{origin}|{index}")
}

/// Survival Instinct: "After a player activates a system that contains your ships: you may move up
/// to 2 of your ships into the active system from adjacent systems that do not contain your command
/// tokens." Each ship asks its own transport, from the system it leaves.
fn survival_instinct(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    active: &SystemId,
) -> Result<(), TimingError> {
    let Some(galaxy) = context.galaxy else {
        return Ok(());
    };
    for moved in 0..SURVIVAL_MOVES {
        let candidates = survival_candidates(
            context.state,
            context.content,
            context.sources,
            galaxy,
            owner,
            active,
        );
        if candidates.is_empty() {
            break;
        }
        let mut options: Vec<ChoiceOption> = candidates
            .iter()
            .map(|(origin, index, unit)| {
                ChoiceOption::labelled(
                    survival_ship_id(origin, *index),
                    "ship",
                    format!("{} from {origin}", unit.type_id),
                )
            })
            .collect();
        options.push(ChoiceOption::decline());
        let prompt = if moved == 0 {
            "Survival Instinct: move a ship into the active system?"
        } else {
            "Survival Instinct: move another ship into the active system?"
        };
        let answer = ask(context, owner, prompt.to_owned(), SURVIVAL, "move", options)?;
        let Some((origin, index, unit)) = candidates
            .into_iter()
            .find(|(origin, index, _)| survival_ship_id(origin, *index) == answer.id)
        else {
            break;
        };
        let cargo = passengers(context, owner, &origin, &unit)?;
        let ship = context.state.system_mut(&origin).units.remove(index);
        carry_passengers(context.state, &origin, active, &cargo);
        context.state.system_mut(active).units.push(ship);
    }
    Ok(())
}

/// The fighters and ground forces `owner` takes aboard `unit` from `origin`, asked one by one.
/// Nothing moves here: the caller applies the answer.
fn passengers(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    origin: &SystemId,
    unit: &Unit,
) -> Result<Vec<crate::transit::Cargo>, TimingError> {
    let mut hold = crate::transit::CargoWindow::for_ship(
        context.state,
        context.content,
        context.sources,
        owner,
        origin,
        unit,
        &[],
    );
    while let Some(choice) = hold.pending_choice() {
        let choice = choice.contextualized(decision(context.state, owner, SURVIVAL, "cargo"));
        let answer = context.ask_seeing(&choice).map_err(illegal)?;
        hold.resolve(answer).map_err(|error| {
            illegal(IllegalChoice::DeciderFailed {
                player: owner.clone(),
                prompt: "Survival Instinct: transport".to_owned(),
                reason: error.to_string(),
            })
        })?;
    }
    Ok(hold.cargo())
}

/// Move the passengers out of `from` and into the space area of `to`.
pub(crate) fn carry_passengers(
    state: &mut GameState,
    from: &SystemId,
    to: &SystemId,
    cargo: &[crate::transit::Cargo],
) {
    for carried in cargo {
        match &carried.source {
            crate::transit::CargoSource::Space => state
                .system_mut(from)
                .remove(std::slice::from_ref(&carried.unit)),
            crate::transit::CargoSource::Planet(planet) => state
                .system_mut(from)
                .remove_from_planet(planet, std::slice::from_ref(&carried.unit)),
        }
        if let crate::transit::CargoSource::Planet(planet) = &carried.source {
            crate::coexistence::reconcile(state, from, planet);
        }
        state.system_mut(to).units.push(carried.unit.clone());
    }
}

fn survival_window(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{SURVIVAL}:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            survival_instinct(context, &owner, &system)
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let Some(galaxy) = context.galaxy else {
            return false;
        };
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        let types = ti4_content::units::catalogue(context.content, context.sources);
        is_ralnel(context.state, &condition_owner)
            && context
                .state
                .system_state(&system)
                .units_of(&condition_owner)
                .iter()
                .any(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_ship)
                })
            && !survival_candidates(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &condition_owner,
                &system,
            )
            .is_empty()
    }))
}

// -- Miniaturization ------------------------------------------------------------------------------

/// Whether `unit` is a structure of a Ral Nel player (Miniaturization applies to it).
fn ralnel_structure(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &Unit,
) -> bool {
    is_ralnel(state, &unit.owner)
        && ti4_content::units::catalogue(content, sources)
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::is_structure)
}

/// "While your structures are in the space area, they cannot use their unit abilities": a
/// structure of a Ral Nel player standing in a space area (not on a planet) has no SPACE CANNON.
#[must_use]
pub(crate) fn silenced_in_space(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &Unit,
) -> bool {
    ralnel_structure(state, content, sources, unit)
}

/// "Your structures can be transported by any ship; this does not require or count against
/// capacity." A Ral Nel structure in a space area is cargo for any ship, free of capacity.
pub(crate) fn transportable_structure(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &Unit,
) -> bool {
    ralnel_structure(state, content, sources, unit)
}

fn free_cargo(state: &GameState, content: &ContentStore, sources: SourceSet, unit: &Unit) -> bool {
    ralnel_structure(state, content, sources, unit)
}

/// The planets `owner` controls in `system`, in id order.
fn controlled_in(state: &GameState, owner: &PlayerId, system: &SystemId) -> Vec<PlanetId> {
    state
        .system_state(system)
        .planet_control
        .iter()
        .filter(|(_, holder)| *holder == owner)
        .map(|(planet, _)| planet.clone())
        .collect()
}

/// Move the units at `moves` (`(origin, planet-or-space, unit)`) onto `planet` in `to`.
fn move_onto_planet(
    state: &mut GameState,
    to: &SystemId,
    planet: &PlanetId,
    moves: &[(SystemId, Option<PlanetId>, Unit)],
) {
    for (origin, source, unit) in moves {
        match source {
            None => state.system_mut(origin).remove(std::slice::from_ref(unit)),
            Some(from) => state
                .system_mut(origin)
                .remove_from_planet(from, std::slice::from_ref(unit)),
        }
        state
            .system_mut(to)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(unit.clone());
    }
}

/// Miniaturization, "At the end of your tactical actions": each system where a structure of yours
/// stands in the space area and you control a planet may place those structures onto one of your
/// planets there.
fn miniaturization_window(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{MINIATURIZATION}:TACTICAL_ACTION_ENDED:after"),
        seat.clone(),
        "TACTICAL_ACTION_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            for system in
                space_structure_systems(context.state, context.content, context.sources, &owner)
            {
                let planets = controlled_in(context.state, &owner, &system);
                let Some(_) = planets.first() else {
                    continue;
                };
                let options: Vec<ChoiceOption> = planets
                    .iter()
                    .map(|planet| {
                        ChoiceOption::labelled(
                            planet.to_string(),
                            "planet",
                            format!("{planet} in {system}"),
                        )
                    })
                    .collect();
                let answer = ask(
                    context,
                    &owner,
                    format!(
                        "Miniaturization: place your structures in {system} onto which planet?"
                    ),
                    MINIATURIZATION,
                    "place",
                    options,
                )?;
                let Some(planet) = planets.iter().find(|planet| planet.as_str() == answer.id)
                else {
                    continue;
                };
                let moves: Vec<(SystemId, Option<PlanetId>, Unit)> = context
                    .state
                    .system_state(&system)
                    .units
                    .iter()
                    .filter(|unit| {
                        unit.owner == owner
                            && ti4_content::units::catalogue(context.content, context.sources)
                                .get(unit.type_id.as_str())
                                .is_some_and(UnitType::is_structure)
                    })
                    .map(|unit| (system.clone(), None, unit.clone()))
                    .collect();
                move_onto_planet(context.state, &system, planet, &moves);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        is_ralnel(context.state, &condition_owner)
            && !space_structure_systems(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_empty()
    }))
}

/// Whether Miniaturization could matter now, so the game must announce the end of the tactical
/// action for it (`TACTICAL_ACTION_ENDED` is otherwise only announced when a card listens).
/// Mirrors the window's own condition: a Ral Nel seat with a space-area structure in a system
/// where it controls a planet. Games without Ral Nel never answer true.
#[must_use]
pub fn listens_for_tactical_end(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
) -> bool {
    state.players.iter().any(|seat| {
        is_ralnel(state, &seat.id)
            && !space_structure_systems(state, content, sources, &seat.id).is_empty()
    })
}

/// The systems where `owner` has a structure in the space area and controls a planet, in id order.
fn space_structure_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
) -> Vec<SystemId> {
    state
        .board
        .iter()
        .filter(|(system, board)| {
            !controlled_in(state, owner, system).is_empty()
                && board.units.iter().any(|unit| {
                    &unit.owner == owner
                        && ti4_content::units::catalogue(content, sources)
                            .get(unit.type_id.as_str())
                            .is_some_and(UnitType::is_structure)
                })
        })
        .map(|(system, _)| system.clone())
        .collect()
}

// -- Nano-Link Permit -----------------------------------------------------------------------------

/// The structures a holder of the Nano-Link Permit moves: each one of theirs in an adjacent system
/// with no command token of theirs, in space or on a planet, in board order.
fn nanolink_structures(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    holder: &PlayerId,
    active: &SystemId,
) -> Vec<(SystemId, Option<PlanetId>, Unit)> {
    let types = ti4_content::units::catalogue(content, sources);
    let is_structure = |unit: &Unit| {
        unit.owner == *holder
            && types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_structure)
    };
    let mut found = Vec::new();
    for origin in crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, holder)
        .neighbours(active.as_str())
    {
        let origin = SystemId::new(origin);
        let board = state.system_state(&origin);
        if board.command_tokens.contains(holder) {
            continue;
        }
        for unit in board.units.iter().filter(|unit| is_structure(unit)) {
            found.push((origin.clone(), None, unit.clone()));
        }
        for (planet, units) in &board.planet_units {
            for unit in units.iter().filter(|unit| is_structure(unit)) {
                found.push((origin.clone(), Some(planet.clone()), unit.clone()));
            }
        }
    }
    found
}

/// "After you activate a system: You may move your structures from adjacent systems that do not
/// contain your command tokens onto planets you control in the active system. Then, return this
/// card to the Ral Nel player." Used only when there is something to move and a planet to take it
/// to; the card returns when it was used.
fn nanolink_window(owner_name: &str, seat: &PlayerId) -> Ability {
    let (holder, condition_holder) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("promissory:{owner_name}:nanolink:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            let moves = nanolink_structures(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &holder,
                &system,
            );
            let planets = controlled_in(context.state, &holder, &system);
            if moves.is_empty() || planets.is_empty() {
                return Ok(());
            }
            let options: Vec<ChoiceOption> = planets
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(
                        planet.to_string(),
                        "planet",
                        format!("{planet} in {system}"),
                    )
                })
                .collect();
            let answer = ask(
                context,
                &holder,
                format!("Nano-Link Permit: move your structures into {system} onto which planet?"),
                NANOLINK_NOTE,
                "move",
                options,
            )?;
            let Some(planet) = planets
                .iter()
                .find(|planet| planet.as_str() == answer.id)
                .cloned()
            else {
                return Ok(());
            };
            move_onto_planet(context.state, &system, &planet, &moves);
            crate::promissory::give_back(context.state, NANOLINK_NOTE);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let Some(galaxy) = context.galaxy else {
            return false;
        };
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        event.text("player") == Some(condition_holder.as_str())
            && context.state.promissory_notes.get(NANOLINK_NOTE) == Some(&condition_holder)
            && !nanolink_structures(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &condition_holder,
                &system,
            )
            .is_empty()
            && !controlled_in(context.state, &condition_holder, &system).is_empty()
    }))
}

// -- Linkship: the space-cannon loan ----------------------------------------------------------------

/// The SPACE CANNON shots the Linkships in `system` add: each Linkship I uses one of its owner's
/// structures in the space area (each structure once); each Linkship II uses the best of them
/// again. The shot is the structure's own (its unit record), so the caller rolls it like any gun.
/// `may_fire` is the caller's own gate (barred, Quietus, Solar Flare).
#[must_use]
pub(crate) fn linkship_guns(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    may_fire: &dyn Fn(&PlayerId) -> bool,
) -> Vec<Unit> {
    let types = ti4_content::units::catalogue(content, sources);
    let board = state.system_state(system);
    let owners: std::collections::BTreeSet<PlayerId> =
        board.units.iter().map(|unit| unit.owner.clone()).collect();
    let mut guns = Vec::new();
    for owner in owners {
        if !is_ralnel(state, &owner) || !may_fire(&owner) {
            continue;
        }
        let mut structures: Vec<(i64, Unit)> = board
            .units_of(&owner)
            .into_iter()
            .filter(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_structure)
            })
            .filter_map(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .and_then(UnitType::space_cannon_hits_on)
                    .map(|hits| (hits, unit.clone()))
            })
            .collect();
        if structures.is_empty() {
            continue;
        }
        structures.sort_by(|a, b| a.0.cmp(&b.0));
        let linkship_i = board
            .units_of(&owner)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == DESTROYER)
            .count();
        let linkship_ii = board
            .units_of(&owner)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == DESTROYER2)
            .count();
        for (_, structure) in structures.iter().take(linkship_i) {
            guns.push(structure.clone());
        }
        if let Some((_, best)) = structures.first() {
            for _ in 0..linkship_ii {
                guns.push(best.clone());
            }
        }
    }
    guns
}

// -- Alarum ---------------------------------------------------------------------------------------

/// Whether a Ral Nel Alarum stands on `planet` in `system`.
#[must_use]
pub(crate) fn alarum_on(state: &GameState, system: &SystemId, planet: &PlanetId) -> bool {
    state
        .system_state(system)
        .planet_units
        .get(planet)
        .is_some_and(|units| {
            units
                .iter()
                .any(|unit| unit.type_id.as_str() == MECH && is_ralnel(state, &unit.owner))
        })
}

// -- Last Dispatch --------------------------------------------------------------------------------

/// Whether a Ral Nel flagship (or a Nekro flagship lent its text) is among the `arrived` ship types
/// of a retreat.
#[must_use]
pub(crate) fn flagship_among(state: &GameState, player: &PlayerId, arrived: &[String]) -> bool {
    arrived
        .iter()
        .any(|unit| crate::factions::flagship_has_text(state, player, unit, FLAGSHIP))
}

/// "When this unit retreats, you may destroy 1 ship in the active system that does not have
/// SUSTAIN DAMAGE." Any player's such ship; each is asked by owner and index.
fn last_dispatch(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:FLAGSHIP_RETREATED:when"),
        seat.clone(),
        "FLAGSHIP_RETREATED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let victims =
                sustainless_ships(context.state, context.content, context.sources, &system);
            if victims.is_empty() {
                return Ok(());
            }
            let options: Vec<ChoiceOption> = victims
                .iter()
                .map(|(index, unit)| {
                    ChoiceOption::labelled(
                        format!("{}|{index}", unit.owner),
                        "ship",
                        format!("{} of {}", unit.type_id, unit.owner),
                    )
                })
                .collect();
            let answer = ask(
                context,
                &owner,
                format!("Last Dispatch: destroy a ship in {system}?"),
                FLAGSHIP,
                "destroy",
                options,
            )?;
            let Some((_, victim)) = victims
                .into_iter()
                .find(|(index, unit)| format!("{}|{index}", unit.owner) == answer.id)
            else {
                return Ok(());
            };
            let _ = crate::combat::destroy_units(
                context.state,
                context.content,
                context.sources,
                &victim.owner,
                &system,
                std::slice::from_ref(&victim),
                "last_dispatch",
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        is_ralnel(context.state, &condition_owner)
            && event.text("player") == Some(condition_owner.as_str())
            && event.text("system").is_some_and(|system| {
                !sustainless_ships(
                    context.state,
                    context.content,
                    context.sources,
                    &SystemId::new(system),
                )
                .is_empty()
            })
    }))
}

/// The ships in `system` (any owner) without SUSTAIN DAMAGE, with their index in the system.
fn sustainless_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> Vec<(usize, Unit)> {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .system_state(system)
        .units
        .iter()
        .enumerate()
        .filter(|(_, unit)| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() && !kind.sustain_damage())
        })
        .map(|(index, unit)| (index, unit.clone()))
        .collect()
}

// -- Director Nel ----------------------------------------------------------------------------------

/// Director Nel, "After the last player passes": may un-pass (2 command tokens, 1 action card, then
/// purge). Registered for every seat; the hero must be unlocked and every seat passed.
fn director_nel(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{HERO}:PLAYER_PASSED:after"),
        seat.clone(),
        "PLAYER_PASSED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            // The window's own "may" is the choice to un-pass; nothing else is asked.
            if let Some(seat) = context.state.player_mut(&owner) {
                seat.passed = false;
            }
            crate::leaders::purge(context.state, &owner, &LeaderId::new(HERO));
            crate::strategy_cards::gain_tokens(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &owner,
                2,
            )
            .map_err(illegal)?;
            crate::action_cards::draw(context.state, context.content, context.table, &owner, 1)
                .map_err(illegal)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        is_ralnel(context.state, &condition_owner)
            && context.state.player(&condition_owner).is_some_and(|seat| {
                seat.leaders.get(&LeaderId::new(HERO)) == Some(&LeaderStatus::Unlocked)
            })
            && context.state.all_passed()
    }))
}

// -- the timing table ------------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        survival_window(owner_name, seat),
        miniaturization_window(owner_name, seat),
        nanolink_window(owner_name, seat),
        last_dispatch(owner_name, seat),
        director_nel(owner_name, seat),
    ];
    abilities.extend(super::ralnel_cards::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{
        ActionCardId, BreakthroughId, LeaderId, PlanetId, PlayerId, SystemId, TechnologyId,
        UnitTypeId,
    };
    use ti4_model::state::{GameState, LeaderStatus, Phase};
    use ti4_model::units::Unit;

    use super::super::crimson::testkit::{activated, emit, plain, ring_map, scripted};
    use super::super::ralnel_cards;
    use super::*;
    use crate::choice::ChoiceOption;

    /// The window ids: each optional window asks its own "may" first.
    const SURVIVAL_WINDOW: &str = "ability:ralnel:survivalinstinct:SYSTEM_ACTIVATED:after";
    const MINIATURIZATION_WINDOW: &str =
        "ability:ralnel:miniaturization:TACTICAL_ACTION_ENDED:after";
    const FLAGSHIP_WINDOW: &str = "unit:ralnel:ralnel_flagship:FLAGSHIP_RETREATED:when";
    const ALARUM_WINDOW: &str = "unit:ralnel:ralnel_mech:GROUND_COMBAT_ROUND_ENDED:after";
    const NANOLINK_WINDOW: &str = "promissory:sol:nanolink:SYSTEM_ACTIVATED:after";
    const OJZ_WINDOW: &str = "leader:ralnel:ralnelcommander:RETREAT_DECLARED:when";
    const HERO_WINDOW: &str = "leader:ralnel:ralnelhero:PLAYER_PASSED:after";

    fn ralnel() -> PlayerId {
        PlayerId::new("a")
    }
    fn sol() -> PlayerId {
        PlayerId::new("b")
    }
    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", "ralnel"), ("b", "sol"), ("c", "hacan")], DEFAULT)
    }
    fn sid(system: &str) -> SystemId {
        SystemId::new(system)
    }
    fn put(state: &mut GameState, system: &str, unit: &str, owner: &PlayerId) {
        state
            .system_mut(&sid(system))
            .units
            .push(Unit::new(UnitTypeId::new(unit), owner.clone()));
    }
    fn put_planet(state: &mut GameState, system: &str, planet: &str, unit: &str, owner: &PlayerId) {
        state
            .system_mut(&sid(system))
            .planet_units
            .entry(PlanetId::new(planet))
            .or_default()
            .push(Unit::new(UnitTypeId::new(unit), owner.clone()));
    }
    fn own(state: &mut GameState, system: &str, planet: &str, owner: &PlayerId) {
        state
            .system_mut(&sid(system))
            .planet_control
            .insert(PlanetId::new(planet), owner.clone());
    }
    fn planet_of(system: &str) -> String {
        ti4_content::galaxy::all_systems(content(), DEFAULT)
            .get(system)
            .and_then(|record| record.planets().first().map(|p| (*p).to_owned()))
            .expect("a system with a planet")
    }
    /// A plain centre with six plain neighbours; every neighbour is adjacent to the centre.
    fn map() -> (Galaxy, String, Vec<String>) {
        let ids = plain(7);
        let centre = ids[0].clone();
        let ring: Vec<&str> = ids[1..].iter().map(String::as_str).collect();
        (ring_map(&centre, &ring), centre, ids[1..].to_vec())
    }
    fn count(state: &GameState, system: &str, unit: &str, owner: &PlayerId) -> usize {
        state
            .system_state(&sid(system))
            .units
            .iter()
            .filter(|u| &u.owner == owner && u.type_id.as_str() == unit)
            .count()
    }
    fn on_planet(
        state: &GameState,
        system: &str,
        planet: &str,
        unit: &str,
        owner: &PlayerId,
    ) -> usize {
        state
            .system_state(&sid(system))
            .planet_units
            .get(&PlanetId::new(planet))
            .map_or(0, |units| {
                units
                    .iter()
                    .filter(|u| &u.owner == owner && u.type_id.as_str() == unit)
                    .count()
            })
    }

    // -- Survival Instinct -----------------------------------------------------------------------

    #[test]
    fn survival_instinct_moves_a_ship_from_a_neighbour_into_the_activated_system() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &centre, "cruiser", &ralnel());
        put(&mut state, &ring[0], "cruiser", &ralnel());
        emit(
            &mut state,
            Some(&galaxy),
            &[SURVIVAL_WINDOW, &format!("{}|0", ring[0])],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(count(&state, &ring[0], "cruiser", &ralnel()), 0, "it left");
        assert_eq!(
            count(&state, &centre, "cruiser", &ralnel()),
            2,
            "and arrived"
        );
    }

    #[test]
    fn survival_instinct_brings_the_ground_forces_a_ship_carries_from_its_system() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &centre, "cruiser", &ralnel());
        put(&mut state, &ring[0], "carrier", &ralnel());
        put(&mut state, &ring[0], "infantry", &ralnel());
        emit(
            &mut state,
            Some(&galaxy),
            &[SURVIVAL_WINDOW, &format!("{}|0", ring[0]), "load|0"],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(
            count(&state, &centre, "carrier", &ralnel()),
            1,
            "the carrier arrived"
        );
        assert_eq!(
            count(&state, &centre, "infantry", &ralnel()),
            1,
            "with its passenger"
        );
        assert_eq!(
            count(&state, &ring[0], "infantry", &ralnel()),
            0,
            "which left its system"
        );
    }

    #[test]
    fn survival_instinct_moves_at_most_two_ships_and_may_be_declined() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &centre, "cruiser", &ralnel());
        for system in &ring[..3] {
            put(&mut state, system, "cruiser", &ralnel());
        }
        // Two moves, then the window stops: the third neighbour keeps its ship.
        emit(
            &mut state,
            Some(&galaxy),
            &[
                SURVIVAL_WINDOW,
                &format!("{}|0", ring[0]),
                &format!("{}|0", ring[1]),
            ],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(count(&state, &centre, "cruiser", &ralnel()), 3);
        assert_eq!(
            count(&state, &ring[2], "cruiser", &ralnel()),
            1,
            "the third stays"
        );

        let mut declined = game();
        put(&mut declined, &centre, "cruiser", &ralnel());
        put(&mut declined, &ring[0], "cruiser", &ralnel());
        emit(
            &mut declined,
            Some(&galaxy),
            &["decline"],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(
            count(&declined, &ring[0], "cruiser", &ralnel()),
            1,
            "a decline moves nothing"
        );
    }

    #[test]
    fn survival_instinct_does_not_take_ships_from_a_system_with_your_command_token() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &centre, "cruiser", &ralnel());
        put(&mut state, &ring[0], "cruiser", &ralnel());
        state.system_mut(&sid(&ring[0])).place_token(ralnel());
        // No option is offered, so no question is put: an empty script must not be consulted.
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(count(&state, &ring[0], "cruiser", &ralnel()), 1);
    }

    #[test]
    fn survival_instinct_needs_one_of_your_ships_in_the_activated_system() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &ring[0], "cruiser", &ralnel());
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(
            count(&state, &ring[0], "cruiser", &ralnel()),
            1,
            "no window for it"
        );
    }

    // -- Miniaturization and the Linkships: SPACE CANNON -----------------------------------------

    /// Space combat at system 18, Sol active with a cruiser: the dice the Ral Nel guns roll.
    fn cannon_dice(state: &mut GameState) -> usize {
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(7);
        let _ = crate::combat::space_cannon_offense(
            state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &sid("18"),
            &sol(),
            None,
        );
        dice.count()
    }

    #[test]
    fn a_structure_in_the_space_area_cannot_fire_but_one_on_a_planet_still_does() {
        let mut state = game();
        put(&mut state, "18", "cruiser", &sol());
        put(&mut state, "18", "pds", &ralnel());
        assert_eq!(cannon_dice(&mut state), 0, "a miniaturized PDS is silent");

        let mut planet_state = game();
        put(&mut planet_state, "18", "cruiser", &sol());
        put_planet(&mut planet_state, "18", &planet_of("18"), "pds", &ralnel());
        assert_eq!(cannon_dice(&mut planet_state), 1, "on a planet it fires");
    }

    #[test]
    fn a_structure_in_space_is_cargo_for_any_ship_and_another_players_is_not() {
        let mut state = game();
        put(&mut state, "18", "pds", &ralnel());
        let cargo = crate::transit::loadable(&state, content(), DEFAULT, &ralnel(), &sid("18"));
        assert!(
            cargo.iter().any(|c| c.unit.type_id.as_str() == "pds"),
            "a Ral Nel structure in space is loadable"
        );
        let mut other = game();
        put(&mut other, "18", "pds", &sol());
        let cargo = crate::transit::loadable(&other, content(), DEFAULT, &sol(), &sid("18"));
        assert!(cargo.is_empty(), "another player's structure is not cargo");
    }

    #[test]
    fn at_the_end_of_a_tactical_action_space_structures_settle_on_your_planet() {
        let (galaxy, _, _) = map();
        let mut state = game();
        let planet = planet_of("18");
        own(&mut state, "18", &planet, &ralnel());
        put(&mut state, "18", "pds", &ralnel());
        emit(
            &mut state,
            Some(&galaxy),
            &[MINIATURIZATION_WINDOW, &planet],
            "TACTICAL_ACTION_ENDED",
            &[("player", "a".into())],
        );
        assert_eq!(
            count(&state, "18", "pds", &ralnel()),
            0,
            "left the space area"
        );
        assert_eq!(
            on_planet(&state, "18", &planet, "pds", &ralnel()),
            1,
            "onto the planet"
        );
    }

    #[test]
    fn miniaturization_is_offered_and_places_after_a_real_tactical_action() {
        let ring = plain(6);
        let ring_refs: Vec<&str> = ring.iter().map(String::as_str).collect();
        let galaxy = ring_map("18", &ring_refs);
        let planet = planet_of("18");
        let mut state = game();
        own(&mut state, "18", &planet, &ralnel());
        put(&mut state, &ring[0], "destroyer", &ralnel());
        put(&mut state, &ring[0], "pds", &ralnel());
        assert!(
            !listens_for_tactical_end(&state, content(), DEFAULT),
            "no structure in a space area of a controlled system yet"
        );
        let mv = format!("move|{}|0", ring[0]);
        let mut game = tactical_game(
            state,
            galaxy,
            &[
                crate::game::TACTICAL_ACTION_ID,
                "18",
                &mv,
                "load|0",
                "done_moving",
                MINIATURIZATION_WINDOW,
                &planet,
            ],
        );
        drive_tactical(&mut game, 60);
        assert!(
            game.events.iter().any(|e| e == "TACTICAL_ACTION_ENDED"),
            "{:?}",
            game.events
        );
        assert_eq!(count(&game.state, "18", "pds", &ralnel()), 0);
        assert_eq!(
            on_planet(&game.state, "18", &planet, "pds", &ralnel()),
            1,
            "Miniaturization placed the carried PDS on the planet"
        );
    }

    #[test]
    fn the_tactical_end_gate_needs_a_ralnel_seat_with_a_settleable_structure() {
        let planet = planet_of("18");
        let mut other = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        own(&mut other, "18", &planet, &PlayerId::new("a"));
        put(&mut other, "18", "pds", &PlayerId::new("a"));
        assert!(!listens_for_tactical_end(&other, content(), DEFAULT));
        let mut state = game();
        own(&mut state, "18", &planet, &ralnel());
        put(&mut state, "18", "pds", &ralnel());
        assert!(listens_for_tactical_end(&state, content(), DEFAULT));
    }

    #[test]
    fn linkship_i_borrows_each_structure_once() {
        let mut state = game();
        put(&mut state, "18", "cruiser", &sol());
        put(&mut state, "18", "pds", &ralnel());
        put(&mut state, "18", "ralnel_destroyer", &ralnel());
        put(&mut state, "18", "ralnel_destroyer", &ralnel());
        assert_eq!(
            cannon_dice(&mut state),
            1,
            "two linkships, one structure: one shot"
        );
    }

    #[test]
    fn linkship_ii_borrows_the_structure_once_for_each_linkship() {
        let mut state = game();
        put(&mut state, "18", "cruiser", &sol());
        put(&mut state, "18", "pds", &ralnel());
        put(&mut state, "18", "ralnel_destroyer2", &ralnel());
        put(&mut state, "18", "ralnel_destroyer2", &ralnel());
        assert_eq!(
            cannon_dice(&mut state),
            2,
            "each linkship II fires the structure"
        );
    }

    // -- Last Dispatch ---------------------------------------------------------------------------

    #[test]
    fn last_dispatch_destroys_a_ship_without_sustain_when_the_flagship_retreats() {
        let (galaxy, centre, _) = map();
        let mut state = game();
        put(&mut state, &centre, "dreadnought", &ralnel());
        put(&mut state, &centre, "cruiser", &sol());
        emit(
            &mut state,
            Some(&galaxy),
            &[FLAGSHIP_WINDOW, "b|1"],
            "FLAGSHIP_RETREATED",
            &[
                ("system", centre.clone().into()),
                ("player", "a".into()),
                ("destination", "x".into()),
            ],
        );
        assert_eq!(
            count(&state, &centre, "cruiser", &sol()),
            0,
            "the cruiser is destroyed"
        );
        assert_eq!(
            count(&state, &centre, "dreadnought", &ralnel()),
            1,
            "a sustaining ship is not offered"
        );
    }

    #[test]
    fn last_dispatch_counts_only_a_flagship_that_arrived_in_the_retreat() {
        let state = game();
        assert!(!flagship_among(&state, &ralnel(), &["cruiser".to_owned()]));
        assert!(flagship_among(&state, &ralnel(), &[FLAGSHIP.to_owned()]));
    }

    // -- Alarum ------------------------------------------------------------------------------------

    #[test]
    fn alarum_moves_a_ground_force_from_an_adjacent_planet_to_its_planet() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        let mine = planet_of(&centre);
        let theirs = planet_of(&ring[0]);
        put_planet(&mut state, &centre, &mine, "ralnel_mech", &ralnel());
        put_planet(&mut state, &ring[0], &theirs, "infantry", &ralnel());
        emit(
            &mut state,
            Some(&galaxy),
            &[ALARUM_WINDOW, &format!("{}|{theirs}|0", ring[0])],
            "GROUND_COMBAT_ROUND_ENDED",
            &[
                ("system", centre.clone().into()),
                ("planet", mine.clone().into()),
                ("attacker", "a".into()),
                ("defender", "b".into()),
            ],
        );
        assert_eq!(
            on_planet(&state, &ring[0], &theirs, "infantry", &ralnel()),
            0
        );
        assert_eq!(on_planet(&state, &centre, &mine, "infantry", &ralnel()), 1);
    }

    #[test]
    fn alarum_needs_the_mech_on_the_planet_where_the_round_ended() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        let theirs = planet_of(&ring[0]);
        put_planet(&mut state, &ring[0], &theirs, "infantry", &ralnel());
        assert!(!alarum_on(
            &state,
            &sid(&centre),
            &PlanetId::new(planet_of(&centre))
        ));
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "GROUND_COMBAT_ROUND_ENDED",
            &[
                ("system", centre.clone().into()),
                ("planet", planet_of(&centre).into()),
                ("attacker", "a".into()),
                ("defender", "b".into()),
            ],
        );
        assert_eq!(
            on_planet(&state, &ring[0], &theirs, "infantry", &ralnel()),
            1,
            "nothing moved"
        );
    }

    // -- Nano-Link Permit --------------------------------------------------------------------------

    const NANOLINK: &str = "nanolink:ralnel";

    #[test]
    fn the_nano_link_permit_moves_the_holders_structures_and_goes_back() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        let mine = planet_of(&centre);
        own(&mut state, &centre, &mine, &sol());
        let theirs = planet_of(&ring[0]);
        put_planet(&mut state, &ring[0], &theirs, "pds", &sol());
        state.promissory_notes.insert(NANOLINK.to_owned(), sol());
        emit(
            &mut state,
            Some(&galaxy),
            &[NANOLINK_WINDOW, &mine],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(
            on_planet(&state, &centre, &mine, "pds", &sol()),
            1,
            "moved onto the planet"
        );
        assert_eq!(
            on_planet(&state, &ring[0], &theirs, "pds", &sol()),
            0,
            "and left its system"
        );
        assert_eq!(
            state.promissory_notes.get(NANOLINK),
            Some(&ralnel()),
            "returned to Ral Nel"
        );
    }

    #[test]
    fn declining_the_nano_link_permit_keeps_the_card_and_the_structures() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        let mine = planet_of(&centre);
        own(&mut state, &centre, &mine, &sol());
        put_planet(&mut state, &ring[0], &planet_of(&ring[0]), "pds", &sol());
        state.promissory_notes.insert(NANOLINK.to_owned(), sol());
        emit(
            &mut state,
            Some(&galaxy),
            &["decline"],
            "SYSTEM_ACTIVATED",
            &activated(&sid(&centre), "b"),
        );
        assert_eq!(state.promissory_notes.get(NANOLINK), Some(&sol()));
        assert_eq!(on_planet(&state, &centre, &mine, "pds", &sol()), 0);
    }

    // -- Watchful Ojz ----------------------------------------------------------------------------

    #[test]
    fn watchful_ojz_retreats_up_to_two_ships_when_the_commander_is_unlocked() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        state
            .player_mut(&ralnel())
            .unwrap()
            .leaders
            .insert(LeaderId::new(COMMANDER), LeaderStatus::Unlocked);
        put(&mut state, &centre, "cruiser", &ralnel());
        emit(
            &mut state,
            Some(&galaxy),
            &[OJZ_WINDOW, &format!("to|{}", ring[0]), "ship|0|cruiser"],
            "RETREAT_DECLARED",
            &[
                ("system", centre.clone().into()),
                ("player", "a".into()),
                ("round", 1.into()),
            ],
        );
        assert_eq!(
            count(&state, &ring[0], "cruiser", &ralnel()),
            1,
            "retreated"
        );
        assert!(
            state
                .system_state(&sid(&ring[0]))
                .command_tokens
                .contains(&ralnel())
        );
    }

    // -- Director Nel ----------------------------------------------------------------------------

    fn everyone_passed(state: &mut GameState) {
        for seat in &mut state.players {
            seat.passed = true;
        }
        state
            .player_mut(&ralnel())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
    }

    #[test]
    fn director_nel_takes_back_the_pass_for_two_command_tokens_and_a_card() {
        let mut state = game();
        everyone_passed(&mut state);
        state.player_mut(&ralnel()).unwrap().action_cards = Vec::new();
        let tokens = state.player(&ralnel()).unwrap().tactic_tokens;
        emit(
            &mut state,
            None,
            &[HERO_WINDOW, "tactic_tokens", "tactic_tokens"],
            "PLAYER_PASSED",
            &[("player", "a".into())],
        );
        let seat = state.player(&ralnel()).unwrap();
        assert!(!seat.passed, "no longer passed");
        assert_eq!(seat.tactic_tokens, tokens + 2, "two command tokens");
        assert_eq!(seat.action_cards.len(), 1, "one action card");
        assert_eq!(
            seat.leaders.get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Purged)
        );
    }

    #[test]
    fn director_nel_declined_keeps_the_pass_and_is_asked_only_after_the_last_pass() {
        let mut state = game();
        everyone_passed(&mut state);
        emit(
            &mut state,
            None,
            &["decline"],
            "PLAYER_PASSED",
            &[("player", "a".into())],
        );
        assert!(state.player(&ralnel()).unwrap().passed);

        let mut waiting = game();
        everyone_passed(&mut waiting);
        waiting.player_mut(&sol()).unwrap().passed = false;
        emit(
            &mut waiting,
            None,
            &[],
            "PLAYER_PASSED",
            &[("player", "a".into())],
        );
        assert_eq!(
            waiting
                .player(&ralnel())
                .unwrap()
                .leaders
                .get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Unlocked),
            "not the last pass: no question"
        );
    }

    // -- the Ral Nel cards ------------------------------------------------------------------------

    fn card_perform(state: &mut GameState, who: &PlayerId, option: &str, answers: &[&str]) -> bool {
        let option = ChoiceOption::labelled(option, "component", option);
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(answers), |ctx| {
            ralnel_cards::perform_component(ctx, who, &option)
        })
    }

    fn with_nanomachines(state: &mut GameState) {
        state
            .player_mut(&ralnel())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(ralnel_cards::NANOMACHINES));
    }

    #[test]
    fn nanomachines_places_a_pds_on_a_planet_you_control_and_exhausts() {
        let mut state = game();
        with_nanomachines(&mut state);
        let planet = planet_of("18");
        own(&mut state, "18", &planet, &ralnel());
        assert!(card_perform(
            &mut state,
            &ralnel(),
            "faction|ralnel|nano_pds",
            &[]
        ));
        assert_eq!(on_planet(&state, "18", &planet, "pds", &ralnel()), 1);
        assert!(
            !card_perform(&mut state, &ralnel(), "faction|ralnel|nano_pds", &[]),
            "exhausted until the status phase"
        );
    }

    #[test]
    fn nanomachines_repairs_all_of_your_damaged_units() {
        let mut state = game();
        with_nanomachines(&mut state);
        put(&mut state, "18", "cruiser", &ralnel());
        state
            .system_mut(&sid("18"))
            .units
            .last_mut()
            .unwrap()
            .sustained_damage = true;
        assert!(card_perform(
            &mut state,
            &ralnel(),
            "faction|ralnel|nano_repair",
            &[]
        ));
        assert!(
            state
                .system_state(&sid("18"))
                .units
                .iter()
                .all(|u| !u.sustained_damage),
            "repaired"
        );
    }

    #[test]
    fn nanomachines_discards_one_card_and_draws_one() {
        let mut state = game();
        with_nanomachines(&mut state);
        state.player_mut(&ralnel()).unwrap().action_cards = vec![ActionCardId::new("sabotage")];
        let piles = state.discarded_action_cards.len();
        assert!(card_perform(
            &mut state,
            &ralnel(),
            "faction|ralnel|nano_draw",
            &[]
        ));
        assert_eq!(
            state.discarded_action_cards.len(),
            piles + 1,
            "the card went to the pile"
        );
        assert_eq!(
            state.player(&ralnel()).unwrap().action_cards.len(),
            1,
            "and one was drawn"
        );
    }

    #[test]
    fn nanomachines_is_offered_only_with_the_technology_and_is_readied_by_the_status_phase() {
        let mut state = game();
        assert!(
            ralnel_cards::component_actions(&state, content(), &ralnel()).is_empty(),
            "no technology, no action"
        );
        with_nanomachines(&mut state);
        let planet = planet_of("18");
        own(&mut state, "18", &planet, &ralnel());
        assert!(card_perform(
            &mut state,
            &ralnel(),
            "faction|ralnel|nano_pds",
            &[]
        ));
        emit(&mut state, None, &[], "STATUS_PHASE_ENDED", &[]);
        assert!(
            ralnel_cards::component_actions(&state, content(), &ralnel())
                .iter()
                .any(|option| option.id == "faction|ralnel|nano_pds"),
            "readied"
        );
    }

    #[test]
    fn kan_kip_rel_draws_two_and_gives_one_to_the_player_chosen() {
        let mut state = game();
        state.player_mut(&ralnel()).unwrap().action_cards = Vec::new();
        let total_before: usize = state.players.iter().map(|s| s.action_cards.len()).sum();
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["0", "b"]),
            |ctx| ralnel_cards::use_leader(ctx, &ralnel(), &LeaderId::new(AGENT)),
        );
        assert_eq!(used, Some(true));
        let total_after: usize = state.players.iter().map(|s| s.action_cards.len()).sum();
        assert_eq!(total_after, total_before + 2, "two cards drawn");
        assert_eq!(
            state.player(&ralnel()).unwrap().action_cards.len(),
            1,
            "one kept"
        );
        assert_eq!(
            state.player(&sol()).unwrap().action_cards.len(),
            1,
            "one given to the chosen player"
        );
    }

    #[test]
    fn data_skimmer_takes_another_players_discard_and_gives_one_card_when_the_holder_passes() {
        let mut state = game();
        state.player_mut(&ralnel()).unwrap().breakthrough =
            Some(BreakthroughId::new(ralnel_cards::BREAKTHROUGH));
        state.phase = Phase::Action;
        let card = ActionCardId::new("sabotage");
        assert!(
            ralnel_cards::skimmer_takes(&mut state, &sol(), &card),
            "placed on the card"
        );
        assert!(
            !state.discarded_action_cards.contains(&card),
            "not on the pile"
        );
        state.player_mut(&ralnel()).unwrap().passed = true;
        emit(
            &mut state,
            None,
            &[],
            "PLAYER_PASSED",
            &[("player", "a".into())],
        );
        assert_eq!(state.player(&ralnel()).unwrap().action_cards, vec![card]);
    }

    #[test]
    fn data_skimmer_does_not_take_the_holders_own_discard() {
        let mut state = game();
        state.player_mut(&ralnel()).unwrap().breakthrough =
            Some(BreakthroughId::new(ralnel_cards::BREAKTHROUGH));
        state.phase = Phase::Action;
        assert!(!ralnel_cards::skimmer_takes(
            &mut state,
            &ralnel(),
            &ActionCardId::new("sabotage")
        ));
    }

    // -- driven routes (review round) ------------------------------------------------------------

    fn drive_tactical(game: &mut crate::game::Game<'_>, steps: usize) {
        for _ in 0..steps {
            let result = game.step();
            assert_eq!(result.error, None, "log: {:?}", game.events);
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }
        assert!(
            game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE"),
            "{:?}",
            game.events
        );
    }

    fn tactical_game<'a>(
        mut state: GameState,
        galaxy: Galaxy,
        answers: &[&str],
    ) -> crate::game::Game<'a> {
        state.phase = Phase::Action;
        state.active = Some(ralnel());
        state.player_mut(&ralnel()).unwrap().fleet_tokens = 6;
        crate::game::Game::with_table(state, content(), scripted(answers))
            .with_galaxy(galaxy)
            .with_sources(DEFAULT)
    }

    #[test]
    fn a_destroyer_carries_a_structure_free_but_still_not_infantry_in_a_real_move() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &ring[0], "destroyer", &ralnel());
        put(&mut state, &ring[0], "pds", &ralnel());
        put(&mut state, &ring[0], "infantry", &ralnel());
        put(&mut state, &ring[0], "fighter", &ralnel());
        let mv = format!("move|{}|0", ring[0]);
        // Only the structure is offered to a zero-capacity ship: one load, then it sails.
        let mut game = tactical_game(
            state,
            galaxy,
            &[
                crate::game::TACTICAL_ACTION_ID,
                &centre,
                &mv,
                "load|0",
                "done_moving",
            ],
        );
        drive_tactical(&mut game, 40);
        assert_eq!(
            count(&game.state, &centre, "pds", &ralnel()),
            1,
            "the PDS rode along"
        );
        assert_eq!(count(&game.state, &centre, "destroyer", &ralnel()), 1);
        assert_eq!(
            count(&game.state, &ring[0], "infantry", &ralnel()),
            1,
            "infantry stayed"
        );
        assert_eq!(
            count(&game.state, &ring[0], "fighter", &ralnel()),
            1,
            "the fighter stayed"
        );
    }

    #[test]
    fn structures_do_not_use_a_carriers_capacity_in_a_real_move() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &ring[0], "carrier", &ralnel());
        put(&mut state, &ring[0], "pds", &ralnel());
        for _ in 0..4 {
            put(&mut state, &ring[0], "infantry", &ralnel());
        }
        let mv = format!("move|{}|0", ring[0]);
        let mut game = tactical_game(
            state,
            galaxy,
            &[
                crate::game::TACTICAL_ACTION_ID,
                &centre,
                &mv,
                "load|0",
                "load|1",
                "load|2",
                "load|3",
                "load|4",
                "done_moving",
            ],
        );
        drive_tactical(&mut game, 40);
        // The landing takes the planet, so Miniaturization (now announced) may settle the PDS on
        // it at the end of the action: count it in space or on a planet.
        let settled: usize = game
            .state
            .system_state(&sid(&centre))
            .planet_units
            .values()
            .flatten()
            .filter(|unit| unit.type_id.as_str() == "pds")
            .count();
        assert_eq!(
            count(&game.state, &centre, "pds", &ralnel()) + settled,
            1,
            "the PDS arrived"
        );
        let landed: usize = game
            .state
            .system_state(&sid(&centre))
            .planet_units
            .values()
            .flatten()
            .filter(|unit| unit.type_id.as_str() == "infantry")
            .count();
        assert_eq!(
            landed + count(&game.state, &centre, "infantry", &ralnel()),
            4,
            "four infantry fill all four slots beside it"
        );
    }

    #[test]
    fn a_zero_capacity_hold_with_nothing_free_stays_closed() {
        let (galaxy, centre, ring) = map();
        let mut state = game();
        put(&mut state, &ring[0], "destroyer", &ralnel());
        put(&mut state, &ring[0], "infantry", &ralnel());
        let hold = crate::transit::CargoWindow::for_ship(
            &state,
            content(),
            DEFAULT,
            &ralnel(),
            &sid(&ring[0]),
            &Unit::new(UnitTypeId::new("destroyer"), ralnel()),
            &[centre],
        );
        let _ = galaxy;
        assert!(hold.is_complete(), "no structure, no hold");
    }

    #[test]
    fn a_structure_in_the_space_area_has_no_production_but_one_on_a_planet_does() {
        let mut state = game();
        let planet = planet_of("18");
        own(&mut state, "18", &planet, &ralnel());
        put(&mut state, "18", "spacedock", &ralnel());
        assert_eq!(
            crate::production::capacity(&state, content(), DEFAULT, &ralnel(), &sid("18")),
            0,
            "a Space Dock in the space area cannot produce"
        );
        let mut grounded = game();
        own(&mut grounded, "18", &planet, &ralnel());
        put_planet(&mut grounded, "18", &planet, "spacedock", &ralnel());
        assert!(
            crate::production::capacity(&grounded, content(), DEFAULT, &ralnel(), &sid("18")) > 0,
            "the same dock on a planet produces"
        );
    }

    #[test]
    fn data_skimmer_takes_a_hand_limit_discard() {
        let mut state = game();
        state.player_mut(&ralnel()).unwrap().breakthrough =
            Some(BreakthroughId::new(ralnel_cards::BREAKTHROUGH));
        state.phase = Phase::Action;
        state.player_mut(&ralnel()).unwrap().action_cards = Vec::new();
        state.player_mut(&sol()).unwrap().action_cards = (0..=crate::action_cards::HAND_LIMIT)
            .map(|index| ActionCardId::new(format!("card{index}")))
            .collect();
        let mut table = scripted(&["0"]);
        crate::action_cards::enforce_hand_limit(&mut state, content(), &mut table, &sol())
            .expect("discards");
        assert_eq!(
            state.player(&sol()).unwrap().action_cards.len(),
            crate::action_cards::HAND_LIMIT
        );
        state.player_mut(&ralnel()).unwrap().passed = true;
        emit(
            &mut state,
            None,
            &[],
            "PLAYER_PASSED",
            &[("player", "a".into())],
        );
        assert_eq!(
            state.player(&ralnel()).unwrap().action_cards,
            vec![ActionCardId::new("card0")],
            "the discarded card was on the Data Skimmer"
        );
    }

    #[test]
    fn data_skimmer_takes_a_played_card_instead_of_the_pile() {
        let mut state = game();
        state.player_mut(&ralnel()).unwrap().breakthrough =
            Some(BreakthroughId::new(ralnel_cards::BREAKTHROUGH));
        state.phase = Phase::Action;
        state.player_mut(&ralnel()).unwrap().action_cards = Vec::new();
        let card = ActionCardId::new("sabotage");
        state.player_mut(&sol()).unwrap().action_cards = vec![card.clone()];
        let mut resolver = crate::fixtures::armed_resolver(&state);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut scripted(&[]), |ctx| {
            crate::reactions::play(ctx, &mut resolver, &sol(), &card, None).expect("plays")
        });
        assert!(
            !state.discarded_action_cards.contains(&card),
            "not on the pile"
        );
        state.player_mut(&ralnel()).unwrap().passed = true;
        emit(
            &mut state,
            None,
            &[],
            "PLAYER_PASSED",
            &[("player", "a".into())],
        );
        assert_eq!(state.player(&ralnel()).unwrap().action_cards, vec![card]);
    }

    #[test]
    fn data_skimmer_leaves_a_played_card_on_the_pile_once_the_holder_has_passed() {
        let mut state = game();
        state.player_mut(&ralnel()).unwrap().breakthrough =
            Some(BreakthroughId::new(ralnel_cards::BREAKTHROUGH));
        state.phase = Phase::Action;
        state.player_mut(&ralnel()).unwrap().passed = true;
        let card = ActionCardId::new("sabotage");
        state.player_mut(&sol()).unwrap().action_cards = vec![card.clone()];
        let mut resolver = crate::fixtures::armed_resolver(&state);
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut scripted(&[]), |ctx| {
            crate::reactions::play(ctx, &mut resolver, &sol(), &card, None).expect("plays")
        });
        assert!(state.discarded_action_cards.contains(&card));
    }

    #[test]
    fn director_nel_un_pass_is_honoured_by_the_real_turn_route() {
        let mut state = game();
        state.phase = Phase::Action;
        state.active = Some(ralnel());
        for seat in &mut state.players {
            seat.passed = seat.id != ralnel();
        }
        state
            .player_mut(&ralnel())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
        let tokens = state.player(&ralnel()).unwrap().tactic_tokens;
        let mut game = crate::game::Game::with_table(
            state,
            content(),
            scripted(&["pass", HERO_WINDOW, "tactic_tokens", "tactic_tokens"]),
        )
        .with_sources(DEFAULT);
        let result = game.step();
        assert_eq!(result.error, None, "log: {:?}", game.events);
        let seat = game.state.player(&ralnel()).unwrap();
        assert!(!seat.passed, "the pass was taken back");
        assert_eq!(seat.tactic_tokens, tokens + 2);
        assert_eq!(
            game.state.phase,
            Phase::Action,
            "the action phase did not end"
        );
        assert_eq!(
            game.state.active.as_ref(),
            Some(&ralnel()),
            "the turn stays with the un-passed seat"
        );
        assert_eq!(
            seat.leaders.get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Purged)
        );
    }

    /// A decider for a real combat: `retreater` announces a retreat, everyone else stays; any other
    /// question takes the first of `wants` offered, else declines, else the first option.
    struct Route {
        retreater: PlayerId,
        wants: Vec<String>,
    }

    impl crate::choice::Decider for Route {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            let pick = |id: &str| choice.option(id).cloned();
            let found = if choice.option("stay").is_some() {
                pick(if choice.player == self.retreater {
                    "retreat"
                } else {
                    "stay"
                })
            } else {
                self.wants.iter().find_map(|want| pick(want))
            };
            Ok(found.unwrap_or_else(|| {
                choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .unwrap_or(&choice.options[0])
                    .clone()
            }))
        }
    }

    #[test]
    fn last_dispatch_destroys_a_ship_when_the_flagship_retreats_in_a_real_space_combat() {
        use crate::choice::{Resolving, TimingHandle, Window};
        for seed in 0..120_u64 {
            let hub = crate::fixtures::plain_hub();
            let (system, outer) = (SystemId::new(&hub.centre), SystemId::new(&hub.outer[0]));
            let mut state = game();
            for id in std::iter::once(&hub.centre).chain(hub.outer.iter().take(1)) {
                state.board.entry(SystemId::new(id)).or_default();
            }
            // The seated game's own starting units share the hub's centre; clear them so the indexes are the test's.
            state.system_mut(&system).units.clear();
            state.system_mut(&system).planet_units.clear();
            crate::fixtures::put(&mut state, &system, FLAGSHIP, &ralnel(), 1);
            crate::fixtures::put(&mut state, &system, "dreadnought", &sol(), 1);
            crate::fixtures::put(&mut state, &system, "cruiser", &sol(), 1);
            crate::fixtures::put(&mut state, &outer, "fighter", &ralnel(), 1);
            let mut resolver = crate::fixtures::armed_resolver(&state);
            let mut sequence = crate::event::EventSequence::new();
            let mut table = crate::choice::Table::with_default(Box::new(Route {
                retreater: ralnel(),
                wants: vec![FLAGSHIP_WINDOW.to_owned(), "b|1".to_owned()],
            }));
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(seed);
            let mut window = crate::combat::CombatWindow::new(&state, content(), DEFAULT, &system)
                .with_galaxy(hub.galaxy);
            let mut ctx = Resolving {
                content: content(),
                sources: DEFAULT,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: Some(TimingHandle {
                    resolver: &mut resolver,
                    sequence: &mut sequence,
                    galaxy: None,
                }),
            };
            window.settle_open(&mut state, &mut ctx).expect("opens");
            while window.outcome().is_none() {
                window.drive(&mut state, &mut ctx).expect("drives");
                if window.outcome().is_some() {
                    break;
                }
                let _ = window.take_scoring_occurrence();
                window.settle_open(&mut state, &mut ctx).expect("settles");
            }
            let retreated = count(&state, outer.as_str(), FLAGSHIP, &ralnel()) == 1;
            let dispatched = table.log.records.iter().any(|r| r.chosen == "b|1");
            if !retreated || !dispatched {
                continue;
            }
            assert_eq!(
                count(&state, system.as_str(), "cruiser", &sol()),
                0,
                "Last Dispatch destroyed the sustainless cruiser (seed {seed})"
            );
            assert_eq!(
                count(&state, system.as_str(), "dreadnought", &sol()),
                1,
                "the dreadnought can sustain damage and was not offered"
            );
            return;
        }
        panic!("some seed lets the flagship live to retreat");
    }

    // -- neutrality ----------------------------------------------------------------------------

    #[test]
    fn a_game_without_the_ral_nel_is_untouched_by_every_window() {
        let (galaxy, centre, ring) = map();
        let mut state =
            crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "letnev")], DEFAULT);
        put(&mut state, &centre, "cruiser", &ralnel());
        put(&mut state, &ring[0], "cruiser", &sol());
        put(&mut state, &ring[0], "pds", &sol());
        let before = state.clone();
        for (kind, pairs) in [
            ("SYSTEM_ACTIVATED", activated(&sid(&centre), "b")),
            ("TACTICAL_ACTION_ENDED", vec![("player", "a".into())]),
            ("PLAYER_PASSED", vec![("player", "a".into())]),
            (
                "FLAGSHIP_RETREATED",
                vec![("system", centre.clone().into()), ("player", "a".into())],
            ),
            (
                "GROUND_COMBAT_ROUND_ENDED",
                vec![
                    ("system", centre.clone().into()),
                    ("planet", planet_of(&centre).into()),
                ],
            ),
        ] {
            emit(&mut state, Some(&galaxy), &[], kind, &pairs);
        }
        assert!(
            state == before,
            "no Ral Nel window asks or changes anything"
        );
        let pds = Unit::new(UnitTypeId::new("pds"), sol());
        assert!(!silenced_in_space(&state, content(), DEFAULT, &pds));
        assert!(!transportable_structure(&state, content(), DEFAULT, &pds));
        assert!(
            linkship_guns(
                &state,
                content(),
                DEFAULT,
                &sid(&ring[0]),
                &|_: &PlayerId| true
            )
            .is_empty()
        );
        assert!(!alarum_on(
            &state,
            &sid(&centre),
            &PlanetId::new(planet_of(&centre))
        ));
    }
}
