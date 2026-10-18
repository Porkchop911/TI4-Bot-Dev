//! The Obsidian (`obsidian`, alias in `factions.json`), part B of the two-sided Firmament / Obsidian
//! faction. See `plans/evidence/BF-obsidian.md`; part A (`firmament.rs`, `firmament_flip.rs`) holds
//! the transition that makes a seat the Obsidian.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Nocturne: "This faction cannot be chosen during setup." (`seating::seeded_faction_assignments`
//!   refuses it; a seat is made the Obsidian by `firmament_flip::become_obsidian`.)
//! * The Blade's Orchestra: "When this faction comes into play: flip your home system,
//!   double-sided faction components, and all of your in-play plot cards. Then, ready Cronos Hollow
//!   and Tallin Hollow if you control them." (The body of the transition, `become_obsidian`, which
//!   announces `FACTION_FLIPPED`; the Viper Hollow hangs on that announcement.)
//! * Marionettes: "The player or players whose control tokens are on each plot card are the puppeted
//!   players for that plot." ([`firmament::puppeted`], read by The Reaping.)
//! * Planesplitter (Obsidian): "When you perform a strategic action, you may move an ingress token
//!   into a system that contains or is adjacent to your units. This technology cannot be researched."
//! * Neural Parasite (Obsidian): "At the start of your turn, destroy 1 of another player's infantry
//!   in or adjacent to a system that contains your infantry. This technology cannot be researched."
//! * Heaven's Hollow (flagship): statistics only, no ability text.
//! * Viper Hollow (mech): "If this unit was coexisting when this card flipped to this side, gain
//!   control of its planet; the other player's units are now coexisting."
//! * Vos Hollow (agent): "When a player's ship is destroyed during any combat: You may exhaust this
//!   card; if you do, that player's opponent must destroy 1 of their ships of the same type in the
//!   active system."
//! * Aroz Hollow (commander): "Apply +1 to the result of each of your units' combat rolls in The
//!   Fracture." Unlock: "Have units in The Fracture."
//! * Sharsiss Hollow (hero): "ACTION: Ready all of your planets. Then, purge this card." Unlock:
//!   "Have 3 scored objectives."
//! * Malevolency (promissory note): "At the end of one of your tactical actions: Spend 1 influence to
//!   give this card to one of your neighbors; you can use this ability even if you are the Obsidian
//!   player. At the end of the status phase, if you are not the Obsidian player, you must remove 1
//!   command token from your fleet pool and return it to your reinforcements."
//! * The Reaping (`obsidianbt`): "Place 1 trade good from the supply onto this card each time you win
//!   a combat against a puppeted player. At the start of the status phase, gain all trade goods on
//!   this card, then gain an equal number of trade goods from the supply."

use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::firmament;
use super::hooks_strategy::StrategyHooks;
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = firmament::OBSIDIAN;

/// Nocturne.
pub const NOCTURNE: &str = "nocturne";
/// The Blade's Orchestra.
pub const ORCHESTRA: &str = "bladesorchestra";
/// Marionettes.
pub const MARIONETTES: &str = "marionettes";
/// Planesplitter (Obsidian side).
pub const PLANESPLITTER: &str = "planesplitter-obs";
/// Neural Parasite (Obsidian side).
pub const PARASITE: &str = "parasite-obs";
/// Heaven's Hollow.
pub const FLAGSHIP: &str = "obsidian_flagship";
/// Viper Hollow.
pub const MECH: &str = "obsidian_mech";
/// Vos Hollow.
pub const AGENT: &str = "obsidianagent";
/// Aroz Hollow.
pub const COMMANDER: &str = "obsidiancommander";
/// Sharsiss Hollow.
pub const HERO: &str = "obsidianhero";
/// Malevolency.
pub const NOTE: &str = "malevolency";
/// The Reaping.
pub const BREAKTHROUGH: &str = "obsidianbt";

/// The staged event the transition announces (see `firmament_flip`).
const FLIPPED: &str = "FACTION_FLIPPED";

/// Factions whose setup choice is refused by their own text (Nocturne).
pub const NOT_CHOOSABLE_DURING_SETUP: &[&str] = &[FACTION];

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[NOCTURNE, ORCHESTRA, MARIONETTES],
    technologies: &[PLANESPLITTER, PARASITE],
    units: &[FLAGSHIP, MECH],
    promissory: &[NOTE],
    leaders: &[AGENT, COMMANDER, HERO],
    breakthroughs: &[BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        // Enervate: a puppeted player's secondary without a command token; Leadership's primary
        // instead of its secondary.
        substitutes_primary: Some(super::firmament_plots::substitutes_primary),
        strategy: StrategyHooks {
            secondary_waivers: Some(super::firmament_plots::secondary_waivers),
            secondary_waived: Some(super::firmament_plots::secondary_waived),
            ..StrategyHooks::NONE
        },
        ..Hooks::NONE
    },
};

/// Whether `player` plays the Obsidian.
#[must_use]
pub fn is_obsidian(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn illegal(error: crate::choice::IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

/// The one place the faction asks. Every Obsidian choice made inside a timing window goes through
/// here; `who` is the player who decides (the Obsidian, or the opponent of Vos Hollow).
pub(crate) fn ask(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    card: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        context.state.phase,
        context.state.round,
    ));
    context.ask_seeing(&choice).map_err(illegal)
}

/// Every system holding a unit (space or planet) of `player`.
fn occupied(state: &GameState, player: &PlayerId) -> BTreeSet<SystemId> {
    state
        .board
        .iter()
        .filter(|(_, board)| {
            board
                .units
                .iter()
                .chain(board.planet_units.values().flatten())
                .any(|unit| &unit.owner == player)
        })
        .map(|(system, _)| system.clone())
        .collect()
}

/// `systems` and every system adjacent to one of them for `player` (just `systems` without a map).
fn with_adjacent(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    systems: &BTreeSet<SystemId>,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
) -> BTreeSet<SystemId> {
    let mut reach = systems.clone();
    if let Some(galaxy) = galaxy {
        let adjacency =
            crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, player);
        for system in systems {
            reach.extend(adjacency.neighbours(system.as_str()).into_iter().map(SystemId::new));
        }
    }
    reach
}

// -- Planesplitter ---------------------------------------------------------------------------------

/// Every legal move of one ingress token: from where it stands into a regular-map system that holds
/// none (rule 14) and contains or is adjacent to a unit of `player`. Needs The Fracture in play (the
/// tokens exist only then).
fn ingress_moves(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    player: &PlayerId,
) -> Vec<(SystemId, SystemId)> {
    if !state.fracture_in_play {
        return Vec::new();
    }
    let mut targets = with_adjacent(state, content, sources, &occupied(state, player), galaxy, player);
    targets.retain(|system| {
        !state.ingress_tokens.contains(system)
            && !crate::fracture::is_fracture_system(content, sources, system)
    });
    let mut moves = Vec::new();
    for from in &state.ingress_tokens {
        for to in &targets {
            moves.push((from.clone(), to.clone()));
        }
    }
    moves
}

/// Planesplitter: when the owner performs a strategic action they may move an ingress token into a
/// system that contains or is adjacent to their units. Choosing the ability is the consent; the move
/// is then asked.
fn planesplitter(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{PLANESPLITTER}:STRATEGIC_ACTION_BEGAN:when"),
        seat.clone(),
        "STRATEGIC_ACTION_BEGAN",
        Relation::When,
        Arc::new(move |_event, _resolver, context| {
            let moves = ingress_moves(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                &owner,
            );
            if moves.is_empty() {
                return Ok(());
            }
            let options = moves
                .iter()
                .map(|(from, to)| {
                    ChoiceOption::labelled(
                        format!("{from}>{to}"),
                        "ingress_move",
                        format!("move the ingress token in {from} into {to}"),
                    )
                })
                .collect();
            let answer = ask(
                context,
                &owner,
                "Planesplitter: move an ingress token into a system with or next to your units"
                    .to_owned(),
                PLANESPLITTER,
                "ingress_move",
                options,
            )?;
            if let Some((from, to)) = moves
                .into_iter()
                .find(|(from, to)| format!("{from}>{to}") == answer.id)
            {
                context.state.ingress_tokens.remove(&from);
                context.state.ingress_tokens.insert(to);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && crate::technology::has_technology_text(
                context.state,
                &condition_owner,
                PLANESPLITTER,
            )
            && !ingress_moves(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                &condition_owner,
            )
            .is_empty()
    }))
}

// -- Neural Parasite -------------------------------------------------------------------------------

/// One infantry that Neural Parasite may destroy: where it stands (`None` for the space area), whose
/// it is and its unit type.
pub(crate) type Victim = (SystemId, Option<PlanetId>, PlayerId, String);

pub(super) fn is_infantry(
    types: &std::collections::BTreeMap<&str, ti4_content::units::UnitType<'_>>,
    unit: &Unit,
) -> bool {
    types
        .get(unit.type_id.as_str())
        .is_some_and(|kind| kind.base_type() == "infantry")
}

/// Another player's infantry in or adjacent to a system that contains `owner`'s infantry.
pub(crate) fn parasite_victims(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    owner: &PlayerId,
) -> Vec<Victim> {
    let types = ti4_content::units::catalogue(content, sources);
    let anywhere = |board: &ti4_model::state::SystemState| {
        board
            .units
            .iter()
            .chain(board.planet_units.values().flatten())
            .any(|unit| &unit.owner == owner && is_infantry(&types, unit))
    };
    let mine: BTreeSet<SystemId> = state
        .board
        .iter()
        .filter(|(_, board)| anywhere(board))
        .map(|(system, _)| system.clone())
        .collect();
    let mut found: BTreeSet<Victim> = BTreeSet::new();
    for system in with_adjacent(state, content, sources, &mine, galaxy, owner) {
        let Some(board) = state.board.get(&system) else {
            continue;
        };
        for unit in &board.units {
            if &unit.owner != owner && is_infantry(&types, unit) {
                found.insert((
                    system.clone(),
                    None,
                    unit.owner.clone(),
                    unit.type_id.to_string(),
                ));
            }
        }
        for (planet, units) in &board.planet_units {
            for unit in units {
                if &unit.owner != owner && is_infantry(&types, unit) {
                    found.insert((
                        system.clone(),
                        Some(planet.clone()),
                        unit.owner.clone(),
                        unit.type_id.to_string(),
                    ));
                }
            }
        }
    }
    found.into_iter().collect()
}

pub(super) fn victim_id((system, planet, owner, kind): &Victim) -> String {
    format!(
        "{system}|{}|{owner}|{kind}",
        planet
            .as_ref()
            .map_or_else(|| "space".to_owned(), ToString::to_string)
    )
}

/// Destroy one infantry as the effect `cause` (a [`parasite_victims`] entry, or a plot's).
pub(super) fn destroy_infantry(
    state: &mut GameState,
    (system, planet, owner, kind): &Victim,
    cause: &str,
) {
    let unit = {
        let board = state.system_state(system);
        let pool: &[Unit] = match planet {
            Some(planet) => board.on_planet(planet),
            None => &board.units,
        };
        pool.iter()
            .find(|unit| &unit.owner == owner && unit.type_id.as_str() == kind)
            .cloned()
    };
    let Some(unit) = unit else {
        return;
    };
    match planet {
        Some(planet) => {
            state
                .system_mut(system)
                .remove_from_planet(planet, std::slice::from_ref(&unit));
            super::hooks_ground::stage_ground_force_destroyed(state, system, planet, &unit, cause);
        }
        None => state.system_mut(system).remove(std::slice::from_ref(&unit)),
    }
}

/// Neural Parasite: at the start of the owner's turn, destroy 1 of another player's infantry in or
/// adjacent to a system that contains the owner's infantry. Mandatory; the owner picks which when
/// there is a choice.
fn parasite(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{PARASITE}:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let victims = parasite_victims(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                &owner,
            );
            let chosen = match victims.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let options = victims
                        .iter()
                        .map(|victim| {
                            let (system, planet, who, kind) = victim;
                            ChoiceOption::labelled(
                                victim_id(victim),
                                "infantry",
                                format!(
                                    "destroy {who}'s {kind} {}",
                                    planet.as_ref().map_or_else(
                                        || format!("in the space area of {system}"),
                                        |planet| format!("on {planet} in {system}"),
                                    )
                                ),
                            )
                        })
                        .collect();
                    let answer = ask(
                        context,
                        &owner,
                        "Neural Parasite: destroy 1 of another player's infantry".to_owned(),
                        PARASITE,
                        "neural_parasite",
                        options,
                    )?;
                    let Some(victim) = victims
                        .into_iter()
                        .find(|victim| victim_id(victim) == answer.id)
                    else {
                        return Ok(());
                    };
                    victim
                }
            };
            destroy_infantry(context.state, &chosen, "technology_parasite_obs");
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && crate::technology::has_technology_text(context.state, &condition_owner, PARASITE)
            && !parasite_victims(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                &condition_owner,
            )
            .is_empty()
    }))
}

// -- Viper Hollow ----------------------------------------------------------------------------------

/// Planets where `owner` has a Viper Hollow and is coexisting, with the player who controls each.
fn coexisting_vipers(state: &GameState, owner: &PlayerId) -> Vec<(SystemId, PlanetId, PlayerId)> {
    let mut found = Vec::new();
    for (system, board) in &state.board {
        for (planet, units) in &board.planet_units {
            if !units
                .iter()
                .any(|unit| &unit.owner == owner && unit.type_id.as_str() == MECH)
                || !crate::coexistence::is_coexisting(state, system, planet, owner)
            {
                continue;
            }
            if let Some(holder) = board.planet_control.get(planet)
                && holder != owner
            {
                found.push((system.clone(), planet.clone(), holder.clone()));
            }
        }
    }
    found
}

/// Viper Hollow: when the card flips to this side, on each planet where the unit was coexisting its
/// owner gains control and the other player's units are now coexisting (coexistence 3.2: the planet
/// is exhausted by the change of control).
fn viper(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{MECH}:{FLIPPED}:after"),
        seat.clone(),
        FLIPPED,
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            for (system, planet, holder) in coexisting_vipers(context.state, &owner) {
                if super::deepwrought::begin_coexisting(
                    context.state,
                    &holder,
                    &system,
                    &planet,
                    Some(&owner),
                )
                .is_err()
                {
                    continue;
                }
                super::hooks_ground::stage_planet_control_gained(
                    context.state,
                    &system,
                    &planet,
                    &owner,
                    Some(&holder),
                );
                crate::technology::control_gained(
                    context.state,
                    context.content,
                    context.sources,
                    context.galaxy,
                    context.table,
                    &owner,
                    &system,
                    &planet,
                )
                .map_err(illegal)?;
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("to") == Some(FACTION)
            && event.text("player") == Some(condition_owner.as_str())
            && is_obsidian(context.state, &condition_owner)
            && !coexisting_vipers(context.state, &condition_owner).is_empty()
    }))
}

// -- Vos Hollow ------------------------------------------------------------------------------------

fn agent_ready(state: &GameState, owner: &PlayerId) -> bool {
    state
        .player(owner)
        .is_some_and(|seat| seat.leaders.get(&LeaderId::new(AGENT)) == Some(&LeaderStatus::Readied))
}

/// A ship destroyed in a space combat: the system, whose it was and its type.
fn destroyed_ship(event: &crate::event::Event) -> Option<(SystemId, PlayerId, String)> {
    (event.boolean("during_space_combat") == Some(true)).then_some(())?;
    Some((
        SystemId::new(event.text("system")?),
        PlayerId::new(event.text("player")?),
        event.text("unit")?.to_owned(),
    ))
}

/// The victim's opponents in the system that still have a ship of the destroyed type.
fn agent_opponents(
    context_state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    victim: &PlayerId,
    kind: &str,
) -> Vec<PlayerId> {
    context_state
        .seating_order
        .iter()
        .filter(|seat| *seat != victim)
        .filter(|seat| {
            crate::combat::ships_of(context_state, content, sources, seat, system)
                .iter()
                .any(|ship| ship.type_id.as_str() == kind)
        })
        .cloned()
        .collect()
}

/// Vos Hollow: when a ship is destroyed during a combat, the owner may exhaust the agent; that
/// player's opponent must destroy a ship of the same type in the system (their choice which).
fn vos_hollow(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{AGENT}:SHIP_DESTROYED:when"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let Some((system, victim, kind)) = destroyed_ship(event) else {
                return Ok(());
            };
            let opponents = agent_opponents(
                context.state,
                context.content,
                context.sources,
                &system,
                &victim,
                &kind,
            );
            if opponents.is_empty()
                || !crate::leaders::exhaust(context.state, &owner, &LeaderId::new(AGENT))
            {
                return Ok(());
            }
            let opponent = match opponents.as_slice() {
                [only] => only.clone(),
                _ => {
                    let options = opponents
                        .iter()
                        .map(|seat| {
                            ChoiceOption::labelled(
                                seat.to_string(),
                                "player",
                                format!("{seat} destroys a {kind}"),
                            )
                        })
                        .collect();
                    let answer = ask(
                        context,
                        &owner,
                        format!("Vos Hollow: whose opponent destroys a {kind} in {system}"),
                        AGENT,
                        "vos_hollow_player",
                        options,
                    )?;
                    let Some(seat) = opponents
                        .into_iter()
                        .find(|seat| seat.as_str() == answer.id)
                    else {
                        return Ok(());
                    };
                    seat
                }
            };
            let mut distinct: Vec<Unit> = Vec::new();
            for ship in crate::combat::ships_of(
                context.state,
                context.content,
                context.sources,
                &opponent,
                &system,
            ) {
                if ship.type_id.as_str() == kind && !distinct.contains(&ship) {
                    distinct.push(ship);
                }
            }
            let ship = match distinct.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let options = distinct
                        .iter()
                        .enumerate()
                        .map(|(index, ship)| {
                            ChoiceOption::labelled(
                                format!("ship|{index}"),
                                "ship",
                                format!(
                                    "destroy your {} {kind}",
                                    if ship.sustained_damage {
                                        "damaged"
                                    } else {
                                        "undamaged"
                                    }
                                ),
                            )
                        })
                        .collect();
                    let answer = ask(
                        context,
                        &opponent,
                        format!("Vos Hollow: destroy 1 of your {kind} ships in {system}"),
                        AGENT,
                        "vos_hollow_ship",
                        options,
                    )?;
                    let Some(ship) = distinct
                        .into_iter()
                        .enumerate()
                        .find(|(index, _)| format!("ship|{index}") == answer.id)
                        .map(|(_, ship)| ship)
                    else {
                        return Ok(());
                    };
                    ship
                }
            };
            crate::combat::destroy_units_with_context(
                context.state,
                context.content,
                context.sources,
                &opponent,
                &system,
                &[ship],
                "leader:obsidianagent",
                true,
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        agent_ready(context.state, &condition_owner)
            && destroyed_ship(event).is_some_and(|(system, victim, kind)| {
                !agent_opponents(
                    context.state,
                    context.content,
                    context.sources,
                    &system,
                    &victim,
                    &kind,
                )
                .is_empty()
            })
    }))
}

// -- Aroz Hollow -----------------------------------------------------------------------------------

fn has_units_in_the_fracture(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    occupied(state, player)
        .iter()
        .any(|system| crate::fracture::is_fracture_system(content, sources, system))
}

fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    _galaxy: Option<&Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == COMMANDER)
        .then(|| has_units_in_the_fracture(state, content, sources, player))
}

// The effect itself, "+1 to the result of each of your units' combat rolls in The Fracture", is
// `borrowed_commanders::unit_roll_modifier`, which `factions::unit_roll_modifier` adds for every
// seat through `promissory::has_commander_ability`: the Obsidian's own unlocked commander, an
// Alliance copy, Yin's and Mahact's grants all reach it. This module does not add a second one.

// -- Sharsiss Hollow -------------------------------------------------------------------------------

/// `player`'s planets, including any ocean cards, that are exhausted.
fn exhausted_planets(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    state
        .controlled_planets(player)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .chain(super::deepwrought::oceans(state, player))
        .filter(|planet| state.exhausted_planets.contains(planet))
        .collect()
}

fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO)
        .then(|| is_obsidian(state, player) && !exhausted_planets(state, player).is_empty())
}

fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == HERO).then(|| {
        let planets = exhausted_planets(context.state, player);
        if !is_obsidian(context.state, player) || planets.is_empty() {
            return false;
        }
        for planet in &planets {
            context.state.ready_planet(planet);
        }
        true
    })
}

// -- Malevolency -----------------------------------------------------------------------------------

/// The player holding Malevolency, wherever it is (its owner included).
#[must_use]
pub fn malevolency_holder(state: &GameState) -> Option<PlayerId> {
    state
        .promissory_notes
        .get(&crate::promissory::note_id(NOTE, FACTION))
        .cloned()
}

/// Whether Malevolency is in the game, so the game must announce the end of tactical actions for it
/// (`TACTICAL_ACTION_ENDED` is otherwise only announced when a card listens for it).
#[must_use]
pub fn listens_for_tactical_end(state: &GameState) -> bool {
    malevolency_holder(state).is_some()
}

/// Malevolency, first sentence: at the end of one of the holder's tactical actions they may spend
/// 1 influence to give the card to one of their neighbors. Usable by the Obsidian as well. Choosing
/// the ability is the consent; the neighbor is asked, then the payment.
fn malevolency_give(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("promissory:{owner_name}:{NOTE}:TACTICAL_ACTION_ENDED:after"),
        seat.clone(),
        "TACTICAL_ACTION_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            let neighbours = crate::transactions::neighbours(context.state, galaxy, &owner);
            if neighbours.is_empty() {
                return Ok(());
            }
            let options = neighbours
                .iter()
                .map(|seat| {
                    ChoiceOption::labelled(
                        seat.to_string(),
                        "player",
                        format!("give Malevolency to {seat}"),
                    )
                })
                .collect();
            let answer = ask(
                context,
                &owner,
                "Malevolency: give this card to one of your neighbors".to_owned(),
                NOTE,
                "malevolency_neighbour",
                options,
            )?;
            let Some(neighbour) = neighbours
                .into_iter()
                .find(|seat| seat.as_str() == answer.id)
            else {
                return Ok(());
            };
            let before = context.state.clone();
            match crate::production::pay_seeing(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &owner,
                1,
                Spend::Influence,
            ) {
                Ok(true) => {
                    crate::promissory::take(
                        context.state,
                        context.content,
                        &neighbour,
                        &crate::promissory::note_id(NOTE, FACTION),
                    );
                    Ok(())
                }
                Ok(false) => {
                    *context.state = before;
                    Ok(())
                }
                Err(error) => {
                    *context.state = before;
                    Err(illegal(error))
                }
            }
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_owner.as_str())
            && malevolency_holder(context.state).as_ref() == Some(&condition_owner)
            && context.galaxy.is_some_and(|galaxy| {
                !crate::transactions::neighbours(context.state, galaxy, &condition_owner).is_empty()
            })
            && crate::payment::affordable(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                1,
                Spend::Influence,
            )
    }))
}

/// Whether `holder` holds Malevolency without being the Obsidian player.
fn levied(state: &GameState, holder: &PlayerId) -> bool {
    malevolency_holder(state).as_ref() == Some(holder)
        && !is_obsidian(state, holder)
        && state
            .player(holder)
            .is_some_and(|seat| seat.fleet_tokens > 0)
}

/// Malevolency, second sentence: at the end of the status phase a holder who is not the Obsidian
/// player removes 1 command token from their fleet pool and returns it to their reinforcements.
/// Mandatory, with nothing to choose.
fn malevolency_levy(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("promissory:{owner_name}:{NOTE}:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if levied(context.state, &owner)
                && let Some(seat) = context.state.player_mut(&owner)
            {
                seat.fleet_tokens -= 1;
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        levied(context.state, &condition_owner)
    }))
}

// -- The Reaping -----------------------------------------------------------------------------------

/// "Place 1 trade good from the supply onto this card." The supply is unlimited.
fn reap_one(state: &mut GameState, owner: &PlayerId) {
    firmament::add_goods_to_card(state, owner, 1);
}

/// The Reaping, first sentence, space combats: the owner won against a puppeted player.
fn reaping_space(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    let won_against_puppet =
        move |event: &crate::event::Event, state: &GameState, who: &PlayerId| {
            event.text("player") == Some(who.as_str())
                && crate::breakthroughs::holds(state, who, BREAKTHROUGH)
                && event
                    .decode::<Vec<String>>("opponents")
                    .unwrap_or_default()
                    .iter()
                    .any(|loser| firmament::puppeted(state, who).contains(&PlayerId::new(loser)))
        };
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:SPACE_COMBAT_WON:after"),
        seat.clone(),
        "SPACE_COMBAT_WON",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            reap_one(context.state, &owner);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        won_against_puppet(event, context.state, &condition_owner)
    }))
}

/// The Reaping, first sentence, ground combats: the owner won the fight on a planet against a
/// puppeted player.
fn reaping_ground(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:GROUND_COMBAT_ENDED:after"),
        seat.clone(),
        "GROUND_COMBAT_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            reap_one(context.state, &owner);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        if event.text("winner") != Some(condition_owner.as_str())
            || !crate::breakthroughs::holds(context.state, &condition_owner, BREAKTHROUGH)
        {
            return false;
        }
        let loser = [event.text("attacker"), event.text("defender")]
            .into_iter()
            .flatten()
            .find(|side| *side != condition_owner.as_str());
        loser.is_some_and(|loser| {
            firmament::puppeted(context.state, &condition_owner).contains(&PlayerId::new(loser))
        })
    }))
}

/// The Reaping, second sentence: at the start of the status phase, gain every trade good on the
/// card, then an equal number from the supply.
fn reaping_harvest(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}:STATUS_PHASE_BEGAN:after"),
        seat.clone(),
        "STATUS_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let held = firmament::take_goods_from_card(context.state, &owner);
            if held > 0 {
                crate::supply::gain_trade_goods_staged(context.state, &owner, held, BREAKTHROUGH);
                crate::supply::gain_trade_goods_staged(context.state, &owner, held, BREAKTHROUGH);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        crate::breakthroughs::holds(context.state, &condition_owner, BREAKTHROUGH)
            && firmament::goods_on_card(context.state, &condition_owner) > 0
    }))
}

// -- the module's timing abilities -----------------------------------------------------------------

pub(crate) fn timing_abilities(
    _state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    vec![
        planesplitter(owner_name, seat),
        parasite(owner_name, seat),
        viper(owner_name, seat),
        vos_hollow(owner_name, seat),
        malevolency_give(owner_name, seat),
        malevolency_levy(owner_name, seat),
        reaping_space(owner_name, seat),
        reaping_ground(owner_name, seat),
        reaping_harvest(owner_name, seat),
    ]
    .into_iter()
    .chain(super::firmament_plots::abilities(owner_name, seat))
    .collect()
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::{ContentType, DEFAULT};
    use ti4_model::id::{BreakthroughId, ObjectiveId, TechnologyId, UnitTypeId};

    use super::super::crimson::testkit::{emit, scripted};
    use super::super::firmament::testkit::{a, b, c, content, home, plain, ring};
    use super::*;

    const PLANESPLITTER_WINDOW: &str =
        "technology:obsidian:planesplitter-obs:STRATEGIC_ACTION_BEGAN:when";
    const AGENT_WINDOW: &str = "leader:obsidian:obsidianagent:SHIP_DESTROYED:when";
    const GIVE_WINDOW: &str = "promissory:obsidian:malevolency:TACTICAL_ACTION_ENDED:after";

    /// `a` the Obsidian (seated directly, as the test fixture may; Nocturne forbids it at setup),
    /// `b` Sol, `c` Hacan, every seat holding its own notes.
    fn game() -> GameState {
        let mut state = crate::fixtures::seated_game(
            &[("a", "obsidian"), ("b", "sol"), ("c", "hacan")],
            DEFAULT,
        );
        crate::promissory::deal(&mut state, content(), DEFAULT);
        state
    }

    fn unit(kind: &str, who: &PlayerId) -> Unit {
        Unit::new(UnitTypeId::new(kind), who.clone())
    }

    fn give_tech(state: &mut GameState, tech: &str) {
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new(tech));
    }

    fn leader_status(state: &GameState, who: &PlayerId, leader: &str) -> Option<LeaderStatus> {
        state
            .player(who)
            .unwrap()
            .leaders
            .get(&LeaderId::new(leader))
            .copied()
    }

    fn goods(state: &GameState, who: &PlayerId) -> i32 {
        state.player(who).unwrap().trade_goods
    }

    /// Seven plain systems: `ids[0]` in the middle, `ids[1..]` around it in ring order.
    fn map() -> (Vec<String>, Galaxy) {
        let ids = plain(7);
        let galaxy = ring(
            &ids[0],
            &ids[1..].iter().map(String::as_str).collect::<Vec<_>>(),
        );
        (ids, galaxy)
    }

    fn planet_of(system: &str) -> PlanetId {
        PlanetId::new(
            *ti4_content::galaxy::system(content(), system, DEFAULT)
                .unwrap()
                .planets()
                .first()
                .expect("a planet"),
        )
    }

    /// Run `FACTION_FLIPPED` (and everything else staged) through an armed resolver, as the game does
    /// after a component action.
    fn flush(state: &mut GameState, galaxy: &Galaxy, answers: &[&str]) -> usize {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut table = scripted(answers);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        let mut sequence = crate::event::EventSequence::new();
        let mut ctx = crate::choice::Resolving {
            content: content(),
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(galaxy),
            }),
        };
        crate::supply::flush_staged_events(state, &mut ctx)
    }

    // -- the module ----------------------------------------------------------------------------

    #[test]
    fn the_ledger_has_every_asset_on_the_obsidian_sheet() {
        let assets = super::super::assets(content(), DEFAULT, FACTION);
        assert_eq!(assets.len(), 12, "{assets:?}");
        assert!(
            super::super::missing(content(), DEFAULT, FACTION).is_empty(),
            "{:?}",
            super::super::missing(content(), DEFAULT, FACTION)
        );
        assert_eq!(
            BREAKTHROUGH,
            super::super::firmament_flip::OBSIDIAN_BREAKTHROUGH
        );
        assert_eq!(FLIPPED, super::super::firmament_flip::FLIPPED);
    }

    // -- Nocturne --------------------------------------------------------------------------------

    #[test]
    fn nocturne_keeps_the_obsidian_out_of_a_setup_roster() {
        let printed = content()
            .get(ContentType::Abilities, NOCTURNE)
            .and_then(|record| record.text("permanentEffect"))
            .unwrap();
        assert!(printed.contains("cannot be chosen during setup"));
        let players = [PlayerId::new("p0"), PlayerId::new("p1")];
        assert_eq!(
            crate::seating::seeded_faction_assignments(&["sol", "obsidian"], &players, 1),
            Err(crate::seating::FactionAssignmentError::NotChoosable(
                "obsidian".to_owned()
            ))
        );
        assert!(
            crate::seating::seeded_faction_assignments(&["sol", "hacan"], &players, 1).is_ok(),
            "every other roster is unchanged"
        );
        for alias in crate::seating::IN_SCOPE_FACTIONS {
            assert!(!NOT_CHOOSABLE_DURING_SETUP.contains(&alias));
        }
    }

    #[test]
    fn a_seat_that_is_the_obsidian_from_the_start_is_coherent_but_has_no_plots_or_vipers_to_flip() {
        // Not a legal setup (Nocturne); the fixture and the soak seat it directly for coverage.
        let state = game();
        let seat = state.player(&a()).unwrap();
        assert_eq!(seat.faction.as_str(), FACTION);
        assert_eq!(seat.home_system, Some(SystemId::new("96b")));
        assert_eq!(
            seat.home_planets,
            [PlanetId::new("cronoshollow"), PlanetId::new("tallinhollow")]
        );
        let board = state.system_state(&SystemId::new("96b"));
        for planet in &seat.home_planets {
            assert_eq!(board.planet_control.get(planet), Some(&a()));
        }
        assert!(!board.units.is_empty(), "the starting fleet is at home");
        assert!(seat.plots.is_empty() && seat.technologies.is_empty());
        for (leader, status) in [
            (AGENT, LeaderStatus::Readied),
            (COMMANDER, LeaderStatus::Locked),
            (HERO, LeaderStatus::Locked),
        ] {
            assert_eq!(
                leader_status(&state, &a(), leader),
                Some(status),
                "{leader}"
            );
        }
        assert_eq!(
            malevolency_holder(&state),
            Some(a()),
            "its own note is dealt"
        );
        assert!(crate::supply::staging_enabled(&state));
        // Nothing to flip: the Firmament's action is not offered.
        let (_, galaxy) = map();
        assert!(
            crate::factions::mapped_component_actions(&state, content(), DEFAULT, &galaxy, &a())
                .iter()
                .all(|option| option.id != super::super::firmament_flip::BECOME)
        );
    }

    #[test]
    fn the_hollow_home_system_builds_onto_a_board() {
        let seats: std::collections::BTreeMap<PlayerId, ti4_model::id::FactionId> =
            [("a", "obsidian"), ("b", "sol"), ("c", "hacan")]
                .iter()
                .map(|(player, faction)| {
                    (
                        PlayerId::new(*player),
                        ti4_model::id::FactionId::new(*faction),
                    )
                })
                .collect();
        let filler = crate::seating::neutral_systems(content(), 30, DEFAULT);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy =
            crate::seating::build_board(content(), &seats, &refs, DEFAULT).expect("a board");
        assert!(galaxy.coord_of("96b").is_some() && galaxy.coord_of("96a").is_none());
        for planet in ["cronoshollow", "tallinhollow"] {
            let record = ti4_content::galaxy::planet(content(), planet, DEFAULT).unwrap();
            assert_eq!(record.system_id(), Some("96b"));
            assert_eq!((record.resources(), record.influence()), (3, 0), "{planet}");
        }
    }

    // -- the flip: The Blade's Orchestra ----------------------------------------------------------

    /// The Firmament (`a`) with plots carrying `b`'s and `c`'s tokens in a ring map whose
    /// first ring system is its home tile.
    fn firmament() -> (GameState, Galaxy) {
        plotted(&[("enervate", "b"), ("siphon", "c")])
    }

    /// The Firmament (`a`) with the given facedown plot cards (card, token) in a ring map whose
    /// first ring system is its home tile.
    fn plotted(cards: &[(&str, &str)]) -> (GameState, Galaxy) {
        let mut state = crate::fixtures::seated_game(
            &[("a", "firmament"), ("b", "sol"), ("c", "hacan")],
            DEFAULT,
        );
        crate::promissory::deal(&mut state, content(), DEFAULT);
        let ids = plain(6);
        let mut ring_ids: Vec<&str> = ids[1..].iter().map(String::as_str).collect();
        ring_ids[0] = "96a";
        let galaxy = ring(&ids[0], &ring_ids);
        for (card, token) in cards {
            firmament::testkit::place_card(&mut state, &a(), card, &PlayerId::new(*token));
        }
        (state, galaxy)
    }

    /// Take Puppets of the Blade through the real component-action route.
    fn flip(state: &mut GameState, galaxy: &Galaxy) {
        let option =
            crate::factions::mapped_component_actions(state, content(), DEFAULT, galaxy, &a())
                .into_iter()
                .find(|option| option.id == super::super::firmament_flip::BECOME)
                .expect("Puppets of the Blade is offered");
        let mut table = scripted(&[]);
        let done =
            crate::fixtures::with_context(state, DEFAULT, Some(galaxy), &mut table, |context| {
                crate::factions::perform_component(context, &a(), &option)
            });
        assert!(done, "the transition resolves");
    }

    #[test]
    fn the_blades_orchestra_flips_the_home_the_components_and_the_plots_and_readies_the_hollows() {
        let (mut state, galaxy) = firmament();
        let old_home = home(&state, &a());
        crate::fixtures::put(&mut state, &old_home, "firmament_flagship", &a(), 1);
        let cronos = PlanetId::new("cronos");
        state.exhausted_planets.insert(cronos.clone());
        state.exhausted_planets.insert(PlanetId::new("tallin"));
        state
            .system_mut(&old_home)
            .set_control(PlanetId::new("tallin"), b());
        flip(&mut state, &galaxy);
        let seat = state.player(&a()).unwrap();
        assert!(is_obsidian(&state, &a()));
        assert_eq!(seat.home_system, Some(SystemId::new("96b")));
        assert_eq!(
            seat.plots,
            ["u:enervate:b", "u:siphon:c"],
            "plots are flipped faceup"
        );
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        assert!(
            !state
                .exhausted_planets
                .contains(&PlanetId::new("cronoshollow")),
            "a Hollow the Obsidian controls is readied"
        );
        assert!(
            state
                .exhausted_planets
                .contains(&PlanetId::new("tallinhollow")),
            "a Hollow it does not control is not"
        );
        assert!(
            state
                .system_state(&SystemId::new("96b"))
                .units
                .iter()
                .any(|unit| unit.type_id.as_str() == FLAGSHIP)
        );
        assert_eq!(
            crate::supply::staged_event_types(&state),
            [FLIPPED],
            "the arrival is announced"
        );
        // It comes into play once.
        assert!(
            crate::factions::mapped_component_actions(&state, content(), DEFAULT, &galaxy, &a())
                .iter()
                .all(|option| option.id != super::super::firmament_flip::BECOME)
        );
    }

    #[test]
    fn a_firmament_game_flips_and_then_the_viper_hollow_the_reaping_and_the_notes_work() {
        let (mut state, galaxy) = firmament();
        let old_home = home(&state, &a());
        let cronos = PlanetId::new("cronos");
        // The Firmament's Viper coexists with Sol on Cronos, which Sol holds.
        state.system_mut(&old_home).set_control(cronos.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &old_home, &cronos, "firmament_mech", &a(), 1);
        crate::coexistence::begin(&mut state, &old_home, &cronos, &a(), None).unwrap();
        // The Sowing holds three trade goods; the Firmament holds none of its own.
        state.player_mut(&a()).unwrap().breakthrough =
            Some(BreakthroughId::new(firmament::BREAKTHROUGH));
        firmament::add_goods_to_card(&mut state, &a(), 3);
        flip(&mut state, &galaxy);
        assert_eq!(
            state.player(&a()).unwrap().breakthrough,
            Some(BreakthroughId::new(BREAKTHROUGH))
        );
        assert_eq!(firmament::goods_on_card(&state, &a()), 3);

        // The game announces the arrival; the Viper Hollow takes the planet it was coexisting on.
        let hollow = PlanetId::new("cronoshollow");
        let new_home = SystemId::new("96b");
        assert_eq!(
            state.system_state(&new_home).planet_control.get(&hollow),
            Some(&b())
        );
        assert_eq!(flush(&mut state, &galaxy, &[]), 1);
        let board = state.system_state(&new_home);
        assert_eq!(
            board.planet_control.get(&hollow),
            Some(&a()),
            "gained control"
        );
        assert!(
            crate::coexistence::is_coexisting(&state, &new_home, &hollow, &b()),
            "the other player's units are now coexisting"
        );
        assert!(!crate::coexistence::is_coexisting(
            &state,
            &new_home,
            &hollow,
            &a()
        ));
        assert!(
            state.exhausted_planets.contains(&hollow),
            "a change of control exhausts the planet (coexistence 3.2)"
        );
        assert!(super::super::hooks_ground::has_staged_events(&state));

        // The Reaping: a combat won against a puppeted player (c's token is on a plot).
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "SPACE_COMBAT_WON",
            &[
                ("player", "a".into()),
                ("system", "18".into()),
                ("opponents", serde_json::json!(["c"])),
            ],
        );
        assert_eq!(firmament::goods_on_card(&state, &a()), 4);
        let before = goods(&state, &a());
        emit(&mut state, Some(&galaxy), &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(firmament::goods_on_card(&state, &a()), 0);
        assert_eq!(
            goods(&state, &a()),
            before + 8,
            "the card's goods, then as many again"
        );

        // Malevolency: the note the flip dealt is the Obsidian's, and Sol (not the Obsidian) holds
        // one it was given.
        assert_eq!(malevolency_holder(&state), Some(a()));
        // Neural Parasite (Obsidian side, carried over) destroys Sol's infantry next to the
        // Obsidian's at the start of its turn.
        give_tech(&mut state, PARASITE);
        let spot = &new_home;
        let tallin = PlanetId::new("tallinhollow");
        crate::fixtures::put_on_planet(&mut state, spot, &tallin, "infantry", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, spot, &hollow, "infantry", &b(), 1);
        let victims = |state: &GameState| {
            state
                .system_state(spot)
                .on_planet_of(&hollow, &b())
                .iter()
                .filter(|unit| unit.type_id.as_str() == "infantry")
                .count()
        };
        assert_eq!(victims(&state), 1);
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(victims(&state), 0);
    }

    // -- Marionettes -----------------------------------------------------------------------------

    #[test]
    fn marionettes_make_every_player_with_a_token_on_a_plot_a_puppeted_player() {
        let mut state = game();
        firmament::testkit::place_plot(&mut state, &a(), &b());
        assert!(firmament::add_token(&mut state, &a(), 0, &c()));
        assert_eq!(
            firmament::puppeted(&state, &a()),
            BTreeSet::from([b(), c()]),
            "one plot, two players' tokens: both are puppeted for it"
        );
        state.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new(BREAKTHROUGH));
        for loser in ["b", "c"] {
            let before = firmament::goods_on_card(&state, &a());
            emit(
                &mut state,
                None,
                &[],
                "SPACE_COMBAT_WON",
                &[
                    ("player", "a".into()),
                    ("system", "18".into()),
                    ("opponents", serde_json::json!([loser])),
                ],
            );
            assert_eq!(
                firmament::goods_on_card(&state, &a()),
                before + 1,
                "{loser}"
            );
        }
        // Nobody else's token is on a plot: an unmarked player is not puppeted.
        let bare = game();
        assert!(firmament::puppeted(&bare, &a()).is_empty());
    }

    // -- The Reaping -----------------------------------------------------------------------------

    fn reaper() -> GameState {
        let mut state = game();
        state.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new(BREAKTHROUGH));
        firmament::testkit::place_plot(&mut state, &a(), &b());
        state
    }

    fn won_space(state: &mut GameState, winner: &str, losers: &[&str]) {
        emit(
            state,
            None,
            &[],
            "SPACE_COMBAT_WON",
            &[
                ("player", winner.into()),
                ("system", "18".into()),
                ("opponents", serde_json::json!(losers)),
            ],
        );
    }

    fn ground(
        winner: Option<&str>,
        attacker: &str,
        defender: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        let mut payload = vec![
            ("system", "18".into()),
            ("planet", "mecatol".into()),
            ("attacker", attacker.into()),
            ("defender", defender.into()),
            ("control_changed", false.into()),
        ];
        if let Some(winner) = winner {
            payload.push(("winner", winner.into()));
        }
        payload
    }

    #[test]
    fn the_reaping_takes_a_trade_good_for_each_space_combat_won_against_a_puppeted_player() {
        let mut state = reaper();
        won_space(&mut state, "a", &["b"]);
        assert_eq!(firmament::goods_on_card(&state, &a()), 1);
        won_space(&mut state, "a", &["b"]);
        assert_eq!(firmament::goods_on_card(&state, &a()), 2);
        assert_eq!(
            goods(&state, &a()),
            goods(&game(), &a()),
            "the goods come from the supply"
        );
        // Not against a player with no token on a plot, not when someone else won, not without the card.
        won_space(&mut state, "a", &["c"]);
        won_space(&mut state, "b", &["a"]);
        assert_eq!(firmament::goods_on_card(&state, &a()), 2);
        let mut bare = game();
        firmament::testkit::place_plot(&mut bare, &a(), &b());
        won_space(&mut bare, "a", &["b"]);
        assert_eq!(
            firmament::goods_on_card(&bare, &a()),
            0,
            "no breakthrough, no card"
        );
    }

    #[test]
    fn the_reaping_also_counts_ground_combats_won_by_either_side() {
        let mut state = reaper();
        emit(
            &mut state,
            None,
            &[],
            "GROUND_COMBAT_ENDED",
            &ground(Some("a"), "a", "b"),
        );
        emit(
            &mut state,
            None,
            &[],
            "GROUND_COMBAT_ENDED",
            &ground(Some("a"), "b", "a"),
        );
        assert_eq!(
            firmament::goods_on_card(&state, &a()),
            2,
            "attacking and defending"
        );
        emit(
            &mut state,
            None,
            &[],
            "GROUND_COMBAT_ENDED",
            &ground(Some("b"), "a", "b"),
        );
        emit(
            &mut state,
            None,
            &[],
            "GROUND_COMBAT_ENDED",
            &ground(None, "a", "b"),
        );
        emit(
            &mut state,
            None,
            &[],
            "GROUND_COMBAT_ENDED",
            &ground(Some("a"), "a", "c"),
        );
        assert_eq!(
            firmament::goods_on_card(&state, &a()),
            2,
            "a lost fight, a draw and an unpuppeted opponent give nothing"
        );
    }

    #[test]
    fn the_reaping_gains_the_card_then_as_many_again_at_the_start_of_the_status_phase() {
        let mut state = reaper();
        firmament::add_goods_to_card(&mut state, &a(), 2);
        let before = goods(&state, &a());
        emit(&mut state, None, &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(goods(&state, &a()), before + 4);
        assert_eq!(firmament::goods_on_card(&state, &a()), 0);
        assert_eq!(
            crate::supply::staged_events(&state),
            2,
            "two gains are announced, the card's goods then the supply's"
        );
        let again = state.clone();
        emit(&mut state, None, &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(
            state.player(&a()),
            again.player(&a()),
            "an empty card does nothing"
        );
    }

    // -- Planesplitter (Obsidian) ----------------------------------------------------------------

    fn splitter() -> (GameState, Galaxy, Vec<String>) {
        let (ids, galaxy) = map();
        let mut state = game();
        give_tech(&mut state, PLANESPLITTER);
        state.fracture_in_play = true;
        state.ingress_tokens.insert(SystemId::new(ids[4].as_str()));
        crate::fixtures::put(
            &mut state,
            &SystemId::new(ids[1].as_str()),
            "cruiser",
            &a(),
            1,
        );
        (state, galaxy, ids)
    }

    #[test]
    fn planesplitter_moves_an_ingress_token_next_to_the_owners_units_when_it_performs_a_strategic_action()
     {
        let (mut state, galaxy, ids) = splitter();
        let (from, to) = (ids[4].as_str(), ids[2].as_str());
        assert!(
            galaxy.are_adjacent(&ids[1], to),
            "the ring neighbour is adjacent"
        );
        let pick = format!("{from}>{to}");
        emit(
            &mut state,
            Some(&galaxy),
            &[PLANESPLITTER_WINDOW, pick.as_str()],
            "STRATEGIC_ACTION_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(
            state.ingress_tokens,
            BTreeSet::from([SystemId::new(to)]),
            "moved, not copied"
        );
        // Only to a system with or next to its units: the far side of the ring is not on offer.
        let moves = ingress_moves(&state, content(), DEFAULT, Some(&galaxy), &a());
        assert!(!moves.iter().any(|(_, target)| target.as_str() == ids[5]));
        assert!(
            moves.iter().any(|(_, target)| target.as_str() == ids[1]),
            "a system that contains the owner's units"
        );
        assert!(
            moves
                .iter()
                .all(|(_, target)| !state.ingress_tokens.contains(target)),
            "never into a system that holds one (rule 14)"
        );
    }

    #[test]
    fn planesplitter_is_a_may_for_its_owner_alone_and_needs_the_fracture() {
        let (state, galaxy, ids) = splitter();
        let strategic = [("player", "a".into())];
        let mut declined = state.clone();
        emit(
            &mut declined,
            Some(&galaxy),
            &["decline"],
            "STRATEGIC_ACTION_BEGAN",
            &strategic,
        );
        assert_eq!(declined.ingress_tokens, state.ingress_tokens);
        // Another player's strategic action is not the owner's.
        let mut other = state.clone();
        emit(
            &mut other,
            Some(&galaxy),
            &[],
            "STRATEGIC_ACTION_BEGAN",
            &[("player", "b".into())],
        );
        assert_eq!(other.ingress_tokens, state.ingress_tokens);
        // No Fracture, no ingress tokens to move.
        let mut absent = state.clone();
        absent.fracture_in_play = false;
        assert!(ingress_moves(&absent, content(), DEFAULT, Some(&galaxy), &a()).is_empty());
        // No card, no ability.
        let mut bare = state.clone();
        bare.player_mut(&a()).unwrap().technologies.clear();
        emit(
            &mut bare,
            Some(&galaxy),
            &[],
            "STRATEGIC_ACTION_BEGAN",
            &strategic,
        );
        assert_eq!(bare.ingress_tokens, state.ingress_tokens);
        // Without a map only a system that holds the owner's units qualifies.
        let near = ingress_moves(&state, content(), DEFAULT, None, &a());
        assert!(
            near.iter()
                .all(|(_, target)| { target.as_str() == ids[1] || target.as_str() == "96b" })
        );
    }

    // -- Neural Parasite (Obsidian) --------------------------------------------------------------

    fn parasite_game() -> (GameState, Galaxy, Vec<String>) {
        let (ids, galaxy) = map();
        let mut state = game();
        give_tech(&mut state, PARASITE);
        let mine = SystemId::new(ids[1].as_str());
        crate::fixtures::put_on_planet(&mut state, &mine, &planet_of(&ids[1]), "infantry", &a(), 1);
        (state, galaxy, ids)
    }

    fn infantry_of(state: &GameState, system: &str, who: &PlayerId) -> usize {
        let board = state.system_state(&SystemId::new(system));
        board
            .planet_units
            .values()
            .flatten()
            .chain(board.units.iter())
            .filter(|unit| &unit.owner == who && unit.type_id.as_str().contains("infantry"))
            .count()
    }

    #[test]
    fn neural_parasite_destroys_an_adjacent_players_infantry_at_the_start_of_the_owners_turn() {
        let (mut state, galaxy, ids) = parasite_game();
        let next = SystemId::new(ids[2].as_str());
        crate::fixtures::put_on_planet(&mut state, &next, &planet_of(&ids[2]), "infantry", &b(), 2);
        let turn = [("player", "a".into())];
        emit(&mut state, Some(&galaxy), &[], "TURN_BEGAN", &turn);
        assert_eq!(
            infantry_of(&state, &ids[2], &b()),
            1,
            "exactly one is destroyed"
        );
        assert_eq!(
            infantry_of(&state, &ids[1], &a()),
            1,
            "its own are never targets"
        );
        assert!(
            super::super::hooks_ground::has_staged_events(&state),
            "the destruction is announced like any other"
        );
        // Not at the start of someone else's turn.
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "b".into())],
        );
        assert_eq!(state.board, before.board);
    }

    #[test]
    fn neural_parasite_lets_its_owner_choose_among_several_players_infantry() {
        let (mut state, galaxy, ids) = parasite_game();
        for (index, who) in [(2, b()), (6, c())] {
            let system = SystemId::new(ids[index].as_str());
            crate::fixtures::put_on_planet(
                &mut state,
                &system,
                &planet_of(&ids[index]),
                "infantry",
                &who,
                1,
            );
            assert!(galaxy.are_adjacent(&ids[1], &ids[index]), "{index}");
        }
        let pick = format!("{}|{}|c|infantry", ids[6], planet_of(&ids[6]));
        emit(
            &mut state,
            Some(&galaxy),
            &[pick.as_str()],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(infantry_of(&state, &ids[6], &c()), 0);
        assert_eq!(infantry_of(&state, &ids[2], &b()), 1);
    }

    #[test]
    fn neural_parasite_needs_infantry_of_the_owner_nearby_and_a_target() {
        let (state, galaxy, ids) = parasite_game();
        // Infantry two systems away from the owner's are out of reach.
        let mut far = state.clone();
        let across = SystemId::new(ids[4].as_str());
        crate::fixtures::put_on_planet(&mut far, &across, &planet_of(&ids[4]), "infantry", &b(), 1);
        assert!(!galaxy.are_adjacent(&ids[1], &ids[4]));
        emit(
            &mut far,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(infantry_of(&far, &ids[4], &b()), 1);
        // The same system counts as "in".
        let mut here = state.clone();
        let mine = SystemId::new(ids[1].as_str());
        let planet = planet_of(&ids[1]);
        crate::fixtures::put_on_planet(&mut here, &mine, &planet, "infantry", &b(), 1);
        emit(
            &mut here,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(infantry_of(&here, &ids[1], &b()), 0);
        // Without the card nothing happens, whatever stands there.
        let mut bare = state.clone();
        bare.player_mut(&a()).unwrap().technologies.clear();
        crate::fixtures::put_on_planet(&mut bare, &mine, &planet, "infantry", &b(), 1);
        emit(
            &mut bare,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(infantry_of(&bare, &ids[1], &b()), 1);
        // Only infantry: a mech beside it is untouched.
        let mut mech = state.clone();
        crate::fixtures::put_on_planet(&mut mech, &mine, &planet, "mech", &b(), 1);
        emit(
            &mut mech,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        assert_eq!(
            mech.system_state(&mine).on_planet_of(&planet, &b()).len(),
            1
        );
    }

    #[test]
    fn the_obsidian_technologies_cannot_be_researched_by_anyone() {
        let mut state = game();
        for tech in [PLANESPLITTER, PARASITE] {
            let id = TechnologyId::new(tech);
            assert!(
                !crate::technology::can_research(&state, content(), DEFAULT, &a(), &id),
                "{tech}"
            );
            assert!(
                !crate::technology::researchable(&state, content(), DEFAULT, &a()).contains(&id)
            );
        }
        state.player_mut(&b()).unwrap().technologies.clear();
        assert!(
            !crate::technology::researchable(&state, content(), DEFAULT, &b())
                .iter()
                .any(|id| id.as_str().ends_with("-obs"))
        );
    }

    // -- Heaven's Hollow and Viper Hollow ---------------------------------------------------------

    #[test]
    fn heavens_hollow_is_statistics_only() {
        let types = ti4_content::units::catalogue(content(), DEFAULT);
        let flagship = types.get(FLAGSHIP).expect("in the catalogue");
        assert!(flagship.is_ship() && flagship.sustain_damage());
        assert_eq!(
            (
                flagship.move_value(),
                flagship.capacity(),
                flagship.combat_hits_on(),
                flagship.combat_dice()
            ),
            (1, 3, Some(5), 3)
        );
        assert!((flagship.cost() - 8.0).abs() < f64::EPSILON);
        assert!(
            content()
                .get(ContentType::Units, FLAGSHIP)
                .and_then(|record| record.text("ability"))
                .is_none(),
            "it prints no ability, so there is no text for a Nekro Z token to lend"
        );
        let mech = types.get(MECH).expect("in the catalogue");
        assert!(mech.is_ground_force() && mech.sustain_damage());
        assert_eq!((mech.combat_hits_on(), mech.combat_dice()), (Some(6), 1));
    }

    #[test]
    fn a_viper_that_was_not_coexisting_changes_nothing_when_the_card_flips() {
        let (mut state, galaxy) = firmament();
        let old_home = home(&state, &a());
        let cronos = PlanetId::new("cronos");
        crate::fixtures::put_on_planet(&mut state, &old_home, &cronos, "firmament_mech", &a(), 1);
        flip(&mut state, &galaxy);
        let hollow = PlanetId::new("cronoshollow");
        assert_eq!(flush(&mut state, &galaxy, &[]), 1);
        let board = state.system_state(&SystemId::new("96b"));
        assert_eq!(board.planet_control.get(&hollow), Some(&a()));
        assert!(board.coexisting.get(&hollow).is_none_or(BTreeSet::is_empty));
        assert!(!state.exhausted_planets.contains(&hollow));
        // The ability is the flip's alone: a second announcement does nothing either.
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            FLIPPED,
            &[
                ("player", "a".into()),
                ("from", "firmament".into()),
                ("to", "obsidian".into()),
            ],
        );
        assert_eq!(state.board, before.board);
    }

    // -- Vos Hollow ------------------------------------------------------------------------------

    fn lost(system: &SystemId, victim: &str, kind: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("player", victim.into()),
            ("unit", kind.into()),
            ("last", false.into()),
            ("cause", "space_combat".into()),
            ("during_space_combat", true.into()),
        ]
    }

    fn arena() -> (GameState, SystemId) {
        let mut state = game();
        let system = SystemId::new("18");
        crate::fixtures::put(&mut state, &system, "cruiser", &c(), 1);
        (state, system)
    }

    #[test]
    fn vos_hollow_makes_the_victims_opponent_destroy_a_ship_of_the_same_type() {
        let (mut state, system) = arena();
        emit(
            &mut state,
            None,
            &[AGENT_WINDOW],
            "SHIP_DESTROYED",
            &lost(&system, "b", "cruiser"),
        );
        assert!(
            crate::combat::ships_of(&state, content(), DEFAULT, &c(), &system).is_empty(),
            "the opponent's cruiser is gone"
        );
        assert_eq!(
            state.pending_destructions.len(),
            1,
            "and announced like any destruction"
        );
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
        // Once: exhausted, it is not offered again.
        let (mut again, system) = arena();
        again
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        emit(
            &mut again,
            None,
            &[],
            "SHIP_DESTROYED",
            &lost(&system, "b", "cruiser"),
        );
        assert_eq!(
            crate::combat::ships_of(&again, content(), DEFAULT, &c(), &system).len(),
            1
        );
    }

    #[test]
    fn vos_hollow_is_a_may_and_needs_a_matching_ship_in_a_combat() {
        let (state, system) = arena();
        let mut declined = state.clone();
        emit(
            &mut declined,
            None,
            &["decline"],
            "SHIP_DESTROYED",
            &lost(&system, "b", "cruiser"),
        );
        assert_eq!(
            crate::combat::ships_of(&declined, content(), DEFAULT, &c(), &system).len(),
            1
        );
        assert_eq!(
            leader_status(&declined, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        // The opponent has no ship of that type.
        let mut other = state.clone();
        emit(
            &mut other,
            None,
            &[],
            "SHIP_DESTROYED",
            &lost(&system, "b", "destroyer"),
        );
        assert_eq!(
            leader_status(&other, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        // A ship destroyed outside a combat does not count.
        let mut outside = state.clone();
        let mut payload = lost(&system, "b", "cruiser");
        payload.retain(|(key, _)| *key != "during_space_combat");
        payload.push(("during_space_combat", false.into()));
        emit(&mut outside, None, &[], "SHIP_DESTROYED", &payload);
        assert_eq!(
            leader_status(&outside, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
    }

    #[test]
    fn vos_hollow_lets_the_opponent_pick_which_ship_and_the_owner_pick_which_opponent() {
        let (mut state, system) = arena();
        let mut damaged = unit("cruiser", &c());
        damaged.sustained_damage = true;
        state.system_mut(&system).units.push(damaged);
        // `c` has an undamaged and a damaged cruiser and chooses; ships list in board order.
        emit(
            &mut state,
            None,
            &[AGENT_WINDOW, "ship|1"],
            "SHIP_DESTROYED",
            &lost(&system, "b", "cruiser"),
        );
        let left = crate::combat::ships_of(&state, content(), DEFAULT, &c(), &system);
        assert_eq!(left.len(), 1);
        assert!(
            !left[0].sustained_damage,
            "the damaged one was the one destroyed"
        );
        // Two opponents of the victim: the agent's owner names whose ship goes.
        let (mut state, system) = arena();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        emit(
            &mut state,
            None,
            &[AGENT_WINDOW, "a"],
            "SHIP_DESTROYED",
            &lost(&system, "b", "cruiser"),
        );
        assert!(crate::combat::ships_of(&state, content(), DEFAULT, &a(), &system).is_empty());
        assert_eq!(
            crate::combat::ships_of(&state, content(), DEFAULT, &c(), &system).len(),
            1
        );
    }

    // -- Aroz Hollow -----------------------------------------------------------------------------

    fn fracture_planet() -> (SystemId, PlanetId) {
        crate::fracture::systems(content(), DEFAULT)
            .into_iter()
            .find_map(|system| {
                ti4_content::galaxy::system(content(), system.as_str(), DEFAULT)
                    .and_then(|tile| tile.planets().first().map(|planet| PlanetId::new(*planet)))
                    .map(|planet| (system, planet))
            })
            .expect("a Fracture system with a planet")
    }

    #[test]
    fn aroz_hollow_unlocks_with_units_in_the_fracture() {
        let mut state = game();
        let (system, _) = fracture_planet();
        crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Locked)
        );
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Unlocked)
        );
        // Another player's presence there does not unlock it.
        let mut other = game();
        crate::fixtures::put(&mut other, &system, "cruiser", &b(), 1);
        crate::leaders::check_unlocks(&mut other, content(), DEFAULT, None, &a());
        assert_eq!(
            leader_status(&other, &a(), COMMANDER),
            Some(LeaderStatus::Locked)
        );
    }

    #[test]
    fn aroz_hollow_adds_one_to_space_and_ground_rolls_in_the_fracture_only() {
        let (system, planet) = fracture_planet();
        let mut state = game();
        state.fracture_in_play = true;
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(COMMANDER), LeaderStatus::Unlocked);
        let cruiser = unit("cruiser", &a());
        let needed = |state: &GameState, at: &SystemId| {
            let mut state = state.clone();
            state.active_system = Some(at.clone());
            crate::combat::effective_hits_on(&state, content(), DEFAULT, &a(), &cruiser).unwrap()
        };
        let ordinary = SystemId::new("18");
        assert_eq!(
            needed(&state, &system),
            needed(&state, &ordinary) - 1,
            "space"
        );
        let infantry = |state: &GameState, at: &SystemId, on: &PlanetId| {
            crate::invasion::ground_combat_value(
                state,
                content(),
                DEFAULT,
                &a(),
                at,
                on,
                "infantry",
            )
            .unwrap()
        };
        let elsewhere = PlanetId::new("mecatol");
        assert_eq!(
            infantry(&state, &system, &planet),
            infantry(&state, &ordinary, &elsewhere) - 1,
            "ground"
        );
        // Locked, it gives nothing; another player's rolls are never improved.
        let mut locked = state.clone();
        locked
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(COMMANDER), LeaderStatus::Locked);
        assert_eq!(needed(&locked, &system), needed(&locked, &ordinary));
        let theirs = crate::invasion::ground_combat_value(
            &state,
            content(),
            DEFAULT,
            &b(),
            &system,
            &planet,
            "infantry",
        );
        let theirs_elsewhere = crate::invasion::ground_combat_value(
            &state,
            content(),
            DEFAULT,
            &b(),
            &ordinary,
            &elsewhere,
            "infantry",
        );
        assert_eq!(theirs, theirs_elsewhere);
    }

    // -- Sharsiss Hollow -------------------------------------------------------------------------

    fn hero_game() -> GameState {
        let mut state = game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
        state
            .exhausted_planets
            .insert(PlanetId::new("cronoshollow"));
        state
            .exhausted_planets
            .insert(PlanetId::new("tallinhollow"));
        state
    }

    #[test]
    fn sharsiss_hollow_readies_every_planet_and_is_purged() {
        let mut state = hero_game();
        let elsewhere = PlanetId::new("mecatol");
        state
            .system_mut(&SystemId::new("18"))
            .set_control(elsewhere.clone(), a());
        state.exhausted_planets.insert(elsewhere.clone());
        state.exhausted_planets.insert(PlanetId::new("jord"));
        assert!(crate::leaders::usable(&state, content(), &a()).contains(&LeaderId::new(HERO)));
        let mut table = scripted(&[]);
        let used =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
                crate::leaders::use_leader(context, &a(), &LeaderId::new(HERO))
            });
        assert!(used);
        assert!(
            !state
                .exhausted_planets
                .contains(&PlanetId::new("cronoshollow"))
        );
        assert!(
            !state
                .exhausted_planets
                .contains(&PlanetId::new("tallinhollow"))
        );
        assert!(!state.exhausted_planets.contains(&elsewhere));
        assert!(
            state.exhausted_planets.contains(&PlanetId::new("jord")),
            "only its own planets"
        );
        assert_eq!(
            leader_status(&state, &a(), HERO),
            Some(LeaderStatus::Purged)
        );
    }

    #[test]
    fn sharsiss_hollow_is_offered_only_when_unlocked_and_something_is_exhausted() {
        assert!(!crate::leaders::usable(&game(), content(), &a()).contains(&LeaderId::new(HERO)));
        let mut ready = hero_game();
        ready.exhausted_planets.clear();
        assert_eq!(
            crate::factions::leader_action(&ready, content(), &a(), &LeaderId::new(HERO)),
            Some(false),
            "nothing to ready"
        );
        assert_eq!(
            crate::factions::leader_action(&hero_game(), content(), &a(), &LeaderId::new(HERO)),
            Some(true)
        );
        // Three scored objectives unlock it through the shared check.
        let mut state = game();
        for objective in ["fwm", "eap", "btv"] {
            state.record_score(&a(), ObjectiveId::new(objective));
        }
        crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert_eq!(
            leader_status(&state, &a(), HERO),
            Some(LeaderStatus::Unlocked)
        );
    }

    // -- Malevolency -----------------------------------------------------------------------------

    /// The Obsidian's cruiser and Sol's destroyer in neighbouring ring systems.
    fn neighbours() -> (GameState, Galaxy) {
        let (ids, galaxy) = map();
        let mut state = game();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(ids[1].as_str()),
            "cruiser",
            &a(),
            1,
        );
        crate::fixtures::put(
            &mut state,
            &SystemId::new(ids[2].as_str()),
            "destroyer",
            &b(),
            1,
        );
        state.player_mut(&a()).unwrap().trade_goods = 1;
        (state, galaxy)
    }

    fn ended(who: &str) -> [(&'static str, serde_json::Value); 2] {
        [("player", who.into()), ("system", "18".into())]
    }

    #[test]
    fn malevolency_is_given_to_a_neighbour_for_one_influence_even_by_the_obsidian() {
        let (mut state, galaxy) = neighbours();
        assert!(listens_for_tactical_end(&state));
        emit(
            &mut state,
            Some(&galaxy),
            &[GIVE_WINDOW, "b"],
            "TACTICAL_ACTION_ENDED",
            &ended("a"),
        );
        assert_eq!(malevolency_holder(&state), Some(b()));
        assert_eq!(goods(&state, &a()), 0, "one trade good paid the influence");
    }

    #[test]
    fn malevolency_is_a_may_that_needs_a_neighbour_and_the_influence() {
        let (state, galaxy) = neighbours();
        let mut declined = state.clone();
        emit(
            &mut declined,
            Some(&galaxy),
            &["decline"],
            "TACTICAL_ACTION_ENDED",
            &ended("a"),
        );
        assert_eq!(malevolency_holder(&declined), Some(a()));
        assert_eq!(goods(&declined, &a()), 1);
        // Broke: nothing to pay with, no offer.
        let mut broke = state.clone();
        broke.player_mut(&a()).unwrap().trade_goods = 0;
        emit(
            &mut broke,
            Some(&galaxy),
            &[],
            "TACTICAL_ACTION_ENDED",
            &ended("a"),
        );
        assert_eq!(malevolency_holder(&broke), Some(a()));
        // Only at the end of the holder's own tactical action.
        let mut theirs = state.clone();
        emit(
            &mut theirs,
            Some(&galaxy),
            &[],
            "TACTICAL_ACTION_ENDED",
            &ended("b"),
        );
        assert_eq!(malevolency_holder(&theirs), Some(a()));
        // Without a map there are no neighbours.
        let mut lost = state.clone();
        emit(&mut lost, None, &[], "TACTICAL_ACTION_ENDED", &ended("a"));
        assert_eq!(malevolency_holder(&lost), Some(a()));
    }

    #[test]
    fn a_holder_who_is_not_the_obsidian_loses_a_fleet_token_at_the_end_of_the_status_phase() {
        let mut state = game();
        state
            .promissory_notes
            .insert(crate::promissory::note_id(NOTE, FACTION), b());
        let fleet = |state: &GameState, who: &PlayerId| state.player(who).unwrap().fleet_tokens;
        let before = (fleet(&state, &b()), fleet(&state, &a()));
        emit(&mut state, None, &[], "STATUS_PHASE_ENDED", &[]);
        assert_eq!(fleet(&state, &b()), before.0 - 1);
        assert_eq!(fleet(&state, &a()), before.1, "nobody else pays");
        // The Obsidian holding it pays nothing.
        let mut own = game();
        emit(&mut own, None, &[], "STATUS_PHASE_ENDED", &[]);
        assert_eq!(fleet(&own, &a()), before.1);
        // An empty fleet pool has nothing to give up.
        let mut empty = state.clone();
        empty.player_mut(&b()).unwrap().fleet_tokens = 0;
        emit(&mut empty, None, &[], "STATUS_PHASE_ENDED", &[]);
        assert_eq!(fleet(&empty, &b()), 0);
    }

    #[test]
    fn the_game_announces_the_end_of_a_tactical_action_to_malevolency() {
        let (ids, galaxy) = map();
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        let mine = SystemId::new(ids[1].as_str());
        crate::fixtures::put(&mut state, &mine, "cruiser", &a(), 1);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(ids[2].as_str()),
            "destroyer",
            &b(),
            1,
        );
        state.player_mut(&a()).unwrap().trade_goods = 1;
        let table = scripted(&[
            crate::game::TACTICAL_ACTION_ID,
            ids[3].as_str(),
            "done_moving",
            GIVE_WINDOW,
            "b",
        ]);
        let mut game = crate::game::Game::with_table(state, content(), table)
            .with_galaxy(galaxy)
            .with_sources(DEFAULT);
        for _ in 0..60 {
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
        assert_eq!(
            malevolency_holder(&game.state),
            Some(b()),
            "{:?}",
            game.table.log.records
        );
    }

    // -- neutrality and views --------------------------------------------------------------------

    #[test]
    fn a_game_without_the_obsidian_is_untouched_by_every_window() {
        let mut state =
            crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "xxcha")], DEFAULT);
        let (ids, galaxy) = map();
        crate::fixtures::put_on_planet(
            &mut state,
            &SystemId::new(ids[1].as_str()),
            &planet_of(&ids[1]),
            "infantry",
            &a(),
            1,
        );
        crate::fixtures::put_on_planet(
            &mut state,
            &SystemId::new(ids[2].as_str()),
            &planet_of(&ids[2]),
            "infantry",
            &b(),
            1,
        );
        state.fracture_in_play = true;
        state.ingress_tokens.insert(SystemId::new(ids[4].as_str()));
        let before = state.clone();
        assert!(!listens_for_tactical_end(&state));
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "STRATEGIC_ACTION_BEGAN",
            &[("player", "a".into())],
        );
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "TURN_BEGAN",
            &[("player", "a".into())],
        );
        emit(&mut state, Some(&galaxy), &[], "STATUS_PHASE_BEGAN", &[]);
        emit(&mut state, Some(&galaxy), &[], "STATUS_PHASE_ENDED", &[]);
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "TACTICAL_ACTION_ENDED",
            &ended("a"),
        );
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "SHIP_DESTROYED",
            &lost(&SystemId::new("18"), "b", "cruiser"),
        );
        won_space(&mut state, "a", &["b"]);
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            "GROUND_COMBAT_ENDED",
            &ground(Some("a"), "a", "b"),
        );
        emit(
            &mut state,
            Some(&galaxy),
            &[],
            FLIPPED,
            &[
                ("player", "a".into()),
                ("from", "firmament".into()),
                ("to", "obsidian".into()),
            ],
        );
        assert_eq!(state.board, before.board);
        assert_eq!(state.faction_marks, before.faction_marks);
        assert_eq!(state.ingress_tokens, before.ingress_tokens);
        assert_eq!(state.promissory_notes, before.promissory_notes);
        for seat in &state.players {
            assert_eq!(Some(seat), before.player(&seat.id));
        }
    }

    #[test]
    fn once_flipped_the_plots_and_the_reapings_goods_are_public() {
        let (mut state, galaxy) = firmament();
        firmament::add_goods_to_card(&mut state, &a(), 2);
        let facedown = ti4_model::view::view_for(&state, &b());
        assert_eq!(
            facedown.player(&a()).unwrap().plots,
            ["d:?:b", "d:?:c"],
            "the tokens are public, which card each is is not"
        );
        flip(&mut state, &galaxy);
        let view = ti4_model::view::view_for(&state, &b());
        assert_eq!(
            view.player(&a()).unwrap().plots,
            ["u:enervate:b", "u:siphon:c"]
        );
        assert!(ti4_model::view::leaks(&view, &b()).is_empty());
        assert_eq!(
            view.faction_marks
                .get("firmament:sowing:a")
                .map(String::as_str),
            Some("2"),
            "the goods on the card are open information"
        );
        let _ = ContentStore::embedded();
    }

    // -- the plot cards (operator rulings 2026-10-08) ----------------------------------------------

    /// `who` controls the planet of ring system `ids[index]` and has `units` on it.
    fn hold(
        state: &mut GameState,
        ids: &[String],
        index: usize,
        who: &PlayerId,
        units: &[(&str, usize)],
    ) -> (SystemId, PlanetId) {
        let system = SystemId::new(ids[index].as_str());
        let planet = planet_of(&ids[index]);
        state
            .system_mut(&system)
            .set_control(planet.clone(), who.clone());
        for (kind, count) in units {
            crate::fixtures::put_on_planet(state, &system, &planet, kind, who, *count);
        }
        (system, planet)
    }

    fn units_on(state: &GameState, system: &SystemId, planet: &PlanetId) -> usize {
        state.system_state(system).on_planet(planet).len()
    }

    fn all_infantry_of(state: &GameState, who: &PlayerId) -> usize {
        state
            .board
            .values()
            .flat_map(|board| {
                board
                    .units
                    .iter()
                    .chain(board.planet_units.values().flatten())
            })
            .filter(|unit| &unit.owner == who && unit.type_id.as_str() == "infantry")
            .count()
    }

    /// An Obsidian seat (`a`, seated directly) with the given faceup plot cards (card, token).
    fn obsidian_with(cards: &[(&str, &str)]) -> GameState {
        let mut state = game();
        for (card, token) in cards {
            firmament::testkit::place_card(&mut state, &a(), card, &PlayerId::new(*token));
        }
        state
    }

    /// The Obsidian (`a`) can pay only with `goods` trade goods: every planet it controls is exhausted.
    fn pays_with_goods_only(state: &mut GameState, goods: i32) {
        let planets: Vec<PlanetId> = state
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        state.exhausted_planets.extend(planets);
        state.player_mut(&a()).unwrap().trade_goods = goods;
    }

    /// Replace the technologies of `who` with exactly `techs`.
    fn owns_only(state: &mut GameState, who: &PlayerId, techs: &[&str]) {
        let seat = state.player_mut(who).unwrap();
        seat.technologies = techs.iter().map(|tech| TechnologyId::new(*tech)).collect();
    }

    #[test]
    fn seethe_destroys_every_unit_on_a_non_home_planet_when_a_real_flip_reveals_it() {
        let (mut state, galaxy) = plotted(&[("seethe", "b")]);
        let ids = plain(6);
        let (system, planet) = hold(&mut state, &ids, 2, &b(), &[("infantry", 2), ("pds", 1)]);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &c(), 1);
        let b_home = home(&state, &b());
        let home_before = state.system_state(&b_home).clone();
        // Facedown it does nothing; the flip reveals it and announces the arrival.
        assert_eq!(units_on(&state, &system, &planet), 4);
        flip(&mut state, &galaxy);
        assert_eq!(units_on(&state, &system, &planet), 4, "not until announced");
        flush(&mut state, &galaxy, &[]);
        assert_eq!(
            units_on(&state, &system, &planet),
            0,
            "all units on the planet, whoever's they are"
        );
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b()),
            "the planet stays the puppeted player's"
        );
        assert_eq!(
            state.system_state(&b_home),
            home_before,
            "a home planet is not touched"
        );
    }

    #[test]
    fn seethe_lets_its_owner_choose_the_planet_and_a_puppeted_player_with_none_loses_nothing() {
        let (mut state, galaxy) = plotted(&[("seethe", "b")]);
        let ids = plain(6);
        let (s2, p2) = hold(&mut state, &ids, 2, &b(), &[("infantry", 1)]);
        let (s3, p3) = hold(&mut state, &ids, 3, &b(), &[("infantry", 1)]);
        flip(&mut state, &galaxy);
        flush(&mut state, &galaxy, &[&format!("{s3}|{p3}")]);
        assert_eq!(units_on(&state, &s3, &p3), 0, "the chosen planet");
        assert_eq!(units_on(&state, &s2, &p2), 1, "the other one stands");
        // No non-home planet: nothing is asked and nothing happens.
        let (mut bare, galaxy) = plotted(&[("seethe", "b")]);
        flip(&mut bare, &galaxy);
        let before = bare.board.clone();
        flush(&mut bare, &galaxy, &[]);
        assert_eq!(bare.board, before);
    }

    #[test]
    fn seethe_destroys_one_infantry_of_each_puppeted_player_at_the_start_of_the_status_phase() {
        let mut state = obsidian_with(&[("seethe", "b")]);
        assert!(firmament::add_token(&mut state, &a(), 0, &c()));
        let ids = plain(6);
        hold(&mut state, &ids, 2, &b(), &[("infantry", 2)]);
        let (s3, p3) = hold(&mut state, &ids, 3, &b(), &[("infantry", 1)]);
        hold(&mut state, &ids, 4, &c(), &[("infantry", 1)]);
        let (b_before, c_before) = (all_infantry_of(&state, &b()), all_infantry_of(&state, &c()));
        // b has infantry in several places (asked which); c has one place (forced).
        let pick = format!("{s3}|{p3}|b|infantry");
        emit(&mut state, None, &[&pick], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(all_infantry_of(&state, &b()), b_before - 1);
        assert_eq!(all_infantry_of(&state, &c()), c_before - 1);
        assert_eq!(units_on(&state, &s3, &p3), 0, "the one the owner chose");
    }

    #[test]
    fn a_facedown_seethe_does_nothing() {
        let (mut state, galaxy) = plotted(&[("seethe", "b")]);
        let ids = plain(6);
        hold(&mut state, &ids, 2, &b(), &[("infantry", 2)]);
        let before = state.board.clone();
        emit(&mut state, Some(&galaxy), &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(
            state.board, before,
            "the Firmament's plots are not revealed"
        );
    }

    #[test]
    fn extract_gains_a_non_faction_technology_the_puppeted_player_owns_when_revealed() {
        let (mut state, galaxy) = plotted(&[("extract", "b")]);
        owns_only(&mut state, &b(), &["ps", "sar", "mi"]);
        flip(&mut state, &galaxy);
        // Plasma Scoring and Self-Assembly Routines qualify; Mageon Implants is a faction technology.
        flush(&mut state, &galaxy, &["sar"]);
        let mine = &state.player(&a()).unwrap().technologies;
        assert!(mine.contains(&TechnologyId::new("sar")), "{mine:?}");
        assert!(!mine.contains(&TechnologyId::new("ps")));
        assert!(
            !mine.contains(&TechnologyId::new("mi")),
            "a faction technology"
        );
        assert!(
            state
                .player(&b())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("sar")),
            "gained, not taken"
        );
        // One candidate is gained without a question.
        let (mut single, galaxy) = plotted(&[("extract", "b")]);
        owns_only(&mut single, &b(), &["ps"]);
        flip(&mut single, &galaxy);
        flush(&mut single, &galaxy, &[]);
        assert!(
            single
                .player(&a())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("ps"))
        );
    }

    const EXTRACT_WINDOW: &str = "ability:obsidian:plot_extract:TECHNOLOGY_GAINED:after";

    fn gained(tech: &str) -> [(&'static str, serde_json::Value); 2] {
        [("player", "b".into()), ("technology", tech.into())]
    }

    #[test]
    fn extract_lets_its_owner_spend_four_resources_to_gain_what_the_puppeted_player_gains() {
        let mut state = obsidian_with(&[("extract", "b")]);
        pays_with_goods_only(&mut state, 5);
        owns_only(&mut state, &b(), &["ps"]);
        let before = goods(&state, &a());
        emit(
            &mut state,
            None,
            &[EXTRACT_WINDOW],
            "TECHNOLOGY_GAINED",
            &gained("ps"),
        );
        assert!(
            state
                .player(&a())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("ps"))
        );
        assert_eq!(goods(&state, &a()), before - 4, "four resources were spent");
        // A may: declined, nothing changes.
        let mut declined = obsidian_with(&[("extract", "b")]);
        pays_with_goods_only(&mut declined, 5);
        owns_only(&mut declined, &b(), &["ps"]);
        emit(
            &mut declined,
            None,
            &["decline"],
            "TECHNOLOGY_GAINED",
            &gained("ps"),
        );
        assert!(
            !declined
                .player(&a())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("ps"))
        );
        assert_eq!(goods(&declined, &a()), 5);
    }

    #[test]
    fn extract_is_not_offered_for_a_faction_technology_without_the_resources_or_for_a_stranger() {
        // A faction technology (no question is scripted, so an offer would fail the run).
        let mut state = obsidian_with(&[("extract", "b")]);
        pays_with_goods_only(&mut state, 5);
        owns_only(&mut state, &b(), &["mi"]);
        emit(&mut state, None, &[], "TECHNOLOGY_GAINED", &gained("mi"));
        assert!(
            !state
                .player(&a())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("mi"))
        );
        // Three resources are not four.
        let mut poor = obsidian_with(&[("extract", "b")]);
        pays_with_goods_only(&mut poor, 3);
        owns_only(&mut poor, &b(), &["ps"]);
        emit(&mut poor, None, &[], "TECHNOLOGY_GAINED", &gained("ps"));
        assert!(
            !poor
                .player(&a())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("ps"))
        );
        // c has no token on the card.
        let mut stranger = obsidian_with(&[("extract", "b")]);
        pays_with_goods_only(&mut stranger, 5);
        owns_only(&mut stranger, &c(), &["ps"]);
        emit(
            &mut stranger,
            None,
            &[],
            "TECHNOLOGY_GAINED",
            &[("player", "c".into()), ("technology", "ps".into())],
        );
        assert!(
            !stranger
                .player(&a())
                .unwrap()
                .technologies
                .contains(&TechnologyId::new("ps"))
        );
    }

    #[test]
    fn siphon_gives_the_owner_trade_goods_equal_to_the_commodities_a_puppeted_player_gains() {
        let (_, galaxy) = map();
        let mut state = obsidian_with(&[("siphon", "b")]);
        state.player_mut(&b()).unwrap().commodities = 1;
        let limit = crate::strategy_cards::commodity_limit(&state, content(), &b());
        let before = goods(&state, &a());
        // The real replenish route (the Trade card's primary and secondary).
        crate::strategy_cards::replenish(&mut state, content(), &b());
        flush(&mut state, &galaxy, &[]);
        assert_eq!(
            goods(&state, &a()),
            before + (limit - 1),
            "only the commodities actually gained"
        );
        // Nothing gained, nothing given.
        crate::strategy_cards::replenish(&mut state, content(), &b());
        let after = goods(&state, &a());
        flush(&mut state, &galaxy, &[]);
        assert_eq!(goods(&state, &a()), after);
        // A player without a token on the card is not watched.
        state.player_mut(&c()).unwrap().commodities = 0;
        crate::strategy_cards::replenish(&mut state, content(), &c());
        assert_eq!(crate::supply::staged_events(&state), 0);
        flush(&mut state, &galaxy, &[]);
        assert_eq!(goods(&state, &a()), after);
    }

    #[test]
    fn siphon_reads_the_tokens_on_its_own_card_and_not_while_facedown() {
        let (_, galaxy) = map();
        let mut state = obsidian_with(&[("siphon", "b")]);
        assert!(firmament::add_token(&mut state, &a(), 0, &c()));
        for gainer in [b(), c()] {
            state.player_mut(&gainer).unwrap().commodities = 0;
            let limit = crate::strategy_cards::commodity_limit(&state, content(), &gainer);
            let before = goods(&state, &a());
            crate::strategy_cards::replenish(&mut state, content(), &gainer);
            flush(&mut state, &galaxy, &[]);
            assert_eq!(goods(&state, &a()), before + limit, "{gainer}");
        }
        // The Firmament's facedown Siphon has no effect and stages nothing.
        let (mut firm, _) = plotted(&[("siphon", "b")]);
        firm.player_mut(&b()).unwrap().commodities = 0;
        crate::strategy_cards::replenish(&mut firm, content(), &b());
        assert_eq!(crate::supply::staged_events(&firm), 0);
    }

    #[test]
    fn assail_adds_one_to_combat_rolls_against_a_puppeted_player_only() {
        let (system, planet) = super::super::deepwrought::testkit::plain_planet();
        let setup = |token: &str| {
            let mut state = obsidian_with(&[("assail", token)]);
            crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
            crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
            state.active_system = Some(system.clone());
            state
        };
        let space = |state: &GameState| {
            crate::combat::effective_hits_on(
                state,
                content(),
                DEFAULT,
                &a(),
                &Unit::new(UnitTypeId::new("cruiser"), a()),
            )
        };
        let ground = |state: &GameState| {
            crate::invasion::ground_combat_value(
                state,
                content(),
                DEFAULT,
                &a(),
                &system,
                &planet,
                "infantry",
            )
        };
        let (against_b, against_c) = (setup("b"), setup("c"));
        assert_eq!(
            space(&against_c).unwrap() - 1,
            space(&against_b).unwrap(),
            "+1 to the result is one fewer to hit on"
        );
        assert_eq!(ground(&against_c).unwrap() - 1, ground(&against_b).unwrap());
        // The other side's rolls are not helped.
        let other = crate::combat::effective_hits_on(
            &against_b,
            content(),
            DEFAULT,
            &b(),
            &Unit::new(UnitTypeId::new("cruiser"), b()),
        );
        let plain = crate::combat::effective_hits_on(
            &against_c,
            content(),
            DEFAULT,
            &b(),
            &Unit::new(UnitTypeId::new("cruiser"), b()),
        );
        assert_eq!(other, plain);
    }

    #[test]
    fn assail_adds_one_to_unit_ability_rolls_too() {
        let (system, planet) = super::super::deepwrought::testkit::plain_planet();
        let shots = |token: &str| -> usize {
            let mut state = obsidian_with(&[("assail", token)]);
            state.system_mut(&system).set_control(planet.clone(), a());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &a(), 1);
            crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
            // Not Antimass Deflectors' -1, which the target's faction may start with.
            state
                .player_mut(&b())
                .unwrap()
                .technologies
                .remove(&ti4_model::id::TechnologyId::new("amd"));
            // Every die shows 5; a PDS hits on 6.
            let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(5, 12));
            let mut rng = crate::rng::GameRng::new(0);
            crate::combat::space_cannon_offense(
                &mut state,
                content(),
                DEFAULT,
                &mut dice,
                &mut rng,
                &system,
                &b(),
                None,
            )
            .into_iter()
            .filter(|(owner, _, _)| *owner == a())
            .map(|(_, hits, _)| hits)
            .sum()
        };
        assert_eq!(shots("c"), 0, "a 5 misses a PDS");
        assert!(
            shots("b") > 0,
            "a 5 plus 1 hits against the puppeted player"
        );
    }

    #[test]
    fn enervate_waives_the_token_for_a_puppeted_players_secondary_after_a_real_flip() {
        use crate::strategy::{SecondaryResolution, StrategySecondaryWindow};
        let (mut state, galaxy) = plotted(&[("enervate", "b")]);
        flip(&mut state, &galaxy);
        let politics = state
            .unclaimed_strategy_cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(content(), card.as_str()).as_deref()
                    == Some("Politics")
            })
            .cloned()
            .expect("Politics");
        state.player_mut(&a()).unwrap().strategic_tokens = 0;
        state.player_mut(&c()).unwrap().strategic_tokens = 2;
        let mut window = StrategySecondaryWindow::foreign(b(), politics.clone(), vec![a(), c()]);
        let choice = window
            .pending_choice(&state, content(), DEFAULT)
            .expect("tokenless but waived: offered");
        assert_eq!(choice.player, a());
        let waiver = choice
            .ids()
            .into_iter()
            .find(|id| id.ends_with("|enervate"))
            .map(ToOwned::to_owned)
            .expect("Enervate's waiver is offered");
        assert!(!choice.ids().contains(&"yes"), "no token to spend");
        let resolved = window
            .take_choice(
                &mut state,
                content(),
                DEFAULT,
                crate::choice::ChoiceOption::new(&waiver, crate::strategy::STRATEGY_KIND),
            )
            .unwrap();
        assert_eq!(resolved, SecondaryResolution::Followed);
        assert_eq!(state.player(&a()).unwrap().strategic_tokens, 0);
        // The next follower is not helped.
        let next = window.pending_choice(&state, content(), DEFAULT).unwrap();
        assert_eq!(next.player, c());
        assert!(next.ids().iter().all(|id| !id.ends_with("|enervate")));
        // A strategic action of a player with no token on the card is not covered.
        let mut other = StrategySecondaryWindow::foreign(c(), politics, vec![a()]);
        assert!(other.next_choice(&mut state, content(), DEFAULT).is_none());
    }

    #[test]
    fn enervate_lets_the_obsidian_perform_leadership_s_primary_instead_of_its_secondary() {
        use crate::strategy::StrategySecondaryWindow;
        let (mut state, galaxy) = plotted(&[("enervate", "b")]);
        flip(&mut state, &galaxy);
        let leadership = state
            .unclaimed_strategy_cards
            .iter()
            .find(|card| {
                crate::strategy_cards::card_name(content(), card.as_str()).as_deref()
                    == Some("Leadership")
            })
            .cloned()
            .expect("Leadership");
        let mut window = StrategySecondaryWindow::foreign(b(), leadership.clone(), vec![a(), c()]);
        let choice = window
            .pending_choice(&state, content(), DEFAULT)
            .expect("the Obsidian follows Leadership");
        assert_eq!(choice.player, a());
        assert!(
            choice
                .ids()
                .contains(&crate::factions::firmament_plots::PRIMARY_INSTEAD_ID),
            "{:?}",
            choice.ids()
        );
        assert!(
            !crate::faction_abilities::substitutes_primary(&state, content(), &a(), "Leadership"),
            "not until chosen"
        );
        window
            .take_choice(
                &mut state,
                content(),
                DEFAULT,
                crate::choice::ChoiceOption::new(
                    crate::factions::firmament_plots::PRIMARY_INSTEAD_ID,
                    crate::strategy::STRATEGY_KIND,
                ),
            )
            .unwrap();
        assert!(
            crate::faction_abilities::substitutes_primary(&state, content(), &a(), "Leadership"),
            "the game now resolves the primary for a"
        );
        assert!(!crate::faction_abilities::substitutes_primary(
            &state,
            content(),
            &c(),
            "Leadership"
        ));
        assert!(!crate::faction_abilities::substitutes_primary(
            &state,
            content(),
            &a(),
            "Diplomacy"
        ));
        // The next answer (here a's own, in a later Leadership) forgets it.
        let mut later = StrategySecondaryWindow::foreign(b(), leadership.clone(), vec![a()]);
        later
            .take_choice(
                &mut state,
                content(),
                DEFAULT,
                crate::choice::ChoiceOption::new("no", crate::strategy::STRATEGY_KIND),
            )
            .unwrap();
        assert!(
            !crate::faction_abilities::substitutes_primary(&state, content(), &a(), "Leadership"),
            "a stale choice is cleared"
        );
        // Only for a puppeted player's Leadership.
        let other = StrategySecondaryWindow::foreign(c(), leadership, vec![a()]);
        let choice = other.pending_choice(&state, content(), DEFAULT);
        assert!(choice.is_none_or(|choice| {
            !choice
                .ids()
                .contains(&crate::factions::firmament_plots::PRIMARY_INSTEAD_ID)
        }));
    }

    #[test]
    fn siphon_and_assail_take_effect_only_once_a_real_flip_has_turned_them_faceup() {
        let (mut state, galaxy) = plotted(&[("siphon", "b"), ("assail", "b")]);
        let (system, _) = super::super::deepwrought::testkit::plain_planet();
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        state.active_system = Some(system);
        let roll = |state: &GameState| {
            crate::combat::effective_hits_on(
                state,
                content(),
                DEFAULT,
                &a(),
                &Unit::new(UnitTypeId::new("cruiser"), a()),
            )
        };
        let facedown = roll(&state);
        state.player_mut(&b()).unwrap().commodities = 0;
        crate::strategy_cards::replenish(&mut state, content(), &b());
        assert_eq!(
            crate::supply::staged_events(&state),
            0,
            "facedown: no effect"
        );
        flip(&mut state, &galaxy);
        flush(&mut state, &galaxy, &[]);
        assert_eq!(
            roll(&state).unwrap(),
            facedown.unwrap() - 1,
            "Assail is faceup"
        );
        state.player_mut(&b()).unwrap().commodities = 0;
        let before = goods(&state, &a());
        crate::strategy_cards::replenish(&mut state, content(), &b());
        flush(&mut state, &galaxy, &[]);
        assert!(goods(&state, &a()) > before, "Siphon is faceup");
    }

    #[test]
    fn a_game_without_plot_effects_stages_no_commodity_events() {
        let mut state = game();
        state.player_mut(&b()).unwrap().commodities = 0;
        crate::strategy_cards::replenish(&mut state, content(), &b());
        assert_eq!(crate::supply::staged_events(&state), 0);
    }

    use ti4_content::galaxy::Galaxy;
}
