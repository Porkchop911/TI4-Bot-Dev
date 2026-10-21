//! The Crimson Rebellion's cards: Subatomic Splicer, Sever, Ahk Ravin, Ahk Siever, Homesick Phantom
//! and the Resonance Generator's ACTION. See `crimson.rs` for the breach model and
//! `plans/evidence/BF-crimson.md`.
//!
//! Card texts (latest printing):
//!
//! * Subatomic Splicer (`subatomic`): "When one of your ships is destroyed, you may produce a ship of
//!   the same type at a space dock in your home system."
//! * Sever (promissory note): "ACTION: place this card face up in your play area, and place the sever
//!   token in a system that contains your units, wormholes in that system have no effect during
//!   movement. Remove the sever token and return this card to the Rebellion player at end of the
//!   status phase."
//! * Ahk Ravin (`crimsonagent`): "ACTION: Exhaust this card to choose 1 player. That player may swap
//!   the position of 2 of their ships in any systems; they may transport units when they swap."
//!   Per Dane a swap is not a move.
//! * Ahk Siever (`crimsoncommander`): "At the end of a combat between any players: Gain 1 commodity or
//!   convert 1 of your commodities to a trade good." Unlock: "Place a breach token in a system that
//!   contains another player's unit." (`crimson::put_breach`.) The payment is the pre-existing
//!   `borrowed_commanders::crimson_commander`, registered for every seat and gated on
//!   `promissory::has_commander_ability`, so the owner, an Alliance holder, Yin, Mahact Imperia and a
//!   Nekro with the Z breakthrough all reach it; this module owns the unlock, applied the moment the
//!   breach is placed so the commander is live in the very combat that unlocked it.
//! * Homesick Phantom (`crimsonhero`): "When you produce ships: You may place any of those ships onto
//!   this card. At the start of a space combat, you may purge this card to place all ships from this
//!   card into the active system." Unlock: 3 scored objectives (generic). Per Dane it cannot be
//!   played in a combat the owner is not in, and placing ships onto it ignores fleet supply and
//!   capacity in the producing system.
//!
//! # The card
//!
//! Ships on the hero are the mark `crimson:card:<player>`: their unit type ids, comma-separated. They
//! are off the board but still out of the owner's reinforcements (`supply::held` reads
//! [`held_on_card`]). A produced ship is placed onto the card through the placement spot
//! [`CARD_SYSTEM`]`@space`, offered by [`offer_card_spot`] (`production.rs`), so it is an ordinary
//! placement choice, not a second window.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::crimson::{self, BREAKTHROUGH, FACTION, ask};
use crate::choice::{ChoiceOption, IllegalChoice, Resolving};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// Subatomic Splicer.
pub const SUBATOMIC: &str = "subatomic";
/// Sever.
pub const SEVER: &str = "sever";
/// Ahk Ravin.
pub const AGENT: &str = "crimsonagent";
/// Homesick Phantom.
pub const HERO: &str = "crimsonhero";

/// The pseudo-system of a placement spot that puts a produced ship onto the hero card.
pub const CARD_SYSTEM: &str = "crimsoncard";

const SEVER_ACTION: &str = "faction|crimson|sever";
const RESONANCE_ACTION: &str = "faction|crimson|resonance";
const SEVER_PREFIX: &str = "crimson:sever:";
const CARD_PREFIX: &str = "crimson:card:";
const BT_EXHAUSTED_PREFIX: &str = "crimson:bt:exhausted:";

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

fn bt_exhausted_key(player: &PlayerId) -> String {
    format!("{BT_EXHAUSTED_PREFIX}{player}")
}

fn card_key(player: &PlayerId) -> String {
    format!("{CARD_PREFIX}{player}")
}

fn sever_key(note: &str) -> String {
    format!("{SEVER_PREFIX}{note}")
}

// -- Subatomic Splicer -----------------------------------------------------------------------------

/// A space dock of `player` on a planet of their home system, with the home system.
fn home_dock(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Option<SystemId> {
    let home = state.player(player)?.home_system.clone()?;
    let types = ti4_content::units::catalogue(content, sources);
    state
        .system_state(&home)
        .planet_units
        .values()
        .flatten()
        .any(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == "spacedock")
        })
        .then_some(home)
}

/// "When one of your ships is destroyed, you may produce a ship of the same type at a space dock in
/// your home system": paid for as ordinary production, placed in the home system's space area, and
/// offered only when that production can happen. The destroyed ship is already off the board when
/// the event is announced, so this reads as "after" it.
fn subatomic_splicer(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{SUBATOMIC}:SHIP_DESTROYED:after"),
        seat.clone(),
        "SHIP_DESTROYED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(unit) = event.text("unit") else {
                return Ok(());
            };
            let Some(home) = home_dock(context.state, context.content, context.sources, &owner)
            else {
                return Ok(());
            };
            produce_ship(context, &owner, &home, unit).map_err(illegal)
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let Some(unit) = event.text("unit") else {
            return false;
        };
        event.text("player") == Some(condition_owner.as_str())
            && crate::technology::has_technology_text(context.state, &condition_owner, SUBATOMIC)
            && ti4_content::units::catalogue(context.content, context.sources)
                .get(unit)
                .is_some_and(ti4_content::units::UnitType::is_ship)
            && home_dock(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_some_and(|home| {
                crate::production::can_produce_unit_by_ability(
                    context.state,
                    context.content,
                    context.sources,
                    &condition_owner,
                    &home,
                    unit,
                )
            })
    }))
}

fn produce_ship(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    home: &SystemId,
    unit: &str,
) -> Result<(), IllegalChoice> {
    let TimingContext {
        state,
        content,
        sources,
        table,
        dice,
        rng,
        galaxy,
        ..
    } = context;
    let galaxy = *galaxy;
    let mut ctx = Resolving {
        content,
        sources: *sources,
        dice,
        rng,
        table,
        timing: None,
    };
    crate::production::produce_unit_by_ability(state, &mut ctx, galaxy, player, home, unit)
        .map(|_| ())
}

// -- Sever ----------------------------------------------------------------------------------------------

/// The systems that hold units of `player`.
fn systems_with_units(state: &GameState, player: &PlayerId) -> Vec<SystemId> {
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

/// The Sever notes `player` holds in hand and could play now: they need a system with their units.
fn playable_severs(state: &GameState, player: &PlayerId) -> Vec<String> {
    if systems_with_units(state, player).is_empty() {
        return Vec::new();
    }
    crate::promissory::action_notes_in_hand(state, player)
        .into_iter()
        .filter(|note| crate::promissory::alias_of(note) == SEVER)
        .collect()
}

/// Systems whose wormholes have no effect during movement: where a Sever token lies.
pub(crate) fn severed_systems(state: &GameState) -> Vec<String> {
    state
        .faction_marks
        .range(SEVER_PREFIX.to_owned()..)
        .take_while(|(key, _)| key.starts_with(SEVER_PREFIX))
        .filter_map(|(_, value)| value.split_once('|').map(|(_, system)| system.to_owned()))
        .collect()
}

fn sever(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let Some(note) = playable_severs(context.state, player).into_iter().next() else {
        return false;
    };
    let systems = systems_with_units(context.state, player);
    let system = match systems.as_slice() {
        [] => return false,
        [only] => only.clone(),
        _ => {
            let Ok(answer) = ask(
                context,
                player,
                "Sever: place the sever token in which system".to_owned(),
                SEVER,
                "sever_system",
                systems
                    .iter()
                    .map(|system| {
                        ChoiceOption::labelled(
                            system.to_string(),
                            "system",
                            format!("sever the wormholes of {system}"),
                        )
                    })
                    .chain(std::iter::once(ChoiceOption::decline()))
                    .collect(),
            ) else {
                return false;
            };
            let Some(chosen) = systems
                .into_iter()
                .find(|system| system.as_str() == answer.id)
            else {
                return false;
            };
            chosen
        }
    };
    if !crate::promissory::play_action_note(context.state, player, &note) {
        return false;
    }
    context
        .state
        .faction_marks
        .insert(sever_key(&note), format!("{player}|{system}"));
    true
}

/// "Remove the sever token and return this card to the Rebellion player at end of the status phase."
fn sever_returns(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let pending = |state: &GameState, who: &PlayerId| -> Vec<String> {
        state
            .faction_marks
            .range(SEVER_PREFIX.to_owned()..)
            .take_while(|(key, _)| key.starts_with(SEVER_PREFIX))
            .filter_map(|(key, value)| {
                let (holder, _) = value.split_once('|')?;
                (holder == who.as_str()).then(|| key[SEVER_PREFIX.len()..].to_owned())
            })
            .collect()
    };
    Ability::stateful(
        format!("promissory:{owner_name}:sever_return:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            for note in pending(context.state, &owner) {
                crate::promissory::give_back(context.state, &note);
                context.state.faction_marks.remove(&sever_key(&note));
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        !pending(context.state, &condition_seat).is_empty()
    }))
}

// -- Resonance Generator --------------------------------------------------------------------------------

fn resonance_ready(state: &GameState, player: &PlayerId) -> bool {
    crate::breakthroughs::holds(state, player, BREAKTHROUGH)
        && !state.faction_marks.contains_key(&bt_exhausted_key(player))
}

/// The systems `player` could place an active breach in: not a home system, holding a unit of theirs.
fn resonance_placements(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    let homes: std::collections::BTreeSet<&str> =
        ti4_content::galaxy::home_systems(content, sources);
    systems_with_units(state, player)
        .into_iter()
        .filter(|system| {
            !homes.contains(system.as_str())
                && state
                    .players
                    .iter()
                    .all(|seat| seat.home_system.as_ref() != Some(system))
                && crimson::can_place_breach(state, system)
        })
        .collect()
}

fn resonance_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let mut options: Vec<ChoiceOption> = state
        .breach_tokens
        .iter()
        .map(|system| {
            ChoiceOption::labelled(
                format!("flip|{system}"),
                "breach",
                format!("flip the breach in {system}"),
            )
        })
        .collect();
    options.extend(
        resonance_placements(state, content, sources, player)
            .into_iter()
            .map(|system| {
                ChoiceOption::labelled(
                    format!("place|{system}"),
                    "breach",
                    format!("place an active breach in {system}"),
                )
            }),
    );
    options
}

fn resonance(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    if !resonance_ready(context.state, player) {
        return false;
    }
    let mut options = resonance_options(context.state, context.content, context.sources, player);
    if options.is_empty() {
        return false;
    }
    options.push(ChoiceOption::decline());
    let Ok(answer) = ask(
        context,
        player,
        "Resonance Generator: flip a breach or place an active breach".to_owned(),
        BREAKTHROUGH,
        "resonance_generator",
        options,
    ) else {
        return false;
    };
    let done = if let Some(system) = answer.id.strip_prefix("flip|") {
        crimson::flip_breach(context.state, &SystemId::new(system))
    } else if let Some(system) = answer.id.strip_prefix("place|") {
        let system = SystemId::new(system);
        resonance_placements(context.state, context.content, context.sources, player)
            .contains(&system)
            && crimson::place_breach(context, player, &system, true, BREAKTHROUGH).unwrap_or(false)
    } else {
        false
    };
    if done {
        context
            .state
            .faction_marks
            .insert(bt_exhausted_key(player), "1".to_owned());
    }
    done
}

fn resonance_readies(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:{BREAKTHROUGH}_ready:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context
                .state
                .faction_marks
                .remove(&bt_exhausted_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&bt_exhausted_key(&condition_owner))
    }))
}

// -- Homesick Phantom ----------------------------------------------------------------------------------

/// The ships on `player`'s hero card, by unit type id, in the order they were placed.
#[must_use]
pub fn card_ships(state: &GameState, player: &PlayerId) -> Vec<String> {
    state
        .faction_marks
        .get(&card_key(player))
        .map(|ships| {
            ships
                .split(',')
                .filter(|ship| !ship.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// How many plastic pieces of `base_type` `player` has on the hero card, for the reinforcement count
/// (`supply::held`): off the board, but not in the box.
#[must_use]
pub fn held_on_card(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> i64 {
    let ships = card_ships(state, player);
    if ships.is_empty() {
        return 0;
    }
    let types = ti4_content::units::catalogue(content, sources);
    i64::try_from(
        ships
            .iter()
            .filter(|ship| {
                types
                    .get(ship.as_str())
                    .map_or(ship.as_str(), |kind| kind.base_type())
                    == base_type
            })
            .count(),
    )
    .unwrap_or(0)
}

/// Whether `player` holds an unlocked Homesick Phantom.
fn hero_ready(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.leaders.get(&LeaderId::new(HERO)) == Some(&LeaderStatus::Unlocked))
}

/// Offer the hero card as a placement spot for a produced ship (called by
/// `production::ProductionWindow::spots` for a ship, when the producing system has a legal spot).
pub(crate) fn offer_card_spot(state: &GameState, player: &PlayerId, spots: &mut Vec<String>) {
    if !hero_ready(state, player) {
        return;
    }
    let spot = format!("{CARD_SYSTEM}{}space", crate::production::REMOTE_SEPARATOR);
    if !spots.contains(&spot) {
        spots.push(spot);
    }
}

/// Put one produced ship on the card (called by `production::ProductionWindow::place`).
pub(crate) fn put_on_card(state: &mut GameState, player: &PlayerId, unit: &str) {
    let mut ships = card_ships(state, player);
    ships.push(unit.to_owned());
    state
        .faction_marks
        .insert(card_key(player), ships.join(","));
}

/// "At the start of a space combat, you may purge this card to place all ships from this card into
/// the active system." Only for a combat its owner fights in (Dane).
fn launch(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{HERO}:SPACE_COMBAT_STARTED:after"),
        seat.clone(),
        "SPACE_COMBAT_STARTED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let ships = card_ships(context.state, &owner);
            if ships.is_empty() || !hero_ready(context.state, &owner) {
                return Ok(());
            }
            crate::leaders::purge(context.state, &owner, &LeaderId::new(HERO));
            context.state.faction_marks.remove(&card_key(&owner));
            for ship in ships {
                context
                    .state
                    .system_mut(&system)
                    .units
                    .push(Unit::new(UnitTypeId::new(ship), owner.clone()));
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        hero_ready(context.state, &condition_owner)
            && !card_ships(context.state, &condition_owner).is_empty()
            && (event.text("attacker") == Some(condition_owner.as_str())
                || event.text("defender") == Some(condition_owner.as_str()))
    }))
}

// -- Ahk Ravin --------------------------------------------------------------------------------------------

/// A distinguishable ship of `player`: where it stands and what it is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Hull {
    system: SystemId,
    unit: Unit,
}

impl Hull {
    fn id(&self) -> String {
        format!(
            "{}|{}|{}",
            self.system,
            self.unit.type_id,
            if self.unit.sustained_damage {
                "damaged"
            } else {
                "whole"
            }
        )
    }

    fn option(&self, label: String) -> ChoiceOption {
        let key = self.id();
        ChoiceOption::labelled(key, "ship", label)
    }

    fn label(&self) -> String {
        format!(
            "{}{} in {}",
            self.unit.type_id,
            if self.unit.sustained_damage {
                " (damaged)"
            } else {
                ""
            },
            self.system
        )
    }
}

fn hulls(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<Hull> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found = Vec::new();
    for (system, board) in &state.board {
        for unit in &board.units {
            if &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
            {
                found.push(Hull {
                    system: system.clone(),
                    unit: unit.clone(),
                });
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Whether `player` has ships in two different systems, so a swap is possible.
fn can_swap(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    let hulls = hulls(state, content, sources, player);
    hulls
        .first()
        .is_some_and(|first| hulls.iter().any(|other| other.system != first.system))
}

pub(crate) fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == AGENT).then(|| {
        crate::promissory::seat_of(state, FACTION).is_some()
            && state
                .players
                .iter()
                .any(|seat| can_swap(state, content, ti4_model::content_types::DEFAULT, &seat.id))
            && state.player(player).is_some()
    })
}

pub(crate) fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == AGENT).then(|| ahk_ravin(context, player))
}

fn ahk_ravin(context: &mut TimingContext<'_>, user: &PlayerId) -> bool {
    let swappers: Vec<PlayerId> = context
        .state
        .seating_order
        .iter()
        .filter(|seat| can_swap(context.state, context.content, context.sources, seat))
        .cloned()
        .collect();
    let chosen = match swappers.as_slice() {
        [] => return false,
        [only] => only.clone(),
        _ => {
            let Ok(answer) = ask(
                context,
                user,
                "Ahk Ravin: choose a player who may swap two of their ships".to_owned(),
                AGENT,
                "ahk_ravin_player",
                swappers
                    .iter()
                    .map(|seat| {
                        ChoiceOption::labelled(
                            seat.to_string(),
                            "player",
                            format!("{seat} may swap two ships"),
                        )
                    })
                    .collect(),
            ) else {
                return false;
            };
            let Some(chosen) = swappers.into_iter().find(|seat| seat.as_str() == answer.id) else {
                return false;
            };
            chosen
        }
    };
    let before = context.state.clone();
    match swap_ships(context, &chosen) {
        Ok(()) => true,
        Err(_) => {
            *context.state = before;
            false
        }
    }
}

/// The chosen player's own decisions: whether to swap, which two ships, what each carries.
fn swap_ships(context: &mut TimingContext<'_>, who: &PlayerId) -> Result<(), TimingError> {
    let all = hulls(context.state, context.content, context.sources, who);
    let mut first_options: Vec<ChoiceOption> = all
        .iter()
        .filter(|hull| all.iter().any(|other| other.system != hull.system))
        .map(|hull| hull.option(format!("swap {}", hull.label())))
        .collect();
    first_options.push(ChoiceOption::decline());
    let answer = ask(
        context,
        who,
        "Ahk Ravin: swap which ship".to_owned(),
        AGENT,
        "swap_first",
        first_options,
    )?;
    let Some(first) = all.iter().find(|hull| hull.id() == answer.id).cloned() else {
        return Ok(()); // the "may": declined
    };
    let others: Vec<&Hull> = all
        .iter()
        .filter(|hull| hull.system != first.system)
        .collect();
    let answer = ask(
        context,
        who,
        format!("Ahk Ravin: swap {} with which ship", first.label()),
        AGENT,
        "swap_second",
        others
            .iter()
            .map(|hull| hull.option(format!("with {}", hull.label())))
            .collect(),
    )?;
    let Some(second) = others
        .into_iter()
        .find(|hull| hull.id() == answer.id)
        .cloned()
    else {
        return Ok(());
    };
    // "They may transport units when they swap": each ship carries what it could pick up in the
    // system it leaves, up to its capacity. Both holds are read before either ship moves.
    let first_cargo = fill_hold(context, who, &first)?;
    let second_cargo = fill_hold(context, who, &second)?;
    // Everything is decided; now the two ships and their passengers change places.
    let state = &mut *context.state;
    state
        .system_mut(&first.system)
        .remove(std::slice::from_ref(&first.unit));
    state
        .system_mut(&second.system)
        .remove(std::slice::from_ref(&second.unit));
    carry(state, &first.system, &second.system, &first_cargo);
    carry(state, &second.system, &first.system, &second_cargo);
    state.system_mut(&second.system).units.push(first.unit);
    state.system_mut(&first.system).units.push(second.unit);
    Ok(())
}

/// Fill one ship's hold from the system it is leaving.
fn fill_hold(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    hull: &Hull,
) -> Result<Vec<crate::transit::Cargo>, TimingError> {
    let mut hold = crate::transit::CargoWindow::for_ship(
        context.state,
        context.content,
        context.sources,
        who,
        &hull.system,
        &hull.unit,
        &[],
    );
    while let Some(choice) = hold.pending_choice() {
        let choice =
            choice.contextualized(crimson::decision(context.state, who, AGENT, "swap_cargo"));
        let answer = context.ask_seeing(&choice).map_err(illegal)?;
        hold.resolve(answer).map_err(|error| {
            illegal(IllegalChoice::DeciderFailed {
                player: who.clone(),
                prompt: "Ahk Ravin: transport".to_owned(),
                reason: error.to_string(),
            })
        })?;
    }
    Ok(hold.cargo())
}

/// Move the passengers out of `from` and into the space area of `to`.
fn carry(state: &mut GameState, from: &SystemId, to: &SystemId, cargo: &[crate::transit::Cargo]) {
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

// -- hooks ------------------------------------------------------------------------------------------------------

pub(crate) fn timing_abilities(
    _state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    vec![
        subatomic_splicer(owner_name, seat),
        sever_returns(owner_name, seat),
        resonance_readies(owner_name, seat),
        launch(owner_name, seat),
    ]
}

pub(crate) fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let mut actions = Vec::new();
    if !playable_severs(state, player).is_empty() {
        actions.push(ChoiceOption::labelled(
            SEVER_ACTION,
            crate::faction_abilities::ACTION_KIND,
            "Sever: place it faceup and put the sever token in a system with your units",
        ));
    }
    if resonance_ready(state, player)
        && !resonance_options(state, content, ti4_model::content_types::DEFAULT, player).is_empty()
    {
        actions.push(ChoiceOption::labelled(
            RESONANCE_ACTION,
            crate::faction_abilities::ACTION_KIND,
            "Resonance Generator: exhaust to flip a breach or place an active breach",
        ));
    }
    actions
}

pub(crate) fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    match option.id.as_str() {
        SEVER_ACTION => sever(context, player),
        RESONANCE_ACTION => resonance(context, player),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{BreakthroughId, TechnologyId};

    use super::super::crimson::testkit::*;
    use super::super::crimson::{flip_breach, has_breach, is_active};
    use super::*;
    use crate::choice::{Choice, Decider, Table};

    fn home(state: &GameState, who: &PlayerId) -> SystemId {
        state.player(who).unwrap().home_system.clone().unwrap()
    }

    // -- Subatomic Splicer ----------------------------------------------------------------------------

    const SPLICER: &str = "technology:crimson:subatomic:SHIP_DESTROYED:after";

    fn ship_destroyed(state: &mut GameState, owner: &str, unit: &str, answers: &[&str]) {
        emit(
            state,
            None,
            answers,
            "SHIP_DESTROYED",
            &[
                ("system", "18".into()),
                ("player", owner.into()),
                ("unit", unit.into()),
                ("last", false.into()),
                ("cause", "space_combat".into()),
                ("during_space_combat", true.into()),
            ],
        );
    }

    fn at_home(state: &GameState, who: &PlayerId, unit: &str) -> usize {
        state
            .system_state(&home(state, who))
            .units
            .iter()
            .filter(|u| &u.owner == who && u.type_id.as_str() == unit)
            .count()
    }

    #[test]
    fn subatomic_splicer_produces_a_ship_of_the_destroyed_type_at_the_home_dock() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new(SUBATOMIC));
        state.player_mut(&a()).unwrap().trade_goods = 12;
        let before = at_home(&state, &a(), "crimson_destroyer");
        ship_destroyed(&mut state, "a", "crimson_destroyer", &[SPLICER]);
        assert_eq!(at_home(&state, &a(), "crimson_destroyer"), before + 1);
        let flagships = at_home(&state, &a(), "crimson_flagship");
        ship_destroyed(&mut state, "a", "crimson_flagship", &[SPLICER]);
        assert_eq!(
            at_home(&state, &a(), "crimson_flagship"),
            flagships + 1,
            "the flagship is a ship of its own type"
        );
    }

    #[test]
    fn subatomic_splicer_is_a_may_and_needs_the_card_a_ship_the_dock_and_the_means() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new(SUBATOMIC));
        state.player_mut(&a()).unwrap().trade_goods = 12;
        let before = state.clone();
        ship_destroyed(&mut state, "a", "crimson_destroyer", &["decline"]);
        assert_eq!(state, before, "declined");
        // Another player's ship is not "one of your ships".
        ship_destroyed(&mut state, "b", "destroyer", &[]);
        assert_eq!(state, before);
        // A ground force is not a ship.
        ship_destroyed(&mut state, "a", "infantry", &[]);
        assert_eq!(state, before);
        // No technology.
        let mut bare = game();
        bare.player_mut(&a()).unwrap().trade_goods = 12;
        let before = bare.clone();
        ship_destroyed(&mut bare, "a", "crimson_destroyer", &[]);
        assert_eq!(bare, before);
        // No dock at home.
        let mut docks = game();
        crate::technology::grant(&mut docks, &a(), &TechnologyId::new(SUBATOMIC));
        docks.player_mut(&a()).unwrap().trade_goods = 12;
        let h = home(&docks, &a());
        for units in docks.system_mut(&h).planet_units.values_mut() {
            units.retain(|unit| !unit.type_id.as_str().contains("spacedock"));
        }
        let before = docks.clone();
        ship_destroyed(&mut docks, "a", "crimson_destroyer", &[]);
        assert_eq!(docks, before);
        // Nothing to pay with.
        let mut broke = game();
        crate::technology::grant(&mut broke, &a(), &TechnologyId::new(SUBATOMIC));
        broke.player_mut(&a()).unwrap().trade_goods = 0;
        for planet in broke
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect::<Vec<_>>()
        {
            broke.exhaust_planet(planet);
        }
        let before = broke.clone();
        ship_destroyed(&mut broke, "a", "crimson_destroyer", &[]);
        assert_eq!(broke, before);
    }

    #[test]
    fn a_nekro_assimilating_subatomic_splicer_uses_its_text() {
        let mut state = crate::fixtures::seated_game(&[("a", "nekro"), ("b", "crimson")], DEFAULT);
        crate::technology::grant(&mut state, &b(), &TechnologyId::new(SUBATOMIC));
        state
            .player_mut(&a())
            .unwrap()
            .assimilated_technologies
            .insert("vax".to_owned(), TechnologyId::new(SUBATOMIC));
        state.player_mut(&a()).unwrap().trade_goods = 12;
        assert!(crate::technology::has_technology_text(
            &state,
            &a(),
            SUBATOMIC
        ));
        let before = at_home(&state, &a(), "cruiser");
        ship_destroyed(
            &mut state,
            "a",
            "cruiser",
            &["technology:nekro:subatomic:SHIP_DESTROYED:after"],
        );
        assert_eq!(at_home(&state, &a(), "cruiser"), before + 1);
    }

    // -- Sever --------------------------------------------------------------------------------------------

    const SEVER_NOTE: &str = "sever:crimson";

    fn perform(state: &mut GameState, who: &PlayerId, option: &str, answers: &[&str]) -> bool {
        let option = ChoiceOption::labelled(option, "component", option);
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(answers), |ctx| {
            perform_component(ctx, who, &option)
        })
    }

    fn alpha_ring() -> (Galaxy, Vec<String>) {
        let alpha = wormhole_tiles("ALPHA");
        let others = plain(5);
        let ids = vec![
            alpha[0].clone(),
            others[0].clone(),
            others[1].clone(),
            alpha[1].clone(),
            others[2].clone(),
            others[3].clone(),
        ];
        let ring: Vec<&str> = ids.iter().map(String::as_str).collect();
        (ring_map(&others[4], &ring), ids)
    }

    use ti4_content::galaxy::Galaxy;

    fn reaches(
        state: &GameState,
        galaxy: &Galaxy,
        player: &PlayerId,
        from: &str,
        to: &str,
    ) -> bool {
        crate::tactical::movable_into(
            state,
            content(),
            DEFAULT,
            galaxy,
            player,
            &SystemId::new(to),
        )
        .iter()
        .any(|ship| ship.origin.as_str() == from)
    }

    #[test]
    fn sever_is_played_for_an_action_and_silences_a_systems_wormholes_for_all_movement_until_the_status_phase_ends()
     {
        let (galaxy, ids) = alpha_ring();
        let mut state = game();
        state.promissory_notes.insert(SEVER_NOTE.to_owned(), b());
        let start = SystemId::new(ids[0].as_str());
        crate::fixtures::put(&mut state, &start, "carrier", &b(), 1);
        crate::fixtures::put(&mut state, &start, "carrier", &c(), 1);
        assert!(
            reaches(&state, &galaxy, &b(), &ids[0], &ids[3]),
            "an alpha wormhole at first"
        );
        let offered = component_actions(&state, content(), &b());
        assert!(
            offered.iter().any(|option| option.id == SEVER_ACTION),
            "{offered:?}"
        );
        // The owner never plays her own copy.
        assert!(
            component_actions(&state, content(), &a())
                .iter()
                .all(|o| o.id != SEVER_ACTION)
        );
        assert!(perform(&mut state, &b(), SEVER_ACTION, &[ids[0].as_str()]));
        assert!(
            state.promissory_faceup.contains(SEVER_NOTE),
            "faceup in the play area"
        );
        assert_eq!(severed_systems(&state), vec![ids[0].clone()]);
        assert!(
            !reaches(&state, &galaxy, &b(), &ids[0], &ids[3]),
            "no effect during movement"
        );
        assert!(
            !reaches(&state, &galaxy, &c(), &ids[0], &ids[3]),
            "for every player's movement"
        );
        assert!(
            component_actions(&state, content(), &b())
                .iter()
                .all(|o| o.id != SEVER_ACTION),
            "the card is in play, not in hand"
        );
        // The end of the status phase returns the card and removes the token.
        emit(&mut state, None, &[], "STATUS_PHASE_ENDED", &[]);
        assert_eq!(
            state.promissory_notes.get(SEVER_NOTE),
            Some(&a()),
            "back with the Rebellion"
        );
        assert!(!state.promissory_faceup.contains(SEVER_NOTE));
        assert!(severed_systems(&state).is_empty());
        assert!(
            reaches(&state, &galaxy, &b(), &ids[0], &ids[3]),
            "the wormhole works again"
        );
    }

    #[test]
    fn sever_may_be_played_in_a_system_with_no_wormhole_and_needs_a_system_with_units() {
        let mut state = game();
        state.promissory_notes.insert(SEVER_NOTE.to_owned(), b());
        let spot = SystemId::new(plain(1).pop().unwrap());
        // b's only units are at home: one system, no question, and it has no wormhole.
        assert!(perform(&mut state, &b(), SEVER_ACTION, &[]));
        assert_eq!(
            severed_systems(&state),
            vec![home(&state, &b()).to_string()]
        );
        let _ = spot;
        // Without units anywhere the action is not offered.
        let mut empty = game();
        empty.promissory_notes.insert(SEVER_NOTE.to_owned(), b());
        empty.board.clear();
        assert!(
            component_actions(&empty, content(), &b())
                .iter()
                .all(|o| o.id != SEVER_ACTION)
        );
        let before = empty.clone();
        assert!(!perform(&mut empty, &b(), SEVER_ACTION, &[]));
        assert_eq!(empty, before);
    }

    // -- Resonance Generator: ACTION ----------------------------------------------------------------------

    const RESONANCE_READIES: &str = "breakthrough:crimson:crimsonbt_ready:STATUS_PHASE_ENDED:after";

    fn with_generator(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough = Some(BreakthroughId::new(BREAKTHROUGH));
    }

    #[test]
    fn the_resonance_generator_flips_a_breach_exhausts_and_readies_in_the_status_phase() {
        let mut state = game();
        with_generator(&mut state);
        let offered = component_actions(&state, content(), &a());
        assert!(
            offered.iter().any(|o| o.id == RESONANCE_ACTION),
            "{offered:?}"
        );
        assert!(perform(&mut state, &a(), RESONANCE_ACTION, &["flip|94"]));
        assert!(
            is_active(&state, &SystemId::new("94")),
            "the Sorrow's breach flipped active"
        );
        assert!(
            component_actions(&state, content(), &a())
                .iter()
                .all(|o| o.id != RESONANCE_ACTION),
            "exhausted"
        );
        let before = state.clone();
        assert!(!perform(&mut state, &a(), RESONANCE_ACTION, &["flip|94"]));
        assert_eq!(state, before, "an exhausted card does nothing");
        emit(
            &mut state,
            None,
            &[RESONANCE_READIES],
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert!(
            component_actions(&state, content(), &a())
                .iter()
                .any(|o| o.id == RESONANCE_ACTION)
        );
        // Flipping it back is just as legal ("flip any breach").
        assert!(perform(&mut state, &a(), RESONANCE_ACTION, &["flip|94"]));
        assert!(!is_active(&state, &SystemId::new("94")));
    }

    #[test]
    fn the_resonance_generator_places_an_active_breach_in_a_non_home_system_with_the_owners_units()
    {
        let mut state = game();
        with_generator(&mut state);
        let spot = SystemId::new(plain(1).pop().unwrap());
        crate::fixtures::put(&mut state, &spot, "cruiser", &a(), 1);
        // The owner's own home, and a rival's home with the owner's ship in it, are not offered.
        let rival_home = home(&state, &b());
        crate::fixtures::put(&mut state, &rival_home, "cruiser", &a(), 1);
        let options = resonance_options(&state, content(), DEFAULT, &a());
        let ids: Vec<&str> = options.iter().map(|o| o.id.as_str()).collect();
        assert!(ids.contains(&format!("place|{spot}").as_str()), "{ids:?}");
        assert!(
            !ids.contains(&format!("place|{rival_home}").as_str()),
            "{ids:?}"
        );
        assert!(!ids.contains(&"place|118"), "{ids:?}");
        assert!(perform(
            &mut state,
            &a(),
            RESONANCE_ACTION,
            &[format!("place|{spot}").as_str()]
        ));
        assert!(is_active(&state, &spot) && has_breach(&state, &spot));
        // A system with none of the owner's units is not offered.
        let mut bare = game();
        with_generator(&mut bare);
        let empty = SystemId::new(plain(2).pop().unwrap());
        assert!(
            resonance_options(&bare, content(), DEFAULT, &a())
                .iter()
                .all(|o| o.id != format!("place|{empty}"))
        );
        // Not held, not offered.
        let mut unheld = game();
        unheld.player_mut(&a()).unwrap().breakthrough = None; // Crimson starts with it
        crate::fixtures::put(&mut unheld, &spot, "cruiser", &a(), 1);
        assert!(component_actions(&unheld, content(), &a()).is_empty());
        let _ = flip_breach;
    }

    // -- Ahk Siever -------------------------------------------------------------------------------------------

    const SIEVER: &str = "leader:crimson:crimsoncommander:SPACE_COMBAT_ENDED:after";

    #[test]
    fn ahk_siever_triggers_off_the_combat_that_unlocked_it_and_pays_a_commodity() {
        let (galaxy, ids) = {
            let others = plain(7);
            let ring: Vec<&str> = others[..6].iter().map(String::as_str).collect();
            (ring_map(&others[6], &ring), others)
        };
        let (fought, near) = (
            SystemId::new(ids[0].as_str()),
            SystemId::new(ids[1].as_str()),
        );
        let mut state = game();
        crate::fixtures::put(&mut state, &near, "crimson_destroyer", &a(), 1);
        crate::fixtures::put(&mut state, &fought, "cruiser", &b(), 1);
        state.player_mut(&a()).unwrap().commodities = 0;
        // Exile places a breach where Sol's ship stands, which unlocks the commander; the commander
        // then resolves at the end of that same combat (the printed note).
        emit(
            &mut state,
            Some(&galaxy),
            &[super::super::crimson::testkit::EXILE, SIEVER],
            "SPACE_COMBAT_ENDED",
            &[("system", fought.to_string().into())],
        );
        assert!(has_breach(&state, &fought));
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(super::super::crimson::COMMANDER)),
            Some(&LeaderStatus::Unlocked)
        );
        assert_eq!(
            state.player(&a()).unwrap().commodities,
            1,
            "paid in the same combat"
        );
        // Both possible: the holder chooses; converting pays a trade good.
        let goods = state.player(&a()).unwrap().trade_goods;
        emit(&mut state, None, &["convert"], "GROUND_COMBAT_ENDED", &[]);
        assert_eq!(state.player(&a()).unwrap().commodities, 0);
        assert_eq!(state.player(&a()).unwrap().trade_goods, goods + 1);
    }

    #[test]
    fn ahk_siever_reaches_an_alliance_holder_through_the_shared_commander_predicate() {
        let mut state = game();
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content(),
            &c(),
            super::super::crimson::COMMANDER
        ));
        state.player_mut(&c()).unwrap().commodities = 0;
        emit(&mut state, None, &[], "SPACE_COMBAT_ENDED", &[]);
        assert_eq!(state.player(&c()).unwrap().commodities, 1);
        assert_eq!(
            state.player(&a()).unwrap().commodities,
            0,
            "the locked owner gets nothing"
        );
    }

    #[test]
    fn ahk_siever_pays_an_actual_alliance_holder_only_while_faceup_and_unlocked() {
        let mut state = game();
        let note = crate::promissory::note_id("an", "crimson");
        crate::promissory::take(&mut state, content(), &c(), &note);
        assert!(state.promissory_faceup.contains(&note));
        state.player_mut(&c()).unwrap().commodities = 0;

        emit(&mut state, None, &[], "SPACE_COMBAT_ENDED", &[]);
        assert_eq!(state.player(&c()).unwrap().commodities, 0, "locked owner");

        state.player_mut(&a()).unwrap().leaders.insert(
            LeaderId::new(super::super::crimson::COMMANDER),
            LeaderStatus::Unlocked,
        );
        emit(&mut state, None, &[], "SPACE_COMBAT_ENDED", &[]);
        assert_eq!(state.player(&c()).unwrap().commodities, 1,
            "the faceup recipient gains the commander's commodity");

        crate::promissory::give_back(&mut state, &note);
        emit(&mut state, None, &[], "SPACE_COMBAT_ENDED", &[]);
        assert_eq!(state.player(&c()).unwrap().commodities, 1,
            "returning the note removes the recipient's ability");
    }

    // -- Ahk Ravin --------------------------------------------------------------------------------------------

    fn agent() -> LeaderId {
        LeaderId::new(AGENT)
    }

    fn use_agent(state: &mut GameState, answers: &[&str]) -> bool {
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(answers), |ctx| {
            crate::leaders::use_leader(ctx, &a(), &agent())
        })
    }

    /// `b` (Sol) has a cruiser in a supernova and a dreadnought carrying infantry elsewhere.
    fn swap_setup() -> (GameState, SystemId, SystemId) {
        let mut state = game();
        let supernova = SystemId::new(crate::fixtures::a_system_where("supernova"));
        let elsewhere = SystemId::new(plain(1).pop().unwrap());
        state.board.entry(supernova.clone()).or_default();
        crate::fixtures::put(&mut state, &supernova, "cruiser", &b(), 1);
        crate::fixtures::put(&mut state, &elsewhere, "dreadnought", &b(), 1);
        crate::fixtures::put(&mut state, &elsewhere, "infantry", &b(), 1);
        // A rival's blockade where the dreadnought goes changes nothing: a swap is not a move.
        crate::fixtures::put(&mut state, &supernova, "destroyer", &c(), 1);
        (state, supernova, elsewhere)
    }

    #[test]
    fn ahk_ravin_swaps_two_ships_and_what_they_carry_ignoring_anomalies_and_blockades() {
        let (mut state, supernova, elsewhere) = swap_setup();
        assert!(
            crate::leaders::component_actions(&state, content(), &a())
                .iter()
                .any(|o| o.id == "component|leader|crimsonagent"),
            "offered once a player has ships in two systems"
        );
        let first = format!("{supernova}|cruiser|whole");
        let second = format!("{elsewhere}|dreadnought|whole");
        assert!(use_agent(
            &mut state,
            &["b", first.as_str(), second.as_str(), "load|0"]
        ));
        let count = |state: &GameState, system: &SystemId, kind: &str| {
            state
                .system_state(system)
                .units
                .iter()
                .filter(|u| u.owner == b() && u.type_id.as_str() == kind)
                .count()
        };
        assert_eq!(
            count(&state, &elsewhere, "cruiser"),
            1,
            "the cruiser went where the dreadnought was"
        );
        assert_eq!(
            count(&state, &supernova, "dreadnought"),
            1,
            "into a supernova, which no move may enter"
        );
        assert_eq!(count(&state, &supernova, "cruiser"), 0);
        assert_eq!(count(&state, &elsewhere, "dreadnought"), 0);
        assert_eq!(
            count(&state, &supernova, "infantry"),
            1,
            "the dreadnought transported its infantry"
        );
        assert_eq!(count(&state, &elsewhere, "infantry"), 0);
        assert_eq!(
            crate::leaders::status(&state, &a(), &agent()),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn ahk_ravin_lets_the_chosen_player_decline_and_needs_a_player_with_ships_in_two_systems() {
        let (mut state, supernova, elsewhere) = swap_setup();
        let before_board = state.board.clone();
        assert!(
            use_agent(&mut state, &["b", "decline"]),
            "the card was used"
        );
        assert_eq!(state.board, before_board, "but the player kept their ships");
        assert_eq!(
            crate::leaders::status(&state, &a(), &agent()),
            Some(LeaderStatus::Exhausted)
        );
        let _ = (supernova, elsewhere);
        // Nobody has ships in two systems at the start: not offered, and nothing happens.
        let mut fresh = game();
        assert!(
            crate::leaders::component_actions(&fresh, content(), &a())
                .iter()
                .all(|o| o.id != "component|leader|crimsonagent")
        );
        let before = fresh.clone();
        assert!(!use_agent(&mut fresh, &[]));
        assert_eq!(fresh, before);
    }

    #[test]
    fn ahk_ravin_asks_the_user_which_player_when_several_could_swap() {
        let (mut state, supernova, elsewhere) = swap_setup();
        let other = SystemId::new(plain(3).pop().unwrap());
        crate::fixtures::put(&mut state, &other, "cruiser", &c(), 1);
        crate::fixtures::put(&mut state, &elsewhere, "cruiser", &c(), 1);
        let first = format!("{other}|cruiser|whole");
        let second = format!("{elsewhere}|cruiser|whole");
        // The Rebellion's owner picks Hacan, who then swaps; Sol keeps everything.
        let sol_before = state.system_state(&supernova).units_of(&b()).len();
        assert!(use_agent(
            &mut state,
            &["c", first.as_str(), second.as_str()]
        ));
        assert_eq!(
            state.system_state(&supernova).units_of(&b()).len(),
            sol_before
        );
    }

    #[test]
    fn ssruu_copies_ahk_ravins_action() {
        let mut state = crate::fixtures::seated_game(
            &[("a", "yssaril"), ("b", "crimson"), ("c", "sol")],
            DEFAULT,
        );
        let (x, y) = (
            SystemId::new(plain(2)[0].as_str()),
            SystemId::new(plain(2)[1].as_str()),
        );
        crate::fixtures::put(&mut state, &x, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &y, "dreadnought", &a(), 1);
        let ssruu = LeaderId::new("yssarilagent");
        assert_eq!(
            crate::leaders::status(&state, &a(), &ssruu),
            Some(LeaderStatus::Readied)
        );
        let first = format!("{x}|cruiser|whole");
        let second = format!("{y}|dreadnought|whole");
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&[first.as_str(), second.as_str()]),
            |ctx| crate::leaders::use_leader_text(ctx, &a(), &agent()),
        );
        assert!(used, "Ssruu has the text ability of the Rebellion's agent");
        assert_eq!(
            state
                .system_state(&y)
                .units_of(&a())
                .iter()
                .map(|u| u.type_id.as_str())
                .collect::<Vec<_>>(),
            vec!["cruiser"]
        );
        assert_eq!(
            crate::leaders::status(&state, &a(), &ssruu),
            Some(LeaderStatus::Exhausted),
            "Ssruu exhausts, not the Rebellion's agent"
        );
        assert_eq!(
            crate::leaders::status(&state, &b(), &agent()),
            Some(LeaderStatus::Readied)
        );
    }

    // -- Homesick Phantom ---------------------------------------------------------------------------------

    const LAUNCH: &str = "leader:crimson:crimsonhero:SPACE_COMBAT_STARTED:after";

    /// Picks one build, puts it on the card when it can, then stops.
    struct OneBuild {
        unit_prefix: &'static str,
        built: bool,
    }
    impl Decider for OneBuild {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            if !self.built
                && let Some(option) = choice
                    .options
                    .iter()
                    .find(|option| option.id.starts_with(self.unit_prefix))
            {
                self.built = true;
                return Ok(option.clone());
            }
            for want in ["place|crimsoncard@space", "done_producing"] {
                if let Some(option) = choice.option(want) {
                    return Ok(option.clone());
                }
            }
            Ok(choice.options.first().cloned().expect("an option"))
        }
    }

    fn produce_one(state: &mut GameState, prefix: &'static str) {
        let h = home(state, &a());
        let mut table = Table::with_default(Box::new(OneBuild {
            unit_prefix: prefix,
            built: false,
        }));
        crate::production::resolve(state, content(), DEFAULT, None, &mut table, &a(), &h)
            .expect("production resolves");
    }

    fn unlock_hero(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(HERO), LeaderStatus::Unlocked);
        state.player_mut(&a()).unwrap().trade_goods = 12;
    }

    #[test]
    fn the_hero_takes_produced_ships_onto_its_card_and_they_stay_out_of_reinforcements() {
        let mut state = game();
        unlock_hero(&mut state);
        let ships = at_home(&state, &a(), "crimson_destroyer");
        let remaining = |state: &GameState| {
            crate::supply::remaining(
                state,
                content(),
                DEFAULT,
                &a(),
                &UnitTypeId::new("crimson_destroyer"),
            )
        };
        let left = remaining(&state);
        produce_one(&mut state, "build|crimson_destroyer");
        assert_eq!(
            card_ships(&state, &a()),
            vec!["crimson_destroyer".to_owned()]
        );
        assert_eq!(
            at_home(&state, &a(), "crimson_destroyer"),
            ships,
            "it is not on the board"
        );
        assert_eq!(
            remaining(&state),
            left - 1,
            "but it is not in reinforcements either"
        );
        // Locked: the same production puts the ship on the board.
        let mut locked = game();
        locked.player_mut(&a()).unwrap().trade_goods = 12;
        produce_one(&mut locked, "build|crimson_destroyer");
        assert!(card_ships(&locked, &a()).is_empty());
        assert_eq!(at_home(&locked, &a(), "crimson_destroyer"), ships + 1);
    }

    #[test]
    fn the_hero_places_every_ship_on_its_card_into_the_active_system_and_is_purged() {
        let mut state = game();
        unlock_hero(&mut state);
        produce_one(&mut state, "build|crimson_destroyer");
        put_on_card(&mut state, &a(), "cruiser");
        let system = SystemId::new(plain(1).pop().unwrap());
        let combat = |attacker: &str, defender: &str| {
            vec![
                ("system", system.to_string().into()),
                ("attacker", attacker.into()),
                ("defender", defender.into()),
            ]
        };
        // A combat the owner is not in: not offered (Dane).
        let before = state.clone();
        emit(
            &mut state,
            None,
            &[LAUNCH],
            "SPACE_COMBAT_STARTED",
            &combat("b", "c"),
        );
        assert_eq!(state, before);
        // The "may".
        emit(
            &mut state,
            None,
            &["decline"],
            "SPACE_COMBAT_STARTED",
            &combat("a", "b"),
        );
        assert_eq!(state, before);
        emit(
            &mut state,
            None,
            &[LAUNCH],
            "SPACE_COMBAT_STARTED",
            &combat("a", "b"),
        );
        let board = state.system_state(&system);
        assert_eq!(board.units_of(&a()).len(), 2, "both ships joined the fight");
        assert!(card_ships(&state, &a()).is_empty());
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new(HERO)),
            Some(&LeaderStatus::Purged)
        );
    }

    // -- a game without the Crimson ------------------------------------------------------------------------

    #[test]
    fn a_game_without_the_crimson_offers_none_of_these_cards() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let before = state.clone();
        assert!(component_actions(&state, content(), &a()).is_empty());
        assert_eq!(
            leader_action(&state, content(), &a(), &agent()),
            Some(false)
        );
        assert!(card_ships(&state, &a()).is_empty());
        assert_eq!(severed_systems(&state), Vec::<String>::new());
        assert_eq!(
            held_on_card(&state, content(), DEFAULT, &a(), "destroyer"),
            0
        );
        ship_destroyed(&mut state, "a", "destroyer", &[]);
        assert_eq!(state, before);
    }
}
