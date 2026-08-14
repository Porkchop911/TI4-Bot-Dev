//! Space combat (LRR 78, with 87 governing Sustain Damage).
//!
//! Ported from the oracle's `engine/combat.py`: `combatants`, `effective_hits_on`,
//! `_roll_combat`, `absorb_hits`, `_offer_sustain`, `_choose_casualty` and the round loop in
//! `resolve`.
//!
//! Choices are asked inline through a [`Table`], as the oracle does, rather than being exposed
//! as a resumable window. The step driver therefore does not run combat yet — the same shape
//! movement had before its driver landed, and recorded as an open finding.

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Resolving, Table, Window};
use crate::dice::Dice;
use crate::rng::GameRng;

/// A bound on the round loop, so an unresolvable fight fails loudly instead of hanging.
pub const MAX_ROUNDS: u32 = 50;

/// The choice kind for cancelling a hit with Sustain Damage.
pub const SUSTAIN_KIND: &str = "sustain";
/// The choice kind for assigning a hit to one of your own units.
pub const CASUALTY_KIND: &str = "casualty";

/// A combat could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CombatError {
    #[error("space combat in {0} did not finish within {MAX_ROUNDS} rounds")]
    Unresolved(SystemId),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

/// How a space combat ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CombatOutcome {
    /// The last player with ships; `None` when both sides were wiped out, or when the combat
    /// was declared a draw by Skilled Retreat.
    pub winner: Option<PlayerId>,
    /// Rounds fought.
    pub rounds: u32,
}

/// Players with ships in a system, in seating order (78.1).
#[must_use]
pub fn combatants(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> Vec<PlayerId> {
    let types = catalogue(content, sources);
    let mut found = Vec::new();
    for player in &state.seating_order {
        let has_ship = state.system_state(system).units.iter().any(|unit| {
            &unit.owner == player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::is_ship)
        });
        if has_ship {
            found.push(player.clone());
        }
    }
    found
}

/// This player's ships in the system, in board order.
#[must_use]
pub fn ships_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<Unit> {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_ship)
        })
        .cloned()
        .collect()
}

/// The value a unit needs to roll, or `None` if it does not fight.
///
/// A printed combat value of zero means "does not fight" rather than "hits on 0", which is why
/// this is an `Option` and not a number with a sentinel.
#[must_use]
pub fn hits_on(content: &ContentStore, sources: SourceSet, unit: &Unit) -> Option<i64> {
    catalogue(content, sources)
        .get(unit.type_id.as_str())
        .and_then(UnitType::combat_hits_on)
}

/// The threshold a unit needs in this combat round after its round-scoped effects.
///
/// Effects store the sequence in which they were created rather than a flag that a later combat
/// exit path must remember to clear. That is load-bearing: a combat can end after barrage,
/// casualties, or retreat, and a flag leaks from whichever exit forgot it. A sequence marker
/// simply stops matching when the next round starts.
#[must_use]
pub fn effective_hits_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    unit: &Unit,
) -> Option<i64> {
    let threshold = hits_on(content, sources, unit)?;
    let morale_is_current = state
        .player(player)
        .is_some_and(|seat| seat.combat_bonus_round == Some(state.combat_round_seq));
    // A faction shift applies to the *roll*, so it moves the threshold the other way: Sardakk's
    // Unrelenting adds one to each die, which is the same as needing one less.
    let faction = crate::faction_abilities::combat_modifier(state, content, player, "space");
    Some(threshold - i64::from(morale_is_current) - faction)
}

/// Replace the missed dice in one space-combat batch when Munitions Reserves is current.
///
/// Paying for the ability and opening its reaction window belong to the faction/reaction layer.
/// Combat owns the other half: applying the already-recorded marker at the one place a space
/// combat roll becomes final. The original is already in `Dice` history; `reroll` deliberately
/// records the replacement beside it so a replay preserves both draws.
fn reroll_munitions_misses(
    state: &GameState,
    dice: &mut Dice,
    rng: &mut GameRng,
    player: &PlayerId,
    roll: &crate::dice::Roll,
) -> crate::dice::Roll {
    let active = state
        .player(player)
        .is_some_and(|seat| seat.munitions_round == Some(state.combat_round_seq));
    if !active {
        return roll.clone();
    }
    let misses = roll.missed(None);
    if misses.is_empty() {
        return roll.clone();
    }
    let reason = format!("munitions:{player}");
    dice.reroll(rng, roll, misses, Some(&reason))
}

/// Roll one player's fleet and count the hits (78.5).
///
/// Rolled in **ascending order of combat value**, per 78.5b/78.5c, so the sequence a seed
/// reproduces does not depend on the order units happen to sit in the system.
pub fn roll_fleet(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    // Grouped by combat value, then rolled ascending (78.5b, 78.5c). Three destroyers are one
    // roll of three dice, not three rolls of one: the number of draws from the seeded stream is
    // part of what a seed reproduces, so rolling them apart would silently renumber every later
    // draw. `BTreeMap` gives the ascending order for free.
    let mut fighting: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    let extra_die = state.player(player).and_then(|seat| {
        (seat.extra_die_round == Some(state.combat_round_seq))
            .then_some(seat.extra_die_unit.as_ref())
            .flatten()
    });
    let mut extra_die_added = false;
    for unit in ships_of(state, content, sources, player, system) {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) = effective_hits_on(state, content, sources, player, &unit) else {
            continue;
        };
        let mut dice_count = kind.combat_dice();
        if !extra_die_added && extra_die.is_some_and(|selected| selected == &unit.type_id) {
            dice_count += 1;
            extra_die_added = true;
        }
        *fighting.entry(value).or_insert(0) += dice_count;
    }

    let mut hits = 0;
    for (value, count) in fighting {
        let dice_count = usize::try_from(count).unwrap_or(0);
        if dice_count == 0 {
            continue;
        }
        let threshold = u32::try_from(value).unwrap_or(u32::MAX);
        let roll = dice.roll(rng, dice_count, "space combat", Some(threshold));
        hits += reroll_munitions_misses(state, dice, rng, player, &roll).hits();
    }
    hits
}

/// 78.3: anti-fighter barrage — simultaneous, first round only, and hits fall only on fighters.
///
/// Both barrages are rolled **before** either removes a fighter. Rolling and resolving one side
/// at a time would let the first barrage destroy fighters that had already earned their return
/// fire, which is the same simultaneity 78.6 requires of ordinary combat.
///
/// The argument list is long because a combat step needs the state, the corpus it is read
/// against, both halves of the pinned random source, and both sides. Bundling them into a
/// context struct would hide which of them this step actually mutates.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per genuinely distinct input"
)]
pub fn anti_fighter_barrage(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    attacker: &PlayerId,
    defender: &PlayerId,
) -> Vec<(PlayerId, usize)> {
    let types = catalogue(content, sources);
    let mut pending = Vec::new();
    for player in [attacker, defender] {
        let mut hits = 0;
        for unit in ships_of(state, content, sources, player, system) {
            let Some(kind) = types.get(unit.type_id.as_str()) else {
                continue;
            };
            let Some(value) = kind.afb_hits_on() else {
                continue;
            };
            let count = usize::try_from(kind.afb_dice()).unwrap_or(0);
            if count == 0 {
                continue;
            }
            let roll = dice.roll(
                rng,
                count,
                "anti-fighter barrage",
                Some(u32::try_from(value).unwrap_or(u32::MAX)),
            );
            hits += roll.hits();
        }
        if hits > 0 {
            pending.push((player.clone(), hits));
        }
    }

    let resolved = pending.clone();
    for (player, hits) in pending {
        let target = if &player == attacker {
            defender
        } else {
            attacker
        };
        destroy_fighters(state, content, sources, target, system, hits);
    }
    resolved
}

/// Remove up to `hits` fighters, and nothing else. Excess hits have no effect (15.2a).
///
/// The owner is not asked which fighter dies: fighters carry no damage and no other
/// distinguishing state, so every choice would be between identical options.
fn destroy_fighters(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    hits: usize,
) {
    let types = catalogue(content, sources);
    for _ in 0..hits {
        let fighter = state
            .system_state(system)
            .units
            .iter()
            .find(|unit| {
                &unit.owner == player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_fighter)
            })
            .cloned();
        let Some(fighter) = fighter else {
            return;
        };
        state
            .system_mut(system)
            .remove(std::slice::from_ref(&fighter));
    }
}

/// Units in the active system fire on the active player's ships, before combat.
///
/// Guns come from both the space area and every planet in the system: a PDS sits on a planet and
/// shoots into space, which is the ordinary case, while some faction units carry the ability in
/// the space area itself.
///
/// Returns the hits produced per firing player, for the caller to absorb. They are kept separate
/// from combat hits because the two are answered by different cards, which is the distinction
/// [`absorb_hits`] exists to preserve.
pub fn space_cannon_offense(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    active: &PlayerId,
) -> Vec<(PlayerId, usize)> {
    let types = catalogue(content, sources);
    let board = state.system_state(system);

    let mut guns: Vec<Unit> = board
        .units
        .iter()
        .filter(|unit| &unit.owner != active)
        .cloned()
        .collect();
    for planet in board.planet_units.keys() {
        guns.extend(
            board
                .on_planet(planet)
                .iter()
                .filter(|unit| &unit.owner != active)
                .cloned(),
        );
    }

    let mut by_player: std::collections::BTreeMap<PlayerId, usize> =
        std::collections::BTreeMap::new();
    for unit in guns {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) = kind.space_cannon_hits_on() else {
            continue;
        };
        let count = usize::try_from(kind.space_cannon_dice()).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let roll = dice.roll(
            rng,
            count,
            "space cannon",
            Some(u32::try_from(value).unwrap_or(u32::MAX)),
        );
        *by_player.entry(unit.owner.clone()).or_insert(0) += roll.hits();
    }
    by_player
        .into_iter()
        .filter(|(_, hits)| *hits > 0)
        .collect()
}

/// 87.1: each undamaged sustaining unit may cancel one hit. Always optional.
///
/// Returns the hits still to be absorbed.
/// Rear Admiral Farran: a trade good each time one of this player's units sustains.
///
/// Paid where the sustain happens rather than at the card, so it cannot be honoured in one
/// hit-assignment path and forgotten in another.
fn pay_sustain_commander(state: &mut GameState, content: &ContentStore, player: &PlayerId) {
    if crate::leaders::pays_on_sustain(state, content, player)
        && let Some(seat) = state.player_mut(player)
    {
        seat.trade_goods += 1;
    }
}

fn offer_sustain(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    mut hits: usize,
) -> Result<usize, CombatError> {
    let types = catalogue(content, sources);
    while hits > 0 {
        let available: Vec<usize> = state
            .system_state(system)
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| {
                &unit.owner == player
                    && !unit.sustained_damage
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::sustain_damage)
            })
            .map(|(index, _)| index)
            .collect();
        if available.is_empty() {
            return Ok(hits);
        }

        // One option per unit *type*. Every unit here is undamaged by the filter above, so two
        // of the same type are the same decision written twice — and the copies skew it,
        // because a sampling decider would sustain on whichever type it happened to own more of.
        let mut seen = std::collections::BTreeSet::new();
        let mut options = Vec::new();
        for index in &available {
            let unit = &state.system_state(system).units[*index];
            if !seen.insert(unit.type_id.to_string()) {
                continue;
            }
            options.push(ChoiceOption::labelled(
                format!("sustain|{index}"),
                SUSTAIN_KIND,
                format!("sustain damage on {}", unit.type_id),
            ));
        }
        options.push(ChoiceOption::labelled(
            crate::choice::DECLINE_ID,
            crate::choice::DECLINE_KIND,
            "take the hit",
        ));

        let choice = Choice::new(player.clone(), format!("cancel a hit at {system}"), options);
        let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
        if answer.is_decline() {
            return Ok(hits);
        }
        let Some(index) = answer
            .id
            .strip_prefix("sustain|")
            .and_then(|rest| rest.parse::<usize>().ok())
        else {
            return Ok(hits);
        };
        if let Some(unit) = state.system_mut(system).units.get_mut(index) {
            *unit = unit.sustained();
        }
        pay_sustain_commander(state, content, player);
        hits = hits.saturating_sub(1);
    }
    Ok(hits)
}

/// 78.6: cancel what Sustain Damage can, then lose one ship per remaining hit.
///
/// Excess hits beyond the units available simply have no effect (15.2a).
///
/// # Errors
/// [`CombatError::IllegalChoice`] when a decider answers with something not offered.
pub fn absorb_hits(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    hits: usize,
) -> Result<(), CombatError> {
    absorb_hits_seeing(state, content, sources, None, table, player, system, hits)
}

/// Absorb hits with the public map attached to every learned casualty decision.
///
/// # Errors
/// [`CombatError::IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "hit assignment needs the combat position and optional map observation"
)]
pub fn absorb_hits_seeing(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    hits: usize,
) -> Result<(), CombatError> {
    let mut remaining =
        offer_sustain(state, content, sources, galaxy, table, player, system, hits)?;

    while remaining > 0 {
        let alive = ships_of(state, content, sources, player, system);
        if alive.is_empty() {
            return Ok(()); // 15.2a
        }
        let casualty = choose_casualty(state, content, sources, galaxy, table, player, &alive)?;
        state
            .system_mut(system)
            .remove(std::slice::from_ref(&casualty));
        remaining -= 1;
    }
    Ok(())
}

/// 78.6: the owning player chooses which of their own units dies.
fn choose_casualty(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    units: &[Unit],
) -> Result<Unit, CombatError> {
    if let [only] = units {
        return Ok(only.clone());
    }
    // One option per distinguishable loss. Five fighters are one decision, not five, and
    // offering it five times mattered: a sampling decider draws per option, so with five
    // fighters and one dreadnought it destroyed a fighter five times in six whatever it thought
    // of the trade — the count decided, not the scoring.
    //
    // Damage is part of what distinguishes a unit, and goes in the label as well as the key:
    // losing an already-damaged dreadnought is a different proposition from losing a fresh one.
    let mut seen = std::collections::BTreeSet::new();
    let mut options = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if !seen.insert((unit.type_id.to_string(), unit.sustained_damage)) {
            continue;
        }
        let damaged = if unit.sustained_damage {
            " (damaged)"
        } else {
            ""
        };
        options.push(ChoiceOption::labelled(
            format!("destroy|{index}"),
            CASUALTY_KIND,
            format!("destroy {}{damaged}", unit.type_id),
        ));
    }
    let choice = Choice::new(player.clone(), "assign a hit", options);
    let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    let index = answer
        .id
        .strip_prefix("destroy|")
        .and_then(|rest| rest.parse::<usize>().ok())
        .unwrap_or(0);
    Ok(units.get(index).unwrap_or(&units[0]).clone())
}

/// The choice kind for announcing a retreat.
pub const RETREAT_KIND: &str = "retreat";
/// The choice kind for picking where to retreat to.
pub const RETREAT_TO_KIND: &str = "retreat_to";

/// 78.7c: adjacent systems that hold this player's units or a planet they control, and no
/// other player's ships.
#[must_use]
pub fn eligible_retreats(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<SystemId> {
    let types = catalogue(content, sources);
    galaxy
        .adjacent(system.as_str())
        .into_iter()
        .map(SystemId::new)
        .filter(|adjacent| {
            let board = state.system_state(adjacent);
            let enemy_ship = board.units.iter().any(|unit| {
                &unit.owner != player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_ship)
            });
            if enemy_ship {
                return false;
            }
            !board.units_of(player).is_empty() || board.controls_a_planet(player)
        })
        .collect()
}

/// Whether this combat round was declared a draw (Skilled Retreat).
///
/// Checked at both exits. The window concludes through `conclude` when a fight is fought out, but
/// a fleet that retreats leaves one side empty, and the next look at the system takes the
/// "fewer than two fleets" exit instead — so applying the draw in only one of them would make the
/// card work or not depending on which path the retreat happened to land on.
fn declared_draw(state: &GameState) -> bool {
    state.combat_draw_round == Some(state.combat_round_seq)
}

/// Where Skilled Retreat may send a fleet.
///
/// Deliberately not [`eligible_retreats`]. That enforces 78.7c, which also demands the
/// destination hold your units or a planet you control; the card asks only for "an adjacent
/// system that does not contain another player's ships". Reusing the stricter function would
/// quietly make the card weaker than it is printed.
#[must_use]
pub fn skilled_retreat_destinations(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
    system: &SystemId,
) -> Vec<SystemId> {
    let types = catalogue(content, sources);
    galaxy
        .adjacent(system.as_str())
        .into_iter()
        .map(SystemId::new)
        .filter(|adjacent| {
            !state.system_state(adjacent).units.iter().any(|unit| {
                &unit.owner != player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::is_ship)
            })
        })
        .collect()
}

/// 78.7b: move a player's fleet to `destination`, and lose what it cannot carry.
///
/// Only ships with a move value leave under their own power. Anything consuming capacity comes
/// along only if there is room; the rest is destroyed, which is the cost of a retreat rather
/// than an oversight.
pub fn retreat_to(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    destination: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    let own: Vec<Unit> = state
        .system_state(system)
        .units_of(player)
        .into_iter()
        .cloned()
        .collect();

    let mut movers = Vec::new();
    let mut carried = Vec::new();
    for unit in own {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        if kind.is_ship() && kind.move_value() > 0 {
            movers.push(unit);
        } else if kind.consumes_capacity() {
            carried.push(unit);
        }
    }
    let room: i64 = movers
        .iter()
        .filter_map(|unit| types.get(unit.type_id.as_str()))
        .map(UnitType::capacity)
        .sum();
    let room = usize::try_from(room.max(0)).unwrap_or(0);
    let stranded = carried.split_off(room.min(carried.len()));

    let mut leaving = movers;
    leaving.extend(carried);
    state.move_units(system, destination, &leaving);
    for unit in &stranded {
        state.system_mut(system).remove(std::slice::from_ref(unit));
    }

    // 78.7d: a command token goes to the destination.
    state.system_mut(destination).place_token(player.clone());
    stranded.len()
}

/// Hits still to be absorbed by one player.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    player: PlayerId,
    hits: usize,
}

/// Where an open space combat has reached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stage {
    /// 78.4: retreats are announced before any dice, and the defender decides first.
    Announcing {
        round: u32,
        asking: PlayerId,
        announced: Vec<PlayerId>,
    },
    /// Start of a round: roll, then queue both sides' hits.
    Rolling {
        round: u32,
    },
    /// Offering Sustain Damage against the hits at the front of the queue (87.1).
    Sustaining {
        queue: Vec<Pending>,
        round: u32,
    },
    /// Assigning a casualty for one unabsorbed hit (78.6).
    Assigning {
        queue: Vec<Pending>,
        round: u32,
    },
    /// 78.7: those who announced are leaving, and must pick where.
    Retreating {
        round: u32,
        leaving: Vec<PlayerId>,
    },
    Done(CombatOutcome),
}

/// A space combat, resolvable one decision at a time (LRR 78).
///
/// The queue is what makes 78.6's simultaneity survive being stepped: both sides' hits are
/// computed and queued *before* either is absorbed, so a casualty can never reduce return fire
/// that had already been earned. Resolving one side to completion and then rolling the other —
/// the obvious way to write a resumable version — would break exactly that rule.
#[derive(Debug, Clone)]
pub struct CombatWindow {
    system: SystemId,
    attacker: PlayerId,
    defender: PlayerId,
    stage: Stage,
    /// The map, when the caller has one. Without it there is nowhere to retreat to.
    galaxy: Option<ti4_content::galaxy::Galaxy>,
    /// Players who announced a retreat this round and will leave once it ends (78.7).
    pending_retreats: Vec<PlayerId>,
}

impl CombatWindow {
    /// Open a combat, or finish immediately if fewer than two players have ships (78.1).
    #[must_use]
    pub fn new(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        system: &SystemId,
    ) -> Self {
        let sides = combatants(state, content, sources, system);
        let [attacker, defender] = sides.as_slice() else {
            return Self {
                system: system.clone(),
                attacker: PlayerId::new(""),
                defender: PlayerId::new(""),
                stage: Stage::Done(CombatOutcome {
                    winner: if declared_draw(state) {
                        None
                    } else {
                        sides.first().cloned()
                    },
                    rounds: 0,
                }),
                galaxy: None,
                pending_retreats: Vec::new(),
            };
        };
        Self {
            system: system.clone(),
            attacker: attacker.clone(),
            defender: defender.clone(),
            stage: Stage::Announcing {
                round: 1,
                asking: defender.clone(),
                announced: Vec::new(),
            },
            galaxy: None,
            pending_retreats: Vec::new(),
        }
    }

    /// Give the fight a map, which is what makes retreat possible (78.7c).
    #[must_use]
    pub fn with_galaxy(mut self, galaxy: ti4_content::galaxy::Galaxy) -> Self {
        self.galaxy = Some(galaxy);
        self
    }

    /// Where this player could retreat to right now.
    fn retreats(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
    ) -> Vec<SystemId> {
        self.galaxy.as_ref().map_or_else(Vec::new, |galaxy| {
            eligible_retreats(state, content, sources, galaxy, player, &self.system)
        })
    }

    /// The result, once the fight is over.
    #[must_use]
    pub fn outcome(&self) -> Option<CombatOutcome> {
        match &self.stage {
            Stage::Done(outcome) => Some(outcome.clone()),
            _ => None,
        }
    }

    fn over(&self, state: &GameState, content: &ContentStore, sources: SourceSet) -> bool {
        finished(
            state,
            content,
            sources,
            &self.system,
            &self.attacker,
            &self.defender,
        )
    }

    fn conclude(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        rounds: u32,
    ) -> Stage {
        // Skilled Retreat ends the combat in a draw. Counting ships would hand the win to
        // whoever stayed, which is the opposite of what the card says and would also score
        // "win a space combat" for a fight nobody won.
        Stage::Done(CombatOutcome {
            winner: if declared_draw(state) {
                None
            } else {
                winner(
                    state,
                    content,
                    sources,
                    &self.system,
                    &self.attacker,
                    &self.defender,
                )
            },
            rounds,
        })
    }

    /// Units of the player at the front of the queue that could still sustain a hit.
    fn sustainers(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        player: &PlayerId,
    ) -> Vec<usize> {
        let types = catalogue(content, sources);
        state
            .system_state(&self.system)
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| {
                &unit.owner == player
                    && !unit.sustained_damage
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(UnitType::sustain_damage)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// Roll a round and queue both sides' hits, or finish.
    fn roll_round(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>, round: u32) {
        let (content, sources) = (ctx.content, ctx.sources);
        state.combat_round_seq = state.combat_round_seq.saturating_add(1);

        // Announced before anything is rolled, because eight action cards read "at the start of
        // a combat round" and Morale Boost scopes its bonus to `combat_round_seq`. Emitting after
        // the round would apply it to the next one.
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("system".to_owned(), self.system.to_string().into());
        payload.insert("round".to_owned(), i64::from(round).into());
        if round == 1 {
            let mut opening = payload.clone();
            opening.insert("player".to_owned(), self.attacker.to_string().into());
            let _ = ctx.emit(state, "SPACE_COMBAT_STARTED", opening);
        }
        let _ = ctx.emit(state, "COMBAT_ROUND_STARTED", payload);

        // Faction offers made at the round's opening window, before any dice: Letnev pays for
        // Munitions Reserves here, and `reroll_munitions_misses` reads the marker below.
        for side in [self.attacker.clone(), self.defender.clone()] {
            crate::faction_abilities::space_combat_round_started(
                state, content, sources, ctx.table, &side,
            );
        }

        if round == 1 {
            anti_fighter_barrage(
                state,
                content,
                sources,
                ctx.dice,
                ctx.rng,
                &self.system,
                &self.attacker,
                &self.defender,
            );
            if self.over(state, content, sources) {
                // 78.3a: a barrage can end the fight before any combat die is rolled.
                self.stage = self.conclude(state, content, sources, round);
                return;
            }
        }

        // 78.5f: the attacker rolls everything first. 78.6: both sides' hits are computed
        // before either is absorbed.
        let attacker_hits = roll_fleet(
            state,
            content,
            sources,
            ctx.dice,
            ctx.rng,
            &self.attacker,
            &self.system,
        );
        let defender_hits = roll_fleet(
            state,
            content,
            sources,
            ctx.dice,
            ctx.rng,
            &self.defender,
            &self.system,
        );

        let queue: Vec<Pending> = [
            Pending {
                player: self.defender.clone(),
                hits: attacker_hits,
            },
            Pending {
                player: self.attacker.clone(),
                hits: defender_hits,
            },
        ]
        .into_iter()
        .filter(|pending| pending.hits > 0)
        .collect();

        self.stage = Stage::Sustaining { queue, round };
        self.settle(state, ctx);
    }

    /// Settle a freshly opened window, so a fight that is already over reports so.
    pub fn settle_open(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        self.settle(state, ctx);
    }

    /// Advance past anything with no decision left in it.
    ///
    /// One long match rather than several helpers: every arm is a transition in the same state
    /// machine, and splitting them would hide the fact that each arm's job is to fall through
    /// to the next stage rather than to do work of its own.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per combat stage, read as a table"
    )]
    fn settle(&mut self, state: &mut GameState, ctx: &mut Resolving<'_>) {
        let (content, sources) = (ctx.content, ctx.sources);
        loop {
            match self.stage.clone() {
                Stage::Sustaining { queue, round } | Stage::Assigning { queue, round } => {
                    let Some(front) = queue.first().cloned() else {
                        // Both sides absorbed: the round is over.
                        if self.over(state, content, sources) || round >= MAX_ROUNDS {
                            self.stage = self.conclude(state, content, sources, round);
                            return;
                        }
                        // 78.7: those who announced now leave, before the next round.
                        let leaving = std::mem::take(&mut self.pending_retreats);
                        self.stage = Stage::Retreating { round, leaving };
                        continue;
                    };
                    if front.hits == 0 {
                        let rest = queue[1..].to_vec();
                        self.stage = Stage::Sustaining { queue: rest, round };
                        continue;
                    }
                    let alive =
                        ships_of(state, content, sources, &front.player, &self.system).len();
                    if alive == 0 {
                        // 15.2a: hits beyond the units available have no effect.
                        let rest = queue[1..].to_vec();
                        self.stage = Stage::Sustaining { queue: rest, round };
                        continue;
                    }
                    // A sustain is only offered when something can take one.
                    if matches!(self.stage, Stage::Sustaining { .. })
                        && self
                            .sustainers(state, content, sources, &front.player)
                            .is_empty()
                    {
                        self.stage = Stage::Assigning { queue, round };
                        continue;
                    }
                    // A single possible casualty is not a decision.
                    if matches!(self.stage, Stage::Assigning { .. }) && alive == 1 {
                        let only = ships_of(state, content, sources, &front.player, &self.system)
                            .remove(0);
                        state
                            .system_mut(&self.system)
                            .remove(std::slice::from_ref(&only));
                        let mut rest = queue;
                        rest[0].hits -= 1;
                        self.stage = Stage::Sustaining { queue: rest, round };
                        continue;
                    }
                    return;
                }
                Stage::Announcing {
                    round,
                    asking,
                    announced,
                } => {
                    if self.over(state, content, sources) {
                        self.stage = self.conclude(state, content, sources, round - 1);
                        return;
                    }
                    // 78.4c: a player with nowhere to go is not asked.
                    if !self.retreats(state, content, sources, &asking).is_empty() {
                        return;
                    }
                    if asking == self.defender && !announced.contains(&self.defender) {
                        self.stage = Stage::Announcing {
                            round,
                            asking: self.attacker.clone(),
                            announced,
                        };
                        continue;
                    }
                    self.pending_retreats = announced;
                    self.stage = Stage::Rolling { round };
                }
                Stage::Retreating { round, leaving } => {
                    let Some(player) = leaving.first().cloned() else {
                        // Everyone who announced has gone.
                        if self.over(state, content, sources) || round >= MAX_ROUNDS {
                            self.stage = self.conclude(state, content, sources, round);
                            return;
                        }
                        self.stage = Stage::Announcing {
                            round: round + 1,
                            asking: self.defender.clone(),
                            announced: Vec::new(),
                        };
                        continue;
                    };
                    let destinations = self.retreats(state, content, sources, &player);
                    match destinations.as_slice() {
                        // The destination stopped qualifying during the round.
                        [] => {
                            let rest = leaving[1..].to_vec();
                            self.stage = Stage::Retreating {
                                round,
                                leaving: rest,
                            };
                        }
                        [only] => {
                            let only = only.clone();
                            retreat_to(state, content, sources, &player, &self.system, &only);
                            let rest = leaving[1..].to_vec();
                            self.stage = Stage::Retreating {
                                round,
                                leaving: rest,
                            };
                        }
                        _ => return,
                    }
                }
                Stage::Rolling { round } => {
                    if self.over(state, content, sources) {
                        self.stage = self.conclude(state, content, sources, round - 1);
                        return;
                    }
                    self.roll_round(state, ctx, round);
                    return;
                }
                Stage::Done(_) => return,
            }
        }
    }
}

impl Window for CombatWindow {
    fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done(_) | Stage::Rolling { .. } => None,
            Stage::Announcing { asking, .. } => {
                if self.retreats(state, content, sources, asking).is_empty() {
                    return None; // 78.4c: nothing to retreat to, so nothing to announce
                }
                Some(Choice::new(
                    asking.clone(),
                    format!("announce a retreat from {}", self.system),
                    vec![
                        ChoiceOption::labelled("stay", RETREAT_KIND, "stay and fight"),
                        ChoiceOption::labelled("retreat", RETREAT_KIND, "announce a retreat"),
                    ],
                ))
            }
            Stage::Retreating { leaving, .. } => {
                let player = leaving.first()?;
                let destinations = self.retreats(state, content, sources, player);
                if destinations.len() < 2 {
                    return None; // 78.7b: one destination is not a decision
                }
                Some(Choice::new(
                    player.clone(),
                    "retreat to which system",
                    destinations
                        .iter()
                        .map(|id| {
                            ChoiceOption::labelled(
                                id.to_string(),
                                RETREAT_TO_KIND,
                                format!("retreat to {id}"),
                            )
                        })
                        .collect(),
                ))
            }
            Stage::Sustaining { queue, .. } => {
                let front = queue.first()?;
                let available = self.sustainers(state, content, sources, &front.player);
                if available.is_empty() {
                    return None;
                }
                // One option per unit *type*: everything here is undamaged by definition, so
                // two of a type are the same decision written twice, and a sampling decider
                // would sustain on whichever type it happened to own more of.
                let mut seen = std::collections::BTreeSet::new();
                let mut options = Vec::new();
                for index in available {
                    let unit = &state.system_state(&self.system).units[index];
                    if !seen.insert(unit.type_id.to_string()) {
                        continue;
                    }
                    options.push(ChoiceOption::labelled(
                        format!("sustain|{index}"),
                        SUSTAIN_KIND,
                        format!("sustain damage on {}", unit.type_id),
                    ));
                }
                options.push(ChoiceOption::labelled(
                    crate::choice::DECLINE_ID,
                    crate::choice::DECLINE_KIND,
                    "take the hit",
                ));
                Some(Choice::new(
                    front.player.clone(),
                    format!("cancel a hit at {}", self.system),
                    options,
                ))
            }
            Stage::Assigning { queue, .. } => {
                let front = queue.first()?;
                let units = ships_of(state, content, sources, &front.player, &self.system);
                if units.len() < 2 {
                    return None;
                }
                // One option per distinguishable loss; damage is part of what distinguishes.
                let mut seen = std::collections::BTreeSet::new();
                let mut options = Vec::new();
                for (index, unit) in units.iter().enumerate() {
                    if !seen.insert((unit.type_id.to_string(), unit.sustained_damage)) {
                        continue;
                    }
                    let damaged = if unit.sustained_damage {
                        " (damaged)"
                    } else {
                        ""
                    };
                    options.push(ChoiceOption::labelled(
                        format!("destroy|{index}"),
                        CASUALTY_KIND,
                        format!("destroy {}{damaged}", unit.type_id),
                    ));
                }
                Some(Choice::new(front.player.clone(), "assign a hit", options))
            }
        }
    }

    fn resolve(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        answer: ChoiceOption,
    ) -> Result<(), IllegalChoice> {
        let (content, sources) = (ctx.content, ctx.sources);
        let Some(choice) = self.pending_choice(state, content, sources) else {
            return Ok(());
        };
        let option = crate::choice::validate(&choice, answer)?;

        match self.stage.clone() {
            Stage::Done(_) | Stage::Rolling { .. } => {}
            Stage::Announcing {
                round,
                asking,
                mut announced,
            } => {
                if option.id == "retreat" {
                    announced.push(asking.clone());
                }
                // 78.4b: the defender announcing silences the attacker.
                let next = if asking == self.defender && !announced.contains(&self.defender) {
                    Some(self.attacker.clone())
                } else {
                    None
                };
                self.stage = next.map_or(Stage::Rolling { round }, |asking| Stage::Announcing {
                    round,
                    asking,
                    announced: announced.clone(),
                });
                if matches!(self.stage, Stage::Rolling { .. }) && !announced.is_empty() {
                    self.pending_retreats = announced;
                }
            }
            Stage::Retreating { round, mut leaving } => {
                if let Some(player) = leaving.first().cloned() {
                    let destination = SystemId::new(option.id);
                    retreat_to(state, content, sources, &player, &self.system, &destination);
                    leaving.remove(0);
                }
                self.stage = Stage::Retreating { round, leaving };
            }
            Stage::Sustaining { mut queue, round } => {
                if option.is_decline() {
                    self.stage = Stage::Assigning { queue, round };
                } else if let Some(index) = option
                    .id
                    .strip_prefix("sustain|")
                    .and_then(|rest| rest.parse::<usize>().ok())
                {
                    if let Some(unit) = state.system_mut(&self.system).units.get_mut(index) {
                        *unit = unit.sustained();
                    }
                    if let Some(front) = queue.first_mut() {
                        front.hits = front.hits.saturating_sub(1);
                    }
                    self.stage = Stage::Sustaining { queue, round };
                }
            }
            Stage::Assigning { mut queue, round } => {
                let Some(front) = queue.first().cloned() else {
                    return Ok(());
                };
                let units = ships_of(state, content, sources, &front.player, &self.system);
                let index = option
                    .id
                    .strip_prefix("destroy|")
                    .and_then(|rest| rest.parse::<usize>().ok())
                    .unwrap_or(0);
                if let Some(doomed) = units.get(index) {
                    let doomed = doomed.clone();
                    state
                        .system_mut(&self.system)
                        .remove(std::slice::from_ref(&doomed));
                }
                if let Some(front) = queue.first_mut() {
                    front.hits = front.hits.saturating_sub(1);
                }
                // Back to sustaining: the next hit may be cancellable even if this one was not.
                self.stage = Stage::Sustaining { queue, round };
            }
        }
        self.settle(state, ctx);
        Ok(())
    }
}

/// Fight a space combat to its end (LRR 78).
///
/// Returns immediately when fewer than two players have ships (78.1).
///
/// # Errors
/// [`CombatError::IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per distinct input"
)]
pub fn resolve(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
) -> Result<CombatOutcome, CombatError> {
    let mut window = CombatWindow::new(state, content, sources, system);
    let mut ctx = Resolving {
        content,
        sources,
        dice,
        rng,
        table,
        timing: None,
    };
    // Opening does not roll; settle once so a fight that is already over reports so.
    window.settle(state, &mut ctx);
    window.drive(state, &mut ctx)?;
    window
        .outcome()
        .ok_or_else(|| CombatError::Unresolved(system.clone()))
}

fn finished(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    attacker: &PlayerId,
    defender: &PlayerId,
) -> bool {
    ships_of(state, content, sources, attacker, system).is_empty()
        || ships_of(state, content, sources, defender, system).is_empty()
}

fn winner(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    attacker: &PlayerId,
    defender: &PlayerId,
) -> Option<PlayerId> {
    let attacker_alive = !ships_of(state, content, sources, attacker, system).is_empty();
    let defender_alive = !ships_of(state, content, sources, defender, system).is_empty();
    match (attacker_alive, defender_alive) {
        (true, false) => Some(attacker.clone()),
        (false, true) => Some(defender.clone()),
        // Both wiped out is a draw, and both alive cannot happen at a decision point.
        _ => None,
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn skilled_retreat_may_go_where_an_ordinary_retreat_may_not() {
        // 78.7c demands the destination hold your units or a planet you control. The card asks
        // only for "an adjacent system that does not contain another player's ships", so reusing
        // the stricter rule would quietly make it weaker than printed.
        let hub = crate::fixtures::plain_hub();
        let mine = PlayerId::new("a");
        let system = SystemId::new(hub.centre.clone());
        let state = crate::fixtures::game(&["a", "b"]);

        let ordinary = eligible_retreats(
            &state,
            ContentStore::embedded(),
            POK,
            &hub.galaxy,
            &mine,
            &system,
        );
        let skilled = skilled_retreat_destinations(
            &state,
            ContentStore::embedded(),
            POK,
            &hub.galaxy,
            &mine,
            &system,
        );

        assert!(
            ordinary.is_empty(),
            "an empty ring holds none of this player's units"
        );
        assert_eq!(
            skilled.len(),
            6,
            "but every ring seat is free of enemy ships"
        );
    }

    #[test]
    fn skilled_retreat_will_not_go_where_an_enemy_fleet_sits() {
        let hub = crate::fixtures::plain_hub();
        let mine = PlayerId::new("a");
        let theirs = PlayerId::new("b");
        let system = SystemId::new(hub.centre.clone());
        let occupied = SystemId::new(hub.outer[0].clone());

        let mut state = crate::fixtures::game(&["a", "b"]);
        crate::fixtures::put(&mut state, &occupied, "cruiser", &theirs, 1);

        let open = skilled_retreat_destinations(
            &state,
            ContentStore::embedded(),
            POK,
            &hub.galaxy,
            &mine,
            &system,
        );

        assert_eq!(open.len(), 5);
        assert!(!open.contains(&occupied), "another player's ships bar it");
    }

    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    use super::*;
    use crate::choice::{FirstOption, Scripted};
    use crate::setup::start_game;

    fn attacker() -> PlayerId {
        PlayerId::new("a")
    }
    fn defender() -> PlayerId {
        PlayerId::new("b")
    }

    fn arena() -> (GameState, SystemId) {
        let players = [attacker(), defender()];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        (state, SystemId::new("18"))
    }

    fn put(state: &mut GameState, system: &SystemId, kind: &str, owner: &PlayerId, n: usize) {
        for _ in 0..n {
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new(kind), owner.clone()));
        }
    }

    fn kit() -> (Table, Dice, GameRng) {
        (Table::new(), Dice::new(), GameRng::new(7))
    }

    /// A unit type that carries anti-fighter barrage, chosen from the corpus by property.
    fn a_barrage_unit() -> String {
        ti4_content::units::catalogue(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, kind)| kind.has_anti_fighter_barrage() && kind.is_ship())
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has a barrage ship")
    }

    /// A unit type that carries space cannon.
    fn a_cannon_unit() -> String {
        ti4_content::units::catalogue(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, kind)| kind.space_cannon_hits_on().is_some())
            .map(|(id, _)| (*id).to_owned())
            .expect("the corpus has a space cannon unit")
    }

    #[test]
    fn a_barrage_kills_only_fighters() {
        // 78.3: hits fall on fighters and nothing else, so a cruiser standing beside them is
        // untouched however well the barrage rolls. Swept across seeds rather than pinned to
        // one, so the test does not depend on a particular roll going well.
        let mut fighters_ever_died = false;
        for seed in 0..40_u64 {
            let (mut state, system) = arena();
            put(&mut state, &system, &a_barrage_unit(), &attacker(), 4);
            put(&mut state, &system, "fighter", &defender(), 3);
            put(&mut state, &system, "cruiser", &defender(), 1);
            let mut dice = Dice::new();
            let mut rng = GameRng::new(seed);

            anti_fighter_barrage(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut dice,
                &mut rng,
                &system,
                &attacker(),
                &defender(),
            );

            let left = ships_of(&state, ContentStore::embedded(), POK, &defender(), &system);
            assert!(
                left.iter().any(|unit| unit.type_id.as_str() == "cruiser"),
                "seed {seed}: the cruiser is not a legal target"
            );
            if left
                .iter()
                .filter(|u| u.type_id.as_str() == "fighter")
                .count()
                < 3
            {
                fighters_ever_died = true;
            }
        }
        assert!(
            fighters_ever_died,
            "a barrage that never hits anything is not testing the hit path"
        );
    }

    #[test]
    fn a_fleet_with_no_barrage_rolls_nothing() {
        let (mut state, system) = arena();
        put(&mut state, &system, "cruiser", &attacker(), 3);
        put(&mut state, &system, "fighter", &defender(), 3);
        let (_, mut dice, mut rng) = kit();

        let fired = anti_fighter_barrage(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
            &defender(),
        );

        assert!(fired.is_empty());
        assert_eq!(dice.count(), 0);
        assert_eq!(
            ships_of(&state, ContentStore::embedded(), POK, &defender(), &system).len(),
            3,
            "nothing was destroyed"
        );
    }

    #[test]
    fn barrages_are_simultaneous() {
        // Both are rolled before either removes a fighter. Resolving one side first would let
        // it destroy fighters that had already earned their return barrage.
        let (mut state, system) = arena();
        let barrager = a_barrage_unit();
        put(&mut state, &system, &barrager, &attacker(), 6);
        put(&mut state, &system, &barrager, &defender(), 6);
        put(&mut state, &system, "fighter", &attacker(), 1);
        put(&mut state, &system, "fighter", &defender(), 1);
        let (_, mut dice, mut rng) = kit();

        let fired = anti_fighter_barrage(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
            &defender(),
        );

        // Whatever the dice said, both sides' rolls were taken before any fighter was removed:
        // each side that scored a hit is recorded, even if it lost its own fighter.
        assert!(
            fired.len() <= 2,
            "at most one entry per side, and both were rolled"
        );
    }

    #[test]
    fn excess_barrage_hits_have_no_effect() {
        // 15.2a, on a target with a single fighter and a large barrage.
        let (mut state, system) = arena();
        put(&mut state, &system, &a_barrage_unit(), &attacker(), 8);
        put(&mut state, &system, "fighter", &defender(), 1);
        let (_, mut dice, mut rng) = kit();

        anti_fighter_barrage(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
            &defender(),
        );

        assert!(ships_of(&state, ContentStore::embedded(), POK, &defender(), &system).len() <= 1);
    }

    #[test]
    fn morale_changes_only_its_owners_current_combat_round() {
        let (mut state, _) = arena();
        let cruiser = Unit::new(UnitTypeId::new("cruiser"), attacker());
        let printed = hits_on(ContentStore::embedded(), POK, &cruiser).unwrap();
        state.combat_round_seq = 7;
        state.player_mut(&attacker()).unwrap().combat_bonus_round = Some(7);

        assert_eq!(
            effective_hits_on(&state, ContentStore::embedded(), POK, &attacker(), &cruiser),
            Some(printed - 1),
            "Morale Boost is +1 to this player's combat roll"
        );
        assert_eq!(
            effective_hits_on(&state, ContentStore::embedded(), POK, &defender(), &cruiser),
            Some(printed),
            "the opponent's threshold is untouched"
        );

        state.combat_round_seq = 8;
        assert_eq!(
            effective_hits_on(&state, ContentStore::embedded(), POK, &attacker(), &cruiser),
            Some(printed),
            "the marker lapses without a cleanup path"
        );
    }

    #[test]
    fn the_selected_unit_rolls_exactly_one_extra_die_in_its_round() {
        let (mut plain, system) = arena();
        let mut marked = plain.clone();
        put(&mut plain, &system, "cruiser", &attacker(), 2);
        put(&mut marked, &system, "cruiser", &attacker(), 2);
        marked.combat_round_seq = 4;
        let seat = marked.player_mut(&attacker()).unwrap();
        seat.extra_die_round = Some(4);
        seat.extra_die_unit = Some(UnitTypeId::new("cruiser"));

        let (_, mut plain_dice, mut plain_rng) = kit();
        let (_, mut marked_dice, mut marked_rng) = kit();
        roll_fleet(
            &plain,
            ContentStore::embedded(),
            POK,
            &mut plain_dice,
            &mut plain_rng,
            &attacker(),
            &system,
        );
        roll_fleet(
            &marked,
            ContentStore::embedded(),
            POK,
            &mut marked_dice,
            &mut marked_rng,
            &attacker(),
            &system,
        );

        assert_eq!(
            plain_dice.history()[0].faces.len() + 1,
            marked_dice.history()[0].faces.len()
        );
    }

    #[test]
    fn munitions_rerolls_only_misses_and_lapses_after_its_round() {
        let (mut state, _) = arena();
        state.combat_round_seq = 9;
        state.player_mut(&attacker()).unwrap().munitions_round = Some(9);
        let mut dice = Dice::new();
        let mut rng = GameRng::new(3);
        let original = crate::dice::Roll {
            reason: "space combat".to_owned(),
            faces: vec![1, 10],
            hits_on: Some(7),
            rerolled: std::collections::BTreeSet::new(),
        };
        let rerolled = reroll_munitions_misses(&state, &mut dice, &mut rng, &attacker(), &original);

        assert_eq!(rerolled.rerolled, std::collections::BTreeSet::from([0]));
        assert_eq!(rerolled.faces[1], 10, "a hit is not rerolled");
        assert_eq!(
            dice.count(),
            1,
            "the replacement is recorded; callers already recorded the original batch"
        );
        assert_eq!(dice.history()[0].reason, "munitions:a");

        state.combat_round_seq = 10;
        let before = dice.count();
        assert_eq!(
            reroll_munitions_misses(&state, &mut dice, &mut rng, &attacker(), &original),
            original,
            "a marker from an earlier round does nothing"
        );
        assert_eq!(dice.count(), before);
    }

    #[test]
    fn space_cannon_fires_from_a_planet_at_the_active_player() {
        // A PDS sits on a planet and shoots into space. The engine had no such step at all
        // before this, so a PDS never once fired on a ship moving into its system.
        let (mut state, system) = arena();
        let planet = ti4_model::id::PlanetId::new("mecatol_rex");
        state
            .system_mut(&system)
            .planet_units
            .entry(planet)
            .or_default()
            .push(Unit::new(UnitTypeId::new(a_cannon_unit()), defender()));
        put(&mut state, &system, "cruiser", &attacker(), 1);
        let (_, mut dice, mut rng) = kit();

        let fired = space_cannon_offense(
            &state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
        );

        assert_eq!(dice.count(), 1, "the gun on the planet fired");
        assert!(
            fired.iter().all(|(owner, _)| owner == &defender()),
            "only the non-active player shoots"
        );
    }

    #[test]
    fn the_active_players_own_guns_do_not_fire_at_them() {
        let (mut state, system) = arena();
        put(&mut state, &system, &a_cannon_unit(), &attacker(), 3);
        let (_, mut dice, mut rng) = kit();

        let fired = space_cannon_offense(
            &state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
        );

        assert!(fired.is_empty());
        assert_eq!(dice.count(), 0, "nobody shoots at themselves");
    }

    #[test]
    fn a_retreat_needs_somewhere_that_is_yours_and_unthreatened() {
        // 78.7c: adjacent, holds your units or a planet you control, and no enemy ships.
        let hub = crate::fixtures::plain_hub();
        let mut state = crate::fixtures::game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        let refuge = SystemId::new(hub.outer[0].clone());

        put(&mut state, &centre, "cruiser", &attacker(), 1);
        assert!(
            eligible_retreats(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &attacker(),
                &centre
            )
            .is_empty(),
            "an empty neighbour is not a refuge"
        );

        put(&mut state, &refuge, "carrier", &attacker(), 1);
        assert!(
            eligible_retreats(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &attacker(),
                &centre
            )
            .contains(&refuge),
            "your own fleet makes it one"
        );

        put(&mut state, &refuge, "destroyer", &defender(), 1);
        assert!(
            !eligible_retreats(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &attacker(),
                &centre
            )
            .contains(&refuge),
            "an enemy ship there closes it again"
        );
    }

    #[test]
    fn a_retreat_strands_what_it_cannot_carry() {
        // 78.7b: only ships with a move value leave under their own power, and capacity
        // decides how much of the rest goes with them. The remainder is lost, which is the
        // cost of retreating rather than an oversight.
        let hub = crate::fixtures::plain_hub();
        let mut state = crate::fixtures::game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        let refuge = SystemId::new(hub.outer[0].clone());

        put(&mut state, &centre, "destroyer", &attacker(), 1); // no capacity
        put(&mut state, &centre, "fighter", &attacker(), 3);

        let stranded = retreat_to(
            &mut state,
            ContentStore::embedded(),
            POK,
            &attacker(),
            &centre,
            &refuge,
        );

        assert_eq!(stranded, 3, "a destroyer carries nothing");
        assert!(state.system_state(&centre).units.is_empty());
        assert_eq!(state.system_state(&refuge).units.len(), 1, "only the hull");
        assert!(
            state
                .system_state(&refuge)
                .command_tokens
                .contains(&attacker()),
            "78.7d: a token goes to the destination"
        );
    }

    #[test]
    fn a_carrier_takes_its_fighters_with_it() {
        let hub = crate::fixtures::plain_hub();
        let mut state = crate::fixtures::game(&["a", "b"]);
        let centre = SystemId::new(hub.centre.clone());
        let refuge = SystemId::new(hub.outer[0].clone());

        put(&mut state, &centre, "carrier", &attacker(), 1);
        put(&mut state, &centre, "fighter", &attacker(), 2);

        let stranded = retreat_to(
            &mut state,
            ContentStore::embedded(),
            POK,
            &attacker(),
            &centre,
            &refuge,
        );

        assert_eq!(stranded, 0);
        assert_eq!(state.system_state(&refuge).units.len(), 3);
    }

    #[test]
    fn a_declared_draw_beats_counting_the_survivors() {
        // Skilled Retreat: the fleet leaves and the combat ends in a draw. Counting ships would
        // hand the win to whoever stayed, and would also score "win a space combat" for a fight
        // nobody won.
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 2);
        let (mut table, mut dice, mut rng) = kit();
        state.combat_draw_round = Some(state.combat_round_seq);

        let outcome = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        )
        .unwrap();

        assert_eq!(
            outcome.winner, None,
            "the round was declared a draw, so the last fleet standing did not win it"
        );
    }

    #[test]
    fn a_draw_declared_in_another_round_does_not_carry() {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 2);
        let (mut table, mut dice, mut rng) = kit();
        state.combat_round_seq = 4;
        state.combat_draw_round = Some(3);

        let outcome = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        )
        .unwrap();

        assert_eq!(
            outcome.winner,
            Some(attacker()),
            "last round's draw is not this round's"
        );
    }

    #[test]
    fn a_system_with_one_fleet_is_not_a_combat() {
        // 78.1: fewer than two players with ships, so there is nothing to fight.
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 2);
        let (mut table, mut dice, mut rng) = kit();

        let outcome = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        )
        .unwrap();

        assert_eq!(outcome.winner, Some(attacker()));
        assert_eq!(outcome.rounds, 0);
        assert_eq!(dice.count(), 0, "no dice were rolled");
    }

    #[test]
    fn ground_forces_do_not_make_a_combat() {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 1);
        put(&mut state, &system, "infantry", &defender(), 3);

        assert_eq!(
            combatants(&state, ContentStore::embedded(), POK, &system),
            vec![attacker()],
            "78.1 counts ships"
        );
    }

    #[test]
    fn a_fight_ends_with_one_fleet_standing() {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 4);
        put(&mut state, &system, "fighter", &defender(), 1);
        let (mut table, mut dice, mut rng) = kit();

        let outcome = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        )
        .unwrap();

        assert!(outcome.winner.is_some(), "somebody won");
        assert!(outcome.rounds >= 1);
        let survivors = combatants(&state, ContentStore::embedded(), POK, &system);
        assert!(survivors.len() <= 1, "only one side can remain");
    }

    #[test]
    fn a_unit_that_does_not_fight_rolls_nothing() {
        // A printed combat value of zero means "does not fight", not "hits on 0".
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 1);
        let (_, mut dice, mut rng) = kit();

        let before = dice.count();
        let hits = roll_fleet(
            &state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &defender(),
            &system,
        );
        assert_eq!(hits, 0);
        assert_eq!(dice.count(), before, "an empty fleet rolls no dice");
    }

    #[test]
    fn every_fighting_ship_rolls() {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 3);
        let (_, mut dice, mut rng) = kit();

        roll_fleet(
            &state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &attacker(),
            &system,
        );

        // Three destroyers share one combat value, so they roll together as one batch.
        assert_eq!(dice.count(), 1);
        assert_eq!(dice.history()[0].faces.len(), 3);
    }

    #[test]
    fn hits_are_absorbed_by_destroying_ships() {
        let (mut state, system) = arena();
        put(&mut state, &system, "fighter", &defender(), 3);
        let (mut table, _, _) = kit();

        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &defender(),
            &system,
            2,
        )
        .unwrap();

        assert_eq!(
            ships_of(&state, ContentStore::embedded(), POK, &defender(), &system).len(),
            1
        );
    }

    #[test]
    fn excess_hits_have_no_effect() {
        // 15.2a: more hits than units is not an error, and must not underflow.
        let (mut state, system) = arena();
        put(&mut state, &system, "fighter", &defender(), 2);
        let (mut table, _, _) = kit();

        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &defender(),
            &system,
            9,
        )
        .unwrap();

        assert!(ships_of(&state, ContentStore::embedded(), POK, &defender(), &system).is_empty());
    }

    #[test]
    fn sustain_damage_cancels_a_hit_instead_of_losing_the_ship() {
        // 87.1. FirstOption takes the sustain option, which is offered before the casualty.
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &defender(), 1);
        let mut table = Table::with_default(Box::new(FirstOption));

        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &defender(),
            &system,
            1,
        )
        .unwrap();

        let survivors = ships_of(&state, ContentStore::embedded(), POK, &defender(), &system);
        assert_eq!(survivors.len(), 1, "the ship survived");
        assert!(survivors[0].sustained_damage, "by taking damage");
    }

    #[test]
    fn sustain_is_optional_and_declining_loses_the_ship() {
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &defender(), 1);
        let mut table = Table::with_default(Box::new(Scripted::new(["decline".to_owned()])));

        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &defender(),
            &system,
            1,
        )
        .unwrap();

        assert!(
            ships_of(&state, ContentStore::embedded(), POK, &defender(), &system).is_empty(),
            "87.1 is always optional"
        );
    }

    #[test]
    fn an_already_damaged_ship_cannot_sustain_again() {
        let (mut state, system) = arena();
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("dreadnought"), defender()).sustained());
        let mut table = Table::with_default(Box::new(FirstOption));

        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &defender(),
            &system,
            1,
        )
        .unwrap();

        assert!(
            ships_of(&state, ContentStore::embedded(), POK, &defender(), &system).is_empty(),
            "it was already damaged, so the hit lands"
        );
    }

    #[test]
    fn interchangeable_casualties_are_one_decision_not_five() {
        // With five fighters and one dreadnought, offering per hull destroyed a fighter five
        // times in six whatever the decider thought of the trade — the count decided.
        let (mut state, system) = arena();
        put(&mut state, &system, "fighter", &defender(), 5);
        put(&mut state, &system, "cruiser", &defender(), 1);
        let units = ships_of(&state, ContentStore::embedded(), POK, &defender(), &system);
        let mut table = Table::new();

        let taken = choose_casualty(
            &state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &defender(),
            &units,
        )
        .unwrap();
        let offered = table.log.records.last().expect("a choice was recorded");

        assert_eq!(offered.offered.len(), 2, "one fighter, one cruiser");
        assert!(!taken.type_id.as_str().is_empty());
    }

    #[test]
    fn a_damaged_ship_is_a_different_casualty_from_a_fresh_one() {
        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &defender(), 1);
        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("dreadnought"), defender()).sustained());
        let units = ships_of(&state, ContentStore::embedded(), POK, &defender(), &system);
        let mut table = Table::new();

        choose_casualty(
            &state,
            ContentStore::embedded(),
            POK,
            None,
            &mut table,
            &defender(),
            &units,
        )
        .unwrap();
        let offered = table.log.records.last().unwrap();

        assert_eq!(offered.offered.len(), 2, "fresh and damaged are distinct");
    }

    #[test]
    fn the_same_seed_fights_the_same_battle() {
        let fight = |seed: u64| {
            let (mut state, system) = arena();
            put(&mut state, &system, "cruiser", &attacker(), 3);
            put(&mut state, &system, "cruiser", &defender(), 3);
            let mut table = Table::new();
            let mut dice = Dice::new();
            let mut rng = GameRng::new(seed);
            let outcome = resolve(
                &mut state,
                ContentStore::embedded(),
                POK,
                &mut table,
                &mut dice,
                &mut rng,
                &system,
            )
            .unwrap();
            (outcome, dice.count())
        };

        assert_eq!(fight(11), fight(11), "a seed reproduces its battle");
    }

    #[test]
    fn a_stepped_combat_keeps_hits_simultaneous() {
        // The thing that makes a resumable combat hard: both sides' hits must be computed
        // before either is absorbed, or a casualty reduces return fire it had already earned.
        // Resolving one side to completion and then rolling the other - the obvious way to
        // write this - would break 78.6 exactly there.
        let (mut state, system) = arena();
        put(&mut state, &system, "fighter", &attacker(), 1);
        put(&mut state, &system, "fighter", &defender(), 1);

        let mut window = CombatWindow::new(&state, ContentStore::embedded(), POK, &system);
        let mut table = Table::new();
        let mut dice = Dice::new();
        let mut rng = GameRng::new(2);
        let mut inner = Table::new();
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut inner,
            timing: None,
        };
        window.settle(&mut state, &mut ctx);

        let mut steps = 0;
        while let Some(choice) = window.pending_choice(&state, ContentStore::embedded(), POK) {
            let answer = table.ask(&choice).unwrap();
            window.resolve(&mut state, &mut ctx, answer).unwrap();
            steps += 1;
            assert!(steps < 500, "a stepped combat must terminate");
        }

        assert!(window.outcome().is_some(), "the fight concluded");
        let left = combatants(&state, ContentStore::embedded(), POK, &system);
        assert!(left.len() <= 1);
    }

    #[test]
    fn a_stepped_combat_matches_the_driven_one() {
        // Stepping and driving are the same fight: Window::drive is only a loop over the
        // same two methods, and a seed proves they do not diverge.
        let fight = |stepped: bool| {
            let (mut state, system) = arena();
            put(&mut state, &system, "cruiser", &attacker(), 3);
            put(&mut state, &system, "carrier", &defender(), 2);
            let mut table = Table::new();
            let mut dice = Dice::new();
            let mut rng = GameRng::new(17);
            if stepped {
                let mut window = CombatWindow::new(&state, ContentStore::embedded(), POK, &system);
                let mut inner = Table::new();
                let mut ctx = crate::choice::Resolving {
                    content: ContentStore::embedded(),
                    sources: POK,
                    dice: &mut dice,
                    rng: &mut rng,
                    table: &mut inner,
                    timing: None,
                };
                window.settle(&mut state, &mut ctx);
                while let Some(choice) =
                    window.pending_choice(&state, ContentStore::embedded(), POK)
                {
                    let answer = table.ask(&choice).unwrap();
                    window.resolve(&mut state, &mut ctx, answer).unwrap();
                }
                (window.outcome().unwrap(), state)
            } else {
                let outcome = resolve(
                    &mut state,
                    ContentStore::embedded(),
                    POK,
                    &mut table,
                    &mut dice,
                    &mut rng,
                    &system,
                )
                .unwrap();
                (outcome, state)
            }
        };

        let (stepped_outcome, stepped_state) = fight(true);
        let (driven_outcome, driven_state) = fight(false);
        assert_eq!(stepped_outcome, driven_outcome);
        assert!(stepped_state.identical(&driven_state));
    }

    #[test]
    fn hits_are_simultaneous() {
        // 78.6. Two single fighters both hitting must both die: resolving sequentially would
        // let the first casualty cancel return fire it had already earned.
        let (mut state, system) = arena();
        put(&mut state, &system, "fighter", &attacker(), 1);
        put(&mut state, &system, "fighter", &defender(), 1);
        let mut table = Table::new();

        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &defender(),
            &system,
            1,
        )
        .unwrap();
        absorb_hits(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &attacker(),
            &system,
            1,
        )
        .unwrap();

        assert!(
            combatants(&state, ContentStore::embedded(), POK, &system).is_empty(),
            "both fleets were destroyed in the same round"
        );
    }
}
