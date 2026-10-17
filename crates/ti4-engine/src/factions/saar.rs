//! The Clan of Saar (`saar`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-saar.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Nomadic: "You can score objectives even if you do not control the planets in your home system."
//! * Scavenge: "After you gain control of a planet: Gain 1 trade good."
//! * Scavenger Zeta (mech): "DEPLOY: After you gain control of a planet, you may spend 1 trade
//!   good to place 1 mech on that planet"
//! * Ragh's Call (`ragh`): "After you commit 1 or more units to land on a planet: Remove all of
//!   the Saar player's ground forces from that planet and place them on a planet controlled by the
//!   Saar player. Then, return this card to the Saar player."
//! * Chaos Mapping (`cm`): "Other players cannot activate asteroid fields that contain 1 or more
//!   of your ships. At the start of your turn during the action phase, you may produce 1 unit in a
//!   system that contains at least 1 of your units that has PRODUCTION."
//! * Gurno Aggero (`saarhero`): "ACTION: Choose 1 system that is adjacent to 1 of your space
//!   docks. Destroy all other players' infantry and fighters in that system. Then, purge this
//!   card." Unlock: "Have 3 scored objectives."
//! * Rowl Sarring (`saarcommander`) unlock: "Have 3 space docks on the game board."
//! * Son of Ragh (flagship): ANTI-FIGHTER BARRAGE 6 (x4) -- statistics only.
//! * Deorbit Barrage: ACTION: exhaust, spend any amount of resources, and roll to hit a ground
//!   force on a planet within two Saar-adjacent systems of an asteroid field holding Saar ships.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_economy::EconomyHooks;
use super::hooks_movement::{MoveSite, MovementHooks};
use super::hooks_strategy::StrategyHooks;
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids (`ragh:saar`).
const FACTION: &str = "saar";
/// Ragh's Call's note id.
const RAGH_NOTE: &str = "ragh:saar";
const DEORBIT_ACTION: &str = "faction|saar|deorbit_barrage";
const DEORBIT_EXHAUSTED: &str = "saar:deorbit_barrage:exhausted:";

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["nomadic", "scavenge"],
    technologies: &["cm", "ffac2"],
    units: &[
        "saar_flagship",
        "saar_mech",
        "saar_spacedock",
        "saar_spacedock2",
    ],
    promissory: &["ragh"],
    leaders: &["saarhero", "saaragent", "saarcommander"],
    breakthroughs: &["saarbt"],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        mapped_component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        movement: MovementHooks {
            cannot_activate: Some(chaos_mapping_blocks_activation),
            move_bonus: Some(agent_move_bonus),
            ..MovementHooks::NONE
        },
        strategy: StrategyHooks {
            scores_without_home: Some(nomadic),
            ..StrategyHooks::NONE
        },
        economy: EconomyHooks {
            production_destinations: Some(commander_docks),
            ..EconomyHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- small readers -------------------------------------------------------------------------------

fn is_saar(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn deorbit_exhausted(player: &PlayerId) -> String {
    format!("{DEORBIT_EXHAUSTED}{player}")
}

fn deorbit_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<SystemId> {
    let types = catalogue(content, sources);
    let adjacency = crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, player);
    let mut result = std::collections::BTreeSet::new();
    for field in galaxy
        .system_ids()
        .into_iter()
        .map(SystemId::new)
        .filter(|system| {
            ti4_content::galaxy::system(content, system.as_str(), sources)
                .is_some_and(|tile| tile.is_asteroid_field())
        })
    {
        if !state.system_state(&field).units.iter().any(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ship)
        }) {
            continue;
        }
        result.insert(field.clone());
        let one = adjacency.neighbours(field.as_str());
        for system in &one {
            result.insert(SystemId::new(system.clone()));
        }
        for system in one {
            result.extend(adjacency.neighbours(&system).into_iter().map(SystemId::new));
        }
    }
    result.into_iter().collect()
}

fn deorbit_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<(SystemId, PlanetId, PlayerId)> {
    deorbit_systems(state, content, sources, galaxy, player)
        .into_iter()
        .flat_map(|system| {
            let owners: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
            owners.into_iter().flat_map(move |owner| {
                units_of_base(state, content, sources, &owner, "infantry")
                    .into_iter()
                    .chain(units_of_base(state, content, sources, &owner, "mech"))
                    .filter_map({
                        let system = system.clone();
                        move |(at, planet, _)| {
                            (at == system).then_some((system.clone(), planet?, owner.clone()))
                        }
                    })
            })
        })
        .collect()
}

fn component_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    if !is_saar(state, player)
        || !crate::breakthroughs::holds(state, player, "saarbt")
        || state.faction_marks.contains_key(&deorbit_exhausted(player))
        || state.phase != ti4_model::state::Phase::Action
    {
        return Vec::new();
    }
    if deorbit_planets(state, content, sources, galaxy, player).is_empty() {
        return Vec::new();
    }
    vec![ChoiceOption::labelled(
        DEORBIT_ACTION,
        crate::faction_abilities::ACTION_KIND,
        "Deorbit Barrage: exhaust and spend resources to bombard a planet",
    )]
}

fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    if option.id != DEORBIT_ACTION
        || !is_saar(context.state, player)
        || !crate::breakthroughs::holds(context.state, player, "saarbt")
        || context.state.phase != ti4_model::state::Phase::Action
        || context
            .state
            .faction_marks
            .contains_key(&deorbit_exhausted(player))
    {
        return false;
    }
    let Some(galaxy) = context.galaxy else {
        return false;
    };
    let planets = deorbit_planets(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
    );
    if planets.is_empty() {
        return false;
    }
    let max = crate::production::available(
        context.state,
        context.content,
        context.sources,
        player,
        crate::production::Spend::Resources,
    )
    .max(0);
    let spend_options: Vec<ChoiceOption> = (0..=max)
        .map(|n| {
            ChoiceOption::labelled(
                n.to_string(),
                "deorbit_spend",
                format!("spend {n} resources"),
            )
        })
        .collect();
    let Ok(spend_answer) = ask(
        context,
        player,
        "Deorbit Barrage: how many resources".to_owned(),
        "saarbt",
        "deorbit_spend",
        spend_options,
        false,
    ) else {
        return false;
    };
    let Ok(amount) = spend_answer.id.parse::<i64>() else {
        return false;
    };
    if amount < 0 || amount > max {
        return false;
    }
    let planet_options: Vec<ChoiceOption> = planets
        .iter()
        .map(|(system, planet, _owner)| {
            ChoiceOption::labelled(
                format!("{system}|{planet}"),
                "deorbit_target",
                format!("{planet} ({system})"),
            )
        })
        .map(|option| (option.id.clone(), option))
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_values()
        .collect();
    let Ok(answer) = ask(
        context,
        player,
        "Deorbit Barrage: choose a planet".to_owned(),
        "saarbt",
        "deorbit_target",
        planet_options,
        false,
    ) else {
        return false;
    };
    let Some((system, planet, _)) = planets
        .iter()
        .find(|(s, p, _)| answer.id == format!("{s}|{p}"))
        .cloned()
    else {
        return false;
    };
    let before_payment = context.state.clone();
    match crate::production::pay_seeing(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        context.table,
        player,
        amount,
        crate::production::Spend::Resources,
    ) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            *context.state = before_payment;
            return false;
        }
    }
    context
        .state
        .faction_marks
        .insert(deorbit_exhausted(player), "1".to_owned());
    let mut payload = std::collections::BTreeMap::new();
    payload.insert(
        "player".to_owned(),
        serde_json::Value::String(player.to_string()),
    );
    payload.insert(
        "system".to_owned(),
        serde_json::Value::String(system.to_string()),
    );
    payload.insert(
        "planet".to_owned(),
        serde_json::Value::String(planet.to_string()),
    );
    payload.insert(
        "hits".to_owned(),
        serde_json::Value::String(amount.to_string()),
    );
    crate::supply::stage_event(context.state, "SAAR_DEORBIT_BARRAGE", &payload);
    true
}

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
}

/// The Saar seat, if one is playing.
fn saar_seat(state: &GameState) -> Option<PlayerId> {
    state
        .players
        .iter()
        .find(|seat| seat.faction.as_str() == FACTION)
        .map(|seat| seat.id.clone())
}

fn illegal(error: crate::choice::IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

fn ask(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: String,
    ability: &str,
    subtype: &str,
    mut options: Vec<ChoiceOption>,
    optional: bool,
) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
    if optional {
        options.push(ChoiceOption::decline());
    }
    let choice = Choice::new(player.clone(), prompt, options).contextualized(DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(ability.to_owned()),
        subtype,
        context.state.phase,
        context.state.round,
    ));
    context.ask_seeing(&choice)
}

/// Every unit of `player` (space and planets) whose base type is `base`, with where it stands.
fn units_of_base(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base: &str,
) -> Vec<(SystemId, Option<PlanetId>, Unit)> {
    let types = catalogue(content, sources);
    let is = |unit: &Unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == base)
    };
    let mut found = Vec::new();
    for (system, board) in &state.board {
        for unit in board.units.iter().filter(|unit| is(unit)) {
            found.push((system.clone(), None, unit.clone()));
        }
        for (planet, units) in &board.planet_units {
            for unit in units.iter().filter(|unit| is(unit)) {
                found.push((system.clone(), Some(planet.clone()), unit.clone()));
            }
        }
    }
    found
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    crate::leaders::status(state, player, &LeaderId::new(leader))
}

// -- Rowl Sarring unlock -------------------------------------------------------------------------

/// "Have 3 space docks on the game board." A Floating Factory is a space dock (same base type).
fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "saarcommander" {
        return None;
    }
    Some(units_of_base(state, content, sources, player, "spacedock").len() >= 3)
}

/// Rowl Sarring: "When you produce fighters or infantry: You may place each of those units at
/// any of your space docks that are not blockaded." (`EconomyHooks::production_destinations`.)
/// A dock is blockaded when its system holds another player's ship and none of the Saar
/// player's (the space-dock blockade `production::destroy_blockaded_mobile_docks` uses; neutral
/// ships count). Fighters go to the space area; infantry go on a dock's planet, or into the space
/// area for a Floating Factory there. The "may" is the placement choice, which keeps the
/// producing system's own spots.
fn commander_docks(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    _producing: &SystemId,
    unit_base: &str,
) -> Vec<(SystemId, Option<PlanetId>)> {
    // Own unlocked commander, a faceup Alliance, or a Yin grant: the docks are the holder's.
    if !matches!(unit_base, "fighter" | "infantry")
        || !crate::promissory::has_commander_ability(state, player, "saarcommander")
    {
        return Vec::new();
    }
    let types = catalogue(content, sources);
    let is_ship = |unit: &Unit| {
        types
            .get(unit.type_id.as_str())
            .is_some_and(UnitType::is_ship)
    };
    let blockaded = |system: &SystemId| {
        let units = &state.system_state(system).units;
        units
            .iter()
            .any(|unit| &unit.owner != player && is_ship(unit))
            && !units
                .iter()
                .any(|unit| &unit.owner == player && is_ship(unit))
    };
    units_of_base(state, content, sources, player, "spacedock")
        .into_iter()
        .filter(|(system, _, _)| !blockaded(system))
        .map(|(system, planet, _)| match planet {
            Some(planet) if unit_base == "infantry" => (system, Some(planet)),
            _ => (system, None),
        })
        .collect()
}

// -- timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        scavenge(owner_name, seat),
        scavenger_zeta(owner_name, seat),
        ragh_call(owner_name, seat),
        agent(owner_name, seat),
        deorbit_resolve(owner_name, seat),
        deorbit_ready(owner_name, seat),
    ];
    // Resolver listeners are armed once at setup and cannot be registered for a later Ssruu gain.
    // Install a static source/borrower pair for every seated borrower, then let its live condition
    // consult the current borrowable-agent roster.
    if is_saar(state, seat) {
        abilities.extend(
            state
                .players
                .iter()
                .filter(|borrower| &borrower.id != seat)
                .map(|borrower| borrowed_agent(owner_name, seat, &borrower.id)),
        );
    }
    abilities
}

fn deorbit_resolve(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:saarbt:SAAR_DEORBIT_BARRAGE:after"),
        seat.clone(),
        "SAAR_DEORBIT_BARRAGE",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            if event.text("player") != Some(owner.as_str()) {
                return Ok(());
            }
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let Some(planet) = event.text("planet").map(PlanetId::new) else {
                return Ok(());
            };
            let Some(count) = event.text("hits").and_then(|n| n.parse::<usize>().ok()) else {
                return Ok(());
            };
            let roll = context
                .dice
                .roll_by(context.rng, count, "saarbt", Some(4), &owner);
            let hits = roll.faces.iter().filter(|face| **face >= 4).count();
            let mut remaining = hits;
            while remaining > 0 {
                remaining -= 1;
                let types = catalogue(context.content, context.sources);
                let units: Vec<_> = context
                    .state
                    .system_state(&system)
                    .on_planet(&planet)
                    .iter()
                    .filter(|unit| {
                        types
                            .get(unit.type_id.as_str())
                            .is_some_and(UnitType::is_ground_force)
                    })
                    .cloned()
                    .collect();
                if units.is_empty() {
                    break;
                }
                let options = units
                    .iter()
                    .enumerate()
                    .map(|(index, unit)| {
                        ChoiceOption::labelled(
                            index.to_string(),
                            "deorbit_hit",
                            format!("assign hit to {}'s {}", unit.owner, unit.type_id),
                        )
                    })
                    .collect();
                let answer = ask(
                    context,
                    &owner,
                    "Deorbit Barrage: assign a hit".to_owned(),
                    "saarbt",
                    "deorbit_hit",
                    options,
                    false,
                )
                .map_err(illegal)?;
                let Some(unit) = answer
                    .id
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| units.get(index))
                else {
                    continue;
                };
                // A sustain under Non-Euclidean Shielding cancels a second hit as well.
                let used = crate::invasion::assign_selected_ground_hit_in_timing(
                    resolver, context, &system, &planet, unit, "saarbt",
                )?;
                remaining = remaining.saturating_sub(used.saturating_sub(1));
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, _| {
        event.text("player") == Some(condition_owner.as_str())
    }))
}

fn deorbit_ready(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let key_owner = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:saarbt_ready:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context
                .state
                .faction_marks
                .remove(&deorbit_exhausted(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&deorbit_exhausted(&key_owner))
    }))
}

fn gained_by(event: &crate::event::Event, player: &PlayerId) -> Option<(SystemId, PlanetId)> {
    if event.text("player") != Some(player.as_str()) {
        return None;
    }
    Some((
        SystemId::new(event.text("system")?),
        PlanetId::new(event.text("planet")?),
    ))
}

/// Scavenge: "After you gain control of a planet: Gain 1 trade good." Mandatory.
fn scavenge(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:scavenge:PLANET_CONTROL_GAINED:after"),
        seat.clone(),
        "PLANET_CONTROL_GAINED",
        Relation::After,
        Arc::new(move |_event, resolver, context| {
            crate::supply::gain_trade_goods_via(context, resolver, &owner, 1, "scavenge")?;
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        crate::faction_abilities::has(context.state, context.content, &condition_owner, "scavenge")
            && gained_by(event, &condition_owner).is_some()
    }))
}

fn mech_id(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Option<UnitTypeId> {
    let faction = state.player(player)?.faction.to_string();
    ti4_content::units::faction_unit(content, &faction, "mech", sources)
        .map(|unit| UnitTypeId::new(unit.id().to_owned()))
}

/// Scavenger Zeta DEPLOY: "After you gain control of a planet, you may spend 1 trade good to place
/// 1 mech on that planet." The window's own use/decline is the "may".
fn scavenger_zeta(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:saar_mech:PLANET_CONTROL_GAINED:after"),
        seat.clone(),
        "PLANET_CONTROL_GAINED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((system, planet)) = gained_by(event, &owner) else {
                return Ok(());
            };
            if !deploy_ready(context.state, context.content, context.sources, &owner) {
                return Ok(());
            }
            // Xander Alexin Victori III (Keleres): the agent may let a commodity pay the trade good.
            let paid = crate::supply::with_goods_window(context, &owner, 1, |context| {
                crate::supply::spend_goods(context.state, &owner, 1)
            });
            if paid != Some(true) {
                return Ok(());
            }
            crate::action_cards::place_units_counted(
                context,
                &owner,
                &system,
                Some(&planet),
                "mech",
                1,
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        gained_by(event, &condition_owner).is_some()
            && deploy_ready(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
    }))
}

/// A trade good to spend and a mech left in the box (31.4).
fn deploy_ready(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
) -> bool {
    is_saar(state, owner)
        && crate::supply::potential_goods(state, owner) >= 1
        && mech_id(state, content, sources, owner)
            .is_some_and(|id| crate::supply::allowed(state, content, sources, owner, &id, 1) > 0)
}

// -- Ragh's Call ---------------------------------------------------------------------------------

/// The Saar ground forces on a planet, and the Saar player's other planets they can go to.
#[allow(
    clippy::type_complexity,
    reason = "one private tuple: saar seat, its forces, its destinations"
)]
fn ragh_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    holder: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> Option<(PlayerId, Vec<Unit>, Vec<(SystemId, PlanetId)>)> {
    if crate::promissory::held_foreign(state, holder, "ragh") == 0 {
        return None;
    }
    let saar = saar_seat(state)?;
    if &saar == holder {
        return None;
    }
    let types = catalogue(content, sources);
    let forces: Vec<Unit> = state
        .system_state(system)
        .on_planet_of(planet, &saar)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_ground_force)
        })
        .cloned()
        .collect();
    if forces.is_empty() {
        return None;
    }
    let destinations: Vec<(SystemId, PlanetId)> = state
        .controlled_planets(&saar)
        .into_iter()
        .filter(|(to_system, to_planet)| !(*to_system == system && *to_planet == planet))
        .map(|(to_system, to_planet)| (to_system.clone(), to_planet.clone()))
        .collect();
    if destinations.is_empty() {
        return None;
    }
    Some((saar, forces, destinations))
}

/// Ragh's Call, played by its holder after committing units to a planet. The Saar player picks the
/// destination (it is "a planet controlled by the Saar player"); the choice is asked before any
/// unit moves.
fn ragh_call(owner_name: &str, seat: &PlayerId) -> Ability {
    let holder = seat.clone();
    let condition_holder = seat.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:ragh:UNITS_COMMITTED:after"),
        seat.clone(),
        "UNITS_COMMITTED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((system, planet)) = gained_by(event, &holder) else {
                return Ok(());
            };
            let Some((saar, forces, destinations)) = ragh_targets(
                context.state,
                context.content,
                context.sources,
                &holder,
                &system,
                &planet,
            ) else {
                return Ok(());
            };
            let options = destinations
                .iter()
                .map(|(to_system, to_planet)| {
                    ChoiceOption::labelled(
                        format!("{to_system}|{to_planet}"),
                        "planet",
                        format!("move the Saar ground forces to {to_planet}"),
                    )
                })
                .collect();
            let answer = ask(
                context,
                &saar,
                format!("Ragh's Call: where do the Saar ground forces on {planet} go"),
                "ragh",
                "ragh_destination",
                options,
                false,
            )
            .map_err(illegal)?;
            let Some((to_system, to_planet)) = destinations
                .iter()
                .find(|(s, p)| answer.id == format!("{s}|{p}"))
            else {
                return Ok(());
            };
            context
                .state
                .system_mut(&system)
                .remove_from_planet(&planet, &forces);
            context
                .state
                .system_mut(to_system)
                .planet_units
                .entry(to_planet.clone())
                .or_default()
                .extend(forces);
            crate::promissory::give_back(context.state, RAGH_NOTE);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        gained_by(event, &condition_holder).is_some_and(|(system, planet)| {
            ragh_targets(
                context.state,
                context.content,
                context.sources,
                &condition_holder,
                &system,
                &planet,
            )
            .is_some()
        })
    }))
}

// -- Nomadic and Chaos Mapping activation --------------------------------------------------------

/// "You can score objectives even if you do not control the planets in your home system."
/// Lifts only the home-control prerequisite (LRR 61.16); objective requirements still apply.
fn nomadic(state: &GameState, player: &PlayerId) -> bool {
    is_saar(state, player)
}

/// "Other players cannot activate asteroid fields that contain 1 or more of your ships."
/// Checked while generating legal activations (LRR 89.1). Floating Factories move as ships but
/// remain structures, so they do not satisfy this ship requirement.
fn chaos_mapping_blocks_activation(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    activator: &PlayerId,
    system: &SystemId,
) -> bool {
    if !ti4_content::galaxy::system(content, system.as_str(), sources)
        .is_some_and(|tile| tile.is_asteroid_field())
    {
        return false;
    }
    let types = catalogue(content, sources);
    state.system_state(system).units.iter().any(|unit| {
        &unit.owner != activator
            && has_technology(state, &unit.owner, "cm")
            && types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_ship)
    })
}

// -- Captain Mendosa -----------------------------------------------------------------------------

/// `GameState::faction_marks` key for the ship the agent boosted, valued
/// `"<activation_seq>|<origin>|<index>|<bonus>"`, per activating player.
fn agent_key(player: &PlayerId) -> String {
    format!("saar:agent:boost:{player}")
}

/// The highest printed move value of any ship on the board.
fn highest_move(state: &GameState, content: &ContentStore, sources: SourceSet) -> i64 {
    let types = catalogue(content, sources);
    state
        .board
        .values()
        .flat_map(|board| board.units.iter())
        .filter_map(|unit| types.get(unit.type_id.as_str()))
        .filter(|kind| kind.is_ship())
        .map(UnitType::move_value)
        .max()
        .unwrap_or(0)
}

/// `activator`'s ships that are slower than the fastest ship on the board:
/// `(origin, index in ships_of, bonus, label)`.
fn agent_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    activator: &PlayerId,
) -> Vec<(SystemId, usize, i64, String)> {
    let top = highest_move(state, content, sources);
    let types = catalogue(content, sources);
    let modern = state.rules_version >= ti4_model::state::RULES_MENDOSA_SHIP;
    let mut found = Vec::new();
    for system in state.board.keys() {
        for (index, unit) in state.ships_of(activator, system).into_iter().enumerate() {
            let Some(kind) = types.get(unit.type_id.as_str()) else {
                continue;
            };
            let own = kind.move_value();
            if kind.is_ship() && own < top {
                // Equal ships are one choice (the boost goes to the first of the kind).
                if modern
                    && found
                        .iter()
                        .any(|(sys, i, _, _): &(SystemId, usize, i64, String)| {
                            sys == system
                                && state
                                    .ships_of(activator, system)
                                    .get(*i)
                                    .is_some_and(|other| {
                                        other.type_id == unit.type_id
                                            && other.sustained_damage == unit.sustained_damage
                                    })
                        })
                {
                    continue;
                }
                found.push((
                    system.clone(),
                    index,
                    top - own,
                    format!("{} in {system} (move {own} to {top})", unit.type_id),
                ));
            }
        }
    }
    found
}

/// Captain Mendosa: "When a player activates a system: You may exhaust this card to increase the
/// move value of 1 of that player's ships to match the move value of the ship on the game board that
/// has the highest move value." The boost is remembered against this activation and read by
/// [`agent_move_bonus`].
fn agent(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:saaragent:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(activator) = event.text("player").map(PlayerId::new) else {
                return Ok(());
            };
            if leader_status(context.state, &owner, "saaragent") != Some(LeaderStatus::Readied) {
                return Ok(());
            }
            let ships = agent_ships(context.state, context.content, context.sources, &activator);
            if ships.is_empty() {
                return Ok(());
            }
            let options = ships
                .iter()
                .enumerate()
                .map(|(n, (_, _, _, label))| {
                    ChoiceOption::labelled(format!("ship|{n}"), "ship", label.clone())
                })
                .collect();
            let answer = ask(
                context,
                &owner,
                "Captain Mendosa: raise which of the activating player's ships".to_owned(),
                "saaragent",
                "agent_ship",
                options,
                true,
            )
            .map_err(illegal)?;
            let Some((origin, index, bonus, _)) = ships
                .iter()
                .enumerate()
                .find(|(n, _)| answer.id == format!("ship|{n}"))
                .map(|(_, ship)| ship)
            else {
                return Ok(());
            };
            if !crate::leaders::exhaust(context.state, &owner, &LeaderId::new("saaragent")) {
                return Ok(());
            }
            store_boost(context.state, &activator, origin, *index, *bonus);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        leader_status(context.state, &condition_owner, "saaragent") == Some(LeaderStatus::Readied)
            && event.text("player").is_some_and(|activator| {
                !agent_ships(
                    context.state,
                    context.content,
                    context.sources,
                    &PlayerId::new(activator),
                )
                .is_empty()
            })
    }))
}

/// Captain Mendosa copied through Ssruu. The Saar source retains the agent; this listener belongs
/// to the borrower and is active only while the exact source is in Ssruu's live roster.
fn borrowed_agent(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    Ability::stateful(
        format!(
            "leader:{owner_name}:{source}:yssarilagent:{borrower}:saaragent:SYSTEM_ACTIVATED:after"
        ),
        borrower.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(activator) = event.text("player").map(PlayerId::new) else {
                return Ok(());
            };
            if !has_borrowable_saar_agent(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) {
                return Ok(());
            }
            let ships = agent_ships(context.state, context.content, context.sources, &activator);
            if ships.is_empty() {
                return Ok(());
            }
            let options = ships
                .iter()
                .enumerate()
                .map(|(n, (_, _, _, label))| {
                    ChoiceOption::labelled(format!("ship|{n}"), "ship", label.clone())
                })
                .collect();
            let answer = ask(
                context,
                &effect_borrower,
                "Captain Mendosa (Ssruu): raise which of the activating player's ships".to_owned(),
                "saaragent",
                "agent_ship",
                options,
                true,
            )
            .map_err(illegal)?;
            let Some((origin, index, bonus, _)) = ships
                .iter()
                .enumerate()
                .find(|(n, _)| answer.id == format!("ship|{n}"))
                .map(|(_, ship)| ship)
            else {
                return Ok(());
            };
            if !crate::leaders::exhaust(
                context.state,
                &effect_borrower,
                &LeaderId::new("yssarilagent"),
            ) {
                return Ok(());
            }
            store_boost(context.state, &activator, origin, *index, *bonus);
            super::hooks_cards::borrowed_agent_used(
                context,
                &effect_borrower,
                &LeaderId::new("saaragent"),
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player").is_some_and(|activator| {
            has_borrowable_saar_agent(
                context.state,
                context.content,
                &condition_source,
                &condition_borrower,
            ) && !agent_ships(
                context.state,
                context.content,
                context.sources,
                &PlayerId::new(activator),
            )
            .is_empty()
        })
    }))
}

fn has_borrowable_saar_agent(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == "saaragent")
}

/// Remember the boost for this activation. From [`ti4_model::state::RULES_MENDOSA_SHIP`] it is
/// `"<seq>|<origin>|<type>|<damaged>|<count of that kind at the origin>|<bonus>"`: the ship's
/// kind, not its list position, which shifts when another ship leaves first. Before, it is
/// `"<seq>|<origin>|<index>|<bonus>"`.
fn store_boost(
    state: &mut GameState,
    activator: &PlayerId,
    origin: &SystemId,
    index: usize,
    bonus: i64,
) {
    let seq = state.activation_seq;
    let value = if state.rules_version >= ti4_model::state::RULES_MENDOSA_SHIP {
        let ships = state.ships_of(activator, origin);
        let Some(unit) = ships.get(index) else {
            return;
        };
        let (kind, damaged) = (unit.type_id.to_string(), unit.sustained_damage);
        let count = kind_count(state, activator, origin, &kind, damaged);
        format!(
            "{seq}|{origin}|{kind}|{}|{count}|{bonus}",
            u8::from(damaged)
        )
    } else {
        format!("{seq}|{origin}|{index}|{bonus}")
    };
    state.faction_marks.insert(agent_key(activator), value);
}

fn kind_count(
    state: &GameState,
    player: &PlayerId,
    origin: &SystemId,
    kind: &str,
    damaged: bool,
) -> usize {
    state
        .ships_of(player, origin)
        .into_iter()
        .filter(|ship| ship.type_id.as_str() == kind && ship.sustained_damage == damaged)
        .count()
}

/// The boost the agent granted to one ship for this activation.
fn agent_move_bonus(state: &GameState, site: &MoveSite<'_>) -> i32 {
    let Some(mark) = state.faction_marks.get(&agent_key(site.player)) else {
        return 0;
    };
    let parts: Vec<&str> = mark.split('|').collect();
    if state.rules_version >= ti4_model::state::RULES_MENDOSA_SHIP {
        // The boosted kind's first ship, until one of that kind has left the origin (the boosted
        // one, or an equal one standing in for it).
        let [seq, origin, kind, damaged, count, bonus] = parts[..] else {
            return 0;
        };
        let damaged = damaged == "1";
        let (Some(index), Ok(count)) = (site.index, count.parse::<usize>()) else {
            return 0;
        };
        let first = state
            .ships_of(site.player, site.origin)
            .into_iter()
            .enumerate()
            .filter(|(_, ship)| ship.type_id.as_str() == kind && ship.sustained_damage == damaged)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        return if seq == state.activation_seq.to_string()
            && origin == site.origin.as_str()
            && first.len() == count
            && first.first() == Some(&index)
        {
            bonus.parse().unwrap_or(0)
        } else {
            0
        };
    }
    let [seq, origin, index, bonus] = parts[..] else {
        return 0;
    };
    if seq == state.activation_seq.to_string()
        && origin == site.origin.as_str()
        && Some(index) == site.index.map(|i| i.to_string()).as_deref()
    {
        bonus.parse().unwrap_or(0)
    } else {
        0
    }
}

// -- Gurno Aggero --------------------------------------------------------------------------------

fn dock_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    let mut found: Vec<SystemId> = units_of_base(state, content, sources, player, "spacedock")
        .into_iter()
        .map(|(system, _, _)| system)
        .collect();
    found.dedup();
    found
}

fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "saarhero").then(|| {
        // Any adjacent system is a legal choice, so the action can resolve whenever a dock stands.
        !dock_systems(state, content, ti4_model::content_types::DEFAULT, player).is_empty()
            && leader_status(state, player, "saarhero").is_some()
    })
}

fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == "saarhero").then(|| hero(context, player))
}

/// "Choose 1 system that is adjacent to 1 of your space docks. Destroy all other players' infantry
/// and fighters in that system." The system is chosen before anything is destroyed.
fn hero(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let Some(galaxy) = context.galaxy else {
        return false;
    };
    let docks = dock_systems(context.state, context.content, context.sources, player);
    let mut candidates: Vec<SystemId> = Vec::new();
    let adjacency = crate::movement::PlayerAdjacency::new(
        context.state,
        context.content,
        context.sources,
        galaxy,
        player,
    );
    for dock in &docks {
        for neighbour in adjacency.neighbours(dock.as_str()) {
            let neighbour = SystemId::new(neighbour);
            if !candidates.contains(&neighbour) {
                candidates.push(neighbour);
            }
        }
    }
    candidates.sort();
    if candidates.is_empty() {
        return false;
    }
    let options = candidates
        .iter()
        .map(|s| ChoiceOption::labelled(s.to_string(), "system", format!("system {s}")))
        .collect();
    let Ok(answer) = ask(
        context,
        player,
        "Gurno Aggero: destroy all other players' infantry and fighters in which system".to_owned(),
        "saarhero",
        "armageddon_relay",
        options,
        false,
    ) else {
        return false;
    };
    let Some(system) = candidates.iter().find(|s| s.as_str() == answer.id).cloned() else {
        return false;
    };
    // Decide everything first, then remove. Fighters and infantry alike are removed directly (no
    // `pending_destructions` entry is staged: a leader effect has no way to announce them).
    let mut doomed: Vec<(Option<PlanetId>, Unit)> = Vec::new();
    for seat in &context.state.players {
        if &seat.id == player {
            continue;
        }
        for base in ["fighter", "infantry"] {
            for (at, planet, unit) in units_of_base(
                context.state,
                context.content,
                context.sources,
                &seat.id,
                base,
            ) {
                if at == system {
                    doomed.push((planet, unit));
                }
            }
        }
    }
    for (planet, unit) in doomed {
        let board = context.state.system_mut(&system);
        match planet {
            Some(planet) => board.remove_from_planet(&planet, std::slice::from_ref(&unit)),
            None => board.remove(std::slice::from_ref(&unit)),
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::DEFAULT;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }

    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn c() -> PlayerId {
        PlayerId::new("c")
    }

    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }

    fn emit(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let payload = pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
                .collect();
            let event = ctx
                .event_sequence
                .next(event_type, payload)
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

    fn trade_goods(state: &GameState, who: &PlayerId) -> i32 {
        state.player(who).unwrap().trade_goods
    }

    /// A planet in the Saar home system, as `(system, planet)`.
    fn home_planet(state: &GameState, who: &PlayerId) -> (SystemId, PlanetId) {
        let (s, p) = state.controlled_planets(who)[0];
        (s.clone(), p.clone())
    }

    fn mechs(state: &GameState, system: &SystemId, planet: &PlanetId, who: &PlayerId) -> usize {
        state
            .system_state(system)
            .on_planet_of(planet, who)
            .iter()
            .filter(|unit| unit.type_id.as_str() == "saar_mech")
            .count()
    }

    #[test]
    fn scavenge_pays_a_trade_good_for_each_planet_gained_and_only_to_the_gainer() {
        let mut state = game();
        let (system, planet) = home_planet(&state, &a());
        let before = (trade_goods(&state, &a()), trade_goods(&state, &b()));
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("system", system.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        assert_eq!(trade_goods(&state, &a()), before.0 + 1);
        assert_eq!(trade_goods(&state, &b()), before.1);
        // The other player gaining a planet pays Saar nothing.
        emit(
            &mut state,
            &mut scripted(&[]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "b"),
                ("system", system.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        assert_eq!(trade_goods(&state, &a()), before.0 + 1);
    }

    #[test]
    fn scavenger_zeta_costs_a_trade_good_and_places_a_mech_or_is_declined() {
        let mut state = game();
        let (system, planet) = home_planet(&state, &a());
        let id = "unit:saar:saar_mech:PLANET_CONTROL_GAINED:after";
        let base = mechs(&state, &system, &planet, &a());
        let tg = trade_goods(&state, &a());
        emit(
            &mut state,
            &mut scripted(&[id]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("system", system.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        assert_eq!(mechs(&state, &system, &planet, &a()), base + 1);
        assert_eq!(trade_goods(&state, &a()), tg, "+1 Scavenge, -1 DEPLOY");
        // Declined: the mech is not placed and the trade good is kept.
        let mut state = game();
        let tg = trade_goods(&state, &a());
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "PLANET_CONTROL_GAINED",
            &[
                ("player", "a"),
                ("system", system.as_str()),
                ("planet", planet.as_str()),
            ],
        );
        assert_eq!(mechs(&state, &system, &planet, &a()), base);
        assert_eq!(trade_goods(&state, &a()), tg + 1);
    }

    #[test]
    fn the_stats_of_the_flagship_are_data_driven() {
        let content = ContentStore::embedded();
        let types = catalogue(content, DEFAULT);
        let flagship = types.get("saar_flagship").expect("flagship");
        assert!(flagship.is_ship());
        assert_eq!(flagship.base_type(), "flagship");
    }

    #[test]
    fn the_commander_unlocks_at_three_space_docks() {
        let content = ContentStore::embedded();
        let mut state = game();
        let leader = LeaderId::new("saarcommander");
        let (system, planet) = home_planet(&state, &a());
        let unlocked =
            |state: &GameState| commander_unlocked(state, content, DEFAULT, None, &a(), &leader);
        let docks = units_of_base(&state, content, DEFAULT, &a(), "spacedock").len();
        assert!(docks < 3);
        for _ in docks..2 {
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "saar_spacedock", &a(), 1);
        }
        assert_eq!(unlocked(&state), Some(false), "two docks");
        // A dock in the space area counts (a Floating Factory).
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("saar_spacedock"), a()));
        assert_eq!(unlocked(&state), Some(true), "three docks");
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &a(), &LeaderId::new("x")),
            None
        );
    }

    #[test]
    fn the_commander_docks_are_every_unblockaded_dock() {
        let content = ContentStore::embedded();
        let mut state = game();
        let hub = crate::fixtures::plain_hub();
        let dock = SystemId::new(hub.outer[0].clone());
        let home = SystemId::new(hub.centre.clone());
        for id in std::iter::once(&hub.centre).chain(hub.outer.iter()) {
            let board = state.system_mut(&SystemId::new(id.clone()));
            board.units.clear();
            board.planet_units.clear();
        }
        state
            .system_mut(&dock)
            .units
            .push(Unit::new(UnitTypeId::new("saar_spacedock"), a()));
        let docks = |state: &GameState, unit: &str| {
            commander_docks(state, content, DEFAULT, &a(), &home, unit)
                .into_iter()
                .filter(|(system, _)| system == &dock)
                .collect::<Vec<_>>()
        };
        assert!(
            docks(&state, "fighter").is_empty(),
            "commander still locked"
        );
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("saarcommander"), LeaderStatus::Unlocked);
        assert_eq!(docks(&state, "fighter"), [(dock.clone(), None)]);
        assert_eq!(
            docks(&state, "infantry"),
            [(dock.clone(), None)],
            "Floating Factory: space"
        );
        assert!(
            docks(&state, "mech").is_empty(),
            "fighters and infantry only"
        );
        assert!(docks(&state, "cruiser").is_empty());
        // An enemy ship blockades the dock; one of the Saar player's own ships lifts it.
        state
            .system_mut(&dock)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()));
        assert!(docks(&state, "fighter").is_empty(), "blockaded");
        state
            .system_mut(&dock)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        assert_eq!(
            docks(&state, "fighter").len(),
            1,
            "not blockaded with a friendly ship"
        );
        let sol = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        assert!(commander_docks(&sol, content, DEFAULT, &a(), &home, "fighter").is_empty());
    }

    #[test]
    fn a_game_without_saar_is_offered_no_saar_ability() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let (system, planet) = {
            let (s, p) = state.controlled_planets(&a())[0];
            (s.clone(), p.clone())
        };
        let before = state.clone();
        for event in ["PLANET_CONTROL_GAINED", "UNITS_COMMITTED", "TURN_BEGAN"] {
            // An empty script fails the test if any choice is asked.
            emit(
                &mut state,
                &mut scripted(&[]),
                event,
                &[
                    ("player", "a"),
                    ("system", system.as_str()),
                    ("planet", planet.as_str()),
                ],
            );
        }
        assert_eq!(
            state.player(&a()).unwrap().trade_goods,
            before.player(&a()).unwrap().trade_goods
        );
        assert_eq!(state.board, before.board);
        let content = ContentStore::embedded();
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("saarhero")),
            Some(false)
        );
    }

    #[test]
    fn ragh_s_call_moves_the_saar_forces_to_a_planet_they_control_and_returns_the_note() {
        let mut state = game();
        let (home_system, home_planet_id) = home_planet(&state, &a());
        let (their_system, their_planet) = home_planet(&state, &b());
        crate::fixtures::put_on_planet(
            &mut state,
            &their_system,
            &their_planet,
            "infantry",
            &a(),
            2,
        );
        state.promissory_notes.insert(RAGH_NOTE.to_owned(), b());
        let destination = format!("{home_system}|{home_planet_id}");
        let at_home = |state: &GameState| {
            state
                .system_state(&home_system)
                .on_planet_of(&home_planet_id, &a())
                .len()
        };
        let home_before = at_home(&state);
        emit(
            &mut state,
            &mut scripted(&["promissory:sol:ragh:UNITS_COMMITTED:after", &destination]),
            "UNITS_COMMITTED",
            &[
                ("player", "b"),
                ("system", their_system.as_str()),
                ("planet", their_planet.as_str()),
            ],
        );
        assert!(
            state
                .system_state(&their_system)
                .on_planet_of(&their_planet, &a())
                .is_empty()
        );
        assert_eq!(at_home(&state), home_before + 2);
        assert_ne!(
            state.promissory_notes.get(RAGH_NOTE),
            Some(&b()),
            "returned"
        );
    }

    #[test]
    fn ragh_s_call_is_not_offered_without_the_note_or_without_saar_forces_there() {
        let mut state = game();
        let (their_system, their_planet) = home_planet(&state, &b());
        crate::fixtures::put_on_planet(
            &mut state,
            &their_system,
            &their_planet,
            "infantry",
            &a(),
            1,
        );
        let pairs = [
            ("player", "b"),
            ("system", their_system.as_str()),
            ("planet", their_planet.as_str()),
        ];
        // No note.
        let before = state.board.clone();
        emit(&mut state, &mut scripted(&[]), "UNITS_COMMITTED", &pairs);
        assert_eq!(state.board, before);
        // Note held, but no Saar ground force on the planet.
        let mut state = game();
        state.promissory_notes.insert(RAGH_NOTE.to_owned(), b());
        let before = state.board.clone();
        emit(&mut state, &mut scripted(&[]), "UNITS_COMMITTED", &pairs);
        assert_eq!(state.board, before);
    }

    #[test]
    fn the_hero_destroys_other_players_infantry_and_fighters_next_to_a_dock_only() {
        let mut state = game();
        let hub = crate::fixtures::plain_hub();
        let dock = SystemId::new(hub.outer[0].clone());
        let target = SystemId::new(hub.centre.clone());
        let far = SystemId::new(hub.across(&hub.outer[0]));
        state
            .system_mut(&dock)
            .units
            .push(Unit::new(UnitTypeId::new("saar_spacedock"), a()));
        let planet = PlanetId::new("hero_test_planet");
        for system in [&target, &far] {
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new("fighter"), b()));
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new("infantry"), b()));
            crate::fixtures::put_on_planet(&mut state, system, &planet, "infantry", &b(), 1);
            crate::fixtures::put_on_planet(&mut state, system, &planet, "infantry", &a(), 1);
        }
        let mut table = scripted(&[target.as_str()]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |context| use_leader(context, &a(), &LeaderId::new("saarhero")),
        );
        assert_eq!(done, Some(true));
        let board = state.system_state(&target);
        assert!(
            board.units.iter().all(|u| u.type_id.as_str() != "fighter"),
            "the fighter is destroyed"
        );
        assert_eq!(
            board.units.len(),
            3,
            "the carriers and destroyer are not fighters"
        );
        assert!(board.on_planet_of(&planet, &b()).is_empty());
        assert_eq!(
            board.on_planet_of(&planet, &a()).len(),
            1,
            "Saar's own stay"
        );
        let board = state.system_state(&far);
        assert_eq!(
            board.units.len(),
            2,
            "a system not adjacent to a dock is untouched"
        );
        assert_eq!(board.on_planet_of(&planet, &b()).len(), 1);
    }

    #[test]
    fn deorbit_barrage_pays_rolls_resolves_hits_and_readies_in_status() {
        let content = ContentStore::embedded();
        let field = asteroid_field();
        let hub = crate::fixtures::hub_with_centre(field.as_str());
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::BreakthroughId::new("saarbt"));
        state
            .system_mut(&field)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        let target_system = SystemId::new(
            hub.galaxy
                .adjacent(field.as_str())
                .iter()
                .next()
                .expect("an asteroid tile has a neighboring system")
                .to_owned(),
        );
        let tile = ti4_content::galaxy::system(content, target_system.as_str(), DEFAULT).unwrap();
        let planet = PlanetId::new(tile.planets()[0]);
        state
            .system_mut(&target_system)
            .planet_units
            .insert(planet.clone(), Vec::new());
        crate::fixtures::put_on_planet(&mut state, &target_system, &planet, "mech", &b(), 1);

        assert_eq!(
            component_actions(&state, content, DEFAULT, &hub.galaxy, &a()).len(),
            1
        );
        state.system_mut(&field).units.clear();
        assert!(component_actions(&state, content, DEFAULT, &hub.galaxy, &a()).is_empty());
        state
            .system_mut(&field)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));

        let mut table = scripted(&["invalid_spend"]);
        let refused = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                perform_component(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(DEORBIT_ACTION, "component", ""),
                )
            },
        );
        assert!(
            !refused,
            "an invalid spend answer does not consume the action"
        );
        assert!(!state.faction_marks.contains_key(&deorbit_exhausted(&a())));

        let target_id = format!("{target_system}|{planet}");
        let mut zero_spend_state = state.clone();
        let mut zero_table = scripted(&["0", &target_id]);
        let zero_done = crate::fixtures::with_context(
            &mut zero_spend_state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut zero_table,
            |ctx| {
                perform_component(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(DEORBIT_ACTION, "component", ""),
                )
            },
        );
        assert!(
            zero_done,
            "the printed 'any amount' includes zero resources"
        );
        assert_eq!(trade_goods(&zero_spend_state, &a()), 0);
        assert!(
            zero_spend_state
                .faction_marks
                .contains_key(&deorbit_exhausted(&a()))
        );

        state.player_mut(&a()).unwrap().trade_goods = 1;
        let mut table = scripted(&["1", &target_id, "trade_good"]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                perform_component(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(DEORBIT_ACTION, "component", ""),
                )
            },
        );
        assert!(done);
        assert_eq!(
            trade_goods(&state, &a()),
            0,
            "the selected amount is paid once"
        );
        assert!(state.faction_marks.contains_key(&deorbit_exhausted(&a())));
        assert_eq!(crate::supply::staged_events(&state), 1);
        assert!(component_actions(&state, content, DEFAULT, &hub.galaxy, &a()).is_empty());

        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut dice = crate::dice::Dice::from_faces([10]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut table = scripted(&["0", "sustain"]);
        let mut resolving = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&hub.galaxy),
            }),
        };
        assert_eq!(
            crate::supply::flush_staged_events(&mut state, &mut resolving),
            1
        );
        assert_eq!(crate::supply::staged_events(&state), 0);
        drop(resolving);
        let roll = dice.rolled("saarbt")[0];
        assert_eq!(roll.faces.len(), 1);
        let mech = state
            .system_state(&target_system)
            .on_planet_of(&planet, &b())[0]
            .clone();
        assert_eq!(
            mech.sustained_damage,
            roll.faces[0] >= 4,
            "decisions={:?}; resolver={:?}",
            table.log.records,
            resolver.log(),
        );

        emit(&mut state, &mut scripted(&[]), "STATUS_PHASE_ENDED", &[]);
        assert!(!state.faction_marks.contains_key(&deorbit_exhausted(&a())));
    }

    #[test]
    fn deorbit_hits_choose_each_ground_force_and_owner_handles_sustain() {
        let content = ContentStore::embedded();
        let field = asteroid_field();
        let hub = crate::fixtures::hub_with_centre(field.as_str());
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "hacan")], DEFAULT);
        state.phase = ti4_model::state::Phase::Action;
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::BreakthroughId::new("saarbt"));
        state.player_mut(&a()).unwrap().trade_goods = 2;
        let controlled: Vec<_> = state
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        for planet in controlled {
            state.exhausted_planets.insert(planet);
        }
        state
            .system_mut(&field)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        let target_system = SystemId::new(
            hub.galaxy
                .adjacent(field.as_str())
                .iter()
                .next()
                .unwrap()
                .to_owned(),
        );
        let tile = ti4_content::galaxy::system(content, target_system.as_str(), DEFAULT).unwrap();
        let planet = PlanetId::new(tile.planets()[0]);
        state
            .system_mut(&target_system)
            .planet_units
            .insert(planet.clone(), Vec::new());
        crate::fixtures::put_on_planet(&mut state, &target_system, &planet, "infantry", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &target_system, &planet, "mech", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &target_system, &planet, "mech", &c(), 1);

        let target_id = format!("{target_system}|{planet}");
        let mut table = scripted(&["2", &target_id]);
        assert!(crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                perform_component(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(DEORBIT_ACTION, "component", ""),
                )
            }
        ));
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut dice = crate::dice::Dice::from_faces([10, 10]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let (decider, offered) =
            crate::choice::Capturing::new(Box::new(crate::choice::Scripted::new(
                ["1", "sustain", "2", "sustain"]
                    .iter()
                    .map(|s| (*s).to_owned()),
            )));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut resolving = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&hub.galaxy),
            }),
        };
        assert_eq!(
            crate::supply::flush_staged_events(&mut state, &mut resolving),
            1
        );
        drop(resolving);
        // The sustain question is an offer card: the unit, where it is and what each answer does
        // (display only; the option ids stay `sustain` and `decline`).
        let asked = offered.borrow();
        let sustain = asked
            .iter()
            .find(|choice| {
                choice
                    .context
                    .as_ref()
                    .is_some_and(|context| context.subtype == "ground_effect_sustain")
            })
            .expect("a sustain question was asked");
        assert_eq!(sustain.details["kind"], "offer");
        assert_eq!(sustain.details["card"]["title"], "Sustain damage");
        assert_eq!(sustain.details["facts"][0]["unit"], "mech");
        assert_eq!(sustain.details["facts"][1]["planet"], planet.as_str());
        assert_eq!(
            sustain.details["captions"]["sustain"]["label"],
            "Sustain damage"
        );
        assert_eq!(
            sustain
                .options
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            ["sustain", "decline"]
        );
        drop(asked);
        let units = state
            .system_state(&target_system)
            .on_planet(&planet)
            .to_vec();
        assert_eq!(
            units
                .iter()
                .filter(|unit| unit.owner == b() && unit.type_id.as_str() == "infantry")
                .count(),
            1,
            "the first hit chose B's mech, leaving B's cheaper infantry in place"
        );
        let diagnostics = format!(
            "decisions={:?}; resolver={:?}",
            table.log.records,
            resolver.log()
        );
        assert!(
            units.iter().any(|unit| unit.owner == b()
                && unit.type_id.as_str() == "mech"
                && unit.sustained_damage),
            "B's mech should sustain damage; {diagnostics}"
        );
        assert!(
            units.iter().any(|unit| unit.owner == c()
                && unit.type_id.as_str() == "mech"
                && unit.sustained_damage),
            "the second hit chose C's mech; owners decide their own sustain damage; {diagnostics}"
        );
    }

    #[test]
    fn deorbit_invalid_resource_payment_leaves_the_action_state_unchanged() {
        let content = ContentStore::embedded();
        let field = asteroid_field();
        let hub = crate::fixtures::hub_with_centre(field.as_str());
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::BreakthroughId::new("saarbt"));
        state.player_mut(&a()).unwrap().trade_goods = 2;
        state
            .system_mut(&field)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        let target_system = SystemId::new(
            hub.galaxy
                .adjacent(field.as_str())
                .iter()
                .next()
                .unwrap()
                .to_owned(),
        );
        let tile = ti4_content::galaxy::system(content, target_system.as_str(), DEFAULT).unwrap();
        let planet = PlanetId::new(tile.planets()[0]);
        crate::fixtures::put_on_planet(&mut state, &target_system, &planet, "infantry", &b(), 1);
        let before = state.clone();
        let mut table = scripted(&[
            "2",
            &format!("{target_system}|{planet}"),
            "trade_good",
            "not-a-payment",
        ]);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            Some(&hub.galaxy),
            &mut table,
            |ctx| {
                perform_component(
                    ctx,
                    &a(),
                    &ChoiceOption::labelled(DEORBIT_ACTION, "component", ""),
                )
            },
        );
        assert!(!done);
        assert_eq!(
            state, before,
            "payment validation precedes spending and exhaustion"
        );
    }

    fn asteroid_field() -> SystemId {
        let content = ContentStore::embedded();
        (1..=100)
            .map(|n| n.to_string())
            .find(|id| {
                ti4_content::galaxy::system(content, id, DEFAULT)
                    .is_some_and(|tile| tile.is_asteroid_field())
            })
            .map(SystemId::new)
            .expect("an asteroid field tile")
    }

    #[test]
    fn chaos_mapping_bars_other_players_from_an_asteroid_field_holding_a_cm_ship() {
        let content = ContentStore::embedded();
        let mut state = game();
        let field = asteroid_field();
        state
            .system_mut(&field)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        let barred = |state: &GameState, who: &PlayerId| {
            chaos_mapping_blocks_activation(state, content, DEFAULT, who, &field)
        };
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .remove(&ti4_model::id::TechnologyId::new("cm"));
        assert!(!barred(&state, &b()), "no technology");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("cm"));
        assert!(barred(&state, &b()));
        assert!(!barred(&state, &a()), "not the owner");
        state.system_mut(&field).units.clear();
        assert!(!barred(&state, &b()), "no ship left");
    }

    #[test]
    fn nomadic_scores_without_the_home_planets_and_others_still_need_them() {
        let content = ContentStore::embedded();
        let mut state = game();
        for who in [a(), b()] {
            let held: Vec<(SystemId, PlanetId)> = state
                .controlled_planets(&who)
                .into_iter()
                .map(|(s, p)| (s.clone(), p.clone()))
                .collect();
            for (system, planet) in held {
                state.system_mut(&system).planet_control.remove(&planet);
            }
        }
        let home = |state: &GameState, who: &PlayerId| {
            crate::objectives::controls_home_system(&crate::objectives::Position::new(
                state, content, DEFAULT, who,
            ))
        };
        assert!(home(&state, &a()), "Nomadic");
        assert!(!home(&state, &b()), "61.16 still binds Sol");
    }

    #[test]
    fn floating_factories_move_as_ships_and_a_blockade_destroys_them() {
        let content = ContentStore::embedded();
        let types = catalogue(content, DEFAULT);
        let ffac2 = types.get("saar_spacedock2").expect("ffac2 unit");
        assert!(ffac2.moves_as_ship());
        assert_eq!(ffac2.move_value(), 2);
        let mut state = game();
        let hub = crate::fixtures::plain_hub();
        let origin = SystemId::new(hub.outer[0].clone());
        let active = SystemId::new(hub.centre.clone());
        state
            .system_mut(&origin)
            .units
            .push(Unit::new(UnitTypeId::new("saar_spacedock"), a()));
        let moves =
            crate::tactical::movable_into(&state, content, DEFAULT, &hub.galaxy, &a(), &active);
        assert!(
            moves
                .iter()
                .any(|m| m.origin == origin && m.unit.type_id.as_str() == "saar_spacedock"),
            "the dock is offered as a mover"
        );
        state
            .system_mut(&origin)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()));
        let gone = crate::production::destroy_blockaded_mobile_docks(
            &mut state, content, DEFAULT, &origin,
        );
        assert_eq!(gone.len(), 1);
    }

    #[test]
    fn the_agent_raises_one_ship_to_the_fastest_move_on_the_board() {
        let mut state = game();
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let origin = SystemId::new(hub.outer[0].clone());
        state
            .system_mut(&origin)
            .units
            .push(Unit::new(UnitTypeId::new("carrier"), b()));
        state
            .system_mut(&SystemId::new(hub.outer[1].clone()))
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), a()));
        state.activation_seq = 7;
        let ships = agent_ships(&state, content, DEFAULT, &b());
        let n = ships
            .iter()
            .position(|(system, ..)| system == &origin)
            .expect("the carrier is slower");
        let (_, index, bonus, _) = ships[n].clone();
        let types = catalogue(content, DEFAULT);
        let carrier = types.get("carrier").unwrap();
        let (pb, o) = (b(), origin.clone());
        let site = |idx: Option<usize>| MoveSite {
            player: &pb,
            origin: &o,
            index: idx,
            ship: carrier,
        };
        assert_eq!(
            agent_move_bonus(&state, &site(Some(index))),
            0,
            "nothing chosen yet"
        );
        emit(
            &mut state,
            &mut scripted(&[
                "leader:saar:saaragent:SYSTEM_ACTIVATED:after",
                &format!("ship|{n}"),
            ]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", hub.centre.as_str())],
        );
        assert_eq!(
            leader_status(&state, &a(), "saaragent"),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            i64::from(agent_move_bonus(&state, &site(Some(index)))),
            bonus
        );
        assert_eq!(
            agent_move_bonus(&state, &site(None)),
            0,
            "needs the ship index"
        );
        state.activation_seq = 8;
        assert_eq!(
            agent_move_bonus(&state, &site(Some(index))),
            0,
            "next activation"
        );
    }

    #[test]
    fn the_agent_is_not_offered_exhausted_or_in_a_game_without_saar() {
        let mut state = game();
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("saaragent"), LeaderStatus::Exhausted);
        emit(
            &mut state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", "18")],
        );
        assert!(state.faction_marks.is_empty());
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        emit(
            &mut state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", "18")],
        );
        assert!(state.faction_marks.is_empty());
    }
    /// Destroyer, carrier A and carrier B two hexes from the active system; Mendosa boosts the last
    /// carrier, then the destroyer leaves. Returns the carriers `movable_into` offers afterwards.
    fn carriers_offered_after_the_destroyer_leaves(rules_version: u32) -> Vec<usize> {
        let mut state = game();
        state.rules_version = rules_version;
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let origin = SystemId::new(hub.outer[0].clone());
        let active = SystemId::new(hub.centre.clone());
        let far = SystemId::new(hub.outer[3].clone());
        crate::fixtures::put(&mut state, &origin, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &origin, "carrier", &b(), 2);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[1].clone()),
            "cruiser",
            &a(),
            1,
        );
        state.activation_seq = 11;
        let _ = active;
        let ships = agent_ships(&state, content, DEFAULT, &b());
        let boosted = ships
            .iter()
            .enumerate()
            .filter(|(_, (system, ..))| system == &origin)
            .filter(|(_, (_, i, ..))| {
                state.ships_of(&b(), &origin)[*i].type_id.as_str() == "carrier"
            })
            .last()
            .map(|(n, _)| n)
            .expect("a carrier is slower");
        emit(
            &mut state,
            &mut scripted(&[
                "leader:saar:saaragent:SYSTEM_ACTIVATED:after",
                &format!("ship|{boosted}"),
            ]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", far.as_str())],
        );
        let offered = |state: &GameState| -> Vec<usize> {
            crate::tactical::movable_into(state, content, DEFAULT, &hub.galaxy, &b(), &far)
                .iter()
                .filter(|m| m.origin == origin && m.unit.type_id.as_str() == "carrier")
                .map(|m| m.index)
                .collect()
        };
        assert_eq!(
            offered(&state).len(),
            1,
            "one carrier is boosted into reach"
        );
        // The destroyer sails first.
        let destroyer = state.ships_of(&b(), &origin)[0].clone();
        state
            .system_mut(&origin)
            .remove(std::slice::from_ref(&destroyer));
        offered(&state)
    }

    #[test]
    fn mendosas_boost_stays_with_its_ship_when_an_earlier_ship_leaves() {
        // Carriers are now at positions 0 and 1; the boosted one is still offered, once.
        assert_eq!(
            carriers_offered_after_the_destroyer_leaves(ti4_model::state::RULES_VERSION),
            vec![0]
        );
    }

    #[test]
    fn a_legacy_game_keeps_the_position_based_boost() {
        // Version 0: the boost was stored against position 2, which is empty now.
        assert!(carriers_offered_after_the_destroyer_leaves(0).is_empty());
    }

    #[test]
    fn ssruu_borrows_captain_mendosa_on_the_activation_and_boosts_only_the_selected_ship() {
        let mut state = game();
        let content = ContentStore::embedded();
        let hub = crate::fixtures::plain_hub();
        let carrier_origin = SystemId::new(hub.outer[0].clone());
        let other_origin = SystemId::new(hub.outer[1].clone());
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("saaragent"), LeaderStatus::Exhausted);
        crate::fixtures::put(&mut state, &carrier_origin, "carrier", &b(), 1);
        crate::fixtures::put(&mut state, &other_origin, "cruiser", &b(), 1);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &a(),
            1,
        );
        state.activation_seq = 37;
        let types = catalogue(content, DEFAULT);
        let carrier = types.get("carrier").expect("carrier");
        let ships = agent_ships(&state, content, DEFAULT, &b());
        let selected = ships
            .iter()
            .position(|(system, ..)| system == &carrier_origin)
            .unwrap();
        let (pb, origin) = (b(), carrier_origin.clone());
        let site = MoveSite {
            player: &pb,
            origin: &origin,
            index: Some(ships[selected].1),
            ship: carrier,
        };
        assert_eq!(agent_move_bonus(&state, &site), 0, "before activation");
        emit(
            &mut state,
            &mut scripted(&[
                "leader:saar:a:yssarilagent:b:saaragent:SYSTEM_ACTIVATED:after",
                &format!("ship|{selected}"),
            ]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", hub.centre.as_str())],
        );
        assert_eq!(
            leader_status(&state, &b(), "yssarilagent"),
            Some(LeaderStatus::Exhausted)
        );
        assert_eq!(
            leader_status(&state, &a(), "saaragent"),
            Some(LeaderStatus::Exhausted),
            "borrow never changes source status"
        );
        let (_, _, gap, _) = &ships[selected];
        assert_eq!(i64::from(agent_move_bonus(&state, &site)), *gap);
        let other = MoveSite {
            player: &pb,
            origin: &other_origin,
            index: Some(0),
            ship: types.get("cruiser").unwrap(),
        };
        assert_eq!(
            agent_move_bonus(&state, &other),
            0,
            "the copied bonus is scoped to the selected ship"
        );
        state.activation_seq += 1;
        assert_eq!(
            agent_move_bonus(&state, &site),
            0,
            "the copied bonus expires with its activation"
        );
    }

    #[test]
    fn ssruu_captain_mendosa_listener_tracks_source_and_borrower_readiness_live() {
        let content = ContentStore::embedded();
        let mut state = game();
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("yssaril");
        let agent = LeaderId::new("saaragent");
        let ssruu = LeaderId::new("yssarilagent");
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(agent.clone(), LeaderStatus::Readied);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(ssruu.clone(), LeaderStatus::Readied);
        assert!(has_borrowable_saar_agent(&state, content, &a(), &b()));
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(agent.clone(), LeaderStatus::Exhausted);
        assert!(
            has_borrowable_saar_agent(&state, content, &a(), &b()),
            "an exhausted other-seat source is still borrowable"
        );
        let hub = crate::fixtures::plain_hub();
        let origin = SystemId::new(hub.outer[0].clone());
        crate::fixtures::put(&mut state, &origin, "carrier", &b(), 1);
        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[1].clone()),
            "cruiser",
            &a(),
            1,
        );
        state.player_mut(&a()).unwrap().leaders.remove(&agent);
        assert!(
            !has_borrowable_saar_agent(&state, content, &a(), &b()),
            "source removal disables the already-armed listener"
        );
        emit(
            &mut state,
            &mut scripted(&[]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", hub.centre.as_str())],
        );
        assert!(
            state.faction_marks.is_empty(),
            "actual activation has no copied effect without its source"
        );
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(agent.clone(), LeaderStatus::Readied);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(ssruu.clone(), LeaderStatus::Exhausted);
        assert!(
            !has_borrowable_saar_agent(&state, content, &a(), &b()),
            "an exhausted Ssruu disables the listener"
        );
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", hub.centre.as_str())],
        );
        assert!(
            state.faction_marks.is_empty(),
            "actual activation stays inert when Ssruu is exhausted"
        );
    }
}
