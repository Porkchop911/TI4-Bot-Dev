//! The Winnu (`winnu`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-winnu.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Blood Ties: "You do not have to spend influence to remove the custodians token from Mecatol
//!   Rex."
//! * Reclamation: "After you resolve a tactical action during which you gained control of Mecatol
//!   Rex: You may place 1 PDS and 1 space dock from your reinforcements on Mecatol Rex."
//! * Lazax Gate Folding (`lgf`): "During your tactical actions, if you do not control Mecatol Rex,
//!   treat its system as if it has both an alpha and beta wormhole. ACTION: If you control Mecatol
//!   Rex, exhaust this card to place 1 infantry from your reinforcements on Mecatol Rex."
//! * Hegemonic Trade Policy (`htp`): "Exhaust this card when 1 or more of your units use
//!   PRODUCTION; swap the resource and influence values of 1 planet you control during that use of
//!   Production."
//! * Salai Sai Corian (flagship): "When this unit makes a combat roll, it rolls a number of dice
//!   (hit on a 7) equal to the number of your opponent's non-fighter ships in this system."
//! * Reclaimer (mech): "After you resolve a tactical action where you gained control of this
//!   planet, you may place 1 PDS or 1 Space Dock from your reinforcements on this planet."
//! * Acquiescence (`acq`): "When the Winnu player resolves a strategic action: You do not have to
//!   spend or place a command token to resolve the secondary ability of that strategy card. Then,
//!   return this card to the Winnu player."
//! * Berekar Berekon (`winnuagent`): "When 1 or more of a player's units use PRODUCTION: You may
//!   exhaust this card to reduce the combined cost of the produced units by 2."
//! * Rickar Rickani (`winnucommander`): "Apply +2 to the result of each of your unit's combat
//!   rolls in the Mecatol Rex system, your home system, and each system that contains a legendary
//!   planet." Unlock: "Control Mecatol Rex or enter into a combat in the Mecatol Rex system."
//! * Mathis Mathinus (`winnuhero`): "ACTION: Perform the primary ability of any strategy card.
//!   Then, choose any number of other players. Those players may perform the secondary ability of
//!   that strategy card. Then, purge this card."
//! * Imperator (`winnubt`): "Apply +1 to the results of each of your unit's combat rolls for each
//!   \"Support for the Throne\" in your opponent's play area. After you activate a system that
//!   contains a legendary planet, apply +1 to the move value of 1 of your ships during this
//!   tactical action."

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::catalogue;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, StrategyCardId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};

use super::hooks_economy::EconomyHooks;
use super::hooks_ground::GroundHooks;
use super::hooks_movement::{MoveSite, MovementHooks};
use super::hooks_strategy::{SecondaryWaiver, StrategyHooks};
use super::{CombatUnit, FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids (`acq:winnu`).
const FACTION: &str = "winnu";
const ACQUIESCENCE: &str = "acq:winnu";
const COMMANDER: &str = "winnucommander";
const AGENT: &str = "winnuagent";

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["blood_ties", "reclamation"],
    technologies: &["lgf", "htp"],
    units: &["winnu_flagship", "winnu_mech"],
    promissory: &["acq"],
    leaders: &[AGENT, COMMANDER, "winnuhero"],
    breakthroughs: &["winnubt"],
    hooks: Hooks {
        component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        use_leader_strategy_primary: Some(use_leader_strategy_primary),
        leader_strategy_followers: Some(leader_strategy_follower_choices),
        timing_abilities: Some(timing_abilities),
        unit_roll_modifier: Some(unit_roll_modifier),
        unit_dice: Some(unit_dice),
        ground: GroundHooks {
            custodians_free: Some(custodians_free),
            ..GroundHooks::NONE
        },
        economy: EconomyHooks {
            planet_spend_value: Some(planet_spend_value),
            ..EconomyHooks::NONE
        },
        movement: MovementHooks {
            linked_systems: Some(linked_systems),
            move_bonus: Some(move_bonus),
            ..MovementHooks::NONE
        },
        strategy: StrategyHooks {
            secondary_waivers: Some(secondary_waivers),
            secondary_waived: Some(secondary_waived),
            ..StrategyHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- small readers -------------------------------------------------------------------------------

fn is_winnu(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
}

fn technology_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::technology_text_ready(state, player, alias)
}

fn exhaust_technology(state: &mut GameState, player: &PlayerId, alias: &str) {
    crate::technology::exhaust_technology_text(state, player, alias);
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    crate::leaders::status(state, player, &LeaderId::new(leader))
}

fn mecatol(state: &GameState) -> SystemId {
    SystemId::new(crate::seating::mecatol_on(state))
}

/// The planets of the Mecatol Rex system that `player` controls (Mecatol Rex itself).
fn mecatol_planets(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    state
        .system_state(&mecatol(state))
        .planet_control
        .iter()
        .filter(|(_, owner)| *owner == player)
        .map(|(planet, _)| planet.clone())
        .collect()
}

fn controls_mecatol(state: &GameState, player: &PlayerId) -> bool {
    !mecatol_planets(state, player).is_empty()
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

/// The unit id the box is counted under for a base type (`pds`, `spacedock`, `infantry`).
fn box_has(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base: &str,
) -> bool {
    catalogue(content, sources).get(base).is_some_and(|kind| {
        crate::supply::allowed(
            state,
            content,
            sources,
            player,
            &ti4_model::id::UnitTypeId::new(kind.id().to_owned()),
            1,
        ) > 0
    })
}

// -- Blood Ties ----------------------------------------------------------------------------------

fn custodians_free(state: &GameState, _content: &ContentStore, player: &PlayerId) -> bool {
    is_winnu(state, player)
}

// -- gained-control marks (Reclamation, Reclaimer) -----------------------------------------------

/// Private mark: `"<system>|<planet>,..."` of planets this seat gained control of during the
/// current turn's tactical action.
fn gained_key(player: &PlayerId) -> String {
    format!("private:{player}:winnu:gained")
}

fn gained(state: &GameState, player: &PlayerId) -> Vec<(SystemId, PlanetId)> {
    state
        .faction_marks
        .get(&gained_key(player))
        .map(|value| {
            value
                .split(',')
                .filter_map(|entry| entry.split_once('|'))
                .map(|(system, planet)| (SystemId::new(system), PlanetId::new(planet)))
                .collect()
        })
        .unwrap_or_default()
}

fn controls(state: &GameState, player: &PlayerId, system: &SystemId, planet: &PlanetId) -> bool {
    state.system_state(system).planet_control.get(planet) == Some(player)
}

fn tactical_key(player: &PlayerId) -> String {
    format!("private:{player}:winnu:tactical")
}

/// A tactical action began this turn (a system was activated): only then does control gained
/// count for Reclamation and the Reclaimer.
fn tactical_mark(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:reclamation_tactical:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context
                .state
                .faction_marks
                .insert(tactical_key(&owner), "1".to_owned());
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        is_winnu(context.state, &condition_seat)
            && event.text("player") == Some(condition_seat.as_str())
    }))
}

fn gained_mark(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:reclamation_mark:PLANET_CONTROL_GAINED:after"),
        seat.clone(),
        "PLANET_CONTROL_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if let (Some(system), Some(planet)) = (event.text("system"), event.text("planet")) {
                let mut list = context
                    .state
                    .faction_marks
                    .get(&gained_key(&owner))
                    .cloned()
                    .unwrap_or_default();
                if !list.is_empty() {
                    list.push(',');
                }
                list.push_str(system);
                list.push('|');
                list.push_str(planet);
                context.state.faction_marks.insert(gained_key(&owner), list);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        is_winnu(context.state, &condition_seat)
            && event.text("player") == Some(condition_seat.as_str())
            && context.state.active.as_ref() == Some(&condition_seat)
            && context
                .state
                .faction_marks
                .contains_key(&tactical_key(&condition_seat))
    }))
}

fn gained_clear(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:reclamation_clear:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&gained_key(&owner));
            context.state.faction_marks.remove(&tactical_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        let marks = &context.state.faction_marks;
        marks.contains_key(&gained_key(&condition_seat))
            || marks.contains_key(&tactical_key(&condition_seat))
    }))
}

/// Structures of one base type `player` already has on a planet.
fn structures_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    base: &str,
) -> usize {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .on_planet_of(planet, player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == base)
        })
        .count()
}

/// A planet holds at most 1 space dock and 2 PDS.
fn room_for(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    base: &str,
) -> bool {
    let limit = if base == "pds" { 2 } else { 1 };
    structures_on(state, content, sources, player, system, planet, base) < limit
}

fn reclamation_ready(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    seat: &PlayerId,
) -> Option<PlanetId> {
    if !is_winnu(state, seat) {
        return None;
    }
    let planet = gained(state, seat)
        .into_iter()
        .find(|(system, planet)| {
            crate::seating::is_mecatol(system.as_str()) && controls(state, seat, system, planet)
        })?
        .1;
    let system = mecatol(state);
    (["pds", "spacedock"].iter().all(|base| {
        box_has(state, content, sources, seat, base)
            && room_for(state, content, sources, seat, &system, &planet, base)
    }))
    .then_some(planet)
}

/// Reclamation: both structures or nothing; the box is checked before the first is placed.
fn reclamation(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:reclamation:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event.text("player") != Some(owner.as_str()) {
                return Ok(());
            }
            let Some(planet) =
                reclamation_ready(context.state, context.content, context.sources, &owner)
            else {
                return Ok(());
            };
            let system = mecatol(context.state);
            for base in ["pds", "spacedock"] {
                crate::action_cards::place_units_counted(
                    context,
                    &owner,
                    &system,
                    Some(&planet),
                    base,
                    1,
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_seat.as_str())
            && reclamation_ready(
                context.state,
                context.content,
                context.sources,
                &condition_seat,
            )
            .is_some()
    }))
}

fn reclaimer_options(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    seat: &PlayerId,
) -> Vec<ChoiceOption> {
    if !is_winnu(state, seat) {
        return Vec::new();
    }
    let mut options = Vec::new();
    for (system, planet) in gained(state, seat) {
        let has_mech = state
            .system_state(&system)
            .on_planet_of(&planet, seat)
            .iter()
            .any(|unit| unit.type_id.as_str() == "winnu_mech");
        if !has_mech || !controls(state, seat, &system, &planet) {
            continue;
        }
        for (base, label) in [("pds", "PDS"), ("spacedock", "space dock")] {
            if box_has(state, content, sources, seat, base)
                && room_for(state, content, sources, seat, &system, &planet, base)
            {
                options.push(
                    ChoiceOption::labelled(
                        format!("{base}|{system}|{planet}"),
                        "unit",
                        format!("Reclaimer: place a {label} on {planet}"),
                    )
                    .with("system", system.to_string())
                    .with("planet", planet.to_string()),
                );
            }
        }
    }
    options
}

/// Reclaimer: one structure on a planet it stands on and that this action took.
fn reclaimer(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:winnu_mech:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event.text("player") != Some(owner.as_str()) {
                return Ok(());
            }
            let options =
                reclaimer_options(context.state, context.content, context.sources, &owner);
            if options.is_empty() {
                return Ok(());
            }
            let choice = Choice::new(
                owner.clone(),
                "Reclaimer: place a structure".to_owned(),
                options,
            )
            .contextualized(decision(context.state, &owner, "winnu_mech", "reclaimer"));
            let answer = context
                .ask_seeing(&choice)
                .map_err(TimingError::IllegalChoice)?;
            let mut parts = answer.id.split('|');
            if let (Some(base), Some(system), Some(planet)) =
                (parts.next(), parts.next(), parts.next())
            {
                crate::action_cards::place_units_counted(
                    context,
                    &owner,
                    &SystemId::new(system),
                    Some(&PlanetId::new(planet)),
                    base,
                    1,
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_seat.as_str())
            && !reclaimer_options(
                context.state,
                context.content,
                context.sources,
                &condition_seat,
            )
            .is_empty()
    }))
}

// -- Lazax Gate Folding --------------------------------------------------------------------------

/// During this player's tactical action, Mecatol Rex links to every alpha and beta system.
fn linked_systems(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<(String, String)> {
    if !has_technology(state, player, "lgf")
        || state.active.as_ref() != Some(player)
        || state.active_system.is_none()
        || controls_mecatol(state, player)
        || galaxy
            .coord_of(crate::seating::mecatol_in_galaxy(galaxy))
            .is_none()
    {
        return Vec::new();
    }
    galaxy
        .wormhole_systems()
        .into_iter()
        .filter(|system| !crate::seating::is_mecatol(system))
        .filter(|system| {
            galaxy
                .wormhole_kinds(system)
                .iter()
                .any(|kind| *kind == "ALPHA" || *kind == "BETA")
        })
        .map(|system| {
            (
                crate::seating::mecatol_in_galaxy(galaxy).to_owned(),
                system.to_owned(),
            )
        })
        .collect()
}

const LGF_ACTION: &str = "faction|winnu|lgf";

fn lgf_ready(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Option<PlanetId> {
    if !technology_ready(state, player, "lgf") {
        return None;
    }
    let planet = mecatol_planets(state, player).into_iter().next()?;
    box_has(state, content, sources, player, "infantry").then_some(planet)
}

fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    // Sources are not passed to this hook; the corpus default is what games are built with.
    if lgf_ready(state, content, ti4_model::content_types::DEFAULT, player).is_some() {
        vec![ChoiceOption::labelled(
            LGF_ACTION,
            crate::faction_abilities::ACTION_KIND,
            "Lazax Gate Folding: exhaust to place 1 infantry on Mecatol Rex",
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
    if option.id != LGF_ACTION {
        return false;
    }
    let Some(planet) = lgf_ready(context.state, context.content, context.sources, player) else {
        return false;
    };
    exhaust_technology(context.state, player, "lgf");
    let system = mecatol(context.state);
    crate::action_cards::place_units_counted(context, player, &system, Some(&planet), "infantry", 1)
        > 0
}

// -- Hegemonic Trade Policy ----------------------------------------------------------------------

fn htp_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(SystemId, PlanetId)> {
    if !technology_ready(state, player, "htp") {
        return Vec::new();
    }
    state
        .controlled_planets(player)
        .into_iter()
        .filter(|(_, planet)| {
            crate::production::planet_value_now(state, content, sources, planet, Spend::Resources)
                != crate::production::planet_value_now(
                    state,
                    content,
                    sources,
                    planet,
                    Spend::Influence,
                )
        })
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect()
}

/// "When 1 or more of your units use PRODUCTION": exhaust, swap one planet for this use.
fn htp(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:htp:PRODUCTION_USED:after"),
        seat.clone(),
        "PRODUCTION_USED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if event.text("player") != Some(owner.as_str()) {
                return Ok(());
            }
            let planets = htp_planets(context.state, context.content, context.sources, &owner);
            if planets.is_empty() {
                return Ok(());
            }
            let options: Vec<ChoiceOption> = planets
                .iter()
                .map(|(system, planet)| {
                    ChoiceOption::labelled(
                        planet.to_string(),
                        "planet",
                        format!("swap the values of {planet}"),
                    )
                    .with("system", system.to_string())
                })
                .collect();
            let choice = Choice::new(
                owner.clone(),
                "Hegemonic Trade Policy: swap which planet's values".to_owned(),
                options,
            )
            .contextualized(decision(context.state, &owner, "htp", "htp_planet"));
            let answer = context
                .ask_seeing(&choice)
                .map_err(TimingError::IllegalChoice)?;
            let Some((_, planet)) = planets.iter().find(|(_, p)| p.as_str() == answer.id) else {
                return Ok(());
            };
            exhaust_technology(context.state, &owner, "htp");
            crate::production::begin_value_swap(context.state, planet);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_seat.as_str())
            && !htp_planets(
                context.state,
                context.content,
                context.sources,
                &condition_seat,
            )
            .is_empty()
    }))
}

fn planet_spend_value(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    planet: &PlanetId,
    kind: Spend,
    value: i64,
) -> i64 {
    if !has_technology(state, player, "htp") {
        return value;
    }
    let owned = state
        .controlled_planets(player)
        .into_iter()
        .any(|(_, held)| held == planet);
    if !owned {
        return value;
    }
    crate::production::swapped_value(
        state,
        content,
        ti4_model::content_types::DEFAULT,
        planet,
        kind,
    )
    .unwrap_or(value)
}

// -- Salai Sai Corian ----------------------------------------------------------------------------

/// The flagship has 0 printed dice, so the count here is the whole of its roll.
fn unit_dice(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &CombatUnit<'_>,
    dice: i64,
) -> i64 {
    let own = unit.unit_type == "winnu_flagship" && is_winnu(state, unit.player);
    let lent = unit.unit_type != "winnu_flagship"
        && super::flagship_has_text(state, unit.player, unit.unit_type, "winnu_flagship");
    if !(own || lent) || unit.context != "space" {
        return dice;
    }
    let Some(system) = unit.system else {
        return dice;
    };
    let types = catalogue(content, sources);
    let theirs = state
        .system_state(system)
        .units
        .iter()
        .filter(|ship| &ship.owner != unit.player)
        .filter(|ship| {
            types
                .get(ship.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() && !kind.is_fighter())
        })
        .count();
    let theirs = i64::try_from(theirs).unwrap_or(0);
    if unit.unit_type == "winnu_flagship" {
        dice + theirs
    } else {
        // The Nekro flagship with the text: "rolls a number of dice equal to ..." replaces its
        // printed dice; the hit value stays its own printed stat.
        theirs
    }
}

// -- Acquiescence --------------------------------------------------------------------------------

fn secondary_waivers(
    state: &GameState,
    _content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    _card: &str,
) -> Vec<SecondaryWaiver> {
    let held = state.promissory_notes.get(ACQUIESCENCE) == Some(follower);
    if held && follower != primary && is_winnu(state, primary) {
        vec![SecondaryWaiver {
            id: "acq".to_owned(),
            label: "resolve the secondary using Acquiescence".to_owned(),
        }]
    } else {
        Vec::new()
    }
}

fn secondary_waived(
    state: &mut GameState,
    _content: &ContentStore,
    follower: &PlayerId,
    primary: &PlayerId,
    waiver: &str,
) {
    if waiver == "acq"
        && state.promissory_notes.get(ACQUIESCENCE) == Some(follower)
        && is_winnu(state, primary)
    {
        crate::promissory::give_back(state, ACQUIESCENCE);
    }
}

// -- Berekar Berekon -----------------------------------------------------------------------------

fn agent(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:winnuagent:PRODUCTION_USED:after"),
        seat.clone(),
        "PRODUCTION_USED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if leader_status(context.state, &owner, AGENT) == Some(LeaderStatus::Readied)
                && crate::leaders::exhaust(context.state, &owner, &LeaderId::new(AGENT))
            {
                // Read back by `ProductionWindow::refresh` right after this event.
                context.state.production_discount_remaining += 2;
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        leader_status(context.state, &condition_seat, AGENT) == Some(LeaderStatus::Readied)
    }))
}

/// Ssruu copies Berekar Berekon's production discount. Its holder controls the optional window;
/// the recipient's agent stays in the state it had when copied.
fn borrowed_agent(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    Ability::stateful(
        format!("leader:{owner_name}:{source}:yssarilagent:{AGENT}:PRODUCTION_USED:after"),
        borrower.clone(),
        "PRODUCTION_USED",
        Relation::After,
        Arc::new(move |_event, _, context| {
            if !has_borrowable_winnu_agent(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) {
                return Ok(());
            }
            if crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                // Read back by `ProductionWindow::refresh` immediately after this event.
                context.state.production_discount_remaining += 2;
                super::hooks_cards::borrowed_agent_used(
                    context,
                    &effect_borrower,
                    &LeaderId::new(AGENT),
                );
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        has_borrowable_winnu_agent(
            context.state,
            context.content,
            &condition_source,
            &condition_borrower,
        )
    }))
}

fn has_borrowable_winnu_agent(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == AGENT)
}

// -- Rickar Rickani ------------------------------------------------------------------------------

/// Control Mecatol Rex; the combat half of the unlock is `commander_unlock`.
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == COMMANDER).then(|| controls_mecatol(state, player))
}

/// "Enter into a combat in the Mecatol Rex system."
fn commander_unlock(owner_name: &str, seat: &PlayerId, event_type: &'static str) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("leader:{owner_name}:winnucommander_unlock:{event_type}:after"),
        seat.clone(),
        event_type,
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if leader_status(context.state, &owner, COMMANDER) == Some(LeaderStatus::Locked)
                && let Some(seat) = context.state.player_mut(&owner)
            {
                seat.leaders
                    .insert(LeaderId::new(COMMANDER), LeaderStatus::Unlocked);
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        leader_status(context.state, &condition_seat, COMMANDER) == Some(LeaderStatus::Locked)
            && event.text("system").is_some_and(crate::seating::is_mecatol)
            && (event.text("attacker") == Some(condition_seat.as_str())
                || event.text("defender") == Some(condition_seat.as_str()))
    }))
}

fn commander_bonus(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    if !crate::promissory::has_commander_ability(state, player, COMMANDER) {
        return 0;
    }
    let home = state
        .player(player)
        .and_then(|seat| seat.home_system.as_ref())
        == Some(system);
    let legendary =
        ti4_content::galaxy::system(content, system.as_str(), sources).is_some_and(|s| {
            s.planets().iter().any(|planet| {
                ti4_content::galaxy::planet(content, planet, sources)
                    .is_some_and(|planet| planet.is_legendary())
            })
        });
    if crate::seating::is_mecatol(system.as_str()) || home || legendary {
        2
    } else {
        0
    }
}

// -- Imperator -----------------------------------------------------------------------------------

fn support_in_play(state: &GameState, holder: &PlayerId) -> i64 {
    i64::try_from(
        state
            .support_holders
            .values()
            .filter(|who| *who == holder)
            .count(),
    )
    .unwrap_or(0)
}

/// The player this unit fights: another owner of ships (space) or ground forces (ground) there.
fn opponent_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> Option<PlayerId> {
    let system = unit.system?;
    let board = state.system_state(system);
    let types = catalogue(content, sources);
    let units: Vec<&ti4_model::units::Unit> = match unit.planet {
        Some(planet) => board.on_planet(planet).iter().collect(),
        None => board.units.iter().collect(),
    };
    units
        .into_iter()
        .filter(|other| &other.owner != unit.player)
        .find(|other| {
            types
                .get(other.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() || kind.is_ground_force())
        })
        .map(|other| other.owner.clone())
}

fn unit_roll_modifier(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> i64 {
    let Some(system) = unit.system else { return 0 };
    let mut shift = commander_bonus(state, content, sources, unit.player, system);
    if crate::breakthroughs::holds(state, unit.player, "winnubt")
        && let Some(opponent) = opponent_of(state, content, sources, unit)
    {
        shift += support_in_play(state, &opponent);
    }
    shift
}

fn imperator_key(player: &PlayerId) -> String {
    format!("private:{player}:winnu:imperator")
}

/// `"<origin>|<ship type>|<damaged>|<matching ships there when chosen>"`.
fn imperator_mark(state: &GameState, player: &PlayerId) -> Option<(SystemId, String, bool, usize)> {
    let mark = state.faction_marks.get(&imperator_key(player))?;
    let mut parts = mark.split('|');
    let origin = SystemId::new(parts.next()?);
    let kind = parts.next()?.to_owned();
    let damaged = parts.next()? == "1";
    let count = parts.next()?.parse().ok()?;
    Some((origin, kind, damaged, count))
}

fn matching_ships(
    state: &GameState,
    player: &PlayerId,
    system: &SystemId,
    kind: &str,
    damaged: bool,
) -> Vec<usize> {
    state
        .ships_of(player, system)
        .into_iter()
        .enumerate()
        .filter(|(_, ship)| ship.type_id.as_str() == kind && ship.sustained_damage == damaged)
        .map(|(index, _)| index)
        .collect()
}

/// The marked ship type, first of its kind at the origin: any ship of that type and damage stands
/// for the chosen one, so another ship leaving first does not lose the bonus.
fn move_bonus(state: &GameState, site: &MoveSite<'_>) -> i32 {
    let Some(index) = site.index else { return 0 };
    let Some((origin, kind, damaged, _)) = imperator_mark(state, site.player) else {
        return 0;
    };
    i32::from(
        origin == *site.origin
            && matching_ships(state, site.player, &origin, &kind, damaged).first() == Some(&index)
            && crate::breakthroughs::holds(state, site.player, "winnubt"),
    )
}

/// The bonus is spent when a ship of the marked type and damage leaves the origin.
fn imperator_consume(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let spent = |state: &GameState, who: &PlayerId| {
        imperator_mark(state, who).is_some_and(|(origin, kind, damaged, count)| {
            matching_ships(state, who, &origin, &kind, damaged).len() < count
        })
    };
    Ability::stateful(
        format!("breakthrough:{owner_name}:winnubt_consume:SHIP_MOVED:after"),
        seat.clone(),
        "SHIP_MOVED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&imperator_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(condition_seat.as_str())
            && spent(context.state, &condition_seat)
    }))
}

fn has_legendary(content: &ContentStore, sources: SourceSet, system: &SystemId) -> bool {
    ti4_content::galaxy::system(content, system.as_str(), sources).is_some_and(|s| {
        s.planets().iter().any(|planet| {
            ti4_content::galaxy::planet(content, planet, sources)
                .is_some_and(|planet| planet.is_legendary())
        })
    })
}

fn imperator_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(SystemId, usize, String, bool)> {
    let types = catalogue(content, sources);
    let mut found: Vec<(SystemId, usize, String, bool)> = Vec::new();
    for system in state.board.keys() {
        for (index, ship) in state.ships_of(player, system).into_iter().enumerate() {
            if !types
                .get(ship.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ship)
            {
                continue;
            }
            let kind = ship.type_id.to_string();
            let damaged = ship.sustained_damage;
            if !found
                .iter()
                .any(|(s, _, k, d)| s == system && *k == kind && *d == damaged)
            {
                found.push((system.clone(), index, kind, damaged));
            }
        }
    }
    found
}

/// "After you activate a system that contains a legendary planet, apply +1 to the move value of
/// 1 of your ships during this tactical action."
fn imperator(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    let ready = |context: &TimingContext<'_>, event: &crate::event::Event, who: &PlayerId| {
        crate::breakthroughs::holds(context.state, who, "winnubt")
            && event.text("player") == Some(who.as_str())
            && event.text("system").is_some_and(|system| {
                has_legendary(context.content, context.sources, &SystemId::new(system))
            })
            && !imperator_ships(context.state, context.content, context.sources, who).is_empty()
    };
    Ability::stateful(
        format!("breakthrough:{owner_name}:winnubt:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            if !ready(context, event, &owner) {
                return Ok(());
            }
            let ships = imperator_ships(context.state, context.content, context.sources, &owner);
            let options: Vec<ChoiceOption> = ships
                .iter()
                .enumerate()
                .map(|(n, (system, _, kind, damaged))| {
                    ChoiceOption::labelled(
                        format!("ship|{n}"),
                        "ship",
                        format!(
                            "+1 move for {kind}{} in {system}",
                            if *damaged { " (damaged)" } else { "" }
                        ),
                    )
                })
                .collect();
            let choice = Choice::new(
                owner.clone(),
                "Imperator: which ship gets +1 move".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                &owner,
                "winnubt",
                "imperator_ship",
            ));
            let answer = context
                .ask_seeing(&choice)
                .map_err(TimingError::IllegalChoice)?;
            let Some(picked) = choice.options.iter().position(|o| o.id == answer.id) else {
                return Ok(());
            };
            let (system, _, kind, damaged) = &ships[picked];
            let count = matching_ships(context.state, &owner, system, kind, *damaged).len();
            context.state.faction_marks.insert(
                imperator_key(&owner),
                format!("{system}|{kind}|{}|{count}", u8::from(*damaged)),
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        ready(context, event, &condition_seat)
    }))
}

fn imperator_clear(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_seat) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("breakthrough:{owner_name}:winnubt_clear:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&imperator_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&imperator_key(&condition_seat))
    }))
}

// -- Mathis Mathinus -----------------------------------------------------------------------------

/// Every strategy card in the game (held by a seat or still unclaimed).
fn hero_cards(state: &GameState) -> Vec<StrategyCardId> {
    let mut cards: Vec<StrategyCardId> = state
        .players
        .iter()
        .flat_map(|seat| seat.strategy_cards.iter().cloned())
        .chain(state.unclaimed_strategy_cards.iter().cloned())
        .collect();
    cards.sort();
    cards.dedup();
    cards
}

fn leader_action(
    state: &GameState,
    _content: &ContentStore,
    _player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "winnuhero")
        .then(|| is_winnu(state, _player) && !hero_cards(state).is_empty())
}

/// Resolve the hero's chosen primary and return its continuation to the `Game` driver.
/// Warfare returns `FreeTactical`; the driver opens that action and resumes with followers later.
fn use_leader_strategy_primary(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<Option<(StrategyCardId, crate::strategy_cards::Ability)>, TimingError> {
    if leader.as_str() != "winnuhero" || !is_winnu(context.state, player) {
        return Ok(None);
    }
    let cards = hero_cards(context.state);
    if cards.is_empty() {
        return Ok(None);
    }
    let options = cards
        .iter()
        .map(|card| {
            ChoiceOption::labelled(
                card.to_string(),
                "strategy_card",
                crate::draft::strategy_card_label(context.content, card.as_str()),
            )
        })
        .collect();
    let choice = Choice::new(
        player.clone(),
        "Mathis Mathinus: which strategy card".to_owned(),
        options,
    )
    .contextualized(decision(context.state, player, "winnuhero", "hero_card"));
    let answer = context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)?;
    let card = cards
        .into_iter()
        .find(|card| card.as_str() == answer.id)
        .expect("ask_seeing validates the selected strategy card");
    let ability = crate::strategy_cards::primary(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
        card.as_str(),
    )
    .map_err(TimingError::IllegalChoice)?;
    Ok(Some((card, ability)))
}

/// Choices offered after Mathis's primary completes, driven one answer per Game step.
fn leader_strategy_follower_choices(
    state: &GameState,
    player: &PlayerId,
    leader: &LeaderId,
    _card: &StrategyCardId,
) -> Option<Vec<Choice>> {
    if leader.as_str() != "winnuhero" || !is_winnu(state, player) {
        return None;
    }
    let order = &state.seating_order;
    let start = order.iter().position(|who| who == player)?;
    Some(
        (1..order.len())
            .map(|step| {
                let other = &order[(start + step) % order.len()];
                Choice::new(
                    player.clone(),
                    format!("Mathis Mathinus: may {other} follow the secondary"),
                    vec![
                        ChoiceOption::labelled(
                            format!("yes|{other}"),
                            "follower",
                            format!("let {other} follow"),
                        ),
                        ChoiceOption::labelled(
                            format!("no|{other}"),
                            "follower",
                            format!("not {other}"),
                        ),
                    ],
                )
                .contextualized(decision(
                    state,
                    player,
                    "winnuhero",
                    "hero_follower",
                ))
            })
            .collect(),
    )
}

#[allow(
    clippy::too_many_lines,
    reason = "one leader effect: ask, resolve, then follow, in order"
)]
fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "winnuhero" {
        return None;
    }
    let cards = hero_cards(context.state);
    if cards.is_empty() {
        return Some(false);
    }
    // Every question is asked before the primary is resolved.
    let options: Vec<ChoiceOption> = cards
        .iter()
        .map(|card| {
            ChoiceOption::labelled(
                card.to_string(),
                "strategy_card",
                crate::draft::strategy_card_label(context.content, card.as_str()),
            )
        })
        .collect();
    let choice = Choice::new(
        player.clone(),
        "Mathis Mathinus: which strategy card".to_owned(),
        options,
    )
    .contextualized(decision(context.state, player, "winnuhero", "hero_card"));
    let answer = context.ask_seeing(&choice).ok()?;
    let card = cards
        .iter()
        .find(|card| card.as_str() == answer.id)?
        .clone();
    // Resolve the primary first; anything but a complete resolution is rolled back and refused.
    let snapshot = context.state.clone();
    let outcome = crate::strategy_cards::primary(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
        card.as_str(),
    );
    if !matches!(outcome, Ok(crate::strategy_cards::Ability::Resolved)) {
        *context.state = snapshot;
        return Some(false);
    }
    // "Then, choose any number of other players."
    let mut followers = Vec::new();
    let order = context.state.seating_order.clone();
    let start = order.iter().position(|who| who == player).unwrap_or(0);
    for step in 1..order.len() {
        let other = order[(start + step) % order.len()].clone();
        let ask = Choice::new(
            player.clone(),
            format!("Mathis Mathinus: may {other} follow the secondary"),
            vec![
                ChoiceOption::labelled(
                    format!("yes|{other}"),
                    "follower",
                    format!("let {other} follow"),
                ),
                ChoiceOption::labelled(format!("no|{other}"), "follower", format!("not {other}")),
            ],
        )
        .contextualized(decision(
            context.state,
            player,
            "winnuhero",
            "hero_follower",
        ));
        let Ok(picked) = context.ask_seeing(&ask) else {
            return Some(true); // the primary stands; no followers were chosen
        };
        if picked.id.starts_with("yes|") {
            followers.push(other);
        }
    }
    let mut window =
        crate::strategy::StrategySecondaryWindow::foreign(player.clone(), card.clone(), followers);
    while let Some(pending) = window.next_choice(context.state, context.content, context.sources) {
        let follower = pending.player.clone();
        let Ok(answer) = context.ask_seeing(&pending) else {
            break;
        };
        // The token and the secondary are one unit: undone together if the secondary fails.
        let before = context.state.clone();
        let Ok(resolution) =
            window.take_choice(context.state, context.content, context.sources, answer)
        else {
            *context.state = before;
            break;
        };
        if resolution == crate::strategy::SecondaryResolution::Followed
            && crate::strategy_cards::secondary(
                context.state,
                context.content,
                context.sources,
                context.galaxy,
                context.table,
                &follower,
                card.as_str(),
            )
            .is_err()
        {
            *context.state = before;
            break;
        }
    }
    Some(true)
}

// -- timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        gained_mark(owner_name, seat),
        gained_clear(owner_name, seat),
        reclamation(owner_name, seat),
        reclaimer(owner_name, seat),
        htp(owner_name, seat),
        agent(owner_name, seat),
        commander_unlock(owner_name, seat, "SPACE_COMBAT_STARTED"),
        commander_unlock(owner_name, seat, "GROUND_COMBAT_STARTED"),
        imperator(owner_name, seat),
        imperator_consume(owner_name, seat),
        tactical_mark(owner_name, seat),
        imperator_clear(owner_name, seat),
    ];
    // Register from the static card roster rather than current readiness: the live condition
    // below sees agents readied or exhausted after resolver construction.
    if state
        .player(seat)
        .is_some_and(|source| source.leaders.contains_key(&LeaderId::new(AGENT)))
    {
        for candidate in &state.players {
            if &candidate.id != seat
                && candidate
                    .leaders
                    .contains_key(&LeaderId::new("yssarilagent"))
            {
                abilities.push(borrowed_agent(owner_name, seat, &candidate.id));
            }
        }
    }
    abilities
}

#[cfg(test)]
mod tests {
    fn leader_strategy_followers(
        context: &mut TimingContext<'_>,
        player: &PlayerId,
        leader: &LeaderId,
        card: &StrategyCardId,
    ) -> Result<Option<Vec<PlayerId>>, TimingError> {
        let Some(choices) =
            super::leader_strategy_follower_choices(context.state, player, leader, card)
        else {
            return Ok(None);
        };
        let mut followers = Vec::new();
        for choice in choices {
            let answer = context
                .ask_seeing(&choice)
                .map_err(TimingError::IllegalChoice)?;
            if let Some(player) = answer.id.strip_prefix("yes|") {
                followers.push(PlayerId::new(player));
            }
        }
        Ok(Some(followers))
    }

    use super::*;
    use ti4_model::id::TechnologyId;

    /// These fixtures have no map, so Mecatol Rex is the base tile.
    fn mecatol() -> SystemId {
        SystemId::new(crate::seating::MECATOL)
    }
    use crate::choice::Window;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }
    fn ssruu_game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "yssaril"), ("c", "sol")], DEFAULT)
    }

    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
            .collect()
    }

    fn emit(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, payload(pairs))
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
    }

    fn scripted(answers: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::new(
            answers.iter().map(|s| (*s).to_owned()),
        )))
    }

    #[derive(Debug)]
    struct BorrowWinnuAgent;
    impl crate::choice::Decider for BorrowWinnuAgent {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            if choice.player == a()
                && choice
                    .options
                    .iter()
                    .any(|option| option.id == AGENT_ABILITY)
            {
                return choice
                    .options
                    .iter()
                    .find(|option| option.is_decline())
                    .cloned()
                    .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                    });
            }
            if let Some(option) = choice.options.iter().find(|option| {
                option
                    .id
                    .contains(":yssarilagent:winnuagent:PRODUCTION_USED:after")
            }) {
                assert_eq!(choice.player, b(), "copied ability is controlled by Ssruu");
                return Ok(option.clone());
            }
            choice
                .options
                .iter()
                .find(|option| option.is_decline())
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }

    fn borrow_winnu_agent_table() -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(BorrowWinnuAgent))
    }

    fn count_on(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        who: &PlayerId,
        base: &str,
    ) -> usize {
        let types = catalogue(ContentStore::embedded(), DEFAULT);
        state
            .system_state(system)
            .on_planet_of(planet, who)
            .into_iter()
            .filter(|u| {
                types
                    .get(u.type_id.as_str())
                    .is_some_and(|k| k.base_type() == base)
            })
            .count()
    }

    fn take_mecatol(state: &mut GameState, who: &PlayerId) -> PlanetId {
        let planet = PlanetId::new("mr");
        state
            .system_mut(&mecatol())
            .planet_control
            .insert(planet.clone(), who.clone());
        planet
    }

    fn home_of(state: &GameState, who: &PlayerId) -> SystemId {
        state.player(who).unwrap().home_system.clone().unwrap()
    }

    fn unit(kind: &str, who: &PlayerId) -> ti4_model::units::Unit {
        ti4_model::units::Unit::new(ti4_model::id::UnitTypeId::new(kind), who.clone())
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_game_without_winnu_is_offered_nothing() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let content = ContentStore::embedded();
        let planet = take_mecatol(&mut state, &a());
        assert!(component_actions(&state, content, &a()).is_empty());
        assert!(!custodians_free(&state, content, &a()));
        let _ = planet;
        // The control of Mecatol, an action completing and a production use offer no Winnu window.
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[("player", "a"), ("system", "18"), ("planet", "mr")],
        );
        assert!(!state.faction_marks.keys().any(|k| k.contains("winnu")));
        let before = state.clone();
        emit(
            &mut state,
            &mut scripted(&[]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        emit(
            &mut state,
            &mut scripted(&[]),
            "PRODUCTION_USED",
            &[("player", "a"), ("system", "18")],
        );
        assert_eq!(
            state.production_discount_remaining,
            before.production_discount_remaining
        );
        assert_eq!(state.board, before.board);
        assert!(secondary_waivers(&state, content, &a(), &b(), "Trade").is_empty());
    }

    // -- Blood Ties ------------------------------------------------------------------------------

    #[test]
    fn blood_ties_makes_the_custodians_free_for_winnu_only() {
        let state = game();
        let content = ContentStore::embedded();
        assert_eq!(crate::invasion::custodians_cost(&state, content, &a()), 0);
        assert_eq!(
            crate::invasion::custodians_cost(&state, content, &b()),
            crate::invasion::CUSTODIANS_COST
        );
    }

    // -- Reclamation and Reclaimer ---------------------------------------------------------------

    const RECLAMATION: &str = "ability:winnu:reclamation:ACTION_COMPLETED:after";
    const RECLAIMER: &str = "unit:winnu:winnu_mech:ACTION_COMPLETED:after";

    fn activate(state: &mut GameState) {
        state.active = Some(a());
        emit(
            state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", "19")],
        );
    }

    fn gain_mecatol(state: &mut GameState) {
        activate(state);
        take_mecatol(state, &a());
        emit(
            state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[("player", "a"), ("system", "18"), ("planet", "mr")],
        );
    }

    #[test]
    fn reclamation_places_a_pds_and_a_space_dock_after_taking_mecatol() {
        let mut state = game();
        gain_mecatol(&mut state);
        let planet = PlanetId::new("mr");
        emit(
            &mut state,
            &mut scripted(&[RECLAMATION]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(count_on(&state, &mecatol(), &planet, &a(), "pds"), 1);
        assert_eq!(count_on(&state, &mecatol(), &planet, &a(), "spacedock"), 1);
    }

    #[test]
    fn reclamation_needs_control_gained_this_action_and_may_be_declined() {
        let planet = PlanetId::new("mr");
        // Mecatol held but not gained during an action: nothing is offered.
        let mut state = game();
        take_mecatol(&mut state, &a());
        emit(
            &mut state,
            &mut scripted(&[RECLAMATION]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(count_on(&state, &mecatol(), &planet, &a(), "pds"), 0);
        // Gained, then declined.
        let mut state = game();
        gain_mecatol(&mut state);
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(count_on(&state, &mecatol(), &planet, &a(), "pds"), 0);
        // The mark does not survive into the next turn.
        let mut state = game();
        gain_mecatol(&mut state);
        emit(
            &mut state,
            &mut scripted(&[]),
            "TURN_BEGAN",
            &[("player", "b")],
        );
        assert!(gained(&state, &a()).is_empty());
    }

    #[test]
    fn reclamation_is_atomic_when_the_box_lacks_a_space_dock() {
        let mut state = game();
        let home = home_of(&state, &a());
        let hp = state.controlled_planets(&a())[0].1.clone();
        // Fill every space dock model: 3 in the box in all.
        crate::fixtures::put_on_planet(&mut state, &home, &hp, "winnu_spacedock", &a(), 0);
        for _ in 0..3 {
            crate::fixtures::put_on_planet(&mut state, &home, &hp, "spacedock", &a(), 1);
        }
        gain_mecatol(&mut state);
        let planet = PlanetId::new("mr");
        emit(
            &mut state,
            &mut scripted(&[RECLAMATION]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(
            count_on(&state, &mecatol(), &planet, &a(), "pds"),
            0,
            "all or nothing"
        );
    }

    #[test]
    fn the_reclaimer_places_one_structure_on_the_planet_it_stands_on() {
        let mut state = game();
        let (system, planet) = crate::fixtures::a_placed_planet();
        activate(&mut state);
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "winnu_mech", &a(), 1);
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("system", system.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        let pick = format!("spacedock|{system}|{planet}");
        emit(
            &mut state,
            &mut scripted(&[RECLAIMER, &pick]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(count_on(&state, &system, &planet, &a(), "spacedock"), 1);
        assert_eq!(
            count_on(&state, &system, &planet, &a(), "pds"),
            0,
            "one structure"
        );
    }

    #[test]
    fn the_reclaimer_needs_the_mech_on_the_planet() {
        let mut state = game();
        let (system, planet) = crate::fixtures::a_placed_planet();
        state.active = Some(a());
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), a());
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("system", system.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        assert!(reclaimer_options(&state, ContentStore::embedded(), DEFAULT, &a()).is_empty());
    }

    // -- Lazax Gate Folding ----------------------------------------------------------------------

    fn alpha_galaxy() -> ti4_content::galaxy::Galaxy {
        ti4_content::galaxy::Galaxy::build(
            ContentStore::embedded(),
            &["18", "39", "19"],
            DEFAULT,
            1,
        )
        .expect("galaxy")
    }

    #[test]
    fn gate_folding_links_mecatol_to_wormhole_systems_only_while_not_holding_it() {
        let mut state = game();
        let galaxy = alpha_galaxy();
        let content = ContentStore::embedded();
        let links = |state: &GameState| linked_systems(state, content, DEFAULT, &galaxy, &a());
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("lgf"));
        assert!(links(&state).is_empty(), "no tactical action under way");
        state.active = Some(a());
        state.active_system = Some(SystemId::new("39"));
        assert_eq!(links(&state), vec![("18".to_owned(), "39".to_owned())]);
        // Through the player-aware adjacency helper the engine uses.
        let adjacency =
            crate::movement::PlayerAdjacency::new(&state, content, DEFAULT, &galaxy, &a());
        assert!(adjacency.are_adjacent("18", "39"));
        let other = crate::movement::PlayerAdjacency::new(&state, content, DEFAULT, &galaxy, &b());
        assert!(!other.are_adjacent("18", "39") || galaxy.adjacent("18").contains("39"));
        take_mecatol(&mut state, &a());
        assert!(links(&state).is_empty(), "holding Mecatol Rex");
    }

    #[test]
    fn gate_folding_needs_the_technology() {
        let mut state = game();
        state.active = Some(a());
        state.active_system = Some(SystemId::new("39"));
        let galaxy = alpha_galaxy();
        assert!(
            linked_systems(&state, ContentStore::embedded(), DEFAULT, &galaxy, &a()).is_empty()
        );
    }

    #[test]
    fn gate_folding_action_places_an_infantry_and_exhausts() {
        let mut state = game();
        let content = ContentStore::embedded();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("lgf"));
        assert!(
            component_actions(&state, content, &a()).is_empty(),
            "no Mecatol Rex"
        );
        let planet = take_mecatol(&mut state, &a());
        let options = component_actions(&state, content, &a());
        assert_eq!(options.len(), 1);
        let done =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut scripted(&[]), |ctx| {
                perform_component(ctx, &a(), &options[0])
            });
        assert!(done);
        assert_eq!(count_on(&state, &mecatol(), &planet, &a(), "infantry"), 1);
        assert!(!technology_ready(&state, &a(), "lgf"));
        assert!(
            component_actions(&state, content, &a()).is_empty(),
            "exhausted"
        );
    }

    // -- Hegemonic Trade Policy ------------------------------------------------------------------

    #[test]
    fn trade_policy_swaps_one_planet_for_one_use_and_exhausts() {
        let mut state = game();
        let content = ContentStore::embedded();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("htp"));
        let (system, planet) = state
            .controlled_planets(&a())
            .into_iter()
            .map(|(s, p)| (s.clone(), p.clone()))
            .find(|(_, p)| {
                crate::production::planet_value_now(&state, content, DEFAULT, p, Spend::Resources)
                    != crate::production::planet_value_now(
                        &state,
                        content,
                        DEFAULT,
                        p,
                        Spend::Influence,
                    )
            })
            .expect("a home planet with unequal values");
        let res = crate::production::planet_value_now(
            &state,
            content,
            DEFAULT,
            &planet,
            Spend::Resources,
        );
        let inf = crate::production::planet_value_now(
            &state,
            content,
            DEFAULT,
            &planet,
            Spend::Influence,
        );
        state.production_seq += 1;
        emit(
            &mut state,
            &mut scripted(&[
                "technology:winnu:htp:PRODUCTION_USED:after",
                planet.as_str(),
            ]),
            "PRODUCTION_USED",
            &[("player", "a"), ("system", system.as_str())],
        );
        assert!(!technology_ready(&state, &a(), "htp"));
        assert_eq!(
            planet_spend_value(&state, content, &a(), &planet, Spend::Resources, res),
            inf
        );
        assert_eq!(
            planet_spend_value(&state, content, &a(), &planet, Spend::Influence, inf),
            res
        );
        assert_eq!(
            planet_spend_value(&state, content, &b(), &planet, Spend::Resources, res),
            res,
            "not its controller"
        );
        // The next use of PRODUCTION no longer sees it.
        state.production_seq += 1;
        assert_eq!(
            planet_spend_value(&state, content, &a(), &planet, Spend::Resources, res),
            res
        );
    }

    #[test]
    fn trade_policy_is_not_offered_exhausted_or_for_another_players_production() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new("htp"));
        let home = home_of(&state, &b());
        state.production_seq += 1;
        emit(
            &mut state,
            &mut scripted(&["decline"]), // only the agent window is offered
            "PRODUCTION_USED",
            &[("player", "b"), ("system", home.as_str())],
        );
        assert!(technology_ready(&state, &a(), "htp"));
        assert!(crate::production::swapped_planet(&state).is_none());
        exhaust_technology(&mut state, &a(), "htp");
        assert!(htp_planets(&state, ContentStore::embedded(), DEFAULT, &a()).is_empty());
    }

    // -- Salai Sai Corian ------------------------------------------------------------------------

    #[test]
    fn the_flagship_rolls_a_die_per_opposing_non_fighter_ship() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("19");
        state.system_mut(&system).units.clear();
        crate::fixtures::put(&mut state, &system, "winnu_flagship", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 2);
        crate::fixtures::put(&mut state, &system, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &system, "fighter", &b(), 4);
        crate::fixtures::put(&mut state, &system, "carrier", &a(), 1);
        let dice = |who: &PlayerId, kind: &str, base: i64| {
            crate::factions::unit_dice(
                &state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: who,
                    system: Some(&system),
                    planet: None,
                    unit_type: kind,
                    context: "space",
                },
                base,
            )
        };
        assert_eq!(
            dice(&a(), "winnu_flagship", 0),
            3,
            "fighters and its own ships do not count"
        );
        assert_eq!(dice(&a(), "carrier", 1), 1, "no other unit changes");
        assert_eq!(
            dice(&b(), "winnu_flagship", 0),
            0,
            "only the Winnu player's flagship"
        );
    }

    // -- Acquiescence ----------------------------------------------------------------------------

    #[test]
    fn acquiescence_is_offered_to_its_holder_and_returns_when_used() {
        let mut state = game();
        let content = ContentStore::embedded();
        assert!(
            secondary_waivers(&state, content, &b(), &a(), "Trade").is_empty(),
            "not held"
        );
        state.promissory_notes.insert(ACQUIESCENCE.to_owned(), b());
        assert_eq!(
            secondary_waivers(&state, content, &b(), &a(), "Trade").len(),
            1
        );
        assert!(
            secondary_waivers(&state, content, &a(), &b(), "Trade").is_empty(),
            "wrong primary"
        );
        secondary_waived(&mut state, content, &b(), &a(), "acq");
        assert_eq!(state.promissory_notes.get(ACQUIESCENCE), Some(&a()));
        assert!(secondary_waivers(&state, content, &b(), &a(), "Trade").is_empty());
    }

    // -- Berekar Berekon -------------------------------------------------------------------------

    const AGENT_ABILITY: &str = "leader:winnu:winnuagent:PRODUCTION_USED:after";

    #[test]
    fn the_agent_cuts_any_players_production_cost_by_two() {
        let mut state = game();
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        state.production_discount_remaining = 1;
        emit(
            &mut state,
            &mut scripted(&[AGENT_ABILITY]),
            "PRODUCTION_USED",
            &[("player", "b"), ("system", "18")],
        );
        assert_eq!(state.production_discount_remaining, 3);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn the_agent_may_be_declined_and_needs_to_be_ready() {
        let mut state = game();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "PRODUCTION_USED",
            &[("player", "a"), ("system", "18")],
        );
        assert_eq!(state.production_discount_remaining, 0);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        crate::leaders::exhaust(&mut state, &a(), &LeaderId::new(AGENT));
        emit(
            &mut state,
            &mut scripted(&[AGENT_ABILITY]),
            "PRODUCTION_USED",
            &[("player", "a"), ("system", "18")],
        );
        assert_eq!(state.production_discount_remaining, 0);
    }

    #[test]
    fn ssruu_copies_winnu_production_discount_for_readied_or_exhausted_source() {
        let content = ContentStore::embedded();
        let borrower = b();
        let target = PlayerId::new("c");
        for source_status in [LeaderStatus::Readied, LeaderStatus::Exhausted] {
            let mut state = ssruu_game();
            state
                .player_mut(&a())
                .unwrap()
                .leaders
                .insert(LeaderId::new(AGENT), source_status);
            state
                .player_mut(&borrower)
                .unwrap()
                .leaders
                .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
            let (system, planet) = crate::fixtures::a_placed_planet();
            state
                .system_mut(&system)
                .set_control(planet.clone(), target.clone());
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &target, 1);
            state.player_mut(&target).unwrap().trade_goods = 0;
            state.exhausted_planets.extend(
                state
                    .controlled_planets(&target)
                    .into_iter()
                    .map(|(_, planet)| planet.clone())
                    .collect::<Vec<_>>(),
            );

            emit(
                &mut state,
                &mut borrow_winnu_agent_table(),
                "PRODUCTION_USED",
                &[("player", "c"), ("system", system.as_str())],
            );
            assert_eq!(state.production_discount_remaining, 2);
            assert_eq!(
                leader_status(&state, &borrower, "yssarilagent"),
                Some(LeaderStatus::Exhausted)
            );
            assert_eq!(
                leader_status(&state, &a(), AGENT),
                Some(source_status),
                "native optional branch is declined for a ready source"
            );

            let mut window = crate::production::ProductionWindow::new(
                &state, content, DEFAULT, &target, &system,
            );
            window.refresh(&state, content, DEFAULT);
            let offer = window
                .pending_choice(&state, content, DEFAULT)
                .expect("the spacedock offers production");
            let cruiser = offer
                .options
                .iter()
                .find(|option| option.id == "build|cruiser|1")
                .expect("the combined discount makes a zero-resource cruiser affordable")
                .clone();
            assert_eq!(
                cruiser.payload.get("cost").and_then(|value| value.as_i64()),
                Some(0)
            );
            assert_eq!(
                cruiser
                    .payload
                    .get("discount")
                    .and_then(|value| value.as_i64()),
                Some(2)
            );

            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(31);
            let mut table = crate::choice::Table::default();
            let mut resolving = crate::choice::Resolving {
                content,
                sources: DEFAULT,
                dice: &mut dice,
                rng: &mut rng,
                table: &mut table,
                timing: None,
            };
            crate::choice::Window::resolve(&mut window, &mut state, &mut resolving, cruiser)
                .expect("the discounted cruiser places without a payment choice");
            assert_eq!(state.player(&target).unwrap().trade_goods, 0);
            assert!(
                state
                    .system_state(&system)
                    .units
                    .iter()
                    .any(|unit| { unit.owner == target && unit.type_id.as_str() == "cruiser" })
            );
        }
    }

    #[test]
    fn borrowed_agent_readiness_is_live_and_registration_uses_the_static_roster() {
        let mut state = ssruu_game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Exhausted);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Exhausted);
        let mut resolver = crate::fixtures::armed_resolver(&state);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(AGENT), LeaderStatus::Readied);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        let mut table = borrow_winnu_agent_table();
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            let event = context
                .event_sequence
                .next(
                    "PRODUCTION_USED",
                    payload(&[("player", "c"), ("system", "18")]),
                )
                .expect("an event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("the live copy resolves");
        });
        assert_eq!(state.production_discount_remaining, 2);
        assert_eq!(
            leader_status(&state, &a(), AGENT),
            Some(LeaderStatus::Readied)
        );
        assert_eq!(
            leader_status(&state, &b(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
    }

    #[test]
    fn borrowed_agent_is_inert_without_ready_ssruu_or_an_available_source() {
        for (source_status, borrower_status, remove_source) in [
            (Some(LeaderStatus::Readied), None, false),
            (
                Some(LeaderStatus::Readied),
                Some(LeaderStatus::Exhausted),
                false,
            ),
            (
                Some(LeaderStatus::Locked),
                Some(LeaderStatus::Readied),
                false,
            ),
            (None, Some(LeaderStatus::Readied), true),
        ] {
            let mut state = ssruu_game();
            if remove_source {
                state
                    .player_mut(&a())
                    .unwrap()
                    .leaders
                    .remove(&LeaderId::new(AGENT));
            } else if let Some(status) = source_status {
                state
                    .player_mut(&a())
                    .unwrap()
                    .leaders
                    .insert(LeaderId::new(AGENT), status);
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
            emit(
                &mut state,
                &mut crate::choice::Table::with_default(Box::new(crate::choice::AlwaysDecline)),
                "PRODUCTION_USED",
                &[("player", "c"), ("system", "18")],
            );
            assert_eq!(state.production_discount_remaining, 0);
            assert_eq!(leader_status(&state, &a(), AGENT), source_status);
            assert_eq!(leader_status(&state, &b(), "yssarilagent"), borrower_status);
        }
    }

    // -- Rickar Rickani --------------------------------------------------------------------------

    fn unlock(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new(COMMANDER), LeaderStatus::Unlocked);
    }

    #[test]
    fn the_commander_unlocks_by_holding_mecatol_or_fighting_there() {
        let content = ContentStore::embedded();
        let mut state = game();
        let leader = LeaderId::new(COMMANDER);
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &a(), &leader),
            Some(false)
        );
        take_mecatol(&mut state, &a());
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &a(), &leader),
            Some(true)
        );
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &a(), &LeaderId::new("x")),
            None
        );

        let mut state = game();
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Locked)
        );
        emit(
            &mut state,
            &mut scripted(&[]),
            "SPACE_COMBAT_STARTED",
            &[("system", "19"), ("attacker", "a"), ("defender", "b")],
        );
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Locked),
            "elsewhere"
        );
        emit(
            &mut state,
            &mut scripted(&[]),
            "SPACE_COMBAT_STARTED",
            &[("system", "18"), ("attacker", "b"), ("defender", "a")],
        );
        assert_eq!(
            leader_status(&state, &a(), COMMANDER),
            Some(LeaderStatus::Unlocked)
        );
    }

    #[test]
    fn the_commander_adds_two_in_mecatol_home_and_legendary_systems() {
        let mut state = game();
        let content = ContentStore::embedded();
        let shift = |state: &GameState, who: &PlayerId, system: &str| {
            commander_bonus(state, content, DEFAULT, who, &SystemId::new(system))
        };
        let home = home_of(&state, &a());
        assert_eq!(shift(&state, &a(), "18"), 0, "locked");
        unlock(&mut state);
        assert_eq!(shift(&state, &a(), "18"), 2);
        assert_eq!(shift(&state, &a(), home.as_str()), 2);
        assert_eq!(shift(&state, &a(), "19"), 0, "an ordinary system");
        assert_eq!(shift(&state, &b(), "18"), 0, "only its owner");
        let legendary = ti4_content::galaxy::all_systems(content, DEFAULT)
            .into_iter()
            .find(|(id, _)| has_legendary(content, DEFAULT, &SystemId::new(*id)))
            .map(|(id, _)| id.to_owned())
            .expect("a legendary system");
        assert_eq!(shift(&state, &a(), &legendary), 2);
    }

    #[test]
    fn an_alliance_commander_grant_uses_the_recipient_home_system() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let content = ContentStore::embedded();
        let home = home_of(&state, &a());
        assert!(
            state
                .players
                .iter()
                .all(|seat| seat.faction.as_str() != FACTION)
        );
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            content,
            &a(),
            COMMANDER,
        ));
        assert_eq!(
            commander_bonus(&state, content, DEFAULT, &a(), &home),
            2,
            "Alliance conveys the Winnu ability, applied to the recipient's home system"
        );
        assert_eq!(
            commander_bonus(&state, content, DEFAULT, &b(), &home),
            0,
            "the grant is held by a, not b"
        );
    }

    // -- Imperator -------------------------------------------------------------------------------

    fn grant_bt(state: &mut GameState) {
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("winnubt"));
    }

    #[test]
    fn the_breakthrough_adds_one_per_support_in_the_opponents_play_area() {
        let mut state = game();
        let content = ContentStore::embedded();
        let system = SystemId::new("19");
        state.system_mut(&system).units.clear();
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        let shift = |state: &GameState| {
            unit_roll_modifier(
                state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: &a(),
                    system: Some(&system),
                    planet: None,
                    unit_type: "cruiser",
                    context: "space",
                },
            )
        };
        grant_bt(&mut state);
        state.support_holders.insert(PlayerId::new("c"), b());
        state.support_holders.insert(PlayerId::new("d"), b());
        state.support_holders.insert(PlayerId::new("e"), a());
        assert_eq!(shift(&state), 2, "only those in the opponent's area");
        let mut plain = game();
        plain.support_holders.insert(PlayerId::new("c"), b());
        plain.system_mut(&system).units = state.system_mut(&system).units.clone();
        assert_eq!(shift(&plain), 0, "without the breakthrough");
    }

    #[test]
    fn the_breakthrough_gives_one_ship_plus_one_move_after_a_legendary_activation() {
        let mut state = game();
        let content = ContentStore::embedded();
        let legendary = ti4_content::galaxy::all_systems(content, DEFAULT)
            .into_iter()
            .find(|(id, _)| has_legendary(content, DEFAULT, &SystemId::new(*id)))
            .map(|(id, _)| id.to_owned())
            .expect("a legendary system");
        let home = home_of(&state, &a());
        state.system_mut(&home).units.clear();
        crate::fixtures::put(&mut state, &home, "carrier", &a(), 2);
        grant_bt(&mut state);
        emit(
            &mut state,
            &mut scripted(&[
                "breakthrough:winnu:winnubt:SYSTEM_ACTIVATED:after",
                "ship|0",
            ]),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", legendary.as_str())],
        );
        let kind = ti4_content::units::unit_type(content, "carrier", DEFAULT).unwrap();
        let bonus = |index| {
            move_bonus(
                &state,
                &MoveSite {
                    player: &a(),
                    origin: &home,
                    index: Some(index),
                    ship: &kind,
                },
            )
        };
        assert_eq!(bonus(0), 1);
        assert_eq!(bonus(1), 0, "one ship only");
        // A not-legendary activation offers nothing.
        let mut other = game();
        grant_bt(&mut other);
        other.system_mut(&home).units.clear();
        crate::fixtures::put(&mut other, &home, "carrier", &a(), 1);
        emit(
            &mut other,
            &mut scripted(&[
                "breakthrough:winnu:winnubt:SYSTEM_ACTIVATED:after",
                "ship|0",
            ]),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", "19")],
        );
        assert!(other.faction_marks.keys().all(|k| !k.contains("imperator")));
    }

    // -- Mathis Mathinus -------------------------------------------------------------------------

    /// The answer that keeps a player's command tokens where they are (Thunder's Edge Warfare
    /// offers a redistribution before its free tactical action).
    fn keep_tokens(state: &GameState, player: &PlayerId) -> String {
        let seat = state.player(player).unwrap();
        format!(
            "{}|{}|{}",
            seat.tactic_tokens, seat.fleet_tokens, seat.strategic_tokens
        )
    }

    #[test]
    fn hero_primary_offers_warfare_and_returns_the_free_tactical_result() {
        let content = ContentStore::embedded();
        let hub = crate::fixtures::hub_with_centre("18");
        let system = SystemId::new(hub.galaxy.system_ids().into_iter().next().unwrap());
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state
            .unclaimed_strategy_cards
            .push(StrategyCardId::new("te6warfare"));
        assert!(hero_cards(&state).contains(&StrategyCardId::new("te6warfare")));
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("winnuhero")),
            Some(true)
        );
        let keep = keep_tokens(&state, &a());
        let mut table = scripted(&["te6warfare", keep.as_str(), system.as_str()]);
        let result = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |context| use_leader_strategy_primary(context, &a(), &LeaderId::new("winnuhero")),
        );
        assert!(matches!(
            result,
            Ok(Some((card, crate::strategy_cards::Ability::FreeTactical(target))))
                if card.as_str() == "te6warfare" && target == system
        ));
    }

    #[test]
    fn hero_chooses_followers_clockwise_after_primary_resolution() {
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "hacan")], DEFAULT);
        state.phase = ti4_model::state::Phase::Action;
        state
            .unclaimed_strategy_cards
            .push(StrategyCardId::new("te6warfare"));
        let hub = crate::fixtures::hub_with_centre("18");
        let system = SystemId::new(hub.galaxy.system_ids().into_iter().next().unwrap());
        let keep = keep_tokens(&state, &a());
        let mut table = scripted(&[
            "te6warfare",
            keep.as_str(),
            system.as_str(),
            "yes|b",
            "no|c",
        ]);
        let result = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |context| {
                let primary =
                    use_leader_strategy_primary(context, &a(), &LeaderId::new("winnuhero"))?;
                assert!(matches!(
                    primary,
                    Some((_, crate::strategy_cards::Ability::FreeTactical(_)))
                ));
                leader_strategy_followers(
                    context,
                    &a(),
                    &LeaderId::new("winnuhero"),
                    &StrategyCardId::new("te6warfare"),
                )
            },
        );
        assert_eq!(result.unwrap(), Some(vec![b()]));
    }

    #[test]
    fn hero_invalid_primary_or_follower_answer_is_propagated() {
        let mut state = game();
        state
            .unclaimed_strategy_cards
            .push(StrategyCardId::new("te6warfare"));
        let before = state.clone();
        let mut table = scripted(&["not-a-card"]);
        let invalid_primary =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
                use_leader_strategy_primary(context, &a(), &LeaderId::new("winnuhero"))
            });
        assert!(invalid_primary.is_err());
        assert_eq!(state, before, "the invalid card answer mutates nothing");

        let mut state = crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT);
        let before = state.clone();
        let mut table = scripted(&["not-a-follower"]);
        let invalid_follower =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
                leader_strategy_followers(
                    context,
                    &a(),
                    &LeaderId::new("winnuhero"),
                    &StrategyCardId::new("te6warfare"),
                )
            });
        assert!(invalid_follower.is_err());
        assert_eq!(state, before, "the invalid follower answer mutates nothing");
    }

    #[test]
    fn the_hero_resolves_a_primary_and_lets_chosen_players_follow() {
        let mut state = game();
        let content = ContentStore::embedded();
        for (who, card) in [(&a(), "pok1leadership"), (&b(), "pok2diplomacy")] {
            state.player_mut(who).unwrap().strategy_cards = vec![StrategyCardId::new(card)];
        }
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("winnuhero"), LeaderStatus::Unlocked);
        let cards = hero_cards(&state);
        assert!(cards.contains(&StrategyCardId::new("pok1leadership")));
        assert!(
            cards.contains(&state.unclaimed_strategy_cards[0])
                || state.unclaimed_strategy_cards.is_empty()
        );
        let tokens = |state: &GameState, who: &PlayerId| {
            let seat = state.player(who).unwrap();
            seat.tokens(ti4_model::state::TokenPool::Tactic)
                + seat.tokens(ti4_model::state::TokenPool::Fleet)
                + seat.tokens(ti4_model::state::TokenPool::Strategic)
        };
        let before = tokens(&state, &a());
        // Leadership primary: first-option answers; b is chosen to follow and declines.
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["pok1leadership"]),
            |ctx| use_leader(ctx, &a(), &LeaderId::new("winnuhero")),
        );
        let _ = content;
        assert_eq!(done, Some(true));
        assert!(tokens(&state, &a()) >= before, "the primary resolved");
    }

    #[test]
    fn the_hero_is_not_offered_without_cards_and_other_leaders_are_not_claimed() {
        let mut state = game();
        let content = ContentStore::embedded();
        state.unclaimed_strategy_cards.clear();
        for seat in &mut state.players {
            seat.strategy_cards.clear();
        }
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("winnuhero")),
            Some(false)
        );
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("solhero")),
            None
        );
        let mut state = game();
        let none =
            crate::fixtures::with_context(&mut state, DEFAULT, None, &mut scripted(&[]), |ctx| {
                use_leader(ctx, &a(), &LeaderId::new("solhero"))
            });
        assert_eq!(none, None);
        let _ = unit("cruiser", &a());
    }

    #[test]
    fn the_hero_offers_unclaimed_cards_too() {
        let mut state = game();
        for seat in &mut state.players {
            seat.strategy_cards.clear();
        }
        state.unclaimed_strategy_cards = vec![StrategyCardId::new("pok1leadership")];
        assert_eq!(
            hero_cards(&state),
            vec![StrategyCardId::new("pok1leadership")]
        );
    }

    #[test]
    fn a_failed_primary_leaves_the_game_untouched() {
        let mut state = game();
        state.unclaimed_strategy_cards = vec![StrategyCardId::new("pok7technology")];
        let before = state.clone();
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["pok7technology", "not-an-option"]),
            |ctx| use_leader(ctx, &a(), &LeaderId::new("winnuhero")),
        );
        if done == Some(false) {
            assert_eq!(state, before, "rolled back");
        }
    }

    #[test]
    fn a_structure_is_not_placed_where_the_planet_has_no_room() {
        let mut state = game();
        let planet = PlanetId::new("mr");
        gain_mecatol(&mut state);
        crate::fixtures::put_on_planet(&mut state, &mecatol(), &planet, "winnu_mech", &a(), 1);
        // Reclamation first, then the Reclaimer in the same window: still one dock.
        emit(
            &mut state,
            &mut scripted(&[RECLAMATION, RECLAIMER, "pds|18|mr"]),
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(count_on(&state, &mecatol(), &planet, &a(), "spacedock"), 1);
        assert!(
            !reclaimer_options(&state, ContentStore::embedded(), DEFAULT, &a())
                .iter()
                .any(|o| o.id.starts_with("spacedock|")),
            "the dock is not offered again"
        );
    }

    #[test]
    fn control_gained_in_a_strategic_action_does_not_count() {
        let mut state = game();
        state.active = Some(a());
        take_mecatol(&mut state, &a());
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[("player", "a"), ("system", "18"), ("planet", "mr")],
        );
        assert!(
            gained(&state, &a()).is_empty(),
            "no activation, no tactical action"
        );
    }

    #[test]
    fn the_imperator_bonus_survives_another_ship_leaving_and_is_spent_by_its_own() {
        let mut state = game();
        let content = ContentStore::embedded();
        let legendary = ti4_content::galaxy::all_systems(content, DEFAULT)
            .into_iter()
            .find(|(id, _)| has_legendary(content, DEFAULT, &SystemId::new(*id)))
            .map(|(id, _)| id.to_owned())
            .expect("a legendary system");
        let home = home_of(&state, &a());
        state.system_mut(&home).units.clear();
        crate::fixtures::put(&mut state, &home, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &home, "carrier", &a(), 1);
        grant_bt(&mut state);
        emit(
            &mut state,
            &mut scripted(&[
                "breakthrough:winnu:winnubt:SYSTEM_ACTIVATED:after",
                "ship|1",
            ]),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", legendary.as_str())],
        );
        let mark = state
            .faction_marks
            .get(&imperator_key(&a()))
            .cloned()
            .unwrap();
        assert!(mark.contains("carrier"), "{mark}");
        // The cruiser leaves first: the carrier shifts to index 0 and still has the bonus.
        state
            .system_mut(&home)
            .units
            .retain(|u| u.type_id.as_str() != "cruiser");
        let kind = ti4_content::units::unit_type(content, "carrier", DEFAULT).unwrap();
        let owner = a();
        let site = |index| MoveSite {
            player: &owner,
            origin: &home,
            index: Some(index),
            ship: &kind,
        };
        assert_eq!(move_bonus(&state, &site(0)), 1);
        emit(
            &mut state,
            &mut scripted(&[]),
            "SHIP_MOVED",
            &[("player", "a")],
        );
        assert!(
            state.faction_marks.contains_key(&imperator_key(&a())),
            "not spent by the cruiser"
        );
        // The carrier leaves: spent.
        state.system_mut(&home).units.clear();
        emit(
            &mut state,
            &mut scripted(&[]),
            "SHIP_MOVED",
            &[("player", "a")],
        );
        assert!(!state.faction_marks.contains_key(&imperator_key(&a())));
    }

    #[test]
    fn the_claims_are_the_sheet() {
        assert_eq!(MODULE.leaders.len(), 3);
        assert_eq!(MODULE.units.len(), 2);
    }

    #[test]
    fn a_nekro_flagship_with_the_winnu_z_token_rolls_a_die_per_opposing_non_fighter_ship() {
        let content = ContentStore::embedded();
        let system = SystemId::new("19");
        let dice = |lent: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            state.system_mut(&system).units.clear();
            crate::fixtures::put(&mut state, &system, "nekro_flagship", &a(), 1);
            crate::fixtures::put(&mut state, &system, "cruiser", &b(), 3);
            crate::fixtures::put(&mut state, &system, "fighter", &b(), 4);
            crate::factions::unit_dice(
                &state,
                content,
                DEFAULT,
                &CombatUnit {
                    player: &a(),
                    system: Some(&system),
                    planet: None,
                    unit_type: "nekro_flagship",
                    context: "space",
                },
                2,
            )
        };
        assert_eq!(dice(&[]), 2, "off by default: its own printed dice");
        assert_eq!(dice(&["winnu"]), 3, "three opposing non-fighter ships");
    }
}
