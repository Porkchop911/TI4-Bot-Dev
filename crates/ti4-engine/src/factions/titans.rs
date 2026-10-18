//! The Titans of Ul (`titans`). See `factions/mod.rs` for the contract and
//! `plans/BASE_FACTIONS_PLAN_2026-10-02.md` for scope; the per-item record is
//! `plans/evidence/BF-titans.md`.
//!
//! Split for parallel work: this file holds the faction abilities, units and unit-upgrade
//! technologies; `titans_leaders.rs` holds the leaders, the Terraform note and the breakthrough.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Terragenesis: "After you explore a planet that does not have a sleeper token: You may place or
//!   move 1 sleeper token onto that planet."
//! * Awaken: "After you activate a system that contains 1 or more of your sleeper tokens: You may
//!   replace each of those tokens with 1 PDS from your reinforcements."
//! * Coalescence: "If your flagship or your AWAKEN faction ability places your units into the same
//!   space area or onto the same planet as another player's units, your units must participate in
//!   combat during 'Space Combat' or 'Ground Combat' steps."
//! * Ouranos (flagship): "DEPLOY: After you activate a system that contains 1 or more of your PDS,
//!   you may replace 1 of those PDS with this unit."
//! * Hecatoncheires (mech): "DEPLOY: When you would place a PDS on a planet, you may place 1 mech
//!   and 1 infantry on that planet instead."
//! * Hel-Titan I / II: "This unit is treated as both a structure and a ground force. It cannot be
//!   transported." (II: "You may use this unit's SPACE CANNON against ships that are adjacent to
//!   this unit's systems.")
//!
//! # Sleeper tokens
//!
//! The 5 sleeper tokens are public information kept in `GameState::faction_marks`, one row per
//! occupied planet: key `titans:sleeper:<planet>`, value `<owner>|<system>`. A planet carries at
//! most one. A game without a Titans seat never writes either namespace.
//!
//! Coalescence leaves a row `titans:coalescence:<planet>` = `<activation_seq>|<system>` when Awaken
//! puts a unit onto a planet that already holds another player's units; it is read only during
//! that activation (`GroundHooks::forced_combat_planets`) and stale rows are ignored.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use super::hooks_economy::EconomyHooks;
use super::hooks_ground::GroundHooks;
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::timing::{Ability, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "titans";

/// Sleeper tokens a Titans player owns.
pub const SLEEPER_TOKENS: usize = 5;

const SLEEPER: &str = "titans:sleeper:";
const COALESCENCE: &str = "titans:coalescence:";
const TERRAGENESIS: &str = "terragenesis";
const AWAKEN: &str = "awaken";
const FLAGSHIP: &str = "titans_flagship";
const MECH: &str = "titans_mech";

/// What this faction implements; grows package by package.
///
/// Everything the Titans have is claimed, `titans_mech` (Hecatoncheires) included: its "when you
/// would place a PDS" alternative is offered by Awaken (here) and by Construction
/// (`EconomyHooks::pds_placement_alternative`). Placement sites without a window are listed in
/// `plans/evidence/BF-titans.md`.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[TERRAGENESIS, AWAKEN, "coalescence"],
    technologies: &["se2", "ht2"],
    units: &[
        "titans_pds",
        "titans_pds2",
        "titans_cruiser",
        "titans_cruiser2",
        FLAGSHIP,
        "titans_mech",
    ],
    promissory: super::titans_leaders::PROMISSORY,
    leaders: super::titans_leaders::LEADERS,
    breakthroughs: super::titans_leaders::BREAKTHROUGHS,
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        ground: GroundHooks {
            forced_combat_planets: Some(forced_combat_planets),
            ..GroundHooks::NONE
        },
        economy: EconomyHooks {
            pds_placement_alternative: Some(hecatoncheires_offer),
            pds_placement_alternative_performed: Some(hecatoncheires_perform),
            ..super::titans_leaders::HOOKS.economy
        },
        ..super::titans_leaders::HOOKS
    },
};

/// Hecatoncheires' alternative id at a "place a PDS" site.
const HECATONCHEIRES_PDS: &str = "titans|hecatoncheires";

/// Hecatoncheires (`titans_mech`), DEPLOY: "When you would place a PDS on a planet, you may place 1
/// mech and 1 infantry on that planet instead." Offered at every shared PDS placement site through
/// `EconomyHooks::pds_placement_alternative` (Construction), only while both models fit in the
/// reinforcements (`supply::allowed`, which also honours effect-placement bans). The Awaken half
/// of the DEPLOY is in `awaken`.
fn hecatoncheires_offer(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    _system: &SystemId,
    planet: &PlanetId,
) -> Vec<ChoiceOption> {
    if !is_titans(state, player) {
        return Vec::new();
    }
    let fits = |base: &str| {
        crate::action_cards::placed_unit_id(state, content, sources, player, base).is_some_and(
            |unit| crate::supply::allowed(state, content, sources, player, &unit, 1) == 1,
        )
    };
    if !fits("mech") || !fits("infantry") {
        return Vec::new();
    }
    vec![ChoiceOption::labelled(
        HECATONCHEIRES_PDS,
        "build",
        format!("place 1 mech and 1 infantry on {planet} instead"),
    )]
}

fn hecatoncheires_perform(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    option: &str,
) -> bool {
    if option != HECATONCHEIRES_PDS
        || hecatoncheires_offer(state, content, sources, player, system, planet).is_empty()
    {
        return false;
    }
    for base in ["mech", "infantry"] {
        let Some(unit) = crate::action_cards::placed_unit_id(state, content, sources, player, base)
        else {
            return false; // unreachable: the offer checked both
        };
        state
            .system_mut(system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(Unit::new(unit, player.clone()));
    }
    true
}

fn timing_abilities(state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        terragenesis(owner_name, seat),
        awaken(owner_name, seat),
        ouranos(owner_name, seat),
    ];
    abilities.extend(super::titans_leaders::timing_abilities(
        state, owner_name, seat,
    ));
    abilities
}

// -- sleeper tokens ------------------------------------------------------------------------------

/// Whether this seat is the Titans of Ul.
fn is_titans(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

fn sleeper_rows(state: &GameState) -> impl Iterator<Item = (PlanetId, PlayerId, SystemId)> + '_ {
    state
        .faction_marks
        .range(SLEEPER.to_owned()..)
        .take_while(|(key, _)| key.starts_with(SLEEPER))
        .filter_map(|(key, value)| {
            let planet = PlanetId::new(key.strip_prefix(SLEEPER)?);
            let (owner, system) = value.split_once('|')?;
            Some((planet, PlayerId::new(owner), SystemId::new(system)))
        })
}

/// The planets (and their systems) carrying one of `player`'s sleeper tokens, in planet-id order.
#[must_use]
pub(crate) fn sleeper_planets(state: &GameState, player: &PlayerId) -> Vec<(PlanetId, SystemId)> {
    sleeper_rows(state)
        .filter(|(_, owner, _)| owner == player)
        .map(|(planet, _, system)| (planet, system))
        .collect()
}

/// Whether any sleeper token lies on this planet.
#[must_use]
pub(crate) fn has_sleeper(state: &GameState, planet: &PlanetId) -> bool {
    state
        .faction_marks
        .contains_key(&format!("{SLEEPER}{planet}"))
}

/// Put one of `player`'s sleeper tokens on a planet that has none. `false` and untouched when the
/// planet already has one or all five are on the board.
pub(crate) fn place_sleeper(
    state: &mut GameState,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    if has_sleeper(state, planet) || sleeper_planets(state, player).len() >= SLEEPER_TOKENS {
        return false;
    }
    state
        .faction_marks
        .insert(format!("{SLEEPER}{planet}"), format!("{player}|{system}"));
    true
}

/// Return the sleeper token on a planet to its owner's supply; `true` if there was one.
pub(crate) fn remove_sleeper(state: &mut GameState, planet: &PlanetId) -> bool {
    state
        .faction_marks
        .remove(&format!("{SLEEPER}{planet}"))
        .is_some()
}

/// The system a planet sits in: the catalogue's answer, else the board's.
fn system_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
) -> Option<SystemId> {
    ti4_content::galaxy::planet(content, planet.as_str(), sources)
        .and_then(|found| found.system_id().map(SystemId::new))
        .or_else(|| {
            state
                .board
                .iter()
                .find(|(_, board)| {
                    board.planet_units.contains_key(planet)
                        || board.planet_control.contains_key(planet)
                })
                .map(|(id, _)| id.clone())
        })
}

// -- shared readers ------------------------------------------------------------------------------

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
    card: &str,
    subtype: &str,
    prompt: &str,
    options: Vec<ChoiceOption>,
) -> Result<String, TimingError> {
    let choice = Choice::new(player.clone(), prompt.to_owned(), options).contextualized(decision(
        context.state,
        player,
        card,
        subtype,
    ));
    context
        .ask_seeing(&choice)
        .map(|answer| answer.id)
        .map_err(TimingError::IllegalChoice)
}

fn event_player(event: &crate::event::Event) -> Option<PlayerId> {
    event.text("player").map(PlayerId::new)
}

/// The unit id this player places for a base type: an unlocked upgrade, else the faction's own
/// unit, else the generic one, as `action_cards::place_units_counted` resolves it.
fn placed_id(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
) -> Option<UnitTypeId> {
    let seat = state.player(player)?;
    let faction = seat.faction.to_string();
    let held: Vec<String> = seat
        .technologies
        .iter()
        .map(|tech| tech.as_str().to_owned())
        .collect();
    ti4_content::units::unlocked_upgrade(content, sources, base_type, &faction, &held)
        .or_else(|| ti4_content::units::faction_unit(content, &faction, base_type, sources))
        .or_else(|| {
            ti4_content::units::catalogue(content, sources)
                .get(base_type)
                .copied()
        })
        .map(|kind| {
            crate::factions::hooks_strategy::unit_form_override(
                state,
                content,
                sources,
                player,
                base_type,
                kind.id(),
            )
            .unwrap_or_else(|| UnitTypeId::new(kind.id().to_owned()))
        })
}

/// How many more of this unit's plastic the player may place, net of `planned` placements already
/// decided but not yet made. Zero when a game effect bars the unit (Naaz Maximum).
fn room(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    base_type: &str,
    planned: i64,
) -> bool {
    let Some(id) = placed_id(state, content, sources, player, base_type) else {
        return false;
    };
    crate::supply::allowed(state, content, sources, player, &id, 1) == 1
        && crate::supply::remaining(state, content, sources, player, &id) - planned >= 1
}

/// Whether the box still holds a PDS or a mech for the player: Slumberstate's sleeper allowance is
/// pointless when no sleeper token could ever be awakened.
#[must_use]
pub(crate) fn awakening_plastic_available(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> bool {
    room(state, content, sources, player, "pds", 0)
        || room(state, content, sources, player, "mech", 0)
}

/// The planets of `system` holding one of `player`'s units of this base type.
fn planets_holding(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    base_type: &str,
) -> Vec<PlanetId> {
    let types = ti4_content::units::catalogue(content, sources);
    let board = state.system_state(system);
    board
        .planet_units
        .iter()
        .filter(|(_, units)| {
            units.iter().any(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == base_type)
            })
        })
        .map(|(planet, _)| planet.clone())
        .collect()
}

// -- Terragenesis --------------------------------------------------------------------------------

/// What Terragenesis can do for `planet`: place a token from the supply and/or move one from
/// another planet. Empty when the planet already has a token or the player has none to spare.
fn terragenesis_options(
    state: &GameState,
    player: &PlayerId,
    planet: &PlanetId,
) -> Vec<ChoiceOption> {
    if has_sleeper(state, planet) {
        return Vec::new();
    }
    let on_board = sleeper_planets(state, player);
    let mut options = Vec::new();
    if on_board.len() < SLEEPER_TOKENS {
        options.push(ChoiceOption::labelled(
            "place".to_owned(),
            "sleeper",
            format!("place a sleeper token on {planet}"),
        ));
    }
    for (from, _) in on_board {
        options.push(ChoiceOption::labelled(
            format!("move|{from}"),
            "sleeper",
            format!("move the sleeper token on {from} to {planet}"),
        ));
    }
    options
}

/// Terragenesis: "After you explore a planet that does not have a sleeper token: You may place or
/// move 1 sleeper token onto that planet."
fn terragenesis(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{TERRAGENESIS}:PLANET_EXPLORED:after"),
        seat.clone(),
        "PLANET_EXPLORED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(planet) = event.text("planet").map(PlanetId::new) else {
                return Ok(());
            };
            if event_player(event).as_ref() != Some(&owner) || !is_titans(context.state, &owner) {
                return Ok(());
            }
            let Some(system) = system_of(context.state, context.content, context.sources, &planet)
            else {
                return Ok(());
            };
            let options = terragenesis_options(context.state, &owner, &planet);
            // Every question before the first mutation.
            let answer = match options.as_slice() {
                [] => return Ok(()),
                [only] => only.id.clone(),
                _ => ask(
                    context,
                    &owner,
                    TERRAGENESIS,
                    "terragenesis_token",
                    &format!("Terragenesis: sleeper token for {planet}"),
                    options.clone(),
                )?,
            };
            if !options.iter().any(|option| option.id == answer) {
                return Ok(());
            }
            if let Some(from) = answer.strip_prefix("move|") {
                remove_sleeper(context.state, &PlanetId::new(from));
            }
            place_sleeper(context.state, &owner, &system, &planet);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && is_titans(context.state, &condition_owner)
            && event.text("planet").is_some_and(|planet| {
                let planet = PlanetId::new(planet);
                system_of(context.state, context.content, context.sources, &planet).is_some()
                    && !terragenesis_options(context.state, &condition_owner, &planet).is_empty()
            })
    }))
}

// -- Awaken --------------------------------------------------------------------------------------

/// One thing a sleeper token may become.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Awakening {
    Pds,
    /// Hecatoncheires: 1 mech and 1 infantry in place of the PDS.
    MechAndInfantry,
}

impl Awakening {
    fn id(self) -> &'static str {
        match self {
            Self::Pds => "pds",
            Self::MechAndInfantry => "mech",
        }
    }
}

/// Whether Hecatoncheires may stand in for a PDS: the player's mech is a Hecatoncheires and the
/// box holds a mech and an infantry beyond what is already planned.
fn hecatoncheires_possible(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    planned_mechs: i64,
) -> bool {
    placed_id(state, content, sources, player, "mech").is_some_and(|id| id.as_str() == MECH)
        && room(state, content, sources, player, "mech", planned_mechs)
        && room(state, content, sources, player, "infantry", 0)
}

/// What each sleeper token in `system` may become right now, with the PDS and mech plastic that
/// earlier tokens of the same activation have already claimed. Tokens that can become nothing are
/// left out.
fn awakenings(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    claimed: &[(PlanetId, Awakening)],
) -> Vec<(PlanetId, Vec<Awakening>)> {
    let count = |wanted: Awakening| {
        i64::try_from(claimed.iter().filter(|(_, kind)| *kind == wanted).count()).unwrap_or(0)
    };
    let (planned_pds, planned_mechs) = (count(Awakening::Pds), count(Awakening::MechAndInfantry));
    sleeper_planets(state, player)
        .into_iter()
        .filter(|(_, at)| at == system)
        .filter(|(planet, _)| {
            !crate::laws::planet_is_demilitarized(state, planet)
                && !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources)
        })
        .filter_map(|(planet, _)| {
            let mut kinds = Vec::new();
            let held =
                crate::production::structures_on(state, content, sources, player, &planet, "pds");
            let cap_ok = crate::laws::structure_cap_lifted(state, "pds")
                || crate::production::structure_limit("pds").is_none_or(|cap| held < cap);
            if cap_ok && room(state, content, sources, player, "pds", planned_pds) {
                kinds.push(Awakening::Pds);
            }
            if hecatoncheires_possible(state, content, sources, player, planned_mechs) {
                kinds.push(Awakening::MechAndInfantry);
            }
            (!kinds.is_empty()).then_some((planet, kinds))
        })
        .collect()
}

/// Record that Awaken put this player's units onto a planet that holds another player's units
/// (Coalescence), for this activation only.
fn note_coalescence(
    state: &mut GameState,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) {
    let shared = state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .any(|unit| &unit.owner != player);
    if shared {
        state.faction_marks.insert(
            format!("{COALESCENCE}{planet}"),
            format!("{}|{system}", state.activation_seq),
        );
    }
}

/// Awaken: "After you activate a system that contains 1 or more of your sleeper tokens: You may
/// replace each of those tokens with 1 PDS from your reinforcements."
///
/// With Hecatoncheires the PDS may instead be 1 mech and 1 infantry on that planet ("when you would
/// place a PDS"). Several tokens are decided one at a time; a token may also be left where it is.
#[allow(
    clippy::too_many_lines,
    reason = "one window: every question is asked before the first placement"
)]
fn awaken(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("ability:{owner_name}:{AWAKEN}:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if event_player(event).as_ref() != Some(&owner) || !is_titans(context.state, &owner) {
                return Ok(());
            }
            let decision_log = context.table.log.clone();
            let result = (|| {
                // Every question first; nothing is placed until all are answered.
                let mut chosen: Vec<(PlanetId, Awakening)> = Vec::new();
                let first = awakenings(
                    context.state,
                    context.content,
                    context.sources,
                    &owner,
                    &system,
                    &chosen,
                );
                let planets: Vec<PlanetId> = first.iter().map(|(planet, _)| planet.clone()).collect();
                let lone = planets.len() == 1;
                for planet in planets {
                    let now = awakenings(
                        context.state,
                        context.content,
                        context.sources,
                        &owner,
                        &system,
                        &chosen,
                    );
                    let Some((_, kinds)) = now.into_iter().find(|(p, _)| *p == planet) else {
                        continue;
                    };
                    let kind = if lone && kinds.len() == 1 {
                        kinds[0]
                    } else {
                        let mut options: Vec<ChoiceOption> = kinds
                            .iter()
                            .map(|kind| match kind {
                                Awakening::Pds => ChoiceOption::labelled(
                                    kind.id().to_owned(),
                                    "awaken",
                                    format!("replace the sleeper token on {planet} with 1 PDS"),
                                ),
                                Awakening::MechAndInfantry => ChoiceOption::labelled(
                                    kind.id().to_owned(),
                                    "awaken",
                                    format!(
                                        "replace the sleeper token on {planet} with 1 mech and 1 infantry (Hecatoncheires)"
                                    ),
                                ),
                            })
                            .collect();
                        options.push(ChoiceOption::decline());
                        let answer = ask(
                            context,
                            &owner,
                            AWAKEN,
                            "awaken_replace",
                            &format!("Awaken: the sleeper token on {planet}"),
                            options,
                        )?;
                        let Some(kind) = kinds.iter().find(|kind| kind.id() == answer) else {
                            continue;
                        };
                        *kind
                    };
                    chosen.push((planet, kind));
                }
                for (planet, kind) in &chosen {
                    remove_sleeper(context.state, planet);
                    match kind {
                        Awakening::Pds => {
                            crate::action_cards::place_units_counted(
                                context,
                                &owner,
                                &system,
                                Some(planet),
                                "pds",
                                1,
                            );
                        }
                        Awakening::MechAndInfantry => {
                            crate::action_cards::place_units_counted(
                                context,
                                &owner,
                                &system,
                                Some(planet),
                                "mech",
                                1,
                            );
                            crate::action_cards::place_units_counted(
                                context,
                                &owner,
                                &system,
                                Some(planet),
                                "infantry",
                                1,
                            );
                        }
                    }
                    note_coalescence(context.state, &owner, &system, planet);
                }
                Ok(())
            })();
            if result.is_err() {
                context.table.log = decision_log;
            }
            result
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && is_titans(context.state, &condition_owner)
            && event
                .text("system")
                .map(SystemId::new)
                .is_some_and(|system| {
                    !awakenings(
                        context.state,
                        context.content,
                        context.sources,
                        &condition_owner,
                        &system,
                        &[],
                    )
                    .is_empty()
                })
    }))
}

// -- Coalescence ---------------------------------------------------------------------------------

/// Coalescence (ground half): the planets of the active system where Awaken put the invader's units
/// beside another player's units this activation, and the invader still has units. The invasion
/// makes them fight there although nothing was committed. (The space half needs nothing: every ship
/// the active player has in the active system takes part in its space combat, Ouranos included.)
fn forced_combat_planets(
    state: &GameState,
    content: &ContentStore,
    _sources: SourceSet,
    invader: &PlayerId,
    system: &SystemId,
) -> Vec<PlanetId> {
    let _ = content;
    if !is_titans(state, invader) {
        return Vec::new();
    }
    let now = state.activation_seq.to_string();
    state
        .faction_marks
        .range(COALESCENCE.to_owned()..)
        .take_while(|(key, _)| key.starts_with(COALESCENCE))
        .filter_map(|(key, value)| {
            let (seq, at) = value.split_once('|')?;
            (seq == now && at == system.as_str())
                .then(|| PlanetId::new(key.strip_prefix(COALESCENCE).unwrap_or_default()))
        })
        .filter(|planet| {
            !state
                .system_state(system)
                .on_planet_of(planet, invader)
                .is_empty()
        })
        .collect()
}

// -- Ouranos -------------------------------------------------------------------------------------

/// The planets of `system` where Ouranos could replace a PDS: the player's PDS stands there and the
/// flagship is in the reinforcements.
fn ouranos_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<PlanetId> {
    // The flagship's plastic is the PDS being replaced's to give back, not the box's: it must be
    // off the board, as the one flagship a player owns.
    let Some(id) = deploy_flagship(state, content, sources, player) else {
        return Vec::new();
    };
    if crate::supply::allowed(state, content, sources, player, &id, 1) == 0 {
        return Vec::new();
    }
    planets_holding(state, content, sources, player, system, "pds")
}

/// The unit Ouranos' DEPLOY places for `player`: the Titans flagship, or the Nekro flagship when
/// the Z token has switched the Titans flagship's text on ("this unit" is the Nekro flagship).
fn deploy_flagship(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Option<UnitTypeId> {
    if is_titans(state, player) {
        ti4_content::units::faction_unit(content, FACTION, "flagship", sources)
            .map(|kind| UnitTypeId::new(kind.id().to_owned()))
    } else {
        super::flagship_has_text(state, player, super::nekro::NEKRO_FLAGSHIP, FLAGSHIP)
            .then(|| UnitTypeId::new(super::nekro::NEKRO_FLAGSHIP))
    }
}

/// Ouranos DEPLOY: "After you activate a system that contains 1 or more of your PDS, you may
/// replace 1 of those PDS with this unit." The flagship arrives in the system's space area; a
/// damaged PDS is the one given up when the player has both.
fn ouranos(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:SYSTEM_ACTIVATED:after"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            if event_player(event).as_ref() != Some(&owner)
                || deploy_flagship(context.state, context.content, context.sources, &owner)
                    .is_none()
            {
                return Ok(());
            }
            let planets = ouranos_planets(
                context.state,
                context.content,
                context.sources,
                &owner,
                &system,
            );
            let planet = match planets.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let options = planets
                        .iter()
                        .map(|planet| {
                            ChoiceOption::labelled(
                                planet.to_string(),
                                "pds",
                                format!("replace the PDS on {planet} with Ouranos"),
                            )
                        })
                        .collect();
                    let answer = ask(
                        context,
                        &owner,
                        FLAGSHIP,
                        "ouranos_pds",
                        "Ouranos: which PDS to replace",
                        options,
                    )?;
                    let Some(planet) = planets.iter().find(|planet| planet.as_str() == answer)
                    else {
                        return Ok(());
                    };
                    planet.clone()
                }
            };
            replace_pds_with_flagship(context, &owner, &system, &planet);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event_player(event).as_ref() == Some(&condition_owner)
            && deploy_flagship(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_some()
            && event
                .text("system")
                .map(SystemId::new)
                .is_some_and(|system| {
                    !ouranos_planets(
                        context.state,
                        context.content,
                        context.sources,
                        &condition_owner,
                        &system,
                    )
                    .is_empty()
                })
    }))
}

/// Take one of `owner`'s PDS off `planet` (a damaged one first) and put the flagship in the space
/// area. Atomic: `false` and untouched when there is no PDS to give up.
fn replace_pds_with_flagship(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    let types = ti4_content::units::catalogue(context.content, context.sources);
    let Some(flagship) = deploy_flagship(context.state, context.content, context.sources, owner)
    else {
        return false;
    };
    let is_pds = |unit: &Unit| {
        &unit.owner == owner
            && types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "pds")
    };
    let Some(board) = context.state.board.get_mut(system) else {
        return false;
    };
    let Some(standing) = board.planet_units.get_mut(planet) else {
        return false;
    };
    let Some(index) = standing
        .iter()
        .position(|unit| is_pds(unit) && unit.sustained_damage)
        .or_else(|| standing.iter().position(is_pds))
    else {
        return false;
    };
    standing.remove(index);
    board.units.push(Unit::new(flagship, owner.clone()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use ti4_model::content_types::DEFAULT;

    const TERRA: &str = "ability:titans:terragenesis:PLANET_EXPLORED:after";
    const AWAKEN_ID: &str = "ability:titans:awaken:SYSTEM_ACTIVATED:after";
    const OURANOS_ID: &str = "unit:titans:titans_flagship:SYSTEM_ACTIVATED:after";

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", FACTION), ("b", "sol")], DEFAULT)
    }
    /// Declines whenever it may, else takes the first option: what a script falls back to once its
    /// listed answers run out, so a window that opens afterwards (Ouranos after Awaken) is declined.
    #[derive(Debug)]
    struct DeclineOrFirst;
    impl crate::choice::Decider for DeclineOrFirst {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            choice
                .options
                .iter()
                .find(|option| option.is_decline())
                .or_else(|| choice.options.first())
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }
    fn scripted(answers: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(crate::choice::Scripted::with_fallback(
            answers.iter().map(|s| (*s).to_owned()),
            Box::new(DeclineOrFirst),
        )))
    }

    /// Refuses every question: for "nothing is offered".
    #[derive(Debug)]
    struct Refuse;
    impl crate::choice::Decider for Refuse {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            Err(crate::choice::IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: format!("unexpected question: {}", choice.prompt),
            })
        }
    }
    fn silent() -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(Refuse))
    }

    /// Takes the first of `prefer` that is offered, else the first option.
    #[derive(Debug)]
    struct Steer {
        prefer: Vec<String>,
    }
    impl crate::choice::Decider for Steer {
        fn choose(
            &mut self,
            choice: &Choice,
        ) -> Result<ChoiceOption, crate::choice::IllegalChoice> {
            for want in &self.prefer {
                if let Some(option) = choice.option(want) {
                    return Ok(option.clone());
                }
            }
            choice
                .options
                .first()
                .cloned()
                .ok_or_else(|| crate::choice::IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }
    fn steer(prefer: &[&str]) -> crate::choice::Table {
        crate::choice::Table::with_default(Box::new(Steer {
            prefer: prefer.iter().map(|s| (*s).to_owned()).collect(),
        }))
    }

    fn emit(
        state: &mut GameState,
        table: &mut crate::choice::Table,
        event_type: &str,
        pairs: &[(&str, &str)],
    ) {
        let payload: BTreeMap<String, serde_json::Value> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), serde_json::Value::from(*v)))
            .collect();
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let event = ctx
                .event_sequence
                .next(event_type, payload)
                .expect("an event id");
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("the window resolves");
        });
    }
    fn activate(state: &mut GameState, table: &mut crate::choice::Table, who: &str, at: &SystemId) {
        emit(
            state,
            table,
            "SYSTEM_ACTIVATED",
            &[("player", who), ("system", at.as_str())],
        );
    }
    fn explored(state: &mut GameState, table: &mut crate::choice::Table, who: &str, at: &PlanetId) {
        emit(
            state,
            table,
            "PLANET_EXPLORED",
            &[("player", who), ("planet", at.as_str())],
        );
    }

    /// A corpus system with at least `n` planets that is nobody's home and not Mecatol Rex.
    fn system_with_planets(state: &GameState, n: usize) -> (SystemId, Vec<PlanetId>) {
        system_with_planets_in(state, n, DEFAULT)
    }
    fn system_with_planets_in(
        state: &GameState,
        n: usize,
        sources: SourceSet,
    ) -> (SystemId, Vec<PlanetId>) {
        find_system(state, n, sources, false)
    }
    fn find_system(
        state: &GameState,
        n: usize,
        sources: SourceSet,
        explorable: bool,
    ) -> (SystemId, Vec<PlanetId>) {
        let content = ContentStore::embedded();
        let homes: Vec<SystemId> = state
            .players
            .iter()
            .filter_map(|seat| seat.home_system.clone())
            .collect();
        ti4_content::galaxy::all_systems(content, sources)
            .iter()
            .filter(|(id, system)| {
                !system.is_anomaly()
                    && !system.is_hyperlane()
                    && **id != crate::seating::MECATOL
                    && !homes.iter().any(|home| home.as_str() == **id)
                    && system.planets().len() >= n
                    && system.planets().iter().all(|planet| {
                        ti4_content::galaxy::planet(content, planet, sources)
                            .is_some_and(|p| p.homeworld_of().is_none())
                            && (!explorable
                                || crate::exploration::trait_of(
                                    content,
                                    sources,
                                    &PlanetId::new(*planet),
                                )
                                .is_some())
                    })
            })
            .map(|(id, system)| {
                (
                    SystemId::new(*id),
                    system.planets().into_iter().map(PlanetId::new).collect(),
                )
            })
            .next()
            .expect("a multi-planet system")
    }
    fn count_on(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        owner: &PlayerId,
        kind: &str,
    ) -> usize {
        state
            .system_state(system)
            .on_planet_of(planet, owner)
            .iter()
            .filter(|unit| unit.type_id.as_str() == kind)
            .count()
    }
    fn count_space(state: &GameState, system: &SystemId, owner: &PlayerId, kind: &str) -> usize {
        state
            .system_state(system)
            .units_of(owner)
            .iter()
            .filter(|unit| unit.type_id.as_str() == kind)
            .count()
    }
    fn give_tech(state: &mut GameState, who: &PlayerId, alias: &str) {
        state
            .player_mut(who)
            .unwrap()
            .technologies
            .insert(ti4_model::id::TechnologyId::new(alias));
    }

    // -- neutrality ------------------------------------------------------------------------------

    #[test]
    fn a_game_without_the_titans_is_offered_nothing_and_writes_no_marks() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "yin")], DEFAULT);
        let (system, planets) = system_with_planets(&state, 1);
        // Even a stray token on the board and a PDS beside it do nothing for another faction.
        state
            .faction_marks
            .insert(format!("{SLEEPER}{}", planets[0]), format!("a|{system}"));
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "pds", &a(), 1);
        let before = state.clone();
        activate(&mut state, &mut silent(), "a", &system);
        explored(&mut state, &mut silent(), "a", &planets[0]);
        assert_eq!(state, before);
        let fresh = crate::fixtures::seated_game(&[("a", "sol"), ("b", "yin")], DEFAULT);
        assert!(
            fresh
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("titans:")),
            "no Titans namespace without Titans"
        );
    }

    // -- sleeper tokens --------------------------------------------------------------------------

    #[test]
    fn a_player_owns_five_sleeper_tokens_and_a_planet_holds_one() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        assert!(place_sleeper(&mut state, &a(), &system, &planets[0]));
        assert!(
            !place_sleeper(&mut state, &a(), &system, &planets[0]),
            "one per planet"
        );
        assert_eq!(
            sleeper_planets(&state, &a()),
            [(planets[0].clone(), system.clone())]
        );
        assert!(has_sleeper(&state, &planets[0]));
        assert!(remove_sleeper(&mut state, &planets[0]));
        assert!(!remove_sleeper(&mut state, &planets[0]));
        for n in 0..SLEEPER_TOKENS {
            assert!(place_sleeper(
                &mut state,
                &a(),
                &system,
                &PlanetId::new(format!("p{n}"))
            ));
        }
        assert!(
            !place_sleeper(&mut state, &a(), &system, &planets[0]),
            "the sixth does not exist"
        );
    }

    // -- Terragenesis ----------------------------------------------------------------------------

    #[test]
    fn terragenesis_places_a_sleeper_token_on_the_explored_planet() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        explored(&mut state, &mut scripted(&[TERRA]), "a", &planets[0]);
        assert_eq!(
            sleeper_planets(&state, &a()),
            [(planets[0].clone(), system)]
        );
    }

    #[test]
    fn terragenesis_may_be_declined() {
        let mut state = game();
        let (_, planets) = system_with_planets(&state, 1);
        let before = state.clone();
        explored(&mut state, &mut scripted(&["decline"]), "a", &planets[0]);
        assert_eq!(state, before);
    }

    #[test]
    fn terragenesis_asks_between_placing_and_moving_when_both_are_possible() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 2);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        explored(
            &mut state,
            &mut scripted(&[TERRA, &format!("move|{}", planets[0])]),
            "a",
            &planets[1],
        );
        assert_eq!(
            sleeper_planets(&state, &a()),
            [(planets[1].clone(), system.clone())],
            "moved, not added"
        );
        // Placing instead leaves both.
        let mut state = game();
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        explored(
            &mut state,
            &mut scripted(&[TERRA, "place"]),
            "a",
            &planets[1],
        );
        assert_eq!(sleeper_planets(&state, &a()).len(), 2);
    }

    #[test]
    fn with_all_five_tokens_out_terragenesis_can_only_move() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        let others: Vec<PlanetId> = (0..SLEEPER_TOKENS)
            .map(|n| PlanetId::new(format!("far{n}")))
            .collect();
        for planet in &others {
            place_sleeper(&mut state, &a(), &system, planet);
        }
        let options = terragenesis_options(&state, &a(), &planets[0]);
        assert_eq!(options.len(), SLEEPER_TOKENS);
        assert!(options.iter().all(|option| option.id.starts_with("move|")));
        explored(
            &mut state,
            &mut scripted(&[TERRA, "move|far3"]),
            "a",
            &planets[0],
        );
        assert!(has_sleeper(&state, &planets[0]));
        assert!(!has_sleeper(&state, &PlanetId::new("far3")));
        assert_eq!(sleeper_planets(&state, &a()).len(), SLEEPER_TOKENS);
    }

    #[test]
    fn terragenesis_is_not_offered_for_a_planet_with_a_token_or_to_another_explorer() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        let before = state.clone();
        explored(&mut state, &mut silent(), "a", &planets[0]);
        assert_eq!(state, before, "the planet already has a sleeper token");
        let mut state = game();
        let before = state.clone();
        explored(&mut state, &mut silent(), "b", &planets[0]);
        assert_eq!(state, before, "another player's exploration");
    }

    /// Terragenesis through the real exploration path: `exploration::explore_with` with the game's
    /// resolver opens the window.
    #[test]
    fn terragenesis_fires_from_a_real_exploration() {
        let mut state = game();
        let content = ContentStore::embedded();
        let planet = crate::fixtures::non_home_planets(200)
            .into_iter()
            .map(PlanetId::new)
            .find(|p| {
                !crate::exploration::traits_of(content, DEFAULT, p).is_empty()
                    && system_of(&state, content, DEFAULT, p).is_some()
            })
            .expect("an explorable planet");
        let deck = crate::exploration::traits_of(content, DEFAULT, &planet)[0].clone();
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut table = steer(&[TERRA, "place", "decline"]);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut sequence = crate::event::EventSequence::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut table,
            timing: Some(crate::choice::TimingHandle {
                resolver: &mut resolver,
                sequence: &mut sequence,
                galaxy: None,
            }),
        };
        let outcome =
            crate::exploration::explore_with(&mut state, &mut ctx, &a(), &deck, Some(&planet));
        assert!(outcome.is_some(), "a card was drawn");
        assert!(
            has_sleeper(&state, &planet),
            "the token followed the exploration"
        );
    }

    // -- Awaken ----------------------------------------------------------------------------------

    #[test]
    fn awaken_replaces_the_token_with_a_pds_from_the_reinforcements() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        activate(&mut state, &mut scripted(&[AWAKEN_ID, "pds"]), "a", &system);
        assert!(!has_sleeper(&state, &planets[0]), "the token went back");
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_pds"),
            1
        );
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_mech"),
            0
        );
    }

    #[test]
    fn awaken_places_the_upgraded_pds_once_hel_titan_ii_is_owned() {
        let mut state = game();
        give_tech(&mut state, &a(), "ht2");
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        activate(&mut state, &mut scripted(&[AWAKEN_ID, "pds"]), "a", &system);
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_pds2"),
            1
        );
    }

    #[test]
    fn awaken_may_be_declined_and_a_token_may_be_left_where_it_is() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        let before = state.clone();
        activate(&mut state, &mut scripted(&["decline"]), "a", &system);
        assert_eq!(state, before, "declined");
        activate(
            &mut state,
            &mut scripted(&[AWAKEN_ID, "decline"]),
            "a",
            &system,
        );
        assert_eq!(state, before, "the token stays");
    }

    #[test]
    fn awaken_decides_each_token_in_the_system() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 2);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        place_sleeper(&mut state, &a(), &system, &planets[1]);
        // Wake the first, leave the second (planet-id order).
        let mut order = planets.clone();
        order.sort();
        activate(
            &mut state,
            &mut scripted(&[AWAKEN_ID, "pds", "decline"]),
            "a",
            &system,
        );
        assert_eq!(count_on(&state, &system, &order[0], &a(), "titans_pds"), 1);
        assert!(!has_sleeper(&state, &order[0]));
        assert!(has_sleeper(&state, &order[1]), "left asleep");
        assert_eq!(count_on(&state, &system, &order[1], &a(), "titans_pds"), 0);
    }

    #[test]
    fn hecatoncheires_may_stand_in_for_the_awakened_pds() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        activate(
            &mut state,
            &mut scripted(&[AWAKEN_ID, "mech"]),
            "a",
            &system,
        );
        assert!(!has_sleeper(&state, &planets[0]));
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_mech"),
            1
        );
        assert_eq!(count_on(&state, &system, &planets[0], &a(), "infantry"), 1);
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_pds"),
            0
        );
    }

    #[test]
    fn the_mech_alternative_needs_a_mech_and_the_pds_needs_one_in_the_box() {
        // No mech left: only the PDS is offered (and with a single token it is not asked).
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        let far = SystemId::new("far");
        crate::fixtures::put(&mut state, &far, "titans_mech", &a(), 4);
        activate(&mut state, &mut scripted(&[AWAKEN_ID]), "a", &system);
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_pds"),
            1
        );
        // No PDS left, a mech free: only the mech alternative.
        let mut state = game();
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        crate::fixtures::put(&mut state, &far, "titans_pds", &a(), 6);
        activate(&mut state, &mut scripted(&[AWAKEN_ID]), "a", &system);
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_mech"),
            1
        );
        // Neither: nothing is offered and nothing changes.
        let mut state = game();
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        crate::fixtures::put(&mut state, &far, "titans_pds", &a(), 6);
        crate::fixtures::put(&mut state, &far, "titans_mech", &a(), 4);
        let before = state.clone();
        activate(&mut state, &mut silent(), "a", &system);
        assert_eq!(state, before);
    }

    #[test]
    fn a_planet_with_two_pds_takes_no_third() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "titans_pds", &a(), 2);
        // Only the mech alternative remains.
        activate(&mut state, &mut scripted(&[AWAKEN_ID]), "a", &system);
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_pds"),
            2
        );
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_mech"),
            1
        );
    }

    #[test]
    fn awaken_is_not_offered_elsewhere_or_to_another_activation() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        place_sleeper(&mut state, &a(), &system, &planets[0]);
        let before = state.clone();
        activate(&mut state, &mut silent(), "b", &system);
        assert_eq!(state, before, "another player's activation");
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        activate(&mut state, &mut silent(), "a", &home);
        assert_eq!(state, before, "a system without a token");
    }

    // -- Coalescence -----------------------------------------------------------------------------

    #[test]
    fn awaken_beside_another_players_units_marks_the_planet_for_combat() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 2);
        for planet in &planets[..2] {
            place_sleeper(&mut state, &a(), &system, planet);
        }
        let mut order = planets.clone();
        order.sort();
        crate::fixtures::put_on_planet(&mut state, &system, &order[0], "infantry", &b(), 1);
        activate(
            &mut state,
            &mut scripted(&[AWAKEN_ID, "pds", "pds"]),
            "a",
            &system,
        );
        let content = ContentStore::embedded();
        let forced = forced_combat_planets(&state, content, DEFAULT, &a(), &system);
        assert_eq!(
            forced,
            [order[0].clone()],
            "only the planet that shares ground"
        );
        assert!(
            forced_combat_planets(&state, content, DEFAULT, &b(), &system).is_empty(),
            "the rule is the Titans' own"
        );
        state.activation_seq += 1;
        assert!(
            forced_combat_planets(&state, content, DEFAULT, &a(), &system).is_empty(),
            "it lapses with the activation"
        );
    }

    // -- Ouranos ---------------------------------------------------------------------------------

    #[test]
    fn ouranos_replaces_a_pds_with_the_flagship_in_the_active_system() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "titans_pds", &a(), 2);
        activate(&mut state, &mut scripted(&[OURANOS_ID]), "a", &system);
        assert_eq!(
            count_space(&state, &system, &a(), FLAGSHIP),
            1,
            "in the space area"
        );
        assert_eq!(
            count_on(&state, &system, &planets[0], &a(), "titans_pds"),
            1,
            "exactly one PDS was replaced"
        );
    }

    #[test]
    fn ouranos_gives_up_a_damaged_pds_first_and_asks_which_planet() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 2);
        let mut order = planets.clone();
        order.sort();
        crate::fixtures::put_on_planet(&mut state, &system, &order[0], "titans_pds", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &order[1], "titans_pds", &a(), 2);
        let damaged = Unit::new(UnitTypeId::new("titans_pds"), a()).sustained();
        let standing = state
            .system_mut(&system)
            .planet_units
            .get_mut(&order[1])
            .unwrap();
        standing[1] = damaged;
        activate(
            &mut state,
            &mut scripted(&[OURANOS_ID, order[1].as_str()]),
            "a",
            &system,
        );
        assert_eq!(count_on(&state, &system, &order[0], &a(), "titans_pds"), 1);
        let board = state.system_state(&system);
        let left = board.on_planet_of(&order[1], &a());
        assert_eq!(left.len(), 1);
        assert!(
            !left[0].sustained_damage,
            "the damaged one was the one given up"
        );
    }

    #[test]
    fn ouranos_is_not_offered_without_a_pds_with_the_flagship_out_or_to_another_activation() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        let before = state.clone();
        activate(&mut state, &mut silent(), "a", &system);
        assert_eq!(state, before, "no PDS here");
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "titans_pds", &a(), 1);
        let before = state.clone();
        activate(&mut state, &mut silent(), "b", &system);
        assert_eq!(state, before, "another player's activation");
        crate::fixtures::put(&mut state, &SystemId::new("far"), FLAGSHIP, &a(), 1);
        let before = state.clone();
        activate(&mut state, &mut silent(), "a", &system);
        assert_eq!(state, before, "the flagship is already on the board");
        let mut state = game();
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "titans_pds", &a(), 1);
        let before = state.clone();
        activate(&mut state, &mut scripted(&["decline"]), "a", &system);
        assert_eq!(state, before, "declined");
    }

    // -- Hel-Titan II ----------------------------------------------------------------------------

    #[test]
    fn hel_titan_ii_prints_the_card_through_the_data() {
        let content = ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, DEFAULT);
        let pds = types["titans_pds2"];
        assert_eq!(pds.combat_hits_on(), Some(6), "Combat 6");
        assert!(pds.planetary_shield(), "PLANETARY SHIELD");
        assert_eq!(pds.space_cannon_hits_on(), Some(5), "SPACE CANNON 5");
        assert!(pds.sustain_damage(), "SUSTAIN DAMAGE");
        assert_eq!(pds.production(3), 1, "PRODUCTION 1, whatever the planet");
        assert!(pds.is_structure() && pds.is_ground_force());
        assert!(!pds.consumes_capacity(), "a structure takes no hold space");
        assert_eq!(pds.base_type(), "pds", "it counts against the six PDS");
        assert_eq!(pds.required_technology(), Some("ht2"));
    }

    #[test]
    fn hel_titan_ii_defends_a_planet_as_a_ground_force() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        let planet = &planets[0];
        crate::fixtures::put_on_planet(&mut state, &system, planet, "titans_pds2", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, planet, "infantry", &b(), 2);
        let content = ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = scripted(&[]);
        crate::invasion::ground_combat(
            &mut state,
            content,
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            planet,
            &b(),
        )
        .expect("the combat resolves");
        assert!(
            dice.history()
                .iter()
                .any(|roll| roll.by.as_deref() == Some("a") && roll.hits_on == Some(6)),
            "the Hel-Titan rolled as a ground force: {:?}",
            dice.history()
        );
    }

    #[test]
    fn hel_titan_ii_cannot_be_transported() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "titans_pds2", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "infantry", &a(), 1);
        let loadable =
            crate::transit::loadable(&state, ContentStore::embedded(), DEFAULT, &a(), &system);
        let ids: Vec<&str> = loadable.iter().map(|c| c.unit.type_id.as_str()).collect();
        assert_eq!(
            ids,
            ["infantry"],
            "the infantry may be lifted, the Hel-Titan may not"
        );
    }

    #[test]
    fn hel_titan_i_prints_the_card_and_fights_on_its_planet() {
        let content = ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, DEFAULT);
        let pds = types["titans_pds"];
        assert!(pds.is_structure() && pds.is_ground_force());
        assert!(!pds.consumes_capacity());
        assert_eq!(pds.base_type(), "pds");
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        let planet = &planets[0];
        crate::fixtures::put_on_planet(&mut state, &system, planet, "titans_pds", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, planet, "infantry", &b(), 2);
        let hits_on = pds.combat_hits_on().and_then(|v| u32::try_from(v).ok());
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = scripted(&[]);
        crate::invasion::ground_combat(
            &mut state,
            content,
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            planet,
            &b(),
        )
        .expect("the combat resolves");
        assert!(
            dice.history()
                .iter()
                .any(|roll| roll.by.as_deref() == Some("a") && roll.hits_on == hits_on),
            "Hel-Titan I rolled as a ground force: {:?}",
            dice.history()
        );
        // Not transportable.
        crate::fixtures::put_on_planet(&mut state, &system, planet, "infantry", &a(), 1);
        let loadable = crate::transit::loadable(&state, content, DEFAULT, &a(), &system);
        assert!(
            loadable
                .iter()
                .all(|c| c.unit.type_id.as_str() != "titans_pds"),
            "a Hel-Titan may not be lifted"
        );
    }

    #[test]
    fn hel_titan_ii_gives_its_planet_production_1() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        let content = ContentStore::embedded();
        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "titans_pds2", &a(), 1);
        state
            .system_mut(&system)
            .planet_control
            .insert(planets[0].clone(), a());
        assert_eq!(
            crate::production::capacity(&state, content, DEFAULT, &a(), &system),
            1
        );
        let types = ti4_content::units::catalogue(content, DEFAULT);
        let spots = crate::production::placements(
            &state,
            content,
            DEFAULT,
            &a(),
            &system,
            &types["infantry"],
        );
        assert_eq!(
            spots,
            [planets[0].to_string()],
            "ground forces may be produced on the Hel-Titan's planet"
        );
    }

    #[test]
    fn hel_titan_ii_fires_space_cannon_at_its_own_system_and_the_next() {
        let hub = crate::fixtures::plain_hub();
        let content = ContentStore::embedded();
        let centre = SystemId::new(&hub.centre);
        let beside = SystemId::new(&hub.outer[0]);
        let planet = PlanetId::new("somewhere");
        let fired_at_5 = |state: &GameState, active: &SystemId| {
            let mut state = state.clone();
            let mut dice = crate::dice::Dice::new();
            let mut rng = crate::rng::GameRng::new(0);
            crate::combat::space_cannon_offense(
                &mut state,
                content,
                DEFAULT,
                &mut dice,
                &mut rng,
                active,
                &b(),
                Some(&hub.galaxy),
            );
            dice.history()
                .iter()
                .filter(|roll| roll.by.as_deref() == Some("a") && roll.hits_on == Some(5))
                .count()
        };
        let mut state = game();
        // Not Antimass Deflectors' -1, which the target's faction may start with.
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .remove(&ti4_model::id::TechnologyId::new("amd"));
        crate::fixtures::put(&mut state, &centre, "cruiser", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &beside, &planet, "titans_pds2", &a(), 1);
        assert_eq!(
            fired_at_5(&state, &centre),
            1,
            "next door, as its card says"
        );
        crate::fixtures::put_on_planet(&mut state, &centre, &planet, "titans_pds2", &a(), 1);
        assert_eq!(fired_at_5(&state, &centre), 2, "and at home");
    }

    // -- Saturn Engine ---------------------------------------------------------------------------

    #[test]
    fn saturn_engine_prints_both_cards_through_the_data() {
        let content = ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, DEFAULT);
        let one = types["titans_cruiser"];
        assert_eq!(one.base_type(), "cruiser");
        assert_eq!(
            (one.move_value(), one.capacity(), one.combat_hits_on()),
            (2, 1, Some(7)),
            "Saturn Engine I: Cost 2, Combat 7, Move 2, Capacity 1"
        );
        assert!(!one.sustain_damage());
        let two = types["titans_cruiser2"];
        assert_eq!(
            (two.move_value(), two.capacity(), two.combat_hits_on()),
            (3, 2, Some(6)),
            "Saturn Engine II: Cost 2, Combat 6, Move 3, Capacity 2"
        );
        assert!(two.sustain_damage(), "SUSTAIN DAMAGE");
        assert_eq!(two.required_technology(), Some("se2"));
        assert!((one.cost() - 2.0).abs() < f64::EPSILON && (two.cost() - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn saturn_engine_ii_replaces_the_cruiser_on_research_build_and_move() {
        let content = ContentStore::embedded();
        let mut state = game();
        let has = |state: &GameState, kind: &str| {
            state
                .board
                .values()
                .flat_map(|board| board.units.iter())
                .filter(|unit| unit.owner == a() && unit.type_id.as_str() == kind)
                .count()
        };
        assert_eq!(has(&state, "titans_cruiser"), 2, "the starting fleet");
        assert!(
            crate::production::buildable_for(&state, content, DEFAULT, &a())
                .contains(&"titans_cruiser".to_owned()),
            "built as the faction's own cruiser"
        );
        // Prerequisites GYR: one green, one yellow and one red technology.
        let first_of = |kind: &str| -> String {
            content
                .from_sources(ti4_model::content_types::ContentType::Technologies, DEFAULT)
                .find(|record| {
                    record.strings("types") == [kind]
                        && record.text("faction").is_none()
                        && record.text("requirements").is_none_or(str::is_empty)
                })
                .and_then(|record| record.text("alias"))
                .expect("a tier-one technology")
                .to_owned()
        };
        for kind in ["BIOTIC", "CYBERNETIC", "WARFARE"] {
            let alias = first_of(kind);
            give_tech(&mut state, &a(), &alias);
        }
        assert!(crate::technology::research(
            &mut state,
            content,
            DEFAULT,
            &a(),
            &ti4_model::id::TechnologyId::new("se2")
        ));
        assert_eq!(
            has(&state, "titans_cruiser"),
            0,
            "the card went over every cruiser"
        );
        assert_eq!(has(&state, "titans_cruiser2"), 2);
        assert!(
            crate::production::buildable_for(&state, content, DEFAULT, &a())
                .contains(&"titans_cruiser2".to_owned())
        );
        let types = ti4_content::units::catalogue(content, DEFAULT);
        let home = state.player(&a()).unwrap().home_system.clone().unwrap();
        assert_eq!(
            crate::tactical::effective_move_value(&state, &types["titans_cruiser2"], &a(), &home),
            3
        );
        assert_eq!(
            crate::tactical::effective_move_value(&state, &types["titans_cruiser"], &a(), &home),
            2
        );
    }

    // -- real routes: the game's tactical action -------------------------------------------------

    /// A one-ring map of ordinary systems with a planet-bearing system in it, the Titans to move.
    fn tactical_setup() -> (GameState, crate::fixtures::Hub, SystemId, PlanetId) {
        tactical_setup_with(false)
    }
    fn tactical_setup_with(
        explorable: bool,
    ) -> (GameState, crate::fixtures::Hub, SystemId, PlanetId) {
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        let (system, planets) = find_system(&state, 1, ti4_model::content_types::POK, explorable);
        let hub = crate::fixtures::hub_with_outer(system.as_str());
        (state, hub, system, planets[0].clone())
    }

    /// Run the tactical action to its end, answering from `prefer`; returns the finished game.
    fn run_tactical(
        state: GameState,
        hub: crate::fixtures::Hub,
        prefer: &[&str],
    ) -> crate::game::Game<'static> {
        let table = steer(prefer);
        let mut game = crate::game::Game::with_table(state, ContentStore::embedded(), table)
            .with_sources(DEFAULT)
            .with_galaxy(hub.galaxy);
        for _ in 0..200 {
            let result = game.step();
            assert_eq!(result.error, None, "events: {:?}", game.events);
            if game
                .events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE")
            {
                break;
            }
        }
        game
    }

    fn rolled_by(game: &crate::game::Game<'_>, who: &str, hits_on: u32) -> usize {
        game.rolls()
            .iter()
            .filter(|roll| roll.by.as_deref() == Some(who) && roll.hits_on == Some(hits_on))
            .count()
    }

    #[test]
    fn awaken_in_a_real_tactical_action_forces_the_new_pds_into_the_ground_combat() {
        let (mut state, hub, system, planet) = tactical_setup();
        give_tech(&mut state, &a(), "ht2");
        place_sleeper(&mut state, &a(), &system, &planet);
        // A ship already there: the invasion needs the active player to hold the space.
        crate::fixtures::put(&mut state, &system, "titans_cruiser", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), b());
        let game = run_tactical(
            state,
            hub,
            &[
                crate::game::TACTICAL_ACTION_ID,
                system.as_str(),
                AWAKEN_ID,
                "pds",
                "done_moving",
                "fight",
                "decline",
            ],
        );
        assert!(
            game.events
                .iter()
                .any(|event| event == "TACTICAL_ACTION_COMPLETE"),
            "the action ran to its end: {:?}",
            game.events
        );
        assert!(!has_sleeper(&game.state, &planet), "the token was replaced");
        assert!(
            rolled_by(&game, "a", 6) > 0,
            "Hel-Titan II fought although nothing was committed: {:?}",
            game.rolls()
        );
        assert!(rolled_by(&game, "b", 8) > 0, "the defender fought it");
        // Whoever won, the planet is not left with both sides on it.
        let here = game.state.system_state(&system);
        let owners: std::collections::BTreeSet<&PlayerId> = here
            .on_planet(&planet)
            .iter()
            .filter(|unit| !unit.type_id.as_str().contains("pds") || unit.owner == a())
            .map(|unit| &unit.owner)
            .collect();
        assert!(owners.len() <= 1, "the combat ran to a result: {here:?}");
    }

    #[test]
    fn coalescence_forces_the_ground_combat_when_the_titans_have_no_ship_there() {
        let (mut state, hub, system, planet) = tactical_setup();
        give_tech(&mut state, &a(), "ht2");
        place_sleeper(&mut state, &a(), &system, &planet);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), b());
        let game = run_tactical(
            state,
            hub,
            &[
                crate::game::TACTICAL_ACTION_ID,
                system.as_str(),
                AWAKEN_ID,
                "pds",
                "done_moving",
                "fight",
                "decline",
            ],
        );
        assert!(
            rolled_by(&game, "a", 6) > 0,
            "Hel-Titan II fought with no ship in the system: {:?}",
            game.rolls()
        );
        assert!(rolled_by(&game, "b", 8) > 0, "the defender fought it");
    }

    /// Slumberstate Computing: with no other units committed, the Titans may coexist instead of
    /// fighting where Coalescence would make a ground combat.
    #[test]
    fn slumberstate_lets_coalescing_units_coexist_instead_of_fighting() {
        let run = |bt: bool, answer: &str| {
            let (mut state, hub, system, planet) = tactical_setup();
            give_tech(&mut state, &a(), "ht2");
            if bt {
                state.player_mut(&a()).unwrap().breakthrough =
                    Some(ti4_model::id::BreakthroughId::new("titansbt"));
            }
            place_sleeper(&mut state, &a(), &system, &planet);
            crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
            state
                .system_mut(&system)
                .planet_control
                .insert(planet.clone(), b());
            let game = run_tactical(
                state,
                hub,
                &[
                    crate::game::TACTICAL_ACTION_ID,
                    system.as_str(),
                    AWAKEN_ID,
                    "pds",
                    "done_moving",
                    answer,
                    "decline",
                ],
            );
            (game, system, planet)
        };
        let (game, system, planet) = run(true, "coexist");
        assert_eq!(rolled_by(&game, "a", 6), 0, "nobody fought");
        assert!(
            crate::coexistence::is_coexisting(&game.state, &system, &planet, &a()),
            "the Titans coexist"
        );
        assert_eq!(
            game.state.system_state(&system).planet_control.get(&planet),
            Some(&b()),
            "the controller keeps the planet"
        );
        assert_eq!(count_on(&game.state, &system, &planet, &b(), "infantry"), 2);
        let (game, ..) = run(true, "fight");
        assert!(rolled_by(&game, "a", 6) > 0, "declining the option fights");
        // Without the breakthrough the choice does not exist: the same answer script fights.
        let (game, system, planet) = run(false, "coexist");
        assert!(rolled_by(&game, "a", 6) > 0);
        assert!(!crate::coexistence::in_coexistence(
            &game.state,
            &system,
            &planet
        ));
    }

    /// "During the status phase, for each player you are coexisting with, you and that player each
    /// draw 1 additional action card."
    #[test]
    fn slumberstate_adds_a_status_draw_per_coexisting_partner() {
        let mut state = game();
        let content = ContentStore::embedded();
        let (system, planets) = system_with_planets(&state, 1);
        let planet = &planets[0];
        let bonus = |state: &GameState, who: &PlayerId| {
            let seat_hooks = crate::factions::titans_leaders::HOOKS.economy;
            (seat_hooks.action_card_draw_bonus.unwrap())(state, content, who, 1)
        };
        state.phase = ti4_model::state::Phase::Status;
        crate::fixtures::put_on_planet(&mut state, &system, planet, "infantry", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, planet, "infantry", &b(), 1);
        state
            .system_mut(&system)
            .planet_control
            .insert(planet.clone(), b());
        assert_eq!(
            (bonus(&state, &a()), bonus(&state, &b())),
            (0, 0),
            "not coexisting"
        );
        crate::coexistence::begin(&mut state, &system, planet, &a(), None).unwrap();
        assert_eq!(
            (bonus(&state, &a()), bonus(&state, &b())),
            (0, 0),
            "no breakthrough"
        );
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("titansbt"));
        assert_eq!((bonus(&state, &a()), bonus(&state, &b())), (1, 1));
        state.phase = ti4_model::state::Phase::Action;
        assert_eq!(
            (bonus(&state, &a()), bonus(&state, &b())),
            (0, 0),
            "status phase only"
        );
    }

    /// "Other players may allow you to place a sleeper token on a planet they control."
    #[test]
    fn slumberstate_lets_another_player_allow_a_sleeper_on_their_planet() {
        let run = |bt: bool, answers: &[&str]| {
            let mut state = game();
            let (system, planets) = system_with_planets(&state, 1);
            let planet = planets[0].clone();
            state
                .system_mut(&system)
                .planet_control
                .insert(planet.clone(), b());
            if bt {
                state.player_mut(&a()).unwrap().breakthrough =
                    Some(ti4_model::id::BreakthroughId::new("titansbt"));
            }
            let mut table = steer(answers);
            emit(&mut state, &mut table, "TURN_BEGAN", &[("player", "a")]);
            let _ = planet;
            sleeper_rows(&state).count() > 0
        };
        assert!(run(true, &["allow"]), "allowed");
        assert!(!run(false, &["allow"]), "no breakthrough, no offer");
    }

    /// The allowance is offered at most once per game round, whatever the answer, and never when
    /// the box holds neither a PDS nor a mech.
    #[test]
    fn the_sleeper_allowance_is_offered_once_per_round() {
        let mut state = game();
        let (system, planets) = system_with_planets(&state, 1);
        state
            .system_mut(&system)
            .planet_control
            .insert(planets[0].clone(), b());
        state.player_mut(&a()).unwrap().breakthrough =
            Some(ti4_model::id::BreakthroughId::new("titansbt"));
        let tokens = |state: &GameState| sleeper_rows(state).count();
        emit(
            &mut state,
            &mut steer(&["allow"]),
            "TURN_BEGAN",
            &[("player", "a")],
        );
        assert_eq!(tokens(&state), 1, "offered and allowed");
        for (placed, _) in sleeper_planets(&state, &a()) {
            remove_sleeper(&mut state, &placed);
        }
        emit(
            &mut state,
            &mut steer(&["allow"]),
            "TURN_BEGAN",
            &[("player", "a")],
        );
        assert_eq!(tokens(&state), 0, "not offered again in the same round");
        state.round += 1;
        emit(
            &mut state,
            &mut steer(&["allow"]),
            "TURN_BEGAN",
            &[("player", "a")],
        );
        assert_eq!(tokens(&state), 1, "offered again next round");
    }

    #[test]
    fn without_coalescence_a_planet_with_no_rival_forces_starts_no_combat() {
        // The same activation with nobody on the planet: Awaken places the PDS and nothing fights.
        let (mut state, hub, system, planet) = tactical_setup();
        give_tech(&mut state, &a(), "ht2");
        place_sleeper(&mut state, &a(), &system, &planet);
        let game = run_tactical(
            state,
            hub,
            &[
                crate::game::TACTICAL_ACTION_ID,
                system.as_str(),
                AWAKEN_ID,
                "pds",
                "done_moving",
                "decline",
            ],
        );
        assert_eq!(rolled_by(&game, "a", 6), 0);
        assert_eq!(
            count_on(&game.state, &system, &planet, &a(), "titans_pds2"),
            1
        );
    }

    #[test]
    fn ouranos_in_a_real_tactical_action_joins_the_space_combat() {
        let (mut state, hub, system, planet) = tactical_setup();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "titans_pds", &a(), 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b(), 1);
        let game = run_tactical(
            state,
            hub,
            &[
                crate::game::TACTICAL_ACTION_ID,
                system.as_str(),
                OURANOS_ID,
                "done_moving",
                "decline",
            ],
        );
        assert!(
            rolled_by(&game, "a", 7) >= 2
                || game.rolls().iter().any(|r| r.by.as_deref() == Some("a")),
            "the flagship's dice were rolled: {:?}",
            game.rolls()
        );
        assert_eq!(
            count_on(&game.state, &system, &planet, &a(), "titans_pds"),
            0,
            "the PDS was replaced"
        );
    }

    #[test]
    fn terragenesis_follows_a_scanlink_exploration_in_a_real_tactical_action() {
        // The Titans begin with Scanlink Drone Network; its exploration carries the game's timing
        // handle, so Terragenesis is offered after it as after a landing's.
        let (mut state, hub, system, planet) = tactical_setup_with(true);
        assert!(
            state
                .player(&a())
                .unwrap()
                .technologies
                .contains(&ti4_model::id::TechnologyId::new("sdn")),
            "the Titans start with Scanlink"
        );
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        let game = run_tactical(
            state,
            hub,
            &[
                crate::game::TACTICAL_ACTION_ID,
                system.as_str(),
                planet.as_str(),
                TERRA,
                "place",
                "done_moving",
                "decline",
            ],
        );
        assert!(
            game.events
                .iter()
                .any(|event| event.starts_with("SCANLINK_EXPLORED")),
            "the planet was explored: {:?}",
            game.events
        );
        assert!(
            has_sleeper(&game.state, &planet),
            "and Terragenesis placed its token"
        );
    }

    #[test]
    fn a_nekro_flagship_with_the_titans_z_token_deploys_in_place_of_a_pds() {
        let run = |lent: &[&str], answers: &[&str]| {
            let mut state = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "sol")], lent);
            let (system, planets) = system_with_planets(&state, 1);
            crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "pds", &a(), 2);
            activate(&mut state, &mut scripted(answers), "a", &system);
            (
                count_space(&state, &system, &a(), "nekro_flagship"),
                count_on(&state, &system, &planets[0], &a(), "pds"),
            )
        };
        assert_eq!(run(&[], &[]), (0, 2), "off by default");
        assert_eq!(
            run(
                &["titans"],
                &["unit:nekro:titans_flagship:SYSTEM_ACTIVATED:after"]
            ),
            (1, 1),
            "the Nekro flagship, not the Titans one, is deployed"
        );
    }
}
