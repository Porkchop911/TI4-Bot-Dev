//! The Last Bastion (`bastion`, alias in `factions.json`). See `plans/evidence/BF-bastion.md`.
//!
//! Split for size: this file holds the shared Galvanize mechanic, Liberate, Phoenix Standard, Raise
//! the Standard, The Icon (breakthrough) and the commander's unlock; `bastion_units.rs` holds the
//! flagship, the A3 Valiance mech, the Helios docks' resource bonus, Proxima Targeting VI, Dame
//! Briar and Lyra Keen.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Liberate: "When you gain control of a planet: ready that planet if it contains a number of
//!   your infantry equal to or greater than that planet's resource value; otherwise, place 1
//!   infantry on that planet."
//! * Galvanize: "Galvanized units roll 1 additional die for combat rolls and unit abilities." and,
//!   "When a game effect instructs a player to galvanize a unit: they place a galvanize token
//!   beneath it if it does not have one." There are only 7 galvanize tokens.
//! * Phoenix Standard: "At the end of combat: you may galvanize 1 of your units that participated."
//! * Raise the Standard: "At the end of a combat: Galvanize 1 of your units that participated.
//!   Then, return this card to the Last Bastion player."
//! * The Icon (`bastionbt`): "When you produce ships, you may exhaust this card to place those ships
//!   in a system that contains 1 of your command tokens, at least 1 of your ground forces, and no
//!   other player's ships."
//! * Nip and Tuck (`bastioncommander`): unlock "There are 3 galvanized units on the game board".
//!   "Your action cards cannot be canceled by 'Sabotage' action cards. The Nekro Virus cannot place
//!   assimilator tokens on your components."
//!
//! # Galvanize, modelled once
//!
//! A galvanize token is [`Unit::galvanized`], a flag on the physical unit, so it moves with the
//! unit, survives a replay and rides along through transit, flips and sustain/repair (the model
//! carried it before this faction; the Eidolon Maximum uses it only to remember which Eidolon it
//! is). Everything that galvanizes calls [`galvanize`], and every reader asks
//! [`galvanized_on_board`] / [`tokens_left`] or reads the flag.
//!
//! **What a galvanized unit does.** It rolls one additional die for each of its combat rolls and
//! each of its unit abilities (the printed text; the content's `combat_modifiers` agree: combat
//! round, ANTI-FIGHTER BARRAGE, BOMBARDMENT and SPACE CANNON). The die is added where each roll is
//! built: `combat::fleet_groups`, `combat::roll_barrage_side`, `combat::space_cannon_offense`,
//! `invasion::roll_ground`, `invasion::roll_bombard_plan` and `invasion::space_cannon_defense`,
//! through [`extra_die`]. A unit that has no roll of that kind gains none: "1 additional die"
//! needs a die to add to.
//!
//! **When it ends.** The text states no end and no other rule gives one: the token is "beneath" the
//! unit, so it stays while the unit is on the board and goes back to the supply when the unit
//! leaves the board (destroyed, removed, returned, captured). Movement, sustain and repair keep
//! it. The 7-token supply is shared by all players: no unit can be galvanized while 7 galvanized
//! units are on the board, and an already galvanized unit cannot be galvanized again.
//!
//! **Which unit.** The board stores units as interchangeable values, so a choice names a value
//! (system, area, type, damaged) and [`galvanize`] marks one unit equal to it.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "bastion";

const LIBERATE: &str = "liberate";
const GALVANIZE: &str = "galvanize";
const PHOENIX_STANDARD: &str = "phoenixstandard";
const RAISE_THE_STANDARD: &str = "raisethestandard";
/// The Icon.
pub const BREAKTHROUGH: &str = "bastionbt";
/// Nip and Tuck.
pub const COMMANDER: &str = "bastioncommander";

/// How many galvanize tokens exist.
pub const TOKEN_SUPPLY: usize = 7;
/// Galvanized units needed on the board to unlock Nip and Tuck.
const COMMANDER_UNLOCK: usize = 3;

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[LIBERATE, GALVANIZE, PHOENIX_STANDARD],
    technologies: super::bastion_units::TECHNOLOGIES,
    units: super::bastion_units::UNITS,
    promissory: &[RAISE_THE_STANDARD],
    leaders: super::bastion_units::LEADERS,
    breakthroughs: &[BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        control_gained: Some(control_gained),
        commander_unlocked: Some(commander_unlocked),
        unit_roll_modifier: Some(super::bastion_units::unit_roll_modifier),
        ..Hooks::NONE
    },
};

/// Whether `player` plays the Last Bastion.
#[must_use]
pub fn is_bastion(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

// -- Galvanize: the shared typed state ------------------------------------------------------------

/// One unit value where it stands: the system, the planet (`None` for the space area) and the unit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Located {
    /// The system.
    pub system: SystemId,
    /// The planet, or `None` for the system's space area.
    pub spot: Option<PlanetId>,
    /// The unit, exactly as it stands.
    pub unit: Unit,
}

impl Located {
    /// Stable option id: `<system>|<planet or space>|<unit type>|<damaged or undamaged>`.
    #[must_use]
    pub fn id(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.system,
            self.spot.as_ref().map_or("space", PlanetId::as_str),
            self.unit.type_id,
            if self.unit.sustained_damage {
                "damaged"
            } else {
                "undamaged"
            }
        )
    }

    fn label(&self) -> String {
        format!(
            "galvanize the {}{} {} {}",
            if self.unit.sustained_damage {
                "damaged "
            } else {
                ""
            },
            self.unit.type_id,
            self.spot.as_ref().map_or_else(
                || "in space of".to_owned(),
                |planet| format!("on {planet} in")
            ),
            self.system
        )
    }
}

/// How many galvanized units stand on the board (space areas and planets of every system).
#[must_use]
pub fn galvanized_on_board(state: &GameState) -> usize {
    state
        .board
        .values()
        .map(|board| {
            board
                .units
                .iter()
                .chain(board.planet_units.values().flatten())
                .filter(|unit| unit.galvanized)
                .count()
        })
        .sum()
}

/// Galvanize tokens still in the supply.
#[must_use]
pub fn tokens_left(state: &GameState) -> usize {
    TOKEN_SUPPLY.saturating_sub(galvanized_on_board(state))
}

/// The extra dice a unit rolls for a roll it already makes: 1 when galvanized, else 0. Called where
/// each combat roll and unit-ability roll is built, only for a unit that has such a roll.
#[must_use]
pub fn extra_die(unit: &Unit) -> usize {
    usize::from(unit.galvanized)
}

/// "Galvanize" the unit value `at`: place a token beneath one unit equal to it. `false`, changing
/// nothing, when it is already galvanized, no token is left, or no such unit stands there.
pub fn galvanize(state: &mut GameState, at: &Located) -> bool {
    if at.unit.galvanized || tokens_left(state) == 0 {
        return false;
    }
    let Some(board) = state.board.get_mut(&at.system) else {
        return false;
    };
    let standing = match &at.spot {
        None => Some(&mut board.units),
        Some(planet) => board.planet_units.get_mut(planet),
    };
    let Some(standing) = standing else {
        return false;
    };
    let Some(index) = standing.iter().position(|unit| *unit == at.unit) else {
        return false;
    };
    standing[index] = at.unit.galvanized();
    true
}

/// `owner`'s units in `system` (space area and planets) that could take a token, one entry per
/// distinct value. Empty when no token is left.
#[must_use]
pub fn galvanizable_in_system(
    state: &GameState,
    owner: &PlayerId,
    system: &SystemId,
) -> Vec<Located> {
    if tokens_left(state) == 0 {
        return Vec::new();
    }
    let Some(board) = state.board.get(system) else {
        return Vec::new();
    };
    let mut found: std::collections::BTreeSet<Located> = std::collections::BTreeSet::new();
    for unit in board
        .units
        .iter()
        .filter(|unit| &unit.owner == owner && !unit.galvanized)
    {
        found.insert(Located {
            system: system.clone(),
            spot: None,
            unit: unit.clone(),
        });
    }
    for (planet, units) in &board.planet_units {
        for unit in units
            .iter()
            .filter(|unit| &unit.owner == owner && !unit.galvanized)
        {
            found.insert(Located {
                system: system.clone(),
                spot: Some(planet.clone()),
                unit: unit.clone(),
            });
        }
    }
    found.into_iter().collect()
}

// -- the one place the faction asks ---------------------------------------------------------------

fn decision(state: &GameState, who: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

/// Put one question to `who`. Every Last Bastion choice (which unit to galvanize, where the
/// Icon's ships go is a production option and not asked here) is asked here.
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

/// Let `who` pick one of `candidates` (asked only when they differ) and galvanize it. Returns
/// whether a unit was galvanized.
pub(crate) fn galvanize_chosen(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    card: &str,
    candidates: Vec<Located>,
) -> Result<bool, TimingError> {
    let mut candidates = candidates;
    candidates.sort();
    candidates.dedup();
    let target = match candidates.len() {
        0 => return Ok(false),
        1 => candidates.remove(0),
        _ => {
            let options = candidates
                .iter()
                .map(|found| ChoiceOption::labelled(found.id(), "galvanize", found.label()))
                .collect();
            let answer = ask(
                context,
                who,
                format!("{card}: choose the unit to galvanize"),
                card,
                "galvanize_unit",
                options,
            )?;
            let Some(found) = candidates.into_iter().find(|found| found.id() == answer.id) else {
                return Ok(false);
            };
            found
        }
    };
    Ok(galvanize(context.state, &target))
}

// -- the ships and ground forces that were in a combat --------------------------------------------

/// `player`'s units that took part in the combat `event` reports: their ships in the space area for
/// a space combat, their ground forces on the planet for a ground combat. Ships that retreated from
/// the space combat took part in it (operator ruling 2026-10-07) and stand in their retreat
/// destination; ground forces they carried did not take part in a space combat and are not
/// candidates. Fighters are ships, so a carried fighter is.
fn participants(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    event: &Event,
    player: &PlayerId,
) -> Vec<Located> {
    if event.text("attacker") != Some(player.as_str())
        && event.text("defender") != Some(player.as_str())
    {
        return Vec::new();
    }
    let Some(system) = event.text("system").map(SystemId::new) else {
        return Vec::new();
    };
    let Some(board) = state.board.get(&system) else {
        return Vec::new();
    };
    let types = ti4_content::units::catalogue(content, sources);
    let mut found: std::collections::BTreeSet<Located> = std::collections::BTreeSet::new();
    match event.event_type.as_str() {
        "SPACE_COMBAT_ENDED" => {
            for unit in board.units.iter().filter(|unit| {
                &unit.owner == player
                    && !unit.galvanized
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.is_ship())
            }) {
                found.insert(Located {
                    system: system.clone(),
                    spot: None,
                    unit: unit.clone(),
                });
            }
            // Ships that retreated from this combat took part in it: they stand in their retreat
            // destination (see [`note_retreat`]).
            if let Some((destination, kept)) = retreated_from(state, &system, player)
                && let Some(arrived) = state.board.get(&destination)
            {
                for unit in arrived.units.iter().filter(|unit| {
                    &unit.owner == player
                        && !unit.galvanized
                        && kept.contains(&(unit.type_id.to_string(), unit.sustained_damage))
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.is_ship())
                }) {
                    found.insert(Located {
                        system: destination.clone(),
                        spot: None,
                        unit: unit.clone(),
                    });
                }
            }
        }
        "GROUND_COMBAT_ENDED" => {
            let Some(planet) = event.text("planet").map(PlanetId::new) else {
                return Vec::new();
            };
            for unit in board.on_planet(&planet).iter().filter(|unit| {
                &unit.owner == player
                    && !unit.galvanized
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.is_ground_force())
            }) {
                found.insert(Located {
                    system: system.clone(),
                    spot: Some(planet.clone()),
                    unit: unit.clone(),
                });
            }
        }
        _ => {}
    }
    if tokens_left(state) == 0 {
        return Vec::new();
    }
    found.into_iter().collect()
}

// -- timing registration ---------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = Vec::new();
    for event_type in ["SPACE_COMBAT_ENDED", "GROUND_COMBAT_ENDED"] {
        abilities.push(phoenix_standard(owner_name, seat, event_type));
        abilities.push(raise_the_standard(owner_name, seat, event_type));
    }
    abilities.push(icon_readies(owner_name, seat));
    abilities.extend(super::bastion_units::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

/// Phoenix Standard: "At the end of combat: you may galvanize 1 of your units that participated."
fn phoenix_standard(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{PHOENIX_STANDARD}:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let candidates = participants(
                context.state,
                context.content,
                context.sources,
                event,
                &owner,
            );
            galvanize_chosen(context, &owner, PHOENIX_STANDARD, candidates)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        is_bastion(context.state, &condition_owner)
            && !participants(
                context.state,
                context.content,
                context.sources,
                event,
                &condition_owner,
            )
            .is_empty()
    }))
}

/// The Last Bastion seat whose Raise the Standard `holder` holds, if they do.
fn standard_owner(state: &GameState, holder: &PlayerId) -> Option<PlayerId> {
    let owner = crate::promissory::seat_of(state, FACTION)?;
    (owner != *holder
        && state
            .promissory_notes
            .get(&crate::promissory::note_id(RAISE_THE_STANDARD, FACTION))
            == Some(holder))
    .then_some(owner)
}

/// Raise the Standard: "At the end of a combat: Galvanize 1 of your units that participated. Then,
/// return this card to the Last Bastion player." Resolves whenever the holder fights (no "may");
/// the card goes home even if no token is left.
fn raise_the_standard(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (holder, condition_holder) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("promissory:{owner_name}:{RAISE_THE_STANDARD}:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if standard_owner(context.state, &holder).is_none() {
                return Ok(());
            }
            let candidates = participants(
                context.state,
                context.content,
                context.sources,
                event,
                &holder,
            );
            galvanize_chosen(context, &holder, RAISE_THE_STANDARD, candidates)?;
            crate::promissory::give_back(
                context.state,
                &crate::promissory::note_id(RAISE_THE_STANDARD, FACTION),
            );
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        standard_owner(context.state, &condition_holder).is_some()
            && (event.text("attacker") == Some(condition_holder.as_str())
                || event.text("defender") == Some(condition_holder.as_str()))
    }))
}

// -- Liberate --------------------------------------------------------------------------------------

/// Liberate: "When you gain control of a planet: ready that planet if it contains a number of your
/// infantry equal to or greater than that planet's resource value; otherwise, place 1 infantry on
/// that planet." The resource value is the planet's as it now stands (attachments, laws and the
/// Helios dock's bonus). A space station is not a planet. The infantry come from reinforcements
/// and are not placed when none are left.
fn control_gained(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) {
    if !is_bastion(state, player)
        || ti4_content::galaxy::is_space_station(content, planet.as_str(), sources)
        || state.system_state(system).planet_control.get(planet) != Some(player)
    {
        return;
    }
    let types = ti4_content::units::catalogue(content, sources);
    let infantry = state
        .system_state(system)
        .on_planet_of(planet, player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "infantry")
        })
        .count();
    let resources = crate::production::planet_value_now(
        state,
        content,
        sources,
        planet,
        crate::production::Spend::Resources,
    );
    if i64::try_from(infantry).unwrap_or(i64::MAX) >= resources {
        state.ready_planet(planet);
        return;
    }
    let Some(type_id) =
        crate::action_cards::placed_unit_id(state, content, sources, player, "infantry")
    else {
        return;
    };
    if crate::supply::allowed(state, content, sources, player, &type_id, 1) == 0 {
        return;
    }
    state
        .system_mut(system)
        .planet_units
        .entry(planet.clone())
        .or_default()
        .push(Unit::new(type_id, player.clone()));
}

// -- Nip and Tuck: unlock --------------------------------------------------------------------------

/// "There are 3 galvanized units on the game board": any player's.
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    _player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == COMMANDER).then(|| galvanized_on_board(state) >= COMMANDER_UNLOCK)
}

// -- The Icon --------------------------------------------------------------------------------------

/// `faction_marks` key: present while `player`'s Icon is exhausted.
fn icon_key(player: &PlayerId) -> String {
    format!("bastion:icon:exhausted:{player}")
}

/// The base types of ships (what "ships" means for "when you produce ships").
fn is_ship_type(content: &ContentStore, sources: SourceSet, unit: &UnitTypeId) -> bool {
    ti4_content::units::catalogue(content, sources)
        .get(unit.as_str())
        .is_some_and(|kind| kind.is_ship())
}

/// Systems other than `producing` where `player`'s Icon could place ships: it holds a command
/// token of the player, at least 1 of the player's ground forces and no other player's ships.
fn icon_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    producing: &SystemId,
) -> Vec<SystemId> {
    let types = ti4_content::units::catalogue(content, sources);
    let is_ship = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(|kind| kind.is_ship())
    };
    state
        .board
        .iter()
        .filter(|(system, board)| {
            *system != producing
                && board.command_tokens.contains(player)
                && !board
                    .units
                    .iter()
                    .any(|unit| &unit.owner != player && is_ship(unit))
                && board
                    .units
                    .iter()
                    .chain(board.planet_units.values().flatten())
                    .any(|unit| {
                        &unit.owner == player
                            && types
                                .get(unit.type_id.as_str())
                                .is_some_and(|kind| kind.is_ground_force())
                    })
        })
        .map(|(system, _)| system.clone())
        .collect()
}

/// Shape the spots a ship may be placed on in one use of PRODUCTION (called by
/// `production::ProductionWindow::spots` for a ship, when the producing system has a legal spot).
///
/// `produced` is what this use has placed so far. The Icon places "those ships": every ship of one
/// use goes to one system. Until a ship is placed, the Icon's systems are offered as remote spots
/// (`<system>@space`) while the card is ready; once a ship of this use went to an Icon system, the
/// only spot is that system; once a ship was placed in the producing system the Icon is not offered.
pub(crate) fn adjust_ship_spots(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    producing: &SystemId,
    produced: &[(UnitTypeId, String)],
    spots: &mut Vec<String>,
) {
    if !crate::breakthroughs::holds(state, player, BREAKTHROUGH) {
        return;
    }
    let placed: Vec<&String> = produced
        .iter()
        .filter(|(unit, _)| is_ship_type(content, sources, unit))
        .map(|(_, at)| at)
        .collect();
    if let Some(remote) = placed.iter().find(|at| at.contains('@')) {
        spots.clear();
        spots.push((*remote).clone());
        return;
    }
    if !placed.is_empty() || state.faction_marks.contains_key(&icon_key(player)) {
        return;
    }
    for system in icon_targets(state, content, sources, player, producing) {
        let spot = format!("{system}{}space", crate::production::REMOTE_SEPARATOR);
        if !spots.contains(&spot) {
            spots.push(spot);
        }
    }
}

/// A ship was just placed in `target` by a use of PRODUCTION in `producing`: when that is one of
/// the Icon's systems (and not the producing one), the card is exhausted. Called by
/// `production::ProductionWindow::place`.
pub(crate) fn ship_placed(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    producing: &SystemId,
    target: &SystemId,
    unit: &UnitTypeId,
) {
    if target == producing
        || !crate::breakthroughs::holds(state, player, BREAKTHROUGH)
        || state.faction_marks.contains_key(&icon_key(player))
        || !is_ship_type(content, sources, unit)
        || !icon_targets(state, content, sources, player, producing).contains(target)
    {
        return;
    }
    state.faction_marks.insert(icon_key(player), "1".to_owned());
}

/// The card readies with everything else in the status phase.
fn icon_readies(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}_ready:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&icon_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&icon_key(&condition_owner))
    }))
}

/// Whether `player`'s Icon is held and ready.
#[must_use]
pub fn icon_ready(state: &GameState, player: &PlayerId) -> bool {
    crate::breakthroughs::holds(state, player, BREAKTHROUGH)
        && !state.faction_marks.contains_key(&icon_key(player))
}

// -- galvanized units that left the board ----------------------------------------------------------

fn lost_key(system: &SystemId, owner: &PlayerId, unit: &UnitTypeId) -> String {
    format!("bastion:lost:{system}:{owner}:{unit}")
}

/// Record that a galvanized ship of `owner` was just destroyed in `system`, for the
/// `SHIP_DESTROYED` announcement that follows (the token returns to the supply with the unit; the
/// event still reports that it was galvanized). Called by `combat::remove_combat_ship`.
pub(crate) fn note_ship_lost(state: &mut GameState, system: &SystemId, unit: &Unit) {
    if !unit.galvanized {
        return;
    }
    let key = lost_key(system, &unit.owner, &unit.type_id);
    let count = state
        .faction_marks
        .get(&key)
        .and_then(|count| count.parse::<u32>().ok())
        .unwrap_or(0);
    state.faction_marks.insert(key, (count + 1).to_string());
}

/// Take one record of [`note_ship_lost`] for the announcement being built: `true` if the ship was
/// galvanized. Called by `combat::ship_destroyed_payload`.
pub(crate) fn take_ship_lost(
    state: &mut GameState,
    system: &SystemId,
    owner: &PlayerId,
    unit: &UnitTypeId,
) -> bool {
    let key = lost_key(system, owner, unit);
    let Some(count) = state
        .faction_marks
        .get(&key)
        .and_then(|count| count.parse::<u32>().ok())
    else {
        return false;
    };
    if count <= 1 {
        state.faction_marks.remove(&key);
    } else {
        state.faction_marks.insert(key, (count - 1).to_string());
    }
    true
}

// -- ships that retreated from a combat -----------------------------------------------------------

fn retreat_key(system: &SystemId, owner: &PlayerId) -> String {
    format!("bastion:retreated:{system}:{owner}")
}

/// Record that `owner`'s `ships` retreated from the space combat in `system` to `destination`, so
/// Phoenix Standard and Raise the Standard can still count them as having participated. Written
/// only in a game with a Last Bastion seat, and cleared by [`clear_retreated`] when that combat
/// ends. Called by `combat::retreat_to`.
pub(crate) fn note_retreat(
    state: &mut GameState,
    system: &SystemId,
    destination: &SystemId,
    owner: &PlayerId,
    ships: &[Unit],
) {
    if ships.is_empty() || crate::promissory::seat_of(state, FACTION).is_none() {
        return;
    }
    let mut kept: Vec<String> = ships
        .iter()
        .map(|unit| format!("{}:{}", unit.type_id, u8::from(unit.sustained_damage)))
        .collect();
    kept.sort();
    kept.dedup();
    state.faction_marks.insert(
        retreat_key(system, owner),
        format!("{destination}|{}", kept.join(",")),
    );
}

/// Forget the retreats from the combat in `system`. Called by `combat` after `SPACE_COMBAT_ENDED`.
pub(crate) fn clear_retreated(state: &mut GameState, system: &SystemId) {
    if state.faction_marks.is_empty() {
        return;
    }
    let prefix = format!("bastion:retreated:{system}:");
    state
        .faction_marks
        .retain(|key, _| !key.starts_with(&prefix));
}

/// Where `owner`'s ships went when they retreated from `system`, and which (type, damaged) values
/// they were.
fn retreated_from(
    state: &GameState,
    system: &SystemId,
    owner: &PlayerId,
) -> Option<(SystemId, std::collections::BTreeSet<(String, bool)>)> {
    let mark = state.faction_marks.get(&retreat_key(system, owner))?;
    let (destination, kept) = mark.split_once('|')?;
    let kept = kept
        .split(',')
        .filter_map(|entry| {
            let (kind, damaged) = entry.rsplit_once(':')?;
            Some((kind.to_owned(), damaged == "1"))
        })
        .collect();
    Some((SystemId::new(destination), kept))
}

/// Shared fixtures for this faction's tests (`bastion.rs`, `bastion_units.rs`).
#[cfg(test)]
pub(crate) mod testkit {
    use std::collections::BTreeMap;

    use ti4_content::ContentStore;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{PlayerId, SystemId, UnitTypeId};
    use ti4_model::state::GameState;
    use ti4_model::units::Unit;

    use crate::choice::{Scripted, Table};

    pub(crate) fn a() -> PlayerId {
        PlayerId::new("a")
    }
    pub(crate) fn b() -> PlayerId {
        PlayerId::new("b")
    }
    pub(crate) fn c() -> PlayerId {
        PlayerId::new("c")
    }
    pub(crate) fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }
    /// `a` the Last Bastion, `b` Sol, `c` Hacan.
    pub(crate) fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", "bastion"), ("b", "sol"), ("c", "hacan")], DEFAULT)
    }
    /// A system away from every home system, with its first planet.
    pub(crate) fn arena() -> (SystemId, ti4_model::id::PlanetId) {
        crate::fixtures::a_placed_planet()
    }
    pub(crate) fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    pub(crate) fn unit(kind: &str, owner: &PlayerId) -> Unit {
        Unit::new(UnitTypeId::new(kind), owner.clone())
    }
    pub(crate) fn put_galvanized(
        state: &mut GameState,
        system: &SystemId,
        kind: &str,
        owner: &PlayerId,
        count: usize,
    ) {
        for _ in 0..count {
            state
                .system_mut(system)
                .units
                .push(unit(kind, owner).galvanized());
        }
    }
    pub(crate) fn put_galvanized_on(
        state: &mut GameState,
        system: &SystemId,
        planet: &ti4_model::id::PlanetId,
        kind: &str,
        owner: &PlayerId,
        count: usize,
    ) {
        for _ in 0..count {
            state
                .system_mut(system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(unit(kind, owner).galvanized());
        }
    }
    pub(crate) fn galvanized_count(state: &GameState, owner: &PlayerId, kind: &str) -> usize {
        state
            .board
            .values()
            .flat_map(|board| {
                board
                    .units
                    .iter()
                    .chain(board.planet_units.values().flatten())
            })
            .filter(|u| &u.owner == owner && u.type_id.as_str() == kind && u.galvanized)
            .count()
    }

    /// Emit `kind` through a resolver armed as the game arms one, with dice that yield `faces`
    /// first. Returns the dice, for the rolls it made.
    pub(crate) fn emit_rolling(
        state: &mut GameState,
        table: &mut Table,
        faces: &[u32],
        kind: &str,
        pairs: &[(&str, serde_json::Value)],
    ) -> crate::dice::Dice {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        {
            let mut context = crate::timing::TimingContext {
                state,
                content: content(),
                sources: DEFAULT,
                table,
                dice: &mut dice,
                rng: &mut rng,
                event_sequence: &mut sequence,
                galaxy: None,
            };
            let payload: BTreeMap<String, serde_json::Value> = pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = context
                .event_sequence
                .next(kind, payload)
                .expect("an event id");
            resolver
                .emit_with_context(&mut context, event, |_, _| {})
                .expect("emits");
        }
        dice
    }
    pub(crate) fn emit(
        state: &mut GameState,
        answers: &[&str],
        kind: &str,
        pairs: &[(&str, serde_json::Value)],
    ) {
        emit_rolling(state, &mut scripted(answers), &[], kind, pairs);
    }

    /// The payload of a `SPACE_COMBAT_ENDED` between `attacker` and `defender`.
    pub(crate) fn space_ended(
        system: &SystemId,
        attacker: &str,
        defender: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("attacker", attacker.into()),
            ("defender", defender.into()),
        ]
    }
    pub(crate) fn ground_ended(
        system: &SystemId,
        planet: &ti4_model::id::PlanetId,
        attacker: &str,
        defender: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("planet", planet.to_string().into()),
            ("attacker", attacker.into()),
            ("defender", defender.into()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use ti4_model::content_types::DEFAULT;

    fn standing(state: &GameState, system: &SystemId, kind: &str, galvanized: bool) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|u| u.type_id.as_str() == kind && u.galvanized == galvanized)
            .count()
    }

    // -- Galvanize: the shared state ------------------------------------------------------------

    #[test]
    fn galvanizing_marks_one_unit_and_the_seven_tokens_run_out() {
        let mut state = game();
        let (system, _) = arena();
        crate::fixtures::put(&mut state, &system, "infantry", &a(), 8);
        let plain = || Located {
            system: system.clone(),
            spot: None,
            unit: unit("infantry", &a()),
        };
        for expected in 1..=TOKEN_SUPPLY {
            assert!(galvanize(&mut state, &plain()));
            assert_eq!(galvanized_on_board(&state), expected);
        }
        assert_eq!(tokens_left(&state), 0);
        assert_eq!(standing(&state, &system, "infantry", false), 1);
        assert!(
            !galvanize(&mut state, &plain()),
            "all 7 tokens are on the board"
        );
        assert!(
            galvanizable_in_system(&state, &a(), &system).is_empty(),
            "nothing is offered without a token"
        );
        // A unit that leaves the board takes its token back to the supply.
        state
            .system_mut(&system)
            .remove(&[unit("infantry", &a()).galvanized()]);
        assert_eq!(tokens_left(&state), 1);
        assert!(galvanize(&mut state, &plain()));
        // An already galvanized unit takes no second token.
        let again = Located {
            unit: unit("infantry", &a()).galvanized(),
            ..plain()
        };
        state
            .system_mut(&system)
            .remove(&[unit("infantry", &a()).galvanized()]);
        assert!(!galvanize(&mut state, &again));
        // And a unit that is not there takes none either.
        assert!(!galvanize(
            &mut state,
            &Located {
                unit: unit("cruiser", &a()),
                ..plain()
            }
        ));
    }

    #[test]
    fn galvanize_candidates_are_distinct_values_of_one_owner() {
        let mut state = game();
        let (system, planet) = arena();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 2);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 3);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        put_galvanized(&mut state, &system, "carrier", &a(), 1);
        let found = galvanizable_in_system(&state, &a(), &system);
        let ids: Vec<String> = found.iter().map(Located::id).collect();
        assert_eq!(
            ids.len(),
            2,
            "{ids:?}: cruisers and infantry, not the carrier"
        );
        assert!(ids.iter().any(|id| id.contains("|space|cruiser|")));
        assert!(
            ids.iter()
                .any(|id| id.contains(&format!("|{planet}|infantry|")))
        );
    }

    // -- what a galvanized unit rolls -----------------------------------------------------------------

    fn faces_of(dice: &crate::dice::Dice, reason: &str) -> Vec<usize> {
        dice.history()
            .iter()
            .filter(|roll| roll.reason == reason)
            .map(|roll| roll.faces.len())
            .collect()
    }

    #[test]
    fn a_galvanized_ship_rolls_one_extra_combat_die() {
        let mut state = game();
        let (system, _) = arena();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        put_galvanized(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 2);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        crate::combat::roll_fleet(
            &state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &a(),
            &system,
        );
        crate::combat::roll_fleet(
            &state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &b(),
            &system,
        );
        assert_eq!(
            faces_of(&dice, "space combat"),
            vec![3, 2],
            "one plain cruiser + one galvanized (2 dice) = 3; the rival's two plain cruisers = 2"
        );
    }

    #[test]
    fn a_galvanized_unit_rolls_one_extra_die_for_each_unit_ability() {
        let mut state = game();
        let (system, planet) = arena();
        // ANTI-FIGHTER BARRAGE: a destroyer rolls 2, galvanized 3.
        put_galvanized(&mut state, &system, "destroyer", &a(), 1);
        crate::fixtures::put(&mut state, &system, "destroyer", &b(), 1);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        crate::combat::roll_barrage_side(
            &mut state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &system,
            &a(),
        );
        crate::combat::roll_barrage_side(
            &mut state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &system,
            &b(),
        );
        assert_eq!(
            faces_of(&dice, "anti-fighter barrage"),
            vec![3, 2],
            "galvanized 3, plain 2"
        );
        // SPACE CANNON (offense): a PDS rolls 1, galvanized 2, at the active player's ships.
        let mut state = game();
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        state
            .system_mut(&system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(unit("pds", &a()).galvanized());
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        crate::combat::space_cannon_offense(
            &mut state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &system,
            &a(),
            None,
        );
        assert_eq!(faces_of(&dice, "space cannon"), vec![2]);
        // BOMBARDMENT: a dreadnought rolls 1, galvanized 2.
        let mut state = game();
        state.system_mut(&system).planet_units.clear();
        state.system_mut(&system).units.clear();
        put_galvanized(&mut state, &system, "dreadnought", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        state.system_mut(&system).set_control(planet, b());
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        crate::invasion::bombardment(
            &mut state,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &mut scripted(&[]),
            &system,
            &a(),
        )
        .expect("bombards");
        assert_eq!(faces_of(&dice, "bombardment"), vec![2]);
    }

    #[test]
    fn galvanized_ground_forces_roll_an_extra_die_in_ground_combat() {
        let mut state = game();
        let (system, planet) = arena();
        state.system_mut(&system).units.clear();
        state.system_mut(&system).planet_units.clear();
        put_galvanized_on(&mut state, &system, &planet, "infantry", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(1);
        crate::invasion::ground_combat(
            &mut state,
            content(),
            DEFAULT,
            &mut scripted(&[]),
            &mut dice,
            &mut rng,
            &system,
            &planet,
            &a(),
        )
        .expect("fights");
        let rolls = faces_of(&dice, "ground combat");
        assert_eq!(
            &rolls[..2],
            &[3, 1],
            "the invader's two infantry throw 3 dice (one galvanized), the defender's one"
        );
    }

    // -- Liberate -------------------------------------------------------------------------------

    fn rich_planet() -> (SystemId, PlanetId, i64) {
        let content = content();
        ti4_content::galaxy::all_planets(content, DEFAULT)
            .iter()
            .find(|(_, planet)| {
                planet.resources() == 2
                    && planet.system_id().is_some()
                    && !planet.is_placed_during_play()
                    && planet.homeworld_of().is_none()
            })
            .map(|(id, planet)| {
                (
                    SystemId::new(planet.system_id().unwrap()),
                    PlanetId::new(*id),
                    planet.resources(),
                )
            })
            .expect("a 2-resource planet")
    }

    #[test]
    fn liberate_places_one_infantry_until_they_match_the_resources_then_readies() {
        let mut state = game();
        let (system, planet, resources) = rich_planet();
        state.system_mut(&system).set_control(planet.clone(), a());
        state.exhaust_planet(planet.clone());
        let infantry = |s: &GameState| s.system_state(&system).on_planet_of(&planet, &a()).len();
        crate::faction_abilities::control_gained(
            &mut state,
            content(),
            DEFAULT,
            &a(),
            &system,
            &planet,
        );
        assert_eq!(infantry(&state), 1, "fewer than {resources}: one is placed");
        assert!(
            state.exhausted_planets.contains(&planet),
            "and it stays exhausted"
        );
        crate::faction_abilities::control_gained(
            &mut state,
            content(),
            DEFAULT,
            &a(),
            &system,
            &planet,
        );
        assert_eq!(infantry(&state), 2, "one placed again");
        crate::faction_abilities::control_gained(
            &mut state,
            content(),
            DEFAULT,
            &a(),
            &system,
            &planet,
        );
        assert_eq!(infantry(&state), 2, "enough infantry: none placed");
        assert!(
            !state.exhausted_planets.contains(&planet),
            "the planet is readied"
        );
    }

    #[test]
    fn liberate_counts_the_helios_resource_bonus_and_is_the_bastions_alone() {
        let mut state = game();
        let (system, planet, _) = rich_planet();
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "bastion_spacedock", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);
        state.exhaust_planet(planet.clone());
        crate::faction_abilities::control_gained(
            &mut state,
            content(),
            DEFAULT,
            &a(),
            &system,
            &planet,
        );
        assert!(
            state.exhausted_planets.contains(&planet),
            "the dock makes it a 3-resource planet: 2 infantry are not enough"
        );
        assert_eq!(
            state
                .system_state(&system)
                .on_planet_of(&planet, &a())
                .len(),
            4
        );
        // Another faction gaining a planet is not liberating it.
        let mut other = game();
        other.system_mut(&system).set_control(planet.clone(), b());
        other.exhaust_planet(planet.clone());
        crate::faction_abilities::control_gained(
            &mut other,
            content(),
            DEFAULT,
            &b(),
            &system,
            &planet,
        );
        assert!(other.exhausted_planets.contains(&planet));
        assert!(other.system_state(&system).on_planet(&planet).is_empty());
    }

    // -- Phoenix Standard ------------------------------------------------------------------------

    const PHOENIX_SPACE: &str = "ability:bastion:phoenixstandard:SPACE_COMBAT_ENDED:after";
    const PHOENIX_GROUND: &str = "ability:bastion:phoenixstandard:GROUND_COMBAT_ENDED:after";

    #[test]
    fn phoenix_standard_galvanizes_a_participant_at_the_end_of_a_space_combat() {
        let mut state = game();
        let (system, _) = arena();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 2);
        crate::fixtures::put(&mut state, &system, "destroyer", &a(), 1);
        let chosen = format!("{system}|space|destroyer|undamaged");
        emit(
            &mut state,
            &[PHOENIX_SPACE, &chosen],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "b", "a"),
        );
        assert_eq!(standing(&state, &system, "destroyer", true), 1);
        assert_eq!(galvanized_on_board(&state), 1, "exactly one unit");
        // "May": declining leaves everything.
        let mut declined = game();
        crate::fixtures::put(&mut declined, &system, "cruiser", &a(), 2);
        emit(
            &mut declined,
            &["decline"],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "a", "b"),
        );
        assert_eq!(galvanized_on_board(&declined), 0);
        // Someone who did not fight is not offered it.
        let mut bystander = game();
        crate::fixtures::put(&mut bystander, &system, "cruiser", &a(), 1);
        emit(
            &mut bystander,
            &[PHOENIX_SPACE],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "b", "c"),
        );
        assert_eq!(galvanized_on_board(&bystander), 0);
    }

    #[test]
    fn phoenix_standard_works_after_a_ground_combat_and_only_for_ground_forces_there() {
        let mut state = game();
        let (system, planet) = arena();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        emit(
            &mut state,
            &[PHOENIX_GROUND],
            "GROUND_COMBAT_ENDED",
            &ground_ended(&system, &planet, "a", "b"),
        );
        assert_eq!(galvanized_on_board(&state), 1);
        assert_eq!(
            standing(&state, &system, "cruiser", true),
            0,
            "the ship did not fight on the ground"
        );
        assert_eq!(
            state.system_state(&system).on_planet(&planet)[0].galvanized,
            true
        );
    }

    #[test]
    fn phoenix_standard_is_not_offered_without_a_token() {
        let mut state = game();
        let (system, _) = arena();
        crate::fixtures::put(&mut state, &system, "infantry", &c(), 7);
        for _ in 0..TOKEN_SUPPLY {
            assert!(galvanize(
                &mut state,
                &Located {
                    system: system.clone(),
                    spot: None,
                    unit: unit("infantry", &c()),
                }
            ));
        }
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        // The ability would have to be offered to be taken; with nothing offered it is skipped.
        emit(
            &mut state,
            &[PHOENIX_SPACE],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "a", "b"),
        );
        assert_eq!(galvanized_on_board(&state), TOKEN_SUPPLY);
    }

    // -- ships that retreated took part ------------------------------------------------------------

    /// A decider for a real combat: `retreater` announces a retreat, everyone else stays; any
    /// other question takes the first of `wants` offered, else declines, else the first option.
    struct Route {
        retreater: PlayerId,
        wants: Vec<String>,
    }

    impl crate::choice::Decider for Route {
        fn choose(
            &mut self,
            choice: &crate::choice::Choice,
        ) -> Result<crate::choice::ChoiceOption, crate::choice::IllegalChoice> {
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

    /// Fight a real space combat in a hub's centre, `a` and `b` one dreadnought each, with `setup`
    /// free to add more. `retreater` has a fighter in the first outer system to go to. Returns the
    /// end state and the centre and destination systems, for the first seed in which the combat
    /// ended by that player's retreat.
    fn retreat_route(
        retreater: &str,
        wants: &[String],
        setup: impl Fn(&mut GameState, &SystemId),
    ) -> (GameState, SystemId, SystemId) {
        use crate::choice::{Resolving, TimingHandle, Window};
        for seed in 0..80_u64 {
            let hub = crate::fixtures::plain_hub();
            let (system, outer) = (SystemId::new(&hub.centre), SystemId::new(&hub.outer[0]));
            let mut state = game();
            for id in std::iter::once(&hub.centre).chain(hub.outer.iter().take(1)) {
                state.board.entry(SystemId::new(id)).or_default();
            }
            crate::fixtures::put(&mut state, &system, "dreadnought", &a(), 1);
            crate::fixtures::put(&mut state, &system, "dreadnought", &b(), 1);
            crate::fixtures::put(&mut state, &outer, "fighter", &PlayerId::new(retreater), 1);
            setup(&mut state, &system);
            let mut resolver = crate::fixtures::armed_resolver(&state);
            let mut sequence = crate::event::EventSequence::new();
            let mut table = crate::choice::Table::with_default(Box::new(Route {
                retreater: PlayerId::new(retreater),
                wants: wants.to_vec(),
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
            let left = state
                .system_state(&system)
                .units
                .iter()
                .all(|unit| unit.owner.as_str() != retreater);
            let arrived = state.system_state(&outer).units.iter().any(|unit| {
                unit.owner.as_str() == retreater && unit.type_id.as_str() == "dreadnought"
            });
            if left && arrived {
                return (state, system, outer);
            }
        }
        panic!("some seed lets the retreater live to retreat");
    }

    #[test]
    fn the_last_bastion_can_galvanize_a_ship_that_retreated_from_the_combat() {
        let phoenix = PHOENIX_SPACE.to_owned();
        let (state, system, outer) = retreat_route("a", &[phoenix, "decline".into()], |_, _| {});
        // Phoenix Standard was offered at the end of the combat and a retreated ship was chosen
        // (a dreadnought or the fighter, whichever sorts first among the retreated ships).
        let retreated = state
            .system_state(&outer)
            .units
            .iter()
            .filter(|unit| unit.owner == a() && unit.galvanized)
            .count();
        assert_eq!(
            retreated, 1,
            "Phoenix Standard galvanized one retreated ship"
        );
        assert_eq!(galvanized_on_board(&state), 1);
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("bastion:retreated:")),
            "the retreat note goes with the combat: {:?} (system {system})",
            state.faction_marks
        );
    }

    #[test]
    fn a_retreated_ship_is_a_candidate_located_in_its_destination() {
        let (system, _) = arena();
        let destination = SystemId::new("1");
        let mut state = game();
        state.board.entry(destination.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system, "carrier", &a(), 1);
        crate::fixtures::put(&mut state, &system, "infantry", &a(), 2);
        crate::fixtures::put(&mut state, &system, "fighter", &a(), 1);
        let ships: Vec<Unit> = state
            .system_state(&system)
            .units
            .iter()
            .filter(|unit| unit.type_id.as_str() != "infantry")
            .cloned()
            .collect();
        note_retreat(&mut state, &system, &destination, &a(), &ships);
        crate::combat::retreat_to(&mut state, content(), DEFAULT, &a(), &system, &destination);
        let event = Event::new(1, "SPACE_COMBAT_ENDED", {
            space_ended(&system, "a", "b")
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect()
        });
        let candidates = participants(&state, content(), DEFAULT, &event, &a());
        assert!(!candidates.is_empty());
        for found in &candidates {
            assert_eq!(found.system, destination, "located where it retreated to");
            assert!(found.spot.is_none());
            assert_ne!(found.unit.type_id.as_str(), "infantry", "no ground force");
        }
        let kinds: std::collections::BTreeSet<&str> = candidates
            .iter()
            .map(|found| found.unit.type_id.as_str())
            .collect();
        assert_eq!(
            kinds,
            ["carrier", "cruiser", "fighter"].into_iter().collect(),
            "ships and the fighters they carried"
        );
        clear_retreated(&mut state, &system);
        assert!(participants(&state, content(), DEFAULT, &event, &a()).is_empty());
    }

    #[test]
    fn a_game_without_a_last_bastion_keeps_no_retreat_note() {
        let (system, _) = arena();
        let destination = SystemId::new("1");
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        state.board.entry(destination.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::combat::retreat_to(&mut state, content(), DEFAULT, &a(), &system, &destination);
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("bastion:"))
        );
    }

    // -- Raise the Standard ----------------------------------------------------------------------

    #[test]
    fn a_raise_the_standard_holder_that_retreated_galvanizes_a_retreated_ship_and_returns_it() {
        let note = crate::promissory::note_id(RAISE_THE_STANDARD, FACTION);
        let (state, _, outer) = retreat_route("b", &["decline".to_owned()], |state, _| {
            crate::promissory::take(state, content(), &b(), &note);
        });
        assert_eq!(state.promissory_notes[&note], a(), "returned home");
        let galvanized = state
            .system_state(&outer)
            .units
            .iter()
            .filter(|unit| unit.owner == b() && unit.galvanized)
            .count();
        assert_eq!(galvanized, 1, "one retreated ship, in the destination");
        assert_eq!(galvanized_on_board(&state), 1);
    }

    #[test]
    fn raise_the_standard_galvanizes_for_the_holder_and_returns_home() {
        let mut state = game();
        let (system, _) = arena();
        let note = crate::promissory::note_id(RAISE_THE_STANDARD, FACTION);
        crate::promissory::take(&mut state, content(), &b(), &note);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        // Not a participant: the card stays.
        emit(
            &mut state,
            &[],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "a", "c"),
        );
        assert_eq!(state.promissory_notes[&note], b());
        assert_eq!(galvanized_on_board(&state), 0);
        emit(
            &mut state,
            &[],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "b", "c"),
        );
        assert_eq!(standing(&state, &system, "cruiser", true), 1);
        assert_eq!(
            state.promissory_notes[&note],
            a(),
            "returned to the Last Bastion player"
        );
        // The owner holding their own note does not play it.
        let mut own = game();
        crate::fixtures::put(&mut own, &system, "cruiser", &a(), 1);
        emit(
            &mut own,
            &[PHOENIX_SPACE_DECLINE],
            "SPACE_COMBAT_ENDED",
            &space_ended(&system, "a", "b"),
        );
        assert!(
            own.promissory_notes
                .get(&note)
                .is_none_or(|holder| *holder == a())
        );
    }

    const PHOENIX_SPACE_DECLINE: &str = "decline";

    #[test]
    fn raise_the_standard_returns_even_when_no_token_is_left() {
        let mut state = game();
        let (system, planet) = arena();
        let note = crate::promissory::note_id(RAISE_THE_STANDARD, FACTION);
        crate::promissory::take(&mut state, content(), &b(), &note);
        crate::fixtures::put(&mut state, &system, "infantry", &c(), 7);
        for _ in 0..TOKEN_SUPPLY {
            galvanize(
                &mut state,
                &Located {
                    system: system.clone(),
                    spot: None,
                    unit: unit("infantry", &c()),
                },
            );
        }
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        emit(
            &mut state,
            &[],
            "GROUND_COMBAT_ENDED",
            &ground_ended(&system, &planet, "b", "c"),
        );
        assert_eq!(galvanized_on_board(&state), TOKEN_SUPPLY);
        assert_eq!(state.promissory_notes[&note], a());
    }

    // -- Nip and Tuck ----------------------------------------------------------------------------

    #[test]
    fn the_commander_unlocks_with_three_galvanized_units_of_any_player() {
        let mut state = game();
        let (system, _) = arena();
        let leader = LeaderId::new(COMMANDER);
        let ask = |s: &GameState| commander_unlocked(s, content(), DEFAULT, None, &a(), &leader);
        assert_eq!(ask(&state), Some(false));
        put_galvanized(&mut state, &system, "cruiser", &a(), 2);
        put_galvanized(&mut state, &system, "cruiser", &b(), 1);
        assert_eq!(ask(&state), Some(true), "the rival's counts");
        assert_eq!(
            commander_unlocked(&state, content(), DEFAULT, None, &a(), &LeaderId::new("x")),
            None
        );
        let unlocked = crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert!(unlocked.contains(&leader));
    }

    #[test]
    fn the_commander_shields_action_cards_from_sabotage_including_through_alliance() {
        use ti4_model::id::ActionCardId;
        let sabotage = |state: &GameState, actor: &str| {
            let payload = std::collections::BTreeMap::from([
                ("player".to_owned(), serde_json::Value::from(actor)),
                ("card".to_owned(), serde_json::Value::from("fs1")),
            ]);
            let event = crate::event::Event::new(1, "ACTION_CARD_PLAYED", payload);
            crate::reactions::playable_now(
                state,
                content(),
                &PlayerId::new("c"),
                &event,
                crate::timing::Relation::When,
            )
        };
        let mut state = game();
        state.player_mut(&c()).unwrap().action_cards = vec![ActionCardId::new("sabo1")];
        assert!(
            !sabotage(&state, "a").is_empty(),
            "locked commander: cancelable"
        );
        state.player_mut(&a()).unwrap().leaders.insert(
            LeaderId::new(COMMANDER),
            ti4_model::state::LeaderStatus::Unlocked,
        );
        assert!(sabotage(&state, "a").is_empty(), "unlocked: not cancelable");
        assert!(
            !sabotage(&state, "b").is_empty(),
            "other players' cards still are"
        );
        // Through a faceup Alliance, b uses the commander too.
        let alliance = crate::promissory::note_id("an", FACTION);
        crate::promissory::take(&mut state, content(), &b(), &alliance);
        state.promissory_faceup.insert(alliance);
        assert!(
            sabotage(&state, "b").is_empty(),
            "the ally's cards are shielded"
        );
    }

    // -- The Icon --------------------------------------------------------------------------------

    use crate::choice::{Resolving, Window};

    /// One scripted use of PRODUCTION in `home`: build `builds` (a unit id each), paying with trade
    /// goods, and place each at the spot `place` picks from the offered ids (`None`: only one
    /// offered, or declined). Returns the offered placement ids per build and the report.
    fn produce(
        state: &mut GameState,
        home: &SystemId,
        builds: &[&str],
        place: &dyn Fn(&[String]) -> String,
    ) -> (Vec<Vec<String>>, crate::production::ProductionReport) {
        let mut window =
            crate::production::ProductionWindow::new(state, content(), DEFAULT, &a(), home);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = crate::choice::Table::new();
        let mut ctx = Resolving {
            content: content(),
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: None,
        };
        let mut remaining = builds.iter();
        let mut offered = Vec::new();
        for _ in 0..40 {
            let Some(choice) = window.pending_choice(state, content(), DEFAULT) else {
                break;
            };
            let ids: Vec<String> = choice.options.iter().map(|o| o.id.clone()).collect();
            let pick = if ids.iter().any(|id| id.starts_with("place|")) {
                let spots: Vec<String> = ids
                    .iter()
                    .filter_map(|id| id.strip_prefix("place|"))
                    .map(ToOwned::to_owned)
                    .collect();
                offered.push(spots.clone());
                format!("place|{}", place(&spots))
            } else if ids.iter().any(|id| id.starts_with("build|")) {
                match remaining.next() {
                    Some(unit) => ids
                        .iter()
                        .find(|id| id.starts_with(&format!("build|{unit}|")))
                        .cloned()
                        .unwrap_or_else(|| "done_producing".to_owned()),
                    None => "done_producing".to_owned(),
                }
            } else if ids.iter().any(|id| id == "trade_good") {
                "trade_good".to_owned()
            } else {
                ids[0].clone()
            };
            let option = choice
                .option(&pick)
                .cloned()
                .unwrap_or_else(|| panic!("{pick} not in {ids:?}"));
            window.resolve(state, &mut ctx, option).expect("resolves");
        }
        (offered, window.into_report())
    }

    fn icon_game() -> (GameState, SystemId, SystemId) {
        let mut state = game();
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        state.player_mut(&a()).unwrap().trade_goods = 20;
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
        let (target, _) = arena();
        state.system_mut(&target).units.clear();
        state.system_mut(&target).planet_units.clear();
        state.system_mut(&target).place_token(a());
        crate::fixtures::put(&mut state, &target, "infantry", &a(), 1);
        (state, home, target)
    }

    #[test]
    fn the_icon_places_every_ship_of_one_use_in_one_qualifying_system_and_exhausts() {
        let (mut state, home, target) = icon_game();
        let remote = format!("{target}@space");
        let (offered, report) = produce(&mut state, &home, &["destroyer", "destroyer"], &|spots| {
            assert!(spots.contains(&remote), "{spots:?}");
            remote.clone()
        });
        assert_eq!(
            offered.len(),
            1,
            "the second ship is settled without a question"
        );
        let at = |s: &GameState, system: &SystemId| {
            s.system_state(system)
                .units
                .iter()
                .filter(|u| u.type_id.as_str() == "destroyer")
                .count()
        };
        assert_eq!(at(&state, &target), 2, "both went to the Icon's system");
        assert_eq!(report.produced.len(), 2);
        assert!(!icon_ready(&state, &a()), "exhausted");
        // Exhausted: the next use offers no remote spot.
        let (offered, _) = produce(&mut state, &home, &["destroyer"], &|spots| {
            assert!(!spots.iter().any(|s| s.contains('@')), "{spots:?}");
            spots[0].clone()
        });
        let _ = offered;
        // The status phase readies it.
        emit(&mut state, &[], "STATUS_PHASE_ENDED", &[]);
        assert!(icon_ready(&state, &a()));
    }

    #[test]
    fn the_icon_needs_a_token_a_ground_force_and_no_rival_ship_and_is_all_or_nothing() {
        let (mut state, home, target) = icon_game();
        let remote = format!("{target}@space");
        let offers = |state: &mut GameState| {
            let (offered, _) = produce(state, &home, &["destroyer"], &|spots| spots[0].clone());
            offered.iter().flatten().any(|s| *s == remote)
        };
        let mut rival = state.clone();
        crate::fixtures::put(&mut rival, &target, "cruiser", &b(), 1);
        assert!(!offers(&mut rival), "another player's ship");
        let mut no_ground = state.clone();
        no_ground.system_mut(&target).units.clear();
        assert!(!offers(&mut no_ground), "no ground force");
        let mut no_token = state.clone();
        no_token.system_mut(&target).command_tokens.clear();
        assert!(!offers(&mut no_token), "no command token");
        let mut no_card = state.clone();
        no_card.player_mut(&a()).unwrap().breakthrough = None;
        assert!(!offers(&mut no_card), "no breakthrough");
        // A ship placed at home first: the rest of the use stays home.
        let (_, report) = produce(&mut state, &home, &["destroyer", "destroyer"], &|spots| {
            spots.iter().find(|s| !s.contains('@')).cloned().unwrap()
        });
        assert!(
            report.produced.iter().all(|(_, at)| !at.contains('@')),
            "{:?}",
            report.produced
        );
        assert!(icon_ready(&state, &a()), "never used");
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn games_without_the_last_bastion_are_untouched_by_every_window() {
        let mut state =
            crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "xxcha")], DEFAULT);
        let (system, planet) = arena();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 2);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        state.system_mut(&system).set_control(planet.clone(), a());
        let before = state.clone();
        for (kind, pairs) in [
            ("SPACE_COMBAT_ENDED", space_ended(&system, "a", "b")),
            (
                "GROUND_COMBAT_ENDED",
                ground_ended(&system, &planet, "a", "b"),
            ),
            ("STATUS_PHASE_ENDED", vec![]),
        ] {
            emit(&mut state, &[], kind, &pairs);
        }
        crate::faction_abilities::control_gained(
            &mut state,
            content(),
            DEFAULT,
            &a(),
            &system,
            &planet,
        );
        assert!(state == before, "no Last Bastion: nothing changes");
        assert!(state.faction_marks == before.faction_marks);
        assert_eq!(galvanized_on_board(&state), 0);
        assert!(!super::super::bastion_units::watches_ground_round(
            &state,
            &[&a(), &b()]
        ));
        assert_eq!(
            super::super::bastion_units::resource_bonus(&state, &planet),
            0
        );
    }
}
