//! The Ghosts of Creuss (`ghost`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope.
//!
//! Implemented: Creuss Gate (the seating placement, verified), Quantum Entanglement, Slipstream,
//! Dimensional Splicer, Wormhole Generator, Hil Colish (delta wormhole), Icarus Drive, Creuss IFF,
//! Emissary Taivra, Riftwalker Meian, Sai Seravus, and Particle Synthesis.

use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{DEFAULT, SourceSet};
use ti4_model::id::{LeaderId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus, Phase};

use super::hooks_combat::{CombatHooks, CombatMoment, HitSite, ProducedHits};
use super::hooks_economy::EconomyHooks;
use super::hooks_movement::{MoveSite, MovementHooks, has_alpha_or_beta, wormholes_at};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::movement::MapEdit;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// What this faction implements; grows package by package.
pub const MODULE: FactionModule = FactionModule {
    alias: "ghost",
    abilities: &["creuss_gate", "quantum_entanglement", "slipstream"],
    technologies: &["ds", "wg"],
    units: &["ghost_flagship", "ghost_mech"],
    promissory: &["iff"],
    leaders: &["ghostagent", "ghostcommander", "ghosthero"],
    breakthroughs: &["ghostbt"],
    hooks: Hooks {
        component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        timing_abilities: Some(timing_abilities),
        combat: CombatHooks {
            produced_hits: Some(produced_hits),
            ..CombatHooks::NONE
        },
        movement: MovementHooks {
            extra_wormholes: Some(extra_wormholes),
            linked_systems: Some(linked_systems),
            move_bonus: Some(move_bonus),
            ..MovementHooks::NONE
        },
        economy: EconomyHooks {
            extra_production: Some(extra_production),
            production_cost_reduction: Some(production_cost_reduction),
            ..EconomyHooks::NONE
        },
        ..Hooks::NONE
    },
};

const WORMHOLE_GENERATOR: &str = "faction|ghost|wg";
const AGENT_LINK_KEY: &str = "ghost:agent_link";

// -- small helpers -------------------------------------------------------------------------------

fn is_ghost(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == "ghost")
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
}

fn technology_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::technology_text_ready(state, player, alias)
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    state
        .player(player)
        .and_then(|seat| seat.leaders.get(&LeaderId::new(leader)).copied())
}

fn agent_ready(state: &GameState, owner: &PlayerId) -> bool {
    leader_status(state, owner, "ghostagent") == Some(LeaderStatus::Readied)
}

fn decision(state: &GameState, player: &PlayerId, card: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn ask(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: &str,
    card: &str,
    subtype: &str,
    mut options: Vec<ChoiceOption>,
    declinable: bool,
) -> Result<ChoiceOption, IllegalChoice> {
    if declinable {
        options.push(ChoiceOption::decline());
    }
    let choice = Choice::new(player.clone(), prompt.to_owned(), options).contextualized(decision(
        context.state,
        player,
        card,
        subtype,
    ));
    context.ask_seeing(&choice)
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

fn has_units(state: &GameState, system: &SystemId, player: &PlayerId) -> bool {
    state.board.get(system).is_some_and(|board| {
        board.units.iter().any(|unit| &unit.owner == player)
            || board
                .planet_units
                .values()
                .flatten()
                .any(|unit| &unit.owner == player)
    })
}

fn has_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    player: &PlayerId,
) -> bool {
    let types = ti4_content::units::catalogue(content, sources);
    state.system_state(system).units.iter().any(|unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::UnitType::is_ship)
    })
}

/// Counts physical wormholes, rather than their distinct kinds. In particular, a printed alpha
/// plus an alpha token are two wormholes even though [`wormholes_at`] reports one alpha kind.
fn wormhole_count(state: &GameState, system: &SystemId) -> i64 {
    let printed = ti4_content::galaxy::system(ContentStore::embedded(), system.as_str(), DEFAULT)
        .map_or(0, |tile| {
            i64::try_from(tile.wormholes().len()).unwrap_or(i64::MAX)
        });
    let tokens = i64::try_from(
        state
            .wormhole_tokens
            .values()
            .filter(|at| *at == system)
            .count(),
    )
    .unwrap_or(i64::MAX);
    let storm = i64::from(state.ion_storm.as_ref().is_some_and(|(at, _)| at == system));
    let nexus =
        i64::from(state.nexus_unlocked && system.as_str() == crate::seating::LOCKED_NEXUS) * 2;
    let hil_colish = i64::try_from(
        extra_wormholes(state)
            .into_iter()
            .filter(|(at, _)| at == system.as_str())
            .count(),
    )
    .unwrap_or(i64::MAX);
    printed + tokens + storm + nexus + hil_colish
}

/// Particle Synthesis: "Each wormhole in a system that contains your ships gains PRODUCTION 1
/// as if it were a unit you control. Reduce the combined cost of units you produce in systems
/// that contain wormholes by 1 for each wormhole in that system."
fn extra_production(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    if crate::breakthroughs::holds(state, player, "ghostbt")
        && has_ships(state, content, sources, system, player)
    {
        wormhole_count(state, system)
    } else {
        0
    }
}

fn production_cost_reduction(state: &GameState, player: &PlayerId, system: &SystemId) -> i64 {
    if crate::breakthroughs::holds(state, player, "ghostbt") {
        wormhole_count(state, system)
    } else {
        0
    }
}

// -- Quantum Entanglement and Emissary Taivra's link ---------------------------------------------

/// The system Emissary Taivra made adjacent to every wormhole system, while its tactical action is
/// under way.
fn agent_link(state: &GameState) -> Option<SystemId> {
    let mark = state.faction_marks.get(AGENT_LINK_KEY)?;
    let (seq, system) = mark.split_once('|')?;
    let system = SystemId::new(system);
    (seq.parse::<u32>().ok() == Some(state.activation_seq)
        && state.active_system.as_ref() == Some(&system))
    .then_some(system)
}

/// Quantum Entanglement: "You treat all systems that contain either an alpha or beta wormhole as
/// adjacent to each other. Game effects cannot prevent you from using this ability." Built from
/// the galaxy's wormhole kinds, which the wormhole-off switches do not touch, so Enforced Travel
/// Ban does not silence it.
///
/// Emissary Taivra: "that system is adjacent to all other systems that contain a wormhole during
/// this tactical action" -- for every player, while the activation it was used in is under way.
fn linked_systems(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if is_ghost(state, player) {
        // "Game effects cannot prevent you": read the kinds with Nexus Sovereignty's
        // suppression lifted.
        let unsuppressed;
        let galaxy = if galaxy.nexus_wormholes_off {
            let mut copy = galaxy.clone();
            copy.nexus_wormholes_off = false;
            unsuppressed = copy;
            &unsuppressed
        } else {
            galaxy
        };
        let holders: Vec<&str> = galaxy
            .wormhole_systems()
            .into_iter()
            .filter(|system| {
                galaxy
                    .wormhole_kinds(system)
                    .iter()
                    .any(|kind| *kind == "ALPHA" || *kind == "BETA")
            })
            .collect();
        for (index, a) in holders.iter().enumerate() {
            for b in &holders[index + 1..] {
                pairs.push(((*a).to_owned(), (*b).to_owned()));
            }
        }
    }
    if let Some(system) = agent_link(state) {
        for other in galaxy.wormhole_systems() {
            if other != system.as_str() {
                pairs.push((system.to_string(), other.to_owned()));
            }
        }
    }
    pairs
}

// -- Hil Colish ----------------------------------------------------------------------------------

/// Hil Colish: "This ship's system contains a delta wormhole." Derived from the board on every
/// call, so it follows the ship. The second printed line, "During movement, this ship may move
/// before or after your other ships", needs nothing: ships move one at a time in any order and
/// each offer reads the board as it stands.
fn extra_wormholes(state: &GameState) -> Vec<(String, String)> {
    state
        .board
        .iter()
        .filter(|(_, board)| {
            board.units.iter().any(|unit| {
                super::flagship_has_text(
                    state,
                    &unit.owner,
                    unit.type_id.as_str(),
                    "ghost_flagship",
                )
            })
        })
        .map(|(system, _)| (system.to_string(), "DELTA".to_owned()))
        .collect()
}

// -- Slipstream ----------------------------------------------------------------------------------

/// Slipstream: "Apply +1 to the move value of each of your ships that starts its movement in your
/// home system or in a system that contains either an alpha or beta wormhole."
fn move_bonus(state: &GameState, site: &MoveSite<'_>) -> i32 {
    let Some(seat) = state.player(site.player) else {
        return 0;
    };
    i32::from(
        seat.faction.as_str() == "ghost"
            && (seat.home_system.as_ref() == Some(site.origin)
                || has_alpha_or_beta(state, site.origin)),
    )
}

// -- Dimensional Splicer -------------------------------------------------------------------------

/// Dimensional Splicer: "At the start of space combat in a system that contains a wormhole and 1
/// or more of your ships, you may produce 1 hit and assign it to 1 of your opponent's ships."
/// The producer picks the ship (`producer_assigned`); the owner may still sustain.
fn produced_hits(
    context: &mut TimingContext<'_>,
    site: &HitSite<'_>,
) -> Result<ProducedHits, IllegalChoice> {
    let player = site.player;
    if site.moment != CombatMoment::CombatStart
        || !has_technology(context.state, player, "ds")
        || wormholes_at(context.state, site.system).is_empty()
    {
        return Ok(ProducedHits::NONE);
    }
    let ships = |context: &TimingContext<'_>, owner: &PlayerId| {
        crate::combat::ships_of(
            context.state,
            context.content,
            context.sources,
            owner,
            site.system,
        )
    };
    if ships(context, player).is_empty() || ships(context, site.opponent).is_empty() {
        return Ok(ProducedHits::NONE);
    }
    let answer = ask(
        context,
        player,
        "Dimensional Splicer: produce 1 hit against your opponent's ships",
        "ds",
        "dimensional_splicer",
        vec![ChoiceOption::labelled(
            "produce_hit",
            "produce_hit",
            "produce 1 hit and assign it to 1 of your opponent's ships",
        )],
        true,
    )?;
    Ok(if answer.is_decline() {
        ProducedHits::NONE
    } else {
        ProducedHits {
            producer_assigned: 1,
            ..ProducedHits::NONE
        }
    })
}

// -- Creuss wormhole tokens: Wormhole Generator, Creuss IFF, Icarus Drive ------------------------

fn placement_option(kind: &str, system: &SystemId, label: &str) -> ChoiceOption {
    ChoiceOption::labelled(
        format!("{kind}|{system}"),
        "creuss_token",
        format!("{label}: the {kind} token into {system}"),
    )
    .with("system", system.to_string())
}

fn parse_placement(id: &str) -> Option<(&'static str, SystemId)> {
    let (kind, system) = id.split_once('|')?;
    let kind = crate::tokens::CREUSS_TOKENS
        .into_iter()
        .find(|known| *known == kind)?;
    Some((kind, SystemId::new(system)))
}

/// Ask for a destination, place the token, and report whether it was placed (atomic: nothing
/// changes unless it was).
fn choose_and_place(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    card: &str,
    label: &str,
    destinations: &[(&'static str, SystemId)],
    declinable: bool,
) -> Result<bool, IllegalChoice> {
    let Some(galaxy) = context.galaxy else {
        return Ok(false);
    };
    if destinations.is_empty() {
        return Ok(false);
    }
    let options = destinations
        .iter()
        .map(|(kind, system)| placement_option(kind, system, label))
        .collect();
    let answer = ask(
        context,
        player,
        &format!("{label}: place or move a Creuss wormhole token"),
        card,
        "creuss_token",
        options,
        declinable,
    )?;
    if answer.is_decline() {
        return Ok(false);
    }
    let Some((kind, system)) = parse_placement(&answer.id) else {
        return Ok(false);
    };
    Ok(crate::tokens::place_creuss_token(context.state, galaxy, kind, &system).is_ok())
}

/// Wormhole Generator: "ACTION: Exhaust this card to place or move a Creuss wormhole token into
/// either a system that contains a planet you control or a non-home system that does not contain
/// another player's ships."
fn component_actions(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    // Offered whenever the card is ready: there is no map here, and a legal system (a non-home
    // system free of other players' ships, or a controlled planet's) exists on every real board.
    // `perform_component` re-checks against the map and changes nothing if there is none.
    if technology_ready(state, player, "wg") {
        vec![ChoiceOption::labelled(
            WORMHOLE_GENERATOR,
            crate::faction_abilities::ACTION_KIND,
            "Wormhole Generator: exhaust to place or move a Creuss wormhole token",
        )]
    } else {
        Vec::new()
    }
}

fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    if option.id != WORMHOLE_GENERATOR || !technology_ready(context.state, player, "wg") {
        return false;
    }
    let Some(galaxy) = context.galaxy else {
        return false;
    };
    let destinations = crate::tokens::wormhole_generator_destinations(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
    );
    // Asked before the card is exhausted; a refusal changes nothing.
    if !choose_and_place(
        context,
        player,
        "wg",
        "Wormhole Generator",
        &destinations,
        true,
    )
    .unwrap_or(false)
    {
        return false;
    }
    crate::technology::exhaust_technology_text(context.state, player, "wg");
    true
}

/// Creuss IFF: "At the start of your turn during the action phase: Place or move a Creuss
/// wormhole token into either a system that contains a planet you control or a non-home system
/// that does not contain another player's ships. Then, return this card to the Creuss player."
fn iff(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let note = crate::promissory::note_id("iff", "ghost");
    let held = move |state: &GameState, holder: &PlayerId| {
        state.phase == Phase::Action
            && state.promissory_notes.get(&note) == Some(holder)
            && !is_ghost(state, holder)
    };
    let held_in_effect = held.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:iff:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event.text("player") != Some(owner.as_str())
                || !held_in_effect(context.state, &owner)
            {
                return Ok(());
            }
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            let destinations = crate::tokens::wormhole_generator_destinations(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &owner,
            );
            // The ability's own decline already asked "use it?"; the destination is the question.
            if choose_and_place(context, &owner, "iff", "Creuss IFF", &destinations, false)
                .map_err(illegal)?
            {
                crate::promissory::give_back(
                    context.state,
                    &crate::promissory::note_id("iff", "ghost"),
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_seat.as_str())
            && held(context.state, &condition_seat)
            && context.galaxy.is_some_and(|galaxy| {
                !crate::tokens::wormhole_generator_destinations(
                    context.state,
                    context.content,
                    context.sources,
                    galaxy,
                    &condition_seat,
                )
                .is_empty()
            })
    }))
}

/// Systems where `owner` has an Icarus Drive mech, paired with each token not yet there. Only
/// systems on the hex grid (a token cannot be placed off the map).
fn mech_destinations(
    state: &GameState,
    galaxy: &Galaxy,
    owner: &PlayerId,
) -> Vec<(&'static str, SystemId)> {
    let mut found = Vec::new();
    for (system, board) in &state.board {
        let present = board
            .units
            .iter()
            .chain(board.planet_units.values().flatten())
            .any(|unit| &unit.owner == owner && unit.type_id.as_str() == "ghost_mech");
        if !present || galaxy.coord_of(system.as_str()).is_none() {
            continue;
        }
        for kind in crate::tokens::CREUSS_TOKENS {
            if state.wormhole_tokens.get(kind) != Some(system) {
                found.push((kind, system.clone()));
            }
        }
    }
    found
}

/// Icarus Drive: "After any player activates a system, you may remove this unit from the game
/// board to place or move a Creuss wormhole token into this system." "This system" is the
/// system the mech is in, not the active one.
fn icarus_drive(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:ghost_mech:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            let destinations = mech_destinations(context.state, galaxy, &owner);
            if destinations.is_empty() {
                return Ok(());
            }
            let options = destinations
                .iter()
                .map(|(kind, system)| placement_option(kind, system, "Icarus Drive"))
                .collect();
            let answer = ask(
                context,
                &owner,
                "Icarus Drive: remove the mech to place or move a Creuss wormhole token into its system",
                "ghost_mech",
                "icarus_drive",
                options,
                false,
            )
            .map_err(illegal)?;
            let Some((kind, system)) = parse_placement(&answer.id) else {
                return Ok(());
            };
            // Place first (it can refuse), then remove the mech; nothing can fail after that.
            if crate::tokens::place_creuss_token(context.state, galaxy, kind, &system).is_err() {
                return Ok(());
            }
            remove_mech(context.state, &owner, &system);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .galaxy
            .is_some_and(|galaxy| !mech_destinations(context.state, galaxy, &condition_seat).is_empty())
    }))
}

/// Take one of `owner`'s mechs off the board in `system` (a planet first, then space).
fn remove_mech(state: &mut GameState, owner: &PlayerId, system: &SystemId) {
    let is_mech = |unit: &ti4_model::units::Unit| {
        &unit.owner == owner && unit.type_id.as_str() == "ghost_mech"
    };
    let board = state.system_mut(system);
    for units in board.planet_units.values_mut() {
        if let Some(index) = units.iter().position(is_mech) {
            units.remove(index);
            return;
        }
    }
    if let Some(index) = board.units.iter().position(is_mech) {
        board.units.remove(index);
    }
}

// -- Emissary Taivra -----------------------------------------------------------------------------

/// Emissary Taivra: "After a player activates a system that contains a non-delta wormhole: You
/// may exhaust this card: if you do, that system is adjacent to all other systems that contain a
/// wormhole during this tactical action." The adjacency itself is [`linked_systems`], keyed on
/// the activation the mark names.
fn emissary_taivra(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:ghostagent:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if !agent_usable(context.state, context.galaxy, &owner, &system) {
                return Ok(());
            }
            // Through `leaders::exhaust` so a watching Nomad's Temporal Command Suite hears it.
            crate::leaders::exhaust(context.state, &owner, &LeaderId::new("ghostagent"));
            let mark = format!("{}|{system}", context.state.activation_seq);
            context
                .state
                .faction_marks
                .insert(AGENT_LINK_KEY.to_owned(), mark);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event
            .text("system")
            .map(SystemId::new)
            .is_some_and(|system| {
                agent_usable(context.state, context.galaxy, &condition_seat, &system)
            })
    }))
}

/// Ssruu copies Emissary Taivra's activation link. The source and borrower have separate timing
/// abilities so the optional copied effect is controlled by the Ssruu holder, not the Ghost.
fn borrowed_emissary_taivra(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:ghostagent:SYSTEM_ACTIVATED:after"),
        borrower.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if !has_borrowable_ghost_agent(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) || !agent_target_usable(context.state, context.galaxy, &system)
            {
                return Ok(());
            }
            if crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                let mark = format!("{}|{system}", context.state.activation_seq);
                context
                    .state
                    .faction_marks
                    .insert(AGENT_LINK_KEY.to_owned(), mark);
                super::hooks_cards::borrowed_agent_used(
                    context,
                    &effect_borrower,
                    &LeaderId::new("ghostagent"),
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event
            .text("system")
            .map(SystemId::new)
            .is_some_and(|system| {
                has_borrowable_ghost_agent(
                    context.state,
                    context.content,
                    &condition_source,
                    &condition_borrower,
                ) && agent_target_usable(context.state, context.galaxy, &system)
            })
    }))
}

fn has_borrowable_ghost_agent(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == "ghostagent")
}

fn agent_target_usable(state: &GameState, galaxy: Option<&Galaxy>, system: &SystemId) -> bool {
    wormholes_at(state, system)
        .iter()
        .any(|kind| kind != "DELTA")
        && galaxy.is_some_and(|galaxy| {
            galaxy
                .wormhole_systems()
                .into_iter()
                .any(|other| other != system.as_str())
        })
}

fn agent_usable(
    state: &GameState,
    galaxy: Option<&Galaxy>,
    owner: &PlayerId,
    system: &SystemId,
) -> bool {
    agent_ready(state, owner) && agent_target_usable(state, galaxy, system)
}

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        icarus_drive(owner_name, seat),
        iff(owner_name, seat),
        emissary_taivra(owner_name, seat),
        commander_moved(owner_name, seat),
        commander_finished(owner_name, seat),
    ];
    // The static card roster determines which copied listener exists. Its condition reads live
    // readiness through `borrowable_agents`, so a resolver survives card exhaustion/readying.
    if state
        .player(seat)
        .is_some_and(|source| source.leaders.contains_key(&LeaderId::new("ghostagent")))
    {
        for candidate in &state.players {
            if &candidate.id != seat
                && candidate
                    .leaders
                    .contains_key(&LeaderId::new("yssarilagent"))
            {
                abilities.push(borrowed_emissary_taivra(owner_name, seat, &candidate.id));
            }
        }
    }
    abilities
}

// -- Sai Seravus -------------------------------------------------------------------------------

const COMMANDER_MARK: &str = "private:#ghost:commander:";

fn commander_mark(state: &GameState, owner: &PlayerId) -> String {
    format!("{COMMANDER_MARK}{owner}:{}", state.activation_seq)
}

/// Sai Seravus: "After your ships move: For each ship that has a capacity value and moved
/// through 1 or more wormholes, you may place 1 fighter from your reinforcements with that ship
/// if you have unused capacity in the active system."
///
/// `SHIP_MOVED` names one ship and its actual route. Record eligible ships until the entire
/// movement step is over; placing fighters on each ship's arrival would make those fighters
/// available to load onto later ships in the same move.
fn commander_moved(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let eligible = |event: &crate::event::Event, context: &TimingContext<'_>, player: &PlayerId| {
        event.text("player") == Some(player.as_str())
            && event.text("system") == context.state.active_system.as_ref().map(SystemId::as_str)
            && crate::promissory::has_commander_ability(context.state, player, "ghostcommander")
            && event.integer("wormholes").is_some_and(|hops| hops > 0)
            && event
                .text("unit")
                .and_then(|id| ti4_content::units::unit_type(context.content, id, context.sources))
                .is_some_and(|ship| ship.is_ship() && ship.capacity() > 0)
    };
    Ability::stateful(
        format!("leader:{owner_name}:ghostcommander:SHIP_MOVED:after"),
        seat.clone(),
        "SHIP_MOVED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if eligible(event, context, &owner) {
                let key = commander_mark(context.state, &owner);
                let count = context
                    .state
                    .faction_marks
                    .get(&key)
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(0);
                context
                    .state
                    .faction_marks
                    .insert(key, count.saturating_add(1).to_string());
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        eligible(event, context, &condition_seat)
    }))
}

fn commander_count(state: &GameState, owner: &PlayerId) -> usize {
    state
        .faction_marks
        .get(&commander_mark(state, owner))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

fn commander_fighters(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    system: &SystemId,
) -> usize {
    if !crate::promissory::has_commander_ability(state, owner, "ghostcommander") {
        return 0;
    }
    let free = crate::fleet::standing(state, content, sources, owner, system, None).capacity_free();
    let plastic = crate::supply::remaining(
        state,
        content,
        sources,
        owner,
        &ti4_model::id::UnitTypeId::new("fighter"),
    );
    commander_count(state, owner)
        .min(usize::try_from(free).unwrap_or(0))
        .min(usize::try_from(plastic).unwrap_or(0))
}

fn commander_finished(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:ghostcommander:MOVEMENT_FINISHED:after"),
        seat.clone(),
        "MOVEMENT_FINISHED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let key = commander_mark(context.state, &owner);
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let available = commander_fighters(
                context.state,
                context.content,
                context.sources,
                &owner,
                &system,
            );
            if available == 0 {
                context.state.faction_marks.remove(&key);
                return Ok(());
            }
            let options = (1..=available)
                .map(|count| {
                    ChoiceOption::labelled(
                        format!("fighters|{count}"),
                        "fighter",
                        format!("place {count} fighter(s) in {system}"),
                    )
                    .with("count", count)
                })
                .collect();
            let answer = ask(
                context,
                &owner,
                "Sai Seravus: how many fighters do you place from reinforcements",
                "ghostcommander",
                "commander_fighters",
                options,
                true,
            )
            .map_err(illegal)?;
            if !answer.is_decline() {
                let count = answer
                    .payload
                    .get("count")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(0);
                crate::action_cards::place_units_counted(
                    context, &owner, &system, None, "fighter", count,
                );
            }
            context.state.faction_marks.remove(&key);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_seat.as_str())
            && crate::promissory::has_commander_ability(
                context.state,
                &condition_seat,
                "ghostcommander",
            )
            && commander_count(context.state, &condition_seat) > 0
    }))
}

/// Sai Seravus, unlock: "Have units in 3 systems that contain alpha or beta wormholes."
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "ghostcommander").then(|| {
        state
            .board
            .keys()
            .filter(|system| has_alpha_or_beta(state, system) && has_units(state, system, player))
            .count()
            >= 3
    })
}

// -- Riftwalker Meian ----------------------------------------------------------------------------

/// "other than the Creuss system, Ahk Creuxx system, or the Wormhole Nexus", and not a Fracture
/// system: tiles 17 and 51, the Nexus (82...), and the Fracture tiles.
fn hero_excluded(system: &str) -> bool {
    system == crate::seating::CREUSS_GATE
        || system == crate::seating::CREUSS_HOME
        || system.starts_with("82")
        || system.starts_with("fracture")
}

fn hero_eligible(state: &GameState, system: &SystemId, player: &PlayerId) -> bool {
    !hero_excluded(system.as_str())
        && (!wormholes_at(state, system).is_empty() || has_units(state, system, player))
}

/// What the offer can know without a map: systems on the board record that hold a wormhole or
/// the owner's units. A subset of what [`hero_swap`] may choose from.
fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "ghosthero").then(|| {
        let mut seen: BTreeSet<&SystemId> = state.board.keys().collect();
        seen.extend(state.wormhole_tokens.values());
        seen.into_iter()
            .filter(|system| hero_eligible(state, system, player))
            .count()
            >= 2
    })
}

fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "ghosthero").then(|| hero_swap(context, player))
}

/// Riftwalker Meian: "ACTION: Swap the positions of any 2 non-Fracture systems that contain
/// wormholes or your units, other than the Creuss system, Ahk Creuxx system, or the Wormhole
/// Nexus." Both systems are chosen before anything changes; the edit is recorded in the state
/// (`movement::apply_map_edit`) and the game replays it onto its own map at the end of the step.
fn hero_swap(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let Some(galaxy) = context.galaxy else {
        return false;
    };
    let candidates: Vec<SystemId> = galaxy
        .system_ids_holding_things()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| hero_eligible(context.state, system, player))
        .collect();
    if candidates.len() < 2 {
        return false;
    }
    let options = |skip: Option<&SystemId>| -> Vec<ChoiceOption> {
        candidates
            .iter()
            .filter(|system| Some(*system) != skip)
            .map(|system| {
                ChoiceOption::labelled(system.to_string(), "system", format!("system {system}"))
            })
            .collect()
    };
    let Ok(first) = ask(
        context,
        player,
        "Riftwalker Meian: swap the positions of which system",
        "ghosthero",
        "swap_first",
        options(None),
        true,
    ) else {
        return false;
    };
    if first.is_decline() {
        return false;
    }
    let first = SystemId::new(first.id);
    let Ok(second) = ask(
        context,
        player,
        "Riftwalker Meian: swap it with which system",
        "ghosthero",
        "swap_second",
        options(Some(&first)),
        true,
    ) else {
        return false;
    };
    if second.is_decline() {
        return false;
    }
    // Applied to a copy: the live map catches up from the recorded edit when the step ends, and
    // a refusal leaves the state as it was.
    let mut trial = galaxy.clone();
    crate::movement::apply_map_edit(
        context.state,
        &mut trial,
        context.content,
        context.sources,
        &MapEdit::Swap {
            a: first.to_string(),
            b: second.id,
        },
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::{Decider, Scripted, Table, Window};
    use crate::fixtures::{armed_resolver, put, put_on_planet, seated_game, with_context};
    use crate::movement::PlayerAdjacency;
    use crate::production::{ProductionWindow, capacity};
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::TechnologyId;
    use ti4_model::id::{BreakthroughId, PlanetId};

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn c() -> PlayerId {
        PlayerId::new("c")
    }
    fn sys(id: &str) -> SystemId {
        SystemId::new(id)
    }
    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }

    /// Fails the test if it is asked anything.
    struct Never;
    impl Decider for Never {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            panic!("unexpected question: {}", choice.prompt);
        }
    }
    fn never() -> Table {
        Table::with_default(Box::new(Never))
    }

    struct BorrowGhostAgent;
    impl Decider for BorrowGhostAgent {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            if let Some(_option) = choice
                .options
                .iter()
                .find(|option| option.id == "leader:ghost:ghostagent:SYSTEM_ACTIVATED:after")
            {
                assert_eq!(
                    choice.player,
                    a(),
                    "native branch is independently declined"
                );
                return choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .cloned()
                    .ok_or_else(|| IllegalChoice::NoOptions {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                    });
            }
            if let Some(option) = choice.options.iter().find(|option| {
                option
                    .id
                    .contains(":yssarilagent:ghostagent:SYSTEM_ACTIVATED:after")
            }) {
                assert_eq!(
                    choice.player,
                    b(),
                    "the copied optional window belongs to Ssruu"
                );
                return Ok(option.clone());
            }
            panic!(
                "unexpected copied-agent decision for {}: {}",
                choice.player, choice.prompt
            );
        }
    }
    fn borrow_ghost_agent() -> Table {
        Table::with_default(Box::new(BorrowGhostAgent))
    }

    /// Alpha: 39 and 26; beta: 40 and 25; delta: the gate; plain: 18, 19, 20, 21.
    fn galaxy() -> Galaxy {
        let mut galaxy = Galaxy::build(
            content(),
            &["18", "17", "39", "40", "26", "25", "19", "20", "21"],
            DEFAULT,
            3,
        )
        .expect("builds");
        crate::seating::place_creuss_home(&mut galaxy, content(), DEFAULT).expect("home");
        galaxy
    }
    fn ghost_game() -> GameState {
        seated_game(&[("a", "ghost"), ("b", "sol")], DEFAULT)
    }
    fn ghost_ssruu_game() -> GameState {
        seated_game(&[("a", "ghost"), ("b", "yssaril"), ("c", "sol")], DEFAULT)
    }
    fn sol_game() -> GameState {
        seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT)
    }
    fn give_technology(state: &mut GameState, player: &PlayerId, alias: &str) {
        state
            .player_mut(player)
            .unwrap()
            .technologies
            .insert(TechnologyId::new(alias));
    }
    fn give_breakthrough(state: &mut GameState, player: &PlayerId, alias: &str) {
        state.player_mut(player).unwrap().breakthrough = Some(BreakthroughId::new(alias));
    }
    fn cruiser_cost(state: &GameState, player: &PlayerId, system: &SystemId) -> i64 {
        ProductionWindow::new(state, content(), DEFAULT, player, system)
            .pending_choice(state, content(), DEFAULT)
            .expect("a live production choice")
            .options
            .iter()
            .find(|option| {
                option
                    .payload
                    .get("unit")
                    .and_then(serde_json::Value::as_str)
                    == Some("cruiser")
            })
            .and_then(|option| option.payload.get("cost"))
            .and_then(serde_json::Value::as_i64)
            .expect("cruiser is offered with a cost")
    }
    fn add_dock(state: &mut GameState, system: &SystemId, player: &PlayerId) {
        let planet = ti4_content::galaxy::system(content(), system.as_str(), DEFAULT)
            .and_then(|tile| tile.planets().into_iter().next())
            .map(PlanetId::new)
            .expect("the selected system has a planet");
        state
            .system_mut(system)
            .set_control(planet.clone(), player.clone());
        put_on_planet(state, system, &planet, "spacedock", player, 1);
    }
    fn emit(
        state: &mut GameState,
        galaxy: Option<&Galaxy>,
        table: &mut Table,
        event_type: &str,
        payload: &[(&str, serde_json::Value)],
    ) {
        let mut resolver = armed_resolver(state);
        with_context(state, DEFAULT, galaxy, table, |ctx| {
            let payload = payload
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = ctx
                .event_sequence
                .next(event_type, payload)
                .expect("event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("window resolves");
        });
    }
    /// A pair of systems holding alpha and beta that are not hex neighbours.
    fn alpha_beta_far_apart(galaxy: &Galaxy) -> (&'static str, &'static str) {
        for (alpha, beta) in [("39", "40"), ("39", "25"), ("26", "40"), ("26", "25")] {
            if !galaxy.adjacent(alpha).contains(beta) {
                return (alpha, beta);
            }
        }
        panic!("every alpha/beta pair is a neighbour pair");
    }

    // -- Creuss Gate -----------------------------------------------------------------------------

    #[test]
    fn the_creuss_gate_holds_the_home_position_and_the_home_system_sits_beside_it() {
        let state = ghost_game();
        let seat = state.player(&a()).unwrap();
        assert_eq!(
            seat.home_system,
            Some(sys("51")),
            "tile 51 is the home system"
        );
        assert!(
            has_units(&state, &sys("51"), &a()),
            "the starting units are in tile 51, not on the gate"
        );
        let galaxy = galaxy();
        assert!(galaxy.coord_of("17").is_some(), "the gate is on the map");
        assert!(
            galaxy.coord_of("51").is_none(),
            "the home system is off the map"
        );
        assert!(galaxy.adjacent("51").contains("17"));
        assert!(galaxy.adjacent("17").contains("51"));
        assert_eq!(galaxy.adjacent("51").len(), 1, "adjacent to the gate only");
        // "The Creuss Gate system is not a home system": a token may go into it.
        let destinations = crate::tokens::wormhole_generator_destinations(
            &state,
            content(),
            DEFAULT,
            &galaxy,
            &a(),
        );
        assert!(
            destinations
                .iter()
                .any(|(_, system)| system.as_str() == "17")
        );
    }

    // -- Quantum Entanglement --------------------------------------------------------------------

    #[test]
    fn quantum_entanglement_links_every_alpha_and_beta_system_for_the_creuss_player_only() {
        let state = ghost_game();
        let galaxy = galaxy();
        let (alpha, beta) = alpha_beta_far_apart(&galaxy);
        let ghost = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &a());
        let other = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &b());
        assert!(
            ghost.are_adjacent(alpha, beta),
            "alpha and beta are adjacent to Creuss"
        );
        assert!(ghost.are_adjacent(beta, alpha));
        assert!(!other.are_adjacent(alpha, beta), "and to nobody else");
        assert_eq!(
            ghost.are_adjacent("39", "17"),
            galaxy.adjacent("39").contains("17"),
            "the delta gate is neither alpha nor beta: only the hex decides"
        );
        assert_eq!(
            ghost.are_adjacent("18", alpha),
            galaxy.adjacent("18").contains(alpha),
            "a plain system gains nothing"
        );
    }

    #[test]
    fn quantum_entanglement_survives_the_wormholes_being_switched_off() {
        let state = ghost_game();
        let mut galaxy = galaxy();
        galaxy.wormholes_off = true;
        let (alpha, beta) = alpha_beta_far_apart(&galaxy);
        assert!(!galaxy.are_adjacent(alpha, beta));
        let ghost = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &a());
        assert!(
            ghost.are_adjacent(alpha, beta),
            "game effects cannot prevent it"
        );
    }

    #[test]
    fn quantum_entanglement_ignores_nexus_sovereignty() {
        let mut galaxy = galaxy();
        galaxy
            .token_wormholes
            .entry("82a".to_owned())
            .or_default()
            .insert("ALPHA".to_owned());
        galaxy.nexus_wormholes_off = true;
        assert!(
            galaxy.wormhole_kinds("82a").is_empty(),
            "suppressed for others"
        );
        let state = ghost_game();
        let pairs = linked_systems(&state, content(), DEFAULT, &galaxy, &a());
        assert!(pairs.iter().any(|(x, y)| x == "82a" || y == "82a"));
        assert!(linked_systems(&state, content(), DEFAULT, &galaxy, &b()).is_empty());
    }

    // -- Hil Colish ------------------------------------------------------------------------------

    #[test]
    fn hil_colish_gives_its_system_a_delta_wormhole_for_every_player() {
        let mut state = ghost_game();
        let mut plain = galaxy();
        let far = plain
            .system_ids()
            .into_iter()
            .find(|id| *id != "17" && !plain.adjacent("17").contains(id))
            .expect("a system away from the gate")
            .to_owned();
        let far = far.as_str();
        assert!(extra_wormholes(&state).is_empty(), "no flagship, no delta");
        put(&mut state, &sys(far), "ghost_flagship", &a(), 1);
        assert_eq!(
            extra_wormholes(&state),
            vec![(far.to_owned(), "DELTA".to_owned())]
        );
        assert!(crate::factions::hooks_movement::apply_extra_wormholes(
            &state, &mut plain
        ));
        for player in [a(), b()] {
            let adjacency = PlayerAdjacency::new(&state, content(), DEFAULT, &plain, &player);
            assert!(
                adjacency.are_adjacent(far, "17"),
                "the flagship's delta joins the gate's for {player}"
            );
        }
    }

    #[test]
    fn hil_colish_follows_the_ship_and_leaves_games_without_it_alone() {
        let mut state = ghost_game();
        put(&mut state, &sys("19"), "ghost_flagship", &a(), 1);
        state.system_mut(&sys("19")).units.clear();
        put(&mut state, &sys("20"), "ghost_flagship", &a(), 1);
        assert_eq!(
            extra_wormholes(&state),
            vec![("20".to_owned(), "DELTA".to_owned())]
        );
        assert!(extra_wormholes(&sol_game()).is_empty());
    }

    // -- Particle Synthesis ---------------------------------------------------------------------

    #[test]
    fn particle_synthesis_counts_physical_wormholes_for_live_production_capacity() {
        let mut state = ghost_game();
        give_breakthrough(&mut state, &a(), "ghostbt");
        let system = sys("39"); // Printed alpha.
        put(&mut state, &system, "cruiser", &a(), 1);
        state
            .wormhole_tokens
            .insert("ALPHA".to_owned(), system.clone());
        state
            .wormhole_tokens
            .insert("BETA".to_owned(), system.clone());
        assert_eq!(
            wormhole_count(&state, &system),
            3,
            "a printed alpha plus alpha and beta tokens are three physical wormholes"
        );
        assert_eq!(
            capacity(&state, content(), DEFAULT, &a(), &system),
            3,
            "the live capacity route sees one PRODUCTION per physical wormhole"
        );

        let plain = sys("19");
        put(&mut state, &plain, "ghost_flagship", &a(), 1);
        state
            .wormhole_tokens
            .insert("GAMMA".to_owned(), plain.clone());
        state.ion_storm = Some((plain.clone(), "ALPHA".to_owned()));
        assert_eq!(
            wormhole_count(&state, &plain),
            3,
            "token, storm, and Hil Colish"
        );

        let nexus = sys(crate::seating::LOCKED_NEXUS);
        put(&mut state, &nexus, "cruiser", &a(), 1);
        state.nexus_unlocked = true;
        assert_eq!(
            wormhole_count(&state, &nexus),
            3,
            "the printed gamma and unlocked alpha/beta are distinct"
        );
    }

    #[test]
    fn particle_synthesis_needs_its_card_and_a_ship_and_reduces_a_live_production_bill() {
        let mut state = ghost_game();
        let system = sys("26");
        add_dock(&mut state, &system, &a());
        let ordinary_capacity = capacity(&state, content(), DEFAULT, &a(), &system);
        assert_eq!(
            cruiser_cost(&state, &a(), &system),
            2,
            "without the card the ordinary production bill is unchanged"
        );

        give_breakthrough(&mut state, &a(), "ghostbt");
        assert_eq!(
            extra_production(&state, content(), DEFAULT, &a(), &system),
            0,
            "a ground unit or structure is not a ship"
        );
        put(&mut state, &system, "infantry", &a(), 1);
        assert_eq!(
            extra_production(&state, content(), DEFAULT, &a(), &system),
            0,
            "ground forces in the space area do not qualify"
        );
        put(&mut state, &system, "cruiser", &a(), 1);
        assert_eq!(
            capacity(&state, content(), DEFAULT, &a(), &system),
            ordinary_capacity + 1,
            "the printed alpha adds one to the ordinary dock capacity"
        );
        assert_eq!(
            cruiser_cost(&state, &a(), &system),
            1,
            "the live production window applies the combined-bill reduction"
        );
    }

    // -- Slipstream ------------------------------------------------------------------------------

    fn bonus(state: &GameState, player: &PlayerId, origin: &str) -> i32 {
        let kind = ti4_content::units::unit_type(content(), "carrier", DEFAULT).unwrap();
        let origin = sys(origin);
        move_bonus(
            state,
            &MoveSite {
                player,
                origin: &origin,
                index: Some(0),
                ship: &kind,
            },
        )
    }

    #[test]
    fn slipstream_adds_one_from_the_home_system_and_from_alpha_and_beta_systems() {
        let state = ghost_game();
        assert_eq!(bonus(&state, &a(), "51"), 1, "home system");
        assert_eq!(bonus(&state, &a(), "39"), 1, "alpha (printed on the tile)");
        assert_eq!(bonus(&state, &a(), "40"), 1, "beta");
        assert_eq!(bonus(&state, &a(), "19"), 0, "a plain system");
        assert_eq!(bonus(&state, &a(), "17"), 0, "delta alone does not count");
        assert_eq!(bonus(&state, &b(), "39"), 0, "only the Creuss player");
        let mut with_token = state;
        with_token
            .wormhole_tokens
            .insert("ALPHA".to_owned(), sys("19"));
        assert_eq!(bonus(&with_token, &a(), "19"), 1, "a Creuss token counts");
    }

    #[test]
    fn slipstream_reaches_the_value_a_ship_is_offered() {
        let state = ghost_game();
        let kind = ti4_content::units::unit_type(content(), "carrier", DEFAULT).unwrap();
        let value = |origin: &str| {
            crate::tactical::effective_move_value_for_ship(
                &state,
                &kind,
                &a(),
                &sys(origin),
                Some(0),
                false,
                false,
            )
        };
        let printed = i32::try_from(kind.move_value()).unwrap();
        assert_eq!(value("39"), printed + 1);
        assert_eq!(value("19"), printed);
        let sol = sol_game();
        assert_eq!(
            crate::tactical::effective_move_value_for_ship(
                &sol,
                &kind,
                &a(),
                &sys("39"),
                Some(0),
                false,
                false
            ),
            printed,
            "no Creuss seat, no bonus"
        );
    }

    // -- Dimensional Splicer ---------------------------------------------------------------------

    fn site<'a>(
        player: &'a PlayerId,
        opponent: &'a PlayerId,
        system: &'a SystemId,
        moment: CombatMoment,
    ) -> HitSite<'a> {
        HitSite {
            player,
            opponent,
            system,
            round: 1,
            moment,
        }
    }
    fn splice(
        state: &mut GameState,
        table: &mut Table,
        system: &str,
        moment: CombatMoment,
    ) -> ProducedHits {
        let (a, b, system) = (a(), b(), sys(system));
        with_context(state, DEFAULT, None, table, |ctx| {
            produced_hits(ctx, &site(&a, &b, &system, moment))
        })
        .expect("legal")
    }
    fn arena(system: &str) -> GameState {
        let mut state = ghost_game();
        give_technology(&mut state, &a(), "ds");
        put(&mut state, &sys(system), "cruiser", &a(), 1);
        put(&mut state, &sys(system), "destroyer", &b(), 1);
        state
    }

    #[test]
    fn dimensional_splicer_produces_a_hit_the_producer_assigns() {
        let mut state = arena("39");
        let before = state.clone();
        let hits = splice(
            &mut state,
            &mut scripted(&["produce_hit"]),
            "39",
            CombatMoment::CombatStart,
        );
        assert_eq!(
            hits,
            ProducedHits {
                producer_assigned: 1,
                ..ProducedHits::NONE
            }
        );
        assert_eq!(state, before, "producing the hit costs nothing");
        // A system holding only a Creuss token is also a wormhole system, and the flagship's
        // delta counts too.
        let mut tokened = arena("19");
        tokened.wormhole_tokens.insert("BETA".to_owned(), sys("19"));
        let hits = splice(
            &mut tokened,
            &mut scripted(&["produce_hit"]),
            "19",
            CombatMoment::CombatStart,
        );
        assert_eq!(hits.producer_assigned, 1);
    }

    #[test]
    fn dimensional_splicer_needs_a_wormhole_the_technology_and_the_start_of_combat() {
        let mut plain = arena("19");
        assert_eq!(
            splice(&mut plain, &mut never(), "19", CombatMoment::CombatStart),
            ProducedHits::NONE,
            "no wormhole in the system"
        );
        let mut wormhole = arena("39");
        assert_eq!(
            splice(&mut wormhole, &mut never(), "39", CombatMoment::RoundEnded),
            ProducedHits::NONE,
            "only at the start of combat"
        );
        let mut without = arena("39");
        without
            .player_mut(&a())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("ds"));
        assert_eq!(
            splice(&mut without, &mut never(), "39", CombatMoment::CombatStart),
            ProducedHits::NONE,
            "no technology"
        );
        let mut declined = arena("39");
        assert_eq!(
            splice(
                &mut declined,
                &mut scripted(&["decline"]),
                "39",
                CombatMoment::CombatStart
            ),
            ProducedHits::NONE,
            "declined"
        );
        // The opponent's side of the same combat: Sol has no Splicer.
        let mut state = arena("39");
        let (b, a, system) = (b(), a(), sys("39"));
        let hits = with_context(&mut state, DEFAULT, None, &mut never(), |ctx| {
            produced_hits(ctx, &site(&b, &a, &system, CombatMoment::CombatStart))
        })
        .expect("legal");
        assert_eq!(hits, ProducedHits::NONE);
    }

    // -- Wormhole Generator ----------------------------------------------------------------------

    fn ghost_with_generator() -> GameState {
        let mut state = ghost_game();
        give_technology(&mut state, &a(), "wg");
        state
    }
    fn generator_option() -> ChoiceOption {
        ChoiceOption::labelled(
            WORMHOLE_GENERATOR,
            crate::faction_abilities::ACTION_KIND,
            "wg",
        )
    }

    #[test]
    fn the_wormhole_generator_places_a_token_and_exhausts() {
        let mut state = ghost_with_generator();
        assert_eq!(
            component_actions(&state, content(), &a()).len(),
            1,
            "offered"
        );
        let galaxy = galaxy();
        let done = with_context(
            &mut state,
            DEFAULT,
            Some(&galaxy),
            &mut scripted(&["ALPHA|19"]),
            |ctx| perform_component(ctx, &a(), &generator_option()),
        );
        assert!(done);
        assert_eq!(state.wormhole_tokens.get("ALPHA"), Some(&sys("19")));
        assert!(
            state
                .player(&a())
                .unwrap()
                .exhausted_technologies
                .contains(&TechnologyId::new("wg"))
        );
        assert!(
            component_actions(&state, content(), &a()).is_empty(),
            "exhausted"
        );
        // Moving the token: the same kind into a second system takes it out of the first.
        state
            .player_mut(&a())
            .unwrap()
            .exhausted_technologies
            .clear();
        let moved = with_context(
            &mut state,
            DEFAULT,
            Some(&galaxy),
            &mut scripted(&["ALPHA|20"]),
            |ctx| perform_component(ctx, &a(), &generator_option()),
        );
        assert!(moved);
        assert_eq!(state.wormhole_tokens.get("ALPHA"), Some(&sys("20")));
        assert_eq!(state.wormhole_tokens.len(), 1);
    }

    #[test]
    fn the_wormhole_generator_is_refused_without_the_card_or_a_legal_system_and_changes_nothing() {
        let galaxy = galaxy();
        // Not the owner.
        assert!(
            component_actions(&ghost_game(), content(), &a()).is_empty(),
            "no card"
        );
        let mut state = ghost_game();
        let before = state.clone();
        assert!(!with_context(
            &mut state,
            DEFAULT,
            Some(&galaxy),
            &mut never(),
            |ctx| { perform_component(ctx, &a(), &generator_option()) }
        ));
        assert_eq!(state, before);
        // Declining the destination leaves the card ready.
        let mut state = ghost_with_generator();
        let before = state.clone();
        assert!(!with_context(
            &mut state,
            DEFAULT,
            Some(&galaxy),
            &mut scripted(&["decline"]),
            |ctx| { perform_component(ctx, &a(), &generator_option()) }
        ));
        assert_eq!(state, before);
        // Another player's ships bar a system with no planet of the player's.
        let mut blocked = ghost_with_generator();
        put(&mut blocked, &sys("19"), "cruiser", &b(), 1);
        let destinations = crate::tokens::wormhole_generator_destinations(
            &blocked,
            content(),
            DEFAULT,
            &galaxy,
            &a(),
        );
        assert!(
            !destinations
                .iter()
                .any(|(_, system)| system.as_str() == "19")
        );
        // No map: nothing happens.
        let mut state = ghost_with_generator();
        let before = state.clone();
        assert!(!with_context(
            &mut state,
            DEFAULT,
            None,
            &mut never(),
            |ctx| { perform_component(ctx, &a(), &generator_option()) }
        ));
        assert_eq!(state, before);
    }

    // -- Icarus Drive ----------------------------------------------------------------------------

    fn mech_game() -> (GameState, PlanetId) {
        let mut state = ghost_game();
        let planet = PlanetId::new("lodor");
        put_on_planet(&mut state, &sys("26"), &planet, "ghost_mech", &a(), 1);
        (state, planet)
    }
    fn activated(player: &str, system: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![("player", player.into()), ("system", system.into())]
    }

    #[test]
    fn icarus_drive_trades_the_mech_for_a_token_in_its_own_system() {
        let (mut state, planet) = mech_game();
        let galaxy = galaxy();
        // Another player activates a different system: the mech's system is its own.
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["unit:ghost:ghost_mech:SYSTEM_ACTIVATED:after", "BETA|26"]),
            "SYSTEM_ACTIVATED",
            &activated("b", "19"),
        );
        assert_eq!(state.wormhole_tokens.get("BETA"), Some(&sys("26")));
        assert!(
            state.board[&sys("26")].on_planet(&planet).is_empty(),
            "the mech left the board"
        );
    }

    #[test]
    fn icarus_drive_is_not_offered_without_a_mech_a_map_or_a_place_for_a_token() {
        let galaxy = galaxy();
        // No mech.
        let mut state = ghost_game();
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("b", "19"),
        );
        assert_eq!(state, before);
        // A mech but no map.
        let (mut state, _) = mech_game();
        let before = state.clone();
        emit(
            &mut state,
            None,
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("b", "19"),
        );
        assert_eq!(state, before);
        // Both tokens already there: nothing to place.
        let (mut state, _) = mech_game();
        state.wormhole_tokens.insert("ALPHA".to_owned(), sys("26"));
        state.wormhole_tokens.insert("BETA".to_owned(), sys("26"));
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("b", "19"),
        );
        assert_eq!(state, before);
        // Declined: the mech stays.
        let (mut state, _) = mech_game();
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["decline"]),
            "SYSTEM_ACTIVATED",
            &activated("b", "19"),
        );
        assert_eq!(state, before);
    }

    // -- Creuss IFF ------------------------------------------------------------------------------

    fn lend_iff(state: &mut GameState, to: &PlayerId) {
        state
            .promissory_notes
            .insert(crate::promissory::note_id("iff", "ghost"), to.clone());
    }
    fn turn_began(player: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![("player", player.into())]
    }

    #[test]
    fn creuss_iff_places_a_token_and_returns_to_the_creuss_player() {
        let mut state = ghost_game();
        lend_iff(&mut state, &b());
        state.phase = Phase::Action;
        let galaxy = galaxy();
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["promissory:sol:iff:TURN_BEGAN:after", "BETA|19"]),
            "TURN_BEGAN",
            &turn_began("b"),
        );
        assert_eq!(state.wormhole_tokens.get("BETA"), Some(&sys("19")));
        assert_eq!(
            state
                .promissory_notes
                .get(&crate::promissory::note_id("iff", "ghost")),
            Some(&a()),
            "returned to the Creuss player"
        );
    }

    #[test]
    fn creuss_iff_is_not_offered_to_the_owner_off_turn_or_outside_the_action_phase() {
        let galaxy = galaxy();
        // The Creuss player holds their own note.
        let mut state = ghost_game();
        state.phase = Phase::Action;
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "TURN_BEGAN",
            &turn_began("a"),
        );
        assert_eq!(state, before);
        // Held by b, but it is a's turn.
        let mut state = ghost_game();
        lend_iff(&mut state, &b());
        state.phase = Phase::Action;
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "TURN_BEGAN",
            &turn_began("a"),
        );
        assert_eq!(state, before);
        // Held by b on b's turn, in the wrong phase.
        let mut state = ghost_game();
        lend_iff(&mut state, &b());
        state.phase = Phase::Status;
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "TURN_BEGAN",
            &turn_began("b"),
        );
        assert_eq!(state, before);
        // Declined: the note stays with its holder.
        let mut state = ghost_game();
        lend_iff(&mut state, &b());
        state.phase = Phase::Action;
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["decline"]),
            "TURN_BEGAN",
            &turn_began("b"),
        );
        assert_eq!(state, before);
    }

    // -- Emissary Taivra -------------------------------------------------------------------------

    fn activate(state: &mut GameState, player: &PlayerId, system: &str) {
        state.activation_seq += 1;
        state.active = Some(player.clone());
        state.active_system = Some(sys(system));
    }

    #[test]
    fn emissary_taivra_exhausting_is_heard_by_a_watching_nomad() {
        let mut state = ghost_game();
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("nomad");
        give_technology(&mut state, &b(), "tcs");
        let galaxy = galaxy();
        activate(&mut state, &b(), "39");
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["leader:ghost:ghostagent:SYSTEM_ACTIVATED:after"]),
            "SYSTEM_ACTIVATED",
            &activated("b", "39"),
        );
        assert_eq!(
            crate::supply::staged_event_types(&state),
            ["AGENT_EXHAUSTED"]
        );
    }

    #[test]
    fn emissary_taivra_makes_the_activated_system_adjacent_to_every_wormhole_system() {
        let mut state = ghost_game();
        let galaxy = galaxy();
        activate(&mut state, &b(), "39");
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["leader:ghost:ghostagent:SYSTEM_ACTIVATED:after"]),
            "SYSTEM_ACTIVATED",
            &activated("b", "39"),
        );
        assert_eq!(
            leader_status(&state, &a(), "ghostagent"),
            Some(LeaderStatus::Exhausted)
        );
        let sol = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &b());
        for other in ["40", "25", "17"] {
            assert!(
                sol.are_adjacent("39", other),
                "39 is adjacent to {other} for everyone"
            );
        }

        // A later activation of anything else: the link has lapsed.
        activate(&mut state, &b(), "19");
        let later = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &b());
        assert_eq!(
            later.are_adjacent("39", "17"),
            galaxy.adjacent("39").contains("17"),
            "only the hex decides again"
        );
        assert!(agent_link(&state).is_none());
    }

    #[test]
    fn emissary_taivra_needs_a_ready_agent_and_a_non_delta_wormhole() {
        let galaxy = galaxy();
        // Delta only: the gate.
        let mut state = ghost_game();
        activate(&mut state, &b(), "17");
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("b", "17"),
        );
        assert_eq!(state, before, "a delta wormhole does not qualify");
        // No wormhole at all.
        let mut state = ghost_game();
        activate(&mut state, &b(), "19");
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("b", "19"),
        );
        assert_eq!(state, before);
        // Exhausted agent.
        let mut state = ghost_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostagent"), LeaderStatus::Exhausted);
        activate(&mut state, &b(), "39");
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("b", "39"),
        );
        assert_eq!(state, before);
        // Declined.
        let mut state = ghost_game();
        activate(&mut state, &b(), "39");
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut scripted(&["decline"]),
            "SYSTEM_ACTIVATED",
            &activated("b", "39"),
        );
        assert_eq!(state, before);
    }

    #[test]
    fn ssruu_copies_emissary_taivra_for_readied_or_exhausted_source() {
        let galaxy = galaxy();
        for source_status in [LeaderStatus::Readied, LeaderStatus::Exhausted] {
            let mut state = ghost_ssruu_game();
            state
                .player_mut(&a())
                .unwrap()
                .leaders
                .insert(LeaderId::new("ghostagent"), source_status);
            state
                .player_mut(&b())
                .unwrap()
                .leaders
                .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
            activate(&mut state, &c(), "39");
            emit(
                &mut state,
                Some(&galaxy),
                &mut borrow_ghost_agent(),
                "SYSTEM_ACTIVATED",
                &activated("c", "39"),
            );

            assert_eq!(agent_link(&state), Some(sys("39")));
            assert_eq!(
                leader_status(&state, &a(), "ghostagent"),
                Some(source_status),
                "the copied route leaves the source unchanged"
            );
            assert_eq!(
                leader_status(&state, &b(), "yssarilagent"),
                Some(LeaderStatus::Exhausted),
                "only Ssruu exhausts"
            );
            let adjacency = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &c());
            assert!(adjacency.are_adjacent("39", "40"));
        }
    }

    #[test]
    fn borrowed_agent_readiness_is_live_after_resolver_registration() {
        let galaxy = galaxy();
        let mut state = ghost_ssruu_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostagent"), LeaderStatus::Exhausted);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Exhausted);
        activate(&mut state, &c(), "39");
        let mut resolver = armed_resolver(&state);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostagent"), LeaderStatus::Readied);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        let mut table = borrow_ghost_agent();
        with_context(&mut state, DEFAULT, Some(&galaxy), &mut table, |context| {
            let event = context
                .event_sequence
                .next(
                    "SYSTEM_ACTIVATED",
                    activated("c", "39")
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), value))
                        .collect(),
                )
                .expect("an event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("the live borrowed agent resolves");
        });
        assert_eq!(agent_link(&state), Some(sys("39")));
        assert_eq!(
            leader_status(&state, &a(), "ghostagent"),
            Some(LeaderStatus::Readied)
        );
        assert_eq!(
            leader_status(&state, &b(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn copied_agent_rejects_unavailable_cards_and_illegal_wormhole_targets() {
        let galaxy = galaxy();
        for (source_status, borrower_status, remove_source, system) in [
            (
                Some(LeaderStatus::Locked),
                Some(LeaderStatus::Readied),
                false,
                "39",
            ),
            (Some(LeaderStatus::Exhausted), None, false, "39"),
            (
                Some(LeaderStatus::Exhausted),
                Some(LeaderStatus::Exhausted),
                false,
                "39",
            ),
            (None, Some(LeaderStatus::Readied), true, "39"),
            (
                Some(LeaderStatus::Exhausted),
                Some(LeaderStatus::Readied),
                false,
                "17",
            ),
            (
                Some(LeaderStatus::Exhausted),
                Some(LeaderStatus::Readied),
                false,
                "19",
            ),
        ] {
            let mut state = ghost_ssruu_game();
            if remove_source {
                state
                    .player_mut(&a())
                    .unwrap()
                    .leaders
                    .remove(&LeaderId::new("ghostagent"));
            } else if let Some(status) = source_status {
                state
                    .player_mut(&a())
                    .unwrap()
                    .leaders
                    .insert(LeaderId::new("ghostagent"), status);
            }
            if let Some(status) = borrower_status {
                state
                    .player_mut(&b())
                    .unwrap()
                    .leaders
                    .insert(LeaderId::new("yssarilagent"), status);
            } else {
                state
                    .player_mut(&b())
                    .unwrap()
                    .leaders
                    .remove(&LeaderId::new("yssarilagent"));
            }
            activate(&mut state, &c(), system);
            let before = state.clone();
            emit(
                &mut state,
                Some(&galaxy),
                &mut never(),
                "SYSTEM_ACTIVATED",
                &activated("c", system),
            );
            assert_eq!(state, before, "invalid copied-agent case {system}");
            assert_eq!(leader_status(&state, &a(), "ghostagent"), source_status);
            assert_eq!(leader_status(&state, &b(), "yssarilagent"), borrower_status);
        }

        let one_wormhole = Galaxy::build(content(), &["18", "19", "39"], DEFAULT, 3)
            .expect("a map with only system 39 holding a wormhole");
        assert_eq!(one_wormhole.wormhole_systems().len(), 1);
        let mut state = ghost_ssruu_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostagent"), LeaderStatus::Exhausted);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        activate(&mut state, &c(), "39");
        let before = state.clone();
        emit(
            &mut state,
            Some(&one_wormhole),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("c", "39"),
        );
        assert_eq!(state, before, "there is no other wormhole system");
    }

    // -- Sai Seravus -----------------------------------------------------------------------------

    #[test]
    fn sai_seravus_places_one_fighter_per_capacity_ship_after_the_whole_move() {
        let mut state = ghost_game();
        let destination = sys("40");
        state.active_system = Some(destination.clone());
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostcommander"), LeaderStatus::Unlocked);
        put(&mut state, &destination, "carrier", &a(), 2);
        let payload = [
            ("player", serde_json::json!("a")),
            ("system", serde_json::json!("40")),
            ("unit", serde_json::json!("carrier")),
            ("wormholes", serde_json::json!(1)),
        ];
        let mut table = scripted(&["fighters|2"]);
        for _ in 0..2 {
            emit(
                &mut state,
                Some(&galaxy()),
                &mut table,
                "SHIP_MOVED",
                &payload,
            );
        }
        assert_eq!(commander_count(&state, &a()), 2);
        assert_eq!(
            state
                .system_state(&destination)
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "fighter")
                .count(),
            0,
            "the fighters are not placed between ship moves",
        );
        emit(
            &mut state,
            Some(&galaxy()),
            &mut table,
            "MOVEMENT_FINISHED",
            &[
                ("player", serde_json::json!("a")),
                ("system", serde_json::json!("40")),
            ],
        );
        assert_eq!(
            state
                .system_state(&destination)
                .units
                .iter()
                .filter(|unit| unit.type_id.as_str() == "fighter" && unit.owner == a())
                .count(),
            2,
        );
        assert_eq!(
            commander_count(&state, &a()),
            0,
            "the activation mark is cleared"
        );
    }

    #[test]
    fn an_unseated_ghost_commander_grant_works_for_another_faction() {
        let mut state = sol_game();
        let destination = sys("40");
        state.active_system = Some(destination.clone());
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content(),
            &a(),
            "ghostcommander",
        ));
        put(&mut state, &destination, "carrier", &a(), 1);

        let mut table = scripted(&["fighters|1"]);
        emit(
            &mut state,
            Some(&galaxy()),
            &mut table,
            "SHIP_MOVED",
            &[
                ("player", serde_json::json!("a")),
                ("system", serde_json::json!("40")),
                ("unit", serde_json::json!("carrier")),
                ("wormholes", serde_json::json!(1)),
            ],
        );
        assert_eq!(commander_count(&state, &a()), 1);
        emit(
            &mut state,
            Some(&galaxy()),
            &mut table,
            "MOVEMENT_FINISHED",
            &[
                ("player", serde_json::json!("a")),
                ("system", serde_json::json!("40")),
            ],
        );
        assert_eq!(commander_count(&state, &a()), 0);
        assert_eq!(
            state
                .system_state(&destination)
                .units
                .iter()
                .filter(|unit| { unit.owner == a() && unit.type_id.as_str() == "fighter" })
                .count(),
            1,
        );
        assert!(
            !state
                .player(&a())
                .unwrap()
                .leaders
                .contains_key(&LeaderId::new("ghostcommander"))
        );
    }

    #[test]
    fn ordinary_ghost_alliance_stays_locked_until_its_owner_unlocks() {
        let mut state = sol_game();
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("ghost");
        let destination = sys("40");
        state.active_system = Some(destination.clone());
        crate::promissory::take(&mut state, content(), &a(), "an:ghost");
        put(&mut state, &destination, "carrier", &a(), 1);
        let mut table = never();
        emit(
            &mut state,
            Some(&galaxy()),
            &mut table,
            "SHIP_MOVED",
            &[
                ("player", serde_json::json!("a")),
                ("system", serde_json::json!("40")),
                ("unit", serde_json::json!("carrier")),
                ("wormholes", serde_json::json!(1)),
            ],
        );
        emit(
            &mut state,
            Some(&galaxy()),
            &mut table,
            "MOVEMENT_FINISHED",
            &[
                ("player", serde_json::json!("a")),
                ("system", serde_json::json!("40")),
            ],
        );
        assert_eq!(commander_count(&state, &a()), 0);
        assert!(
            !state
                .system_state(&destination)
                .units
                .iter()
                .any(|unit| { unit.owner == a() && unit.type_id.as_str() == "fighter" })
        );
    }

    #[test]
    fn sai_seravus_needs_the_unlocked_commander_a_wormhole_and_unused_capacity() {
        let destination = sys("40");
        let moved = |state: &mut GameState, table: &mut Table, unit: &str, wormholes: i64| {
            emit(
                state,
                Some(&galaxy()),
                table,
                "SHIP_MOVED",
                &[
                    ("player", serde_json::json!("a")),
                    ("system", serde_json::json!("40")),
                    ("unit", serde_json::json!(unit)),
                    ("wormholes", serde_json::json!(wormholes)),
                ],
            );
        };
        let finish = |state: &mut GameState, table: &mut Table| {
            emit(
                state,
                Some(&galaxy()),
                table,
                "MOVEMENT_FINISHED",
                &[
                    ("player", serde_json::json!("a")),
                    ("system", serde_json::json!("40")),
                ],
            );
        };
        let mut state = ghost_game();
        state.active_system = Some(destination.clone());
        put(&mut state, &destination, "carrier", &a(), 1);
        let mut table = never();
        moved(&mut state, &mut table, "carrier", 1);
        finish(&mut state, &mut table);
        assert_eq!(
            commander_count(&state, &a()),
            0,
            "a locked commander does nothing"
        );

        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostcommander"), LeaderStatus::Unlocked);
        moved(&mut state, &mut table, "carrier", 0);
        moved(&mut state, &mut table, "cruiser", 1);
        finish(&mut state, &mut table);
        assert_eq!(
            commander_count(&state, &a()),
            0,
            "no wormhole or no capacity value"
        );

        moved(&mut state, &mut table, "carrier", 1);
        let free = crate::fleet::standing(&state, content(), DEFAULT, &a(), &destination, None)
            .capacity_free();
        put(
            &mut state,
            &destination,
            "infantry",
            &a(),
            usize::try_from(free).unwrap(),
        );
        finish(&mut state, &mut table);
        assert_eq!(commander_count(&state, &a()), 0);
        assert!(
            !state
                .system_state(&destination)
                .units
                .iter()
                .any(|unit| unit.type_id.as_str() == "fighter" && unit.owner == a())
        );
    }

    #[test]
    fn sai_seravus_places_a_fighter_after_a_real_quantum_wormhole_move() {
        use crate::game::{Game, TACTICAL_ACTION_ID};
        struct Drive {
            from: String,
            to: String,
        }
        impl Decider for Drive {
            fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
                let preferred = [
                    TACTICAL_ACTION_ID.to_owned(),
                    self.to.clone(),
                    format!("move|{}|0", self.from),
                    "done_moving".to_owned(),
                    "fighters|1".to_owned(),
                ];
                for id in preferred {
                    if let Some(option) = choice.options.iter().find(|option| option.id == id) {
                        return Ok(option.clone());
                    }
                }
                choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .cloned()
                    .ok_or_else(|| IllegalChoice::DeciderFailed {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                        reason: "unexpected live Ghost commander choice".to_owned(),
                    })
            }
        }
        let mut state = ghost_game();
        let galaxy = galaxy();
        let (from, to) = alpha_beta_far_apart(&galaxy);
        let destination = sys(to);
        state.phase = Phase::Action;
        state.active = Some(a());
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghostcommander"), LeaderStatus::Unlocked);
        put(&mut state, &sys(from), "carrier", &a(), 1);
        let table = Table::with_default(Box::new(Drive {
            from: from.to_owned(),
            to: to.to_owned(),
        }));
        let mut game = Game::with_table(state, content(), table).with_galaxy(galaxy);
        for _ in 0..20 {
            assert_eq!(game.step().error, None, "log: {:?}", game.events);
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        assert!(
            game.events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
        );
        assert_eq!(
            game.state.ships_of(&a(), &destination).len(),
            2,
            "the carrier and its one new fighter reach the active system"
        );
        assert_eq!(commander_count(&game.state, &a()), 0);
    }

    // -- Riftwalker Meian and Sai Seravus --------------------------------------------------------

    fn hero_game() -> GameState {
        let mut state = ghost_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("ghosthero"), LeaderStatus::Unlocked);
        put(&mut state, &sys("19"), "cruiser", &a(), 1);
        // The offer knows only the board record: a second system with the owner's units.
        put(&mut state, &sys("40"), "cruiser", &a(), 1);
        state
    }
    fn hero() -> LeaderId {
        LeaderId::new("ghosthero")
    }

    #[test]
    fn riftwalker_meian_swaps_two_systems_and_the_map_follows() {
        let mut state = hero_game();
        let mut galaxy = galaxy();
        let (hex_19, hex_39) = (
            galaxy.coord_of("19").unwrap(),
            galaxy.coord_of("39").unwrap(),
        );
        assert_eq!(leader_action(&state, content(), &a(), &hero()), Some(true));
        let done = with_context(
            &mut state,
            DEFAULT,
            Some(&galaxy),
            &mut scripted(&["19", "39"]),
            |ctx| use_leader(ctx, &a(), &hero()),
        );
        assert_eq!(done, Some(true));
        assert!(
            state
                .faction_marks
                .keys()
                .any(|key| key.starts_with("map_edit|")),
            "recorded for the game's own map"
        );
        crate::movement::replay_map_edits(&state, &mut galaxy, content(), DEFAULT)
            .expect("replays");
        assert_eq!(galaxy.coord_of("19"), Some(hex_39));
        assert_eq!(galaxy.coord_of("39"), Some(hex_19));
    }

    #[test]
    fn riftwalker_meian_refuses_the_gate_the_home_system_the_nexus_and_fracture_systems() {
        for excluded in ["17", "51", "82a", "fracture1"] {
            assert!(hero_excluded(excluded), "{excluded}");
        }
        let mut state = hero_game();
        put(&mut state, &sys("17"), "cruiser", &a(), 1);
        let galaxy = galaxy();
        // The gate is offered as neither end of the swap.
        let mut table = scripted(&["17"]);
        let before = state.clone();
        let done = with_context(&mut state, DEFAULT, Some(&galaxy), &mut table, |ctx| {
            use_leader(ctx, &a(), &hero())
        });
        assert_eq!(done, Some(false), "17 was not among the options");
        assert_eq!(state, before);
        // Declining at either step changes nothing.
        for answers in [&["decline"][..], &["19", "decline"][..]] {
            let mut state = hero_game();
            let before = state.clone();
            let done = with_context(
                &mut state,
                DEFAULT,
                Some(&galaxy),
                &mut scripted(answers),
                |ctx| use_leader(ctx, &a(), &hero()),
            );
            assert_eq!(done, Some(false));
            assert_eq!(state, before);
        }
        // Not this module's leader.
        assert_eq!(
            leader_action(&state, content(), &a(), &LeaderId::new("naaluhero")),
            None
        );
        // No map: no swap.
        let mut state = hero_game();
        let done = with_context(&mut state, DEFAULT, None, &mut never(), |ctx| {
            use_leader(ctx, &a(), &hero())
        });
        assert_eq!(done, Some(false));
        // Fewer than two eligible systems: not offered.
        let bare = seated_game(&[("a", "ghost"), ("b", "sol")], DEFAULT);
        assert_eq!(leader_action(&bare, content(), &a(), &hero()), Some(false));
    }

    #[test]
    fn sai_seravus_unlocks_with_units_in_three_alpha_or_beta_systems() {
        let mut state = ghost_game();
        let commander = LeaderId::new("ghostcommander");
        let unlocked = |state: &GameState| {
            commander_unlocked(state, content(), DEFAULT, None, &a(), &commander)
        };
        assert_eq!(unlocked(&state), Some(false));
        put(&mut state, &sys("39"), "cruiser", &a(), 1);
        put(&mut state, &sys("40"), "cruiser", &a(), 1);
        assert_eq!(unlocked(&state), Some(false), "two systems");
        put(&mut state, &sys("19"), "cruiser", &a(), 1);
        assert_eq!(
            unlocked(&state),
            Some(false),
            "a plain system does not count"
        );
        put(&mut state, &sys("26"), "cruiser", &b(), 1);
        assert_eq!(
            unlocked(&state),
            Some(false),
            "another player's units do not count"
        );
        put_on_planet(
            &mut state,
            &sys("26"),
            &PlanetId::new("lodor"),
            "infantry",
            &a(),
            1,
        );
        assert_eq!(unlocked(&state), Some(true), "ground forces count");
        assert_eq!(
            commander_unlocked(
                &state,
                content(),
                DEFAULT,
                None,
                &a(),
                &LeaderId::new("naalucommander")
            ),
            None
        );
    }

    // -- Games without a Creuss seat -------------------------------------------------------------

    #[test]
    fn a_game_without_a_creuss_seat_is_unchanged_and_offered_nothing() {
        let mut state = sol_game();
        let galaxy = galaxy();
        let (alpha, beta) = alpha_beta_far_apart(&galaxy);
        for player in [a(), b()] {
            let adjacency = PlayerAdjacency::new(&state, content(), DEFAULT, &galaxy, &player);
            for system in galaxy.system_ids() {
                let plain: BTreeSet<String> = galaxy
                    .adjacent(system)
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect();
                assert_eq!(adjacency.neighbours(system), plain, "{system} for {player}");
            }
            assert!(!adjacency.are_adjacent(alpha, beta));
            assert!(linked_systems(&state, content(), DEFAULT, &galaxy, &player).is_empty());
            let option_ids: Vec<String> =
                crate::factions::component_actions(&state, content(), &player)
                    .into_iter()
                    .map(|option| option.id)
                    .filter(|id| id.starts_with("faction|ghost|"))
                    .collect();
            assert!(option_ids.is_empty());
            assert_eq!(
                leader_action(&state, content(), &player, &hero()),
                Some(false)
            );
        }
        assert!(extra_wormholes(&state).is_empty());
        assert_eq!(bonus(&state, &a(), "39"), 0);
        // No Creuss ability asks anyone anything, whoever activates, whoever begins a turn.
        state.phase = Phase::Action;
        let before = state.clone();
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "SYSTEM_ACTIVATED",
            &activated("a", "39"),
        );
        emit(
            &mut state,
            Some(&galaxy),
            &mut never(),
            "TURN_BEGAN",
            &turn_began("b"),
        );
        assert_eq!(state, before);
    }

    #[test]
    fn a_nekro_flagship_with_the_ghost_z_token_gives_its_system_a_delta_wormhole() {
        let mut bare = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], &[]);
        put(&mut bare, &sys("19"), "nekro_flagship", &a(), 1);
        assert!(extra_wormholes(&bare).is_empty(), "off by default");
        let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], &["ghost"]);
        put(&mut state, &sys("19"), "nekro_flagship", &a(), 1);
        assert_eq!(
            extra_wormholes(&state),
            vec![("19".to_owned(), "DELTA".to_owned())]
        );
    }
}
