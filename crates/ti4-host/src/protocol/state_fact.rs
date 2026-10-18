//! The public facts that no decision tells: a comparison of the public state before and after a
//! batch of the game (the decisions and dice of one step of the engine).
//!
//! Units that the game destroyed without asking, the end of a combat and its winner, a planet
//! taken, an action card played, a card drawn for an exploration, an agenda resolved, an objective
//! revealed. Both hosts keep a [`PublicTally`] of the state they last compared, and after each step
//! [`observe`] gives the facts of the change; the facts that the events of the batch already
//! told (a chosen casualty, a move, a landing, a retreat, an announced card) are not told again.
//!
//! Only public state is read: the units on the board, the control of planets, the combat under
//! way, and the public logs of the state (the cards played, the cards explored, the agendas, the
//! revealed objectives). Never a hand, a deck, a secret objective or a hidden roll.

use std::collections::{BTreeMap, BTreeSet};

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::POK;
use ti4_model::id::{ObjectiveId, PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;

use super::board_delta::{BoardDelta, PublicBoard};
use super::public_fact::{CombatKind, PublicFact};
use super::server::{GameEvent, GameEventKind};

/// Where units stand: their owner, their type, the system, and the planet (`None`: space).
type Spot = (PlayerId, String, SystemId, Option<PlanetId>);

/// A ground combat under way: the system, the planet, the invader and the defender.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroundFight {
    system: SystemId,
    planet: PlanetId,
    attacker: PlayerId,
    defender: PlayerId,
}

/// The public state that the last comparison saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PublicTally {
    units: BTreeMap<Spot, u32>,
    control: BTreeMap<PlanetId, (SystemId, PlayerId)>,
    space_combat: Option<(SystemId, PlayerId, PlayerId)>,
    /// The active system: where a roll that does not name its system happened.
    active: Option<SystemId>,
    /// The seat that retreated from the space combat under way.
    retreated: Option<(SystemId, PlayerId)>,
    ground: Option<GroundFight>,
    plays: usize,
    explorations: usize,
    agendas: usize,
    objectives: Vec<ObjectiveId>,
    /// The public board in the terms of the view ([`super::board_delta`]).
    board: PublicBoard,
}

impl PublicTally {
    /// The tally of `state`.
    #[must_use]
    pub fn of(state: &GameState) -> Self {
        let mut units: BTreeMap<Spot, u32> = BTreeMap::new();
        let mut control = BTreeMap::new();
        for (system, held) in &state.board {
            for unit in &held.units {
                *units
                    .entry((
                        unit.owner.clone(),
                        unit.type_id.to_string(),
                        system.clone(),
                        None,
                    ))
                    .or_default() += 1;
            }
            for (planet, standing) in &held.planet_units {
                for unit in standing {
                    *units
                        .entry((
                            unit.owner.clone(),
                            unit.type_id.to_string(),
                            system.clone(),
                            Some(planet.clone()),
                        ))
                        .or_default() += 1;
                }
            }
            for (planet, owner) in &held.planet_control {
                control.insert(planet.clone(), (system.clone(), owner.clone()));
            }
        }
        Self {
            units,
            control,
            space_combat: state.active_space_combat.clone(),
            active: state.active_system.clone(),
            retreated: None,
            ground: ground_fight(state),
            plays: state.action_card_plays.len(),
            explorations: state.exploration_log.len(),
            agendas: state.agenda_log.len(),
            objectives: state.revealed_objectives.clone(),
            board: PublicBoard::of(state),
        }
    }

    fn total(&self, owner: &PlayerId, unit: &str) -> u32 {
        self.units
            .iter()
            .filter(|((who, kind, _, _), _)| who == owner && kind == unit)
            .map(|(_, count)| *count)
            .sum()
    }

    /// Moves one unit (or all, with `count` `None`) of `owner` from one spot to another.
    fn shift(
        &mut self,
        owner: &PlayerId,
        unit: Option<&str>,
        from: (&SystemId, Option<&PlanetId>),
        to: (&SystemId, Option<&PlanetId>),
        count: Option<u32>,
    ) {
        let moving: Vec<(Spot, u32)> = self
            .units
            .iter()
            .filter(|((who, kind, system, planet), _)| {
                who == owner
                    && unit.is_none_or(|unit| kind == unit)
                    && system == from.0
                    && planet.as_ref() == from.1
            })
            .map(|(spot, held)| (spot.clone(), *held))
            .collect();
        for (spot, held) in moving {
            let taken = count.map_or(held, |count| count.min(held));
            if taken == 0 {
                continue;
            }
            if let Some(left) = self.units.get_mut(&spot) {
                *left -= taken;
            }
            *self
                .units
                .entry((spot.0, spot.1, to.0.clone(), to.1.cloned()))
                .or_default() += taken;
            if count.is_some() {
                return;
            }
        }
    }

    fn system_of(&self, planet: &PlanetId, state: &GameState) -> Option<SystemId> {
        self.control
            .get(planet)
            .map(|(system, _)| system.clone())
            .or_else(|| {
                state
                    .board
                    .iter()
                    .find(|(_, held)| {
                        held.planet_units.contains_key(planet)
                            || held.planet_control.contains_key(planet)
                    })
                    .map(|(system, _)| system.clone())
            })
            .or_else(|| {
                self.units
                    .keys()
                    .find(|(_, _, _, at)| at.as_ref() == Some(planet))
                    .map(|(_, _, system, _)| system.clone())
            })
    }
}

/// The ground combat of `state` under way.
fn ground_fight(state: &GameState) -> Option<GroundFight> {
    let active = state.active_invasion.as_ref()?;
    (active.phase == "ground_battle").then_some(())?;
    Some(GroundFight {
        system: active.system.clone(),
        planet: active.planet.clone()?,
        attacker: active.invader.clone(),
        defender: active.defender.clone()?,
    })
}

/// One fact of the state, with the seat it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub actor: Option<PlayerId>,
    pub fact: PublicFact,
}

/// What the events of a batch already told.
#[derive(Default)]
struct Told {
    /// Combat systems: a combat roll, a casualty or a retreat there.
    systems: BTreeSet<SystemId>,
    /// Rollers of the space combat dice, by system, in the order they rolled.
    space_rollers: BTreeMap<SystemId, Vec<PlayerId>>,
    /// Rollers of the ground combat dice, by system.
    ground_rollers: BTreeMap<SystemId, Vec<PlayerId>>,
    /// Chosen losses: seat, unit, system, planet.
    losses: Vec<(PlayerId, String, Option<String>, Option<String>)>,
    /// Units produced, by seat and unit.
    produced: BTreeMap<(PlayerId, String), u32>,
    /// Cards a decision told as played, by seat.
    played: Vec<(PlayerId, String)>,
}

/// Roll reasons that destroy units of a combat (a gravity rift tells its own loss).
fn is_combat_roll(reason: &str) -> bool {
    !matches!(reason, "gravity rift" | "gravity_rift")
}

/// Reads the events of the batch, and moves the units that a move, a landing, a load or a
/// retreat took from one place to another in `before`, so that what is left is a loss.
fn read_batch(batch: &[GameEvent], before: &mut PublicTally, state: &GameState) -> Told {
    let mut told = Told::default();
    for event in batch {
        if let Some(roll) = event.roll.as_ref()
            && let Some(system) = roll
                .system
                .as_deref()
                .map(SystemId::new)
                .or_else(|| state.active_system.clone())
                .or_else(|| before.active.clone())
            && is_combat_roll(&roll.reason)
        {
            told.systems.insert(system.clone());
            if let Some(roller) = roll.roller.clone() {
                let rollers = match roll.reason.as_str() {
                    "space combat" => Some(&mut told.space_rollers),
                    "ground combat" => Some(&mut told.ground_rollers),
                    _ => None,
                };
                if let Some(rollers) = rollers {
                    let seats = rollers.entry(system).or_default();
                    if !seats.contains(&roller) {
                        seats.push(roller);
                    }
                }
            }
        }
        if !matches!(event.event, GameEventKind::DecisionResolved) {
            continue;
        }
        let (Some(fact), Some(actor)) = (event.fact.as_ref(), event.actor.as_ref()) else {
            continue;
        };
        match fact {
            PublicFact::Casualty {
                seat,
                unit,
                system,
                planet,
                ..
            } => {
                if let Some(system) = system {
                    told.systems.insert(SystemId::new(system));
                }
                told.losses
                    .push((seat.clone(), unit.clone(), system.clone(), planet.clone()));
            }
            PublicFact::UnitRemoved { unit, system } => {
                told.losses
                    .push((actor.clone(), unit.clone(), Some(system.clone()), None));
            }
            PublicFact::Sustained {
                system: Some(system),
                ..
            }
            | PublicFact::SustainDeclined {
                system: Some(system),
                ..
            }
            | PublicFact::RetreatAnnounced { system, .. } => {
                told.systems.insert(SystemId::new(system));
            }
            PublicFact::Produced { unit, count, .. } => {
                *told
                    .produced
                    .entry((actor.clone(), unit.clone()))
                    .or_default() += count;
            }
            PublicFact::CardPlayed { card } => told.played.push((actor.clone(), card.clone())),
            PublicFact::Moved { unit, from, to } => before.shift(
                actor,
                Some(unit),
                (&SystemId::new(from), None),
                (&SystemId::new(to), None),
                Some(1),
            ),
            PublicFact::Landed { unit, planet, .. } => {
                let planet = PlanetId::new(planet);
                if let Some(system) = before.system_of(&planet, state) {
                    before.shift(
                        actor,
                        Some(unit),
                        (&system, None),
                        (&system, Some(&planet)),
                        Some(1),
                    );
                }
            }
            PublicFact::Loaded {
                unit,
                system: Some(system),
                planet: Some(planet),
            } => {
                let system = SystemId::new(system);
                before.shift(
                    actor,
                    Some(unit),
                    (&system, Some(&PlanetId::new(planet))),
                    (&system, None),
                    Some(1),
                );
            }
            PublicFact::Retreated { seat, from, to } => {
                let from = SystemId::new(from);
                told.systems.insert(from.clone());
                before.shift(seat, None, (&from, None), (&SystemId::new(to), None), None);
                before.retreated = Some((from, seat.clone()));
            }
            _ => {}
        }
    }
    told
}

/// The units destroyed in the combat systems of the batch that no decision told.
fn destroyed(
    before: &PublicTally,
    after: &PublicTally,
    systems: &BTreeSet<SystemId>,
    told: &mut Told,
) -> Vec<Observed> {
    // What each owner and type lost on the whole board: a unit that moved is still there.
    let mut budget: BTreeMap<(PlayerId, String), u32> = BTreeMap::new();
    // What each owner gained in a place: a unit that replaced another there (a deployed mech
    // for an infantry) is no loss.
    let mut gained: BTreeMap<(PlayerId, SystemId, Option<PlanetId>), u32> = BTreeMap::new();
    for ((owner, unit, system, planet), now) in &after.units {
        if !systems.contains(system) {
            continue;
        }
        let held = before
            .units
            .get(&(owner.clone(), unit.clone(), system.clone(), planet.clone()))
            .copied()
            .unwrap_or(0);
        if *now > held {
            *gained
                .entry((owner.clone(), system.clone(), planet.clone()))
                .or_default() += now - held;
        }
    }
    let mut facts = Vec::new();
    for ((owner, unit, system, planet), held) in &before.units {
        if !systems.contains(system) {
            continue;
        }
        let now = after
            .units
            .get(&(owner.clone(), unit.clone(), system.clone(), planet.clone()))
            .copied()
            .unwrap_or(0);
        let mut lost = held.saturating_sub(now);
        if let Some(replaced) = gained.get_mut(&(owner.clone(), system.clone(), planet.clone())) {
            let offset = lost.min(*replaced);
            lost -= offset;
            *replaced -= offset;
        }
        if lost == 0 {
            continue;
        }
        let key = (owner.clone(), unit.clone());
        let left = budget.entry(key.clone()).or_insert_with(|| {
            let produced = told.produced.get(&key).copied().unwrap_or(0);
            (before.total(owner, unit) + produced).saturating_sub(after.total(owner, unit))
        });
        let mut count = lost.min(*left);
        *left -= count;
        // A loss that a decision chose is told by its decision.
        while count > 0 {
            let place = |loss: &(PlayerId, String, Option<String>, Option<String>)| {
                &loss.0 == owner
                    && &loss.1 == unit
                    && loss.2.as_deref().is_none_or(|at| at == system.as_str())
                    && loss
                        .3
                        .as_deref()
                        .is_none_or(|at| Some(at) == planet.as_ref().map(PlanetId::as_str))
            };
            let Some(index) = told.losses.iter().position(place) else {
                break;
            };
            told.losses.remove(index);
            count -= 1;
        }
        if count > 0 {
            facts.push(Observed {
                actor: Some(owner.clone()),
                fact: PublicFact::Destroyed {
                    seat: owner.clone(),
                    unit: unit.clone(),
                    count,
                    system: system.to_string(),
                    planet: planet.as_ref().map(ToString::to_string),
                },
            });
        }
    }
    facts
}

fn has_ships(
    tally: &PublicTally,
    types: &BTreeMap<&str, UnitType<'_>>,
    seat: &PlayerId,
    system: &SystemId,
) -> bool {
    tally
        .units
        .iter()
        .any(|((owner, unit, at, planet), count)| {
            owner == seat
                && at == system
                && planet.is_none()
                && *count > 0
                && types.get(unit.as_str()).is_some_and(UnitType::is_ship)
        })
}

fn has_ground_forces(
    tally: &PublicTally,
    types: &BTreeMap<&str, UnitType<'_>>,
    seat: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    tally.units.iter().any(|((owner, unit, at, on), count)| {
        owner == seat
            && at == system
            && on.as_ref() == Some(planet)
            && *count > 0
            && types
                .get(unit.as_str())
                .is_some_and(UnitType::is_ground_force)
    })
}

/// The side that is left, when exactly one is.
fn survivor(attacker: &PlayerId, defender: &PlayerId, alive: (bool, bool)) -> Option<PlayerId> {
    match alive {
        (true, false) => Some(attacker.clone()),
        (false, true) => Some(defender.clone()),
        _ => None,
    }
}

/// The two sides of a fight that only the dice of the batch show: the active seat attacks.
fn sides(rollers: &[PlayerId], active: Option<&PlayerId>) -> Option<(PlayerId, PlayerId)> {
    let attacker = active
        .filter(|seat| rollers.contains(seat))
        .or_else(|| rollers.first())?
        .clone();
    let defender = rollers.iter().find(|seat| **seat != attacker)?.clone();
    Some((attacker, defender))
}

/// The space and ground combats that ended in the batch.
fn combats(
    before: &PublicTally,
    after: &PublicTally,
    state: &GameState,
    told: &Told,
    destroyed: &[Observed],
) -> Vec<Observed> {
    let types = catalogue(ContentStore::embedded(), POK);
    let mut facts = Vec::new();
    // Space: the combat under way before that is no longer, then one that the dice show began
    // and ended in the batch.
    let mut fights: Vec<(SystemId, PlayerId, PlayerId)> = Vec::new();
    if let Some(fight) = before.space_combat.clone()
        && after.space_combat.as_ref() != Some(&fight)
    {
        fights.push(fight);
    }
    for (system, rollers) in &told.space_rollers {
        if fights.iter().any(|(at, _, _)| at == system)
            || after
                .space_combat
                .as_ref()
                .is_some_and(|(at, _, _)| at == system)
        {
            continue;
        }
        if let Some((attacker, defender)) = sides(rollers, state.active.as_ref()) {
            fights.push((system.clone(), attacker, defender));
        }
    }
    for (system, attacker, defender) in fights {
        let alive = (
            has_ships(after, &types, &attacker, &system),
            has_ships(after, &types, &defender, &system),
        );
        let retreated = before
            .retreated
            .as_ref()
            .filter(|(at, _)| *at == system)
            .map(|(_, seat)| seat.clone());
        facts.push(Observed {
            actor: Some(attacker.clone()),
            fact: PublicFact::CombatEnded {
                combat: CombatKind::Space,
                system: system.to_string(),
                planet: None,
                winner: survivor(&attacker, &defender, alive),
                attacker,
                defender,
                retreated,
            },
        });
    }
    // Ground: the fight under way before that is no longer, then the planets of a system with
    // ground dice where a roller lost ground forces or a seat took control.
    let mut fights: Vec<GroundFight> = Vec::new();
    if let Some(fight) = before.ground.clone()
        && after.ground.as_ref() != Some(&fight)
    {
        fights.push(fight);
    }
    for (system, rollers) in &told.ground_rollers {
        let Some((attacker, defender)) = sides(rollers, state.active.as_ref()) else {
            continue;
        };
        let mut planets: BTreeSet<PlanetId> = destroyed
            .iter()
            .filter_map(|observed| match &observed.fact {
                PublicFact::Destroyed {
                    seat,
                    system: at,
                    planet: Some(planet),
                    ..
                } if at == system.as_str() && rollers.contains(seat) => Some(PlanetId::new(planet)),
                _ => None,
            })
            .collect();
        planets.extend(
            after
                .control
                .iter()
                .filter(|(planet, (at, owner))| {
                    at == system && before.control.get(*planet).map(|(_, held)| held) != Some(owner)
                })
                .map(|(planet, _)| planet.clone()),
        );
        // A fight needs ground forces of the defender there before (a landing on a planet
        // that only had structures takes it without a fight).
        planets.retain(|planet| has_ground_forces(before, &types, &defender, system, planet));
        for planet in planets {
            if fights
                .iter()
                .any(|fight| fight.system == *system && fight.planet == planet)
                || after
                    .ground
                    .as_ref()
                    .is_some_and(|fight| fight.system == *system && fight.planet == planet)
            {
                continue;
            }
            fights.push(GroundFight {
                system: system.clone(),
                planet,
                attacker: attacker.clone(),
                defender: defender.clone(),
            });
        }
    }
    for fight in fights {
        let alive = (
            has_ground_forces(after, &types, &fight.attacker, &fight.system, &fight.planet),
            has_ground_forces(after, &types, &fight.defender, &fight.system, &fight.planet),
        );
        facts.push(Observed {
            actor: Some(fight.attacker.clone()),
            fact: PublicFact::CombatEnded {
                combat: CombatKind::Ground,
                system: fight.system.to_string(),
                planet: Some(fight.planet.to_string()),
                winner: survivor(&fight.attacker, &fight.defender, alive),
                attacker: fight.attacker,
                defender: fight.defender,
                retreated: None,
            },
        });
    }
    facts
}

/// What a step changed: the facts that no event of it told, and the change of the public board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub facts: Vec<Observed>,
    /// `None` when the step changed nothing of the board.
    pub board: Option<BoardDelta>,
}

/// [`observe`], and the change of the public board from `tally` to `state`
/// ([`super::board_delta`]). `tally` becomes the tally of `state`.
#[must_use]
pub fn observe_step(
    tally: &mut PublicTally,
    state: &GameState,
    batch: &[GameEvent],
) -> Observation {
    let before = std::mem::take(&mut tally.board);
    let facts = observe(tally, state, batch);
    Observation {
        facts,
        board: tally.board.delta_from(&before),
    }
}

/// The facts of the change from `tally` to `state`, the events of the batch being `batch` (the
/// events recorded since the last comparison). `tally` becomes the tally of `state`.
#[must_use]
pub fn observe(tally: &mut PublicTally, state: &GameState, batch: &[GameEvent]) -> Vec<Observed> {
    let mut before = std::mem::replace(tally, PublicTally::of(state));
    let after = &*tally;
    let mut told = read_batch(batch, &mut before, state);
    let mut facts = Vec::new();

    // The cards played, but not those a decision told.
    for (seat, card) in state.action_card_plays.iter().skip(before.plays) {
        if let Some(index) = told
            .played
            .iter()
            .position(|(who, played)| who == seat && played == card.as_str())
        {
            told.played.remove(index);
            continue;
        }
        facts.push(Observed {
            actor: Some(seat.clone()),
            fact: PublicFact::CardPlayed {
                card: card.to_string(),
            },
        });
    }

    let mut systems = told.systems.clone();
    for (system, _, _) in [&before.space_combat, &after.space_combat]
        .into_iter()
        .flatten()
    {
        systems.insert(system.clone());
    }
    for fight in [&before.ground, &after.ground].into_iter().flatten() {
        systems.insert(fight.system.clone());
    }
    if let Some(invasion) = state.active_invasion.as_ref() {
        systems.insert(invasion.system.clone());
    }
    // A retreat that no decision told (the only system to retreat to): the ships of a side
    // left the combat and are still on the board.
    if before.retreated.is_none()
        && let Some((system, attacker, defender)) = before.space_combat.clone()
    {
        let types = catalogue(ContentStore::embedded(), POK);
        let left = |seat: &PlayerId| {
            !has_ships(after, &types, seat, &system)
                && before
                    .units
                    .iter()
                    .any(|((owner, unit, at, planet), held)| {
                        owner == seat
                            && *at == system
                            && planet.is_none()
                            && *held > 0
                            && types.get(unit.as_str()).is_some_and(UnitType::is_ship)
                            && after.total(seat, unit) >= before.total(seat, unit)
                    })
        };
        before.retreated = [defender, attacker]
            .into_iter()
            .find(|seat| left(seat))
            .map(|seat| (system.clone(), seat));
    }
    let lost = destroyed(&before, after, &systems, &mut told);
    let ended = combats(&before, after, state, &told, &lost);
    facts.extend(lost);
    facts.extend(ended);
    // A retreat is told with the end of its combat.
    if let Some((system, seat)) = before.retreated.take()
        && after
            .space_combat
            .as_ref()
            .is_some_and(|(at, _, _)| *at == system)
    {
        tally.retreated = Some((system, seat));
    }
    let after = &*tally;

    for (planet, (system, seat)) in &after.control {
        let from = before.control.get(planet).map(|(_, held)| held);
        if from == Some(seat) {
            continue;
        }
        facts.push(Observed {
            actor: Some(seat.clone()),
            fact: PublicFact::ControlGained {
                seat: seat.clone(),
                planet: planet.to_string(),
                system: system.to_string(),
                from: from.cloned(),
            },
        });
    }

    for record in state.exploration_log.iter().skip(before.explorations) {
        facts.push(Observed {
            actor: Some(record.player.clone()),
            fact: PublicFact::Explored {
                seat: record.player.clone(),
                card: record.card.clone(),
                deck: record.deck.clone(),
                planet: record.planet.as_ref().map(ToString::to_string),
                system: record.system.clone(),
            },
        });
    }

    for record in state.agenda_log.iter().skip(before.agendas) {
        facts.push(Observed {
            actor: None,
            fact: PublicFact::AgendaResolved {
                agenda: record.agenda.clone(),
                outcome: record.outcome.clone(),
                votes: record
                    .votes
                    .iter()
                    .map(|(outcome, votes)| {
                        (outcome.clone(), u32::try_from(*votes).unwrap_or_default())
                    })
                    .collect(),
                speaker_decided: record.speaker_decided,
                law: record.law,
                enacted: record.enacted,
                discarded: record.discarded,
            },
        });
    }

    for objective in &state.revealed_objectives {
        if !before.objectives.contains(objective) {
            facts.push(Observed {
                actor: None,
                fact: PublicFact::ObjectiveRevealed {
                    objective: objective.to_string(),
                },
            });
        }
    }
    facts
}

/// The fact in words, for the `detail` of its event (ids, as the other lines of the log).
#[must_use]
pub fn describe(fact: &PublicFact) -> Option<String> {
    Some(match fact {
        PublicFact::Destroyed {
            seat,
            unit,
            count,
            system,
            planet,
        } => match planet {
            Some(planet) => format!("{seat} lost {count} {unit} on {planet}"),
            None => format!("{seat} lost {count} {unit} in #{system}"),
        },
        PublicFact::CombatEnded {
            combat,
            system,
            planet,
            winner,
            ..
        } => {
            let place = planet.clone().unwrap_or_else(|| format!("#{system}"));
            let kind = match combat {
                CombatKind::Space => "space",
                CombatKind::Ground => "ground",
            };
            match winner {
                Some(winner) => format!("{winner} won the {kind} combat in {place}"),
                None => format!("the {kind} combat in {place} ended without a winner"),
            }
        }
        PublicFact::ControlGained { seat, planet, .. } => {
            format!("{seat} gained control of {planet}")
        }
        PublicFact::CardPlayed { card } => format!("{card} was played"),
        PublicFact::Explored { seat, card, .. } => format!("{seat} explored {card}"),
        PublicFact::AgendaResolved {
            agenda, outcome, ..
        } => match outcome {
            Some(outcome) => format!("{agenda} resolved: {outcome}"),
            None => format!("{agenda} resolved"),
        },
        PublicFact::ObjectiveRevealed { objective } => format!("{objective} was revealed"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::server::{EventVisibility, GameEvent, GameEventKind, PublicRoll};
    use crate::protocol::status::ViewerRole;
    use ti4_model::id::{ActionCardId, SecretObjectiveId};
    use ti4_model::state::{ActiveInvasion, AgendaRecord, ExplorationRecord, Phase};
    use ti4_model::units::Unit;

    fn p(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn sys(id: &str) -> SystemId {
        SystemId::new(id)
    }

    fn state() -> GameState {
        GameState::new(&[p("p1"), p("p2")], &[], BTreeMap::new(), None, 42)
    }

    fn space(state: &mut GameState, system: &str, seat: &str, unit: &str, count: usize) {
        let held = state.board.entry(sys(system)).or_default();
        held.units
            .retain(|u| !(u.owner == p(seat) && u.type_id.as_str() == unit));
        for _ in 0..count {
            held.units
                .push(Unit::new(ti4_model::UnitTypeId::new(unit), p(seat)));
        }
    }

    fn ground(state: &mut GameState, system: &str, planet: &str, seat: &str, unit: &str, n: usize) {
        let held = state.board.entry(sys(system)).or_default();
        let standing = held.planet_units.entry(PlanetId::new(planet)).or_default();
        standing.retain(|u| !(u.owner == p(seat) && u.type_id.as_str() == unit));
        for _ in 0..n {
            standing.push(Unit::new(ti4_model::UnitTypeId::new(unit), p(seat)));
        }
    }

    fn control(state: &mut GameState, system: &str, planet: &str, seat: &str) {
        state
            .board
            .entry(sys(system))
            .or_default()
            .planet_control
            .insert(PlanetId::new(planet), p(seat));
    }

    fn event(kind: GameEventKind, actor: Option<&str>) -> GameEvent {
        GameEvent {
            id: "e".into(),
            timestamp: String::new(),
            version: Some(1),
            visibility: EventVisibility::Public,
            event: kind,
            decision_count: Some(1),
            batch_id: None,
            batch_start_cursor: None,
            batch_end_cursor: None,
            action_id: None,
            action_start_cursor: None,
            actor: actor.map(p),
            round: None,
            phase: None,
            action_type: None,
            action_actor: None,
            stage: None,
            detail: None,
            movement: None,
            fact: None,
            seat_detail: None,
            private_detail: None,
            roll: None,
            board: None,
        }
    }

    fn decided(actor: &str, fact: PublicFact) -> GameEvent {
        GameEvent {
            fact: Some(fact),
            ..event(GameEventKind::DecisionResolved, Some(actor))
        }
    }

    fn rolled(roller: &str, reason: &str, system: &str) -> GameEvent {
        GameEvent {
            roll: Some(PublicRoll {
                index: 0,
                roller: Some(p(roller)),
                reason: reason.into(),
                label: reason.into(),
                faces: vec![9],
                hits_on: Some(9),
                hits: Some(1),
                rerolled: Vec::new(),
                system: Some(system.into()),
                outcome: None,
                subject: None,
            }),
            ..event(GameEventKind::DiceRolled, Some(roller))
        }
    }

    fn facts(before: &GameState, after: &GameState, batch: &[GameEvent]) -> Vec<PublicFact> {
        let mut tally = PublicTally::of(before);
        let observed = observe(&mut tally, after, batch);
        assert_eq!(tally, {
            let mut fresh = PublicTally::of(after);
            fresh.retreated.clone_from(&tally.retreated);
            fresh
        });
        observed.into_iter().map(|observed| observed.fact).collect()
    }

    fn lost(seat: &str, unit: &str, count: u32, system: &str, planet: Option<&str>) -> PublicFact {
        PublicFact::Destroyed {
            seat: p(seat),
            unit: unit.into(),
            count,
            system: system.into(),
            planet: planet.map(str::to_owned),
        }
    }

    #[test]
    fn a_loss_the_game_assigned_is_a_fact_and_a_chosen_one_is_told_once() {
        let mut before = state();
        space(&mut before, "22", "p1", "carrier", 2);
        space(&mut before, "22", "p1", "fighter", 3);
        space(&mut before, "22", "p2", "destroyer", 2);
        before.active_space_combat = Some((sys("22"), p("p1"), p("p2")));
        let mut after = before.clone();
        // Anti-fighter barrage took two fighters; p1 chose to lose a carrier.
        space(&mut after, "22", "p1", "fighter", 1);
        space(&mut after, "22", "p1", "carrier", 1);
        let batch = [
            rolled("p2", "anti-fighter barrage", "22"),
            decided(
                "p1",
                PublicFact::Casualty {
                    seat: p("p1"),
                    unit: "carrier".into(),
                    system: Some("22".into()),
                    planet: None,
                    damaged: false,
                },
            ),
        ];
        assert_eq!(
            facts(&before, &after, &batch),
            vec![lost("p1", "fighter", 2, "22", None)]
        );
    }

    #[test]
    fn units_that_moved_landed_or_were_replaced_are_not_destroyed() {
        let mut before = state();
        space(&mut before, "21", "p1", "carrier", 1);
        space(&mut before, "21", "p1", "infantry", 2);
        space(&mut before, "22", "p2", "destroyer", 1);
        before.active_system = Some(sys("22"));
        let mut after = before.clone();
        space(&mut after, "21", "p1", "carrier", 0);
        space(&mut after, "21", "p1", "infantry", 0);
        space(&mut after, "22", "p1", "carrier", 1);
        // One infantry landed, the other was replaced by a mech (a deploy).
        ground(&mut after, "22", "lodor", "p1", "mech", 1);
        ground(&mut after, "22", "lodor", "p1", "infantry", 1);
        let batch = [
            decided(
                "p1",
                PublicFact::Moved {
                    unit: "carrier".into(),
                    from: "21".into(),
                    to: "22".into(),
                },
            ),
            rolled("p2", "space cannon", "22"),
            decided(
                "p1",
                PublicFact::Landed {
                    unit: "infantry".into(),
                    planet: "lodor".into(),
                    damaged: false,
                },
            ),
        ];
        assert_eq!(facts(&before, &after, &batch), Vec::new());
        // Outside a system of a fight nothing is a loss.
        let mut gone = before.clone();
        space(&mut gone, "21", "p1", "infantry", 0);
        assert_eq!(facts(&before, &gone, &[]), Vec::new());
    }

    #[test]
    fn the_end_of_a_space_combat_names_its_winner_and_a_retreat() {
        let mut before = state();
        space(&mut before, "22", "p1", "dreadnought", 1);
        space(&mut before, "22", "p2", "cruiser", 2);
        before.active_space_combat = Some((sys("22"), p("p1"), p("p2")));
        // The cruisers are destroyed: p1 wins.
        let mut won = before.clone();
        space(&mut won, "22", "p2", "cruiser", 0);
        won.active_space_combat = None;
        assert_eq!(
            facts(&before, &won, &[rolled("p1", "space combat", "22")]),
            vec![
                lost("p2", "cruiser", 2, "22", None),
                PublicFact::CombatEnded {
                    combat: CombatKind::Space,
                    system: "22".into(),
                    planet: None,
                    attacker: p("p1"),
                    defender: p("p2"),
                    winner: Some(p("p1")),
                    retreated: None,
                },
            ]
        );
        // The cruisers retreat with no decision (one system to go to), and the combat ends at
        // a later step: the retreat is no loss, and the end says it.
        let mut fled = before.clone();
        space(&mut fled, "22", "p2", "cruiser", 0);
        space(&mut fled, "23", "p2", "cruiser", 2);
        let mut tally = PublicTally::of(&before);
        assert_eq!(observe(&mut tally, &fled, &[]), Vec::new());
        let mut over = fled.clone();
        over.active_space_combat = None;
        let ended: Vec<_> = observe(&mut tally, &over, &[])
            .into_iter()
            .map(|observed| observed.fact)
            .collect();
        assert_eq!(
            ended,
            vec![PublicFact::CombatEnded {
                combat: CombatKind::Space,
                system: "22".into(),
                planet: None,
                attacker: p("p1"),
                defender: p("p2"),
                winner: Some(p("p1")),
                retreated: Some(p("p2")),
            }]
        );
        // Both sides destroyed: no winner. A fight that the dice show began and ended in one
        // step is found by its rollers.
        let mut start = before.clone();
        start.active_space_combat = None;
        start.active = Some(p("p1"));
        let mut wiped = start.clone();
        space(&mut wiped, "22", "p1", "dreadnought", 0);
        space(&mut wiped, "22", "p2", "cruiser", 0);
        let batch = [
            rolled("p1", "space combat", "22"),
            rolled("p2", "space combat", "22"),
        ];
        assert_eq!(
            facts(&start, &wiped, &batch).last(),
            Some(&PublicFact::CombatEnded {
                combat: CombatKind::Space,
                system: "22".into(),
                planet: None,
                attacker: p("p1"),
                defender: p("p2"),
                winner: None,
                retreated: None,
            })
        );
    }

    #[test]
    fn a_ground_combat_ends_with_its_winner_and_the_planet_changes_hands() {
        let mut before = state();
        ground(&mut before, "26", "lodor", "p1", "infantry", 2);
        ground(&mut before, "26", "lodor", "p2", "infantry", 1);
        ground(&mut before, "26", "lodor", "p2", "pds", 1);
        control(&mut before, "26", "lodor", "p2");
        before.active_invasion = Some(ActiveInvasion {
            system: sys("26"),
            invader: p("p1"),
            seq: 1,
            phase: "ground_battle".into(),
            planet: Some(PlanetId::new("lodor")),
            defender: Some(p("p2")),
            ground_round: 1,
            last_step: None,
        });
        let mut after = before.clone();
        ground(&mut after, "26", "lodor", "p2", "infantry", 0);
        ground(&mut after, "26", "lodor", "p2", "pds", 0);
        control(&mut after, "26", "lodor", "p1");
        after.active_invasion = None;
        let batch = [
            rolled("p1", "ground combat", "26"),
            rolled("p2", "ground combat", "26"),
        ];
        assert_eq!(
            facts(&before, &after, &batch),
            vec![
                lost("p2", "infantry", 1, "26", Some("lodor")),
                lost("p2", "pds", 1, "26", Some("lodor")),
                PublicFact::CombatEnded {
                    combat: CombatKind::Ground,
                    system: "26".into(),
                    planet: Some("lodor".into()),
                    attacker: p("p1"),
                    defender: p("p2"),
                    winner: Some(p("p1")),
                    retreated: None,
                },
                PublicFact::ControlGained {
                    seat: p("p1"),
                    planet: "lodor".into(),
                    system: "26".into(),
                    from: Some(p("p2")),
                },
            ]
        );
    }

    #[test]
    fn the_public_logs_of_the_state_are_facts_once() {
        let before = state();
        let mut after = before.clone();
        after.action_card_plays = vec![
            (p("p1"), ActionCardId::new("sabotage")),
            (p("p2"), ActionCardId::new("direct_hit")),
        ];
        after.exploration_log.push(ExplorationRecord {
            round: 1,
            player: p("p2"),
            deck: "CULTURAL".into(),
            card: "freelancers".into(),
            planet: Some(PlanetId::new("lodor")),
            outcome: "resolved".into(),
            system: None,
            result: String::new(),
            gains: BTreeMap::new(),
            placed: Vec::new(),
        });
        after.agenda_log.push(AgendaRecord {
            round: 1,
            agenda: "minister_of_peace".into(),
            name: "Minister of Peace".into(),
            outcome: Some("p1".into()),
            votes: BTreeMap::from([("p1".to_owned(), 5), ("p2".to_owned(), 3)]),
            speaker_decided: false,
            law: true,
            enacted: true,
            discarded: false,
            redirected_to: None,
            discard_reason: None,
        });
        after.revealed_objectives.push(ObjectiveId::new("corner"));
        // Sabotage was told by its decision.
        let batch = [decided(
            "p1",
            PublicFact::CardPlayed {
                card: "sabotage".into(),
            },
        )];
        let mut tally = PublicTally::of(&before);
        let observed = observe(&mut tally, &after, &batch);
        assert_eq!(
            observed,
            vec![
                Observed {
                    actor: Some(p("p2")),
                    fact: PublicFact::CardPlayed {
                        card: "direct_hit".into()
                    },
                },
                Observed {
                    actor: Some(p("p2")),
                    fact: PublicFact::Explored {
                        seat: p("p2"),
                        card: "freelancers".into(),
                        deck: "CULTURAL".into(),
                        planet: Some("lodor".into()),
                        system: None,
                    },
                },
                Observed {
                    actor: None,
                    fact: PublicFact::AgendaResolved {
                        agenda: "minister_of_peace".into(),
                        outcome: Some("p1".into()),
                        votes: BTreeMap::from([("p1".to_owned(), 5), ("p2".to_owned(), 3)]),
                        speaker_decided: false,
                        law: true,
                        enacted: true,
                        discarded: false,
                    },
                },
                Observed {
                    actor: None,
                    fact: PublicFact::ObjectiveRevealed {
                        objective: "corner".into(),
                    },
                },
            ]
        );
        // Nothing again at the next step.
        assert_eq!(observe(&mut tally, &after, &[]), Vec::new());
    }

    #[test]
    fn hidden_state_is_no_fact_and_a_fact_reaches_every_viewer() {
        let before = state();
        let mut after = before.clone();
        let seat = after.player_mut(&p("p1")).unwrap();
        seat.action_cards.push(ActionCardId::new("sabotage"));
        seat.secret_objectives
            .push(SecretObjectiveId::new("become_a_martyr"));
        after.objective_deck.reverse();
        after.agenda_deck.push("minister_of_peace".into());
        assert_eq!(facts(&before, &after, &[]), Vec::new());

        let event = crate::journal::build_state_event(
            &crate::clock::ManualClock::new(0, 0),
            "g",
            1,
            &[],
            0,
            1,
            Phase::Action,
            Some(1),
            Observed {
                actor: Some(p("p2")),
                fact: lost("p2", "fighter", 2, "22", None),
            },
        );
        assert_eq!(event.event, GameEventKind::StateChanged);
        assert_eq!(event.detail.as_deref(), Some("p2 lost 2 fighter in #22"));
        for viewer in [
            ViewerRole::Player(p("p1")),
            ViewerRole::Player(p("p2")),
            ViewerRole::Spectator,
        ] {
            let json = serde_json::to_value(event.for_viewer(&viewer).unwrap()).unwrap();
            assert_eq!(json["event"]["kind"], "state_changed");
            assert_eq!(
                json["fact"],
                serde_json::json!({
                    "kind": "destroyed", "seat": "p2", "unit": "fighter", "count": 2,
                    "system": "22",
                })
            );
        }
    }
}
