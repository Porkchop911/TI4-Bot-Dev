//! The Argent Flight (`argent`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-argent.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Zeal: "You always vote first during the agenda phase. When you cast at least 1 vote, cast 1
//!   additional vote for each player in the game including you." (PARTIAL: the extra votes only.)
//! * Raid Formation: "When 1 or more of your units uses ANTI-FIGHTER BARRAGE: for each hit
//!   produced in excess of your opponent's fighters, choose 1 of your opponent's ships that has
//!   SUSTAIN DAMAGE to become damaged."
//! * Strike Wing Alpha II (`swa2`): "When this unit uses ANTI-FIGHTER BARRAGE, each result of 9 or
//!   10 also destroys 1 of your opponent's infantry in the space area of the active system."
//! * Strike Wing Ambuscade (`ambuscade`): "When 1 or more of your units make a roll for a unit
//!   ability: Choose 1 of those units to roll 1 additional die. Then, return this card to the
//!   Argent player."
//! * Trrakan Aun Zulok (`argentcommander`): "When 1 or more of your units make a roll for a unit
//!   ability: You may choose 1 of those units to roll 1 additional die." Unlock: "Have 6 units
//!   that have ANTI-FIGHTER BARRAGE, SPACE CANNON, or BOMBARDMENT on the game board."
//!
//! Not implemented (no route; see the evidence file): the mech's "transported" clause.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_cards::CardHooks;
use super::hooks_combat::{AfbExcess, CombatHooks};
use super::hooks_economy::EconomyHooks;
use super::hooks_movement::MovementHooks;
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption, IllegalChoice};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

const FACTION: &str = "argent";
/// The Ambuscade note's id: the printed alias and the Argent player's faction name.
const AMBUSCADE_NOTE: &str = "ambuscade:argent";
/// The only unit whose barrage Strike Wing Alpha II's text belongs to.
const SWA2_UNIT: &str = "argent_destroyer2";

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["raid_formation", "zeal"],
    technologies: &["swa2", "ah"],
    // `argent_destroyer` is statistics only (verified data-driven by a test).
    units: &[
        "argent_destroyer",
        "argent_destroyer2",
        "argent_flagship",
        "argent_mech",
    ],
    promissory: &["ambuscade"],
    leaders: &["argentagent", "argentcommander", "argenthero"],
    breakthroughs: &["argentbt"],
    hooks: Hooks {
        vote_bonus: Some(vote_bonus),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        timing_abilities: Some(timing_abilities),
        combat: CombatHooks {
            afb_excess: Some(afb_excess),
            space_cannon_barred: Some(space_cannon_barred),
            ..CombatHooks::NONE
        },
        movement: MovementHooks {
            blocks_passage: Some(blocks_passage),
            free_cargo: Some(free_cargo),
            ..MovementHooks::NONE
        },
        economy: EconomyHooks {
            extra_production_planet: Some(extra_production_planet),
            production_destinations: Some(agent_production_destinations),
            ..EconomyHooks::NONE
        },
        cards: CardHooks {
            votes_first: Some(votes_first),
            ..CardHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- small readers -------------------------------------------------------------------------------

fn is_argent(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn decision(state: &GameState, player: &PlayerId, source: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        player.clone(),
        DecisionSource::FactionAbility(source.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

// -- Zeal ----------------------------------------------------------------------------------------

/// Zeal: "When you cast at least 1 vote, cast 1 additional vote for each player in the game
/// including you." `vote::record` adds the bonus only to votes actually cast, so a seat that
/// casts nothing gains nothing.
fn vote_bonus(state: &GameState, player: &PlayerId) -> i64 {
    if is_argent(state, player) {
        i64::try_from(state.players.len()).unwrap_or(0)
    } else {
        0
    }
}

/// Zeal: "You always vote first during the agenda phase."
fn votes_first(state: &GameState, player: &PlayerId) -> bool {
    is_argent(state, player)
}

// -- the flagship, the mech and Aerie Hololattice --------------------------------------------

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn owns_technology(state: &GameState, player: &PlayerId, technology: &str) -> bool {
    crate::technology::has_technology_text(state, player, technology)
}

/// Quetzecoatl: "Other players cannot use SPACE CANNON against your ships in this system."
fn space_cannon_barred(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    shooter: &PlayerId,
    target_owner: &PlayerId,
    system: &SystemId,
) -> bool {
    shooter != target_owner
        && super::has_flagship_text_in(state, target_owner, system, "argent_flagship")
}

/// Aerie Sentinel: "This unit does not count against capacity if it is being transported ..."
/// (the space-area clause is the mech's `capacityUsed 0`).
fn free_cargo(
    _state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    unit: &Unit,
) -> bool {
    unit.type_id.as_str() == "argent_mech"
}

/// Whether `player` has a structure of theirs in the system (space area or any planet).
fn structure_in(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: Option<&PlanetId>,
) -> bool {
    let types = catalogue(content, sources);
    let is_structure = |unit: &&Unit| {
        &unit.owner == player
            && types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_structure)
    };
    let board = state.system_state(system);
    match planet {
        Some(planet) => board.on_planet(planet).iter().any(|u| is_structure(&u)),
        None => {
            board.units.iter().any(|u| is_structure(&u))
                || board
                    .planet_units
                    .values()
                    .any(|units| units.iter().any(|u| is_structure(&u)))
        }
    }
}

/// Aerie Hololattice: "Other players cannot move ships through systems that contain your
/// structures."
fn blocks_passage(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    mover: &PlayerId,
    system: &SystemId,
) -> bool {
    state.players.iter().any(|seat| {
        &seat.id != mover
            && owns_technology(state, &seat.id, "ah")
            && structure_in(state, content, sources, &seat.id, system, None)
    })
}

/// Aerie Hololattice: "Each planet that contains 1 or more of your structures gains the
/// PRODUCTION 1 ability as if it were a unit."
fn extra_production_planet(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> i64 {
    i64::from(
        owns_technology(state, player, "ah")
            && structure_in(state, content, sources, player, system, Some(planet)),
    )
}

fn agent_destination_key(player: &PlayerId, system: &SystemId) -> String {
    format!("argent:argentagent:production:{}:{}", player, system)
}

/// Destinations granted by Argent's agent for the in-progress production. The grant is a
/// one-build mark, removed by `ProductionWindow` after the batch is placed or abandoned.
fn agent_production_destinations(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    unit_base: &str,
) -> Vec<(SystemId, Option<PlanetId>)> {
    let types = catalogue(content, sources);
    if !types
        .get(unit_base)
        .is_some_and(|kind| kind.is_ground_force() && !kind.is_structure())
    {
        return Vec::new();
    }
    let Some(destinations) = state
        .faction_marks
        .get(&agent_destination_key(player, system))
    else {
        return Vec::new();
    };
    destinations
        .split(';')
        .filter_map(|entry| {
            let (target, planet) = entry.split_once('@')?;
            let target = SystemId::new(target);
            let planet = PlanetId::new(planet);
            let still_controlled = state
                .system_state(&target)
                .planet_control
                .get(&planet)
                .is_some_and(|owner| owner == player);
            (target != *system
                && still_controlled
                && !crate::laws::planet_is_demilitarized(state, &planet)
                && !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources))
            .then_some((target, Some(planet)))
        })
        .collect()
}

pub(crate) fn clear_agent_production_destinations(
    state: &mut GameState,
    player: &PlayerId,
    system: &SystemId,
) {
    state
        .faction_marks
        .remove(&agent_destination_key(player, system));
}

// -- Raid Formation ------------------------------------------------------------------------------

/// Undamaged ships of `owner` in the space area that have SUSTAIN DAMAGE, by type (a damaged ship
/// cannot "become damaged"; two undamaged ships of one type are interchangeable).
fn damageable_types(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    system: &SystemId,
) -> Vec<String> {
    let types = catalogue(content, sources);
    let mut found: Vec<String> = Vec::new();
    for unit in &state.system_state(system).units {
        if &unit.owner != owner || unit.sustained_damage {
            continue;
        }
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let has_sustain = kind.sustain_damage()
            || (crate::relics::grants_sustain(state, owner) && !kind.is_fighter());
        if kind.is_ship()
            && has_sustain
            && !crate::laws::sustain_suppressed(state, kind.base_type())
            && !found.iter().any(|seen| seen == unit.type_id.as_str())
        {
            found.push(unit.type_id.to_string());
        }
    }
    found
}

/// Raid Formation, called for every barrage whose hits outnumbered the target's fighters.
fn afb_excess(context: &mut TimingContext<'_>, site: &AfbExcess<'_>) -> Result<(), IllegalChoice> {
    if !is_argent(context.state, site.producer) {
        return Ok(());
    }
    for _ in 0..site.excess {
        let candidates = damageable_types(
            context.state,
            context.content,
            context.sources,
            site.target,
            site.system,
        );
        let chosen = match candidates.as_slice() {
            [] => return Ok(()),
            [only] => only.clone(),
            many => {
                let options = many
                    .iter()
                    .map(|kind| {
                        ChoiceOption::labelled(
                            kind.clone(),
                            "raid_formation_damage",
                            format!("damage 1 {kind} of {}", site.target),
                        )
                    })
                    .collect();
                let choice = Choice::new(
                    site.producer.clone(),
                    "Raid Formation: choose a ship with SUSTAIN DAMAGE to become damaged"
                        .to_owned(),
                    options,
                )
                .contextualized(decision(
                    context.state,
                    site.producer,
                    "raid_formation",
                    "raid_formation_damage",
                ));
                context.ask_seeing(&choice)?.id
            }
        };
        if let Some(unit) = context
            .state
            .system_mut(site.system)
            .units
            .iter_mut()
            .find(|unit| {
                &unit.owner == site.target
                    && !unit.sustained_damage
                    && unit.type_id.as_str() == chosen
            })
        {
            unit.sustained_damage = true;
        }
    }
    Ok(())
}

// -- the commander's unlock ----------------------------------------------------------------------

/// "Have 6 units that have ANTI-FIGHTER BARRAGE, SPACE CANNON, or BOMBARDMENT on the game board."
fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "argentcommander" {
        return None;
    }
    let types = catalogue(content, sources);
    let armed = |unit: &&Unit| {
        &unit.owner == player
            && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                kind.has_anti_fighter_barrage() || kind.has_space_cannon() || kind.has_bombardment()
            })
    };
    let count: usize = state
        .board
        .values()
        .map(|board| {
            board.units.iter().filter(armed).count()
                + board
                    .planet_units
                    .values()
                    .map(|units| units.iter().filter(armed).count())
                    .sum::<usize>()
        })
        .sum();
    Some(count >= 6)
}

// -- the hero ------------------------------------------------------------------------------------

/// Systems Flock Migration may send ships to: they hold the player's command token and no other
/// player's ships.
fn hero_destinations(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    state
        .systems_with_token(player)
        .into_iter()
        .filter(|system| {
            crate::combat::opponent_with_ships(state, content, sources, player, system).is_none()
        })
        .cloned()
        .collect()
}

/// Each distinct ship (system, type, damage) the hero could lift.
fn hero_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(SystemId, Unit)> {
    let types = catalogue(content, sources);
    let mut found: Vec<(SystemId, Unit)> = Vec::new();
    for (system, board) in &state.board {
        for unit in board.units_of(player) {
            if types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_ship)
                && !found.iter().any(|(at, seen)| at == system && seen == unit)
            {
                found.push((system.clone(), unit.clone()));
            }
        }
    }
    found
}

fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "argenthero" {
        return None;
    }
    let sources = SourceSet::all();
    let destinations = hero_destinations(state, content, sources, player);
    Some(
        is_argent(state, player)
            && hero_ships(state, content, sources, player)
                .iter()
                .any(|(from, _)| destinations.iter().any(|to| to != from)),
    )
}

fn ask_option(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    prompt: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, IllegalChoice> {
    ask_option_for(context, player, "argenthero", prompt, subtype, options)
}

fn ask_option_for(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    source: &str,
    prompt: &str,
    subtype: &str,
    mut options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, IllegalChoice> {
    options.push(ChoiceOption::decline());
    let choice = Choice::new(player.clone(), prompt.to_owned(), options).contextualized(decision(
        context.state,
        player,
        source,
        subtype,
    ));
    context.ask_seeing(&choice)
}

/// Mirik Aun Sissiri, Helix Protocol: Flock Migration. "ACTION: Move any number of your ships from
/// any systems to any number of other systems that contain 1 of your command tokens and no other
/// players' ships. Then, purge this card." One ship (with its cargo) per step; declining the first
/// step leaves everything untouched, declining a later one ends the move. Cargo is not landed
/// (it is set down in the destination's space area).
fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "argenthero" || !is_argent(context.state, player) {
        return None;
    }
    let Some(galaxy) = context.galaxy else {
        return Some(false);
    };
    Some(migrate(
        context,
        player,
        galaxy,
        None,
        true,
        "flock_migration",
    ))
}

/// The move shared by the hero and the breakthrough: ships, one at a time, to systems that hold
/// the player's command token and no other player's ships. `scope` limits sources and
/// destinations to those systems (the breakthrough's "active system and adjacent systems").
/// Returns whether any ship moved.
#[expect(
    clippy::too_many_lines,
    reason = "one pass of questions, then the move"
)]
fn migrate(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    galaxy: &ti4_content::galaxy::Galaxy,
    scope: Option<&[SystemId]>,
    with_cargo: bool,
    reason: &'static str,
) -> bool {
    let mut moved = false;
    // Ships that have already moved in this migration, by where they arrived: "move any number of
    // your ships" moves each ship once. Re-offering an arrived ship let a bot shuttle one ship
    // between two systems for ever (BF-22 wide-roster evaluation). Ships carry no identity, so a
    // stack is offered while it holds more copies than have arrived there.
    let mut arrived: Vec<(SystemId, Unit)> = Vec::new();
    loop {
        let mut destinations =
            hero_destinations(context.state, context.content, context.sources, player);
        if let Some(scope) = scope {
            destinations.retain(|system| scope.contains(system));
        }
        let ships: Vec<(SystemId, Unit)> =
            hero_ships(context.state, context.content, context.sources, player)
                .into_iter()
                .filter(|(from, _)| scope.is_none_or(|scope| scope.contains(from)))
                .filter(|(from, _)| destinations.iter().any(|to| to != from))
                .filter(|(from, unit)| {
                    let here = context
                        .state
                        .system_state(from)
                        .units_of(player)
                        .into_iter()
                        .filter(|other| *other == unit)
                        .count();
                    let came = arrived
                        .iter()
                        .filter(|(at, other)| at == from && other == unit)
                        .count();
                    here > came
                })
                .collect();
        if ships.is_empty() {
            break;
        }
        let options = ships
            .iter()
            .enumerate()
            .map(|(index, (from, unit))| {
                ChoiceOption::labelled(
                    format!("ship|{index}"),
                    "flock_migration_ship",
                    format!("move {} from {from}", unit.type_id),
                )
            })
            .collect();
        let Ok(answer) = ask_option(
            context,
            player,
            "Flock Migration: choose a ship to move",
            "flock_migration_ship",
            options,
        ) else {
            break;
        };
        let Some((from, ship)) = answer
            .id
            .strip_prefix("ship|")
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| ships.get(n))
            .cloned()
        else {
            break;
        };
        let targets: Vec<SystemId> = destinations.into_iter().filter(|to| to != &from).collect();
        let options = targets
            .iter()
            .map(|to| {
                ChoiceOption::labelled(
                    format!("to|{to}"),
                    "flock_migration_to",
                    format!("move it to {to}"),
                )
            })
            .collect();
        let Ok(answer) = ask_option(
            context,
            player,
            "Flock Migration: choose the destination",
            "flock_migration_to",
            options,
        ) else {
            break;
        };
        let Some(to) = answer.id.strip_prefix("to|").map(SystemId::new) else {
            break;
        };
        if !targets.contains(&to) {
            break;
        }
        // Cargo, one unit at a time, until the ship is full or the player stops.
        let mut offered = crate::transit::loadable(
            context.state,
            context.content,
            context.sources,
            player,
            &from,
        );
        let mut cargo = Vec::new();
        let capacity = crate::transit::capacity_of(context.content, context.sources, &ship);
        while with_cargo
            && !offered.is_empty()
            && i64::try_from(cargo.len()).unwrap_or(i64::MAX) < capacity + 8
        {
            let options = offered
                .iter()
                .enumerate()
                .map(|(index, load)| {
                    ChoiceOption::labelled(
                        format!("cargo|{index}"),
                        "flock_migration_cargo",
                        format!("carry {}", load.unit.type_id),
                    )
                })
                .collect();
            let Ok(answer) = ask_option(
                context,
                player,
                "Flock Migration: carry a unit aboard (it cannot be landed)",
                "flock_migration_cargo",
                options,
            ) else {
                break;
            };
            let Some(index) = answer
                .id
                .strip_prefix("cargo|")
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| *n < offered.len())
            else {
                break;
            };
            cargo.push(offered.remove(index));
        }
        let ships = [ship];
        let relocation = crate::transit::Relocation {
            player,
            from: &from,
            to: &to,
            ships: &ships,
            require_adjacent: false,
            forbid_foreign_ships_at_destination: true,
            refuse_fleet_overflow: false,
            reason,
        };
        let done = crate::transit::relocate_ships_with_cargo(
            context.state,
            context.content,
            context.sources,
            galaxy,
            &relocation,
            &cargo,
        )
        .or_else(|_| {
            crate::transit::relocate_ships(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &relocation,
            )
        });
        if done.is_err() {
            break;
        }
        arrived.push((to, ships[0].clone()));
        moved = true;
    }
    moved
}

// -- the breakthrough ----------------------------------------------------------------------------

/// `GameState::faction_marks` key for the system this player activated with Wing Transfer in play,
/// remembered until `ACTION_COMPLETED` (which names only the player) so the move can be offered.
fn wing_transfer_key(player: &PlayerId) -> String {
    format!("argent:wing_transfer:{player}")
}

/// Whether the system holds at least one unit and every unit in it (space or planets) is
/// `player`'s.
fn only_units_of(state: &GameState, player: &PlayerId, system: &SystemId) -> bool {
    let board = state.system_state(system);
    let all: Vec<&Unit> = board
        .units
        .iter()
        .chain(board.planet_units.values().flatten())
        .collect();
    !all.is_empty() && all.iter().all(|unit| &unit.owner == player)
}

/// Adjacent systems Wing Transfer may put a token in: only the player's units, no token of
/// theirs yet, and one still in reinforcements.
fn wing_transfer_targets(
    state: &GameState,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
    active: &SystemId,
) -> Vec<SystemId> {
    if !only_units_of(state, player, active) {
        return Vec::new();
    }
    galaxy
        .adjacent(active.as_str())
        .into_iter()
        .map(SystemId::new)
        .filter(|system| {
            only_units_of(state, player, system)
                && crate::tokens::can_place_command_token(state, player, system)
        })
        .collect()
}

fn holds_wing_transfer(state: &GameState, player: &PlayerId) -> bool {
    is_argent(state, player) && crate::breakthroughs::holds(state, player, "argentbt")
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass of questions, then the move"
)]
fn wing_transfer_timing(owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    // The two clauses are independent: activating a system that contains only your units earns
    // the end-of-action move whether or not any token is placed. This records the activation; the
    // optional placement below only adds tokens.
    let (marker, marker_seat) = (seat.clone(), seat.clone());
    let mark = Ability::stateful(
        format!("breakthrough:{owner_name}:argentbt_mark:SYSTEM_ACTIVATED:when"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            if let Some(active) = event.text("system") {
                context
                    .state
                    .faction_marks
                    .insert(wing_transfer_key(&marker), active.to_owned());
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(marker_seat.as_str())
            && holds_wing_transfer(context.state, &marker_seat)
            && event.text("system").is_some_and(|active| {
                only_units_of(context.state, &marker_seat, &SystemId::new(active))
            })
    }));
    let (placer, placer_seat) = (seat.clone(), seat.clone());
    let place = Ability::stateful(
        format!("breakthrough:{owner_name}:argentbt:SYSTEM_ACTIVATED:when"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let Some(active) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            wing_transfer_place(context, &placer, &active)
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(placer_seat.as_str())
            && holds_wing_transfer(context.state, &placer_seat)
            && event.text("system").is_some_and(|active| {
                context.galaxy.is_some_and(|galaxy| {
                    !wing_transfer_targets(
                        context.state,
                        galaxy,
                        &placer_seat,
                        &SystemId::new(active),
                    )
                    .is_empty()
                })
            })
    }));

    let (mover, mover_seat) = (seat.clone(), seat.clone());
    let finish = Ability::stateful(
        format!("breakthrough:{owner_name}:argentbt:ACTION_COMPLETED:after"),
        seat.clone(),
        "ACTION_COMPLETED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let Some(active) = context
                .state
                .faction_marks
                .remove(&wing_transfer_key(&mover))
                .map(SystemId::new)
            else {
                return Ok(());
            };
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            let mut scope = vec![active.clone()];
            scope.extend(
                galaxy
                    .adjacent(active.as_str())
                    .into_iter()
                    .map(SystemId::new)
                    .filter(|system| {
                        context
                            .state
                            .system_state(system)
                            .command_tokens
                            .contains(&mover)
                    }),
            );
            if scope.len() > 1 {
                migrate(
                    context,
                    &mover,
                    galaxy,
                    Some(&scope),
                    false,
                    "wing_transfer",
                );
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") == Some(mover_seat.as_str())
            && context
                .state
                .faction_marks
                .contains_key(&wing_transfer_key(&mover_seat))
    }));

    // A turn that ends without `ACTION_COMPLETED` forgets the activation.
    let forget_seat = seat.clone();
    let forget_when = seat.clone();
    let forget = Ability::stateful(
        format!("breakthrough:{owner_name}:argentbt_forget:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context
                .state
                .faction_marks
                .remove(&wing_transfer_key(&forget_seat));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&wing_transfer_key(&forget_when))
    }));
    vec![mark, place, finish, forget]
}

/// Wing Transfer, first half: "When you activate a system that contains only your units, you may
/// place command tokens from your reinforcements into any systems adjacent to that system that
/// contain only your units". Also remembers the activation for the second half.
fn wing_transfer_place(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    active: &SystemId,
) -> Result<(), TimingError> {
    let Some(galaxy) = context.galaxy else {
        return Ok(());
    };
    context
        .state
        .faction_marks
        .insert(wing_transfer_key(player), active.to_string());
    loop {
        let targets = wing_transfer_targets(context.state, galaxy, player, active);
        if targets.is_empty() {
            return Ok(());
        }
        let options = targets
            .iter()
            .map(|system| {
                ChoiceOption::labelled(
                    format!("token|{system}"),
                    "wing_transfer_token",
                    format!("place a command token in {system}"),
                )
            })
            .collect();
        let answer = ask_option_for(
            context,
            player,
            "argentbt",
            "Wing Transfer: place a command token in an adjacent system",
            "wing_transfer_token",
            options,
        )
        .map_err(illegal)?;
        let Some(system) = answer.id.strip_prefix("token|").map(SystemId::new) else {
            return Ok(());
        };
        if !targets.contains(&system)
            || !crate::tokens::place_command_token_from_reinforcements(
                context.state,
                player,
                &system,
            )
        {
            return Ok(());
        }
    }
}

// -- timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        strike_wing(owner_name, seat),
        extra_die(owner_name, seat, Source::Ambuscade),
        extra_die(owner_name, seat, Source::Commander),
    ];
    if state
        .player(seat)
        .is_some_and(|player| player.faction.as_str() == "argent")
    {
        abilities.push(argent_agent(owner_name, seat));
        abilities.extend(wing_transfer_timing(owner_name, seat));
        if state
            .player(seat)
            .is_some_and(|source| source.leaders.contains_key(&LeaderId::new("argentagent")))
        {
            for candidate in &state.players {
                if candidate.id != *seat
                    && candidate
                        .leaders
                        .contains_key(&LeaderId::new("yssarilagent"))
                {
                    abilities.push(borrowed_argent_agent(owner_name, seat, &candidate.id));
                }
            }
        }
    }
    abilities
}

fn agent_ready(state: &GameState, seat: &PlayerId) -> bool {
    state.player(seat).is_some_and(|player| {
        player.leaders.get(&LeaderId::new("argentagent")) == Some(&LeaderStatus::Readied)
    })
}

fn agent_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    recipient: &PlayerId,
    producing: &SystemId,
) -> Vec<(SystemId, PlanetId)> {
    let adjacency =
        crate::movement::PlayerAdjacency::new(state, content, sources, galaxy, recipient);
    let mut allowed: Vec<SystemId> = adjacency
        .neighbours(producing.as_str())
        .into_iter()
        .map(SystemId::new)
        .collect();
    allowed.sort();
    allowed.dedup();
    state
        .controlled_planets(recipient)
        .into_iter()
        .filter(|(system, planet)| {
            allowed.contains(system)
                && !crate::laws::planet_is_demilitarized(state, planet)
                && !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources)
        })
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect()
}

fn argent_agent(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:argentagent:GROUND_FORCES_BEING_PRODUCED:when"),
        seat.clone(),
        "GROUND_FORCES_BEING_PRODUCED",
        Relation::When,
        Arc::new(move |event, _, context| {
            let Some(recipient) = event.text("player").map(PlayerId::new) else {
                return Ok(());
            };
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let Some(galaxy) = context.galaxy else {
                return Ok(());
            };
            let destinations = agent_planets(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &recipient,
                &system,
            );
            if destinations.is_empty()
                || !crate::leaders::exhaust(context.state, &owner, &LeaderId::new("argentagent"))
            {
                return Ok(());
            }
            let encoded = destinations
                .iter()
                .map(|(target, planet)| format!("{target}@{planet}"))
                .collect::<Vec<_>>()
                .join(";");
            context
                .state
                .faction_marks
                .insert(agent_destination_key(&recipient, &system), encoded);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let (Some(recipient), Some(system), Some(count)) = (
            event.text("player"),
            event.text("system"),
            event.integer("count"),
        ) else {
            return false;
        };
        count > 0
            && agent_ready(context.state, &condition_owner)
            && context.galaxy.is_some_and(|galaxy| {
                !agent_planets(
                    context.state,
                    context.content,
                    context.sources,
                    galaxy,
                    &PlayerId::new(recipient),
                    &SystemId::new(system),
                )
                .is_empty()
            })
    }))
}

/// Copy Argent's production destination grant through Ssruu. The producing player remains the
/// target of the printed effect; Ssruu's holder pays the use by exhausting Ssruu itself.
fn borrowed_argent_agent(owner_name: &str, source: &PlayerId, borrower: &PlayerId) -> Ability {
    let (condition_source, condition_borrower) = (source.clone(), borrower.clone());
    let (effect_source, effect_borrower) = (source.clone(), borrower.clone());
    Ability::stateful(
        format!(
            "leader:{owner_name}:{source}:yssarilagent:argentagent:GROUND_FORCES_BEING_PRODUCED:when"
        ),
        borrower.clone(),
        "GROUND_FORCES_BEING_PRODUCED",
        Relation::When,
        Arc::new(move |event, _, context| {
            if !has_borrowable_argent_agent(
                context.state,
                context.content,
                &effect_source,
                &effect_borrower,
            ) {
                return Ok(());
            }
            let (Some(recipient), Some(system), Some(galaxy)) = (
                event.text("player").map(PlayerId::new),
                event.text("system").map(SystemId::new),
                context.galaxy,
            ) else {
                return Ok(());
            };
            let destinations = agent_planets(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &recipient,
                &system,
            );
            if destinations.is_empty()
                || !crate::leaders::exhaust(
                    context.state,
                    &effect_borrower,
                    &LeaderId::new("yssarilagent"),
                )
            {
                return Ok(());
            }
            let encoded = destinations
                .iter()
                .map(|(target, planet)| format!("{target}@{planet}"))
                .collect::<Vec<_>>()
                .join(";");
            context
                .state
                .faction_marks
                .insert(agent_destination_key(&recipient, &system), encoded);
            super::hooks_cards::borrowed_agent_used(
                context,
                &effect_borrower,
                &LeaderId::new("argentagent"),
            );
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let (Some(recipient), Some(system), Some(count), Some(galaxy)) = (
            event.text("player"),
            event.text("system"),
            event.integer("count"),
            context.galaxy,
        ) else {
            return false;
        };
        count > 0
            && has_borrowable_argent_agent(
                context.state,
                context.content,
                &condition_source,
                &condition_borrower,
            )
            && !agent_planets(
                context.state,
                context.content,
                context.sources,
                galaxy,
                &PlayerId::new(recipient),
                &SystemId::new(system),
            )
            .is_empty()
    }))
}

fn has_borrowable_argent_agent(
    state: &GameState,
    content: &ContentStore,
    source: &PlayerId,
    borrower: &PlayerId,
) -> bool {
    super::hooks_cards::borrowable_agents(state, content, borrower)
        .iter()
        .any(|(owner, agent)| owner == source && agent.as_str() == "argentagent")
}

fn is_unit_ability_roll(set: &ti4_model::state::RerollSet) -> bool {
    matches!(
        set.kind.as_str(),
        "anti_fighter_barrage" | "space_cannon" | "bombardment"
    )
}

/// Whether the event is `seat`'s own unit-ability roll.
fn own_roll(event: &Event, seat: &PlayerId) -> bool {
    event.text("player") == Some(seat.as_str())
}

/// Strike Wing Alpha II: each 9 or 10 on its barrage dice also destroys 1 enemy infantry in the
/// space area. Hangs on the roll's AFTER window, where the staged faces are final (rerolls done).
/// A mandatory effect: the dice decide, and the only choice is which infantry type when the
/// opponent has several.
fn strike_wing(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:swa2:UNIT_ABILITY_ROLLED:after"),
        seat.clone(),
        "UNIT_ABILITY_ROLLED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| strike_wing_effect(context, &owner)),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        own_roll(event, &condition_owner)
            && !strike_wing_victims(context, &condition_owner).is_empty()
    }))
}

/// `(system, opponent)` and the results of 9 or 10 on this seat's staged Alpha II barrage.
fn strike_wing_hits(
    context: &TimingContext<'_>,
    seat: &PlayerId,
) -> Option<(SystemId, PlayerId, usize)> {
    let owns_upgrade = context.state.player(seat).is_some_and(|player| {
        player
            .technologies
            .contains(&ti4_model::id::TechnologyId::new("swa2"))
    });
    let set = context.state.reroll_staging.get(seat)?;
    if !owns_upgrade || set.kind != "anti_fighter_barrage" {
        return None;
    }
    let hits: usize = set
        .rolls
        .iter()
        .filter(|entry| entry.unit == SWA2_UNIT)
        .map(|entry| entry.faces.iter().filter(|face| **face >= 9).count())
        .sum();
    if hits == 0 {
        return None;
    }
    let opponent = crate::combat::opponent_with_ships(
        context.state,
        context.content,
        context.sources,
        seat,
        &set.system,
    )?;
    Some((set.system.clone(), opponent, hits))
}

/// The opponent's infantry in the space area, when the seat's barrage earned a kill.
fn strike_wing_victims(context: &TimingContext<'_>, seat: &PlayerId) -> Vec<Unit> {
    let Some((system, opponent, _)) = strike_wing_hits(context, seat) else {
        return Vec::new();
    };
    let types = catalogue(context.content, context.sources);
    context
        .state
        .system_state(&system)
        .units
        .iter()
        .filter(|unit| {
            unit.owner == opponent
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == "infantry")
        })
        .cloned()
        .collect()
}

fn strike_wing_effect(context: &mut TimingContext<'_>, seat: &PlayerId) -> Result<(), TimingError> {
    let Some((system, _, hits)) = strike_wing_hits(context, seat) else {
        return Ok(());
    };
    for _ in 0..hits {
        let victims = strike_wing_victims(context, seat);
        let mut kinds: Vec<&Unit> = Vec::new();
        for unit in &victims {
            if !kinds.iter().any(|seen| seen.type_id == unit.type_id) {
                kinds.push(unit);
            }
        }
        let victim = match kinds.as_slice() {
            [] => return Ok(()),
            [only] => (*only).clone(),
            many => {
                let options = many
                    .iter()
                    .map(|unit| {
                        ChoiceOption::labelled(
                            unit.type_id.to_string(),
                            "strike_wing_destroy",
                            format!("destroy 1 {} of {}", unit.type_id, unit.owner),
                        )
                    })
                    .collect();
                let choice = Choice::new(
                    seat.clone(),
                    "Strike Wing Alpha II: choose an infantry to destroy".to_owned(),
                    options,
                )
                .contextualized(decision(
                    context.state,
                    seat,
                    "swa2",
                    "swa2_infantry",
                ));
                let answer = context.ask_seeing(&choice).map_err(illegal)?;
                match many.iter().find(|unit| unit.type_id.as_str() == answer.id) {
                    Some(unit) => (*unit).clone(),
                    None => return Ok(()),
                }
            }
        };
        context
            .state
            .system_mut(&system)
            .remove(std::slice::from_ref(&victim));
    }
    Ok(())
}

/// Which card grants the additional die.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Ambuscade,
    Commander,
}

impl Source {
    const fn card(self) -> &'static str {
        match self {
            Self::Ambuscade => "ambuscade",
            Self::Commander => "argentcommander",
        }
    }
}

/// Whether `seat` may use `source` right now.
fn holds(state: &GameState, seat: &PlayerId, source: Source) -> bool {
    match source {
        Source::Ambuscade => {
            state.promissory_notes.get(AMBUSCADE_NOTE) == Some(seat) && !is_argent(state, seat)
        }
        Source::Commander => {
            crate::promissory::has_commander_ability(state, seat, "argentcommander")
        }
    }
}

/// The units of `seat`'s staged unit-ability roll that can take a die, one per (type, planet),
/// as indices into the staged rolls. The Metali relic's own dice name no unit and are skipped.
fn rolling_units(state: &GameState, seat: &PlayerId) -> Vec<usize> {
    let Some(set) = state.reroll_staging.get(seat) else {
        return Vec::new();
    };
    if !is_unit_ability_roll(set) {
        return Vec::new();
    }
    let mut seen: Vec<(&str, Option<&ti4_model::id::PlanetId>)> = Vec::new();
    let mut found = Vec::new();
    for (index, entry) in set.rolls.iter().enumerate() {
        if entry.unit_types.is_empty() || entry.hits_on.is_none() {
            continue;
        }
        let key = (entry.unit.as_str(), entry.planet.as_ref());
        if !seen.contains(&key) {
            seen.push(key);
            found.push(index);
        }
    }
    found
}

/// Strike Wing Ambuscade (a note its holder plays) and the Argent commander share one text:
/// "When 1 or more of your units make a roll for a unit ability: choose 1 of those units to roll 1
/// additional die." The die is appended to the staged roll before the hits are read back, so it
/// counts exactly as if the unit had rolled it with the rest.
fn extra_die(owner_name: &str, seat: &PlayerId, source: Source) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    let (kind, id) = match source {
        Source::Ambuscade => ("promissory", "ambuscade"),
        Source::Commander => ("leader", "argentcommander"),
    };
    Ability::stateful(
        format!("{kind}:{owner_name}:{id}:UNIT_ABILITY_ROLLED:when"),
        seat.clone(),
        "UNIT_ABILITY_ROLLED",
        Relation::When,
        Arc::new(move |_event, _resolver, context| extra_die_effect(context, &owner, source)),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        own_roll(event, &condition_owner)
            && holds(context.state, &condition_owner, source)
            && !rolling_units(context.state, &condition_owner).is_empty()
    }))
}

fn extra_die_effect(
    context: &mut TimingContext<'_>,
    seat: &PlayerId,
    source: Source,
) -> Result<(), TimingError> {
    if !holds(context.state, seat, source) {
        return Ok(());
    }
    let candidates = rolling_units(context.state, seat);
    let index = match candidates.as_slice() {
        [] => return Ok(()),
        [only] => *only,
        many => {
            let Some(set) = context.state.reroll_staging.get(seat) else {
                return Ok(());
            };
            let options = many
                .iter()
                .map(|index| {
                    ChoiceOption::labelled(
                        format!("unit|{index}"),
                        "extra_die",
                        format!("{} rolls 1 additional die", set.rolls[*index].unit),
                    )
                })
                .collect();
            let choice = Choice::new(
                seat.clone(),
                format!("{}: choose a unit to roll 1 additional die", source.card()),
                options,
            )
            .contextualized(decision(
                context.state,
                seat,
                source.card(),
                "extra_die_unit",
            ));
            let answer = context.ask_seeing(&choice).map_err(illegal)?;
            match answer
                .id
                .strip_prefix("unit|")
                .and_then(|n| n.parse::<usize>().ok())
                .filter(|n| many.contains(n))
            {
                Some(index) => index,
                None => return Ok(()),
            }
        }
    };
    let hits_on = context
        .state
        .reroll_staging
        .get(seat)
        .and_then(|set| set.rolls.get(index))
        .and_then(|entry| entry.hits_on);
    let roll = context
        .dice
        .roll_by(context.rng, 1, "argent additional die", hits_on, seat);
    if let Some(entry) = context
        .state
        .reroll_staging
        .get_mut(seat)
        .and_then(|set| set.rolls.get_mut(index))
    {
        entry.faces.extend(roll.faces);
    }
    if source == Source::Ambuscade {
        crate::promissory::give_back(context.state, AMBUSCADE_NOTE);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::choice::{Scripted, Table, Window};
    use crate::fixtures::{armed_resolver, put, seated_game, with_context};
    use std::collections::{BTreeMap, BTreeSet};
    use ti4_model::content_types::DEFAULT;
    use ti4_model::state::{RerollEntry, RerollSet};

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn c() -> PlayerId {
        PlayerId::new("c")
    }
    fn arena() -> (GameState, SystemId) {
        (
            seated_game(&[("a", "argent"), ("b", "sol")], DEFAULT),
            SystemId::new("18"),
        )
    }

    #[test]
    fn agent_destinations_follow_recipient_wormhole_adjacency_only() {
        let content = ContentStore::embedded();
        let ids = ["18", "39", "40", "26", "25", "19", "20", "21"];
        let galaxy = ti4_content::galaxy::Galaxy::build(content, &ids, DEFAULT, 3)
            .expect("small map builds");
        let (producing, remote) = [("39", "25"), ("26", "25")]
            .into_iter()
            .find(|(from, to)| !galaxy.adjacent(from).contains(to))
            .expect("an alpha and beta system are not physically adjacent");
        let mut state = seated_game(&[("a", "argent"), ("b", "ghost")], DEFAULT);
        for system in ids {
            let Some(planet) = ti4_content::galaxy::system(content, system, DEFAULT)
                .and_then(|tile| tile.planets().into_iter().next())
                .map(PlanetId::new)
            else {
                continue;
            };
            state
                .system_mut(&SystemId::new(system))
                .set_control(planet, b());
        }

        let linked = crate::movement::PlayerAdjacency::new(&state, content, DEFAULT, &galaxy, &b());
        assert!(linked.neighbours(producing).contains(remote));
        assert!(!galaxy.adjacent(producing).contains(remote));
        let destinations = agent_planets(
            &state,
            content,
            DEFAULT,
            &galaxy,
            &b(),
            &SystemId::new(producing),
        );
        assert!(
            destinations
                .iter()
                .any(|(system, _)| system.as_str() == remote)
        );
        assert!(
            destinations
                .iter()
                .all(|(system, _)| { linked.neighbours(producing).contains(system.as_str()) })
        );
        assert!(
            state
                .controlled_planets(&b())
                .iter()
                .any(|(system, _)| { !linked.neighbours(producing).contains(system.as_str()) }),
            "the fixture includes controlled planets outside legal adjacency"
        );
    }

    #[test]
    fn ssruu_uses_argent_agent_on_a_real_infantry_production_window() {
        let content = ContentStore::embedded();
        let mut state = seated_game(&[("a", "sol"), ("b", "argent"), ("c", "yssaril")], DEFAULT);
        let mut ids = vec!["18".to_owned()];
        ids.extend(
            crate::fixtures::plain_systems(14)
                .into_iter()
                .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
                .take(6),
        );
        let tile_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let galaxy = ti4_content::galaxy::Galaxy::build(content, &tile_refs, DEFAULT, 1)
            .expect("test map builds");
        let producing = SystemId::new("18");
        let local_planet = ti4_content::galaxy::planets_in(content, "18", DEFAULT)
            .into_iter()
            .find(|planet| !planet.is_placed_during_play())
            .map(|planet| PlanetId::new(planet.id()))
            .expect("system 18 has a planet");
        let (remote, remote_planet) = galaxy
            .adjacent("18")
            .into_iter()
            .find_map(|system| {
                let planet = ti4_content::galaxy::planets_in(content, system, DEFAULT)
                    .into_iter()
                    .find(|planet| !planet.is_placed_during_play())?;
                Some((SystemId::new(system), PlanetId::new(planet.id())))
            })
            .expect("a neighbour has a planet");
        state
            .system_mut(&producing)
            .set_control(local_planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &producing, &local_planet, "spacedock", &a(), 1);
        state
            .system_mut(&remote)
            .set_control(remote_planet.clone(), a());
        state.player_mut(&a()).unwrap().trade_goods = 30;
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("argentagent"), LeaderStatus::Exhausted);
        state
            .player_mut(&c())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);

        let mut window =
            crate::production::ProductionWindow::new(&state, content, DEFAULT, &a(), &producing);
        let offer = window
            .pending_choice(&state, content, DEFAULT)
            .expect("production offer");
        let infantry = offer
            .options
            .iter()
            .find(|option| {
                option
                    .id
                    .strip_prefix("build|")
                    .and_then(|rest| rest.split('|').next())
                    .is_some_and(|id| {
                        catalogue(content, DEFAULT)
                            .get(id)
                            .is_some_and(|kind| kind.base_type() == "infantry")
                    })
            })
            .cloned()
            .expect("infantry can be produced");

        let mut resolver = armed_resolver(&state);
        let borrowed_id =
            "leader:argent:b:yssarilagent:argentagent:GROUND_FORCES_BEING_PRODUCED:when";
        let mut table = Table::default();
        table.seat(b(), Box::new(crate::choice::AlwaysDecline));
        table.seat(c(), Box::new(Scripted::new([borrowed_id])));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(7);
        let mut sequence = crate::event::EventSequence::new();
        let mut resolving = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: Some(&galaxy),
            }),
        };
        window
            .resolve(&mut state, &mut resolving, infantry)
            .expect("the infantry build is selected");
        let placement = loop {
            let choice = window
                .pending_choice(&state, content, DEFAULT)
                .expect("payment and placement choices continue the production window");
            if choice
                .options
                .iter()
                .any(|option| option.id.starts_with("place|"))
            {
                break choice;
            }
            let payment = choice
                .options
                .first()
                .cloned()
                .expect("the remaining production payment has a legal option");
            window
                .resolve(&mut state, &mut resolving, payment)
                .expect("production payment resolves and opens ground-force timing");
        };
        let remote_id = format!("place|{remote}@{remote_planet}");
        let remote_choice = placement
            .option(&remote_id)
            .cloned()
            .expect("the copied destination is offered for this infantry");
        window
            .resolve(&mut state, &mut resolving, remote_choice)
            .expect("remote infantry placement resolves");

        assert_eq!(
            state.player(&c()).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Exhausted
        );
        assert_eq!(
            state.player(&b()).unwrap().leaders[&LeaderId::new("argentagent")],
            LeaderStatus::Exhausted,
            "the source card keeps its prior state"
        );
        let infantry_on = |system: &SystemId, planet: &PlanetId| {
            let here = state.system_state(system);
            here.on_planet(planet)
                .iter()
                .filter(|unit| {
                    unit.owner == a()
                        && catalogue(content, DEFAULT)
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.base_type() == "infantry")
                })
                .count()
        };
        assert!(
            infantry_on(&remote, &remote_planet) > 0,
            "the producing player places the infantry at the copied destination"
        );
        assert_eq!(
            infantry_on(&producing, &local_planet),
            0,
            "the infantry was placed remotely, not at the production system"
        );
    }

    #[test]
    fn copied_agent_readiness_is_live_after_resolver_arming() {
        let content = ContentStore::embedded();
        let mut state = seated_game(&[("a", "argent"), ("b", "yssaril"), ("c", "sol")], DEFAULT);
        let ids = ["18", "39", "40", "26", "25", "19", "20", "21"];
        let galaxy = ti4_content::galaxy::Galaxy::build(content, &ids, DEFAULT, 3)
            .expect("fixture map builds");
        let producing = SystemId::new("18");
        let local = ti4_content::galaxy::planets_in(content, "18", DEFAULT)
            .into_iter()
            .find(|planet| !planet.is_placed_during_play())
            .map(|planet| PlanetId::new(planet.id()))
            .expect("system 18 has a planet");
        let (remote, remote_planet) = galaxy
            .adjacent("18")
            .into_iter()
            .find_map(|system| {
                let planet = ti4_content::galaxy::planets_in(content, system, DEFAULT)
                    .into_iter()
                    .find(|planet| !planet.is_placed_during_play())?;
                Some((SystemId::new(system), PlanetId::new(planet.id())))
            })
            .expect("a neighbour has a planet");
        state.system_mut(&producing).set_control(local, c());
        state
            .system_mut(&remote)
            .set_control(remote_planet.clone(), c());
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("argentagent"), LeaderStatus::Exhausted);
        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Exhausted);

        let mut resolver = armed_resolver(&state);
        let emit =
            |state: &mut GameState, resolver: &mut crate::timing::Resolver, answers: &[&str]| {
                let mut table = scripted(answers);
                with_context(state, DEFAULT, Some(&galaxy), &mut table, |context| {
                    let event = context
                        .event_sequence
                        .next(
                            "GROUND_FORCES_BEING_PRODUCED",
                            BTreeMap::from([
                                ("player".to_owned(), serde_json::Value::from("c")),
                                ("system".to_owned(), serde_json::Value::from("18")),
                                ("count".to_owned(), serde_json::Value::from(1)),
                            ]),
                        )
                        .expect("event id");
                    resolver
                        .emit_with_context(context, event, |_, _| {})
                        .expect("timing window resolves");
                });
            };
        emit(&mut state, &mut resolver, &[]);
        assert_eq!(
            state.player(&b()).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Exhausted,
            "an exhausted Ssruu has no copied use"
        );
        assert!(
            agent_production_destinations(&state, content, DEFAULT, &c(), &producing, "infantry")
                .is_empty()
        );

        state
            .player_mut(&b())
            .unwrap()
            .leaders
            .insert(LeaderId::new("yssarilagent"), LeaderStatus::Readied);
        emit(
            &mut state,
            &mut resolver,
            &["leader:argent:a:yssarilagent:argentagent:GROUND_FORCES_BEING_PRODUCED:when"],
        );
        assert_eq!(
            state.player(&b()).unwrap().leaders[&LeaderId::new("yssarilagent")],
            LeaderStatus::Exhausted
        );
        assert_eq!(
            state.player(&a()).unwrap().leaders[&LeaderId::new("argentagent")],
            LeaderStatus::Exhausted,
            "the source agent remains untouched"
        );
        assert!(
            agent_production_destinations(&state, content, DEFAULT, &c(), &producing, "infantry")
                .contains(&(remote, Some(remote_planet)))
        );
    }

    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    fn damaged(state: &GameState, system: &SystemId, owner: &PlayerId, kind: &str) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|u| &u.owner == owner && u.type_id.as_str() == kind && u.sustained_damage)
            .count()
    }
    fn count(state: &GameState, system: &SystemId, owner: &PlayerId, kind: &str) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|u| &u.owner == owner && u.type_id.as_str() == kind)
            .count()
    }
    fn excess(state: &mut GameState, answers: &[&str], who: &PlayerId, hits: usize) {
        let mut table = scripted(answers);
        let target = if who == &a() { b() } else { a() };
        let system = SystemId::new("18");
        with_context(state, DEFAULT, None, &mut table, |ctx| {
            afb_excess(
                ctx,
                &AfbExcess {
                    producer: who,
                    target: &target,
                    system: &system,
                    excess: hits,
                },
            )
        })
        .expect("legal");
    }
    fn stage(state: &mut GameState, player: &PlayerId, kind: &str, rolls: &[(&str, u32, &[u32])]) {
        let system = SystemId::new("18");
        state.reroll_staging.insert(
            player.clone(),
            RerollSet {
                kind: kind.to_owned(),
                system,
                rolls: rolls
                    .iter()
                    .map(|(unit, on, faces)| RerollEntry {
                        unit: (*unit).to_owned(),
                        planet: None,
                        hits_on: Some(*on),
                        faces: faces.to_vec(),
                        rerolled: BTreeSet::new(),
                        deltas: BTreeMap::new(),
                        unit_types: std::iter::once(((*unit).to_owned(), 1)).collect(),
                    })
                    .collect(),
            },
        );
    }
    fn rolled(state: &mut GameState, answers: &[&str], who: &str, kind: &str) {
        let mut resolver = armed_resolver(state);
        let mut table = scripted(answers);
        with_context(state, DEFAULT, None, &mut table, |ctx| {
            let payload = [("player", who), ("system", "18"), ("kind", kind)]
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
                .collect();
            let event = ctx
                .event_sequence
                .next("UNIT_ABILITY_ROLLED", payload)
                .expect("event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("window resolves");
        });
    }
    fn faces(state: &GameState, who: &PlayerId) -> usize {
        state
            .reroll_staging
            .get(who)
            .map_or(0, |set| set.rolls.iter().map(|r| r.faces.len()).sum())
    }

    // -- Zeal ------------------------------------------------------------------------------------

    #[test]
    fn zeal_casts_one_extra_vote_per_player_for_the_argent_seat_only() {
        let state = seated_game(&[("a", "argent"), ("b", "sol"), ("c", "hacan")], DEFAULT);
        assert_eq!(crate::leaders::vote_bonus(&state, &a()), 3);
        assert_eq!(crate::leaders::vote_bonus(&state, &b()), 0);
    }

    // -- Raid Formation --------------------------------------------------------------------------

    #[test]
    fn raid_formation_damages_one_sustain_ship_per_excess_hit() {
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &b(), 2);
        put(&mut state, &system, "cruiser", &b(), 1);
        excess(&mut state, &[], &a(), 1);
        assert_eq!(damaged(&state, &system, &b(), "dreadnought"), 1);
        // Three excess hits: only the two undamaged sustain ships are left to take them.
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &b(), 2);
        put(&mut state, &system, "cruiser", &b(), 1);
        excess(&mut state, &[], &a(), 3);
        assert_eq!(damaged(&state, &system, &b(), "dreadnought"), 2);
        assert_eq!(damaged(&state, &system, &b(), "cruiser"), 0);
    }

    #[test]
    fn raid_formation_lets_the_producer_choose_between_types() {
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &b(), 1);
        put(&mut state, &system, "carrier", &b(), 1);
        put(&mut state, &system, "warsun", &b(), 1);
        excess(&mut state, &["warsun"], &a(), 1);
        assert_eq!(damaged(&state, &system, &b(), "warsun"), 1);
        assert_eq!(damaged(&state, &system, &b(), "dreadnought"), 0);
    }

    #[test]
    fn raid_formation_needs_the_ability_and_an_undamaged_sustain_ship() {
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &a(), 1);
        // Sol's barrage has no Raid Formation.
        excess(&mut state, &[], &b(), 1);
        assert_eq!(damaged(&state, &system, &a(), "dreadnought"), 0);
        // No sustain ships (fighters, cruisers) and already-damaged ships are not targets.
        put(&mut state, &system, "fighter", &b(), 2);
        put(&mut state, &system, "cruiser", &b(), 1);
        excess(&mut state, &[], &a(), 2);
        assert!(
            state
                .system_state(&system)
                .units
                .iter()
                .all(|u| !u.sustained_damage)
        );
        let mut hurt = Unit::new(ti4_model::id::UnitTypeId::new("dreadnought"), b());
        hurt.sustained_damage = true;
        state.system_mut(&system).units.push(hurt);
        excess(&mut state, &[], &a(), 2);
        assert_eq!(
            damaged(&state, &system, &b(), "dreadnought"),
            1,
            "still the one"
        );
    }

    // -- Strike Wing Alpha II --------------------------------------------------------------------

    fn swa2_arena() -> (GameState, SystemId) {
        let (mut state, system) = arena();
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("swa2"));
        put(&mut state, &system, "argent_destroyer2", &a(), 1);
        put(&mut state, &system, "cruiser", &b(), 1);
        put(&mut state, &system, "infantry", &b(), 3);
        (state, system)
    }

    #[test]
    fn each_nine_or_ten_destroys_an_infantry_in_the_space_area() {
        let (mut state, system) = swa2_arena();
        stage(
            &mut state,
            &a(),
            "anti_fighter_barrage",
            &[(SWA2_UNIT, 6, &[9, 10, 4])],
        );
        rolled(&mut state, &[], "a", "anti_fighter_barrage");
        assert_eq!(count(&state, &system, &b(), "infantry"), 1);
        assert_eq!(
            count(&state, &system, &b(), "cruiser"),
            1,
            "ships are untouched"
        );
    }

    #[test]
    fn alpha_ii_stops_at_the_last_infantry_and_ignores_low_dice_and_the_base_unit() {
        let (mut state, system) = swa2_arena();
        stage(
            &mut state,
            &a(),
            "anti_fighter_barrage",
            &[(SWA2_UNIT, 6, &[10, 10, 9, 9, 9])],
        );
        rolled(&mut state, &[], "a", "anti_fighter_barrage");
        assert_eq!(count(&state, &system, &b(), "infantry"), 0);

        let (mut state, system) = swa2_arena();
        stage(
            &mut state,
            &a(),
            "anti_fighter_barrage",
            &[(SWA2_UNIT, 6, &[8, 6, 1])],
        );
        rolled(&mut state, &[], "a", "anti_fighter_barrage");
        assert_eq!(count(&state, &system, &b(), "infantry"), 3, "no 9 or 10");

        stage(
            &mut state,
            &a(),
            "anti_fighter_barrage",
            &[("argent_destroyer", 9, &[10, 10])],
        );
        rolled(&mut state, &[], "a", "anti_fighter_barrage");
        assert_eq!(
            count(&state, &system, &b(), "infantry"),
            3,
            "the first printing has no clause"
        );

        stage(&mut state, &a(), "space_cannon", &[(SWA2_UNIT, 6, &[10])]);
        rolled(&mut state, &[], "a", "space_cannon");
        assert_eq!(count(&state, &system, &b(), "infantry"), 3, "barrage only");
    }

    #[test]
    fn alpha_ii_does_not_touch_infantry_on_planets_or_the_owners_own() {
        let (mut state, system) = swa2_arena();
        put(&mut state, &system, "infantry", &a(), 2);
        stage(
            &mut state,
            &a(),
            "anti_fighter_barrage",
            &[(SWA2_UNIT, 6, &[10])],
        );
        rolled(&mut state, &[], "a", "anti_fighter_barrage");
        assert_eq!(count(&state, &system, &a(), "infantry"), 2);
        assert_eq!(count(&state, &system, &b(), "infantry"), 2);
    }

    #[test]
    fn the_first_strike_wing_has_the_printed_statistics() {
        let content = ContentStore::embedded();
        let types = catalogue(content, DEFAULT);
        let one = types["argent_destroyer"];
        assert_eq!((one.afb_hits_on(), one.afb_dice()), (Some(9), 2));
        assert_eq!(one.combat_hits_on(), Some(8));
        let two = types["argent_destroyer2"];
        assert_eq!((two.afb_hits_on(), two.afb_dice()), (Some(6), 3));
        assert_eq!(two.combat_hits_on(), Some(7));
        assert_eq!(two.capacity(), 1);
    }

    // -- Ambuscade and the commander -------------------------------------------------------------

    fn holder_game() -> GameState {
        let mut state = seated_game(&[("a", "sol"), ("b", "argent")], DEFAULT);
        crate::promissory::take(&mut state, ContentStore::embedded(), &a(), AMBUSCADE_NOTE);
        state
    }

    #[test]
    fn ambuscade_adds_one_die_to_a_chosen_unit_and_returns_to_argent() {
        let mut state = holder_game();
        assert_eq!(state.promissory_notes.get(AMBUSCADE_NOTE), Some(&a()));
        stage(
            &mut state,
            &a(),
            "space_cannon",
            &[("pds", 6, &[7]), ("pds2", 5, &[3])],
        );
        rolled(
            &mut state,
            &[
                "promissory:sol:ambuscade:UNIT_ABILITY_ROLLED:when",
                "unit|1",
            ],
            "a",
            "space_cannon",
        );
        let set = state.reroll_staging.get(&a()).unwrap();
        assert_eq!(set.rolls[0].faces.len(), 1);
        assert_eq!(
            set.rolls[1].faces.len(),
            2,
            "the chosen unit rolled one more"
        );
        assert_ne!(
            state.promissory_notes.get(AMBUSCADE_NOTE),
            Some(&a()),
            "returned"
        );
    }

    #[test]
    fn ambuscade_may_be_declined_and_needs_a_unit_ability_roll() {
        let mut state = holder_game();
        stage(&mut state, &a(), "space_cannon", &[("pds", 6, &[7])]);
        rolled(&mut state, &["decline"], "a", "space_cannon");
        assert_eq!(faces(&state, &a()), 1);
        assert_eq!(
            state.promissory_notes.get(AMBUSCADE_NOTE),
            Some(&a()),
            "kept"
        );
        // A ground roll is not a unit ability roll; the other player's roll is not the holder's.
        stage(&mut state, &a(), "ground", &[("infantry", 8, &[7])]);
        rolled(&mut state, &[], "a", "ground");
        stage(&mut state, &b(), "space_cannon", &[("pds", 6, &[7])]);
        rolled(&mut state, &[], "b", "space_cannon");
        assert_eq!(faces(&state, &a()), 1);
        assert_eq!(faces(&state, &b()), 1);
        assert_eq!(state.promissory_notes.get(AMBUSCADE_NOTE), Some(&a()));
    }

    #[test]
    fn the_commander_adds_a_die_only_once_unlocked_and_keeps_no_card_to_return() {
        let (mut state, _) = arena();
        stage(&mut state, &a(), "bombardment", &[("dreadnought", 5, &[7])]);
        rolled(
            &mut state,
            &["leader:argent:argentcommander:UNIT_ABILITY_ROLLED:when"],
            "a",
            "bombardment",
        );
        assert_eq!(faces(&state, &a()), 1, "locked");
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(LeaderId::new("argentcommander"), LeaderStatus::Unlocked);
        rolled(
            &mut state,
            &["leader:argent:argentcommander:UNIT_ABILITY_ROLLED:when"],
            "a",
            "bombardment",
        );
        assert_eq!(faces(&state, &a()), 2);
        rolled(&mut state, &["decline"], "a", "bombardment");
        assert_eq!(faces(&state, &a()), 2, "declined");
        assert!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("argentcommander"))
                == Some(&LeaderStatus::Unlocked)
        );
    }

    #[test]
    fn an_unseated_argent_commander_grant_adds_a_die_for_another_faction() {
        let (mut state, _) = arena();
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("yin");
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a(),
            "argentcommander",
        ));
        stage(&mut state, &a(), "bombardment", &[("dreadnought", 5, &[7])]);
        rolled(&mut state, &[], "a", "bombardment");
        assert_eq!(faces(&state, &a()), 2);
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("argentcommander")),
            Some(&LeaderStatus::Locked),
            "borrowed rights do not unlock the recipient's native leader record"
        );
    }

    #[test]
    fn ordinary_argent_alliance_stays_locked_until_its_owner_unlocks() {
        let (mut state, _) = arena();
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("yin");
        state.player_mut(&b()).unwrap().faction = ti4_model::id::FactionId::new("argent");
        crate::promissory::take(&mut state, ContentStore::embedded(), &a(), "an:argent");
        stage(&mut state, &a(), "bombardment", &[("dreadnought", 5, &[7])]);
        rolled(&mut state, &[], "a", "bombardment");
        assert_eq!(faces(&state, &a()), 1);
    }

    #[test]
    fn the_commander_unlocks_with_six_armed_units() {
        let (mut state, system) = arena();
        let content = ContentStore::embedded();
        let leader = LeaderId::new("argentcommander");
        let check =
            |state: &GameState| commander_unlocked(state, content, DEFAULT, None, &a(), &leader);
        let base = check(&state).map(|_| ()).and(Some(()));
        assert_eq!(base, Some(()));
        // Home units may already count; measure from what is there.
        let have = {
            let types = catalogue(content, DEFAULT);
            state
                .board
                .values()
                .flat_map(|board| {
                    board
                        .units
                        .iter()
                        .chain(board.planet_units.values().flatten())
                })
                .filter(|u| u.owner == a())
                .filter(|u| {
                    types.get(u.type_id.as_str()).is_some_and(|k| {
                        k.has_anti_fighter_barrage() || k.has_space_cannon() || k.has_bombardment()
                    })
                })
                .count()
        };
        assert!(have < 6);
        put(&mut state, &system, "argent_destroyer", &a(), 5 - have);
        assert_eq!(check(&state), Some(false), "five");
        put(&mut state, &system, "argent_destroyer", &a(), 1);
        assert_eq!(check(&state), Some(true), "six");
        // Another player's units do not count, and another leader is not this module's.
        assert_eq!(
            commander_unlocked(&state, content, DEFAULT, None, &b(), &leader),
            Some(false)
        );
        assert_eq!(
            commander_unlocked(
                &state,
                content,
                DEFAULT,
                None,
                &a(),
                &LeaderId::new("argentagent")
            ),
            None
        );
    }

    // -- Zeal, the flagship, the mech and Aerie Hololattice ----------------------------------

    #[test]
    fn zeal_seats_the_argent_player_first_in_the_voting_order() {
        let state = seated_game(&[("a", "sol"), ("b", "argent")], DEFAULT);
        assert!(crate::factions::hooks_cards::votes_first(&state, &b()));
        assert!(!crate::factions::hooks_cards::votes_first(&state, &a()));
    }

    #[test]
    fn the_flagship_bars_other_players_space_cannon_against_its_owner_in_its_system() {
        let (mut state, system) = arena();
        let content = ContentStore::embedded();
        let barred =
            |state: &GameState, shooter: &PlayerId, target: &PlayerId, system: &SystemId| {
                crate::factions::hooks_combat::space_cannon_barred(
                    state, content, DEFAULT, shooter, target, system,
                )
            };
        assert!(!barred(&state, &b(), &a(), &system), "no flagship yet");
        put(&mut state, &system, "argent_flagship", &a(), 1);
        assert!(barred(&state, &b(), &a(), &system));
        assert!(
            !barred(&state, &a(), &a(), &system),
            "its owner's own guns fire"
        );
        assert!(
            !barred(&state, &a(), &b(), &system),
            "protects only its owner's ships"
        );
        assert!(
            !barred(&state, &b(), &a(), &SystemId::new("19")),
            "this system only"
        );
    }

    #[test]
    fn the_mech_rides_free_and_no_other_unit_does() {
        let state = seated_game(&[("a", "argent"), ("b", "sol")], DEFAULT);
        let content = ContentStore::embedded();
        let free = |kind: &str, who: &PlayerId| {
            crate::factions::hooks_movement::free_cargo(
                &state,
                content,
                DEFAULT,
                &Unit::new(ti4_model::id::UnitTypeId::new(kind), who.clone()),
            )
        };
        assert!(free("argent_mech", &a()));
        assert!(!free("infantry", &a()));
        assert!(!free("sol_mech", &b()));
        let mech = catalogue(content, DEFAULT)["argent_mech"];
        assert_eq!(mech.capacity_cost(), 0, "and costs nothing in a space area");
    }

    fn hololattice() -> (GameState, SystemId, PlanetId) {
        let mut state = seated_game(&[("a", "argent"), ("b", "sol")], DEFAULT);
        let (system, planet) = crate::fixtures::a_placed_planet();
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new("ah"));
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &a(), 1);
        (state, system, planet)
    }

    #[test]
    fn aerie_hololattice_blocks_passage_and_adds_production_only_for_its_owner() {
        let (mut state, system, planet) = hololattice();
        let content = ContentStore::embedded();
        let blocks = |state: &GameState, mover: &PlayerId, system: &SystemId| {
            crate::factions::hooks_movement::blocks_passage(state, content, DEFAULT, mover, system)
        };
        let production = |state: &GameState, who: &PlayerId, planet: &PlanetId| {
            crate::factions::hooks_economy::extra_production_planet(
                state, content, DEFAULT, who, &system, planet,
            )
        };
        assert!(blocks(&state, &b(), &system));
        assert!(!blocks(&state, &a(), &system), "the owner passes");
        assert_eq!(production(&state, &a(), &planet), 1);
        assert_eq!(production(&state, &b(), &planet), 0);
        // Without the technology, or without a structure, nothing happens.
        state.player_mut(&a()).unwrap().technologies.clear();
        assert!(!blocks(&state, &b(), &system));
        assert_eq!(production(&state, &a(), &planet), 0);
        let (mut bare, system, planet) = hololattice();
        bare.system_mut(&system).planet_units.clear();
        assert!(!crate::factions::hooks_movement::blocks_passage(
            &bare,
            content,
            DEFAULT,
            &b(),
            &system
        ));
        assert_eq!(
            crate::factions::hooks_economy::extra_production_planet(
                &bare,
                content,
                DEFAULT,
                &a(),
                &system,
                &planet
            ),
            0
        );
    }

    // -- the hero --------------------------------------------------------------------------------

    fn hero_board() -> (GameState, ti4_content::galaxy::Galaxy, [SystemId; 3]) {
        let mut state = seated_game(&[("a", "argent"), ("b", "sol")], DEFAULT);
        let content = ContentStore::embedded();
        let ids: Vec<String> = crate::fixtures::plain_systems(14)
            .into_iter()
            .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
            .take(6)
            .collect();
        let mut tiles = vec!["18"];
        tiles.extend(ids.iter().map(String::as_str));
        let galaxy = ti4_content::galaxy::Galaxy::build(content, &tiles, DEFAULT, 2).unwrap();
        let sys = |i: usize| SystemId::new(ids[i].as_str());
        let (from, safe, held) = (sys(0), sys(2), sys(4));
        put(&mut state, &from, "carrier", &a(), 1);
        put(&mut state, &from, "infantry", &a(), 1);
        state.system_mut(&safe).command_tokens.insert(a());
        state.system_mut(&held).command_tokens.insert(a());
        put(&mut state, &held, "cruiser", &b(), 1);
        (state, galaxy, [from, safe, held])
    }

    fn fly(
        state: &mut GameState,
        galaxy: &ti4_content::galaxy::Galaxy,
        answers: &[&str],
    ) -> Option<bool> {
        let mut table = scripted(answers);
        with_context(state, DEFAULT, Some(galaxy), &mut table, |ctx| {
            use_leader(ctx, &a(), &LeaderId::new("argenthero"))
        })
    }

    #[test]
    fn flock_migration_moves_a_ship_and_its_cargo_to_a_tokened_empty_system() {
        let (mut state, galaxy, [from, safe, held]) = hero_board();
        assert_eq!(
            leader_action(
                &state,
                ContentStore::embedded(),
                &a(),
                &LeaderId::new("argenthero")
            ),
            Some(true)
        );
        // Only the system with a token and no foreign ships is offered: `held` is not.
        let done = fly(
            &mut state,
            &galaxy,
            &[
                "ship|0",
                &format!("to|{safe}"),
                "cargo|0",
                "decline",
                "decline",
            ],
        );
        assert_eq!(done, Some(true));
        assert_eq!(count(&state, &from, &a(), "carrier"), 0);
        assert_eq!(count(&state, &safe, &a(), "carrier"), 1);
        assert_eq!(
            count(&state, &safe, &a(), "infantry"),
            1,
            "the cargo came along, in space"
        );
        assert_eq!(count(&state, &held, &a(), "carrier"), 0);
    }

    #[test]
    fn flock_migration_moves_each_ship_once() {
        // Two tokened, empty systems: after the carrier moves to the first, it is not offered
        // again, so it cannot shuttle back and forth and the migration ends.
        let (mut state, galaxy, [from, safe, _]) = hero_board();
        let other = galaxy
            .system_ids()
            .into_iter()
            .map(SystemId::new)
            .find(|id| *id != from && *id != safe && !state.board.contains_key(id))
            .expect("a third system");
        state.system_mut(&other).command_tokens.insert(a());
        // The carrier is Argent's only ship, so the second "ship|0" could only be it again.
        let content = ContentStore::embedded();
        for (system, board) in &mut state.board {
            if *system != from {
                board.units.retain(|unit| {
                    unit.owner != a()
                        || !ti4_content::units::unit_type(content, unit.type_id.as_str(), DEFAULT)
                            .is_some_and(|kind| kind.is_ship())
                });
            }
        }
        let done = fly(
            &mut state,
            &galaxy,
            &["ship|0", &format!("to|{safe}"), "decline", "ship|0"],
        );
        assert_eq!(done, Some(true));
        assert_eq!(
            count(&state, &safe, &a(), "carrier"),
            1,
            "moved once, and stayed"
        );
        assert_eq!(count(&state, &other, &a(), "carrier"), 0);
        assert_eq!(count(&state, &from, &a(), "carrier"), 0);
    }

    #[test]
    fn flock_migration_declined_changes_nothing_and_is_not_offered_without_a_destination() {
        let (mut state, galaxy, [from, _, held]) = hero_board();
        let before = state.clone();
        assert_eq!(fly(&mut state, &galaxy, &["decline"]), Some(false));
        assert_eq!(count(&state, &from, &a(), "carrier"), 1);
        assert_eq!(state.board, before.board);
        // Another player's hero text is not this module's; the other seat's ships are not offered.
        let content = ContentStore::embedded();
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("argentagent")),
            None
        );
        // Remove the only safe destination: nothing to offer.
        let (mut bare, _, [_, safe, _]) = hero_board();
        bare.system_mut(&safe).command_tokens.clear();
        assert_eq!(
            leader_action(&bare, content, &a(), &LeaderId::new("argenthero")),
            Some(false)
        );
        // A system holding a foreign ship is never a destination.
        assert!(!hero_destinations(&state, content, DEFAULT, &a()).contains(&held));
        // Not Argent's: sol's seat is told nothing.
        assert_eq!(
            leader_action(&state, content, &b(), &LeaderId::new("argenthero")),
            Some(false)
        );
    }

    // -- Wing Transfer ---------------------------------------------------------------------------

    /// Active system `m` and a neighbour `n`, both holding only `a`'s units, and a third system `f`
    /// beside `m` with `b`'s ship.
    fn wing_board() -> (GameState, ti4_content::galaxy::Galaxy, SystemId, SystemId) {
        let mut state = seated_game(&[("a", "argent"), ("b", "sol")], DEFAULT);
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("argentbt"));
        let content = ContentStore::embedded();
        let ids: Vec<String> = crate::fixtures::plain_systems(14)
            .into_iter()
            .filter(|id| !state.board.contains_key(&SystemId::new(id.as_str())))
            .take(6)
            .collect();
        let mut tiles = vec!["18"];
        tiles.extend(ids.iter().map(String::as_str));
        let galaxy = ti4_content::galaxy::Galaxy::build(content, &tiles, DEFAULT, 2).unwrap();
        let active = SystemId::new(ids[0].as_str());
        let near = galaxy
            .adjacent(active.as_str())
            .into_iter()
            .find(|id| ids.iter().any(|plain| plain == id))
            .map(SystemId::new)
            .expect("a ring neighbour");
        put(&mut state, &active, "carrier", &a(), 1);
        state.system_mut(&active).command_tokens.insert(a());
        put(&mut state, &near, "cruiser", &a(), 1);
        (state, galaxy, active, near)
    }

    fn emit_with(
        state: &mut GameState,
        galaxy: &ti4_content::galaxy::Galaxy,
        answers: &[&str],
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        let mut resolver = armed_resolver(state);
        let mut table = scripted(answers);
        with_context(state, DEFAULT, Some(galaxy), &mut table, |ctx| {
            let payload = pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
                .collect();
            let event = ctx.event_sequence.next(event_type, payload).expect("id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("window resolves");
        });
    }

    #[test]
    fn wing_transfer_keeps_the_move_when_no_token_is_placed() {
        // The clauses are independent: declining the optional placement still earns the
        // end-of-action move among the active system and adjacent systems with your tokens.
        let (mut state, galaxy, active, _near) = wing_board();
        let before = state.tokens_in_reinforcements(&a());
        emit_with(
            &mut state,
            &galaxy,
            &[
                "breakthrough:argent:argentbt_mark:SYSTEM_ACTIVATED:when",
                "decline",
            ],
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", active.as_str())],
        );
        assert_eq!(
            state.tokens_in_reinforcements(&a()),
            before,
            "nothing placed"
        );
        assert_eq!(
            state
                .faction_marks
                .get(&wing_transfer_key(&a()))
                .map(String::as_str),
            Some(active.as_str()),
            "the activation is remembered for the end of the action"
        );
    }

    #[test]
    fn wing_transfer_places_tokens_then_moves_ships_among_the_tokened_systems() {
        let (mut state, galaxy, active, near) = wing_board();
        let before = state.tokens_in_reinforcements(&a());
        emit_with(
            &mut state,
            &galaxy,
            &[
                "breakthrough:argent:argentbt:SYSTEM_ACTIVATED:when",
                &format!("token|{near}"),
                "decline",
            ],
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", active.as_str())],
        );
        assert!(state.system_state(&near).command_tokens.contains(&a()));
        assert_eq!(state.tokens_in_reinforcements(&a()), before - 1);
        assert!(state.faction_marks.contains_key(&wing_transfer_key(&a())));
        // At the end of the action the carrier may fly to the neighbour.
        let pick = hero_ships(&state, ContentStore::embedded(), DEFAULT, &a())
            .into_iter()
            .filter(|(system, _)| system == &active || system == &near)
            .position(|(system, unit)| system == active && unit.type_id.as_str() == "carrier")
            .expect("the carrier is offered");
        emit_with(
            &mut state,
            &galaxy,
            &[&format!("ship|{pick}"), &format!("to|{near}"), "decline"],
            "ACTION_COMPLETED",
            &[("player", "a")],
        );
        assert_eq!(
            count(&state, &near, &a(), "carrier"),
            1,
            "it flew to the neighbour"
        );
        assert!(
            !state.faction_marks.contains_key(&wing_transfer_key(&a())),
            "spent"
        );
        assert_eq!(count(&state, &active, &a(), "carrier"), 0);
    }

    #[test]
    fn wing_transfer_needs_the_breakthrough_and_systems_with_only_the_players_units() {
        // Not held: nothing is offered.
        let (mut state, galaxy, active, near) = wing_board();
        state.player_mut(&a()).unwrap().breakthrough = None;
        emit_with(
            &mut state,
            &galaxy,
            &[
                "breakthrough:argent:argentbt:SYSTEM_ACTIVATED:when",
                &format!("token|{near}"),
            ],
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", active.as_str())],
        );
        assert!(!state.system_state(&near).command_tokens.contains(&a()));
        assert!(state.faction_marks.is_empty());
        // Another player's unit in the adjacent system, or in the active one, bars the placement.
        let (mut state, galaxy, active, near) = wing_board();
        put(&mut state, &near, "cruiser", &b(), 1);
        emit_with(
            &mut state,
            &galaxy,
            &[
                "breakthrough:argent:argentbt:SYSTEM_ACTIVATED:when",
                &format!("token|{near}"),
            ],
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", active.as_str())],
        );
        assert!(!state.system_state(&near).command_tokens.contains(&a()));
        // Declining places nothing but the action's end still finds no mark.
        let (mut state, galaxy, active, near) = wing_board();
        emit_with(
            &mut state,
            &galaxy,
            &["decline"],
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", active.as_str())],
        );
        assert!(!state.system_state(&near).command_tokens.contains(&a()));
        assert!(state.faction_marks.is_empty());
        // Another player's activation is none of ours.
        let (mut state, galaxy, active, near) = wing_board();
        emit_with(
            &mut state,
            &galaxy,
            &[],
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", active.as_str())],
        );
        assert!(!state.system_state(&near).command_tokens.contains(&a()));
    }

    // -- no Argent, no Argent abilities ----------------------------------------------------------

    #[test]
    fn a_game_without_an_argent_seat_is_offered_no_argent_ability() {
        let (decider, seen) = crate::choice::Capturing::new(Box::new(crate::choice::FirstOption));
        let mut table = Table::with_default(Box::new(decider));
        let mut state = seated_game(&[("a", "hacan"), ("b", "sol")], DEFAULT);
        let system = SystemId::new("18");
        put(&mut state, &system, "argent_destroyer2", &a(), 1);
        put(&mut state, &system, "infantry", &b(), 2);
        put(&mut state, &system, "dreadnought", &b(), 1);
        for who in ["a", "b"] {
            for kind in ["anti_fighter_barrage", "space_cannon", "bombardment"] {
                stage(
                    &mut state,
                    &PlayerId::new(who),
                    kind,
                    &[(SWA2_UNIT, 6, &[10, 9])],
                );
                let mut resolver = armed_resolver(&state);
                with_context(&mut state, DEFAULT, None, &mut table, |ctx| {
                    let payload = [("player", who), ("system", "18"), ("kind", kind)]
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
                        .collect();
                    let event = ctx
                        .event_sequence
                        .next("UNIT_ABILITY_ROLLED", payload)
                        .unwrap();
                    resolver.emit_with_context(ctx, event, |_, _| {}).unwrap();
                });
            }
        }
        assert_eq!(count(&state, &system, &b(), "infantry"), 2);
        assert_eq!(faces(&state, &a()), 2);
        assert_eq!(vote_bonus(&state, &a()), 0);
        let asked: Vec<String> = seen
            .borrow()
            .iter()
            .filter(|c| {
                c.ids()
                    .iter()
                    .any(|id| id.contains("argent") || id.contains("ambuscade"))
            })
            .map(|c| c.prompt.clone())
            .collect();
        assert!(asked.is_empty(), "{asked:?}");
        excess(&mut state, &[], &a(), 2);
        assert_eq!(damaged(&state, &system, &b(), "dreadnought"), 0);
    }

    #[test]
    fn a_nekro_flagship_with_the_argent_z_token_bars_space_cannon_against_its_owner() {
        let content = ContentStore::embedded();
        let system = SystemId::new("18");
        let barred = |state: &GameState| {
            crate::factions::hooks_combat::space_cannon_barred(
                state,
                content,
                DEFAULT,
                &b(),
                &a(),
                &system,
            )
        };
        let mut bare = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], &[]);
        put(&mut bare, &system, "nekro_flagship", &a(), 1);
        assert!(!barred(&bare), "the toggle is off by default");
        let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], &["argent"]);
        put(&mut state, &system, "nekro_flagship", &a(), 1);
        assert!(barred(&state));
    }
}
