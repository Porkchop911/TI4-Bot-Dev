//! Nekro flagship (The Alastor), mech (Mordred), agent, hero, commander unlock and the faction's
//! two further technologies. Everything else Nekro is in `nekro.rs`. Record:
//! `plans/evidence/BF-nekro-units.md`.
//!
//! Card text (content corpus, latest printing):
//!
//! * The Alastor, flagship: "At the start of a space combat, choose any number of your ground
//!   forces in this system to participate in that combat as if they were ships."
//! * Mordred, mech: "During combat against an opponent who has an \"X\" or \"Y\" token on 1 or more
//!   of their technologies, apply +2 to the result of each of this unit's combat rolls."
//! * Nekro Malleon, agent: "During the action phase: You may exhaust this card to choose a player:
//!   that player may discard 1 action card or spend 1 command token from their command sheet to
//!   gain 2 trade goods."
//! * Nekro Acidos, commander: "After you gain a technology: You may draw 1 action card." Unlock:
//!   "Own 3 technologies. A \"Valefar Assimilator\" technology counts only if its X or Y token is
//!   on a technology." (the draw is in `borrowed_commanders.rs`).
//! * UNIT.DSGN.FLAYESH, hero: "ACTION: Choose a planet that has a technology specialty in a system
//!   that contains your units. Destroy any other player's units on that planet. Gain trade goods
//!   equal to the planet's combined resource and influence values and gain 1 technology that
//!   matches the specialty of that planet. Then, purge this card."
//! * `nekroc4y`: "When one of your ships is destroyed, you may produce a ship of the same type at a
//!   space dock in your home system."
//! * `nekroc4r`: "ACTION: Exhaust this card to place 1 PDS on a planet you control. ACTION: Exhaust
//!   this card to repair all of your damaged units. ACTION: Exhaust this card and discard 1 action
//!   card to draw 1 action card."
//!
//! Routes:
//!
//! * The Alastor is a `SPACE_COMBAT_STARTED` window ([`alastor`]). The ground forces it chooses
//!   stay on their planets and are recorded as marks (`nekro:alastor|...`, a count per planet and
//!   unit type, keyed by the activation and the system). `combat::planet_combatants` then
//!   lets them roll, sustain, take hits and be destroyed through the same code that already carries
//!   the Naaz Eidolon Maximum. The marks end with the combat (`SPACE_COMBAT_ENDED` in
//!   `combat.rs`) or when their owner retreats; a participant that is destroyed shrinks its mark.
//! * Mordred is a roll bonus read by `combat::effective_from` and `invasion::ground_combat_value`
//!   ([`mech_roll_bonus`]).
//! * The agent, hero and commander unlock are exposed as [`leader_action`], [`use_leader`] and
//!   [`commander_unlocked`] for `Hooks` in `nekro.rs`; `nekroc4r` as [`component_actions`] and
//!   [`perform_component`]; `nekroc4y` is a `SHIP_DESTROYED` window ([`null_reference`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::{GameState, LeaderStatus, TokenPool};
use ti4_model::units::Unit;

use super::hooks_combat::CombatHooks;
use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving};
use crate::decision_context::{DecisionContext, DecisionSource};
use crate::event::Event;
use crate::production::Spend;
use crate::timing::{Ability, Relation, TimingContext, TimingError};

const FLAGSHIP: &str = "nekro_flagship";
const MECH: &str = "nekro_mech";
const AGENT: &str = "nekroagent";
const COMMANDER: &str = "nekrocommander";
const HERO: &str = "nekrohero";
const NULL_REFERENCE: &str = "nekroc4y";
const ERROR_ERROR: &str = "nekroc4r";
/// Roll bonus of Mordred.
const MECH_BONUS: i64 = 2;
/// The two Valefar Assimilators, whose cards count towards the commander's unlock only with a token.
const ASSIMILATORS: [&str; 2] = ["vax", "vay"];
/// The prefix of the Alastor's marks.
const ALASTOR: &str = "nekro:alastor";
/// Option-id prefix of `nekroc4r`'s component actions.
const C4R_PLACE: &str = "faction|nekro|c4r|pds";
const C4R_REPAIR: &str = "faction|nekro|c4r|repair";
const C4R_DRAW: &str = "faction|nekro|c4r|draw";

/// Leaders claimed by the Nekro module.
pub const LEADERS: &[&str] = &[AGENT, COMMANDER, HERO];
/// Units claimed here (flagship, mech).
pub const UNITS: &[&str] = &[FLAGSHIP, MECH];
/// Technologies claimed here: the two Valefar Assimilators (a Nekro begins with both) and the Thunder's Edge cards.
///
/// `nekroc4y` / `nekroc4r` are implemented below but unclaimed: operator ruling 2026-10-07, they
/// belong to an obscure variant and are not part of the game.
pub const TECHNOLOGIES: &[&str] = &["vax", "vay"];
/// Combat hooks. None: the Alastor works through `combat::planet_combatants` and Mordred through a
/// roll bonus read in `combat.rs` and `invasion.rs`.
pub const COMBAT_HOOKS: CombatHooks = CombatHooks::NONE;

/// Timing abilities of the units and technologies, for one seat.
///
/// Registered for every seat; each condition is false unless the seat owns the flagship in the
/// combat or the technology, so a game without Nekro is unchanged.
pub(crate) fn timing_abilities(
    _state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    vec![alastor(owner_name, seat), null_reference(owner_name, seat)]
}

// -- small helpers -------------------------------------------------------------------------------

fn decision(state: &GameState, who: &PlayerId, source: &str, subtype: &str) -> DecisionContext {
    DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(source.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

fn ask(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    source: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, IllegalChoice> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(decision(
        context.state,
        who,
        source,
        subtype,
    ));
    context.ask_seeing(&choice)
}

fn illegal(error: IllegalChoice) -> TimingError {
    TimingError::IllegalChoice(error)
}

fn owns_technology(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.technologies.contains(&TechnologyId::new(alias)))
}

fn technology_ready(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    let id = TechnologyId::new(alias);
    state.player(player).is_some_and(|seat| {
        seat.technologies.contains(&id) && !seat.exhausted_technologies.contains(&id)
    })
}

fn leader_status(state: &GameState, player: &PlayerId, leader: &str) -> Option<LeaderStatus> {
    state
        .player(player)
        .and_then(|seat| seat.leaders.get(&LeaderId::new(leader)).copied())
}

fn took_part(event: &Event, player: &PlayerId) -> bool {
    event.text("attacker") == Some(player.as_str())
        || event.text("defender") == Some(player.as_str())
}

/// Whether any of `player`'s units stand in the system, in space or on a planet.
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

// -- The Alastor: participation marks ------------------------------------------------------------

fn alastor_prefix(state: &GameState, system: &SystemId, player: &PlayerId) -> String {
    format!("{ALASTOR}|{}|{system}|{player}|", state.activation_seq)
}

fn alastor_key(
    state: &GameState,
    system: &SystemId,
    player: &PlayerId,
    planet: &PlanetId,
    unit: &str,
) -> String {
    format!("{}{planet}|{unit}", alastor_prefix(state, system, player))
}

/// What `player`'s Alastor chose in this activation's combat in `system`: a count per planet and
/// unit type, in key order. Empty without a choice, which is every game without a Nekro.
fn alastor_marks(
    state: &GameState,
    system: &SystemId,
    player: &PlayerId,
) -> Vec<(PlanetId, String, usize)> {
    if state.faction_marks.is_empty() {
        return Vec::new();
    }
    let prefix = alastor_prefix(state, system, player);
    state
        .faction_marks
        .range(prefix.clone()..)
        .map_while(|(key, count)| key.strip_prefix(&prefix).map(|rest| (rest, count)))
        .filter_map(|(rest, count)| {
            let (planet, unit) = rest.split_once('|')?;
            Some((
                PlanetId::new(planet),
                unit.to_owned(),
                count.parse::<usize>().ok()?,
            ))
        })
        .collect()
}

/// The ground forces of `player` standing in `system` that an Alastor chose to fight this combat,
/// as `(planet, index in that planet's unit list)`: the first units of the chosen type there.
#[must_use]
pub(crate) fn alastor_participants(
    state: &GameState,
    player: &PlayerId,
    system: &SystemId,
) -> BTreeSet<(PlanetId, usize)> {
    let mut found = BTreeSet::new();
    let marks = alastor_marks(state, system, player);
    if marks.is_empty() {
        return found;
    }
    let board = state.system_state(system);
    for (planet, unit, count) in marks {
        let Some(units) = board.planet_units.get(&planet) else {
            continue;
        };
        found.extend(
            units
                .iter()
                .enumerate()
                .filter(|(_, held)| &held.owner == player && held.type_id.as_str() == unit)
                .map(|(index, _)| (planet.clone(), index))
                .take(count),
        );
    }
    found
}

/// Whether `owner` has chosen ground forces of `unit` for this combat in `system`.
#[must_use]
pub(crate) fn alastor_covers(
    state: &GameState,
    system: &SystemId,
    owner: &PlayerId,
    unit: &str,
) -> bool {
    alastor_marks(state, system, owner)
        .iter()
        .any(|(_, kind, count)| kind == unit && *count > 0)
}

/// One chosen participant of `unit` on `planet` is gone: the choice shrinks by one.
pub(crate) fn alastor_decrement(
    state: &mut GameState,
    system: &SystemId,
    owner: &PlayerId,
    planet: &PlanetId,
    unit: &str,
) {
    let key = alastor_key(state, system, owner, planet, unit);
    let Some(count) = state
        .faction_marks
        .get(&key)
        .and_then(|count| count.parse::<usize>().ok())
    else {
        return;
    };
    if count <= 1 {
        state.faction_marks.remove(&key);
    } else {
        state.faction_marks.insert(key, (count - 1).to_string());
    }
}

/// End the participation in `system` (of one player, or of everyone), whatever activation made it.
pub(crate) fn alastor_clear(state: &mut GameState, system: &SystemId, player: Option<&PlayerId>) {
    if state.faction_marks.is_empty() {
        return;
    }
    let start = format!("{ALASTOR}|");
    let stale: Vec<String> = state
        .faction_marks
        .range(start.clone()..)
        .map_while(|(key, _)| key.strip_prefix(&start).map(|rest| (key, rest)))
        .filter(|(_, rest)| {
            let mut parts = rest.split('|');
            let (_, marked_system, marked_player) = (parts.next(), parts.next(), parts.next());
            marked_system == Some(system.as_str())
                && player.is_none_or(|who| marked_player == Some(who.as_str()))
        })
        .map(|(key, _)| key.clone())
        .collect();
    for key in stale {
        state.faction_marks.remove(&key);
    }
}

/// `player`'s ground forces on planets of `system`, as a count per planet and unit type.
fn ground_forces_on_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> BTreeMap<(PlanetId, String), usize> {
    let types = ti4_content::units::catalogue(content, sources);
    let mut found = BTreeMap::new();
    for (planet, units) in &state.system_state(system).planet_units {
        for unit in units.iter().filter(|unit| &unit.owner == player) {
            if types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ground_force)
            {
                *found
                    .entry((planet.clone(), unit.type_id.to_string()))
                    .or_insert(0) += 1;
            }
        }
    }
    found
}

/// The Alastor: at the start of a space combat its owner chooses, per planet and unit type, how
/// many of their ground forces in the system join the combat as ships. A mandatory window whose
/// every question may be answered "none" (the card says "any number").
fn alastor(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{FLAGSHIP}:SPACE_COMBAT_STARTED:after"),
        seat.clone(),
        "SPACE_COMBAT_STARTED",
        Relation::After,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            choose_participants(context, &owner, &system)
        }),
    )
    .with_stateful_condition(Arc::new(move |event, _, context| {
        let Some(system) = event.text("system").map(SystemId::new) else {
            return false;
        };
        took_part(event, &condition_owner)
            && context
                .state
                .system_state(&system)
                .units
                .iter()
                .any(|unit| unit.owner == condition_owner && unit.type_id.as_str() == FLAGSHIP)
            && !ground_forces_on_planets(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
                &system,
            )
            .is_empty()
    }))
}

fn choose_participants(
    context: &mut TimingContext<'_>,
    owner: &PlayerId,
    system: &SystemId,
) -> Result<(), TimingError> {
    let groups = ground_forces_on_planets(
        context.state,
        context.content,
        context.sources,
        owner,
        system,
    );
    let mut chosen = Vec::new();
    for ((planet, unit), count) in groups {
        let mut options: Vec<ChoiceOption> = (1..=count)
            .map(|n| {
                ChoiceOption::labelled(
                    format!("alastor|{n}"),
                    "nekro_alastor",
                    format!("{n} {unit} on {planet} fight as ships"),
                )
                .with("planet", planet.to_string())
                .with("unit", unit.clone())
                .with("count", i64::try_from(n).unwrap_or(i64::MAX))
            })
            .collect();
        options.push(ChoiceOption::decline());
        let answer = ask(
            context,
            owner,
            format!("The Alastor: how many {unit} on {planet} fight as ships?"),
            FLAGSHIP,
            "alastor_participants",
            options,
        )
        .map_err(illegal)?;
        let Some(n) = answer
            .id
            .strip_prefix("alastor|")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| (1..=count).contains(n))
        else {
            continue;
        };
        chosen.push((planet, unit, n));
    }
    for (planet, unit, n) in chosen {
        let key = alastor_key(context.state, system, owner, &planet, &unit);
        context.state.faction_marks.insert(key, n.to_string());
    }
    Ok(())
}

// -- Mordred -------------------------------------------------------------------------------------

/// Whether `opponent` has an "X" or "Y" assimilator token on 1 or more of their technologies, for
/// the Nekro `owner` whose tokens they are (`nekro::assimilated_card`).
fn has_token_on_technology(state: &GameState, owner: &PlayerId, opponent: &PlayerId) -> bool {
    state.player(opponent).is_some_and(|seat| {
        seat.technologies
            .iter()
            .any(|tech| super::nekro::assimilated_card(state, owner, tech.as_str()).is_some())
    })
}

/// The roll bonus Mordred gives when `player`'s `unit_type` rolls: +2 for the mech in a combat
/// against an opponent with an "X" or "Y" token on a technology of theirs; zero for everything else,
/// so games without Nekro are unchanged. A space combat (`planet` is `None`) is against the other
/// side of the combat in `system`; a ground combat against the other players' ground forces on the
/// planet. Read by `combat::effective_from` and `invasion::ground_combat_value`.
#[must_use]
pub(crate) fn mech_roll_bonus(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit_type: &str,
    system: Option<&SystemId>,
    planet: Option<&PlanetId>,
) -> i64 {
    if unit_type != MECH || !super::nekro::is_nekro(state, player) {
        return 0;
    }
    let Some(system) = system else {
        return 0;
    };
    let opponents: Vec<PlayerId> = match planet {
        Some(planet) => state
            .system_state(system)
            .planet_units
            .get(planet)
            .into_iter()
            .flatten()
            .map(|unit| unit.owner.clone())
            .filter(|owner| owner != player)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        None => crate::combat::combatants(state, content, sources, system)
            .into_iter()
            .filter(|side| side != player)
            .collect(),
    };
    if opponents
        .iter()
        .any(|opponent| has_token_on_technology(state, player, opponent))
    {
        MECH_BONUS
    } else {
        0
    }
}

// -- Nekro Acidos: the unlock --------------------------------------------------------------------

/// Nekro Acidos' unlock: "Own 3 technologies. A \"Valefar Assimilator\" technology counts only if
/// its X or Y token is on a technology." `None` for any other leader.
pub(crate) fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    if leader.as_str() != COMMANDER {
        return None;
    }
    let seat = state.player(player)?;
    let assimilators = ASSIMILATORS
        .iter()
        .filter(|card| seat.technologies.contains(&TechnologyId::new(**card)))
        .count();
    let counting = seat.technologies.len() - assimilators
        + super::nekro::assimilators_with_tokens(state, player);
    Some(counting >= 3)
}

// -- Nekro Malleon -------------------------------------------------------------------------------

/// Whether `who` could pay for the agent's offer: they hold an action card or a command token.
fn can_pay_for_agent(state: &GameState, who: &PlayerId) -> bool {
    state.player(who).is_some_and(|seat| {
        !seat.action_cards.is_empty()
            || seat.tactic_tokens > 0
            || seat.fleet_tokens > 0
            || seat.strategic_tokens > 0
    })
}

/// Whether `leader` is Nekro Malleon or the hero and `player` could use the ACTION now. The agent
/// needs a player who could pay (a use nobody can answer would only burn the card); the hero needs a
/// planet to devour. `None` for any other leader.
pub(crate) fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    match leader.as_str() {
        AGENT => Some(
            leader_status(state, player, AGENT) == Some(LeaderStatus::Readied)
                && state
                    .players
                    .iter()
                    .any(|seat| can_pay_for_agent(state, &seat.id)),
        ),
        HERO => Some(
            leader_status(state, player, HERO) == Some(LeaderStatus::Unlocked)
                && !devourable_planets(state, content, SourceSet::all(), player).is_empty(),
        ),
        _ => None,
    }
}

/// Use Nekro Malleon or UNIT.DSGN.FLAYESH; `None` for any other leader, else whether it resolved.
/// The shared code exhausts the agent / purges the hero after `Some(true)`.
pub(crate) fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    match leader.as_str() {
        AGENT => Some(malleon(context, player).unwrap_or(false)),
        HERO => Some(flayesh(context, player).unwrap_or(false)),
        _ => None,
    }
}

/// Nekro Malleon: the owner chooses a player who could pay; that player may discard 1 action card or
/// spend 1 command token from their command sheet to gain 2 trade goods (the token returns to the
/// supply). Atomic: every question is asked before the first change.
fn malleon(context: &mut TimingContext<'_>, owner: &PlayerId) -> Result<bool, IllegalChoice> {
    if leader_action(context.state, context.content, owner, &LeaderId::new(AGENT)) != Some(true) {
        return Ok(false);
    }
    let payers: Vec<PlayerId> = context
        .state
        .players
        .iter()
        .map(|seat| seat.id.clone())
        .filter(|who| can_pay_for_agent(context.state, who))
        .collect();
    let target = match payers.as_slice() {
        [only] => only.clone(),
        _ => {
            let options = payers
                .iter()
                .map(|who| ChoiceOption::labelled(who.to_string(), "player", who.to_string()))
                .collect();
            let answer = ask(
                context,
                owner,
                "Nekro Malleon: choose a player".to_owned(),
                AGENT,
                "malleon_player",
                options,
            )?;
            let Some(chosen) = payers.iter().find(|who| who.as_str() == answer.id) else {
                return Ok(false);
            };
            chosen.clone()
        }
    };
    // The chosen player's own answer: a card, a token, or neither ("may").
    let mut options = Vec::new();
    let seat = context.state.player(&target);
    if seat.is_some_and(|seat| !seat.action_cards.is_empty()) {
        options.push(ChoiceOption::labelled(
            "discard",
            "nekro_malleon",
            "discard 1 action card to gain 2 trade goods",
        ));
    }
    let pools: Vec<(&str, TokenPool, i32)> = seat
        .map(|seat| {
            vec![
                ("tactic", TokenPool::Tactic, seat.tactic_tokens),
                ("fleet", TokenPool::Fleet, seat.fleet_tokens),
                ("strategy", TokenPool::Strategic, seat.strategic_tokens),
            ]
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, _, tokens)| *tokens > 0)
        .collect();
    for (name, _, _) in &pools {
        options.push(ChoiceOption::labelled(
            format!("token|{name}"),
            "nekro_malleon",
            format!("spend 1 {name} command token to gain 2 trade goods"),
        ));
    }
    options.push(ChoiceOption::decline());
    let answer = ask(
        context,
        &target,
        "Nekro Malleon: discard 1 action card or spend 1 command token to gain 2 trade goods?"
            .to_owned(),
        AGENT,
        "malleon_payment",
        options,
    )?;
    if answer.id == "discard" {
        let Some(card) = crate::action_cards::choose_from_own_hand(
            context,
            &target,
            AGENT,
            "malleon_discard",
            "Nekro Malleon: discard 1 action card",
            false,
        )?
        else {
            return Ok(true);
        };
        if super::hooks_cards::discard_chosen(context.state, &target, &card) {
            crate::supply::gain_trade_goods_staged(context.state, &target, 2, AGENT);
        }
    } else if let Some(name) = answer.id.strip_prefix("token|")
        && let Some((_, pool, _)) = pools.iter().find(|(pool, _, _)| *pool == name)
    {
        let spent = context
            .state
            .player_mut(&target)
            .is_some_and(|seat| seat.spend_token(*pool));
        if spent {
            crate::supply::gain_trade_goods_staged(context.state, &target, 2, AGENT);
        }
    }
    // The agent was used whether or not the chosen player took the offer.
    Ok(true)
}

// -- UNIT.DSGN.FLAYESH ---------------------------------------------------------------------------

/// The technology colours matching a planet's specialties now.
fn specialty_colours(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
) -> BTreeSet<&'static str> {
    crate::planets::tech_specialties_now(state, content, sources, planet)
        .iter()
        .filter_map(|specialty| {
            let upper = specialty.to_ascii_uppercase();
            ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"]
                .into_iter()
                .find(|colour| *colour == upper)
        })
        .collect()
}

/// Planets with a technology specialty in a system that contains `player`'s units.
fn devourable_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(SystemId, PlanetId)> {
    let mut found = Vec::new();
    for system in state.board.keys() {
        if !has_units(state, system, player) {
            continue;
        }
        let mut planets: Vec<PlanetId> =
            ti4_content::galaxy::planets_in(content, system.as_str(), sources)
                .into_iter()
                .map(|planet| PlanetId::new(planet.id()))
                .collect();
        planets.sort();
        for planet in planets {
            if !specialty_colours(state, content, sources, &planet).is_empty() {
                found.push((system.clone(), planet));
            }
        }
    }
    found
}

/// The technologies `player` could gain that match one of `colours`: current-deck colour
/// technologies they do not own and may hold (a faction technology only of their own faction).
fn gainable_technologies(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    colours: &BTreeSet<&'static str>,
) -> Vec<TechnologyId> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    crate::technology::active_aliases(content, sources)
        .into_iter()
        .filter(|alias| !seat.technologies.contains(alias))
        .filter(|alias| !crate::technology::is_unit_upgrade(content, alias))
        .filter(|alias| {
            crate::technology::colour_type(content, alias).is_some_and(|c| colours.contains(c))
        })
        .filter(|alias| {
            crate::technology::faction_of(content, alias)
                .is_none_or(|faction| faction == seat.faction.as_str())
        })
        .collect()
}

/// Polymorphic Algorithm: Devour World. Asks for the planet and the technology first, then destroys
/// every other player's units on the planet (ground forces announced as destroyed, structures
/// simply removed), gains the planet's combined resources and influence in trade goods and the
/// technology. Gaining is not researching, so Propagation does not apply.
fn flayesh(context: &mut TimingContext<'_>, player: &PlayerId) -> Result<bool, IllegalChoice> {
    let (content, sources) = (context.content, context.sources);
    if leader_action(context.state, content, player, &LeaderId::new(HERO)) != Some(true) {
        return Ok(false);
    }
    let targets = devourable_planets(context.state, content, sources, player);
    let (system, planet) = match targets.as_slice() {
        [only] => only.clone(),
        _ => {
            let options = targets
                .iter()
                .map(|(system, planet)| {
                    ChoiceOption::labelled(
                        format!("{system}|{planet}"),
                        "nekro_flayesh_planet",
                        format!("devour {planet} in {system}"),
                    )
                })
                .collect();
            let answer = ask(
                context,
                player,
                "UNIT.DSGN.FLAYESH: choose a planet with a technology specialty".to_owned(),
                HERO,
                "flayesh_planet",
                options,
            )?;
            let Some(found) = targets
                .iter()
                .find(|(system, planet)| answer.id == format!("{system}|{planet}"))
            else {
                return Ok(false);
            };
            found.clone()
        }
    };
    let colours = specialty_colours(context.state, content, sources, &planet);
    let candidates = gainable_technologies(context.state, content, sources, player, &colours);
    let technology = match candidates.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        _ => {
            let options = candidates
                .iter()
                .map(|alias| {
                    ChoiceOption::labelled(
                        alias.to_string(),
                        "nekro_flayesh_technology",
                        crate::technology::name(content, alias),
                    )
                })
                .collect();
            let answer = ask(
                context,
                player,
                "UNIT.DSGN.FLAYESH: choose a technology that matches the specialty".to_owned(),
                HERO,
                "flayesh_technology",
                options,
            )?;
            let Some(found) = candidates.iter().find(|alias| alias.as_str() == answer.id) else {
                return Ok(false);
            };
            Some(found.clone())
        }
    };
    // Every question is answered: change the position.
    destroy_others_on(context.state, content, sources, player, &system, &planet);
    let goods = crate::production::planet_value_now(
        context.state,
        content,
        sources,
        &planet,
        Spend::Resources,
    ) + crate::production::planet_value_now(
        context.state,
        content,
        sources,
        &planet,
        Spend::Influence,
    );
    crate::supply::gain_trade_goods_staged(
        context.state,
        player,
        i32::try_from(goods).unwrap_or(i32::MAX),
        HERO,
    );
    if let Some(alias) = technology {
        crate::technology::gain(context.state, content, sources, player, &alias);
    }
    Ok(true)
}

/// Destroy every other player's units on one planet: ground forces are staged as destroyed ground
/// forces (cause `nekrohero`), structures are simply removed with them.
fn destroy_others_on(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    owner: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) {
    let victims: Vec<Unit> = state
        .system_state(system)
        .on_planet(planet)
        .iter()
        .filter(|unit| &unit.owner != owner)
        .cloned()
        .collect();
    let types = ti4_content::units::catalogue(content, sources);
    for unit in victims {
        state
            .system_mut(system)
            .remove_from_planet(planet, std::slice::from_ref(&unit));
        if types
            .get(unit.type_id.as_str())
            .is_some_and(ti4_content::units::UnitType::is_ground_force)
        {
            super::hooks_ground::stage_ground_force_destroyed(
                state,
                system,
                planet,
                &unit,
                "nekrohero",
            );
        }
    }
}

// -- nekroc4y ------------------------------------------------------------------------------------

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

/// `nekroc4y`: after one of the owner's ships is destroyed, they may produce a ship of that type at
/// a space dock in their home system (paid for as ordinary production, placed in the home system's
/// space area). Offered only when that production can happen.
fn null_reference(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{NULL_REFERENCE}:SHIP_DESTROYED:after"),
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
            && owns_technology(context.state, &condition_owner, NULL_REFERENCE)
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

// -- nekroc4r ------------------------------------------------------------------------------------

/// Planets `player` controls where a PDS may be placed now: not demilitarized, under the structure
/// cap, and with a PDS left in reinforcements.
fn pds_planets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<(SystemId, PlanetId)> {
    let Some(pds) = crate::action_cards::placed_unit_id(state, content, sources, player, "pds")
    else {
        return Vec::new();
    };
    if crate::supply::allowed(state, content, sources, player, &pds, 1) == 0 {
        return Vec::new();
    }
    crate::action_cards::placement_spots(
        state,
        content,
        sources,
        player,
        crate::action_cards::PlacementTarget::ControlledPlanet,
        None,
    )
    .into_iter()
    .filter_map(|(system, planet)| Some((system, planet?)))
    .filter(|(_, planet)| {
        crate::production::structure_allowed(state, content, sources, player, planet, "pds")
    })
    .collect()
}

/// Whether `player` has a damaged unit anywhere.
fn has_damaged_unit(state: &GameState, player: &PlayerId) -> bool {
    state.board.values().any(|board| {
        board
            .units
            .iter()
            .chain(board.planet_units.values().flatten())
            .any(|unit| &unit.owner == player && unit.sustained_damage)
    })
}

/// The ACTIONs of `nekroc4r` the owner could take now, one option per ACTION that can resolve.
pub(crate) fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    if !technology_ready(state, player, ERROR_ERROR) {
        return Vec::new();
    }
    let sources = SourceSet::all();
    let mut options = Vec::new();
    if !pds_planets(state, content, sources, player).is_empty() {
        options.push(ChoiceOption::labelled(
            C4R_PLACE,
            "component",
            "???_ERROR_ERROR_???: place 1 PDS on a planet you control",
        ));
    }
    if has_damaged_unit(state, player) {
        options.push(ChoiceOption::labelled(
            C4R_REPAIR,
            "component",
            "???_ERROR_ERROR_???: repair all of your damaged units",
        ));
    }
    if state
        .player(player)
        .is_some_and(|seat| !seat.action_cards.is_empty())
        && !state.action_card_deck.is_empty()
    {
        options.push(ChoiceOption::labelled(
            C4R_DRAW,
            "component",
            "???_ERROR_ERROR_???: discard 1 action card to draw 1 action card",
        ));
    }
    options
}

/// Perform one of the ACTIONs of `nekroc4r`; `true` if this module claimed and performed it. Every
/// question is asked before the card is exhausted, and a refusal changes nothing.
pub(crate) fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    if !matches!(option.id.as_str(), C4R_PLACE | C4R_REPAIR | C4R_DRAW)
        || !component_actions(context.state, context.content, player)
            .iter()
            .any(|offered| offered.id == option.id)
    {
        return false;
    }
    let done = match option.id.as_str() {
        C4R_PLACE => place_pds(context, player),
        C4R_REPAIR => {
            for board in context.state.board.values_mut() {
                for unit in board
                    .units
                    .iter_mut()
                    .chain(board.planet_units.values_mut().flatten())
                    .filter(|unit| &unit.owner == player)
                {
                    unit.sustained_damage = false;
                }
            }
            true
        }
        _ => discard_and_draw(context, player),
    };
    if done && let Some(seat) = context.state.player_mut(player) {
        seat.exhausted_technologies
            .insert(TechnologyId::new(ERROR_ERROR));
    }
    done
}

fn place_pds(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let spots = pds_planets(context.state, context.content, context.sources, player);
    let chosen = match spots.as_slice() {
        [] => return false,
        [only] => only.clone(),
        _ => {
            let options = spots
                .iter()
                .map(|(system, planet)| {
                    ChoiceOption::labelled(
                        format!("{system}|{planet}"),
                        "nekro_c4r_planet",
                        format!("place a PDS on {planet} in {system}"),
                    )
                })
                .collect();
            let Ok(answer) = ask(
                context,
                player,
                "???_ERROR_ERROR_???: choose a planet you control".to_owned(),
                ERROR_ERROR,
                "c4r_planet",
                options,
            ) else {
                return false;
            };
            let Some(found) = spots
                .iter()
                .find(|(system, planet)| answer.id == format!("{system}|{planet}"))
            else {
                return false;
            };
            found.clone()
        }
    };
    crate::action_cards::place_units_counted(context, player, &chosen.0, Some(&chosen.1), "pds", 1)
        == 1
}

fn discard_and_draw(context: &mut TimingContext<'_>, player: &PlayerId) -> bool {
    let Ok(Some(card)) = crate::action_cards::choose_from_own_hand(
        context,
        player,
        ERROR_ERROR,
        "c4r_discard",
        "???_ERROR_ERROR_???: discard 1 action card",
        false,
    ) else {
        return false;
    };
    if !super::hooks_cards::discard_chosen(context.state, player, &card) {
        return false;
    }
    // The discard stands: a draw that the hand limit questions is the player's to answer.
    let _ = crate::action_cards::draw(context.state, context.content, context.table, player, 1);
    true
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{ActionCardId, UnitTypeId};

    use super::*;
    use crate::choice::{Scripted, Table, TimingHandle};
    use crate::event::EventSequence;

    fn a() -> PlayerId {
        PlayerId::new("a")
    }
    fn b() -> PlayerId {
        PlayerId::new("b")
    }
    fn game() -> GameState {
        crate::fixtures::seated_game(&[("a", "nekro"), ("b", "sol")], DEFAULT)
    }
    fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }
    /// The Alastor and Technological Singularity's bookkeeping share a window: the first answer
    /// orders them, the rest answer the Alastor's questions.
    fn alastor_script(answers: &[&str]) -> Table {
        let mut script = vec!["unit:nekro:nekro_flagship:SPACE_COMBAT_STARTED:after"];
        script.extend_from_slice(answers);
        scripted(&script)
    }
    fn system() -> SystemId {
        SystemId::new("18")
    }
    fn planet() -> PlanetId {
        PlanetId::new("mr")
    }
    fn payload(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    /// Emit one typed event through a resolver armed as the game arms one, so the module's hook
    /// wiring is part of what is tested.
    fn emit(state: &mut GameState, table: &mut Table, kind: &str, pairs: &[(&str, Value)]) {
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, table, |ctx| {
            let event = EventSequence::new().next(kind, payload(pairs)).unwrap();
            resolver
                .emit_with_context(ctx, event, |_, _| {})
                .expect("emits");
        });
    }

    fn combat_started(state: &mut GameState, table: &mut Table) {
        emit(
            state,
            table,
            "SPACE_COMBAT_STARTED",
            &[
                ("system", "18".into()),
                ("attacker", "a".into()),
                ("defender", "b".into()),
                ("player", "a".into()),
                ("round", 1.into()),
            ],
        );
    }

    fn count_on_planet(state: &GameState, who: &PlayerId, kind: &str) -> usize {
        state
            .system_state(&system())
            .on_planet(&planet())
            .iter()
            .filter(|unit| &unit.owner == who && unit.type_id.as_str() == kind)
            .count()
    }

    /// `a` has the flagship in space and `mechs` mechs on Mecatol Rex; `b` has a cruiser in space.
    fn battle(mechs: usize) -> GameState {
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        state.active_system = Some(system());
        crate::fixtures::put(&mut state, &system(), FLAGSHIP, &a(), 1);
        crate::fixtures::put(&mut state, &system(), "cruiser", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &system(), &planet(), MECH, &a(), mechs);
        state
    }

    fn ships(state: &GameState, who: &PlayerId) -> Vec<String> {
        crate::combat::ships_of(state, ContentStore::embedded(), DEFAULT, who, &system())
            .into_iter()
            .map(|unit| unit.type_id.to_string())
            .collect()
    }

    fn alastor_marks_left(state: &GameState) -> usize {
        state
            .faction_marks
            .keys()
            .filter(|key| key.starts_with(ALASTOR))
            .count()
    }

    fn absorb(state: &mut GameState, table: &mut Table, hits: usize) {
        let content = ContentStore::embedded();
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(0);
        let mut ctx = Resolving {
            content,
            sources: DEFAULT,
            dice: &mut dice,
            rng: &mut rng,
            table,
            timing: None,
        };
        crate::combat::absorb_hits_seeing_with_origin(
            state,
            content,
            DEFAULT,
            None,
            &mut ctx,
            &a(),
            &system(),
            &b(),
            hits,
            crate::combat::HitOrigin::CombatRoll,
        )
        .expect("the hits are assigned");
    }

    // -- The Alastor ------------------------------------------------------------------------------

    #[test]
    fn nekro_alastor_makes_the_chosen_ground_forces_ships() {
        let mut state = battle(2);
        assert_eq!(
            ships(&state, &a()),
            [FLAGSHIP],
            "ground forces are not ships yet"
        );
        combat_started(&mut state, &mut alastor_script(&["alastor|1"]));
        assert_eq!(
            ships(&state, &a()),
            [FLAGSHIP, MECH],
            "exactly the 1 chosen mech fights as a ship"
        );
        assert_eq!(
            count_on_planet(&state, &a(), MECH),
            2,
            "they stay on the planet"
        );
        assert_eq!(
            alastor_participants(&state, &a(), &system()).len(),
            1,
            "one of the two"
        );
        assert!(
            ships(&state, &b()) == ["cruiser"],
            "the opponent's fleet is untouched"
        );
    }

    #[test]
    fn nekro_alastor_chooses_any_number_including_none() {
        let mut state = battle(2);
        combat_started(&mut state, &mut alastor_script(&["decline"]));
        assert_eq!(ships(&state, &a()), [FLAGSHIP], "none chosen, none fight");
        assert_eq!(alastor_marks_left(&state), 0);

        let mut state = battle(2);
        combat_started(&mut state, &mut alastor_script(&["alastor|2"]));
        assert_eq!(ships(&state, &a()), [FLAGSHIP, MECH, MECH]);
    }

    #[test]
    fn nekro_alastor_is_asked_per_planet_and_unit_type() {
        let mut state = battle(1);
        crate::fixtures::put_on_planet(&mut state, &system(), &planet(), "infantry", &a(), 3);
        combat_started(&mut state, &mut alastor_script(&["alastor|2", "alastor|1"]));
        // Groups are asked in planet, then unit-type order: infantry (of 3), then the mech.
        assert_eq!(
            ships(&state, &a()),
            [FLAGSHIP, MECH, "infantry", "infantry"],
            "2 infantry and the mech"
        );
        assert_eq!(count_on_planet(&state, &a(), "infantry"), 3);
    }

    #[test]
    fn nekro_alastor_is_neutral_without_the_flagship_or_a_combat_seat() {
        // No flagship in the system.
        let mut state = battle(1);
        let unit = Unit::new(UnitTypeId::new(FLAGSHIP), a());
        state
            .system_mut(&system())
            .remove(std::slice::from_ref(&unit));
        let mut table = scripted(&[]);
        combat_started(&mut state, &mut table);
        assert_eq!(alastor_marks_left(&state), 0);
        assert!(table.log.is_empty(), "nothing was asked");

        // A seat that is not in the combat.
        let mut state = battle(1);
        let mut table = scripted(&[]);
        emit(
            &mut state,
            &mut table,
            "SPACE_COMBAT_STARTED",
            &[
                ("system", "18".into()),
                ("attacker", "b".into()),
                ("defender", "c".into()),
                ("player", "b".into()),
                ("round", 1.into()),
            ],
        );
        assert_eq!(alastor_marks_left(&state), 0);
        assert!(table.log.is_empty());
    }

    #[test]
    fn nekro_a_game_without_nekro_is_unchanged() {
        let mut state = crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan")], DEFAULT);
        crate::fixtures::put(&mut state, &system(), "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &system(), "cruiser", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &system(), &planet(), "mech", &a(), 1);
        let before = state.clone();
        let mut table = scripted(&[]);
        combat_started(&mut state, &mut table);
        emit(
            &mut state,
            &mut table,
            "SHIP_DESTROYED",
            &[
                ("system", "18".into()),
                ("player", "a".into()),
                ("unit", "cruiser".into()),
            ],
        );
        assert_eq!(state, before);
        assert!(table.log.is_empty());
        assert!(alastor_participants(&state, &a(), &system()).is_empty());
        assert_eq!(
            mech_roll_bonus(
                &state,
                ContentStore::embedded(),
                DEFAULT,
                &a(),
                MECH,
                Some(&system()),
                None
            ),
            0
        );
    }

    #[test]
    fn nekro_alastor_participants_roll_in_a_real_combat_and_the_marks_end_with_it() {
        let content = ContentStore::embedded();
        let mut state = battle(1);
        let mut resolver = crate::fixtures::armed_resolver(&state);
        let mut sequence = EventSequence::new();
        // Ascending combat value: the mech (6) rolls first, then the flagship (9, 2 dice), then the
        // opponent's cruiser. Only the mech's die hits, so the cruiser dies in round 1.
        let mut dice = crate::dice::Dice::from_faces([10, 1, 1, 1]);
        let mut rng = crate::rng::GameRng::new(0);
        let mut table = alastor_script(&["alastor|1"]);
        let outcome = {
            let mut ctx = Resolving {
                content,
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
            crate::combat::resolve_resolving(&mut state, &mut ctx, &system(), None)
                .expect("the combat resolves")
        };
        assert_eq!(outcome.winner, Some(a()));
        assert_eq!(outcome.rounds, 1, "the mech's roll destroyed the cruiser");
        let mech_rolls = dice
            .history()
            .iter()
            .filter(|roll| roll.hits_on == Some(6) && roll.by.as_deref() == Some("a"))
            .count();
        assert_eq!(
            mech_rolls,
            1,
            "the mech rolled its own die at 6: {:?}",
            dice.history()
        );
        assert_eq!(count_on_planet(&state, &a(), MECH), 1, "the mech is whole");
        assert_eq!(
            alastor_marks_left(&state),
            0,
            "the marks ended with the combat"
        );
        assert_eq!(ships(&state, &a()), [FLAGSHIP]);
    }

    #[test]
    fn nekro_a_participant_can_sustain_damage_and_is_then_destroyed() {
        let mut state = battle(1);
        combat_started(&mut state, &mut alastor_script(&["alastor|1"]));
        let flagship = Unit::new(UnitTypeId::new(FLAGSHIP), a());
        state
            .system_mut(&system())
            .remove(std::slice::from_ref(&flagship));
        // Without the flagship in space the combat is still the mech's: it is a ship for it.
        assert_eq!(ships(&state, &a()), [MECH]);

        // First hit: SUSTAIN DAMAGE is offered for the mech and taken.
        absorb(
            &mut state,
            &mut Table::with_default(Box::new(crate::choice::FirstOption)),
            1,
        );
        let mech = state.system_state(&system()).on_planet(&planet())[0].clone();
        assert!(mech.sustained_damage, "the mech used its SUSTAIN DAMAGE");

        // Second hit: nothing left to sustain, the mech is destroyed where it stands.
        absorb(
            &mut state,
            &mut Table::with_default(Box::new(crate::choice::FirstOption)),
            1,
        );
        assert_eq!(count_on_planet(&state, &a(), MECH), 0);
        assert_eq!(alastor_marks_left(&state), 0, "its choice went with it");
    }

    #[test]
    fn nekro_a_destroyed_participant_leaves_the_others_choice_intact() {
        let mut state = battle(2);
        combat_started(&mut state, &mut alastor_script(&["alastor|2"]));
        let flagship = Unit::new(UnitTypeId::new(FLAGSHIP), a());
        state
            .system_mut(&system())
            .remove(std::slice::from_ref(&flagship));
        assert_eq!(ships(&state, &a()), [MECH, MECH]);
        // Decline SUSTAIN DAMAGE (the first answer); the casualty then falls to the first option.
        absorb(&mut state, &mut scripted(&["decline"]), 1);
        assert_eq!(
            count_on_planet(&state, &a(), MECH),
            1,
            "one mech was destroyed"
        );
        assert_eq!(
            ships(&state, &a()),
            [MECH],
            "the other participant still fights"
        );
        assert_eq!(alastor_marks_left(&state), 1, "its choice shrank to 1");
    }

    #[test]
    fn nekro_a_retreat_ends_the_participation_but_leaves_the_forces_on_their_planet() {
        let mut state = battle(1);
        combat_started(&mut state, &mut alastor_script(&["alastor|1"]));
        assert_eq!(ships(&state, &a()), [FLAGSHIP, MECH]);
        let destination = SystemId::new("19");
        crate::combat::retreat_to(
            &mut state,
            ContentStore::embedded(),
            DEFAULT,
            &a(),
            &system(),
            &destination,
        );
        assert_eq!(alastor_marks_left(&state), 0);
        assert_eq!(count_on_planet(&state, &a(), MECH), 1, "the mech stays put");
        assert!(ships(&state, &a()).is_empty());
    }

    // -- Mordred ----------------------------------------------------------------------------------

    /// `b` owns `Advanced Carrier II` and `a`'s Valefar Assimilator X sits on it.
    fn token_on_b(state: &mut GameState) {
        crate::technology::grant(state, &a(), &TechnologyId::new("vax"));
        crate::technology::grant(state, &b(), &TechnologyId::new("ac2"));
        state
            .player_mut(&a())
            .unwrap()
            .assimilated_technologies
            .insert("vax".to_owned(), TechnologyId::new("ac2"));
    }

    fn bonus(state: &GameState, unit: &str, planet: Option<&PlanetId>) -> i64 {
        mech_roll_bonus(
            state,
            ContentStore::embedded(),
            DEFAULT,
            &a(),
            unit,
            Some(&system()),
            planet,
        )
    }

    #[test]
    fn nekro_mordred_gets_plus_two_against_an_opponent_with_a_token() {
        let mut state = battle(1);
        assert_eq!(bonus(&state, MECH, None), 0, "no token on b's technologies");
        token_on_b(&mut state);
        assert_eq!(bonus(&state, MECH, None), 2);
        assert_eq!(bonus(&state, FLAGSHIP, None), 0, "only the mech");
        // The token leaves the technology when b no longer owns it.
        state
            .player_mut(&b())
            .unwrap()
            .technologies
            .remove(&TechnologyId::new("ac2"));
        assert_eq!(bonus(&state, MECH, None), 0);
    }

    #[test]
    fn nekro_mordred_bonus_reaches_the_space_and_ground_thresholds() {
        let content = ContentStore::embedded();
        let mut state = battle(1);
        let mech = Unit::new(UnitTypeId::new(MECH), a());
        let plain = crate::combat::effective_hits_on(&state, content, DEFAULT, &a(), &mech);
        assert_eq!(plain, Some(6));
        crate::fixtures::put_on_planet(&mut state, &system(), &planet(), "infantry", &b(), 1);
        token_on_b(&mut state);
        assert_eq!(
            crate::combat::effective_hits_on(&state, content, DEFAULT, &a(), &mech),
            Some(4),
            "+2 to the roll in a space combat"
        );
        assert_eq!(
            crate::invasion::ground_combat_value(
                &state,
                content,
                DEFAULT,
                &a(),
                &system(),
                &planet(),
                MECH
            ),
            Some(4),
            "+2 to the roll in a ground combat"
        );
        // An opponent without a token on the planet: no bonus.
        state
            .system_mut(&system())
            .planet_units
            .get_mut(&planet())
            .unwrap()
            .retain(|unit| unit.owner == a());
        assert_eq!(
            crate::invasion::ground_combat_value(
                &state,
                content,
                DEFAULT,
                &a(),
                &system(),
                &planet(),
                MECH
            ),
            Some(6),
            "nobody to fight on the planet"
        );
    }

    // -- Nekro Acidos -----------------------------------------------------------------------------

    fn commander_met(state: &GameState) -> Option<bool> {
        commander_unlocked(
            state,
            ContentStore::embedded(),
            DEFAULT,
            None,
            &a(),
            &LeaderId::new(COMMANDER),
        )
    }

    fn technologies(state: &mut GameState, aliases: &[&str]) {
        state.player_mut(&a()).unwrap().technologies = aliases
            .iter()
            .map(|alias| TechnologyId::new(*alias))
            .collect();
    }

    #[test]
    fn nekro_commander_needs_three_technologies_and_valefar_counts_only_with_a_token() {
        let mut state = battle(0);
        technologies(&mut state, &["dxa", "gd"]);
        assert_eq!(commander_met(&state), Some(false), "two technologies");
        technologies(&mut state, &["dxa", "gd", "amd"]);
        assert_eq!(commander_met(&state), Some(true), "three technologies");
        // Valefar Assimilator X without its token does not count.
        technologies(&mut state, &["dxa", "gd", "vax"]);
        assert_eq!(commander_met(&state), Some(false), "vax has no token");
        crate::technology::grant(&mut state, &b(), &TechnologyId::new("ac2"));
        state
            .player_mut(&a())
            .unwrap()
            .assimilated_technologies
            .insert("vax".to_owned(), TechnologyId::new("ac2"));
        assert_eq!(
            commander_met(&state),
            Some(true),
            "vax with its token counts"
        );
        // Both assimilators: only the one with a token counts.
        technologies(&mut state, &["dxa", "vax", "vay"]);
        assert_eq!(
            commander_met(&state),
            Some(false),
            "dxa + vax, vay has none"
        );
        assert_eq!(
            commander_unlocked(
                &state,
                ContentStore::embedded(),
                DEFAULT,
                None,
                &a(),
                &LeaderId::new(AGENT)
            ),
            None,
            "not this module's commander"
        );
    }

    // -- Nekro Malleon ----------------------------------------------------------------------------

    fn agent() -> LeaderId {
        LeaderId::new(AGENT)
    }

    fn use_malleon(state: &mut GameState, answers: &[&str]) -> Option<bool> {
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(answers), |ctx| {
            use_leader(ctx, &a(), &agent())
        })
    }

    fn give_card(state: &mut GameState, who: &PlayerId, card: &str) {
        state
            .player_mut(who)
            .unwrap()
            .action_cards
            .push(ActionCardId::new(card));
    }

    #[test]
    fn nekro_malleon_lets_the_chosen_player_spend_a_token_for_two_trade_goods() {
        let mut state = battle(0);
        assert_eq!(
            leader_action(&state, ContentStore::embedded(), &a(), &agent()),
            Some(true)
        );
        let goods = state.player(&b()).unwrap().trade_goods;
        let tactic = state.player(&b()).unwrap().tactic_tokens;
        assert_eq!(use_malleon(&mut state, &["b", "token|tactic"]), Some(true));
        let seat = state.player(&b()).unwrap();
        assert_eq!(seat.trade_goods, goods + 2);
        assert_eq!(seat.tactic_tokens, tactic - 1);
        assert_eq!(
            state.player(&a()).unwrap().trade_goods,
            0,
            "a gained nothing"
        );
    }

    #[test]
    fn nekro_malleon_lets_the_chosen_player_discard_a_card_for_two_trade_goods() {
        let mut state = battle(0);
        give_card(&mut state, &b(), "sabo1");
        let goods = state.player(&b()).unwrap().trade_goods;
        assert_eq!(use_malleon(&mut state, &["b", "discard"]), Some(true));
        let seat = state.player(&b()).unwrap();
        assert_eq!(seat.trade_goods, goods + 2);
        assert!(seat.action_cards.is_empty(), "the card was discarded");
        assert!(
            crate::factions::hooks_cards::has_staged(&state),
            "the discard is staged for its window"
        );
    }

    #[test]
    fn nekro_malleon_may_be_declined_by_the_chosen_player() {
        let mut state = battle(0);
        give_card(&mut state, &b(), "sabo1");
        let before = state.clone();
        assert_eq!(use_malleon(&mut state, &["b", "decline"]), Some(true));
        assert_eq!(state, before, "the agent resolved and nothing else changed");
    }

    #[test]
    fn nekro_malleon_is_not_offered_when_nobody_can_pay() {
        let mut state = battle(0);
        for seat in &mut state.players {
            seat.tactic_tokens = 0;
            seat.fleet_tokens = 0;
            seat.strategic_tokens = 0;
            seat.action_cards.clear();
        }
        assert_eq!(
            leader_action(&state, ContentStore::embedded(), &a(), &agent()),
            Some(false)
        );
        let before = state.clone();
        assert_eq!(use_malleon(&mut state, &[]), Some(false));
        assert_eq!(state, before);
        assert_eq!(
            leader_action(
                &state,
                ContentStore::embedded(),
                &a(),
                &LeaderId::new("sardakkagent")
            ),
            None
        );
    }

    #[test]
    fn nekro_malleon_may_choose_its_own_owner() {
        let mut state = battle(0);
        let goods = state.player(&a()).unwrap().trade_goods;
        assert_eq!(use_malleon(&mut state, &["a", "token|fleet"]), Some(true));
        assert_eq!(state.player(&a()).unwrap().trade_goods, goods + 2);
    }

    // -- UNIT.DSGN.FLAYESH ------------------------------------------------------------------------

    fn hero() -> LeaderId {
        LeaderId::new(HERO)
    }

    /// A planet with a technology specialty, its system, and the colour it matches.
    fn specialty_planet() -> (SystemId, PlanetId, &'static str) {
        let content = ContentStore::embedded();
        for (id, record) in ti4_content::galaxy::all_planets(content, DEFAULT) {
            let Some(specialty) = record.tech_specialties().first().copied() else {
                continue;
            };
            let Some(tile) = record.system_id() else {
                continue;
            };
            if ti4_content::galaxy::planets_in(content, tile, DEFAULT)
                .iter()
                .filter(|planet| !planet.tech_specialties().is_empty())
                .count()
                != 1
            {
                continue;
            }
            let colour = match specialty.to_ascii_uppercase().as_str() {
                "BIOTIC" => "BIOTIC",
                "CYBERNETIC" => "CYBERNETIC",
                "PROPULSION" => "PROPULSION",
                "WARFARE" => "WARFARE",
                _ => continue,
            };
            return (SystemId::new(tile), PlanetId::new(id), colour);
        }
        panic!("the corpus has a planet with a technology specialty");
    }

    fn unlock_hero(state: &mut GameState) {
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(hero(), LeaderStatus::Unlocked);
    }

    #[test]
    fn nekro_hero_devours_a_specialty_planet_for_goods_and_a_technology() {
        let content = ContentStore::embedded();
        let (system, planet, colour) = specialty_planet();
        let mut state = game();
        unlock_hero(&mut state);
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        assert_eq!(leader_action(&state, content, &a(), &hero()), Some(true));
        let value = crate::production::planet_value_now(
            &state,
            content,
            SourceSet::all(),
            &planet,
            Spend::Resources,
        ) + crate::production::planet_value_now(
            &state,
            content,
            SourceSet::all(),
            &planet,
            Spend::Influence,
        );
        let goods = state.player(&a()).unwrap().trade_goods;
        let owned = state.player(&a()).unwrap().technologies.clone();
        let used = crate::fixtures::with_context(
            &mut state,
            SourceSet::all(),
            None,
            &mut scripted(&[]),
            |ctx| use_leader(ctx, &a(), &hero()),
        );
        assert_eq!(used, Some(true));
        let board = state.system_state(&system);
        assert!(
            board
                .on_planet(&planet)
                .iter()
                .all(|unit| unit.owner == a()),
            "every other player's unit on the planet is destroyed, structures too"
        );
        assert_eq!(
            board
                .on_planet(&planet)
                .iter()
                .filter(|unit| unit.owner == a())
                .count(),
            1,
            "the owner's own units are untouched"
        );
        let seat = state.player(&a()).unwrap();
        assert_eq!(i64::from(seat.trade_goods - goods), value);
        let gained: Vec<&TechnologyId> = seat.technologies.difference(&owned).collect();
        assert_eq!(gained.len(), 1, "exactly 1 technology");
        assert_eq!(
            crate::technology::colour_type(content, gained[0]),
            Some(colour),
            "it matches the specialty"
        );
        assert!(
            crate::factions::hooks_ground::has_staged_events(&state),
            "the destroyed ground forces are announced"
        );
    }

    #[test]
    fn nekro_hero_needs_a_specialty_planet_in_a_system_with_its_units() {
        let content = ContentStore::embedded();
        let (system, planet, _) = specialty_planet();
        let mut state = game();
        unlock_hero(&mut state);
        // Strip every unit of a from systems that hold a specialty planet.
        for (id, board) in &mut state.board {
            if !ti4_content::galaxy::planets_in(content, id.as_str(), SourceSet::all())
                .iter()
                .any(|planet| !planet.tech_specialties().is_empty())
            {
                continue;
            }
            board.units.retain(|unit| unit.owner != a());
            for units in board.planet_units.values_mut() {
                units.retain(|unit| unit.owner != a());
            }
        }
        assert_eq!(leader_action(&state, content, &a(), &hero()), Some(false));
        let before = state.clone();
        let used = crate::fixtures::with_context(
            &mut state,
            SourceSet::all(),
            None,
            &mut scripted(&[]),
            |ctx| use_leader(ctx, &a(), &hero()),
        );
        assert_eq!(used, Some(false));
        assert_eq!(state, before, "a refused hero changes nothing");
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        assert_eq!(leader_action(&state, content, &a(), &hero()), Some(true));
        assert!(
            devourable_planets(&state, content, SourceSet::all(), &a()).contains(&(system, planet))
        );
        assert_eq!(
            leader_action(&state, content, &a(), &LeaderId::new("sardakkhero")),
            None
        );
    }

    // -- nekroc4y ---------------------------------------------------------------------------------

    fn ship_destroyed(state: &mut GameState, table: &mut Table, unit: &str) {
        emit(
            state,
            table,
            "SHIP_DESTROYED",
            &[
                ("system", "18".into()),
                ("player", "a".into()),
                ("unit", unit.into()),
                ("last", false.into()),
                ("cause", "space_combat".into()),
                ("during_space_combat", true.into()),
            ],
        );
    }

    fn home(state: &GameState) -> SystemId {
        state.player(&a()).unwrap().home_system.clone().unwrap()
    }

    fn cruisers_at_home(state: &GameState) -> usize {
        state
            .system_state(&home(state))
            .units
            .iter()
            .filter(|unit| unit.owner == a() && unit.type_id.as_str() == "cruiser")
            .count()
    }

    #[test]
    fn nekro_null_reference_produces_the_destroyed_ship_type_at_the_home_dock() {
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new(NULL_REFERENCE));
        state.player_mut(&a()).unwrap().trade_goods = 10;
        let before = cruisers_at_home(&state);
        ship_destroyed(&mut state, &mut scripted(&[]), "cruiser");
        assert_eq!(
            cruisers_at_home(&state),
            before + 1,
            "a cruiser stands at the home dock"
        );
    }

    #[test]
    fn nekro_null_reference_is_neutral_without_the_card_the_means_or_a_ship() {
        // Without the technology.
        let mut state = game();
        state.player_mut(&a()).unwrap().trade_goods = 10;
        let before = state.clone();
        ship_destroyed(&mut state, &mut scripted(&[]), "cruiser");
        assert_eq!(state, before);

        // A destroyed ground force is not a ship.
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new(NULL_REFERENCE));
        state.player_mut(&a()).unwrap().trade_goods = 10;
        let before = state.clone();
        ship_destroyed(&mut state, &mut scripted(&[]), "infantry");
        assert_eq!(state, before);

        // Another player's ship is not "one of your ships".
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new(NULL_REFERENCE));
        state.player_mut(&a()).unwrap().trade_goods = 10;
        let before = state.clone();
        emit(
            &mut state,
            &mut scripted(&[]),
            "SHIP_DESTROYED",
            &[
                ("system", "18".into()),
                ("player", "b".into()),
                ("unit", "cruiser".into()),
            ],
        );
        assert_eq!(state, before);

        // Nothing to pay with: not offered, nothing changes.
        let mut state = game();
        crate::technology::grant(&mut state, &a(), &TechnologyId::new(NULL_REFERENCE));
        state.player_mut(&a()).unwrap().trade_goods = 0;
        for planet in state
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect::<Vec<_>>()
        {
            state.exhaust_planet(planet);
        }
        let before = state.clone();
        ship_destroyed(&mut state, &mut scripted(&[]), "cruiser");
        assert_eq!(state, before);
    }

    // -- nekroc4r ---------------------------------------------------------------------------------

    fn with_error_error(state: &mut GameState) {
        crate::technology::grant(state, &a(), &TechnologyId::new(ERROR_ERROR));
    }

    fn perform(state: &mut GameState, option: &str, answers: &[&str]) -> bool {
        let option = ChoiceOption::labelled(option, "component", option);
        crate::fixtures::with_context(state, DEFAULT, None, &mut scripted(answers), |ctx| {
            perform_component(ctx, &a(), &option)
        })
    }

    fn exhausted(state: &GameState) -> bool {
        state
            .player(&a())
            .unwrap()
            .exhausted_technologies
            .contains(&TechnologyId::new(ERROR_ERROR))
    }

    fn ids(state: &GameState) -> Vec<String> {
        component_actions(state, ContentStore::embedded(), &a())
            .into_iter()
            .map(|option| option.id)
            .collect()
    }

    fn pds_count(state: &GameState) -> usize {
        state
            .board
            .values()
            .flat_map(|board| board.planet_units.values().flatten())
            .filter(|unit| unit.owner == a() && unit.type_id.as_str() == "pds")
            .count()
    }

    #[test]
    fn nekro_error_error_offers_only_what_can_resolve() {
        let mut state = game();
        assert!(ids(&state).is_empty(), "no technology, no actions");
        with_error_error(&mut state);
        let offered = ids(&state);
        assert!(offered.contains(&C4R_PLACE.to_owned()), "{offered:?}");
        assert!(
            !offered.contains(&C4R_REPAIR.to_owned()),
            "nothing is damaged"
        );
        assert!(
            !offered.contains(&C4R_DRAW.to_owned()),
            "there is no card to discard"
        );
        give_card(&mut state, &a(), "sabo1");
        assert!(ids(&state).contains(&C4R_DRAW.to_owned()));
    }

    #[test]
    fn nekro_error_error_places_a_pds_and_exhausts() {
        let mut state = game();
        with_error_error(&mut state);
        let before = pds_count(&state);
        assert!(perform(&mut state, C4R_PLACE, &[]));
        assert_eq!(pds_count(&state), before + 1);
        assert!(exhausted(&state));
        assert!(ids(&state).is_empty(), "exhausted: no second use");
        assert!(!perform(&mut state, C4R_PLACE, &[]));
        assert_eq!(
            pds_count(&state),
            before + 1,
            "a refused action changes nothing"
        );
    }

    #[test]
    fn nekro_error_error_repairs_every_damaged_unit() {
        let mut state = battle(1);
        with_error_error(&mut state);
        for board in state.board.values_mut() {
            for unit in board
                .units
                .iter_mut()
                .chain(board.planet_units.values_mut().flatten())
                .filter(|unit| unit.owner == a())
            {
                unit.sustained_damage = true;
            }
        }
        state
            .system_mut(&system())
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), b()).sustained());
        assert!(ids(&state).contains(&C4R_REPAIR.to_owned()));
        assert!(perform(&mut state, C4R_REPAIR, &[]));
        for board in state.board.values() {
            assert!(
                board
                    .units
                    .iter()
                    .chain(board.planet_units.values().flatten())
                    .filter(|unit| unit.owner == a())
                    .all(|unit| !unit.sustained_damage),
                "all of a's units are repaired"
            );
        }
        assert!(
            state
                .system_state(&system())
                .units
                .iter()
                .any(|unit| unit.owner == b() && unit.sustained_damage),
            "another player's damage is not repaired"
        );
        assert!(exhausted(&state));
    }

    #[test]
    fn nekro_error_error_discards_a_card_to_draw_one() {
        let mut state = game();
        with_error_error(&mut state);
        give_card(&mut state, &a(), "sabo1");
        let deck = state.action_card_deck.len();
        assert!(perform(&mut state, C4R_DRAW, &[]));
        let seat = state.player(&a()).unwrap();
        assert_eq!(seat.action_cards.len(), 1, "discarded 1, drew 1");
        assert_ne!(seat.action_cards[0].as_str(), "sabo1");
        assert_eq!(state.action_card_deck.len(), deck - 1);
        assert!(crate::factions::hooks_cards::has_staged(&state));
        assert!(exhausted(&state));
    }

    // -- the real leader / component route -----------------------------------------------------

    fn route_option(state: &GameState, leader: &str) -> bool {
        crate::leaders::component_actions(state, ContentStore::embedded(), &a())
            .iter()
            .any(|option| option.id == format!("component|leader|{leader}"))
    }

    #[test]
    fn nekro_agent_is_offered_and_resolves_through_the_leader_route() {
        let mut state = battle(0);
        state
            .player_mut(&a())
            .unwrap()
            .leaders
            .insert(agent(), LeaderStatus::Readied);
        assert!(route_option(&state, AGENT), "offered through leaders");
        let goods = state.player(&b()).unwrap().trade_goods;
        let used = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&["b", "token|tactic"]),
            |ctx| crate::leaders::use_leader(ctx, &a(), &agent()),
        );
        assert!(used);
        assert_eq!(state.player(&b()).unwrap().trade_goods, goods + 2);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&agent()),
            Some(&LeaderStatus::Exhausted)
        );
        assert!(!route_option(&state, AGENT), "exhausted: not offered again");
    }

    #[test]
    fn nekro_hero_is_offered_and_resolves_through_the_leader_route() {
        let (system, planet, _) = specialty_planet();
        let mut state = game();
        unlock_hero(&mut state);
        assert!(!route_option(&state, HERO), "no specialty planet yet");
        crate::fixtures::put(&mut state, &system, "cruiser", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        assert!(route_option(&state, HERO), "offered through leaders");
        let used = crate::fixtures::with_context(
            &mut state,
            SourceSet::all(),
            None,
            &mut scripted(&[]),
            |ctx| crate::leaders::use_leader(ctx, &a(), &hero()),
        );
        assert!(used);
        assert!(
            state
                .system_state(&system)
                .on_planet(&planet)
                .iter()
                .all(|unit| unit.owner != b()),
            "the opponent's units are gone"
        );
        assert!(
            state.player(&a()).unwrap().leaders.get(&hero()) != Some(&LeaderStatus::Unlocked),
            "a used hero is purged"
        );
    }

    #[test]
    fn nekro_commander_unlocks_through_check_unlocks() {
        let mut state = game();
        technologies(&mut state, &["dxa", "gd"]);
        let unlocked = |state: &mut GameState| {
            crate::leaders::check_unlocks(
                state,
                ContentStore::embedded(),
                DEFAULT,
                None,
                &a(),
            )
            .contains(&LeaderId::new(COMMANDER))
        };
        assert!(!unlocked(&mut state), "two technologies");
        technologies(&mut state, &["dxa", "gd", "ac2"]);
        assert!(unlocked(&mut state), "three technologies");
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&LeaderId::new(COMMANDER)),
            Some(&LeaderStatus::Unlocked)
        );
    }

    #[test]
    fn nekro_error_error_is_offered_and_resolves_through_the_component_route() {
        let content = ContentStore::embedded();
        let mut state = game();
        let route = |state: &GameState| {
            crate::faction_abilities::component_actions(state, content, &a())
                .iter()
                .any(|option| option.id == C4R_PLACE)
        };
        assert!(!route(&state));
        with_error_error(&mut state);
        assert!(route(&state), "offered through the faction route");
        let before = pds_count(&state);
        let option = ChoiceOption::labelled(C4R_PLACE, "component", C4R_PLACE);
        let done = crate::fixtures::with_context(
            &mut state,
            DEFAULT,
            None,
            &mut scripted(&[]),
            |ctx| crate::faction_abilities::perform_component(ctx, &a(), &option),
        );
        assert!(done);
        assert_eq!(pds_count(&state), before + 1);
        assert!(exhausted(&state));
        assert!(!route(&state));
    }

    #[test]
    fn nekro_claims_name_what_is_implemented() {
        assert_eq!(UNITS, [FLAGSHIP, MECH]);
        assert_eq!(LEADERS, [AGENT, COMMANDER, HERO]);
        assert_eq!(TECHNOLOGIES, ["vax", "vay"]);
    }
}
