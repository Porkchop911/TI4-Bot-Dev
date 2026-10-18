//! The Crimson Rebellion (`crimson`, alias in `factions.json`). See `plans/evidence/BF-crimson.md`.
//!
//! Split for size: this file holds the breach model, Sorrow, Incursion, Sundered, the Quietus,
//! the Exile destroyers, the Revenant's DEPLOY and the Resonance Generator's movement bonus;
//! `crimson_cards.rs` holds Subatomic Splicer, Sever, Ahk Ravin, Ahk Siever, Homesick Phantom and
//! the Resonance Generator's ACTION.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Sundered: "You cannot use wormholes other than epsilon wormholes. Other players' units that
//!   move or are placed into your home system are destroyed." (Note: you still fire SPACE CANNON
//!   and form neighbourhood through them.)
//! * Incursion: "When you activate a system that contains a breach, you may flip that breach;
//!   systems that contain active breaches are adjacent. At the end of the status phase, any player
//!   with ships in a system that contain an active breach may remove that breach."
//! * Sorrow: "When you create the game board, place the Sorrow (tile 94) where your home system
//!   would normally be placed, then place a inactive breach there. The Sorrow is not a home
//!   system. Then, place your home system (tile 118) in your play area."
//! * Quietus (flagship): "While this unit is in a system that contains an active breach, other
//!   players' units in systems with active breaches lose all of their unit abilities."
//! * Revenant (mech): "DEPLOY: During the 'Commit Ground Forces' step of your tactical action in a
//!   system that contains an active breach, you may commit 1 mech, even if you have no units in the
//!   system."
//! * Exile I / II (destroyer): "At the end of any player's combat in this unit's system or an
//!   adjacent system (II: up to 2 systems away), you may place 1 inactive (II: active or inactive)
//!   breach in that system."
//! * Resonance Generator (`crimsonbt`): "During your tactical actions, apply +1 to the move value
//!   of each of your ships that starts its movement in your home system or in a system that
//!   contains an active breach. ACTION: Exhaust this card to flip any breach or to place an active
//!   breach in a non-home system that contains your units."
//!
//! # Breaches, modelled once
//!
//! `GameState::breach_tokens` is the set of systems that hold a breach token (the model carried it
//! before this faction; the reviewer draws it). Whether a breach is active is the mark
//! `crimson:breach:active:<system>`; a token with no mark is inactive. A system holds at most one
//! breach token. [`BREACH_SUPPLY`] tokens exist; with none left in reinforcements an inactive
//! breach is pulled from the board (the printed note). Everything that places, flips or removes a
//! breach goes through the functions below, so the mark and the set cannot disagree.
//!
//! # Sorrow and the home system
//!
//! The Sorrow (94) is on the map where the home position is and prints an epsilon wormhole; the home
//! system (118, Ahk Creuxx) is beside the board and prints one too, exactly as the Creuss gate and
//! home (`seating::place_crimson_home`). So the home system is reached only by an epsilon wormhole,
//! which is what Sundered keeps open for its owner.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_combat::CombatHooks;
use super::hooks_ground::GroundHooks;
use super::hooks_movement::{MoveSite, MovementHooks};
use super::{CombatUnit, FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
use crate::timing::{Ability, Frequency, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "crimson";

const SUNDERED: &str = "sundered";
const INCURSION: &str = "incursion";
const SORROW: &str = "sorrow";
/// Quietus.
pub const FLAGSHIP: &str = "crimson_flagship";
/// Revenant.
pub const MECH: &str = "crimson_mech";
/// Exile I.
pub const EXILE_I: &str = "crimson_destroyer";
/// Exile II.
pub const EXILE_II: &str = "crimson_destroyer2";
/// Ahk Siever.
pub const COMMANDER: &str = "crimsoncommander";
/// Resonance Generator.
pub const BREAKTHROUGH: &str = "crimsonbt";

/// How many breach tokens exist. The corpus prints no number; the card note ("If you run out of
/// breaches in your reinforcements, you can pull inactive breaches from the board") says the
/// supply is finite. Operator ruling 2026-10-07: there are seven, each with an active and an
/// inactive side.
pub const BREACH_SUPPLY: usize = 7;

/// The only wormhole kind Sundered lets its owner use.
const EPSILON: &str = "EPSILON";

const ACTIVE_PREFIX: &str = "crimson:breach:active:";

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[SUNDERED, INCURSION, SORROW],
    technologies: &[super::crimson_cards::SUBATOMIC, "exile2"],
    units: &[FLAGSHIP, MECH, EXILE_I, EXILE_II],
    promissory: &[super::crimson_cards::SEVER],
    leaders: &[
        super::crimson_cards::AGENT,
        COMMANDER,
        super::crimson_cards::HERO,
    ],
    breakthroughs: &[BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        commander_unlocked: Some(commander_unlocked),
        component_actions: Some(super::crimson_cards::component_actions),
        perform_component: Some(super::crimson_cards::perform_component),
        leader_action: Some(super::crimson_cards::leader_action),
        use_leader: Some(super::crimson_cards::use_leader),
        combat: CombatHooks {
            may_sustain: Some(may_sustain),
            ..CombatHooks::NONE
        },
        ground: GroundHooks {
            may_sustain: Some(may_sustain),
            ..GroundHooks::NONE
        },
        movement: MovementHooks {
            usable_wormhole_kinds: Some(usable_wormhole_kinds),
            severed_systems: Some(super::crimson_cards::severed_systems),
            linked_systems: Some(linked_systems),
            move_bonus: Some(move_bonus),
            ..MovementHooks::NONE
        },
        ..Hooks::NONE
    },
};

/// Whether `player` plays the Crimson Rebellion.
#[must_use]
pub fn is_crimson(state: &GameState, player: &PlayerId) -> bool {
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

/// Put one question to `who`. Every Crimson choice made inside a timing window or a component
/// action is asked here (the commit question lives in `invasion.rs` beside the flow that owns it,
/// the placement spot in `production.rs`).
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

/// A question about one system, so a viewer can point at it.
pub(crate) fn ask_about(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    card: &str,
    subtype: &str,
    system: &SystemId,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(
        decision(context.state, who, card, subtype).about(DecisionTarget::System(system.clone())),
    );
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

// -- breaches --------------------------------------------------------------------------------------

fn active_key(system: &SystemId) -> String {
    format!("{ACTIVE_PREFIX}{system}")
}

/// Whether `system` holds a breach token, active or not.
#[must_use]
pub fn has_breach(state: &GameState, system: &SystemId) -> bool {
    state.breach_tokens.contains(system)
}

/// Whether `system` holds an active breach.
#[must_use]
pub fn is_active(state: &GameState, system: &SystemId) -> bool {
    has_breach(state, system) && state.faction_marks.contains_key(&active_key(system))
}

/// Every system holding an active breach, in id order.
#[must_use]
pub fn active_breaches(state: &GameState) -> Vec<SystemId> {
    state
        .breach_tokens
        .iter()
        .filter(|system| state.faction_marks.contains_key(&active_key(system)))
        .cloned()
        .collect()
}

/// Every system holding an inactive breach, in id order.
#[must_use]
pub fn inactive_breaches(state: &GameState) -> Vec<SystemId> {
    state
        .breach_tokens
        .iter()
        .filter(|system| !state.faction_marks.contains_key(&active_key(system)))
        .cloned()
        .collect()
}

/// Breach tokens still in reinforcements.
#[must_use]
pub fn tokens_in_supply(state: &GameState) -> usize {
    BREACH_SUPPLY.saturating_sub(state.breach_tokens.len())
}

/// Whether a breach could be placed in `system` now: it holds none, and a token is in
/// reinforcements or an inactive breach elsewhere can be pulled.
#[must_use]
pub fn can_place_breach(state: &GameState, system: &SystemId) -> bool {
    !has_breach(state, system)
        && (tokens_in_supply(state) > 0
            || inactive_breaches(state).iter().any(|other| other != system))
}

/// Sorrow: "place a inactive breach there" (the Sorrow, tile 94). Idempotent.
pub fn place_sorrow_breach(state: &mut GameState) {
    let sorrow = SystemId::new(crate::seating::SORROW);
    state.breach_tokens.insert(sorrow.clone());
    state.faction_marks.remove(&active_key(&sorrow));
}

/// Public mark: this Crimson seat has made its setup breakthrough roll for The Fracture.
fn setup_roll_key(player: &PlayerId) -> String {
    format!("crimson:setup_roll:{player}")
}

/// Crimson seats that hold Resonance Generator from setup and have not yet made the breakthrough
/// roll (operator ruling 2026-10-18: Crimson starts with its breakthrough and rolls for The
/// Fracture like any player gaining one). Holding it does not close the expedition to them.
#[must_use]
pub fn setup_rollers(state: &GameState) -> Vec<PlayerId> {
    state
        .seating_order
        .iter()
        .filter(|player| {
            state.player(player).is_some_and(|seat| {
                seat.faction.as_str() == FACTION
                    && seat
                        .breakthrough
                        .as_ref()
                        .is_some_and(|held| held.as_str() == BREAKTHROUGH)
            }) && !state.faction_marks.contains_key(&setup_roll_key(player))
        })
        .cloned()
        .collect()
}

/// Record that `player` has made the setup roll, so it is never made twice.
pub fn mark_setup_rolled(state: &mut GameState, player: &PlayerId) {
    state
        .faction_marks
        .insert(setup_roll_key(player), "1".to_owned());
}

/// Flip the breach in `system`; `false`, changing nothing, when it holds none.
pub fn flip_breach(state: &mut GameState, system: &SystemId) -> bool {
    if !has_breach(state, system) {
        return false;
    }
    let key = active_key(system);
    if state.faction_marks.remove(&key).is_none() {
        state.faction_marks.insert(key, "1".to_owned());
    }
    true
}

/// Remove the breach in `system` to reinforcements; `false` when it holds none.
pub fn remove_breach(state: &mut GameState, system: &SystemId) -> bool {
    if !state.breach_tokens.remove(system) {
        return false;
    }
    state.faction_marks.remove(&active_key(system));
    true
}

/// Put a breach token in `system` and record that `placer` did. The caller has already found a
/// token (supply, or one pulled off the board). Unlocks Ahk Siever for `placer` when the system
/// holds another player's unit.
fn put_breach(state: &mut GameState, placer: &PlayerId, system: &SystemId, active: bool) {
    state.breach_tokens.insert(system.clone());
    if active {
        state
            .faction_marks
            .insert(active_key(system), "1".to_owned());
    } else {
        state.faction_marks.remove(&active_key(system));
    }
    if holds_another_players_unit(state, placer, system) {
        unlock_commander(state, placer);
    }
}

fn holds_another_players_unit(state: &GameState, player: &PlayerId, system: &SystemId) -> bool {
    let board = state.system_state(system);
    board
        .units
        .iter()
        .chain(board.planet_units.values().flatten())
        .any(|unit| &unit.owner != player)
}

/// Ahk Siever's unlock: "Place a breach token in a system that contains another player's unit."
/// Applied the moment it happens, so the commander is live in the very combat that unlocked it
/// (the printed note).
fn unlock_commander(state: &mut GameState, player: &PlayerId) {
    let commander = LeaderId::new(COMMANDER);
    if let Some(seat) = state.player_mut(player)
        && seat.leaders.get(&commander) == Some(&LeaderStatus::Locked)
    {
        seat.leaders.insert(commander, LeaderStatus::Unlocked);
    }
}

/// Place a breach in `system` for `placer`, asking which inactive breach to pull off the board when
/// reinforcements are empty and more than one could be. `Ok(false)`, changing nothing, when no
/// breach can be placed there.
pub(crate) fn place_breach(
    context: &mut TimingContext<'_>,
    placer: &PlayerId,
    system: &SystemId,
    active: bool,
    card: &str,
) -> Result<bool, TimingError> {
    if !can_place_breach(context.state, system) {
        return Ok(false);
    }
    if tokens_in_supply(context.state) == 0 {
        let mut pullable: Vec<SystemId> = inactive_breaches(context.state)
            .into_iter()
            .filter(|other| other != system)
            .collect();
        let pulled = match pullable.len() {
            0 => return Ok(false),
            1 => pullable.remove(0),
            _ => {
                let answer = ask(
                    context,
                    placer,
                    format!(
                        "{card}: no breach is left in reinforcements; pull which inactive breach"
                    ),
                    card,
                    "pull_breach",
                    pullable
                        .iter()
                        .map(|other| {
                            ChoiceOption::labelled(
                                other.to_string(),
                                "system",
                                format!("pull the inactive breach in {other}"),
                            )
                        })
                        .collect(),
                )?;
                let Some(chosen) = pullable
                    .into_iter()
                    .find(|other| other.as_str() == answer.id)
                else {
                    return Ok(false);
                };
                chosen
            }
        };
        remove_breach(context.state, &pulled);
    }
    put_breach(context.state, placer, system, active);
    Ok(true)
}

// -- registration ------------------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        activation_flip(owner_name, seat),
        status_removal(owner_name, seat),
        sundered_after_movement(owner_name, seat),
    ];
    for event_type in ["SPACE_COMBAT_ENDED", "GROUND_COMBAT_ENDED"] {
        abilities.push(exile_breach(owner_name, seat, event_type));
    }
    abilities.extend(super::crimson_cards::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    // Unlocked the moment the breach is placed (`put_breach`); a Locked commander here has not had
    // the condition happen.
    (leader.as_str() == COMMANDER).then(|| {
        state
            .player(player)
            .is_some_and(|seat| seat.leaders.get(leader) == Some(&LeaderStatus::Unlocked))
    })
}

// -- Incursion ---------------------------------------------------------------------------------------

/// "When you activate a system that contains a breach, you may flip that breach."
fn activation_flip(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{INCURSION}:SYSTEM_ACTIVATED:when"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if event.text("player") == Some(owner.as_str()) && has_breach(context.state, &system) {
                flip_breach(context.state, &system);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        is_crimson(context.state, &condition_owner)
            && event.text("player") == Some(condition_owner.as_str())
            && event
                .text("system")
                .is_some_and(|system| has_breach(context.state, &SystemId::new(system)))
    }))
}

/// The breaches `player` could remove: active, with a ship of theirs in the system.
fn removable_breaches(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    active_breaches(state)
        .into_iter()
        .filter(|system| {
            !crate::combat::ships_of(state, content, sources, player, system).is_empty()
        })
        .collect()
}

/// "At the end of the status phase, any player with ships in a system that contain an active breach
/// may remove that breach." Registered for every seat: the right belongs to whoever has the ships.
fn status_removal(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{INCURSION}_remove:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let candidates =
                removable_breaches(context.state, context.content, context.sources, &owner);
            let system = match candidates.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let answer = ask(
                        context,
                        &owner,
                        "Incursion: remove the breach in which system".to_owned(),
                        INCURSION,
                        "remove_breach",
                        candidates
                            .iter()
                            .map(|system| {
                                ChoiceOption::labelled(
                                    system.to_string(),
                                    "system",
                                    format!("remove the active breach in {system}"),
                                )
                            })
                            .collect(),
                    )?;
                    let Some(chosen) = candidates
                        .into_iter()
                        .find(|system| system.as_str() == answer.id)
                    else {
                        return Ok(());
                    };
                    chosen
                }
            };
            remove_breach(context.state, &system);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_frequency(Frequency::Unlimited)
    .with_repeatable_in_window(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        !context.state.breach_tokens.is_empty()
            && !removable_breaches(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_empty()
    }))
}

/// "Systems that contain active breaches are adjacent": every pair of them, for every player.
fn linked_systems(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: &Galaxy,
    _player: &PlayerId,
) -> Vec<(String, String)> {
    breach_pairs(state)
}

/// Every pair of systems that active breaches make adjacent (for every player).
pub(crate) fn breach_pairs(state: &GameState) -> Vec<(String, String)> {
    let active = active_breaches(state);
    let mut pairs = Vec::new();
    for (index, first) in active.iter().enumerate() {
        for second in &active[index + 1..] {
            pairs.push((first.to_string(), second.to_string()));
        }
    }
    pairs
}

// -- Sundered ----------------------------------------------------------------------------------------

/// "You cannot use wormholes other than epsilon wormholes."
fn usable_wormhole_kinds(state: &GameState, mover: &PlayerId) -> Option<Vec<String>> {
    is_crimson(state, mover).then(|| vec![EPSILON.to_owned()])
}

/// The home systems of every Crimson seat.
fn crimson_homes(state: &GameState) -> Vec<(PlayerId, SystemId)> {
    state
        .players
        .iter()
        .filter(|seat| seat.faction.as_str() == FACTION)
        .filter_map(|seat| Some((seat.id.clone(), seat.home_system.clone()?)))
        .collect()
}

/// "Other players' units that move or are placed into your home system are destroyed."
///
/// A reconcile rather than a hook at each route in: units reach a system by a tactical move, a
/// relocation, production, an action card, an exploration or a leader, and each of those writes the
/// board itself. It runs once per step (`Game::step`, beside the station and Keleres reconciles) and
/// again when a movement step finishes, before anything fires (`sundered_after_movement`), so a
/// unit never survives to be shot at, fight or take a planet. Ships are destroyed through the
/// engine's staging (`SHIP_DESTROYED` follows); ground forces through the ground staging.
pub fn enforce_sundered(state: &mut GameState, content: &ContentStore, sources: SourceSet) {
    for (owner, home) in crimson_homes(state) {
        let board = state.system_state(&home);
        let types = ti4_content::units::catalogue(content, sources);
        let mut victims: std::collections::BTreeMap<PlayerId, Vec<Unit>> =
            std::collections::BTreeMap::new();
        for unit in &board.units {
            if unit.owner != owner
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
            {
                victims
                    .entry(unit.owner.clone())
                    .or_default()
                    .push(unit.clone());
            }
        }
        for (victim, ships) in victims {
            crate::combat::destroy_units(
                state, content, sources, &victim, &home, &ships, "sundered",
            );
        }
        // Whatever else stands in the space area (ground forces riding along, which are not ships)
        // and everything on the planets.
        let leftovers: Vec<Unit> = state
            .system_state(&home)
            .units
            .iter()
            .filter(|unit| unit.owner != owner)
            .cloned()
            .collect();
        if !leftovers.is_empty() {
            state.system_mut(&home).remove(&leftovers);
        }
        for (planet, units) in &board.planet_units {
            let foreign: Vec<Unit> = units
                .iter()
                .filter(|unit| unit.owner != owner)
                .cloned()
                .collect();
            if foreign.is_empty() {
                continue;
            }
            state.system_mut(&home).remove_from_planet(planet, &foreign);
            for unit in &foreign {
                if types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ground_force)
                {
                    super::hooks_ground::stage_ground_force_destroyed(
                        state, &home, planet, unit, "sundered",
                    );
                }
            }
            crate::coexistence::reconcile(state, &home, planet);
        }
    }
}

/// The movement step is over: foreign units that arrived in a Crimson home are destroyed before the
/// space cannon, combat or invasion that would follow.
fn sundered_after_movement(owner_name: &str, seat: &PlayerId) -> Ability {
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:{SUNDERED}:MOVEMENT_FINISHED:after"),
        seat.clone(),
        "MOVEMENT_FINISHED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            enforce_sundered(context.state, context.content, context.sources);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        is_crimson(context.state, &condition_owner)
            && event.text("system").is_some_and(|system| {
                context
                    .state
                    .player(&condition_owner)
                    .and_then(|seat| seat.home_system.as_ref())
                    .is_some_and(|home| home.as_str() == system)
            })
    }))
}

// -- Quietus -----------------------------------------------------------------------------------------

/// Whether `owner`'s units in `system` have lost every unit ability to a Quietus: the system holds
/// an active breach and another player's Quietus (or a Nekro flagship carrying its text) stands in a
/// system holding one.
#[must_use]
pub fn abilities_lost(state: &GameState, owner: &PlayerId, system: &SystemId) -> bool {
    if state.breach_tokens.is_empty() || !is_active(state, system) {
        return false;
    }
    let active = active_breaches(state);
    state.players.iter().any(|seat| {
        seat.id != *owner
            && active
                .iter()
                .any(|at| super::has_flagship_text_in(state, &seat.id, at, FLAGSHIP))
    })
}

/// SUSTAIN DAMAGE is a unit ability: a unit that has lost them cannot use it (space and ground).
fn may_sustain(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> bool {
    !unit
        .system
        .is_some_and(|system| abilities_lost(state, unit.player, system))
}

// -- Exile ---------------------------------------------------------------------------------------------

/// The systems within `reach` steps of `from` for `owner` (including `from`), by the player's own
/// adjacency. Without a map only `from` itself.
fn within(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    owner: &PlayerId,
    from: &SystemId,
    reach: usize,
) -> Vec<SystemId> {
    let mut found = vec![from.clone()];
    let Some(galaxy) = galaxy else {
        return found;
    };
    let adjacency = crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, owner);
    let mut frontier = vec![from.clone()];
    for _ in 0..reach {
        let mut next = Vec::new();
        for system in &frontier {
            for neighbour in adjacency.neighbours(system.as_str()) {
                let neighbour = SystemId::new(neighbour);
                if !found.contains(&neighbour) {
                    found.push(neighbour.clone());
                    next.push(neighbour);
                }
            }
        }
        frontier = next;
    }
    found
}

/// What `owner`'s Exile destroyers allow in a combat in `combat_system`: `(inactive, active)` — an
/// Exile I in its own or an adjacent system allows an inactive breach, an Exile II within two
/// systems allows an active or inactive one.
fn exile_reach(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    owner: &PlayerId,
    combat_system: &SystemId,
) -> (bool, bool) {
    let mut inactive = false;
    let mut active = false;
    let near_one = within(state, content, sources, galaxy, owner, combat_system, 1);
    let near_two = within(state, content, sources, galaxy, owner, combat_system, 2);
    for (system, board) in &state.board {
        let ships = board.units.iter().filter(|unit| &unit.owner == owner);
        for unit in ships {
            match unit.type_id.as_str() {
                EXILE_I if near_one.contains(system) => inactive = true,
                EXILE_II if near_two.contains(system) => {
                    inactive = true;
                    active = true;
                }
                _ => {}
            }
        }
    }
    (inactive, active)
}

/// Exile I / II: "At the end of any player's combat in this unit's system or an adjacent system (II:
/// up to 2 systems away), you may place 1 inactive (II: active or inactive) breach in that system."
fn exile_breach(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{EXILE_I}:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let (inactive, active) = exile_reach(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                &owner,
                &system,
            );
            if !can_place_breach(context.state, &system) || !(inactive || active) {
                return Ok(());
            }
            let mut options = vec![ChoiceOption::labelled(
                "inactive",
                "breach",
                format!("place an inactive breach in {system}"),
            )];
            if active {
                options.push(ChoiceOption::labelled(
                    "active",
                    "breach",
                    format!("place an active breach in {system}"),
                ));
            }
            // Exile I alone has nothing to choose between; the window's accept/decline was the "may".
            let face = if let [only] = options.as_slice() {
                only.id.clone()
            } else {
                options.push(ChoiceOption::decline());
                let answer = ask_about(
                    context,
                    &owner,
                    format!("Exile: which breach to place in {system}"),
                    EXILE_I,
                    "exile_breach",
                    &system,
                    options,
                )?;
                if answer.is_decline() {
                    return Ok(());
                }
                answer.id
            };
            place_breach(context, &owner, &system, face == "active", EXILE_I)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        if !is_crimson(context.state, &condition_owner) || !can_place_breach(context.state, &system)
        {
            return false;
        }
        let (inactive, active) = exile_reach(
            context.state,
            context.content,
            context.sources,
            context.galaxy,
            &condition_owner,
            &system,
        );
        inactive || active
    }))
}

// -- Revenant (DEPLOY) -----------------------------------------------------------------------------------

/// The planets of `system` where `invader` may DEPLOY a Revenant while committing ground forces:
/// the invader plays the Rebellion (the unit is theirs), the system holds an active breach, and a
/// mech is left in reinforcements.
#[must_use]
pub fn deploy_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    invader: &PlayerId,
    system: &SystemId,
    landable: &[ti4_model::id::PlanetId],
) -> Vec<ti4_model::id::PlanetId> {
    if !is_crimson(state, invader)
        || !is_active(state, system)
        || state
            .faction_marks
            .contains_key(&deployed_key(state, invader))
        || crate::supply::allowed(state, content, sources, invader, &UnitTypeId::new(MECH), 1) == 0
    {
        return Vec::new();
    }
    landable.to_vec()
}

/// "You may commit 1 mech": the mark that this tactical action's Revenant has been deployed.
fn deployed_key(state: &GameState, invader: &PlayerId) -> String {
    format!("crimson:deployed:{invader}:{}", state.activation_seq)
}

/// Record that the Revenant was deployed in this activation (called by the commit step once the mech
/// has landed). The marks of earlier activations are dropped with it.
pub(crate) fn note_deployed(state: &mut GameState, invader: &PlayerId) {
    let stale: Vec<String> = state
        .faction_marks
        .range("crimson:deployed:".to_owned()..)
        .take_while(|(key, _)| key.starts_with("crimson:deployed:"))
        .map(|(key, _)| key.clone())
        .collect();
    for key in stale {
        state.faction_marks.remove(&key);
    }
    state
        .faction_marks
        .insert(deployed_key(state, invader), "1".to_owned());
}

/// Whether the Revenant could be deployed in `system` for `invader` at all (a planet to land on, an
/// active breach, a mech left). Read by the tactical action to open the Commit Ground Forces step
/// for a player with no ship in the system.
#[must_use]
pub fn can_deploy(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    invader: &PlayerId,
    system: &SystemId,
) -> bool {
    if !is_crimson(state, invader) {
        return false;
    }
    let landable = crate::invasion::landable_planets(state, content, sources, system);
    !deploy_planets(state, content, sources, invader, system, &landable).is_empty()
}

// -- Resonance Generator ------------------------------------------------------------------------------------

/// "During your tactical actions, apply +1 to the move value of each of your ships that starts its
/// movement in your home system or in a system that contains an active breach."
fn move_bonus(state: &GameState, site: &MoveSite<'_>) -> i32 {
    if !crate::breakthroughs::holds(state, site.player, BREAKTHROUGH) {
        return 0;
    }
    let at_home = state
        .player(site.player)
        .and_then(|seat| seat.home_system.as_ref())
        == Some(site.origin);
    i32::from(at_home || is_active(state, site.origin))
}

#[cfg(test)]
pub(crate) mod testkit {
    use std::collections::BTreeMap;

    use ti4_content::ContentStore;
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{PlayerId, SystemId};
    use ti4_model::state::GameState;

    use crate::choice::{Scripted, Table};

    pub(crate) const INCURSION_FLIP: &str = "ability:crimson:incursion:SYSTEM_ACTIVATED:when";
    pub(crate) const EXILE: &str = "unit:crimson:crimson_destroyer:SPACE_COMBAT_ENDED:after";

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
    /// `a` the Crimson Rebellion, `b` Sol, `c` Hacan.
    pub(crate) fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", "crimson"), ("b", "sol"), ("c", "hacan")], DEFAULT)
    }
    pub(crate) fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }

    /// Systems that print this wormhole kind, no anomaly, hyperlane or planetless special tile.
    pub(crate) fn wormhole_tiles(kind: &str) -> Vec<String> {
        ti4_content::galaxy::all_systems(content(), DEFAULT)
            .iter()
            .filter(|(_, system)| {
                system.wormholes().contains(kind)
                    && !system.is_anomaly()
                    && !system.is_hyperlane()
                    && system.wormholes().len() == 1
            })
            .map(|(id, _)| (*id).to_owned())
            .collect()
    }

    /// A one-ring map with `ring` (six systems) around `centre`.
    pub(crate) fn ring_map(centre: &str, ring: &[&str]) -> Galaxy {
        let mut ids = vec![centre];
        ids.extend_from_slice(ring);
        let mut galaxy = Galaxy::build(content(), &ids, DEFAULT, 1).expect("a valid map");
        crate::seating::place_crimson_home(&mut galaxy, content(), DEFAULT).expect("home");
        galaxy
    }

    /// Plain systems with no wormhole and no faction's home, for filling a ring.
    pub(crate) fn plain(count: usize) -> Vec<String> {
        let homes = ti4_content::galaxy::home_systems(content(), DEFAULT);
        ti4_content::galaxy::all_systems(content(), DEFAULT)
            .iter()
            .filter(|(id, system)| {
                !system.is_anomaly()
                    && !system.is_hyperlane()
                    && system.wormholes().is_empty()
                    && !system.planets().is_empty()
                    && !homes.contains(*id)
                    && **id != "18"
                    && **id != "94"
                    && **id != "118"
            })
            .map(|(id, _)| (*id).to_owned())
            .take(count)
            .collect()
    }

    /// Make `first` and `second` adjacent through two active breaches.
    pub(crate) fn link(state: &mut GameState, first: &str, second: &str) {
        for system in [first, second] {
            super::put_breach(state, &a(), &SystemId::new(system), true);
        }
    }

    pub(crate) fn emit(
        state: &mut GameState,
        galaxy: Option<&Galaxy>,
        answers: &[&str],
        kind: &str,
        pairs: &[(&str, serde_json::Value)],
    ) {
        let mut table = scripted(answers);
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, galaxy, &mut table, |context| {
            let payload: BTreeMap<String, serde_json::Value> = pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = context
                .event_sequence
                .next(kind, payload)
                .expect("an event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("emits");
        });
    }

    pub(crate) fn activated(
        system: &SystemId,
        player: &str,
    ) -> Vec<(&'static str, serde_json::Value)> {
        vec![
            ("system", system.to_string().into()),
            ("player", player.into()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT;

    use super::testkit::*;
    use super::*;

    fn home(state: &GameState) -> SystemId {
        state.player(&a()).unwrap().home_system.clone().unwrap()
    }

    // -- starting breakthrough -----------------------------------------------------------------

    #[test]
    fn the_crimson_starts_with_its_breakthrough_and_makes_the_fracture_roll_once() {
        let mut state = game();
        assert!(crate::breakthroughs::holds(&state, &a(), BREAKTHROUGH));
        assert_eq!(setup_rollers(&state), vec![a()]);
        state.phase = ti4_model::state::Phase::Strategy;
        state.round = 1;
        state.starting_tech_picks.clear();
        let mut game = crate::game::Game::new(state, content()).with_sources(DEFAULT);
        let _ = game.step();
        assert!(
            setup_rollers(&game.state).is_empty(),
            "the roll is made and marked"
        );
        let fracture = game.state.fracture_in_play;
        let _ = game.step();
        assert_eq!(game.state.fracture_in_play, fracture, "never rolled twice");
        assert!(
            crate::breakthroughs::holds(&game.state, &a(), BREAKTHROUGH),
            "the roll does not take the breakthrough away"
        );
    }

    // -- Sorrow -------------------------------------------------------------------------------

    #[test]
    fn the_crimson_seat_starts_in_tile_118_and_the_sorrow_holds_an_inactive_breach() {
        let state = game();
        assert_eq!(home(&state), SystemId::new("118"));
        let board = state.system_state(&SystemId::new("118"));
        assert!(
            board.controls_a_planet(&a()),
            "Ahk Creuxx is the home planet"
        );
        assert!(!board.units.is_empty(), "the starting fleet is at home");
        assert!(
            state.system_state(&SystemId::new("94")).units.is_empty(),
            "nothing starts on the Sorrow"
        );
        assert!(has_breach(&state, &SystemId::new("94")));
        assert!(!is_active(&state, &SystemId::new("94")), "inactive");
        assert_eq!(tokens_in_supply(&state), BREACH_SUPPLY - 1);
        // The Sorrow has no planet, so it is not a home system for anything that reads planets.
        assert!(!ti4_content::galaxy::is_home_system(
            content(),
            "94",
            DEFAULT
        ));
        assert!(ti4_content::galaxy::is_home_system(
            content(),
            "118",
            DEFAULT
        ));
    }

    fn board_for(pairs: &[(&str, &str)]) -> Galaxy {
        let seats: std::collections::BTreeMap<PlayerId, ti4_model::id::FactionId> = pairs
            .iter()
            .map(|(p, f)| (PlayerId::new(*p), ti4_model::id::FactionId::new(*f)))
            .collect();
        let filler = crate::seating::neutral_systems(content(), 30, DEFAULT);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        crate::seating::build_board(content(), &seats, &refs, DEFAULT).expect("a board")
    }

    #[test]
    fn the_sorrow_sits_in_the_home_position_and_the_home_system_is_off_the_map() {
        let galaxy = board_for(&[("a", "crimson"), ("b", "sol"), ("c", "hacan")]);
        assert!(galaxy.coord_of("94").is_some(), "the Sorrow is on the grid");
        assert!(galaxy.coord_of("118").is_none(), "the home system is not");
        assert!(
            galaxy.wormhole_kinds("118").contains("EPSILON"),
            "but it is in play"
        );
        assert!(galaxy.are_adjacent("94", "118") && galaxy.are_adjacent("118", "94"));
        assert_eq!(
            galaxy.adjacent("118").into_iter().collect::<Vec<_>>(),
            vec!["94"],
            "only the Sorrow is adjacent to the off-map home"
        );
    }

    #[test]
    fn boards_without_a_crimson_seat_are_unchanged() {
        let galaxy = board_for(&[("a", "sol"), ("b", "hacan"), ("c", "letnev")]);
        assert!(galaxy.coord_of("94").is_none());
        assert!(galaxy.wormhole_kinds("118").is_empty());
        let mut again = galaxy.clone();
        crate::seating::place_crimson_home(&mut again, content(), DEFAULT).unwrap();
        assert_eq!(
            again, galaxy,
            "placing the home is a no-op without the Sorrow"
        );
        // Placing twice changes nothing either.
        let mut crimson = board_for(&[("a", "crimson"), ("b", "sol")]);
        let once = crimson.clone();
        crate::seating::place_crimson_home(&mut crimson, content(), DEFAULT).unwrap();
        assert_eq!(crimson, once);
    }

    // -- the breach model ------------------------------------------------------------------------

    #[test]
    fn breaches_flip_and_come_off_the_board_and_a_system_holds_one() {
        let mut state = game();
        let system = SystemId::new("25");
        assert!(can_place_breach(&state, &system));
        put_breach(&mut state, &a(), &system, true);
        assert!(is_active(&state, &system));
        assert!(!can_place_breach(&state, &system), "one breach per system");
        assert!(flip_breach(&mut state, &system));
        assert!(has_breach(&state, &system) && !is_active(&state, &system));
        assert!(flip_breach(&mut state, &system));
        assert!(is_active(&state, &system));
        assert!(remove_breach(&mut state, &system));
        assert!(!has_breach(&state, &system) && !is_active(&state, &system));
        assert!(
            !remove_breach(&mut state, &system),
            "nothing left to remove"
        );
        assert!(!flip_breach(&mut state, &system));
        assert_eq!(
            tokens_in_supply(&state),
            BREACH_SUPPLY - 1,
            "back in reinforcements"
        );
    }

    #[test]
    fn with_no_breach_in_reinforcements_an_inactive_one_is_pulled_off_the_board() {
        let mut state = game();
        let spots: Vec<SystemId> = plain(BREACH_SUPPLY + 1)
            .into_iter()
            .map(SystemId::new)
            .collect();
        // The Sorrow's breach plus five more empty the supply.
        for spot in &spots[..BREACH_SUPPLY - 1] {
            put_breach(&mut state, &a(), spot, false);
        }
        assert_eq!(tokens_in_supply(&state), 0);
        let target = &spots[BREACH_SUPPLY];
        assert!(
            can_place_breach(&state, target),
            "an inactive one can be pulled"
        );
        // An active breach is never pulled: with every other breach active, none can be placed.
        let mut all_active = state.clone();
        for system in inactive_breaches(&all_active) {
            flip_breach(&mut all_active, &system);
        }
        assert!(!can_place_breach(&all_active, target));
        // Pulling asks which inactive breach goes back, then places the new one.
        let mut table = scripted(&["25"]);
        let _ = &mut table;
        let pulled = SystemId::new(spots[0].as_str());
        let result = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&[pulled.as_str()]),
            |context| place_breach(context, &a(), target, true, "test"),
        );
        assert_eq!(result.expect("asks"), true);
        assert!(!has_breach(&state, &pulled), "the chosen breach was pulled");
        assert!(is_active(&state, target));
        assert_eq!(state.breach_tokens.len(), BREACH_SUPPLY);
    }

    #[test]
    fn placing_a_breach_where_another_players_unit_stands_unlocks_ahk_siever() {
        let mut state = game();
        let commander = LeaderId::new(COMMANDER);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&commander),
            Some(&LeaderStatus::Locked)
        );
        let empty = SystemId::new("25");
        put_breach(&mut state, &a(), &empty, false);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&commander),
            Some(&LeaderStatus::Locked),
            "an empty system does not unlock it"
        );
        let own = SystemId::new("26");
        crate::fixtures::put(&mut state, &own, "destroyer", &a(), 1);
        put_breach(&mut state, &a(), &own, false);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&commander),
            Some(&LeaderStatus::Locked),
            "the placer's own unit does not unlock it"
        );
        let rival = SystemId::new("27");
        crate::fixtures::put(&mut state, &rival, "destroyer", &b(), 1);
        put_breach(&mut state, &a(), &rival, false);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&commander),
            Some(&LeaderStatus::Unlocked)
        );
        assert_eq!(
            commander_unlocked(&state, content(), DEFAULT, None, &a(), &commander),
            Some(true)
        );
    }

    // -- Incursion ------------------------------------------------------------------------------------

    #[test]
    fn incursion_lets_the_crimson_flip_the_breach_in_a_system_they_activate() {
        let mut state = game();
        let system = SystemId::new("25");
        put_breach(&mut state, &a(), &system, false);
        emit(
            &mut state,
            None,
            &[INCURSION_FLIP],
            "SYSTEM_ACTIVATED",
            &activated(&system, "a"),
        );
        assert!(is_active(&state, &system), "flipped to active");
        emit(
            &mut state,
            None,
            &[INCURSION_FLIP],
            "SYSTEM_ACTIVATED",
            &activated(&system, "a"),
        );
        assert!(!is_active(&state, &system), "and back");
        // The "may": declining leaves it.
        emit(
            &mut state,
            None,
            &["decline"],
            "SYSTEM_ACTIVATED",
            &activated(&system, "a"),
        );
        assert!(!is_active(&state, &system));
    }

    #[test]
    fn incursion_is_only_for_the_crimsons_own_activation_of_a_system_with_a_breach() {
        let mut state = game();
        let system = SystemId::new("25");
        put_breach(&mut state, &a(), &system, false);
        // Another player activates it: no window for the Crimson.
        emit(
            &mut state,
            None,
            &[INCURSION_FLIP],
            "SYSTEM_ACTIVATED",
            &activated(&system, "b"),
        );
        assert!(!has_breach(&state, &system) || !is_active(&state, &system));
        // The Crimson activates a system with no breach: nothing to flip.
        let bare = SystemId::new("26");
        emit(
            &mut state,
            None,
            &[INCURSION_FLIP],
            "SYSTEM_ACTIVATED",
            &activated(&bare, "a"),
        );
        assert!(!has_breach(&state, &bare));
    }

    #[test]
    fn systems_with_active_breaches_are_adjacent_for_every_player() {
        let ring = plain(6);
        let ring: Vec<&str> = ring.iter().map(String::as_str).collect();
        let centre = plain(7).pop().unwrap();
        let galaxy = ring_map(&centre, &ring);
        let (first, opposite) = (ring[0], ring[3]);
        let mut state = game();
        let ship_for = |state: &mut GameState, owner: &PlayerId| {
            crate::fixtures::put(state, &SystemId::new(first), "cruiser", owner, 1);
        };
        ship_for(&mut state, &a());
        ship_for(&mut state, &b());
        let reach = |state: &GameState, player: &PlayerId| {
            crate::movement::MovementRules::with_laws(
                &galaxy,
                content(),
                DEFAULT,
                opposite,
                crate::movement::Board::for_player(state, content(), DEFAULT, player),
                Some(state),
            )
            .can_reach(first, 1)
        };
        assert!(
            !reach(&state, &a()) && !reach(&state, &b()),
            "two apart across the ring"
        );
        put_breach(&mut state, &a(), &SystemId::new(first), false);
        put_breach(&mut state, &a(), &SystemId::new(opposite), true);
        assert!(!reach(&state, &a()), "an inactive breach links nothing");
        flip_breach(&mut state, &SystemId::new(first));
        assert!(reach(&state, &a()), "two active breaches are adjacent");
        assert!(
            reach(&state, &b()),
            "for every player, not only the Crimson"
        );
        // The player-aware adjacency (space cannon range, neighbours) sees it too.
        let adjacency =
            crate::movement::PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &c());
        assert!(adjacency.are_adjacent(first, opposite));
    }

    #[test]
    fn at_the_end_of_the_status_phase_a_player_with_ships_may_remove_an_active_breach() {
        let mut state = game();
        let (here, there) = (SystemId::new("25"), SystemId::new("26"));
        put_breach(&mut state, &a(), &here, true);
        put_breach(&mut state, &a(), &there, false);
        crate::fixtures::put(&mut state, &here, "cruiser", &b(), 1);
        crate::fixtures::put(&mut state, &there, "cruiser", &b(), 1);
        let remove_b = "ability:sol:incursion_remove:STATUS_PHASE_ENDED:after";
        // Declined: it stays.
        emit(&mut state, None, &["decline"], "STATUS_PHASE_ENDED", &[]);
        assert!(is_active(&state, &here));
        // Sol (not the Crimson) holds ships there and removes it. The inactive breach is not offered.
        emit(&mut state, None, &[remove_b], "STATUS_PHASE_ENDED", &[]);
        assert!(!has_breach(&state, &here), "removed");
        assert!(has_breach(&state, &there), "an inactive breach stays");
        // Without ships in the system nobody may remove it.
        let mut empty = game();
        put_breach(&mut empty, &a(), &here, true);
        emit(&mut empty, None, &[remove_b], "STATUS_PHASE_ENDED", &[]);
        assert!(is_active(&empty, &here));
    }

    // -- Sundered: wormholes ------------------------------------------------------------------------------

    #[test]
    fn sundered_closes_every_wormhole_but_epsilon_to_its_owner() {
        let alpha = wormhole_tiles("ALPHA");
        assert!(alpha.len() >= 2, "{alpha:?}");
        let others = plain(5);
        let ring = [
            alpha[0].as_str(),
            others[0].as_str(),
            others[1].as_str(),
            alpha[1].as_str(),
            others[2].as_str(),
            others[3].as_str(),
        ];
        let galaxy = ring_map(&others[4], &ring);
        let mut state = game();
        let (start, target) = (SystemId::new(ring[0]), SystemId::new(ring[3]));
        crate::fixtures::put(&mut state, &start, "carrier", &a(), 1);
        crate::fixtures::put(&mut state, &start, "carrier", &b(), 1);
        let movable = |state: &GameState, player: &PlayerId| {
            crate::tactical::movable_into(state, content(), DEFAULT, &galaxy, player, &target)
                .into_iter()
                .filter(|ship| ship.origin == start)
                .count()
        };
        assert_eq!(movable(&state, &b()), 1, "Sol uses the alpha wormholes");
        assert_eq!(
            movable(&state, &a()),
            0,
            "the Crimson is never offered the move"
        );
        // Hex adjacency is untouched: the Crimson ship still reaches a neighbour.
        let next_door = SystemId::new(ring[1]);
        assert_eq!(
            crate::tactical::movable_into(&state, content(), DEFAULT, &galaxy, &a(), &next_door)
                .into_iter()
                .filter(|ship| ship.origin == start)
                .count(),
            1
        );
        // Neighbourhood through the wormhole remains (space cannon range, transactions).
        let adjacency =
            crate::movement::PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &a());
        assert!(adjacency.are_adjacent(ring[0], ring[3]));
    }

    #[test]
    fn a_retreat_is_a_move_so_sundered_closes_the_wormhole_to_it_as_well() {
        let alpha = wormhole_tiles("ALPHA");
        let others = plain(5);
        let ring = [
            alpha[0].as_str(),
            others[0].as_str(),
            others[1].as_str(),
            alpha[1].as_str(),
            others[2].as_str(),
            others[3].as_str(),
        ];
        let galaxy = ring_map(&others[4], &ring);
        let (here, there) = (SystemId::new(ring[0]), SystemId::new(ring[3]));
        let retreats = |owner: &PlayerId| {
            let mut state = game();
            crate::fixtures::put(&mut state, &here, "cruiser", owner, 1);
            crate::fixtures::put(&mut state, &there, "cruiser", owner, 1);
            crate::combat::eligible_retreats(&state, content(), DEFAULT, &galaxy, owner, &here)
        };
        let sol = retreats(&b());
        assert!(
            sol.contains(&there),
            "Sol retreats through the alpha wormhole: {sol:?}"
        );
        let crimson = retreats(&a());
        assert!(
            !crimson.contains(&there),
            "the Crimson may not: {crimson:?}"
        );
    }

    #[test]
    fn sundered_keeps_the_epsilon_wormhole_open_both_ways() {
        let others = plain(7);
        let ring: Vec<&str> = ["94"]
            .into_iter()
            .chain(others[..5].iter().map(String::as_str))
            .collect();
        let galaxy = ring_map(&others[6], &ring);
        let mut state = game();
        let (sorrow, home) = (SystemId::new("94"), SystemId::new("118"));
        crate::fixtures::put(&mut state, &sorrow, "cruiser", &a(), 1);
        let into_home =
            crate::tactical::movable_into(&state, content(), DEFAULT, &galaxy, &a(), &home);
        assert!(
            into_home.iter().any(|ship| ship.origin == sorrow),
            "the Crimson reaches its home through the epsilon wormhole"
        );
        // Home out to the Sorrow: the starting fleet moves out.
        let out = crate::tactical::movable_into(&state, content(), DEFAULT, &galaxy, &a(), &sorrow);
        assert!(
            out.iter().any(|ship| ship.origin == home),
            "and leaves by it"
        );
        // A rival reaches the home system too (and is destroyed there).
        crate::fixtures::put(&mut state, &sorrow, "cruiser", &b(), 1);
        assert!(
            crate::tactical::movable_into(&state, content(), DEFAULT, &galaxy, &b(), &home)
                .iter()
                .any(|ship| ship.origin == sorrow)
        );
    }

    // -- Sundered: the home system ----------------------------------------------------------------------------

    #[test]
    fn units_that_move_into_the_home_system_are_destroyed_in_a_real_tactical_action() {
        let others = plain(7);
        let ring: Vec<&str> = ["94"]
            .into_iter()
            .chain(others[..5].iter().map(String::as_str))
            .collect();
        let galaxy = ring_map(&others[6], &ring);
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(b());
        let (sorrow, home) = (SystemId::new("94"), SystemId::new("118"));
        crate::fixtures::put(&mut state, &sorrow, "cruiser", &b(), 1);
        crate::fixtures::put(&mut state, &sorrow, "carrier", &b(), 1);
        crate::fixtures::put(&mut state, &sorrow, "infantry", &b(), 2);
        state.player_mut(&b()).unwrap().fleet_tokens = 6;
        let table = scripted(&[
            crate::game::TACTICAL_ACTION_ID,
            "118",
            "move|94|1",
            "load|0",
            "load|1",
            "move|94|0",
            "done_moving",
        ]);
        let mut game = crate::game::Game::with_table(state, content(), table)
            .with_galaxy(galaxy)
            .with_sources(DEFAULT);
        for _ in 0..40 {
            let result = game.step();
            assert_eq!(result.error, None, "log: {:?}", game.events);
            if game.events.iter().any(|e| e == "TACTICAL_ACTION_COMPLETE") {
                break;
            }
        }
        let board = game.state.system_state(&home);
        assert!(
            board.units.iter().all(|unit| unit.owner == a()),
            "no rival unit survives in the Crimson home: {:?}",
            board.units
        );
        assert!(
            board
                .planet_units
                .values()
                .flatten()
                .all(|unit| unit.owner == a())
        );
        assert!(
            game.state
                .system_state(&sorrow)
                .units
                .iter()
                .all(|unit| unit.owner != b() || unit.type_id.as_str() != "cruiser"),
            "the cruiser that moved is gone from where it started too"
        );
    }

    #[test]
    fn units_placed_into_the_home_system_are_destroyed_whatever_the_route() {
        let mut state = game();
        let h = home(&state);
        let planet = state
            .controlled_planets(&a())
            .into_iter()
            .find(|(system, _)| **system == h)
            .map(|(_, planet)| planet.clone())
            .unwrap();
        crate::fixtures::put(&mut state, &h, "destroyer", &b(), 2);
        crate::fixtures::put(&mut state, &h, "infantry", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &h, &planet, "infantry", &b(), 3);
        crate::fixtures::put_on_planet(&mut state, &h, &planet, "pds", &c(), 1);
        let own_before = state.system_state(&h).units_of(&a()).len();
        let mut game = crate::game::Game::new(state, content()).with_sources(DEFAULT);
        game.state.phase = ti4_model::state::Phase::Action;
        game.state.active = Some(a());
        let _ = game.step();
        let board = game.state.system_state(&h);
        assert!(
            board.units.iter().all(|unit| unit.owner == a()),
            "{:?}",
            board.units
        );
        assert!(
            board
                .planet_units
                .values()
                .flatten()
                .all(|unit| unit.owner == a()),
            "rival ground forces and structures are destroyed too"
        );
        assert_eq!(
            board.units_of(&a()).len(),
            own_before,
            "the owner's own units stand"
        );
        assert!(
            crate::factions::hooks_ground::has_staged_events(&game.state),
            "the ground forces are announced as destroyed"
        );
    }

    // -- a game without the Crimson ----------------------------------------------------------------------------

    #[test]
    fn a_game_without_the_crimson_is_untouched_by_every_window() {
        let mut state =
            crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "letnev")], DEFAULT);
        assert!(state.breach_tokens.is_empty());
        let before = state.clone();
        enforce_sundered(&mut state, content(), DEFAULT);
        assert_eq!(state, before, "nothing to enforce");
        let system = SystemId::new("25");
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        let with_ship = state.clone();
        for (kind, pairs) in [
            ("SYSTEM_ACTIVATED", activated(&system, "a")),
            ("STATUS_PHASE_ENDED", Vec::new()),
            (
                "SPACE_COMBAT_ENDED",
                vec![("system", system.to_string().into())],
            ),
            (
                "SHIP_DESTROYED",
                vec![("system", system.to_string().into())],
            ),
        ] {
            emit(&mut state, None, &[], kind, &pairs);
            assert_eq!(state, with_ship, "{kind} changes nothing");
        }
        // No wormhole limits, no breach links, no bonus for anyone.
        assert!(super::super::hooks_movement::wormhole_limits(&state, Some(&a())).is_empty());
        assert!(
            linked_systems(
                &state,
                content(),
                DEFAULT,
                &Galaxy::build(content(), &["18"], DEFAULT, 1).unwrap(),
                &a()
            )
            .is_empty()
        );
        assert!(!abilities_lost(&state, &a(), &system));
        assert!(component_actions_none(&state));
    }

    fn component_actions_none(state: &GameState) -> bool {
        super::super::crimson_cards::component_actions(state, content(), &a()).is_empty()
    }

    // -- Quietus -------------------------------------------------------------------------------------

    /// `a`'s Quietus stands in active-breach system `x`; `y` is another active-breach system with a
    /// planet. Returns `(state, x, y, planet_in_y)`.
    fn quietus_game() -> (GameState, SystemId, SystemId, ti4_model::id::PlanetId) {
        let mut state = game();
        let (y, planet) = super::super::deepwrought::testkit::plain_planet();
        let x = SystemId::new(
            plain(9)
                .into_iter()
                .find(|id| id.as_str() != y.as_str())
                .unwrap(),
        );
        crate::fixtures::put(&mut state, &x, FLAGSHIP, &a(), 1);
        put_breach(&mut state, &a(), &x, true);
        put_breach(&mut state, &a(), &y, true);
        (state, x, y, planet)
    }

    fn may_sustain_there(state: &GameState, owner: &PlayerId, system: &SystemId) -> bool {
        crate::factions::hooks_combat::may_sustain(
            state,
            content(),
            DEFAULT,
            &CombatUnit {
                player: owner,
                system: Some(system),
                planet: None,
                unit_type: "dreadnought",
                context: "space",
            },
        )
    }

    #[test]
    fn the_quietus_strips_sustain_damage_from_other_players_units_in_active_breaches() {
        let (mut state, x, y, _) = quietus_game();
        assert!(
            !may_sustain_there(&state, &b(), &y),
            "Sol's units in an active breach"
        );
        assert!(
            !may_sustain_there(&state, &b(), &x),
            "the Quietus's own system counts too"
        );
        assert!(
            may_sustain_there(&state, &a(), &y),
            "its owner keeps every ability"
        );
        assert!(
            may_sustain_there(&state, &b(), &SystemId::new("1")),
            "outside a breach"
        );
        // An inactive breach is no breach for this purpose.
        flip_breach(&mut state, &y);
        assert!(may_sustain_there(&state, &b(), &y));
        flip_breach(&mut state, &y);
        // The Quietus must itself be in a system holding an active breach.
        flip_breach(&mut state, &x);
        assert!(
            may_sustain_there(&state, &b(), &y),
            "its own breach is inactive"
        );
        flip_breach(&mut state, &x);
        assert!(!may_sustain_there(&state, &b(), &y));
        // Without the Quietus on the board nothing is lost.
        state.system_mut(&x).units.clear();
        assert!(may_sustain_there(&state, &b(), &y));
    }

    #[test]
    fn a_nekro_flagship_carrying_the_quietus_text_strips_abilities_too() {
        let mut state = crate::fixtures::nekro_with_z(
            &[("a", "nekro"), ("b", "crimson"), ("c", "sol")],
            &["crimson"],
        );
        let (y, _) = super::super::deepwrought::testkit::plain_planet();
        let x = SystemId::new(plain(9).into_iter().find(|id| id != y.as_str()).unwrap());
        crate::fixtures::put(
            &mut state,
            &x,
            crate::factions::nekro::NEKRO_FLAGSHIP,
            &a(),
            1,
        );
        put_breach(&mut state, &b(), &x, true);
        put_breach(&mut state, &b(), &y, true);
        assert!(abilities_lost(&state, &c(), &y), "Sol loses its abilities");
        assert!(!abilities_lost(&state, &a(), &y), "the Nekro keeps its own");
        assert!(
            abilities_lost(&state, &b(), &y),
            "the text is the Nekro's, so the Crimson player is an 'other player' to it"
        );
    }

    #[test]
    fn the_quietus_stops_production_space_cannon_barrage_bombardment_and_planetary_shield() {
        let (mut state, x, y, planet) = quietus_game();
        let _ = x;
        // PRODUCTION: Sol's space dock in the breach builds nothing.
        state.system_mut(&y).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &y, &planet, "spacedock", &b(), 1);
        let types = ti4_content::units::catalogue(content(), DEFAULT);
        let fighter = types.get("fighter").unwrap();
        let spots = |state: &GameState| {
            crate::production::placements(state, content(), DEFAULT, &b(), &y, fighter)
        };
        assert!(spots(&state).is_empty(), "PRODUCTION is a unit ability");
        let mut calm = state.clone();
        flip_breach(&mut calm, &y);
        assert!(
            !spots(&calm).is_empty(),
            "an inactive breach changes nothing"
        );

        // SPACE CANNON OFFENSE: Sol's PDS fires at a ship that activates the system.
        crate::fixtures::put_on_planet(&mut state, &y, &planet, "pds", &b(), 1);
        crate::fixtures::put(&mut state, &y, "cruiser", &c(), 1);
        let shots = |state: &mut GameState| -> usize {
            let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10, 12));
            let mut rng = crate::rng::GameRng::new(0);
            crate::combat::space_cannon_offense(
                state,
                content(),
                DEFAULT,
                &mut dice,
                &mut rng,
                &y,
                &c(),
                None,
            )
            .into_iter()
            .filter(|(owner, _, _)| *owner == b())
            .map(|(_, hits, _)| hits)
            .sum()
        };
        assert_eq!(shots(&mut state), 0, "the PDS has lost SPACE CANNON");
        let mut calm = state.clone();
        flip_breach(&mut calm, &y);
        assert!(shots(&mut calm) > 0, "with the breach inactive it fires");

        // ANTI-FIGHTER BARRAGE.
        crate::fixtures::put(&mut state, &y, "destroyer", &b(), 1);
        let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10, 12));
        let mut rng = crate::rng::GameRng::new(0);
        assert_eq!(
            crate::combat::roll_barrage_side(
                &mut state,
                content(),
                DEFAULT,
                &mut dice,
                &mut rng,
                &y,
                &b()
            ),
            0,
            "the destroyer has lost ANTI-FIGHTER BARRAGE"
        );
        let mut calm = state.clone();
        flip_breach(&mut calm, &y);
        let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10, 12));
        assert!(
            crate::combat::roll_barrage_side(
                &mut calm,
                content(),
                DEFAULT,
                &mut dice,
                &mut rng,
                &y,
                &b()
            ) > 0
        );

        // PLANETARY SHIELD: Sol's PDS no longer shields its planet from bombardment.
        let mut invaded = state.clone();
        crate::fixtures::put(&mut invaded, &y, "dreadnought", &c(), 1);
        assert!(
            crate::invasion::bombardable(&invaded, content(), DEFAULT, &y, &planet, &c()),
            "the shield is gone"
        );
        let mut shielded = invaded.clone();
        flip_breach(&mut shielded, &y);
        assert!(
            !crate::invasion::bombardable(&shielded, content(), DEFAULT, &y, &planet, &c()),
            "with the breach inactive the PDS shields the planet"
        );

        // BOMBARDMENT: the invader's own dreadnought, in the breach, has lost it.
        let infantry = |state: &GameState| {
            state
                .system_state(&y)
                .on_planet_of(&planet, &b())
                .iter()
                .filter(|u| u.type_id.as_str() == "infantry")
                .count()
        };
        crate::fixtures::put_on_planet(&mut invaded, &y, &planet, "infantry", &b(), 3);
        // No shield in the way: the PDS is not there for this part.
        let shield = invaded
            .system_state(&y)
            .on_planet_of(&planet, &b())
            .into_iter()
            .find(|unit| unit.type_id.as_str() == "pds")
            .cloned()
            .expect("the PDS");
        invaded
            .system_mut(&y)
            .remove_from_planet(&planet, std::slice::from_ref(&shield));
        let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10, 12));
        let killed = crate::invasion::bombardment(
            &mut invaded,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &mut scripted(&[]),
            &y,
            &c(),
        )
        .expect("resolves");
        assert_eq!(killed, 0, "the dreadnought has lost BOMBARDMENT");
        assert_eq!(infantry(&invaded), 3);
        let mut calm = invaded.clone();
        flip_breach(&mut calm, &y);
        let mut dice = crate::dice::Dice::from_faces(std::iter::repeat_n(10, 12));
        let killed = crate::invasion::bombardment(
            &mut calm,
            content(),
            DEFAULT,
            &mut dice,
            &mut rng,
            &mut scripted(&[]),
            &y,
            &c(),
        )
        .expect("resolves");
        assert!(
            killed > 0 || infantry(&calm) < 3,
            "with the breach inactive it bombards"
        );
    }

    // -- Exile -----------------------------------------------------------------------------------------

    fn exile_ring() -> (Galaxy, Vec<String>) {
        let ids = plain(7);
        let ring: Vec<&str> = ids[..6].iter().map(String::as_str).collect();
        (ring_map(&ids[6], &ring), ids)
    }

    #[test]
    fn exile_i_places_an_inactive_breach_where_a_combat_ended_in_or_next_to_its_system() {
        let (galaxy, ids) = exile_ring();
        let (fought, near, far) = (
            SystemId::new(ids[0].as_str()),
            SystemId::new(ids[1].as_str()),
            SystemId::new(ids[2].as_str()),
        );
        let combat = |system: &SystemId| vec![("system", system.to_string().into())];
        // Adjacent: the breach goes into the combat's system, inactive.
        let mut state = game();
        crate::fixtures::put(&mut state, &near, EXILE_I, &a(), 1);
        emit(
            &mut state,
            Some(&galaxy),
            &[EXILE],
            "SPACE_COMBAT_ENDED",
            &combat(&fought),
        );
        assert!(has_breach(&state, &fought) && !is_active(&state, &fought));
        // A ground combat counts as a combat too (same system, no distance).
        let mut ground = game();
        crate::fixtures::put(&mut ground, &fought, EXILE_I, &a(), 1);
        emit(
            &mut ground,
            Some(&galaxy),
            &["unit:crimson:crimson_destroyer:GROUND_COMBAT_ENDED:after"],
            "GROUND_COMBAT_ENDED",
            &combat(&fought),
        );
        assert!(has_breach(&ground, &fought));
        // Two systems away is too far for Exile I.
        let mut distant = game();
        crate::fixtures::put(&mut distant, &far, EXILE_I, &a(), 1);
        let before = distant.clone();
        emit(
            &mut distant,
            Some(&galaxy),
            &[EXILE],
            "SPACE_COMBAT_ENDED",
            &combat(&fought),
        );
        assert_eq!(distant, before, "out of reach");
        // The "may".
        let mut declined = game();
        crate::fixtures::put(&mut declined, &near, EXILE_I, &a(), 1);
        emit(
            &mut declined,
            Some(&galaxy),
            &["decline"],
            "SPACE_COMBAT_ENDED",
            &combat(&fought),
        );
        assert!(!has_breach(&declined, &fought));
        // It is any player's combat, not only the owner's; and a system holds one breach.
        let mut twice = game();
        crate::fixtures::put(&mut twice, &near, EXILE_I, &a(), 1);
        put_breach(&mut twice, &a(), &fought, false);
        let before = twice.clone();
        emit(
            &mut twice,
            Some(&galaxy),
            &[EXILE],
            "SPACE_COMBAT_ENDED",
            &combat(&fought),
        );
        assert_eq!(
            twice.breach_tokens, before.breach_tokens,
            "already holds a breach"
        );
        assert_eq!(twice.faction_marks, before.faction_marks, "no mark changed");
        assert_eq!(twice.board, before.board, "no unit moved");
    }

    #[test]
    fn exile_ii_reaches_two_systems_and_may_place_an_active_breach() {
        let (galaxy, ids) = exile_ring();
        let (fought, far) = (
            SystemId::new(ids[0].as_str()),
            SystemId::new(ids[2].as_str()),
        );
        let combat = vec![("system", fought.to_string().into())];
        let mut state = game();
        crate::fixtures::put(&mut state, &far, EXILE_II, &a(), 1);
        emit(
            &mut state,
            Some(&galaxy),
            &[EXILE, "active"],
            "SPACE_COMBAT_ENDED",
            &combat,
        );
        assert!(
            is_active(&state, &fought),
            "active, two systems from the Exile II"
        );
        let mut inactive = game();
        crate::fixtures::put(&mut inactive, &far, EXILE_II, &a(), 1);
        emit(
            &mut inactive,
            Some(&galaxy),
            &[EXILE, "inactive"],
            "SPACE_COMBAT_ENDED",
            &combat,
        );
        assert!(has_breach(&inactive, &fought) && !is_active(&inactive, &fought));
        let mut declined = game();
        crate::fixtures::put(&mut declined, &far, EXILE_II, &a(), 1);
        emit(
            &mut declined,
            Some(&galaxy),
            &[EXILE, "decline"],
            "SPACE_COMBAT_ENDED",
            &combat,
        );
        assert!(!has_breach(&declined, &fought));
    }

    #[test]
    fn the_exile_destroyers_and_the_exile_ii_upgrade_are_the_corpus_stats() {
        let types = ti4_content::units::catalogue(content(), DEFAULT);
        let one = types.get(EXILE_I).expect("Exile I");
        assert_eq!(
            (one.combat_hits_on(), one.afb_hits_on(), one.afb_dice()),
            (Some(8), Some(9), 2)
        );
        assert_eq!(one.upgrades_to(), Some(EXILE_II));
        let two = types.get(EXILE_II).expect("Exile II");
        assert_eq!(
            (two.combat_hits_on(), two.afb_hits_on(), two.afb_dice()),
            (Some(7), Some(6), 3)
        );
        assert_eq!(two.required_technology(), Some("exile2"));
        // The faction technology upgrades the destroyers in place, through the real route.
        let mut state = game();
        let home = home(&state);
        let before = state
            .system_state(&home)
            .units
            .iter()
            .filter(|unit| unit.owner == a() && unit.type_id.as_str() == EXILE_I)
            .count();
        assert!(before > 0, "the starting fleet carries Exile I destroyers");
        crate::technology::grant(
            &mut state,
            &a(),
            &ti4_model::id::TechnologyId::new("exile2"),
        );
        crate::technology::apply_unit_upgrades(&mut state, content(), DEFAULT, &a());
        let after = state
            .system_state(&home)
            .units
            .iter()
            .filter(|unit| unit.owner == a() && unit.type_id.as_str() == EXILE_II)
            .count();
        assert_eq!(after, before, "every Exile I became an Exile II");
    }

    // -- Revenant ----------------------------------------------------------------------------------------

    fn deploy_setup() -> (GameState, SystemId, ti4_model::id::PlanetId) {
        let mut state = game();
        let (system, planet) = super::super::deepwrought::testkit::plain_planet();
        put_breach(&mut state, &a(), &system, true);
        (state, system, planet)
    }

    fn invade(
        state: &mut GameState,
        who: &PlayerId,
        prefer: &[&str],
        system: &SystemId,
    ) -> crate::invasion::InvasionReport {
        let (mut table, _) = super::super::deepwrought::testkit::steer(prefer);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(7);
        crate::invasion::resolve(
            state,
            content(),
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            system,
            who,
        )
        .expect("the invasion resolves")
    }

    fn commit_offer(state: &GameState, who: &PlayerId, system: &SystemId) -> Vec<String> {
        use crate::choice::Window;
        let mut state = state.clone();
        let window = crate::invasion::InvasionWindow::at_commit_step(&mut state, who, system);
        window
            .pending_choice(&state, content(), DEFAULT)
            .map(|choice| choice.options.iter().map(|o| o.id.clone()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_revenant_deploys_and_commits_with_no_units_in_an_active_breach_system() {
        let (mut state, system, planet) = deploy_setup();
        let offer = commit_offer(&state, &a(), &system);
        assert!(
            offer.contains(&format!("deploy_commit|{planet}")),
            "{offer:?}"
        );
        let mechs_before = crate::supply::remaining(
            &state,
            content(),
            DEFAULT,
            &a(),
            &ti4_model::id::UnitTypeId::new(MECH),
        );
        let report = invade(
            &mut state,
            &a(),
            &[
                format!("deploy_commit|{planet}").as_str(),
                "done_committing",
            ],
            &system,
        );
        assert!(report.committed.contains(&planet));
        assert_eq!(
            state
                .system_state(&system)
                .on_planet_of(&planet, &a())
                .iter()
                .filter(|unit| unit.type_id.as_str() == MECH)
                .count(),
            1,
            "a Revenant stands on the planet"
        );
        assert_eq!(
            crate::supply::remaining(
                &state,
                content(),
                DEFAULT,
                &a(),
                &ti4_model::id::UnitTypeId::new(MECH)
            ),
            mechs_before - 1,
            "it came from reinforcements"
        );
        assert!(
            report
                .captured
                .iter()
                .any(|(captured, _)| *captured == planet),
            "an empty planet is taken"
        );
    }

    #[test]
    fn the_revenant_is_not_deployed_without_an_active_breach_a_mech_or_the_rebellion() {
        let (state, system, planet) = deploy_setup();
        let offer = format!("deploy_commit|{planet}");
        // Another faction cannot.
        assert!(!commit_offer(&state, &b(), &system).contains(&offer));
        // An inactive breach.
        let mut inactive = state.clone();
        flip_breach(&mut inactive, &system);
        assert!(!commit_offer(&inactive, &a(), &system).contains(&offer));
        // No mech left in reinforcements.
        let mut spent = state.clone();
        let (elsewhere, spot) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut spent, &elsewhere, &spot, MECH, &a(), 4);
        assert!(!commit_offer(&spent, &a(), &system).contains(&offer));
        assert_eq!(
            deploy_planets(
                &state,
                content(),
                DEFAULT,
                &a(),
                &system,
                std::slice::from_ref(&planet)
            ),
            vec![planet]
        );
    }

    #[test]
    fn a_tactical_action_opens_the_commit_step_for_a_deploy_with_no_ship_in_the_system() {
        let ids = plain(7);
        let ring: Vec<&str> = ids[..6].iter().map(String::as_str).collect();
        let galaxy = ring_map(&ids[6], &ring);
        let system = SystemId::new(ids[0].as_str());
        let planet = ti4_content::galaxy::system(content(), system.as_str(), DEFAULT)
            .unwrap()
            .planets()
            .first()
            .map(|planet| ti4_model::id::PlanetId::new(*planet))
            .expect("a planet");
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        put_breach(&mut state, &a(), &system, true);
        let deploy = format!("deploy_commit|{planet}");
        // The Incursion flip is declined: the breach stays active for the deploy.
        let table = scripted(&[
            crate::game::TACTICAL_ACTION_ID,
            system.as_str(),
            "decline",
            "done_moving",
            deploy.as_str(),
            "done_committing",
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
        assert!(
            game.state
                .system_state(&system)
                .on_planet_of(&planet, &a())
                .iter()
                .any(|unit| unit.type_id.as_str() == MECH),
            "the Revenant landed with nothing else of the Rebellion's in the system"
        );
        assert_eq!(
            game.state.system_state(&system).planet_control.get(&planet),
            Some(&a()),
            "and took the empty planet"
        );
    }

    // -- Resonance Generator: movement --------------------------------------------------------------------

    #[test]
    fn the_resonance_generator_adds_one_to_ships_starting_at_home_or_in_an_active_breach() {
        let mut state = game();
        let breach = SystemId::new("25");
        put_breach(&mut state, &a(), &breach, true);
        let quiet = SystemId::new("26");
        let inactive = SystemId::new("27");
        put_breach(&mut state, &a(), &inactive, false);
        let types = ti4_content::units::catalogue(content(), DEFAULT);
        let cruiser = types.get("cruiser").unwrap();
        let home = home(&state);
        let value = |state: &GameState, origin: &SystemId, player: &PlayerId| {
            crate::tactical::effective_move_value(state, cruiser, player, origin)
        };
        let base = value(&state, &quiet, &a());
        // Crimson starts with it; take it away to see the bonus is the breakthrough's.
        state.player_mut(&a()).unwrap().breakthrough = None;
        assert_eq!(
            value(&state, &home, &a()),
            base,
            "no breakthrough, no bonus"
        );
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new(BREAKTHROUGH));
        assert_eq!(value(&state, &home, &a()), base + 1, "starting at home");
        assert_eq!(
            value(&state, &breach, &a()),
            base + 1,
            "starting in an active breach"
        );
        assert_eq!(value(&state, &quiet, &a()), base);
        assert_eq!(
            value(&state, &inactive, &a()),
            base,
            "an inactive breach gives nothing"
        );
        assert_eq!(value(&state, &home, &b()), base, "only its holder");
    }
}

/// Phase 4 of the observation-gaps plan: effects that say "adjacent" see active breaches (which
/// are adjacency for every player). Each test fails without the link-aware lookup.
#[cfg(test)]
mod adjacency_audit_tests {
    use ti4_content::galaxy::Galaxy;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{PlanetId, SystemId};
    use ti4_model::state::GameState;

    use super::testkit::{b, c, content, game, link, plain, ring_map};

    /// A ring map and a state whose systems `first` and `opposite` are two apart on the ring.
    fn world() -> (Galaxy, GameState, String, String) {
        let ring = plain(6);
        let ring: Vec<&str> = ring.iter().map(String::as_str).collect();
        let centre = plain(7).pop().unwrap();
        let galaxy = ring_map(&centre, &ring);
        let mut state = game();
        for id in ring.iter().chain(std::iter::once(&centre.as_str())) {
            state.board.entry(SystemId::new(*id)).or_default();
        }
        (galaxy, state, ring[0].to_owned(), ring[3].to_owned())
    }

    fn sys(id: &str) -> SystemId {
        SystemId::new(id)
    }

    fn first_planet(system: &str) -> PlanetId {
        let planets = ti4_content::galaxy::planets_in(content(), system, DEFAULT);
        PlanetId::new(planets[0].id())
    }

    #[test]
    fn retreats_may_cross_a_breach_link() {
        let (galaxy, mut state, first, opposite) = world();
        crate::fixtures::put(&mut state, &sys(&first), "cruiser", &b(), 1);
        crate::fixtures::put(&mut state, &sys(&opposite), "cruiser", &b(), 1);
        let ask = |state: &GameState| {
            (
                crate::combat::eligible_retreats(
                    state, content(), DEFAULT, &galaxy, &b(), &sys(&first),
                ),
                crate::combat::skilled_retreat_destinations(
                    state, content(), DEFAULT, &galaxy, &b(), &sys(&first),
                ),
                crate::combat::det_retreat_destinations(
                    state, content(), DEFAULT, &galaxy, &b(), &sys(&first),
                ),
            )
        };
        let (retreat, skilled, det) = ask(&state);
        assert!(!retreat.contains(&sys(&opposite)));
        assert!(!skilled.contains(&sys(&opposite)));
        assert!(!det.contains(&sys(&opposite)));
        link(&mut state, &first, &opposite);
        let (retreat, skilled, det) = ask(&state);
        assert!(retreat.contains(&sys(&opposite)), "78.7c retreat");
        assert!(skilled.contains(&sys(&opposite)), "Skilled Retreat");
        assert!(det.contains(&sys(&opposite)), "Dark Energy Tap");
    }

    #[test]
    fn space_cannon_reaches_across_a_breach_link() {
        let (galaxy, mut state, first, opposite) = world();
        crate::fixtures::put(&mut state, &sys(&first), "cruiser", &c(), 1);
        crate::fixtures::put_on_planet(
            &mut state,
            &sys(&opposite),
            &first_planet(&opposite),
            "pds2",
            &b(),
            1,
        );
        let guns = |state: &GameState| {
            crate::combat::space_cannon_guns(
                state, content(), DEFAULT, &sys(&first), &c(), Some(&galaxy),
            )
            .units
            .len()
        };
        assert_eq!(guns(&state), 0, "two apart");
        link(&mut state, &first, &opposite);
        assert_eq!(guns(&state), 1, "a PDS II fires through the breach link");
    }

    #[test]
    fn signal_jamming_reaches_a_breach_linked_system() {
        let (galaxy, mut state, first, opposite) = world();
        crate::fixtures::put(&mut state, &sys(&first), "cruiser", &b(), 1);
        let reach = |state: &GameState| {
            crate::action_cards::jamming_systems(state, content(), DEFAULT, Some(&galaxy), &b())
        };
        assert!(!reach(&state).contains(&opposite));
        link(&mut state, &first, &opposite);
        assert!(reach(&state).contains(&opposite));
    }

    #[test]
    fn annexing_and_the_stellar_converter_reach_a_breach_linked_system() {
        let (galaxy, mut state, first, opposite) = world();
        state
            .system_mut(&sys(&first))
            .planet_control
            .insert(first_planet(&first), b());
        crate::fixtures::put(&mut state, &sys(&first), "dreadnought", &b(), 1);
        let annex = |state: &GameState| {
            crate::faction_abilities::annexable(state, content(), DEFAULT, &galaxy, &b())
                .iter()
                .any(|(system, _)| system.as_str() == opposite)
        };
        let converter = |state: &GameState| {
            crate::relics::stellar_converter_targets(state, content(), DEFAULT, &galaxy, &b())
                .iter()
                .any(|(system, _)| system.as_str() == opposite)
        };
        assert!(!annex(&state) && !converter(&state));
        link(&mut state, &first, &opposite);
        assert!(annex(&state), "Civil Accords annexation");
        assert!(converter(&state), "Stellar Converter");
    }

    #[test]
    fn a_space_dock_vortex_and_the_mahact_hero_see_a_breach_link() {
        let (galaxy, mut state, first, opposite) = world();
        crate::fixtures::put_on_planet(
            &mut state,
            &sys(&first),
            &first_planet(&first),
            "spacedock",
            &c(),
            1,
        );
        crate::fixtures::put(&mut state, &sys(&first), "cruiser", &c(), 1);
        crate::fixtures::put(&mut state, &sys(&opposite), "cruiser", &b(), 1);
        let vortex = |state: &GameState| {
            !crate::factions::cabal::vortex_targets(state, content(), DEFAULT, &galaxy, &c())
                .is_empty()
        };
        let hero = |state: &GameState| {
            crate::factions::mahact_units::hero_pairs(state, content(), DEFAULT, &galaxy, &c())
                .contains(&(sys(&first), sys(&opposite)))
        };
        assert!(!vortex(&state) && !hero(&state));
        link(&mut state, &first, &opposite);
        assert!(vortex(&state), "Cabal Vortex");
        assert!(hero(&state), "Mahact hero");
    }

    #[test]
    fn a_ship_beside_an_anomaly_through_a_breach_link_counts_for_the_secret() {
        let ring = plain(6);
        let ring: Vec<&str> = ring.iter().map(String::as_str).collect();
        let centre = plain(7).pop().unwrap();
        let homes = ti4_content::galaxy::home_systems(content(), DEFAULT);
        let anomaly = ti4_content::galaxy::all_systems(content(), DEFAULT)
            .iter()
            .find(|(id, system)| {
                system.is_anomaly() && !system.is_hyperlane() && !homes.contains(*id) && **id != "18"
            })
            .map(|(id, _)| (*id).to_owned())
            .expect("an anomaly tile");
        let mut placed = ring.clone();
        placed[3] = anomaly.as_str();
        let galaxy = ring_map(&centre, &placed);
        let mut state = game();
        crate::fixtures::put(&mut state, &sys(placed[0]), "cruiser", &b(), 1);
        let count = |state: &GameState| {
            let position = crate::secrets::Position {
                state,
                content: content(),
                sources: DEFAULT,
                player: &b(),
                galaxy: Some(&galaxy),
                imagined: crate::objectives::Imagined::NONE,
            };
            crate::secrets::ship_systems_beside_anomaly(&position)
                .map_or(0, |systems| systems.len())
        };
        assert_eq!(count(&state), 0);
        link(&mut state, placed[0], placed[3]);
        assert_eq!(count(&state), 1);
    }
}
