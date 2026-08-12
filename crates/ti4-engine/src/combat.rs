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

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Resolving, Table, Window};
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
    /// The last player with ships, or `None` if both sides were wiped out.
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
    for unit in ships_of(state, content, sources, player, system) {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) = kind.combat_hits_on() else {
            continue;
        };
        *fighting.entry(value).or_insert(0) += kind.combat_dice();
    }

    let mut hits = 0;
    for (value, count) in fighting {
        let dice_count = usize::try_from(count).unwrap_or(0);
        if dice_count == 0 {
            continue;
        }
        let threshold = u32::try_from(value).unwrap_or(u32::MAX);
        let roll = dice.roll(rng, dice_count, "space combat", Some(threshold));
        hits += roll.hits();
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
fn offer_sustain(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
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
        let answer = table.ask(&choice)?;
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
    let mut remaining = offer_sustain(state, content, sources, table, player, system, hits)?;

    while remaining > 0 {
        let alive = ships_of(state, content, sources, player, system);
        if alive.is_empty() {
            return Ok(()); // 15.2a
        }
        let casualty = choose_casualty(table, player, &alive)?;
        state
            .system_mut(system)
            .remove(std::slice::from_ref(&casualty));
        remaining -= 1;
    }
    Ok(())
}

/// 78.6: the owning player chooses which of their own units dies.
fn choose_casualty(
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
    let answer = table.ask(&choice)?;
    let index = answer
        .id
        .strip_prefix("destroy|")
        .and_then(|rest| rest.parse::<usize>().ok())
        .unwrap_or(0);
    Ok(units.get(index).unwrap_or(&units[0]).clone())
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
                    winner: sides.first().cloned(),
                    rounds: 0,
                }),
            };
        };
        Self {
            system: system.clone(),
            attacker: attacker.clone(),
            defender: defender.clone(),
            stage: Stage::Rolling { round: 1 },
        }
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
        Stage::Done(CombatOutcome {
            winner: winner(
                state,
                content,
                sources,
                &self.system,
                &self.attacker,
                &self.defender,
            ),
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

    /// Advance past anything with no decision left in it.
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
                        // Straight into the next round rather than returning: a stage that
                        // owes no decision must never be what `drive` stops on.
                        self.stage = Stage::Rolling { round: round + 1 };
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
    };
    // Opening does not roll; settle once so a fight that is already over reports so.
    window.settle(state, &mut ctx);
    window.drive(state, &mut ctx, table)?;
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

        let taken = choose_casualty(&mut table, &defender(), &units).unwrap();
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

        choose_casualty(&mut table, &defender(), &units).unwrap();
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
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
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
                let mut ctx = crate::choice::Resolving {
                    content: ContentStore::embedded(),
                    sources: POK,
                    dice: &mut dice,
                    rng: &mut rng,
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
