//! The Arborec (`arborec`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-arborec.md`.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Mitosis: "Your space docks cannot produce infantry." / "At the start of the status phase:
//!   Place 1 infantry from your reinforcements on any planet you control."
//! * Bioplasmosis (`bio`): "At the end of the status phase, you may remove any number of infantry
//!   from planets you control and place them on 1 or more planets you control in the same or
//!   adjacent systems."
//! * Letani Warrior II (`lw2`): "Cost 1(x2), Combat 7 PRODUCTION 2. After this unit is destroyed,
//!   roll 1 die. If the result is 6 or greater, place the unit on this card. At the start of your
//!   next turn, place each unit that is on this card on a planet you control in your home system."
//! * Duha Menaimon (flagship): "After you activate this system, you may produce up to 5 units in
//!   this system."
//! * Letani Behemoth (mech): "DEPLOY: When you would use your Mitosis faction ability you may
//!   replace 1 of your infantry with 1 mech from your reinforcements instead."
//! * Stymie (`stymie`): "After another player moves ships into a system that contains 1 or more of
//!   your units: You may place 1 command token from that player's reinforcements in any non-home
//!   system. Then, return this card to the Arborec player."
//! * Letani Ospha (`arborecagent`): "ACTION: Exhaust this card and choose a player's non-fighter
//!   ship: that player may replace that ship with one from their reinforcements that costs up to 2
//!   more than the replaced ship."
//! * Dirzuga Rophal (`arboreccommander`): "After another player activates a system that contains 1
//!   or more of your units that have PRODUCTION: You may produce 1 unit in that system." Unlock:
//!   "Have 12 ground forces on planets you control."
//! * Letani Miasmiala (`arborechero`): "ACTION: Produce any number of units in any number of
//!   systems that contain 1 or more of your ground forces. Then, purge this card." Unlock: "Have 3
//!   scored objectives" (shared code).
//! * Psychospore (`arborecbt`): "ACTION: Exhaust this card to remove a command token from a system
//!   that contains 1 or more of your infantry and return it to your reinforcements. Then, place 1
//!   infantry in that system."

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use super::hooks_economy::EconomyHooks;
use super::{FactionModule, Hooks};
use crate::action_cards::{PlacementTarget, place_units_counted, placement_spots, replace_unit};
use crate::choice::{Choice, ChoiceOption, Resolving, TimingHandle, Window};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::production::ProductionWindow;
use crate::timing::{Ability, Relation, Resolver, TimingContext, TimingError};

/// The faction alias.
const FACTION: &str = "arborec";
/// Stymie's note id (`<alias>:<owner's faction name>`).
const STYMIE_NOTE: &str = "stymie:arborec";
/// The Psychospore component action.
const PSYCHOSPORE_ACTION: &str = "faction|arborec|psychospore";
/// `GameState::faction_marks` key prefix for Letani Warrior II units waiting on the card; the value
/// is how many, per player.
const CARD_PREFIX: &str = "arborec:lw2:";
/// `GameState::faction_marks` key prefix for an exhausted Psychospore; present while exhausted.
const EXHAUSTED_PREFIX: &str = "arborec:psychospore:exhausted:";
/// Letani Warrior II returns on a 6 or greater.
const LW2_THRESHOLD: u32 = 6;
/// "Up to 5 units".
const FLAGSHIP_LIMIT: i64 = 5;
/// "Any number of units": larger than any bill or box can reach.
const HERO_LIMIT: i64 = 1000;

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &["mitosis"],
    // `lw2` / `arborec_infantry2`: every path that *destroys* a ground force announces
    // GROUND_FORCE_DESTROYED (invasion, action cards, agendas, Nova Seed). Cargo left over capacity
    // when its ship dies is *removed* (LRR 16 Capacity), not destroyed, so Letani Warrior II does
    // not roll for it — by the rules, not a gap.
    technologies: &["bio", "lw2"],
    units: &[
        "arborec_flagship",
        "arborec_infantry",
        "arborec_infantry2",
        "arborec_mech",
    ],
    promissory: &["stymie"],
    leaders: &["arborecagent", "arboreccommander", "arborechero"],
    breakthroughs: &["arborecbt"],
    hooks: Hooks {
        component_actions: Some(component_actions),
        perform_component: Some(perform_component),
        commander_unlocked: Some(commander_unlocked),
        leader_action: Some(leader_action),
        use_leader: Some(use_leader),
        timing_abilities: Some(timing_abilities),
        economy: EconomyHooks {
            cannot_produce: Some(cannot_produce),
            ..EconomyHooks::NONE
        },
        ..Hooks::NONE
    },
};

// -- small readers -------------------------------------------------------------------------------

/// Owns the technology, or the Nekro's Valefar Assimilator carries its text.
fn has_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    crate::technology::has_technology_text(state, player, alias)
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

/// The unit type this player puts on the board for a base type: their upgrade if they own it, else
/// their faction's own unit, else the generic one (the same order `action_cards` places in).
fn player_unit<'a>(
    state: &GameState,
    content: &'a ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> Option<UnitType<'a>> {
    let seat = state.player(player)?;
    let faction = seat.faction.to_string();
    let held: Vec<String> = seat
        .technologies
        .iter()
        .map(|tech| tech.as_str().to_owned())
        .collect();
    ti4_content::units::unlocked_upgrade(content, sources, base_type, &faction, &held)
        .or_else(|| ti4_content::units::faction_unit(content, &faction, base_type, sources))
        .or_else(|| catalogue(content, sources).get(base_type).copied())
}

/// Whether the box still holds one of the player's units of this base type (31.4).
fn box_has(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> bool {
    player_unit(state, content, sources, player, base_type).is_some_and(|kind| {
        crate::supply::allowed(
            state,
            content,
            sources,
            player,
            &UnitTypeId::new(kind.id()),
            1,
        ) > 0
    })
}

fn is_base(types: &BTreeMap<&str, UnitType<'_>>, unit: &Unit, base: &str) -> bool {
    types
        .get(unit.type_id.as_str())
        .is_some_and(|kind| kind.base_type() == base)
}

/// The player's infantry on one planet.
fn infantry_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
    player: &PlayerId,
) -> usize {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .on_planet_of(planet, player)
        .into_iter()
        .filter(|unit| is_base(&types, unit, "infantry"))
        .count()
}

fn card_key(player: &PlayerId) -> String {
    format!("{CARD_PREFIX}{player}")
}

/// Letani Warrior II units on the card.
fn on_card(state: &GameState, player: &PlayerId) -> usize {
    state
        .faction_marks
        .get(&card_key(player))
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

fn exhausted_key(player: &PlayerId) -> String {
    format!("{EXHAUSTED_PREFIX}{player}")
}

fn event_player(event: &crate::event::Event) -> Option<PlayerId> {
    event.text("player").map(PlayerId::new)
}

/// Run ability production (flagship, commander, hero) through the caller's context. With a
/// resolver the production announces `UNITS_PRODUCED`; without one (leader hooks get none) nothing
/// can react to it.
fn produce(
    context: &mut TimingContext<'_>,
    resolver: Option<&mut Resolver>,
    player: &PlayerId,
    system: &SystemId,
    limit: i64,
) -> Result<usize, TimingError> {
    let TimingContext {
        state,
        content,
        sources,
        table,
        dice,
        rng,
        event_sequence,
        galaxy,
    } = context;
    let galaxy = *galaxy;
    let mut ctx = Resolving {
        content,
        sources: *sources,
        dice,
        rng,
        table,
        timing: resolver.map(|resolver| TimingHandle {
            resolver,
            sequence: event_sequence,
            galaxy,
        }),
    };
    let report =
        crate::production::produce_by_ability(state, &mut ctx, galaxy, player, system, Some(limit))
            .map_err(TimingError::IllegalChoice)?;
    Ok(report.produced.len())
}

/// Whether production by ability could build at least one thing in `system` right now.
fn can_produce(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    limit: i64,
) -> bool {
    ProductionWindow::for_ability(state, content, sources, player, system, Some(limit))
        .pending_choice(state, content, sources)
        .is_some()
}

// -- Mitosis -------------------------------------------------------------------------------------

/// "Your space docks cannot produce infantry."
fn cannot_produce(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    unit_base: &str,
    producer_base: &str,
) -> bool {
    unit_base == "infantry"
        && producer_base == "spacedock"
        && crate::faction_abilities::has(state, content, player, "mitosis")
}

/// One way to resolve Mitosis: place an infantry on a planet, or (Letani Behemoth) replace an
/// infantry there with a mech.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MitosisMove {
    replace: bool,
    system: SystemId,
    /// `None` is the system's space area.
    planet: Option<PlanetId>,
}

impl MitosisMove {
    fn id(&self) -> String {
        format!(
            "{}|{}|{}",
            if self.replace { "mech" } else { "infantry" },
            self.system,
            self.planet.as_ref().map_or("space", PlanetId::as_str)
        )
    }
}

fn mitosis_moves(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<MitosisMove> {
    let mut moves = Vec::new();
    if box_has(state, content, sources, player, "infantry") {
        for (system, planet) in placement_spots(
            state,
            content,
            sources,
            player,
            PlacementTarget::ControlledPlanet,
            None,
        ) {
            if let Some(planet) = planet {
                moves.push(MitosisMove {
                    replace: false,
                    system,
                    planet: Some(planet),
                });
            }
        }
    }
    if box_has(state, content, sources, player, "mech") {
        for (system, board) in &state.board {
            for planet in board.planet_units.keys() {
                if infantry_on(state, content, sources, system, planet, player) > 0 {
                    moves.push(MitosisMove {
                        replace: true,
                        system: system.clone(),
                        planet: Some(planet.clone()),
                    });
                }
            }
            let types = catalogue(content, sources);
            if board
                .units
                .iter()
                .any(|unit| &unit.owner == player && is_base(&types, unit, "infantry"))
            {
                moves.push(MitosisMove {
                    replace: true,
                    system: system.clone(),
                    planet: None,
                });
            }
        }
    }
    moves
}

/// Mitosis: "Place 1 infantry from your reinforcements on any planet you control." With the
/// Letani Behemoth, the player may instead replace 1 of their infantry with 1 mech.
///
/// Mandatory. The one question (where, or which replacement) is asked before anything changes; a
/// single possible resolution is not a decision.
fn mitosis(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("ability:{owner_name}:mitosis:STATUS_PHASE_BEGAN:after"),
        seat.clone(),
        "STATUS_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if !crate::faction_abilities::has(context.state, context.content, &owner, "mitosis") {
                return Ok(());
            }
            let moves = mitosis_moves(context.state, context.content, context.sources, &owner);
            // The replacement is a "may": with no placement to fall back on it is declinable.
            let only_replacements = moves.iter().all(|step| step.replace);
            let chosen = match moves.as_slice() {
                [] => return Ok(()),
                [only] if !only.replace => only.clone(),
                _ => {
                    let mut options: Vec<ChoiceOption> = moves
                        .iter()
                        .map(|step| {
                            let place = step.planet.as_ref().map_or("space", PlanetId::as_str);
                            let label = if step.replace {
                                format!(
                                    "Mitosis: replace an infantry on {place} in {} with a mech",
                                    step.system
                                )
                            } else {
                                format!("Mitosis: place 1 infantry on {place} in {}", step.system)
                            };
                            ChoiceOption::labelled(step.id(), "mitosis", label)
                        })
                        .collect();
                    if only_replacements {
                        options.push(ChoiceOption::decline());
                    }
                    let choice = Choice::new(
                        owner.clone(),
                        "Mitosis: place an infantry or replace one with a mech".to_owned(),
                        options,
                    )
                    .contextualized(decision(
                        context.state,
                        &owner,
                        "mitosis",
                        "mitosis_place",
                    ));
                    let answer = context
                        .ask_seeing(&choice)
                        .map_err(TimingError::IllegalChoice)?;
                    if answer.is_decline() {
                        return Ok(());
                    }
                    let Some(step) = moves.iter().find(|step| step.id() == answer.id) else {
                        return Ok(());
                    };
                    step.clone()
                }
            };
            if chosen.replace {
                replace_unit(
                    context,
                    &owner,
                    &owner,
                    &chosen.system,
                    chosen.planet.as_ref(),
                    "infantry",
                    "mech",
                );
            } else {
                place_units_counted(
                    context,
                    &owner,
                    &chosen.system,
                    chosen.planet.as_ref(),
                    "infantry",
                    1,
                );
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        crate::faction_abilities::has(context.state, context.content, &condition_owner, "mitosis")
            && !mitosis_moves(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_empty()
    }))
}

// -- Bioplasmosis --------------------------------------------------------------------------------

/// One infantry moved between planets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shift {
    from: (SystemId, PlanetId),
    to: (SystemId, PlanetId),
}

/// The infantry the player has on planets they control, per planet.
fn infantry_sources(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> BTreeMap<(SystemId, PlanetId), usize> {
    state
        .controlled_planets(player)
        .into_iter()
        .filter_map(|(system, planet)| {
            let count = infantry_on(state, content, sources, system, planet, player);
            (count > 0).then(|| ((system.clone(), planet.clone()), count))
        })
        .collect()
}

/// Systems that count as "the same or adjacent" to `system`.
fn near(galaxy: Option<&ti4_content::galaxy::Galaxy>, system: &SystemId) -> BTreeSet<SystemId> {
    let mut near = BTreeSet::new();
    near.insert(system.clone());
    if let Some(galaxy) = galaxy {
        near.extend(
            galaxy
                .adjacent(system.as_str())
                .into_iter()
                .map(SystemId::new),
        );
    }
    near
}

/// Every single-infantry move still open, given what has been taken from each planet already.
fn shifts(
    state: &GameState,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    available: &BTreeMap<(SystemId, PlanetId), usize>,
) -> Vec<Shift> {
    let controlled: Vec<(SystemId, PlanetId)> = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect();
    let mut found = Vec::new();
    for (from, count) in available {
        if *count == 0 {
            continue;
        }
        let reach = near(galaxy, &from.0);
        for to in &controlled {
            if to != from && reach.contains(&to.0) {
                found.push(Shift {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
    }
    found
}

fn bioplasmosis_ready(context: &TimingContext<'_>, player: &PlayerId) -> bool {
    has_technology(context.state, player, "bio")
        && !shifts(
            context.state,
            context.galaxy,
            player,
            &infantry_sources(context.state, context.content, context.sources, player),
        )
        .is_empty()
}

/// Move one infantry (the first on the planet, whatever its upgrade) between planets.
fn move_infantry(context: &mut TimingContext<'_>, player: &PlayerId, shift: &Shift) {
    let types = catalogue(context.content, context.sources);
    let taken = context
        .state
        .board
        .get_mut(&shift.from.0)
        .and_then(|board| board.planet_units.get_mut(&shift.from.1))
        .and_then(|units| {
            let index = units
                .iter()
                .position(|unit| &unit.owner == player && is_base(&types, unit, "infantry"))?;
            Some(units.remove(index))
        });
    if let Some(unit) = taken {
        context
            .state
            .system_mut(&shift.to.0)
            .planet_units
            .entry(shift.to.1.clone())
            .or_default()
            .push(unit);
    }
}

/// Bioplasmosis: "At the end of the status phase, you may remove any number of infantry from
/// planets you control and place them on 1 or more planets you control in the same or adjacent
/// systems."
///
/// Each infantry is one question ("move one from X to Y, or stop"), asked against the infantry
/// still unmoved; nothing is moved until the player has stopped, and an infantry already moved is
/// not moved again.
fn bioplasmosis(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("technology:{owner_name}:bio:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            if !bioplasmosis_ready(context, &owner) {
                return Ok(());
            }
            let mut available =
                infantry_sources(context.state, context.content, context.sources, &owner);
            let mut planned: Vec<Shift> = Vec::new();
            loop {
                let open = shifts(context.state, context.galaxy, &owner, &available);
                if open.is_empty() {
                    break;
                }
                let mut options: Vec<ChoiceOption> = open
                    .iter()
                    .enumerate()
                    .map(|(index, shift)| {
                        ChoiceOption::labelled(
                            format!("move|{index}"),
                            "bioplasmosis",
                            format!(
                                "move 1 infantry from {} in {} to {} in {}",
                                shift.from.1, shift.from.0, shift.to.1, shift.to.0
                            ),
                        )
                    })
                    .collect();
                options.push(ChoiceOption::decline());
                let choice = Choice::new(
                    owner.clone(),
                    format!("Bioplasmosis: move an infantry ({} so far)", planned.len()),
                    options,
                )
                .contextualized(decision(
                    context.state,
                    &owner,
                    "bio",
                    "bioplasmosis_move",
                ));
                let answer = context
                    .ask_seeing(&choice)
                    .map_err(TimingError::IllegalChoice)?;
                if answer.is_decline() {
                    break;
                }
                let Some(shift) = choice
                    .options
                    .iter()
                    .position(|option| option.id == answer.id)
                    .and_then(|index| open.get(index))
                else {
                    break;
                };
                if let Some(count) = available.get_mut(&shift.from) {
                    *count -= 1;
                }
                planned.push(shift.clone());
            }
            for shift in &planned {
                move_infantry(context, &owner, shift);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        bioplasmosis_ready(context, &condition_owner)
    }))
}

// -- Letani Warrior II ---------------------------------------------------------------------------

/// "After this unit is destroyed, roll 1 die. If the result is 6 or greater, place the unit on this
/// card." Listens to `GROUND_FORCE_DESTROYED`, which invasion paths emit.
fn warrior_rolls(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:arborec_infantry2:GROUND_FORCE_DESTROYED:after"),
        seat.clone(),
        "GROUND_FORCE_DESTROYED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let roll = context.dice.roll_by(
                context.rng,
                1,
                "arborec_infantry2",
                Some(LW2_THRESHOLD),
                &owner,
            );
            if roll
                .faces
                .first()
                .is_some_and(|face| *face >= LW2_THRESHOLD)
            {
                let now = on_card(context.state, &owner) + 1;
                context
                    .state
                    .faction_marks
                    .insert(card_key(&owner), now.to_string());
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && event.text("unit") == Some("arborec_infantry2")
            && has_technology(context.state, &condition_owner, "lw2")
    }))
}

/// "At the start of your next turn, place each unit that is on this card on a planet you control in
/// your home system." A unit with no such planet stays on the card.
fn warrior_returns(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:arborec_infantry2_return:TURN_BEGAN:after"),
        seat.clone(),
        "TURN_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let waiting = on_card(context.state, &owner);
            let Some(home) = context
                .state
                .player(&owner)
                .and_then(|seat| seat.home_system.clone())
            else {
                return Ok(());
            };
            let mut landings: Vec<PlanetId> = Vec::new();
            for _ in 0..waiting {
                let spots = placement_spots(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                    PlacementTarget::ControlledPlanet,
                    Some(&home),
                );
                let planets: Vec<PlanetId> =
                    spots.into_iter().filter_map(|(_, planet)| planet).collect();
                let planet = match planets.as_slice() {
                    [] => break,
                    [only] => only.clone(),
                    _ => {
                        let options: Vec<ChoiceOption> = planets
                            .iter()
                            .map(|planet| {
                                ChoiceOption::labelled(
                                    planet.to_string(),
                                    "lw2",
                                    format!("place a Letani Warrior on {planet}"),
                                )
                            })
                            .collect();
                        let choice = Choice::new(
                            owner.clone(),
                            "Letani Warrior II: where in your home system".to_owned(),
                            options,
                        )
                        .contextualized(decision(
                            context.state,
                            &owner,
                            "lw2",
                            "lw2_return",
                        ));
                        let answer = context
                            .ask_seeing(&choice)
                            .map_err(TimingError::IllegalChoice)?;
                        let Some(planet) = planets.iter().find(|p| p.as_str() == answer.id) else {
                            break;
                        };
                        planet.clone()
                    }
                };
                landings.push(planet);
            }
            let mut placed = 0;
            for planet in &landings {
                placed += place_units_counted(context, &owner, &home, Some(planet), "infantry", 1);
            }
            let left = waiting.saturating_sub(placed);
            if left == 0 {
                context.state.faction_marks.remove(&card_key(&owner));
            } else {
                context
                    .state
                    .faction_marks
                    .insert(card_key(&owner), left.to_string());
            }
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && on_card(context.state, &condition_owner) > 0
    }))
}

// -- Duha Menaimon and Dirzuga Rophal ------------------------------------------------------------

/// Whether the player has their flagship in the system.
fn flagship_in(state: &GameState, player: &PlayerId, system: &SystemId) -> bool {
    super::has_flagship_text_in(state, player, system, "arborec_flagship")
}

/// Duha Menaimon: "After you activate this system, you may produce up to 5 units in this system."
fn flagship(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("unit:{owner_name}:arborec_flagship:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if event_player(event).as_ref() != Some(&owner)
                || !flagship_in(context.state, &owner, &system)
            {
                return Ok(());
            }
            produce(context, Some(resolver), &owner, &system, FLAGSHIP_LIMIT)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && event
                .text("system")
                .map(SystemId::new)
                .is_some_and(|system| {
                    flagship_in(context.state, &condition_owner, &system)
                        && can_produce(
                            context.state,
                            context.content,
                            context.sources,
                            &condition_owner,
                            &system,
                            FLAGSHIP_LIMIT,
                        )
                })
    }))
}

/// The commander can act on this activation: unlocked, another player activated, and the system
/// holds one of the owner's units with PRODUCTION that could build something.
fn commander_ready(
    context: &TimingContext<'_>,
    owner: &PlayerId,
    event: &crate::event::Event,
) -> Option<SystemId> {
    if !crate::promissory::has_commander_ability(context.state, owner, "arboreccommander") {
        return None;
    }
    let activator = event_player(event)?;
    if &activator == owner {
        return None;
    }
    let system = SystemId::new(event.text("system")?);
    if crate::production::producers(
        context.state,
        context.content,
        context.sources,
        owner,
        &system,
    )
    .is_empty()
        || !can_produce(
            context.state,
            context.content,
            context.sources,
            owner,
            &system,
            1,
        )
    {
        return None;
    }
    Some(system)
}

/// Dirzuga Rophal: "After another player activates a system that contains 1 or more of your units
/// that have PRODUCTION: You may produce 1 unit in that system."
fn commander(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("leader:{owner_name}:arboreccommander:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, resolver, context| {
            let Some(system) = commander_ready(context, &owner, event) else {
                return Ok(());
            };
            produce(context, Some(resolver), &owner, &system, 1)?;
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        commander_ready(context, &condition_owner, event).is_some()
    }))
}

/// "Have 12 ground forces on planets you control."
fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != "arboreccommander" {
        return None;
    }
    let types = catalogue(content, sources);
    let held: usize = state
        .controlled_planets(player)
        .into_iter()
        .map(|(system, planet)| {
            state
                .system_state(system)
                .on_planet_of(planet, player)
                .into_iter()
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_ground_force)
                })
                .count()
        })
        .sum();
    Some(held >= 12)
}

// -- Stymie --------------------------------------------------------------------------------------

/// Systems the mover could be given a command token in: not a home system, no token of theirs
/// there, and a token in their reinforcements. Every system of the map when there is one (an empty
/// system has no board entry), plus any with a board entry.
fn stymie_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    mover: &PlayerId,
) -> Vec<SystemId> {
    let mut all: BTreeSet<SystemId> = state.board.keys().cloned().collect();
    if let Some(galaxy) = galaxy {
        all.extend(
            galaxy
                .system_ids_holding_things()
                .into_iter()
                .map(SystemId::new),
        );
    }
    all.into_iter()
        .filter(|system| !ti4_content::galaxy::is_home_system(content, system.as_str(), sources))
        .filter(|system| crate::tokens::can_place_command_token(state, mover, system))
        .collect()
}

/// Whether the holder has a unit of any kind in the system.
fn has_units_in(state: &GameState, holder: &PlayerId, system: &SystemId) -> bool {
    let board = state.system_state(system);
    !board.units_of(holder).is_empty()
        || board
            .planet_units
            .values()
            .any(|units| units.iter().any(|unit| &unit.owner == holder))
}

fn stymie_ready(
    context: &TimingContext<'_>,
    holder: &PlayerId,
    event: &crate::event::Event,
) -> Option<(PlayerId, Vec<SystemId>)> {
    let mover = event_player(event)?;
    let system = SystemId::new(event.text("system")?);
    if &mover == holder
        || context.state.promissory_notes.get(STYMIE_NOTE) != Some(holder)
        || crate::promissory::held_foreign(context.state, holder, "stymie") == 0
        || !has_units_in(context.state, holder, &system)
    {
        return None;
    }
    let systems = stymie_systems(
        context.state,
        context.content,
        context.sources,
        context.galaxy,
        &mover,
    );
    (!systems.is_empty()).then_some((mover, systems))
}

/// Stymie, played by whoever holds it. "After another player moves ships into a system that
/// contains 1 or more of your units: You may place 1 command token from that player's
/// reinforcements in any non-home system. Then, return this card to the Arborec player."
fn stymie(owner_name: &str, seat: &PlayerId) -> Ability {
    let holder = seat.clone();
    let condition_holder = seat.clone();
    Ability::stateful(
        format!("promissory:{owner_name}:stymie:SHIP_MOVED:after"),
        seat.clone(),
        "SHIP_MOVED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some((mover, systems)) = stymie_ready(context, &holder, event) else {
                return Ok(());
            };
            let target = if let [only] = systems.as_slice() {
                only.clone()
            } else {
                {
                    let options: Vec<ChoiceOption> = systems
                        .iter()
                        .map(|system| {
                            ChoiceOption::labelled(
                                system.to_string(),
                                "stymie",
                                format!("place {mover}'s command token in {system}"),
                            )
                        })
                        .collect();
                    let choice = Choice::new(
                        holder.clone(),
                        format!("Stymie: where to put {mover}'s command token"),
                        options,
                    )
                    .contextualized(decision(
                        context.state,
                        &holder,
                        "stymie",
                        "stymie_system",
                    ));
                    let answer = context
                        .ask_seeing(&choice)
                        .map_err(TimingError::IllegalChoice)?;
                    let Some(system) = systems.iter().find(|s| s.as_str() == answer.id) else {
                        return Ok(());
                    };
                    system.clone()
                }
            };
            if crate::tokens::place_command_token_from_reinforcements(
                context.state,
                &mover,
                &target,
            ) {
                crate::promissory::give_back(context.state, STYMIE_NOTE);
            }
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        stymie_ready(context, &condition_holder, event).is_some()
    }))
}

// -- Letani Ospha --------------------------------------------------------------------------------

/// One ship Letani Ospha can act on, with what its owner could replace it with.
struct ShipTarget {
    system: SystemId,
    ship: Unit,
    replacements: Vec<String>,
}

/// Cost in half-units, so "up to 2 more" is exact integer arithmetic (fighters and infantry cost
/// halves; no ship does, but the comparison does not care).
#[allow(
    clippy::cast_possible_truncation,
    reason = "unit costs are small multiples of 0.5"
)]
fn half_cost(kind: UnitType<'_>) -> i64 {
    (kind.cost() * 2.0).round() as i64
}

fn is_plain_ship(kind: UnitType<'_>) -> bool {
    kind.is_ship() && !kind.is_fighter()
}

/// Ship base types the corpus knows, in a fixed order.
fn ship_bases(content: &ContentStore, sources: SourceSet) -> BTreeSet<String> {
    catalogue(content, sources)
        .values()
        .filter(|kind| is_plain_ship(**kind))
        .map(|kind| kind.base_type().to_owned())
        .collect()
}

/// Every ship in play (anyone's) that its owner could swap for something dearer by at most 2.
fn ship_targets(state: &GameState, content: &ContentStore, sources: SourceSet) -> Vec<ShipTarget> {
    let types = catalogue(content, sources);
    let bases = ship_bases(content, sources);
    let mut found: Vec<ShipTarget> = Vec::new();
    for (system, board) in &state.board {
        for ship in &board.units {
            let Some(current) = types.get(ship.type_id.as_str()) else {
                continue;
            };
            if !is_plain_ship(*current)
                || found
                    .iter()
                    .any(|target| &target.system == system && target.ship == *ship)
            {
                continue;
            }
            let ceiling = half_cost(*current) + 4;
            let replacements: Vec<String> = bases
                .iter()
                .filter(|base| base.as_str() != current.base_type())
                .filter(|base| {
                    player_unit(state, content, sources, &ship.owner, base).is_some_and(|kind| {
                        is_plain_ship(kind)
                            && half_cost(kind) <= ceiling
                            && box_has(state, content, sources, &ship.owner, base)
                    })
                })
                .cloned()
                .collect();
            if !replacements.is_empty() {
                found.push(ShipTarget {
                    system: system.clone(),
                    ship: ship.clone(),
                    replacements,
                });
            }
        }
    }
    found
}

#[allow(
    clippy::too_many_lines,
    reason = "one card: every question is asked before the ship is replaced"
)]
fn agent_use(context: &mut TimingContext<'_>, player: &PlayerId) -> Result<bool, TimingError> {
    let targets = ship_targets(context.state, context.content, context.sources);
    let target = match targets.as_slice() {
        [] => return Ok(false),
        [only] => only,
        _ => {
            let options: Vec<ChoiceOption> = targets
                .iter()
                .enumerate()
                .map(|(index, target)| {
                    ChoiceOption::labelled(
                        format!("ship|{index}"),
                        "ship",
                        format!(
                            "{}'s {}{} in {}",
                            target.ship.owner,
                            target.ship.type_id,
                            if target.ship.sustained_damage {
                                " (damaged)"
                            } else {
                                ""
                            },
                            target.system
                        ),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "Letani Ospha: choose a non-fighter ship".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                player,
                "arborecagent",
                "ospha_ship",
            ));
            let answer = context
                .ask_seeing(&choice)
                .map_err(TimingError::IllegalChoice)?;
            let Some(target) = choice
                .options
                .iter()
                .position(|option| option.id == answer.id)
                .and_then(|index| targets.get(index))
            else {
                return Ok(false);
            };
            target
        }
    };
    // That player may replace it: they are asked, and may decline. The card is used either way.
    let owner = target.ship.owner.clone();
    let mut options: Vec<ChoiceOption> = target
        .replacements
        .iter()
        .map(|base| {
            ChoiceOption::labelled(
                format!("replace|{base}"),
                "ship",
                format!("replace it with a {base} from reinforcements"),
            )
        })
        .collect();
    options.push(ChoiceOption::decline());
    let choice = Choice::new(
        owner.clone(),
        format!(
            "Letani Ospha: replace {}'s {} in {}",
            owner, target.ship.type_id, target.system
        ),
        options,
    )
    .contextualized(decision(
        context.state,
        &owner,
        "arborecagent",
        "ospha_replace",
    ));
    let answer = context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)?;
    let Some(base) = answer.id.strip_prefix("replace|") else {
        return Ok(true);
    };
    let Some(kind) = player_unit(
        context.state,
        context.content,
        context.sources,
        &owner,
        base,
    ) else {
        return Ok(true);
    };
    let new_id = UnitTypeId::new(kind.id());
    let Some(board) = context.state.board.get_mut(&target.system) else {
        return Ok(true);
    };
    let Some(index) = board.units.iter().position(|unit| *unit == target.ship) else {
        return Ok(true);
    };
    board.units[index] = Unit::new(new_id, owner);
    Ok(true)
}

// -- Letani Miasmiala ----------------------------------------------------------------------------

/// Systems that hold one of the player's ground forces, in board order.
fn ground_force_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    let types = catalogue(content, sources);
    state
        .board
        .iter()
        .filter(|(_, board)| {
            board
                .units
                .iter()
                .chain(board.planet_units.values().flatten())
                .any(|unit| {
                    &unit.owner == player
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(UnitType::is_ground_force)
                })
        })
        .map(|(system, _)| system.clone())
        .collect()
}

fn hero_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    ground_force_systems(state, content, sources, player)
        .into_iter()
        .filter(|system| can_produce(state, content, sources, player, system, HERO_LIMIT))
        .collect()
}

// -- component actions: leaders ------------------------------------------------------------------

fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    // The hook is not given the sources; the corpus default is what games are built with.
    let sources = ti4_model::content_types::DEFAULT;
    match leader.as_str() {
        "arborecagent" => Some(!ship_targets(state, content, sources).is_empty()),
        "arborechero" => Some(!hero_systems(state, content, sources, player).is_empty()),
        _ => None,
    }
}

fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    match leader.as_str() {
        "arborecagent" => agent_use(context, player).ok(),
        "arborechero" => {
            let systems = hero_systems(context.state, context.content, context.sources, player);
            if systems.is_empty() {
                return Some(false);
            }
            let mut made = 0;
            for system in &systems {
                // An earlier system's purchases may have emptied the box or the purse.
                if !can_produce(
                    context.state,
                    context.content,
                    context.sources,
                    player,
                    system,
                    HERO_LIMIT,
                ) {
                    continue;
                }
                match produce(context, None, player, system, HERO_LIMIT) {
                    Ok(count) => made += count,
                    // Units already placed and paid for stay: the card is used.
                    Err(_) => return Some(made > 0),
                }
            }
            Some(made > 0)
        }
        _ => None,
    }
}

// -- Psychospore ---------------------------------------------------------------------------------

/// Systems Psychospore can act in: the player's command token is there, so is one of their infantry,
/// and a planet they control in it can take the new infantry.
fn psychospore_systems(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    if !crate::breakthroughs::holds(state, player, "arborecbt")
        || state.faction_marks.contains_key(&exhausted_key(player))
        || !box_has(state, content, sources, player, "infantry")
    {
        return Vec::new();
    }
    let types = catalogue(content, sources);
    state
        .board
        .iter()
        .filter(|(_, board)| board.command_tokens.contains(player))
        .filter(|(_, board)| {
            board
                .units
                .iter()
                .chain(board.planet_units.values().flatten())
                .any(|unit| &unit.owner == player && is_base(&types, unit, "infantry"))
        })
        .filter(|(system, _)| {
            !placement_spots(
                state,
                content,
                sources,
                player,
                PlacementTarget::ControlledPlanet,
                Some(system),
            )
            .is_empty()
        })
        .map(|(system, _)| system.clone())
        .collect()
}

fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    let sources = ti4_model::content_types::DEFAULT;
    if psychospore_systems(state, content, sources, player).is_empty() {
        return Vec::new();
    }
    vec![ChoiceOption::labelled(
        PSYCHOSPORE_ACTION,
        crate::faction_abilities::ACTION_KIND,
        "Psychospore: exhaust to swap a command token for an infantry",
    )]
}

fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    if option.id != PSYCHOSPORE_ACTION {
        return false;
    }
    let systems = psychospore_systems(context.state, context.content, context.sources, player);
    let system = match systems.as_slice() {
        [] => return false,
        [only] => only.clone(),
        _ => {
            let options: Vec<ChoiceOption> = systems
                .iter()
                .map(|system| {
                    ChoiceOption::labelled(
                        system.to_string(),
                        "psychospore",
                        format!("remove the command token from {system}"),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "Psychospore: which command token".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                player,
                "arborecbt",
                "psychospore_system",
            ));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return false;
            };
            let Some(system) = systems.iter().find(|s| s.as_str() == answer.id) else {
                return false;
            };
            system.clone()
        }
    };
    let planets: Vec<PlanetId> = placement_spots(
        context.state,
        context.content,
        context.sources,
        player,
        PlacementTarget::ControlledPlanet,
        Some(&system),
    )
    .into_iter()
    .filter_map(|(_, planet)| planet)
    .collect();
    let planet = match planets.as_slice() {
        [] => return false,
        [only] => only.clone(),
        _ => {
            let options: Vec<ChoiceOption> = planets
                .iter()
                .map(|planet| {
                    ChoiceOption::labelled(
                        planet.to_string(),
                        "psychospore",
                        format!("place 1 infantry on {planet}"),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "Psychospore: where the infantry goes".to_owned(),
                options,
            )
            .contextualized(decision(
                context.state,
                player,
                "arborecbt",
                "psychospore_planet",
            ));
            let Ok(answer) = context.ask_seeing(&choice) else {
                return false;
            };
            let Some(planet) = planets.iter().find(|p| p.as_str() == answer.id) else {
                return false;
            };
            planet.clone()
        }
    };
    // Everything is decided: exhaust, return the token, place the infantry.
    context
        .state
        .faction_marks
        .insert(exhausted_key(player), "1".to_owned());
    context
        .state
        .system_mut(&system)
        .command_tokens
        .remove(player);
    place_units_counted(context, player, &system, Some(&planet), "infantry", 1);
    true
}

/// The card readies with everything else in the status phase.
fn psychospore_readies(owner_name: &str, seat: &PlayerId) -> Ability {
    let owner = seat.clone();
    let condition_owner = seat.clone();
    Ability::stateful(
        format!("breakthrough:{owner_name}:arborecbt_ready:STATUS_PHASE_ENDED:after"),
        seat.clone(),
        "STATUS_PHASE_ENDED",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            context.state.faction_marks.remove(&exhausted_key(&owner));
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        context
            .state
            .faction_marks
            .contains_key(&exhausted_key(&condition_owner))
    }))
}

// -- timing abilities ----------------------------------------------------------------------------

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![commander(owner_name, seat), stymie(owner_name, seat)];
    // A Nekro flagship may carry the flagship's text (Valefar Assimilator Z); the condition decides.
    if state
        .player(seat)
        .is_some_and(|player| player.faction.as_str() == super::nekro::FACTION)
    {
        abilities.push(flagship(owner_name, seat));
    }
    if state
        .player(seat)
        .is_some_and(|player| player.faction.as_str() == "arborec")
    {
        abilities.extend([
            mitosis(owner_name, seat),
            bioplasmosis(owner_name, seat),
            warrior_rolls(owner_name, seat),
            warrior_returns(owner_name, seat),
            flagship(owner_name, seat),
            psychospore_readies(owner_name, seat),
        ]);
    }
    abilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::TechnologyId;
    use ti4_model::state::LeaderStatus;

    use crate::choice::{Decider, IllegalChoice, Scripted, Table};

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }

    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }

    fn home_of(state: &GameState, who: &PlayerId) -> SystemId {
        state.player(who).unwrap().home_system.clone().unwrap()
    }

    fn payload(pairs: &[(&str, &str)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), serde_json::Value::from(*value)))
            .collect()
    }

    /// Answers the queue in order, then stops (`done_producing`), declines, or takes the first
    /// option; remembers every prompt it saw.
    struct Steer {
        queue: VecDeque<String>,
        asked: Arc<Mutex<Vec<String>>>,
    }

    impl Decider for Steer {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.asked.lock().unwrap().push(choice.prompt.clone());
            if let Some(wanted) = self.queue.pop_front() {
                return choice.option(&wanted).cloned().ok_or_else(|| {
                    IllegalChoice::ScriptDiverged {
                        player: choice.player.clone(),
                        wanted,
                        offered: choice.ids().into_iter().map(str::to_owned).collect(),
                    }
                });
            }
            Ok(choice
                .option("done_producing")
                .or_else(|| choice.options.iter().find(|o| o.is_decline()))
                .unwrap_or(&choice.options[0])
                .clone())
        }
    }

    fn steered(answers: &[&str]) -> (Table, Arc<Mutex<Vec<String>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let table = Table::with_default(Box::new(Steer {
            queue: answers.iter().map(|a| (*a).to_owned()).collect(),
            asked: asked.clone(),
        }));
        (table, asked)
    }

    /// A table that fails the test if anything is asked.
    fn silent() -> Table {
        Table::with_default(Box::new(Scripted::new(["never offered"])))
    }

    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(
            answers.iter().map(|answer| (*answer).to_owned()),
        )))
    }

    /// Emit one typed event through a resolver armed exactly as the game arms it, with these dice
    /// faces loaded first. Returns how many dice were rolled.
    fn emit_with(
        state: &mut GameState,
        table: &mut Table,
        galaxy: Option<&ti4_content::galaxy::Galaxy>,
        faces: &[u32],
        event_type: &str,
        pairs: &[(&str, &str)],
    ) -> usize {
        let mut resolver = crate::fixtures::armed_resolver(state);
        let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut context = TimingContext {
            state,
            content: ContentStore::embedded(),
            sources: DEFAULT,
            table,
            dice: &mut dice,
            rng: &mut rng,
            event_sequence: &mut sequence,
            galaxy,
        };
        let event = context
            .event_sequence
            .next(event_type, payload(pairs))
            .expect("an event id");
        resolver
            .emit_with_context(&mut context, event, |_, _| {})
            .expect("the window resolves");
        context.dice.history().len()
    }

    fn emit(state: &mut GameState, table: &mut Table, event_type: &str, pairs: &[(&str, &str)]) {
        emit_with(state, table, None, &[], event_type, pairs);
    }

    fn infantry_total(state: &GameState, who: &PlayerId) -> usize {
        let content = ContentStore::embedded();
        state
            .controlled_planets(who)
            .into_iter()
            .map(|(system, planet)| infantry_on(state, content, DEFAULT, system, planet, who))
            .sum()
    }

    fn on_planet(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        who: &PlayerId,
        kind: &str,
    ) -> usize {
        state
            .system_state(system)
            .on_planet_of(planet, who)
            .into_iter()
            .filter(|unit| unit.type_id.as_str() == kind)
            .count()
    }

    fn count(state: &GameState, system: &SystemId, who: &PlayerId, kind: &str) -> usize {
        state
            .system_state(system)
            .units
            .iter()
            .filter(|unit| &unit.owner == who && unit.type_id.as_str() == kind)
            .count()
    }

    fn planets_in_home(state: &GameState, who: &PlayerId) -> Vec<PlanetId> {
        let home = home_of(state, who);
        state
            .controlled_planets(who)
            .into_iter()
            .filter(|(system, _)| **system == home)
            .map(|(_, planet)| planet.clone())
            .collect()
    }

    fn give_goods(state: &mut GameState, who: &PlayerId, goods: i32) {
        state.player_mut(who).unwrap().trade_goods += goods;
    }

    fn set_leader(state: &mut GameState, who: &PlayerId, leader: &str, status: LeaderStatus) {
        state
            .player_mut(who)
            .unwrap()
            .leaders
            .insert(LeaderId::new(leader), status);
    }

    fn with_context<T>(
        state: &mut GameState,
        table: &mut Table,
        run: impl FnOnce(&mut TimingContext<'_>) -> T,
    ) -> T {
        crate::fixtures::with_context(state, DEFAULT, None, table, run)
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_game_with_no_arborec_seat_is_offered_nothing_of_theirs() {
        let content = ContentStore::embedded();
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        let home = home_of(&state, &a());
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put(&mut state, &home, "cruiser", &a(), 1);
        give_goods(&mut state, &a(), 10);
        state.system_mut(&home).place_token(a());
        let before = state.clone();
        // Every window an Arborec card hangs on, with a table that fails if anything is asked and
        // dice that would be counted if anything rolled.
        for (event, pairs) in [
            ("STATUS_PHASE_BEGAN", vec![]),
            ("STATUS_PHASE_ENDED", vec![]),
            ("TURN_BEGAN", vec![("player", "a")]),
            (
                "SYSTEM_ACTIVATED",
                vec![("player", "b"), ("system", home.as_str())],
            ),
            (
                "SYSTEM_ACTIVATED",
                vec![("player", "a"), ("system", home.as_str())],
            ),
            (
                "SHIP_MOVED",
                vec![("player", "b"), ("system", home.as_str()), ("origin", "1")],
            ),
            (
                "GROUND_FORCE_DESTROYED",
                vec![
                    ("player", "a"),
                    ("unit", "arborec_infantry2"),
                    ("system", system.as_str()),
                    ("planet", planet.as_str()),
                ],
            ),
        ] {
            let rolled = emit_with(&mut state, &mut silent(), None, &[], event, &pairs);
            assert_eq!(rolled, 0, "{event} rolled a die");
        }
        assert_eq!(state, before, "no Arborec card changed the game");
        assert!(
            crate::factions::component_actions(&state, content, &a()).is_empty(),
            "no module offers a component action to a game without the Arborec"
        );
        for leader in ["arborecagent", "arboreccommander", "arborechero"] {
            assert!(
                !crate::leaders::component_actions(&state, content, &a())
                    .iter()
                    .any(|option| option.id.contains(leader)),
                "{leader}"
            );
        }
        assert!(!crate::factions::hooks_economy::cannot_produce(
            &state,
            content,
            &a(),
            "infantry",
            "spacedock"
        ));
    }

    // -- Mitosis ---------------------------------------------------------------------------------

    #[test]
    fn space_docks_cannot_produce_infantry_and_only_the_arborec_are_barred() {
        let content = ContentStore::embedded();
        let state = game();
        let barred = |who: &PlayerId, unit: &str, producer: &str| {
            crate::factions::hooks_economy::cannot_produce(&state, content, who, unit, producer)
        };
        assert!(barred(&a(), "infantry", "spacedock"));
        assert!(!barred(&a(), "mech", "spacedock"), "only infantry");
        assert!(!barred(&a(), "cruiser", "spacedock"), "only infantry");
        assert!(
            !barred(&a(), "infantry", "infantry"),
            "only space docks: Letani Warriors still produce"
        );
        assert!(!barred(&b(), "infantry", "spacedock"), "only the Arborec");
    }

    #[test]
    fn a_dock_alone_is_no_spot_for_infantry_but_a_letani_warrior_is_a_producer() {
        let content = ContentStore::embedded();
        let mut state = game();
        let home = home_of(&state, &a());
        let infantry = *catalogue(content, DEFAULT).get("infantry").unwrap();
        let planet = planets_in_home(&state, &a())[0].clone();
        let spots = |state: &GameState| {
            crate::production::placements(state, content, DEFAULT, &a(), &home, &infantry)
        };
        assert!(
            !spots(&state).is_empty(),
            "a Letani Warrior has PRODUCTION of its own"
        );
        state
            .system_mut(&home)
            .planet_units
            .values_mut()
            .for_each(|units| {
                units.retain(|unit| unit.owner != a() || unit.type_id.as_str() == "spacedock");
            });
        crate::fixtures::put_on_planet(&mut state, &home, &planet, "spacedock", &a(), 1);
        assert!(
            spots(&state).is_empty(),
            "with only docks left there is nowhere to produce infantry"
        );
        // Sol's dock is not barred.
        let mut sol = crate::fixtures::seated_game(&[("a", "sol"), ("b", FACTION)], DEFAULT);
        let sol_home = home_of(&sol, &a());
        let sol_planet = planets_in_home(&sol, &a())[0].clone();
        sol.system_mut(&sol_home)
            .planet_units
            .values_mut()
            .for_each(|units| units.retain(|unit| unit.type_id.as_str() == "spacedock"));
        crate::fixtures::put_on_planet(&mut sol, &sol_home, &sol_planet, "spacedock", &a(), 1);
        assert!(
            !crate::production::placements(&sol, content, DEFAULT, &a(), &sol_home, &infantry)
                .is_empty()
        );
    }

    #[test]
    fn mitosis_places_an_infantry_on_the_planet_you_choose() {
        let mut state = game();
        let (system, target) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(target.clone(), a());
        let before = infantry_total(&state, &a());
        emit(
            &mut state,
            &mut scripted(&[&format!("infantry|{system}|{target}")]),
            "STATUS_PHASE_BEGAN",
            &[],
        );
        assert_eq!(infantry_total(&state, &a()), before + 1);
        assert_eq!(
            on_planet(&state, &system, &target, &a(), "arborec_infantry"),
            1,
            "on the planet chosen, not the home planet"
        );
    }

    #[test]
    fn mitosis_is_not_a_question_with_one_planet_and_no_mech_to_spare() {
        let mut state = game();
        let home = home_of(&state, &a());
        for _ in 0..4 {
            crate::fixtures::put(&mut state, &home, "arborec_mech", &a(), 1);
        }
        let before = infantry_total(&state, &a());
        emit(&mut state, &mut silent(), "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(infantry_total(&state, &a()), before + 1);
    }

    #[test]
    fn the_behemoth_may_replace_an_infantry_instead() {
        let mut state = game();
        let home = home_of(&state, &a());
        let planet = planets_in_home(&state, &a())[0].clone();
        let infantry_before = infantry_total(&state, &a());
        let mechs_before = on_planet(&state, &home, &planet, &a(), "arborec_mech");
        assert!(
            on_planet(&state, &home, &planet, &a(), "arborec_infantry") > 0,
            "an infantry to replace"
        );
        emit(
            &mut state,
            &mut scripted(&[&format!("mech|{home}|{planet}")]),
            "STATUS_PHASE_BEGAN",
            &[],
        );
        assert_eq!(
            infantry_total(&state, &a()),
            infantry_before - 1,
            "one infantry was replaced, none placed"
        );
        assert_eq!(
            on_planet(&state, &home, &planet, &a(), "arborec_mech"),
            mechs_before + 1
        );
    }

    #[test]
    fn the_mech_replacement_needs_a_mech_in_the_box_and_an_infantry_to_replace() {
        let content = ContentStore::embedded();
        let mut state = game();
        let moves = |state: &GameState| mitosis_moves(state, content, DEFAULT, &a());
        assert!(moves(&state).iter().any(|step| step.replace));
        let home = home_of(&state, &a());
        for _ in 0..4 {
            crate::fixtures::put(&mut state, &home, "arborec_mech", &a(), 1);
        }
        assert!(
            !moves(&state).iter().any(|step| step.replace),
            "no mech in reinforcements"
        );
        assert!(moves(&state).iter().any(|step| !step.replace));
        let mut state = game();
        state.board.values_mut().for_each(|board| {
            board
                .planet_units
                .values_mut()
                .for_each(|units| units.retain(|unit| unit.type_id.as_str() != "arborec_infantry"));
        });
        assert!(!moves(&state).iter().any(|step| step.replace));
    }

    #[test]
    fn with_no_planet_to_receive_it_the_replacement_is_a_declinable_may() {
        let mut state = game();
        state.board.values_mut().for_each(|board| {
            board.planet_control.retain(|_, owner| owner != &a());
        });
        let before = state.clone();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "STATUS_PHASE_BEGAN",
            &[],
        );
        assert_eq!(state, before, "declined: nothing happened");
        // With no infantry either, there is nothing to ask.
        state.board.values_mut().for_each(|board| {
            board.planet_units.values_mut().for_each(|units| {
                units.retain(|unit| unit.type_id.as_str() != "arborec_infantry");
            });
        });
        let before = state.clone();
        emit(&mut state, &mut silent(), "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(state, before);
    }

    // -- the units' printed statistics -----------------------------------------------------------

    #[test]
    fn the_unit_sheet_is_what_the_cards_print() {
        let content = ContentStore::embedded();
        let types = catalogue(content, DEFAULT);
        let get = |id: &str| *types.get(id).unwrap_or_else(|| panic!("{id}"));
        let warrior = get("arborec_infantry");
        assert_eq!(
            (warrior.combat_hits_on(), warrior.production(0)),
            (Some(8), 1)
        );
        assert!((warrior.cost() - 0.5).abs() < f64::EPSILON, "1 for 2");
        let warrior2 = get("arborec_infantry2");
        assert_eq!(
            (warrior2.combat_hits_on(), warrior2.production(0)),
            (Some(7), 2)
        );
        assert_eq!(warrior2.upgrades_from(), Some("arborec_infantry"));
        assert_eq!(warrior2.required_technology(), Some("lw2"));
        let mech = get("arborec_mech");
        assert_eq!(mech.combat_hits_on(), Some(6));
        assert!(mech.sustain_damage() && mech.planetary_shield());
        assert!((mech.cost() - 2.0).abs() < f64::EPSILON);
        let flagship = get("arborec_flagship");
        assert_eq!(
            (
                flagship.combat_hits_on(),
                flagship.combat_dice(),
                flagship.move_value(),
                flagship.capacity()
            ),
            (Some(7), 2, 1, 5)
        );
        assert!(flagship.sustain_damage());
        assert!((flagship.cost() - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn owning_letani_warrior_ii_places_the_upgraded_unit() {
        let content = ContentStore::embedded();
        let mut state = game();
        let kind = |state: &GameState| {
            player_unit(state, content, DEFAULT, &a(), "infantry")
                .unwrap()
                .id()
                .to_owned()
        };
        assert_eq!(kind(&state), "arborec_infantry");
        state
            .player_mut(&a())
            .unwrap()
            .technologies
            .insert(TechnologyId::new("lw2"));
        assert_eq!(kind(&state), "arborec_infantry2");
    }

    // -- Bioplasmosis ----------------------------------------------------------------------------

    const BIO: &str = "technology:arborec:bio:STATUS_PHASE_ENDED:after";

    fn learn(state: &mut GameState, who: &PlayerId, tech: &str) {
        state
            .player_mut(who)
            .unwrap()
            .technologies
            .insert(TechnologyId::new(tech));
    }

    /// Take every unit and planet away from the Arborec seat, so only what a test adds exists.
    fn bare(state: &mut GameState) {
        state.board.values_mut().for_each(|board| {
            board.planet_control.retain(|_, owner| owner != &a());
            board
                .planet_units
                .values_mut()
                .for_each(|units| units.retain(|unit| unit.owner != a()));
            board.units.retain(|unit| unit.owner != a());
        });
    }

    fn claim(state: &mut GameState, system: &str, planet: &str, infantry: usize) {
        let system = SystemId::new(system);
        let planet = PlanetId::new(planet);
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(state, &system, &planet, "arborec_infantry", &a(), infantry);
    }

    fn there(state: &GameState, system: &str, planet: &str) -> usize {
        on_planet(
            state,
            &SystemId::new(system),
            &PlanetId::new(planet),
            &a(),
            "arborec_infantry",
        )
    }

    #[test]
    fn bioplasmosis_moves_infantry_within_a_system_one_at_a_time() {
        let mut state = game();
        bare(&mut state);
        learn(&mut state, &a(), "bio");
        let (system, _) = crate::fixtures::a_placed_planet();
        claim(&mut state, system.as_str(), "src", 2);
        claim(&mut state, system.as_str(), "dst", 0);
        emit(
            &mut state,
            &mut scripted(&[BIO, "move|0", "move|0"]),
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(
            there(&state, system.as_str(), "src"),
            0,
            "both were removed"
        );
        assert_eq!(there(&state, system.as_str(), "dst"), 2, "and placed");
    }

    #[test]
    fn bioplasmosis_may_stop_after_some_or_decline_outright() {
        let mut state = game();
        bare(&mut state);
        learn(&mut state, &a(), "bio");
        let (system, _) = crate::fixtures::a_placed_planet();
        claim(&mut state, system.as_str(), "src", 2);
        claim(&mut state, system.as_str(), "dst", 0);
        emit(
            &mut state,
            &mut scripted(&[BIO, "move|0", "decline"]),
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(there(&state, system.as_str(), "src"), 1);
        assert_eq!(there(&state, system.as_str(), "dst"), 1);
        let before = state.clone();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(state, before, "declined: nothing moved");
    }

    #[test]
    fn bioplasmosis_reaches_adjacent_systems_and_no_further() {
        let hub = crate::fixtures::plain_hub();
        let near = hub.centre.clone();
        let source = hub.outer[0].clone();
        let far = hub.across(&source);
        let mut state = game();
        bare(&mut state);
        learn(&mut state, &a(), "bio");
        claim(&mut state, &source, "src", 1);
        claim(&mut state, &near, "near", 0);
        claim(&mut state, &far, "far", 0);
        // The only reachable planet is the one beside the source; "far" is two systems away.
        emit_with(
            &mut state,
            &mut scripted(&[BIO, "move|0"]),
            Some(&hub.galaxy),
            &[],
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(there(&state, &source, "src"), 0);
        assert_eq!(there(&state, &near, "near"), 1);
        assert_eq!(there(&state, &far, "far"), 0);
    }

    #[test]
    fn bioplasmosis_is_not_offered_without_the_card_or_a_reachable_planet() {
        let hub = crate::fixtures::plain_hub();
        let source = hub.outer[0].clone();
        let far = hub.across(&source);
        // No card.
        let mut state = game();
        bare(&mut state);
        claim(&mut state, &source, "src", 1);
        claim(&mut state, &source, "dst", 0);
        let before = state.clone();
        emit_with(
            &mut state,
            &mut silent(),
            Some(&hub.galaxy),
            &[],
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(state, before, "no Bioplasmosis");
        // The card, but the only other planet is out of reach.
        let mut state = game();
        bare(&mut state);
        learn(&mut state, &a(), "bio");
        claim(&mut state, &source, "src", 1);
        claim(&mut state, &far, "far", 0);
        let before = state.clone();
        emit_with(
            &mut state,
            &mut silent(),
            Some(&hub.galaxy),
            &[],
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(state, before, "nowhere to go");
        // Another player's planets are not destinations.
        let mut state = game();
        bare(&mut state);
        learn(&mut state, &a(), "bio");
        claim(&mut state, &source, "src", 1);
        state
            .system_mut(&SystemId::new(&source))
            .set_control(PlanetId::new("theirs"), b());
        let before = state.clone();
        emit_with(
            &mut state,
            &mut silent(),
            Some(&hub.galaxy),
            &[],
            "STATUS_PHASE_ENDED",
            &[],
        );
        assert_eq!(state, before, "a planet you do not control");
    }

    // -- Letani Warrior II -----------------------------------------------------------------------

    fn destroyed(state: &mut GameState, who: &str, unit: &str, faces: &[u32]) -> usize {
        let home = home_of(state, &a());
        emit_with(
            state,
            &mut silent(),
            None,
            faces,
            "GROUND_FORCE_DESTROYED",
            &[
                ("player", who),
                ("unit", unit),
                ("system", home.as_str()),
                ("planet", "nestphar"),
            ],
        )
    }

    #[test]
    fn a_destroyed_letani_warrior_ii_goes_on_the_card_on_a_six_or_more() {
        for (face, expected) in [(6, 1), (10, 1), (5, 0), (1, 0)] {
            let mut state = game();
            learn(&mut state, &a(), "lw2");
            let rolled = destroyed(&mut state, "a", "arborec_infantry2", &[face]);
            assert_eq!(rolled, 1, "one die");
            assert_eq!(on_card(&state, &a()), expected, "rolled {face}");
        }
    }

    #[test]
    fn units_on_the_card_accumulate() {
        let mut state = game();
        learn(&mut state, &a(), "lw2");
        destroyed(&mut state, "a", "arborec_infantry2", &[7]);
        destroyed(&mut state, "a", "arborec_infantry2", &[9]);
        assert_eq!(on_card(&state, &a()), 2);
    }

    #[test]
    fn only_the_owners_letani_warrior_ii_rolls() {
        // Letani Warrior I, another player's unit, and a seat without the upgrade: no die at all.
        let mut state = game();
        learn(&mut state, &a(), "lw2");
        assert_eq!(destroyed(&mut state, "a", "arborec_infantry", &[6]), 0);
        assert_eq!(destroyed(&mut state, "b", "arborec_infantry2", &[6]), 0);
        assert_eq!(destroyed(&mut state, "a", "infantry", &[6]), 0);
        let mut state = game();
        assert_eq!(
            destroyed(&mut state, "a", "arborec_infantry2", &[6]),
            0,
            "without lw2"
        );
        assert_eq!(on_card(&state, &a()), 0);
    }

    fn put_on_card(state: &mut GameState, count: usize) {
        state
            .faction_marks
            .insert(card_key(&a()), count.to_string());
    }

    #[test]
    fn units_on_the_card_return_to_the_home_system_at_the_start_of_your_next_turn() {
        let mut state = game();
        learn(&mut state, &a(), "lw2");
        let home = home_of(&state, &a());
        let planet = planets_in_home(&state, &a())[0].clone();
        put_on_card(&mut state, 2);
        let before = on_planet(&state, &home, &planet, &a(), "arborec_infantry2");
        emit(&mut state, &mut silent(), "TURN_BEGAN", &[("player", "b")]);
        assert_eq!(on_card(&state, &a()), 2, "not the owner's turn");
        emit(&mut state, &mut silent(), "TURN_BEGAN", &[("player", "a")]);
        assert_eq!(
            on_planet(&state, &home, &planet, &a(), "arborec_infantry2"),
            before + 2
        );
        assert_eq!(on_card(&state, &a()), 0);
    }

    #[test]
    fn each_returning_unit_may_go_to_a_different_home_planet() {
        let mut state = game();
        learn(&mut state, &a(), "lw2");
        let home = home_of(&state, &a());
        let first = planets_in_home(&state, &a())[0].clone();
        state
            .system_mut(&home)
            .set_control(PlanetId::new("zz-extra"), a());
        put_on_card(&mut state, 2);
        emit(
            &mut state,
            &mut scripted(&["zz-extra", first.as_str()]),
            "TURN_BEGAN",
            &[("player", "a")],
        );
        let extra = PlanetId::new("zz-extra");
        assert_eq!(
            on_planet(&state, &home, &extra, &a(), "arborec_infantry2"),
            1
        );
        assert_eq!(
            on_planet(&state, &home, &first, &a(), "arborec_infantry2"),
            1
        );
    }

    #[test]
    fn units_stay_on_the_card_with_no_home_planet_to_land_on() {
        let mut state = game();
        learn(&mut state, &a(), "lw2");
        let home = home_of(&state, &a());
        state.system_mut(&home).planet_control.clear();
        put_on_card(&mut state, 1);
        emit(&mut state, &mut silent(), "TURN_BEGAN", &[("player", "a")]);
        assert_eq!(on_card(&state, &a()), 1, "still waiting");
    }

    // -- Duha Menaimon ---------------------------------------------------------------------------

    const FLAGSHIP: &str = "unit:arborec:arborec_flagship:SYSTEM_ACTIVATED:after";

    fn flagship_game() -> (GameState, SystemId) {
        let mut state = game();
        let home = home_of(&state, &a());
        crate::fixtures::put(&mut state, &home, "arborec_flagship", &a(), 1);
        give_goods(&mut state, &a(), 20);
        (state, home)
    }

    #[test]
    fn the_flagship_produces_up_to_five_units_in_the_activated_system() {
        let (mut state, home) = flagship_game();
        let before = count(&state, &home, &a(), "cruiser");
        let (mut table, asked) = steered(&[FLAGSHIP, "build|cruiser|1"]);
        emit(
            &mut state,
            &mut table,
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(count(&state, &home, &a(), "cruiser"), before + 1);
        let asked = asked.lock().unwrap();
        assert!(
            asked.iter().any(|prompt| prompt.contains("(5 left)")),
            "the limit is the ability's 5, not the system's PRODUCTION: {asked:?}"
        );
    }

    #[test]
    fn the_flagship_may_be_declined_and_then_produces_nothing() {
        let (mut state, home) = flagship_game();
        let before = state.clone();
        emit(
            &mut state,
            &mut scripted(&["decline"]),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(state, before);
    }

    #[test]
    fn the_flagship_is_not_offered_to_another_activation_a_missing_ship_or_an_empty_purse() {
        let (mut state, home) = flagship_game();
        let before = state.clone();
        emit(
            &mut state,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", home.as_str())],
        );
        assert_eq!(state, before, "another player's activation");
        // The flagship is elsewhere.
        let (mut state, home) = flagship_game();
        state
            .system_mut(&home)
            .units
            .retain(|unit| unit.type_id.as_str() != "arborec_flagship");
        let before = state.clone();
        emit(
            &mut state,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(state, before, "no flagship in the system");
        // Nothing to pay with.
        let (mut state, home) = flagship_game();
        state.player_mut(&a()).unwrap().trade_goods = 0;
        let planets: Vec<PlanetId> = planets_in_home(&state, &a());
        state.exhausted_planets.extend(planets);
        let before = state.clone();
        emit(
            &mut state,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(state, before, "nothing affordable");
    }

    // -- Dirzuga Rophal --------------------------------------------------------------------------

    const COMMANDER: &str = "leader:arborec:arboreccommander:SYSTEM_ACTIVATED:after";

    #[test]
    fn the_commander_produces_one_unit_when_another_player_activates_a_system_of_yours() {
        let (mut state, home) = flagship_game();
        set_leader(&mut state, &a(), "arboreccommander", LeaderStatus::Unlocked);
        let before = count(&state, &home, &a(), "cruiser");
        let (mut table, asked) = steered(&[COMMANDER, "build|cruiser|1"]);
        emit(
            &mut state,
            &mut table,
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", home.as_str())],
        );
        assert_eq!(count(&state, &home, &a(), "cruiser"), before + 1);
        let asked = asked.lock().unwrap();
        assert!(
            asked.iter().any(|prompt| prompt.contains("(1 left)")),
            "one unit, whatever the system's PRODUCTION: {asked:?}"
        );
        assert!(
            !asked.iter().any(|prompt| prompt.contains("(2 left)")),
            "a second unit is not allowed"
        );
    }

    #[test]
    fn an_unseated_arborec_commander_grant_arms_for_another_faction() {
        let (mut state, home) = flagship_game();
        state.player_mut(&a()).unwrap().faction = ti4_model::id::FactionId::new("yin");
        assert!(crate::promissory::grant_commander_ability(
            &mut state,
            ContentStore::embedded(),
            &a(),
            "arboreccommander",
        ));
        let before = count(&state, &home, &a(), "cruiser");
        let mut table = scripted(&[
            "leader:yin:arboreccommander:SYSTEM_ACTIVATED:after",
            "build|cruiser|1",
        ]);
        emit(
            &mut state,
            &mut table,
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", home.as_str())],
        );
        assert_eq!(count(&state, &home, &a(), "cruiser"), before + 1);
        assert_eq!(
            state
                .player(&a())
                .unwrap()
                .leaders
                .get(&LeaderId::new("arboreccommander")),
            Some(&LeaderStatus::Locked)
        );
    }

    #[test]
    fn the_commander_needs_to_be_unlocked_another_players_activation_and_a_producer_there() {
        let (mut state, home) = flagship_game();
        // Locked.
        let before = state.clone();
        emit(
            &mut state,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", home.as_str())],
        );
        assert_eq!(state, before, "locked");
        set_leader(&mut state, &a(), "arboreccommander", LeaderStatus::Unlocked);
        // Your own activation is the flagship's window, not the commander's.
        let (mut own, _) = flagship_game();
        set_leader(&mut own, &a(), "arboreccommander", LeaderStatus::Unlocked);
        own.system_mut(&home)
            .units
            .retain(|unit| unit.type_id.as_str() != "arborec_flagship");
        let before = own.clone();
        emit(
            &mut own,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(own, before, "your own activation");
        // A system where you have no unit with PRODUCTION.
        let elsewhere = home_of(&state, &b());
        let before = state.clone();
        emit(
            &mut state,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "b"), ("system", elsewhere.as_str())],
        );
        assert_eq!(state, before, "no producer there");
    }

    #[test]
    fn the_commander_unlocks_with_twelve_ground_forces_on_planets_you_control() {
        let content = ContentStore::embedded();
        let mut state = game();
        let home = home_of(&state, &a());
        let planet = planets_in_home(&state, &a())[0].clone();
        let leader = LeaderId::new("arboreccommander");
        let unlocked =
            |state: &GameState| commander_unlocked(state, content, DEFAULT, None, &a(), &leader);
        let held = infantry_total(&state, &a())
            + state
                .controlled_planets(&a())
                .into_iter()
                .map(|(system, planet)| on_planet(&state, system, planet, &a(), "arborec_mech"))
                .sum::<usize>();
        assert!(held < 12);
        assert_eq!(unlocked(&state), Some(false));
        crate::fixtures::put_on_planet(
            &mut state,
            &home,
            &planet,
            "arborec_infantry",
            &a(),
            11 - held,
        );
        assert_eq!(unlocked(&state), Some(false), "eleven");
        crate::fixtures::put_on_planet(&mut state, &home, &planet, "arborec_mech", &a(), 1);
        assert_eq!(unlocked(&state), Some(true), "twelve, a mech counting");
        // Ground forces in a space area (on a ship) are not on planets.
        let mut state2 = game();
        crate::fixtures::put(&mut state2, &home, "arborec_infantry", &a(), 12);
        assert_eq!(unlocked(&state2), Some(false));
        assert_eq!(
            commander_unlocked(
                &state,
                content,
                DEFAULT,
                None,
                &a(),
                &LeaderId::new("solcommander")
            ),
            None,
            "not this module's commander"
        );
        // And the shared check delivers it.
        assert_eq!(
            crate::leaders::commander_unlocked(&state, content, DEFAULT, None, &a(), &leader),
            Some(true)
        );
    }

    // -- Stymie ----------------------------------------------------------------------------------

    /// A system on the map that is nobody's home.
    fn open_system() -> SystemId {
        let content = ContentStore::embedded();
        let id = ti4_content::galaxy::all_systems(content, DEFAULT)
            .keys()
            .find(|id| !ti4_content::galaxy::is_home_system(content, id, DEFAULT))
            .map(|id| (*id).to_owned())
            .expect("a non-home system");
        SystemId::new(id)
    }

    const STYMIE: &str = "promissory:hacan:stymie:SHIP_MOVED:after";

    fn stymie_game() -> GameState {
        let mut state =
            crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol"), ("c", "hacan")], DEFAULT);
        state
            .promissory_notes
            .insert(STYMIE_NOTE.to_owned(), PlayerId::new("c"));
        state
    }

    fn moved(state: &mut GameState, table: &mut Table, mover: &str, system: &SystemId) {
        state.system_mut(&open_system());
        emit_with(
            state,
            table,
            None,
            &[],
            "SHIP_MOVED",
            &[
                ("player", mover),
                ("system", system.as_str()),
                ("origin", "1"),
            ],
        );
    }

    #[test]
    fn stymie_places_the_movers_command_token_in_a_non_home_system_and_returns_home() {
        let content = ContentStore::embedded();
        let mut state = stymie_game();
        let c = PlayerId::new("c");
        let system = home_of(&state, &c);
        state.system_mut(&open_system());
        let targets = stymie_systems(&state, content, DEFAULT, None, &b());
        assert_eq!(targets, vec![open_system()]);
        assert!(
            targets
                .iter()
                .all(|target| !ti4_content::galaxy::is_home_system(
                    content,
                    target.as_str(),
                    DEFAULT
                )),
            "no home system is offered"
        );
        let target = targets[0].clone();
        let reinforcements = state.tokens_in_reinforcements(&b());
        let theirs = state.tokens_in_reinforcements(&c);
        moved(
            &mut state,
            &mut scripted(&[STYMIE, target.as_str()]),
            "b",
            &system,
        );
        assert!(state.system_state(&target).command_tokens.contains(&b()));
        assert_eq!(
            state.tokens_in_reinforcements(&b()),
            reinforcements - 1,
            "from the mover's reinforcements"
        );
        assert_eq!(
            state.tokens_in_reinforcements(&c),
            theirs,
            "not the holder's own"
        );
        assert_eq!(
            state.promissory_notes.get(STYMIE_NOTE),
            Some(&PlayerId::new("a")),
            "returned to the Arborec player"
        );
    }

    #[test]
    fn stymie_may_be_declined_and_stays_with_its_holder() {
        let mut state = stymie_game();
        let system = home_of(&state, &PlayerId::new("c"));
        let before = state.clone();
        moved(&mut state, &mut scripted(&["decline"]), "b", &system);
        assert_eq!(state, before);
    }

    #[test]
    fn stymie_needs_another_players_move_into_a_system_with_a_unit_of_the_holders() {
        let c = PlayerId::new("c");
        // The holder moving is not "another player".
        let mut state = stymie_game();
        let system = home_of(&state, &c);
        let before = state.clone();
        moved(&mut state, &mut silent(), "c", &system);
        assert_eq!(state, before, "the holder's own move");
        // A system with none of the holder's units.
        let mut state = stymie_game();
        let elsewhere = home_of(&state, &b());
        let before = state.clone();
        moved(&mut state, &mut silent(), "a", &elsewhere);
        assert_eq!(state, before, "none of the holder's units there");
        // The Arborec still holding their own note cannot play it.
        let mut state = stymie_game();
        state
            .promissory_notes
            .insert(STYMIE_NOTE.to_owned(), PlayerId::new("a"));
        let before = state.clone();
        let arborec_home = home_of(&state, &PlayerId::new("a"));
        moved(&mut state, &mut silent(), "b", &arborec_home);
        assert_eq!(state, before, "a note at home is not playable");
    }

    #[test]
    fn stymie_needs_a_command_token_in_the_movers_reinforcements() {
        let mut state = stymie_game();
        let system = home_of(&state, &PlayerId::new("c"));
        let seat = state.player_mut(&b()).unwrap();
        seat.tactic_tokens = 16;
        seat.fleet_tokens = 0;
        seat.strategic_tokens = 0;
        assert_eq!(state.tokens_in_reinforcements(&b()), 0);
        let before = state.clone();
        moved(&mut state, &mut silent(), "b", &system);
        assert_eq!(state, before);
    }

    // -- Letani Ospha ----------------------------------------------------------------------------

    fn agent_game() -> (GameState, SystemId) {
        let mut state = game();
        set_leader(&mut state, &a(), "arborecagent", LeaderStatus::Readied);
        state
            .board
            .values_mut()
            .for_each(|board| board.units.clear());
        let theirs = home_of(&state, &b());
        crate::fixtures::put(&mut state, &theirs, "cruiser", &b(), 1);
        (state, theirs)
    }

    #[test]
    fn the_agent_lets_the_ships_owner_trade_up_by_at_most_two() {
        let content = ContentStore::embedded();
        let (mut state, system) = agent_game();
        let targets = ship_targets(&state, content, DEFAULT);
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].replacements,
            ["carrier", "destroyer", "dreadnought"],
            "up to cost 4: no war sun (12), no flagship (8)"
        );
        let leader = LeaderId::new("arborecagent");
        assert_eq!(leader_action(&state, content, &a(), &leader), Some(true));
        assert!(
            crate::leaders::component_actions(&state, content, &a())
                .iter()
                .any(|option| option.id == "component|leader|arborecagent")
        );
        let done = with_context(&mut state, &mut scripted(&["replace|dreadnought"]), |ctx| {
            crate::leaders::use_leader(ctx, &a(), &leader)
        });
        assert!(done);
        assert_eq!(count(&state, &system, &b(), "cruiser"), 0);
        assert_eq!(count(&state, &system, &b(), "dreadnought"), 1);
        assert_eq!(
            crate::leaders::status(&state, &a(), &leader),
            Some(LeaderStatus::Exhausted),
            "the shared code exhausts the agent"
        );
    }

    #[test]
    fn the_ships_owner_may_decline_and_the_card_is_still_used() {
        let (mut state, system) = agent_game();
        let leader = LeaderId::new("arborecagent");
        let done = with_context(&mut state, &mut scripted(&["decline"]), |ctx| {
            use_leader(ctx, &a(), &leader)
        });
        assert_eq!(done, Some(true));
        assert_eq!(count(&state, &system, &b(), "cruiser"), 1, "unchanged");
    }

    #[test]
    fn the_agent_can_replace_your_own_ship_and_chooses_among_ships() {
        let (mut state, system) = agent_game();
        let home = home_of(&state, &a());
        crate::fixtures::put(&mut state, &home, "destroyer", &a(), 1);
        let leader = LeaderId::new("arborecagent");
        // Targets are listed in board order; ask for the Arborec's own destroyer.
        let content = ContentStore::embedded();
        let targets = ship_targets(&state, content, DEFAULT);
        let own = targets
            .iter()
            .position(|target| target.ship.owner == a())
            .unwrap();
        let answer = format!("ship|{own}");
        with_context(
            &mut state,
            &mut scripted(&[&answer, "replace|cruiser"]),
            |ctx| use_leader(ctx, &a(), &leader),
        );
        assert_eq!(count(&state, &home, &a(), "cruiser"), 1);
        assert_eq!(
            count(&state, &system, &b(), "cruiser"),
            1,
            "theirs untouched"
        );
    }

    #[test]
    fn the_agent_is_not_offered_for_fighters_or_a_box_that_cannot_supply() {
        let content = ContentStore::embedded();
        let leader = LeaderId::new("arborecagent");
        // Only a fighter on the board.
        let (mut state, system) = agent_game();
        state
            .board
            .values_mut()
            .for_each(|board| board.units.clear());
        crate::fixtures::put(&mut state, &system, "fighter", &b(), 3);
        assert_eq!(leader_action(&state, content, &a(), &leader), Some(false));
        assert!(
            !crate::leaders::component_actions(&state, content, &a())
                .iter()
                .any(|option| option.id.contains("arborecagent"))
        );
        // The box holds no dreadnought: it is no longer a replacement.
        let (mut state, system) = agent_game();
        crate::fixtures::put(&mut state, &system, "dreadnought", &b(), 5);
        let targets = ship_targets(&state, content, DEFAULT);
        let cruiser = targets
            .iter()
            .find(|target| target.ship.type_id.as_str() == "cruiser")
            .unwrap();
        assert!(
            !cruiser
                .replacements
                .iter()
                .any(|base| base == "dreadnought")
        );
        // Not this module's leader.
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("solagent")),
            None
        );
    }

    // -- Letani Miasmiala ------------------------------------------------------------------------

    #[test]
    fn the_hero_produces_in_systems_with_your_ground_forces_then_is_purged() {
        let content = ContentStore::embedded();
        let (mut state, home) = flagship_game();
        set_leader(&mut state, &a(), "arborechero", LeaderStatus::Unlocked);
        let leader = LeaderId::new("arborechero");
        assert_eq!(leader_action(&state, content, &a(), &leader), Some(true));
        let before = count(&state, &home, &a(), "cruiser");
        let (mut table, asked) = steered(&["build|cruiser|1"]);
        let done = with_context(&mut state, &mut table, |ctx| {
            crate::leaders::use_leader(ctx, &a(), &leader)
        });
        assert!(done);
        assert_eq!(count(&state, &home, &a(), "cruiser"), before + 1);
        assert!(
            asked
                .lock()
                .unwrap()
                .iter()
                .any(|prompt| prompt.contains("(1000 left)")),
            "any number, not the system's PRODUCTION"
        );
        assert_eq!(
            crate::leaders::status(&state, &a(), &leader),
            Some(LeaderStatus::Purged)
        );
    }

    #[test]
    fn the_hero_is_not_used_up_by_producing_nothing_or_offered_without_ground_forces() {
        let content = ContentStore::embedded();
        let (mut state, _) = flagship_game();
        set_leader(&mut state, &a(), "arborechero", LeaderStatus::Unlocked);
        let leader = LeaderId::new("arborechero");
        let done = with_context(&mut state, &mut steered(&[]).0, |ctx| {
            use_leader(ctx, &a(), &leader)
        });
        assert_eq!(done, Some(false), "stopped at once");
        assert_eq!(
            crate::leaders::status(&state, &a(), &leader),
            Some(LeaderStatus::Unlocked)
        );
        // No ground force anywhere.
        state.board.values_mut().for_each(|board| {
            board.units.retain(|unit| unit.owner != a());
            board
                .planet_units
                .values_mut()
                .for_each(|units| units.retain(|unit| unit.owner != a()));
        });
        assert_eq!(leader_action(&state, content, &a(), &leader), Some(false));
    }

    // -- Psychospore -----------------------------------------------------------------------------

    fn psychospore_game() -> (GameState, SystemId) {
        let mut state = game();
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("arborecbt"));
        let home = home_of(&state, &a());
        state.system_mut(&home).place_token(a());
        (state, home)
    }

    fn psychospore_offered(state: &GameState) -> bool {
        component_actions(state, ContentStore::embedded(), &a())
            .iter()
            .any(|option| option.id == PSYCHOSPORE_ACTION)
    }

    #[test]
    fn psychospore_swaps_a_command_token_for_an_infantry_and_exhausts() {
        let (mut state, home) = psychospore_game();
        assert!(psychospore_offered(&state));
        let option = component_actions(&state, ContentStore::embedded(), &a())
            .pop()
            .unwrap();
        let reinforcements = state.tokens_in_reinforcements(&a());
        let infantry = infantry_total(&state, &a());
        let done = with_context(&mut state, &mut silent(), |ctx| {
            crate::factions::perform_component(ctx, &a(), &option)
        });
        assert!(done);
        assert!(!state.system_state(&home).command_tokens.contains(&a()));
        assert_eq!(state.tokens_in_reinforcements(&a()), reinforcements + 1);
        assert_eq!(infantry_total(&state, &a()), infantry + 1);
        assert!(!psychospore_offered(&state), "exhausted");
        // It readies at the end of the status phase.
        emit(&mut state, &mut silent(), "STATUS_PHASE_ENDED", &[]);
        state.system_mut(&home).place_token(a());
        assert!(psychospore_offered(&state), "ready again");
    }

    #[test]
    fn psychospore_needs_the_card_a_token_and_infantry_in_the_same_system() {
        let (mut state, home) = psychospore_game();
        state.player_mut(&a()).unwrap().breakthrough = None;
        assert!(!psychospore_offered(&state), "no breakthrough");
        let (mut state2, home2) = psychospore_game();
        state2.system_mut(&home2).command_tokens.clear();
        assert!(!psychospore_offered(&state2), "no token");
        let elsewhere = home_of(&state2, &b());
        state2.system_mut(&elsewhere).place_token(a());
        assert!(
            !psychospore_offered(&state2),
            "token where there is no infantry"
        );
        // The token's system has infantry, but no planet to put the new one on.
        let (mut state3, home3) = psychospore_game();
        state3.system_mut(&home3).planet_control.clear();
        assert!(!psychospore_offered(&state3), "no controlled planet");
        let option = ChoiceOption::labelled(PSYCHOSPORE_ACTION, "component", "x");
        let done = with_context(&mut state, &mut silent(), |ctx| {
            perform_component(ctx, &a(), &option)
        });
        assert!(!done, "refused without the card");
        assert!(state.system_state(&home).command_tokens.contains(&a()));
        let _ = home3;
    }

    #[test]
    fn a_staged_destruction_from_a_card_or_agenda_reaches_letani_warrior_ii() {
        // Plague and the agenda clears stage `GROUND_FORCE_DESTROYED`; the flush announces it.
        let flush = |state: &mut GameState, faces: &[u32]| {
            let mut resolver = crate::fixtures::armed_resolver(state);
            let mut dice = crate::dice::Dice::from_faces(faces.iter().copied());
            let mut rng = crate::rng::GameRng::new(0);
            let mut sequence = crate::event::EventSequence::new();
            let mut table = silent();
            let mut ctx = Resolving {
                content: ContentStore::embedded(),
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
            crate::factions::hooks_ground::announce_staged_events(state, &mut ctx);
            dice.history().len()
        };
        let stage = |state: &mut GameState, unit: &str| {
            let home = home_of(state, &a());
            let planet = planets_in_home(state, &a())[0].clone();
            crate::factions::hooks_ground::stage_ground_force_destroyed(
                state,
                &home,
                &planet,
                &Unit::new(UnitTypeId::new(unit), a()),
                "action_card:plague",
            );
        };
        let mut state = game();
        learn(&mut state, &a(), "lw2");
        stage(&mut state, "arborec_infantry2");
        assert_eq!(flush(&mut state, &[6]), 1);
        assert_eq!(on_card(&state, &a()), 1);
        assert!(!crate::factions::hooks_ground::has_staged_events(&state));
        stage(&mut state, "arborec_infantry2");
        flush(&mut state, &[2]);
        assert_eq!(on_card(&state, &a()), 1, "a 2 does not return it");
        stage(&mut state, "arborec_infantry");
        assert_eq!(flush(&mut state, &[9]), 0, "Letani Warrior I does not roll");
    }

    #[test]
    fn the_heros_production_is_announced_as_units_produced() {
        let (mut state, _) = flagship_game();
        set_leader(&mut state, &a(), "arborechero", LeaderStatus::Unlocked);
        let leader = LeaderId::new("arborechero");
        let (mut table, _) = steered(&["build|cruiser|1"]);
        with_context(&mut state, &mut table, |ctx| {
            crate::leaders::use_leader(ctx, &a(), &leader)
        });
        assert!(
            state
                .faction_marks
                .values()
                .any(|row| row.contains("UNITS_PRODUCED")),
            "staged for the game to flush after the leader action"
        );
    }

    #[test]
    fn a_nekro_flagship_with_the_arborec_z_token_produces_up_to_five_units() {
        let mut state =
            crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "arborec")], &["arborec"]);
        let home = home_of(&state, &a());
        crate::fixtures::put(&mut state, &home, "nekro_flagship", &a(), 1);
        give_goods(&mut state, &a(), 20);
        let before = count(&state, &home, &a(), "cruiser");
        let (mut table, asked) = steered(&[
            "unit:nekro:arborec_flagship:SYSTEM_ACTIVATED:after",
            "build|cruiser|1",
        ]);
        emit(
            &mut state,
            &mut table,
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(count(&state, &home, &a(), "cruiser"), before + 1);
        assert!(
            asked
                .lock()
                .unwrap()
                .iter()
                .any(|prompt| prompt.contains("(5 left)"))
        );
        // Without the token the Nekro flagship offers nothing.
        let mut bare = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "arborec")], &[]);
        crate::fixtures::put(&mut bare, &home, "nekro_flagship", &a(), 1);
        give_goods(&mut bare, &a(), 20);
        let before = bare.clone();
        emit(
            &mut bare,
            &mut silent(),
            "SYSTEM_ACTIVATED",
            &[("player", "a"), ("system", home.as_str())],
        );
        assert_eq!(bare, before);
    }
}
