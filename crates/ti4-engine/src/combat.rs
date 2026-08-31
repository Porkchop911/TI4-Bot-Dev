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
use ti4_content::galaxy::Galaxy;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, RelicId, SystemId};
use ti4_model::state::{Feat, FeatOccurrence, GameState, RerollEntry, RerollSet};
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
    #[error(transparent)]
    Timing(#[from] crate::timing::TimingError),
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
    // Fighter Prototype: "+2 to the result of each of your fighters' combat rolls". One entry
    // per copy, so two cards played in the same round give four.
    let fighter_bonus = if catalogue(content, sources)
        .get(unit.type_id.as_str())
        .is_some_and(UnitType::is_fighter)
    {
        fighter_bonus_now(state, player)
    } else {
        0
    };
    // 59.5: "If a space combat occurs in a nebula, the defender applies +1 to each combat roll of
    // their ships during that combat." The defender is whoever is not the active player -- an
    // activation is what starts a space combat, so the attacker is always the active seat.
    //
    // Applied to the threshold rather than the die, like the faction shift above: +1 to the roll is
    // the same as needing one less. Only ships, and this function is reached only from
    // `fleet_groups`, which iterates `ships_of` -- so ground combat in a nebula is untouched, which
    // is what the rule says.
    let nebula_defender = state
        .active_system
        .as_ref()
        .filter(|_| state.active.as_ref() != Some(player))
        .and_then(|system| ti4_content::galaxy::system(content, system.as_str(), sources))
        .is_some_and(|system| system.is_nebula());

    Some(
        threshold
            - i64::from(morale_is_current)
            - faction
            - fighter_bonus
            - i64::from(nebula_defender),
    )
}

/// The roll bonus this seat's fighters carry in the current combat round (Fighter Prototype):
/// two per copy held, counted only while the round the cards were played in is the live one.
fn fighter_bonus_now(state: &GameState, player: &PlayerId) -> i64 {
    let prototypes = state.player(player).map_or(0, |seat| {
        2 * i64::try_from(
            seat.fighter_bonus_round
                .iter()
                .filter(|round| **round == state.combat_round_seq)
                .count(),
        )
        .unwrap_or(i64::MAX)
    });
    // Prophecy of Ixth is the same shape and stacks with the card: "+1 to the result of their
    // fighter's combat rolls", for as long as its owner still holds the law.
    prototypes + crate::laws::fighter_combat_bonus(state, player)
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

// -- reroll windows -----------------------------------------------------------
//
// Fire Team, Scramble Frequency, and Aglnlan Oln all act on dice that have been rolled
// but not yet applied. Each roll site stages its rolls in `GameState::reroll_staging`,
// opens the window, and then recomputes the hits from whatever faces remain when the
// window closes.

/// Ask the roller, one optional question per die, which dice to reroll.
///
/// A die whose question fails (the decider answered with something not offered) is kept
// as-is, the same way the Letnev Munitions ask degrades, so an optional reroll can never
/// wedge the combat.
#[must_use]
pub fn choose_reroll_dice(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    player: &PlayerId,
) -> Vec<(usize, usize)> {
    let Some(set) = state.reroll_staging.get(player) else {
        return Vec::new();
    };
    let observed = Observed::new(state, content, sources, galaxy);
    let mut picks = Vec::new();
    for (unit, entry) in set.rolls.iter().enumerate() {
        for (die, face) in entry.faces.iter().enumerate() {
            let choice = Choice::new(
                player.clone(),
                format!("reroll die {} of {}", die + 1, entry.unit),
                vec![
                    ChoiceOption::labelled(
                        format!("reroll|{unit}:{die}"),
                        "reroll_die",
                        format!("reroll die {} of {} (shows {})", die + 1, entry.unit, face),
                    ),
                    ChoiceOption::decline(),
                ],
            );
            let Ok(answer) = table.ask_seeing(&choice, &observed) else {
                continue;
            };
            if !answer.is_decline() {
                picks.push((unit, die));
            }
        }
    }
    picks
}

/// Re-draw the chosen dice of the staged set through the game's roller, in place.
///
/// A re-drawn die replaces whatever the previous faces — and any Thalnos +1 sitting on
/// that position — said: the offset is removed with the die it adjusted.
pub fn apply_reroll_dice(
    dice: &mut Dice,
    rng: &mut GameRng,
    set: &mut RerollSet,
    picks: &[(usize, usize)],
    reason: &str,
) {
    for (unit, die) in picks {
        let Some(entry) = set.rolls.get_mut(*unit) else {
            continue;
        };
        if *die >= entry.faces.len() {
            continue;
        }
        let original = crate::dice::Roll {
            reason: set.kind.clone(),
            faces: entry.faces.clone(),
            hits_on: entry.hits_on,
            rerolled: std::collections::BTreeSet::new(),
        };
        let again = dice.reroll(rng, &original, [*die], Some(reason));
        entry.faces = again.faces;
        entry.rerolled.insert(*die);
        entry.deltas.remove(die);
    }
}

/// The hits a staged set currently produces from its (possibly rerolled) faces.
#[must_use]
pub fn staged_hits(set: &RerollSet) -> usize {
    set.rolls.iter().map(RerollEntry::hits).sum()
}

/// Open the reroll window for the roll `side` just made: the roller's own commander reroll
/// first (Agnlan Oln), then the relics that watch the dice, then the event other players
/// react to (Scramble Frequency).
///
/// The event goes through the timing resolver so reaction windows actually fire. Called at
/// the space cannon, anti-fighter barrage, and bombardment sites; the caller recomputes
/// the hits from the staging afterwards and clears it.
///
/// # Panics
/// If the commander hook re-enters the staging it just read and finds it gone (a card effect
/// ran in the meantime and removed it), which cannot happen on the call sites: the staged set
/// is only removed by the caller after this returns.
pub fn open_reroll_windows(state: &mut GameState, ctx: &mut Resolving<'_>, side: &PlayerId) {
    reroll_window(state, ctx, side, true);
}

/// The fleet's own window. A fleet roll is not a unit-ability roll, so Aglnlan Oln and
/// Scramble Frequency stand down and no `UNIT_ABILITY_ROLLED` event goes out; the relics
/// that watch *any* die do not.
pub fn open_fleet_reroll_windows(state: &mut GameState, ctx: &mut Resolving<'_>, side: &PlayerId) {
    reroll_window(state, ctx, side, false);
}

/// The window both entry points share: the rerolls the dice's owner may make, the units
/// those rerolls destroy, and (for unit-ability rolls only) the event other players react
/// to, after which the Heart of Ixth may bend whatever faces survive.
fn reroll_window(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    side: &PlayerId,
    unit_ability: bool,
) {
    let Some(set) = state.reroll_staging.get(side) else {
        return;
    };
    let (kind, system) = (set.kind.clone(), set.system.clone());
    let has_dice = set.rolls.iter().any(|entry| !entry.faces.is_empty());
    // Aglnlan Oln: "After you roll dice for a unit ability: You may reroll any of those
    // dice." Ground rolls are not unit ability rolls, so the commander stands down there,
    // and fleet rolls are not either.
    let commander_reroll = unit_ability
        && has_dice
        && kind != "ground"
        && state.player(side).is_some_and(|seat| {
            seat.leaders.iter().any(|(leader, status)| {
                leader.as_str() == "jolnarcommander"
                    && *status == ti4_model::state::LeaderStatus::Unlocked
            })
        });
    if commander_reroll {
        let picks = choose_reroll_dice(state, ctx.content, ctx.sources, None, ctx.table, side);
        if !picks.is_empty() {
            let set = state.reroll_staging.get_mut(side).expect("checked above");
            apply_reroll_dice(ctx.dice, ctx.rng, set, &picks, "jolnar commander");
        }
    }
    // The Crown of Thalnos (relic): "this card's owner may reroll any number of their dice,
    // applying +1 to the results".
    // Both Thalnos cards read "During each combat round", and only two of the five staged roll
    // kinds are one: the fleet roll of a space combat round, and a ground combat round. Space
    // cannon, anti-fighter barrage and bombardment are steps of a tactical action or of an
    // invasion, not rounds -- so neither card reaches them, however tempting the dice look.
    let combat_round = matches!(kind.as_str(), "fleet" | "ground");
    let thalnos_picks = if combat_round && has_dice && holds_crown_relic(state, side) {
        choose_reroll_dice(state, ctx.content, ctx.sources, None, ctx.table, side)
    } else {
        Vec::new()
    };
    if !thalnos_picks.is_empty() {
        let set = state.reroll_staging.get_mut(side).expect("checked above");
        apply_reroll_dice(ctx.dice, ctx.rng, set, &thalnos_picks, "thalnos");
        for (unit, die) in &thalnos_picks {
            if let Some(entry) = set.rolls.get_mut(*unit) {
                entry.deltas.insert(*die, 1);
            }
        }
    }
    // The Crown of Thalnos (law): the elected player "may reroll any number of dice" —
    // the destruction clause below does not forgive the misses the reroll leaves.
    let crown_picks = if combat_round && has_dice && owns_crown_law(state, side) {
        choose_reroll_dice(state, ctx.content, ctx.sources, None, ctx.table, side)
    } else {
        Vec::new()
    };
    if !crown_picks.is_empty() {
        let set = state.reroll_staging.get_mut(side).expect("checked above");
        apply_reroll_dice(ctx.dice, ctx.rng, set, &crown_picks, "crown of thalnos");
    }
    destroy_reroll_casualties(
        state,
        ctx,
        &kind,
        &system,
        side,
        &thalnos_picks,
        &crown_picks,
    );
    if unit_ability {
        let hits = staged_hits(state.reroll_staging.get(side).expect("checked above"));
        let mut payload = std::collections::BTreeMap::new();
        payload.insert("kind".to_owned(), kind.into());
        payload.insert("player".to_owned(), side.to_string().into());
        payload.insert("system".to_owned(), system.to_string().into());
        payload.insert("hits".to_owned(), i64::try_from(hits).unwrap_or(0).into());
        let _ = ctx.emit(state, "UNIT_ABILITY_ROLLED", payload);
    }
    heart_ixth(state, ctx, side);
}

/// Whether `player` holds the Crown of Thalnos relic.
fn holds_crown_relic(state: &GameState, player: &PlayerId) -> bool {
    crate::relics::holds(state, player, &RelicId::new("thalnos"))
}

/// Whether `player` owns the Crown of Thalnos law — the elected player is its owner.
fn owns_crown_law(state: &GameState, player: &PlayerId) -> bool {
    crate::laws::elected(state, "crown_of_thalnos").is_some_and(|owner| owner == player.as_str())
}

/// The Crown of Thalnos' destruction clauses, judged from the faces every reroll left behind.
///
/// The relic is wide and the +1 is part of the result: "any units that reroll dice but do
/// not produce at least 1 hit are destroyed" looks at the entry's whole total, adjusted.
/// The law is narrow and unforgiving: "they must destroy each of their units that did not
/// produce a hit **with its reroll**" looks only at the dice the law's own reroll re-drew.
/// A die the later Crown re-drew replaced the die the relic blessed, so both clauses read
/// the final faces and agree on what the entry now shows.
fn destroy_reroll_casualties(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    kind: &str,
    system: &SystemId,
    player: &PlayerId,
    thalnos_picks: &[(usize, usize)],
    crown_picks: &[(usize, usize)],
) {
    if thalnos_picks.is_empty() && crown_picks.is_empty() {
        return;
    }
    let (content, sources) = (ctx.content, ctx.sources);
    let set = state
        .reroll_staging
        .get_mut(player)
        .expect("opened from it");
    let doomed: Vec<usize> = set
        .rolls
        .iter()
        .enumerate()
        .filter_map(|(entry, roll)| {
            let thalnos_doomed =
                thalnos_picks.iter().any(|(unit, _)| *unit == entry) && roll.hits() == 0;
            let crown_doomed =
                entry_reroll_hits(roll, crown_picks, entry).is_some_and(|hits| hits == 0);
            (thalnos_doomed || crown_doomed).then_some(entry)
        })
        .collect();
    if doomed.is_empty() {
        return;
    }
    // The casualties are lifted out of the staging before the board is touched: the set
    // is a mutable borrow of the state the removals run through.
    let doomed_units: Vec<RerollEntry> = {
        let mut out = Vec::new();
        for index in doomed.iter().rev() {
            let entry = set.rolls[*index].clone();
            set.rolls.remove(*index);
            out.push(entry);
        }
        out
    };
    for entry in doomed_units {
        remove_casualty_units(state, ctx, content, sources, kind, system, player, &entry);
    }
}

/// If `picks` re-drew any die of entry `entry`, the hits those dice now show — the plain
/// faces, since a re-drawn die carries no adjustment left to count.
fn entry_reroll_hits(entry: &RerollEntry, picks: &[(usize, usize)], which: usize) -> Option<usize> {
    let dice: Vec<usize> = picks
        .iter()
        .filter(|(unit, _)| *unit == which)
        .map(|(_, die)| *die)
        .collect();
    if dice.is_empty() {
        return None;
    }
    Some(entry.hits_on.map_or(0, |on| {
        dice.iter().filter(|die| entry.faces[**die] >= on).count()
    }))
}

/// Remove the units a reroll casualty stands for.
///
/// A unit carries no identity past its type and owner, so "the unit that made this roll"
/// is "a unit of these types": that many are removed in deterministic order — space first,
/// then the system's planets in order, which is where a planet-mounted cannon sits. The
/// entry's `unit_types` says how many of each type pooled into the roll; a pooled entry
/// that produced no hits loses every unit in the pool, because every one of them rerolled
/// a die and none of them produced a hit. Fleet dice are the one case where the loss is
/// announced: the ships die mid-combat, and cards read "when your last ship is destroyed".
fn remove_casualty_units(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    content: &ContentStore,
    sources: SourceSet,
    kind: &str,
    system: &SystemId,
    player: &PlayerId,
    entry: &RerollEntry,
) {
    if entry.unit_types.is_empty() {
        // A relic's own dice, not a unit's (Metali Void Armaments' extra barrage): there
        // is no unit to lose.
        return;
    }
    let announce = kind == "fleet";
    for (type_id, count) in &entry.unit_types {
        let mut due = usize::try_from(*count).unwrap_or(0);
        if due == 0 {
            continue;
        }
        let board = state.system_state(system);
        let matching: Vec<Unit> = board
            .units
            .iter()
            .filter(|unit| unit.owner == *player && unit.type_id.as_str() == type_id)
            .cloned()
            .chain(
                board
                    .planet_units
                    .values()
                    .flatten()
                    .filter(|unit| unit.owner == *player && unit.type_id.as_str() == type_id)
                    .cloned(),
            )
            .take(due)
            .collect();
        for unit in matching {
            due -= 1;
            state.system_mut(system).remove(std::slice::from_ref(&unit));
            if announce {
                announce_ship_destroyed(state, ctx, system, player, &unit, content, sources);
            }
        }
    }
}

/// Heart of Ixth: "After any die is rolled, you may exhaust this card to add or subtract
/// 1 from its result."
///
/// A relic cannot hold a reaction slot, so the driver asks instead: after the roll and
/// every reroll of it are done, each holder who still has the card ready — in seat order,
/// so two holders may each bend a die of the same roll — gets one question. The die may
/// be any die `side` just rolled, which is what lets a holder bend an opponent's result
/// as well as their own. Each use exhausts the card until the status phase readies it.
// Four lines over the limit, and every one of them is the offer: a die, a sign, and the label that
// tells the holder what the shift would do. Splitting it would put a boundary inside one question.
#[expect(
    clippy::too_many_lines,
    reason = "one offer, built per die and per sign"
)]
fn heart_ixth(state: &mut GameState, ctx: &mut Resolving<'_>, side: &PlayerId) {
    const RELIC: &str = "heartofixth";
    let Some(set) = state.reroll_staging.get(side) else {
        return;
    };
    let dice: Vec<(usize, usize)> = set
        .rolls
        .iter()
        .enumerate()
        .flat_map(|(entry, roll)| {
            roll.faces
                .iter()
                .enumerate()
                .map(move |(die, _face)| (entry, die))
        })
        .collect();
    if dice.is_empty() {
        return;
    }
    let relic = RelicId::new(RELIC);
    // The question is identical for every holder, so it is built once, before any of
    // the answers below takes a mutable borrow of the state.
    let options: Vec<ChoiceOption> = if dice.len() == 1 {
        let (_, die) = dice[0];
        let unit = &set.rolls[0].unit;
        vec![
            ChoiceOption::labelled(
                format!("hfix|0:{die}:add"),
                "heart_ixth",
                format!("add 1 to the die of {unit}"),
            ),
            ChoiceOption::labelled(
                format!("hfix|0:{die}:sub"),
                "heart_ixth",
                format!("subtract 1 from the die of {unit}"),
            ),
        ]
    } else {
        dice.iter()
            .flat_map(|(entry, die)| {
                let unit = &set.rolls[*entry].unit;
                vec![
                    ChoiceOption::labelled(
                        format!("hfix|{entry}:{die}:add"),
                        "heart_ixth",
                        format!("add 1 to die {} of {die} of {unit}", die + 1),
                    ),
                    ChoiceOption::labelled(
                        format!("hfix|{entry}:{die}:sub"),
                        "heart_ixth",
                        format!("subtract 1 from die {} of {die} of {unit}", die + 1),
                    ),
                ]
            })
            .collect()
    };
    // The eligible holders, in seat order, gathered before any answer below takes a
    // mutable borrow of the state.
    let candidates: Vec<PlayerId> = state
        .players
        .iter()
        .map(|seat| seat.id.clone())
        .filter(|id| {
            crate::relics::holds(state, id, &relic)
                && !state
                    .player(id)
                    .is_some_and(|seat| seat.exhausted_relics.contains(&relic))
        })
        .collect();
    for seat_id in candidates {
        let mut options = options.clone();
        options.push(ChoiceOption::decline());
        let choice = Choice::new(
            seat_id.clone(),
            "Heart of Ixth: add or subtract 1 from a die that was just rolled",
            options,
        );
        let picked = {
            let observed = Observed::new(state, ctx.content, ctx.sources, None);
            match ctx.table.ask_seeing(&choice, &observed) {
                Ok(answer) if !answer.is_decline() => Some(answer.id),
                _ => None,
            }
        };
        let Some(id) = picked else {
            continue;
        };
        // The id is the option's: "hfix|{entry}:{die}:{sign}". Anything else is a decider
        // that answered with something not offered, and the card stays ready for the next
        // roll rather than being spent on a die nobody chose.
        let mut parts = id.split('|');
        let (Some(head), Some(entry), Some(die), Some(sign)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if head != "hfix" {
            continue;
        }
        let (Ok(entry), Ok(die)) = (entry.parse::<usize>(), die.parse::<usize>()) else {
            continue;
        };
        let delta: i8 = match sign {
            "add" => 1,
            "sub" => -1,
            _ => continue,
        };
        let set = state.reroll_staging.get_mut(side).expect("checked above");
        if entry >= set.rolls.len() || die >= set.rolls[entry].faces.len() {
            continue;
        }
        set.rolls[entry].deltas.insert(die, delta);
        if let Some(seat) = state.player_mut(&seat_id) {
            seat.exhausted_relics.insert(relic.clone());
        }
    }
}

/// One combat value's pooled fleet dice: how many, and which unit types sit behind them.
struct FleetGroup {
    dice: i64,
    types: std::collections::BTreeMap<String, u32>,
}

/// The fleet's rolls, grouped by combat value in the ascending order 78.5b/78.5c fixes.
///
/// Three destroyers are one roll of three dice, not three rolls of one: the number of
/// draws from the seeded stream is part of what a seed reproduces, so rolling them apart
/// would silently renumber every later draw. `BTreeMap` gives the ascending order for free.
fn fleet_groups(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> std::collections::BTreeMap<i64, FleetGroup> {
    let types = catalogue(content, sources);
    let extra_die = state.player(player).and_then(|seat| {
        (seat.extra_die_round == Some(state.combat_round_seq))
            .then_some(seat.extra_die_unit.as_ref())
            .flatten()
    });
    let mut extra_die_added = false;
    let mut groups: std::collections::BTreeMap<i64, FleetGroup> = std::collections::BTreeMap::new();
    for unit in ships_of(state, content, sources, player, system) {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) = effective_hits_on(state, content, sources, player, &unit) else {
            continue;
        };
        let mut dice = kind.combat_dice();
        if !extra_die_added && extra_die.is_some_and(|selected| selected == &unit.type_id) {
            dice += 1;
            extra_die_added = true;
        }
        let group = groups.entry(value).or_insert(FleetGroup {
            dice: 0,
            types: std::collections::BTreeMap::new(),
        });
        group.dice += dice;
        *group.types.entry(unit.type_id.to_string()).or_insert(0) += 1;
    }
    groups
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
    let mut hits = 0;
    for (value, group) in fleet_groups(state, content, sources, player, system) {
        let dice_count = usize::try_from(group.dice).unwrap_or(0);
        if dice_count == 0 {
            continue;
        }
        let threshold = u32::try_from(value).unwrap_or(u32::MAX);
        let roll = dice.roll(rng, dice_count, "space combat", Some(threshold));
        hits += reroll_munitions_misses(state, dice, rng, player, &roll).hits();
    }
    hits
}

/// Roll one player's fleet, stage the dice, and open the window that may still touch
/// them — the two Crowns of Thalnos and the Heart of Ixth — before any hit lands.
///
/// The staged faces are what the hits are read from: a die rerolled with a +1 is a new
/// result, and a unit the rerolls destroy is gone before the opponent's dice leave the
/// bag. A seed's draw sequence is untouched: a reroll is a fresh draw from the same
/// roller, and a window nobody answers changes nothing.
pub fn roll_fleet_and_open(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let (content, sources) = (ctx.content, ctx.sources);
    let mut set = RerollSet {
        kind: "fleet".into(),
        system: system.clone(),
        rolls: Vec::new(),
    };
    let mut hits = 0;
    for (value, group) in fleet_groups(state, content, sources, player, system) {
        let dice_count = usize::try_from(group.dice).unwrap_or(0);
        if dice_count == 0 {
            continue;
        }
        let threshold = u32::try_from(value).unwrap_or(u32::MAX);
        let roll = ctx
            .dice
            .roll(ctx.rng, dice_count, "space combat", Some(threshold));
        let roll = reroll_munitions_misses(state, ctx.dice, ctx.rng, player, &roll);
        hits += roll.hits();
        set.rolls.push(RerollEntry {
            unit: group.types.keys().cloned().collect::<Vec<_>>().join("+"),
            planet: None,
            hits_on: Some(threshold),
            faces: roll.faces,
            rerolled: std::collections::BTreeSet::new(),
            deltas: std::collections::BTreeMap::new(),
            unit_types: group.types,
        });
    }
    if set.rolls.iter().any(|roll| !roll.faces.is_empty()) {
        state.reroll_staging.insert(player.clone(), set);
        state.last_reroll_player = Some(player.clone());
    }
    open_fleet_reroll_windows(state, ctx, player);
    let hits = state.reroll_staging.get(player).map_or(hits, staged_hits);
    state.reroll_staging.remove(player);
    state.last_reroll_player = None;
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
///
/// # Errors
///
/// A Waylay casualty answer that the table rejects, or a dice source that runs dry.
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
) -> Result<Vec<(PlayerId, usize)>, CombatError> {
    let occurrence = state.begin_feat_occurrence();
    anti_fighter_barrage_at(
        state, content, sources, dice, rng, system, attacker, defender, occurrence,
    )
    .map(|(resolved, _)| resolved)
}

/// Roll one side's anti-fighter barrage and stage the roll for the reroll windows.
///
/// The dice-consuming half: both the combat window and the synchronous wrapper below call
/// this, so a given seed makes the same barrage rolls however the hits are later assigned.
pub fn roll_barrage_side(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    player: &PlayerId,
) -> usize {
    let types = catalogue(content, sources);
    let mut set = RerollSet {
        kind: "anti_fighter_barrage".into(),
        system: system.clone(),
        rolls: Vec::new(),
    };
    let mut hits = 0;
    // Metali Void Armaments fires once for its holder, not once per ship: the card grants the
    // barrage to the player.
    if let Some((value, count)) = crate::relics::extra_barrage(state, player) {
        let roll = dice.roll(rng, count, "anti_fighter_barrage", Some(value));
        hits += roll.hits();
        set.rolls.push(RerollEntry {
            unit: "extra barrage".into(),
            planet: None,
            hits_on: Some(value),
            faces: roll.faces,
            rerolled: std::collections::BTreeSet::new(),
            deltas: std::collections::BTreeMap::new(),
            // The relic's own dice, not a unit's: nothing here can be a casualty.
            unit_types: std::collections::BTreeMap::new(),
        });
    }
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
        // Fighter Prototype names "each of your fighters' combat rolls": the barrage is a
        // fighters' combat roll, so it gets the same +2 per copy as the fleet rolls do.
        let value = if kind.is_fighter() {
            value - fighter_bonus_now(state, player)
        } else {
            value
        };
        let roll = dice.roll(
            rng,
            count,
            "anti-fighter barrage",
            Some(u32::try_from(value).unwrap_or(u32::MAX)),
        );
        hits += roll.hits();
        set.rolls.push(RerollEntry {
            unit: unit.type_id.to_string(),
            planet: None,
            hits_on: Some(u32::try_from(value).unwrap_or(u32::MAX)),
            faces: roll.faces,
            rerolled: std::collections::BTreeSet::new(),
            deltas: std::collections::BTreeMap::new(),
            unit_types: std::iter::once((unit.type_id.to_string(), 1)).collect(),
        });
    }
    if set.rolls.iter().any(|roll| !roll.faces.is_empty()) {
        state.reroll_staging.insert(player.clone(), set);
        state.last_reroll_player = Some(player.clone());
    }
    hits
}

/// Resolve both barrages: remove the target's fighters for each side's (possibly rerolled)
/// hits, and keep the event-scoped feats that creates.
fn apply_barrage(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    ctx: &mut Resolving<'_>,
    system: &SystemId,
    attacker: &PlayerId,
    defender: &PlayerId,
    results: &[(PlayerId, usize)],
    occurrence: FeatOccurrence,
) -> Result<Vec<PlayerId>, CombatError> {
    let mut feat_players = Vec::new();
    for (player, hits) in results {
        let target = if player == attacker {
            defender
        } else {
            attacker
        };
        // Fight with Precision asks for the last fighter specifically, and specifically during
        // this step, so the count is taken either side of the removal rather than after the
        // combat: by then ordinary combat rounds have taken fighters too, and nothing would say
        // which step emptied the system.
        let before = fighters_of(state, content, sources, target, system);
        if *hits > 0 {
            // Waylay ("hits from this roll are produced against all ships (not just
            // fighters)", played before this side's barrage roll): the hits are assigned
            // like any other combat hits — SUSTAIN DAMAGE first, then the target's owner
            // chooses the losses — instead of taking fighters in unit order.
            let waylay = state
                .player(player)
                .and_then(|seat| seat.waylay_barrage_round)
                .is_some_and(|played| played == state.combat_round_seq);
            if waylay {
                absorb_hits_seeing(
                    state, content, sources, galaxy, ctx, target, system, player, *hits,
                )?;
            } else {
                destroy_fighters(state, content, sources, target, system, *hits);
            }
        }
        if before > 0 && fighters_of(state, content, sources, target, system) == 0 {
            state.record_event_feat(player, Feat::BarrageTookTheLastFighters, occurrence);
            feat_players.push(player.clone());
        }
    }
    Ok(feat_players)
}

/// Resolve anti-fighter barrage without the reroll windows: roll both sides and apply. The
/// synchronous API the tests and standalone callers use; the combat window instead rolls the
/// same [`roll_barrage_side`] with the windows opened in between, so the two paths cannot
/// disagree about the dice. The staging left behind is stale by construction and is cleared
/// at the start of the next windowed roll.
///
/// Without the caller's timing machinery there is no resolver to ask a Waylay casualty
/// choice to, so the synchronous path answers its own questions with the table's default
/// decider. The stale-staging and dice guarantees above are unaffected.
#[allow(
    clippy::too_many_arguments,
    reason = "one parameter per genuinely distinct input"
)]
#[allow(
    clippy::type_complexity,
    reason = "hits and feat players are one result pair, not a new type"
)]
fn anti_fighter_barrage_at(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    attacker: &PlayerId,
    defender: &PlayerId,
    occurrence: FeatOccurrence,
) -> Result<(Vec<(PlayerId, usize)>, Vec<PlayerId>), CombatError> {
    let mut pending = Vec::new();
    for player in [attacker, defender] {
        let hits = roll_barrage_side(state, content, sources, dice, rng, system, player);
        if hits > 0 {
            pending.push((player.clone(), hits));
        }
    }
    let resolved = pending.clone();
    let mut table = Table::new();
    let mut ctx = Resolving {
        content,
        sources,
        dice,
        rng,
        table: &mut table,
        timing: None,
    };
    let feat_players = apply_barrage(
        state, content, sources, None, &mut ctx, system, attacker, defender, &pending, occurrence,
    )?;
    Ok((resolved, feat_players))
}

/// Fighters this player has in the space area of a system.
fn fighters_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(UnitType::is_fighter)
        })
        .count()
}

/// Non-fighter ships this player has in the space area of a system.
pub fn non_fighter_ships_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.is_ship() && !kind.is_fighter())
        })
        .count()
}

/// War suns and flagships this player has in the space area of a system.
///
/// The two ships Destroy Their Greatest Ship names. Read by base type rather than by alias so a
/// faction's upgraded flagship counts as the flagship it is.
fn capital_ships_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| matches!(kind.base_type(), "warsun" | "flagship"))
        })
        .count()
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
/// Guns in *adjacent* systems that may fire into this one.
///
/// PDS II ("you may use this unit's SPACE CANNON against ships that are in adjacent systems") and
/// Xxcha's Indomitus mech, which prints the same clause. Both stand on planets, so only planets are
/// searched; and both belong to somebody other than the active player, since space cannon offence
/// fires at the seat that activated the system.
fn reaching_guns(
    state: &GameState,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    system: &SystemId,
    active: &PlayerId,
) -> Vec<Unit> {
    const REACHES: [&str; 2] = ["pds2", "xxcha_mech"];
    let Some(galaxy) = galaxy else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for neighbour in galaxy.adjacent(system.as_str()) {
        let board = state.system_state(&SystemId::new(neighbour));
        for standing in board.planet_units.values() {
            found.extend(
                standing
                    .iter()
                    .filter(|unit| &unit.owner != active)
                    .filter(|unit| REACHES.contains(&unit.type_id.as_str()))
                    .cloned(),
            );
        }
    }
    found
}

pub fn space_cannon_offense(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut Dice,
    rng: &mut GameRng,
    system: &SystemId,
    active: &PlayerId,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Vec<(PlayerId, usize, Vec<RerollEntry>)> {
    // Solar Flare: during the named tactical action, other players cannot use SPACE CANNON
    // against the active player's ships. Every gun below belongs to another player and fires
    // at the active player's ships, which is exactly what the card forbids, so the whole step
    // is suppressed rather than gun by gun. The marker is activation-scoped, like the card's
    // "this tactical action" wording.
    if state
        .player(active)
        .is_some_and(|seat| seat.solar_flare.contains(&state.activation_seq))
    {
        return Vec::new();
    }
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

    // PDS II and Xxcha's Indomitus mech both read "You may use this unit's SPACE CANNON against
    // ships that are in adjacent systems." Two cards, one clause, and neither reached the active
    // system before: `space_cannon_offense` read only the system being activated, so an upgraded
    // PDS next door -- a technology every faction can research -- never fired at all.
    guns.extend(reaching_guns(state, galaxy, system, active));

    let mut by_player: std::collections::BTreeMap<PlayerId, (usize, Vec<RerollEntry>)> =
        std::collections::BTreeMap::new();
    for unit in guns {
        let Some(kind) = types.get(unit.type_id.as_str()) else {
            continue;
        };
        let Some(value) = kind.space_cannon_hits_on() else {
            continue;
        };
        // Disable strips SPACE CANNON from opponents' PDS during the invasion. In the driven
        // game the cannon step precedes the invasion window, so this binds only callers that
        // fire the guns after the card has played; it keeps the two PDS effects of the card
        // in lockstep either way.
        if kind.base_type() == "pds"
            && state.players.iter().any(|seat| {
                seat.id != unit.owner && seat.disable_invasion.contains(&state.activation_seq)
            })
        {
            continue;
        }
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
        let entry = RerollEntry {
            unit: unit.type_id.to_string(),
            planet: None,
            hits_on: Some(u32::try_from(value).unwrap_or(u32::MAX)),
            faces: roll.faces,
            rerolled: std::collections::BTreeSet::new(),
            deltas: std::collections::BTreeMap::new(),
            unit_types: std::iter::once((unit.type_id.to_string(), 1)).collect(),
        };
        let slot = by_player
            .entry(unit.owner.clone())
            .or_insert_with(|| (0, Vec::new()));
        slot.0 += entry.hits();
        slot.1.push(entry);
    }
    // Stage each gunner's rolls for the reroll windows; the caller opens one window per
    // gunner and names them with `last_reroll_player`.
    for (player, (_, rolls)) in &by_player {
        if rolls.iter().any(|roll| !roll.faces.is_empty()) {
            state.reroll_staging.insert(
                player.clone(),
                RerollSet {
                    kind: "space_cannon".into(),
                    system: system.clone(),
                    rolls: rolls.clone(),
                },
            );
        }
    }
    by_player
        .into_iter()
        .filter(|(_, (hits, _))| *hits > 0)
        .map(|(player, (hits, rolls))| (player, hits, rolls))
        .collect()
}

/// Hits a card has let this seat cancel in the current combat round (Shields Holding).
///
/// Consumed as they are used, so "cancel up to 2 hits" is two hits across the round rather than two
/// per assignment. Scoped to the round the card names.
#[must_use]
pub fn cancellable_hits(state: &GameState, player: &PlayerId) -> usize {
    state
        .player(player)
        .and_then(|seat| seat.cancel_hits_round)
        .filter(|(round, _)| *round == state.combat_round_seq)
        .map_or(0, |(_, hits)| hits)
}

/// Grant this seat cancellable hits for the current combat round.
pub fn grant_hit_cancellation(state: &mut GameState, player: &PlayerId, hits: usize) {
    let round = state.combat_round_seq;
    if let Some(seat) = state.player_mut(player) {
        let running = match seat.cancel_hits_round {
            Some((held, had)) if held == round => had,
            _ => 0,
        };
        seat.cancel_hits_round = Some((round, running + hits));
    }
}

/// Spend up to `wanted` of this seat's cancellations, returning how many were spent.
fn spend_cancellations(state: &mut GameState, player: &PlayerId, wanted: usize) -> usize {
    let available = cancellable_hits(state, player);
    let spent = available.min(wanted);
    if spent > 0 {
        let round = state.combat_round_seq;
        if let Some(seat) = state.player_mut(player) {
            seat.cancel_hits_round = Some((round, available - spent));
        }
    }
    spent
}

/// Whether a card has barred this seat from retreating this combat round (Intercept).
#[must_use]
pub fn retreat_barred(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .and_then(|seat| seat.retreat_barred_round)
        .is_some_and(|round| round == state.combat_round_seq)
}

/// Bar this seat from retreating for the current combat round.
pub fn bar_retreat(state: &mut GameState, player: &PlayerId) {
    let round = state.combat_round_seq;
    if let Some(seat) = state.player_mut(player) {
        seat.retreat_barred_round = Some(round);
    }
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

/// Barony of Letnev, Non-Euclidean Shielding: each use of SUSTAIN DAMAGE cancels two hits.
fn non_euclidean_shielding(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.technologies.iter().any(|held| held.as_str() == "nes"))
}

fn offer_sustain(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    ctx: &mut Resolving<'_>,
    player: &PlayerId,
    system: &SystemId,
    producer: &PlayerId,
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
                let Some(kind) = types.get(unit.type_id.as_str()) else {
                    return false;
                };
                &unit.owner == player
                    && !unit.sustained_damage
                    // Publicize Weapon Schematics: all war suns lose SUSTAIN DAMAGE. Asked of the
                    // unit here rather than removed from the type, so repealing the law gives it
                    // back.
                    && !crate::laws::sustain_suppressed(state, kind.base_type())
                    // Metali Void Shielding grants the ability to a non-fighter ship that lacks it.
                    // Asked here rather than of the unit type, so a dreadnought is not given a
                    // second sustain it never had.
                    && (kind.sustain_damage()
                        || (crate::relics::grants_sustain(state, player)
                            && kind.is_ship()
                            && !kind.is_fighter()))
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
        let answer = ctx
            .table
            .ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
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
            let kind = unit.type_id.to_string();
            *unit = unit.sustained();
            pay_sustain_commander(state, content, player);
            emit_sustain_used(state, ctx, galaxy, system, player, &kind, producer)?;
        }
        // Non-Euclidean Shielding: "When 1 of your units uses SUSTAIN DAMAGE, cancel 2 hits
        // instead of 1." Per *use*, so a second hit costs the same one damaged ship -- which is
        // why it is a subtraction here rather than a second pass through the offer.
        hits = hits.saturating_sub(if non_euclidean_shielding(state, player) {
            2
        } else {
            1
        });
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
    producer: &PlayerId,
    hits: usize,
) -> Result<(), CombatError> {
    // The windowless path: the resolver carries the table the questions go to but no timing
    // machinery, so a sustained hit announces nothing and cannot be reacted to. The dice and
    // roller are never touched on this path and exist only to satisfy the handle's shape.
    let mut dice = crate::dice::Dice::new();
    let mut rng = crate::rng::GameRng::new(0);
    let mut ctx = Resolving {
        content,
        sources,
        dice: &mut dice,
        rng: &mut rng,
        table,
        timing: None,
    };
    absorb_hits_seeing(
        state, content, sources, None, &mut ctx, player, system, producer, hits,
    )
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
    ctx: &mut Resolving<'_>,
    player: &PlayerId,
    system: &SystemId,
    producer: &PlayerId,
    hits: usize,
) -> Result<(), CombatError> {
    // Shields Holding and Maneuvering Jets friends cancel hits before any are assigned: the
    // round-scoped pool they grant into is spent here, the way the combat window's queue
    // spends it, so a cancelled cannon or barrage hit is one nobody has to absorb.
    let hits = hits.saturating_sub(spend_cancellations(state, player, hits));
    let mut remaining = offer_sustain(
        state, content, sources, galaxy, ctx, player, system, producer, hits,
    )?;

    while remaining > 0 {
        let alive = ships_of(state, content, sources, player, system);
        if alive.is_empty() {
            return Ok(()); // 15.2a
        }
        let casualty = choose_casualty(state, content, sources, galaxy, ctx.table, player, &alive)?;
        state
            .system_mut(system)
            .remove(std::slice::from_ref(&casualty));
        remaining -= 1;
    }
    Ok(())
}

/// Announce one destroyed ship, and whether it was the owner's last in the system.
///
/// Two printed windows read this: "after 1 of your ships is destroyed during a space combat", and
/// "when your last ship in the active system is destroyed". The second is not a separate moment --
/// it is the first with a fact attached -- so one event carries `last` rather than two events
/// racing to describe the same removal.
///
/// Called after the unit is off the board, so `last` is read from the position a reacting card
/// would see. The handoff [`GameState::last_ship_destroyed`] is recorded first: an effect that
/// needs to know *which* ship was destroyed cannot read the event once the window has run.
fn announce_ship_destroyed(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    system: &SystemId,
    owner: &PlayerId,
    destroyed: &Unit,
    content: &ContentStore,
    sources: SourceSet,
) {
    let remaining = ships_of(state, content, sources, owner, system).len();
    state.last_ship_destroyed = Some((system.clone(), owner.clone(), destroyed.type_id.clone()));
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("system".to_owned(), system.to_string().into());
    payload.insert("player".to_owned(), owner.to_string().into());
    payload.insert("unit".to_owned(), destroyed.type_id.to_string().into());
    payload.insert("last".to_owned(), (remaining == 0).into());
    let _ = ctx.emit(state, "SHIP_DESTROYED", payload);
}

/// The retreat is named by the declaring player, so the window guard is `actor_is_not`.
fn emit_retreat_declared(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    system: &SystemId,
    player: &PlayerId,
    round: u32,
) {
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("system".to_owned(), system.to_string().into());
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("round".to_owned(), i64::from(round).into());
    let _ = ctx.emit(state, "RETREAT_DECLARED", payload);
}

impl CombatError {
    /// Windows mid-answer only ever surface a choice failure; a combat that merely ran past
    /// the round limit is reported by the standalone driver, never by a window. The second
    /// arm maps that impossible case to an honest "no options" refusal rather than a panic.
    pub(crate) fn into_illegal_choice(self) -> IllegalChoice {
        match self {
            CombatError::IllegalChoice(error) => error,
            CombatError::Timing(error) => match error {
                crate::timing::TimingError::IllegalChoice(error) => error,
                other => IllegalChoice::NoOptions {
                    player: PlayerId::new(""),
                    prompt: other.to_string(),
                },
            },
            CombatError::Unresolved(system) => IllegalChoice::NoOptions {
                player: PlayerId::new(""),
                prompt: format!("combat in {system} did not settle"),
            },
        }
    }
}

/// Both sustain windows read the moment, and both need the unit, so the event names it.
/// The `producer` is the player whose unit or ability produced the cancelled hit — Direct
/// Hit's window only opens for the producer, not for any sustained hit on a rival's fleet.
/// The handoff [`GameState::last_sustain`] is recorded first: the effect cannot see the
/// event once the window has run.
///
/// Reflective Shielding, played in the window the emission just opened, stages hits against
/// the sustained hit's producer ("your opponent"). The window has closed by the time this
/// runs, so the victim's absorption — with its own sustain offers and loss choices — can now
/// run against the same resolver. The drain lives here rather than at the two sustain sites
/// because the combat window's queue and the absorption path both apply a sustain through
/// this one emission, and a reflective hit must answer either.
///
/// # Errors
///
/// [`CombatError::IllegalChoice`] when the decider refuses the victim's loss choice.
fn emit_sustain_used(
    state: &mut GameState,
    ctx: &mut Resolving<'_>,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    system: &SystemId,
    player: &PlayerId,
    unit: &str,
    producer: &PlayerId,
) -> Result<(), CombatError> {
    state.last_sustain = Some((
        system.clone(),
        player.clone(),
        ti4_model::id::UnitTypeId::new(unit),
        producer.clone(),
    ));
    let mut payload = std::collections::BTreeMap::new();
    payload.insert("system".to_owned(), system.to_string().into());
    payload.insert("player".to_owned(), player.to_string().into());
    payload.insert("unit".to_owned(), unit.to_owned().into());
    payload.insert("producer".to_owned(), producer.to_string().into());
    let _ = ctx.emit(state, "SUSTAIN_DAMAGE_USED", payload);
    if let Some((sys, victim, hits)) = std::mem::take(&mut state.pending_reflective_hits)
        && victim != *player
    {
        absorb_hits_seeing(
            state,
            ctx.content,
            ctx.sources,
            galaxy,
            ctx,
            &victim,
            &sys,
            player,
            hits,
        )?;
    }
    Ok(())
}

/// 78.6: the owning player chooses which of their own units dies.
pub(crate) fn choose_casualty(
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
    /// Whose roll produced these hits: the `producer` of the SUSTAIN DAMAGE use that
    /// cancels them, and what Direct Hit keys on.
    producer: PlayerId,
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
    /// Anti-fighter barrage scored a secret before ordinary space-combat dice.
    RollingAfterBarrage {
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
    /// One identity spans the barrage and resolution of this combat (61.7).
    combat_occurrence: Option<FeatOccurrence>,
    /// A timing pause that the game driver has not yet opened a scoring window for.
    pending_scoring_occurrence: Option<FeatOccurrence>,
}

impl CombatWindow {
    /// Open a combat, or finish immediately if fewer than two players have ships (78.1).
    ///
    /// The roles are anchored to the active player (LRR 13 / 29.1), not to seating order:
    /// whoever is active when the combat opens is the attacker.
    #[must_use]
    pub fn new(
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
        system: &SystemId,
    ) -> Self {
        let mut sides = combatants(state, content, sources, system);
        if sides.len() == 2
            && state
                .active
                .as_ref()
                .is_some_and(|active| &sides[1] == active)
        {
            // LRR 13: during combat the active player is the attacker. `combatants` lists
            // the sides in seating order, so move the active seat to the front when it is
            // the second of the two; a combat with no active player (a test state the
            // rules never produce) keeps the seating order.
            sides.swap(0, 1);
        }
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
                combat_occurrence: None,
                pending_scoring_occurrence: None,
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
            combat_occurrence: None,
            pending_scoring_occurrence: None,
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
        // Intercept: "your opponent cannot retreat during this round of space combat." A seat with
        // nowhere to go is not asked (78.4c), so barring is expressed as having nowhere to go.
        if retreat_barred(state, player) {
            return Vec::new();
        }
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

    #[must_use]
    pub fn take_scoring_occurrence(&mut self) -> Option<FeatOccurrence> {
        self.pending_scoring_occurrence.take()
    }

    #[must_use]
    pub fn combat_occurrence(&self) -> Option<FeatOccurrence> {
        self.combat_occurrence
    }

    fn ensure_combat_occurrence(&mut self, state: &mut GameState) -> FeatOccurrence {
        *self
            .combat_occurrence
            .get_or_insert_with(|| state.begin_feat_occurrence())
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
    #[allow(
        clippy::too_many_lines,
        reason = "one windowed step per side, read as a table"
    )]
    fn roll_round(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
        round: u32,
        run_barrage: bool,
    ) -> Result<(), CombatError> {
        let (content, sources) = (ctx.content, ctx.sources);
        if run_barrage {
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
        }

        if run_barrage && round == 1 {
            let occurrence = self.ensure_combat_occurrence(state);
            // Both barrages are rolled before either is applied (78.3), one side at a time:
            // each side's reroll windows (Agnlan Oln, Scramble Frequency) open between its
            // roll and either side's removals, and the hits are read from the possibly
            // rerolled dice only afterwards.
            let mut results: Vec<(PlayerId, usize)> = Vec::new();
            for side in [self.attacker.clone(), self.defender.clone()] {
                // "Before you roll dice for ANTI-FIGHTER BARRAGE": Waylay binds here,
                // per side, before that side's dice leave the bag. The dice are rolled
                // only after the window, so the card cannot see or shape them.
                let mut payload = std::collections::BTreeMap::new();
                payload.insert("system".to_owned(), self.system.to_string().into());
                payload.insert("player".to_owned(), side.to_string().into());
                payload.insert("round".to_owned(), i64::from(round).into());
                let _ = ctx.emit(state, "ANTI_FIGHTER_BARRAGE_STARTED", payload);
                let _ = roll_barrage_side(
                    state,
                    content,
                    sources,
                    ctx.dice,
                    ctx.rng,
                    &self.system,
                    &side,
                );
                open_reroll_windows(state, ctx, &side);
                if let Some(set) = state.reroll_staging.get(&side).cloned() {
                    let hits = staged_hits(&set);
                    if hits > 0 {
                        results.push((side.clone(), hits));
                    }
                }
                state.reroll_staging.remove(&side);
            }
            state.last_reroll_player = None;
            let feat_players = apply_barrage(
                state,
                content,
                sources,
                self.galaxy.as_ref(),
                ctx,
                &self.system,
                &self.attacker,
                &self.defender,
                &results,
                occurrence,
            )?;
            if !feat_players.is_empty() {
                self.pending_scoring_occurrence = Some(occurrence);
                self.stage = Stage::RollingAfterBarrage { round };
                return Ok(());
            }
            if self.over(state, content, sources) {
                // 78.3a: a barrage can end the fight before any combat die is rolled.
                self.stage = self.conclude(state, content, sources, round);
                return Ok(());
            }
        }

        // 78.5f: the attacker rolls everything first. 78.6: both sides' hits are computed
        // before either is absorbed. Each side's window opens before the next side's dice
        // are drawn, like the barrage's, so a reroll can empty a fleet mid-roll and end
        // the fight where it stands.
        let attacker_hits = roll_fleet_and_open(state, ctx, &self.attacker, &self.system);
        if self.over(state, content, sources) {
            self.stage = self.conclude(state, content, sources, round);
            return Ok(());
        }
        let defender_hits = roll_fleet_and_open(state, ctx, &self.defender, &self.system);
        if self.over(state, content, sources) {
            self.stage = self.conclude(state, content, sources, round);
            return Ok(());
        }

        let queue: Vec<Pending> = [
            Pending {
                player: self.defender.clone(),
                hits: attacker_hits,
                producer: self.attacker.clone(),
            },
            Pending {
                player: self.attacker.clone(),
                hits: defender_hits,
                producer: self.defender.clone(),
            },
        ]
        .into_iter()
        .filter(|pending| pending.hits > 0)
        .collect();

        self.stage = Stage::Sustaining { queue, round };
        self.settle(state, ctx)
    }

    /// Settle a freshly opened window, so a fight that is already over reports so.
    ///
    /// # Errors
    ///
    /// A Waylay casualty answer that the table rejects, or a dice source that runs dry.
    pub fn settle_open(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
    ) -> Result<(), CombatError> {
        self.settle(state, ctx)
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
    fn settle(
        &mut self,
        state: &mut GameState,
        ctx: &mut Resolving<'_>,
    ) -> Result<(), CombatError> {
        let (content, sources) = (ctx.content, ctx.sources);
        loop {
            match self.stage.clone() {
                Stage::Sustaining { queue, round } | Stage::Assigning { queue, round } => {
                    let Some(front) = queue.first().cloned() else {
                        // Both sides absorbed: the round is over.
                        if self.over(state, content, sources) || round >= MAX_ROUNDS {
                            self.stage = self.conclude(state, content, sources, round);
                            return Ok(());
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
                    // Shields Holding and friends cancel hits before any are assigned. Spent here,
                    // after the window that grants them has had its chance to fire and before the
                    // sustain offer, because a cancelled hit is one nobody has to absorb.
                    let cancelled = spend_cancellations(state, &front.player, front.hits);
                    if cancelled > 0 {
                        let mut rest = queue.clone();
                        rest[0].hits -= cancelled;
                        self.stage = match self.stage {
                            Stage::Assigning { .. } => Stage::Assigning { queue: rest, round },
                            _ => Stage::Sustaining { queue: rest, round },
                        };
                        continue;
                    }
                    // "Before you assign hits to your ships during a space combat." Emitted as the
                    // first of a player's hits is about to land: `front` still carries its full
                    // count here and the stage has consumed none of it.
                    if matches!(self.stage, Stage::Sustaining { .. }) {
                        let mut payload = std::collections::BTreeMap::new();
                        payload.insert("system".to_owned(), self.system.to_string().into());
                        payload.insert("player".to_owned(), front.player.to_string().into());
                        payload.insert(
                            "hits".to_owned(),
                            i64::try_from(front.hits).unwrap_or(0).into(),
                        );
                        payload.insert("round".to_owned(), i64::from(round).into());
                        let _ = ctx.emit(state, "HITS_TO_ASSIGN", payload);
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
                        announce_ship_destroyed(
                            state,
                            ctx,
                            &self.system,
                            &front.player,
                            &only,
                            content,
                            sources,
                        );
                        let mut rest = queue;
                        rest[0].hits -= 1;
                        self.stage = Stage::Sustaining { queue: rest, round };
                        continue;
                    }
                    return Ok(());
                }
                Stage::Announcing {
                    round,
                    asking,
                    announced,
                } => {
                    // "At the start of the 'Announce Retreats' step of space combat, if you are the
                    // defender." The defender is asked first (78.4b), so the step starts when they
                    // are the one being asked and nobody has announced yet.
                    if asking == self.defender && announced.is_empty() {
                        let mut payload = std::collections::BTreeMap::new();
                        payload.insert("system".to_owned(), self.system.to_string().into());
                        payload.insert("player".to_owned(), self.defender.to_string().into());
                        payload.insert("round".to_owned(), i64::from(round).into());
                        let _ = ctx.emit(state, "RETREAT_STEP_STARTED", payload);
                    }
                    if self.over(state, content, sources) {
                        self.stage = self.conclude(state, content, sources, round - 1);
                        return Ok(());
                    }
                    // 78.4c: a player with nowhere to go is not asked.
                    if !self.retreats(state, content, sources, &asking).is_empty() {
                        return Ok(());
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
                            return Ok(());
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
                        _ => return Ok(()),
                    }
                }
                Stage::Rolling { round } => {
                    if self.over(state, content, sources) {
                        self.stage = self.conclude(state, content, sources, round - 1);
                        return Ok(());
                    }
                    self.roll_round(state, ctx, round, true)?;
                    return Ok(());
                }
                Stage::RollingAfterBarrage { round } => {
                    self.roll_round(state, ctx, round, false)?;
                    return Ok(());
                }
                Stage::Done(_) => return Ok(()),
            }
        }
    }
}

impl Window for CombatWindow {
    #[allow(clippy::too_many_lines, reason = "one arm per stage, read as a table")]
    fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        match &self.stage {
            Stage::Done(_) | Stage::Rolling { .. } | Stage::RollingAfterBarrage { .. } => None,
            Stage::Announcing { asking, .. } => {
                if self.retreats(state, content, sources, asking).is_empty() {
                    return None; // 78.4c: nothing to retreat to, so nothing to announce
                }
                // Rout ("your opponent must announce a retreat, if able", played at the start
                // of this step by the defender): the opponent's retreat is forced, so
                // "stay" is not on the table for them this round. The defender's marker is
                // scoped to `combat_round_seq` and cannot match any later round or combat.
                let forced = asking == &self.attacker
                    && state
                        .player(&self.defender)
                        .and_then(|seat| seat.rout_round)
                        .is_some_and(|played| played == state.combat_round_seq);
                let options = if forced {
                    vec![ChoiceOption::labelled(
                        "retreat",
                        RETREAT_KIND,
                        "retreat (forced)",
                    )]
                } else {
                    vec![
                        ChoiceOption::labelled("stay", RETREAT_KIND, "stay and fight"),
                        ChoiceOption::labelled("retreat", RETREAT_KIND, "announce a retreat"),
                    ]
                };
                Some(Choice::new(
                    asking.clone(),
                    format!("announce a retreat from {}", self.system),
                    options,
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

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per answer kind, read as a table"
    )]
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
            Stage::Done(_) | Stage::Rolling { .. } | Stage::RollingAfterBarrage { .. } => {}
            Stage::Announcing {
                round,
                asking,
                mut announced,
            } => {
                if option.id == "retreat" {
                    announced.push(asking.clone());
                    // "After your opponent declares a retreat during a space combat." Named by the
                    // declaring player, so `actor_is_not` gives it to the opponent.
                    emit_retreat_declared(state, ctx, &self.system, &asking, round);
                }
                // 78.4b: the defender announcing silences the attacker.
                let next = (asking == self.defender && !announced.contains(&self.defender))
                    .then(|| self.attacker.clone());
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
                let front_player = queue
                    .first()
                    .map_or_else(|| self.defender.clone(), |front| front.player.clone());
                // Whose roll produced the hit being cancelled: Direct Hit keys on it.
                let front_producer = queue
                    .first()
                    .map_or_else(|| self.defender.clone(), |front| front.producer.clone());
                if option.is_decline() {
                    self.stage = Stage::Assigning { queue, round };
                } else if let Some(index) = option
                    .id
                    .strip_prefix("sustain|")
                    .and_then(|rest| rest.parse::<usize>().ok())
                {
                    let sustained =
                        state
                            .system_mut(&self.system)
                            .units
                            .get_mut(index)
                            .map(|unit| {
                                *unit = unit.sustained();
                                unit.type_id.to_string()
                            });
                    // Two printed windows read this moment -- "when one of your ships uses SUSTAIN
                    // DAMAGE" and "after another player's ship uses SUSTAIN DAMAGE to cancel a hit
                    // produced by your units". Both need the *unit*, so the event names it.
                    if let Some(kind) = sustained {
                        emit_sustain_used(
                            state,
                            ctx,
                            self.galaxy.as_ref(),
                            &self.system,
                            &front_player,
                            &kind,
                            &front_producer,
                        )
                        .map_err(CombatError::into_illegal_choice)?;
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
                    announce_ship_destroyed(
                        state,
                        ctx,
                        &self.system,
                        &front.player,
                        &doomed,
                        content,
                        sources,
                    );
                }
                if let Some(front) = queue.first_mut() {
                    front.hits = front.hits.saturating_sub(1);
                }
                // Back to sustaining: the next hit may be cancellable even if this one was not.
                self.stage = Stage::Sustaining { queue, round };
            }
        }
        self.settle(state, ctx)
            .map_err(CombatError::into_illegal_choice)?;
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
    let before = before_combat(state, content, sources, system);
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
    window.settle(state, &mut ctx)?;
    while window.outcome().is_none() {
        window.drive(state, &mut ctx)?;
        if window.outcome().is_some() {
            break;
        }
        // The synchronous API has no outer Game scoring window. Preserve the occurrence facts,
        // consume the pause, and continue the same combat to completion.
        let _ = window.take_scoring_occurrence();
        window.settle_open(state, &mut ctx)?;
    }
    complete_window(state, content, sources, system, &before, &window)
        .ok_or_else(|| CombatError::Unresolved(system.clone()))
}

/// Complete a driven combat window: return its outcome and record event feats at completion.
/// Both the synchronous API and the stepped test harness call this so they cannot drift apart
/// (M07-022). The Game driver keeps its own inline bookkeeping, because there a noted occurrence
/// pauses for scoring before the fight is over.
fn complete_window(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    before: &BeforeCombat,
    window: &CombatWindow,
) -> Option<CombatOutcome> {
    let outcome = window.outcome()?;
    if let Some(occurrence) = window.combat_occurrence() {
        note_combat_event_feats(
            state, content, sources, system, before, &outcome, occurrence,
        );
    }
    Some(outcome)
}

/// What a system held before a space combat, for the feats that ask what changed.
///
/// Taken before the first die because every fact in it is destroyed by the fight itself: the war
/// sun that was killed is gone, and the loser's fleet is gone with it. A card that asks "did you
/// destroy their flagship" cannot be answered from the wreckage.
#[derive(Debug, Clone, Default)]
pub struct BeforeCombat {
    /// The players with ships when the fight opened, in seating order.
    sides: Vec<PlayerId>,
    /// War suns and flagships each side had, to see which were destroyed.
    capitals: std::collections::BTreeMap<PlayerId, usize>,
    /// Whose promissory notes each side held when the tactical action started. Standalone combat
    /// callers snapshot when their combat opens because they have no enclosing tactical window.
    notes: NoteHoldings,
}

impl BeforeCombat {
    /// The players with ships when the fight opened, in seating order. A Salvage played into
    /// the win window knows its opponents only from this list — by then the losers' ships are
    /// off the board and the board itself answers with the winner alone.
    #[must_use]
    pub fn sides(&self) -> &[PlayerId] {
        &self.sides
    }
}

/// Promissory-note issuers held by each player at a defined timing boundary.
pub type NoteHoldings = std::collections::BTreeMap<PlayerId, std::collections::BTreeSet<PlayerId>>;

/// Snapshot note holdings for objectives that name the start of a tactical action.
///
/// Betray a Friend counts only notes in the holder's **play area** (faceup), not hand-held
/// notes: "'In your play area' is not the same as 'in your hand'." Support for the Throne is
/// play-area by construction, which is why it lives in `support_holders` and needs no filter.
#[must_use]
pub fn note_holdings(state: &GameState) -> NoteHoldings {
    let mut notes = NoteHoldings::new();
    for seat in &state.players {
        let mut issuers = std::collections::BTreeSet::new();
        for (note, holder) in &state.promissory_notes {
            // The key is note_id(alias, owner_faction): the suffix names the issuer's faction,
            // so the issuer is that faction's seat. A PlayerId built from the faction name
            // matches no seat and could never fire; an unseated owner resolves to nothing.
            // Unlike rival_note_issuers_count there is deliberately no own-faction guard: baf
            // tests notes[winner].contains(loser) with winner != loser, and a seat can never hold
            // its own faction's note — deal keeps each seat's own notes in hand (never face-up),
            // and factions are unique per seat, so no transfer can deliver one.
            if holder == &seat.id
                && state.promissory_faceup.contains(note)
                && let Some(owner_faction) = crate::promissory::owner_of(note)
                && let Some(issuer_seat) = crate::promissory::seat_of(state, &owner_faction)
            {
                issuers.insert(issuer_seat);
            }
        }
        for (owner, holder) in &state.support_holders {
            if holder == &seat.id {
                issuers.insert(owner.clone());
            }
        }
        notes.insert(seat.id.clone(), issuers);
    }
    notes
}

/// Whether `system` is another player's home system from `winner`'s perspective.
#[must_use]
pub fn is_rival_home_system(state: &GameState, winner: &PlayerId, system: &SystemId) -> bool {
    state
        .players
        .iter()
        .any(|seat| seat.id != *winner && seat.home_system.as_ref() == Some(system))
}

/// Snapshot a system before a space combat.
#[must_use]
pub fn before_combat(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> BeforeCombat {
    before_combat_with_notes(state, content, sources, system, note_holdings(state))
}

/// Snapshot combat-only facts while retaining note holdings from the tactical-action boundary.
#[must_use]
pub fn before_combat_with_notes(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    notes: NoteHoldings,
) -> BeforeCombat {
    let sides = combatants(state, content, sources, system);
    let mut capitals = std::collections::BTreeMap::new();
    for side in &sides {
        capitals.insert(
            side.clone(),
            capital_ships_of(state, content, sources, side, system),
        );
    }
    BeforeCombat {
        sides,
        capitals,
        notes,
    }
}

/// Record what a finished space combat did, for the secrets that ask about the event.
///
/// Six of the thirteen unimplemented secrets are decided here. None of them can be read off the
/// board afterwards, which is why they had no requirement: "win a combat in an anomaly" leaves
/// the same board as losing one somewhere else.
pub fn note_combat_feats(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    before: &BeforeCombat,
    outcome: &CombatOutcome,
) {
    let occurrence = state.begin_feat_occurrence();
    let _ = note_combat_feats_at(state, content, sources, system, before, outcome, occurrence);
}

/// Record finished-combat feats against the concrete combat that caused them.
///
/// Returns whether the resolution created at least one event feat, so callers can skip opening
/// an empty scoring window.
pub fn note_combat_event_feats(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    before: &BeforeCombat,
    outcome: &CombatOutcome,
    occurrence: FeatOccurrence,
) -> bool {
    note_combat_feats_at(state, content, sources, system, before, outcome, occurrence)
}

fn record_combat_feat(
    state: &mut GameState,
    player: &PlayerId,
    feat: Feat,
    occurrence: FeatOccurrence,
) {
    state.record_event_feat(player, feat, occurrence);
}

fn note_combat_feats_at(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    before: &BeforeCombat,
    outcome: &CombatOutcome,
    occurrence: FeatOccurrence,
) -> bool {
    if outcome.rounds == 0 {
        // Nobody fought, so nothing was won and nothing was destroyed.
        return false;
    }
    let mut noted = false;

    // Destroy Their Greatest Ship. In a two-sided fight whoever is not the owner did the
    // destroying, so a drop in one side's count is the other side's feat — including when both
    // sides lost one and both score.
    for side in &before.sides {
        let had = before.capitals.get(side).copied().unwrap_or(0);
        if had == 0 {
            continue;
        }
        if capital_ships_of(state, content, sources, side, system) < had {
            for other in &before.sides {
                if other != side {
                    record_combat_feat(state, other, Feat::DestroyedACapitalShip, occurrence);
                    noted = true;
                }
            }
        }
    }

    // Demonstrate Your Power asks about the fleet at the end of the combat and says nothing
    // about winning, so it is offered to whoever is still standing there.
    for side in &before.sides {
        if non_fighter_ships_of(state, content, sources, side, system) >= 3 {
            record_combat_feat(
                state,
                side,
                Feat::HeldThreeShipsAfterASpaceCombat,
                occurrence,
            );
            noted = true;
        }
    }

    let Some(winner) = outcome.winner.clone() else {
        // A draw wins nothing. Skilled Retreat exists to produce exactly this, and treating the
        // survivor as the winner would score four cards off a fight nobody won.
        return noted;
    };

    if ti4_content::galaxy::all_systems(content, sources)
        .get(system.as_str())
        .is_some_and(ti4_content::galaxy::System::is_anomaly)
    {
        record_combat_feat(state, &winner, Feat::WonInAnAnomaly, occurrence);
        noted = true;
    }

    if is_rival_home_system(state, &winner, system) {
        record_combat_feat(state, &winner, Feat::WonInARivalHome, occurrence);
        noted = true;
    }

    if capital_ships_of(state, content, sources, &winner, system) > 0
        && has_surviving_flagship(state, content, sources, &winner, system)
    {
        record_combat_feat(
            state,
            &winner,
            Feat::WonBesideASurvivingFlagship,
            occurrence,
        );
        noted = true;
    }

    // "The most victory points" includes a tie: nothing in the card breaks one, and the leader
    // board holding two names does not make either of them not the leader.
    let most = state
        .players
        .iter()
        .map(|seat| seat.victory_points)
        .max()
        .unwrap_or(0);
    for loser in &before.sides {
        if loser == &winner {
            continue;
        }
        if state
            .player(loser)
            .is_some_and(|seat| seat.victory_points == most)
        {
            record_combat_feat(state, &winner, Feat::WonAgainstThePointsLeader, occurrence);
            noted = true;
        }
        if before
            .notes
            .get(&winner)
            .is_some_and(|issuers| issuers.contains(loser))
        {
            record_combat_feat(state, &winner, Feat::WonAgainstANoteHolder, occurrence);
            noted = true;
        }
    }
    noted
}

/// Whether this player still has a flagship in the system.
fn has_surviving_flagship(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> bool {
    let types = catalogue(content, sources);
    state
        .system_state(system)
        .units
        .iter()
        .filter(|unit| &unit.owner == player)
        .any(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(|kind| kind.base_type() == "flagship")
        })
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

    /// PDS II fires into the next system, and PDS I does not.
    ///
    /// "You may use this unit's SPACE CANNON against ships that are in adjacent systems" is printed
    /// on PDS II and on Xxcha's Indomitus mech. `space_cannon_offense` read only the activated
    /// system, so neither ever fired -- an upgraded PDS next door, from a technology every faction
    /// can research, did nothing at all.
    #[test]
    fn an_upgraded_pds_fires_from_an_adjacent_system() {
        let hub = crate::fixtures::plain_hub();
        let active = SystemId::new(&hub.centre);
        let next_door = SystemId::new(&hub.outer[0]);
        let planet = ti4_model::id::PlanetId::new("a_planet_next_door");

        let mut state = crate::fixtures::game(&["a", "b"]);
        for id in std::iter::once(&hub.centre).chain(hub.outer.iter()) {
            state.board.entry(SystemId::new(id)).or_default();
        }
        put(&mut state, &active, "cruiser", &attacker(), 1);

        let guns =
            |state: &GameState| reaching_guns(state, Some(&hub.galaxy), &active, &attacker()).len();

        if let Some(here) = state.board.get_mut(&next_door) {
            here.planet_units
                .entry(planet.clone())
                .or_default()
                .push(Unit::new(UnitTypeId::new("pds"), defender()));
        }
        assert_eq!(guns(&state), 0, "a PDS I stays at home");

        if let Some(here) = state.board.get_mut(&next_door) {
            here.planet_units
                .entry(planet.clone())
                .or_default()
                .push(Unit::new(UnitTypeId::new("pds2"), defender()));
        }
        assert_eq!(guns(&state), 1, "an upgraded one reaches next door");

        if let Some(here) = state.board.get_mut(&next_door) {
            here.planet_units
                .entry(planet.clone())
                .or_default()
                .push(Unit::new(UnitTypeId::new("xxcha_mech"), defender()));
        }
        assert_eq!(guns(&state), 2, "and so does Xxcha's Indomitus mech");
    }

    /// Non-Euclidean Shielding cancels two hits per sustain, not two sustains.
    ///
    /// "When 1 of your units uses SUSTAIN DAMAGE, cancel 2 hits instead of 1" -- so one dreadnought
    /// absorbs two hits and takes one point of damage. A reading that cancelled two hits by
    /// damaging two ships would cost the holder twice what the technology is for.
    #[test]
    fn non_euclidean_shielding_cancels_two_hits_for_one_damaged_ship() {
        let content = ContentStore::embedded();
        let (mut state, system) = arena();
        let player = defender();
        put(&mut state, &system, "dreadnought", &player, 1);

        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);
        let mut inner = Table::with_default(Box::new(crate::choice::FirstOption));
        let mut ctx = crate::choice::Resolving {
            content,
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut inner,
            timing: None,
        };

        let left = offer_sustain(
            &mut state,
            content,
            POK,
            None,
            &mut ctx,
            &player,
            &system,
            &attacker(),
            2,
        )
        .expect("the offer resolves");
        assert_eq!(
            left, 1,
            "without the technology one sustain cancels one hit"
        );

        let (mut state, system) = arena();
        put(&mut state, &system, "dreadnought", &player, 1);
        if let Some(seat) = state.player_mut(&player) {
            seat.technologies
                .insert(ti4_model::id::TechnologyId::new("nes"));
        }
        let left = offer_sustain(
            &mut state,
            content,
            POK,
            None,
            &mut ctx,
            &player,
            &system,
            &attacker(),
            2,
        )
        .expect("the offer resolves");
        assert_eq!(left, 0, "with it, the same one ship cancels both");
        assert_eq!(
            state
                .system_state(&system)
                .units_of(&player)
                .iter()
                .filter(|unit| unit.sustained_damage)
                .count(),
            1,
            "and only one ship is damaged"
        );
    }

    /// The Thalnos cards reach combat rounds and nothing else.
    ///
    /// Both read "During each combat round". A space cannon roll is a step of a tactical action, a
    /// barrage is a step of a space combat, and a bombardment is a step of an invasion -- none of
    /// them is a round, so neither card may touch their dice. They stage through the same window as
    /// the fleet roll, which is exactly why the scope has to be checked rather than assumed.
    #[test]
    fn thalnos_does_not_reach_a_space_cannon_roll() {
        let (mut state, system) = arena();
        let player = attacker();
        put(&mut state, &system, "cruiser", &player, 1);
        if let Some(seat) = state.player_mut(&player) {
            seat.relics = vec![ti4_model::id::RelicId::new("thalnos")];
        }
        let staged = |state: &mut GameState, kind: &str| {
            state.reroll_staging.insert(
                player.clone(),
                RerollSet {
                    kind: kind.to_owned(),
                    system: system.clone(),
                    rolls: vec![RerollEntry {
                        unit: "pds".to_owned(),
                        unit_types: std::collections::BTreeMap::new(),
                        planet: None,
                        hits_on: Some(5),
                        faces: vec![1],
                        rerolled: std::collections::BTreeSet::new(),
                        deltas: std::collections::BTreeMap::new(),
                    }],
                },
            );
        };
        let mut dice = Dice::new();
        let mut rng = GameRng::new(1);
        let mut inner = Table::with_default(Box::new(crate::choice::FirstOption));
        let mut ctx = crate::choice::Resolving {
            content: ContentStore::embedded(),
            sources: POK,
            dice: &mut dice,
            rng: &mut rng,
            table: &mut inner,
            timing: None,
        };

        staged(&mut state, "space_cannon");
        reroll_window(&mut state, &mut ctx, &player, true);
        assert_eq!(
            state.reroll_staging.get(&player).expect("staged").rolls[0].faces,
            vec![1],
            "a space cannon roll is not a combat round"
        );

        // A fleet roll *is* a combat round, so the relic takes it. Either the die was rerolled or
        // the unit that rerolled and still missed was destroyed with its entry -- both prove the
        // card fired, and which one happens depends on the draw.
        staged(&mut state, "fleet");
        reroll_window(&mut state, &mut ctx, &player, false);
        let after = state.reroll_staging.get(&player).expect("staged");
        assert!(
            after
                .rolls
                .first()
                .is_none_or(|entry| entry.faces != vec![1]),
            "but a fleet roll is, and the relic takes it"
        );
    }

    /// 59.5: the defender rolls one better in a nebula, and the attacker does not.
    ///
    /// Applied to the threshold because that is where every other roll modifier lives -- +1 to the
    /// die is the same as needing one less, and keeping them in one place is what stops two
    /// modifiers disagreeing about which direction they push.
    #[test]
    fn a_nebula_helps_the_defender_only() {
        let content = ContentStore::embedded();
        let nebula = SystemId::new(crate::fixtures::a_system_where("nebula"));
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.board.entry(nebula.clone()).or_default();
        state.active = Some(attacker());
        state.active_system = Some(nebula.clone());

        let cruiser = Unit::new(UnitTypeId::new("cruiser"), attacker());
        let printed = hits_on(content, POK, &cruiser).expect("a cruiser has a combat value");

        assert_eq!(
            effective_hits_on(&state, content, POK, &attacker(), &cruiser),
            Some(printed),
            "the attacker gets nothing from the nebula"
        );
        assert_eq!(
            effective_hits_on(&state, content, POK, &defender(), &cruiser),
            Some(printed - 1),
            "the defender needs one less"
        );

        // And nowhere else: the same fleet in an ordinary system is unmodified.
        let plain = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.active_system = Some(plain);
        assert_eq!(
            effective_hits_on(&state, content, POK, &defender(), &cruiser),
            Some(printed),
            "an ordinary system helps nobody"
        );
    }

    /// Shields Holding cancels hits across the round, not per assignment.
    ///
    /// "Cancel up to 2 hits" is two hits in that combat round. Granting two and spending them one
    /// at a time must leave none, which is what `spend_cancellations` returning the smaller of
    /// wanted and available buys.
    #[test]
    fn granted_hit_cancellations_are_spent_once_each() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a", "b"]);
        state.combat_round_seq = 4;

        grant_hit_cancellation(&mut state, &player, 2);
        assert_eq!(cancellable_hits(&state, &player), 2);

        assert_eq!(spend_cancellations(&mut state, &player, 1), 1);
        assert_eq!(cancellable_hits(&state, &player), 1);
        assert_eq!(
            spend_cancellations(&mut state, &player, 5),
            1,
            "capped by what is left"
        );
        assert_eq!(cancellable_hits(&state, &player), 0);
    }

    /// A cancellation belongs to the round it was granted in.
    #[test]
    fn hit_cancellations_expire_with_their_round() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.combat_round_seq = 4;
        grant_hit_cancellation(&mut state, &player, 2);

        state.combat_round_seq = 5;
        assert_eq!(
            cancellable_hits(&state, &player),
            0,
            "the card names this combat round"
        );
    }

    /// Intercept bars a retreat for its round and no longer.
    #[test]
    fn a_barred_retreat_lasts_one_round() {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.combat_round_seq = 2;
        assert!(!retreat_barred(&state, &player));

        bar_retreat(&mut state, &player);
        assert!(retreat_barred(&state, &player));

        state.combat_round_seq = 3;
        assert!(!retreat_barred(&state, &player));
    }

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
            )
            .unwrap();

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
        )
        .unwrap();

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
            fired.unwrap().len() <= 2,
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
        )
        .unwrap();

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
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
            None,
        );

        assert_eq!(dice.count(), 1, "the gun on the planet fired");
        assert!(
            fired.iter().all(|(owner, _, _)| owner == &defender()),
            "only the non-active player shoots"
        );
    }

    #[test]
    fn the_active_players_own_guns_do_not_fire_at_them() {
        let (mut state, system) = arena();
        put(&mut state, &system, &a_cannon_unit(), &attacker(), 3);
        let (_, mut dice, mut rng) = kit();

        let fired = space_cannon_offense(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut dice,
            &mut rng,
            &system,
            &attacker(),
            None,
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
            &attacker(),
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
            &attacker(),
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
            &attacker(),
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
            &attacker(),
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
            &attacker(),
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
        window.settle(&mut state, &mut ctx).unwrap();

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
    fn the_active_player_is_the_attacker_whoever_is_seated_where() {
        // LRR 13: "During combat, the active player is the attacker." LRR 29.1: the
        // defender is the player who is not the active player. `combatants` lists the
        // two sides in seating order, so the combat must anchor the roles to the active
        // seat: with b (seated second) active, b is the attacker and a the defender --
        // not the reverse that the raw seating order gives.
        let (mut state, system) = arena();
        put(&mut state, &system, "fighter", &attacker(), 1);
        put(&mut state, &system, "fighter", &defender(), 1);

        state.active = Some(attacker().clone());
        let window = CombatWindow::new(&state, ContentStore::embedded(), POK, &system);
        assert_eq!(
            window.attacker,
            attacker(),
            "the active seat is the attacker"
        );
        assert_eq!(window.defender, defender());

        state.active = Some(defender().clone());
        let window = CombatWindow::new(&state, ContentStore::embedded(), POK, &system);
        assert_eq!(
            window.attacker,
            defender(),
            "active-second is still the attacker"
        );
        assert_eq!(window.defender, attacker());

        // A combat opened without an active player (a test state the rules never
        // produce) keeps the seating order the engine always had.
        state.active = None;
        let window = CombatWindow::new(&state, ContentStore::embedded(), POK, &system);
        assert_eq!(window.attacker, attacker());
        assert_eq!(window.defender, defender());
    }

    /// Drive a combat window by hand — `settle` / `pending_choice` / `resolve` — consuming scoring
    /// pauses exactly as the synchronous API does (M07-022). Completion bookkeeping goes through
    /// the same `complete_window` that `resolve()` calls, so this replica cannot drift from it.
    /// Returns the outcome and how many choices had been asked when the first scoring pause was
    /// consumed (`None` if no pause was ever consumed) — the ordering pin for M07-023's review Q1.
    fn stepped_fight(
        state: &mut GameState,
        system: &SystemId,
        table: &mut Table,
        dice: &mut Dice,
        rng: &mut GameRng,
    ) -> (CombatOutcome, Option<usize>) {
        let content = ContentStore::embedded();
        // The Game driver and the synchronous resolve() both snapshot before the first die
        // (M07-021).
        let before = crate::combat::before_combat(state, content, POK, system);
        let mut window = CombatWindow::new(state, content, POK, system);
        // The context keeps its own table (the original harness shape): one long-lived context
        // borrows it for the whole loop, so asking must go through a separate table. resolve()
        // uses one table because Window::drive asks through ctx.table internally; here the ask
        // table is what tests assert on via its log, and the final assertion below keeps that
        // comparison honest (M07-023 review Q2).
        let mut inner = Table::new();
        let mut ctx = crate::choice::Resolving {
            content,
            sources: POK,
            dice,
            rng,
            table: &mut inner,
            timing: None,
        };
        // How many choices had been asked when the first scoring pause was consumed (M07-023
        // review Q1): tests use this to assert that a choice came after the pause.
        let mut asks_before_pause: Option<usize> = None;
        window.settle(state, &mut ctx).unwrap();
        while window.outcome().is_none() {
            if let Some(choice) = window.pending_choice(state, content, POK) {
                let answer = table.ask(&choice).unwrap();
                window.resolve(state, &mut ctx, answer).unwrap();
            } else {
                // Mirror the synchronous API: consume scoring pauses and drive automatic
                // transitions.
                let occurrence = window.take_scoring_occurrence();
                if asks_before_pause.is_none() && occurrence.is_some() {
                    asks_before_pause = Some(table.log.records.len());
                }
                window.settle_open(state, &mut ctx).unwrap();
            }
        }
        let outcome = crate::combat::complete_window(state, content, POK, system, &before, &window)
            .expect("the fight resolved");
        // The log-equality assertions in the tests compare against the ask table only; if a
        // future fixture ever routes an internal ask through the context's table (e.g. faction
        // combat-round offers), fail informatively instead of comparing split logs (Q2).
        assert!(
            ctx.table.log.records.is_empty(),
            "the harness's context table must stay unasked: log assertions compare against the \
             ask table only"
        );
        (outcome, asks_before_pause)
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
                let (outcome, _asks_before_pause) =
                    stepped_fight(&mut state, &system, &mut table, &mut dice, &mut rng);
                (outcome, state)
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
    fn a_stepped_combat_matches_the_driven_one_across_a_barrage_pause() {
        // The same fight stepped by hand and driven synchronously must end identically even when
        // the round-1 barrage fires a feat and pauses for scoring (M07-022): the stepped side must
        // consume the pause exactly as resolve() does, or it stalls with the fight unresolved.
        let fight = |stepped: bool| {
            let (mut state, system) = arena();
            put(&mut state, &system, "destroyer", &attacker(), 1);
            put(&mut state, &system, "fighter", &defender(), 1);
            put(&mut state, &system, "cruiser", &defender(), 1);
            let mut table = Table::new();
            let mut dice = Dice::from_faces([10, 10, 10, 1]);
            let mut rng = GameRng::new(1);
            if stepped {
                let (outcome, _asks_before_pause) =
                    stepped_fight(&mut state, &system, &mut table, &mut dice, &mut rng);
                (outcome, state)
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
        // The barrage feat must be recorded on both sides — identity alone would also pass if
        // neither side had fired it.
        for state in [&stepped_state, &driven_state] {
            assert!(
                state
                    .player(&attacker())
                    .unwrap()
                    .event_feats
                    .iter()
                    .any(|(feat, _)| *feat == Feat::BarrageTookTheLastFighters),
                "the round-1 barrage feat must be recorded on both sides"
            );
        }
        assert!(stepped_state.identical(&driven_state));
    }

    #[test]
    fn a_stepped_combat_matches_the_driven_one_across_a_pause_and_assignment() {
        // The composition M07-022's review P2 left open: the fight pauses for scoring, resumes,
        // and then reaches a choice at the retained frame — the stepped driver must answer it.
        // No choice can arise before the round-1 barrage pause in this fixture (the barrage
        // stage offers none), so any recorded ask is necessarily after the pause.
        let fight = |stepped: bool| {
            let (mut state, system) = arena();
            put(&mut state, &system, "destroyer", &attacker(), 1);
            put(&mut state, &system, "fighter", &defender(), 1);
            put(&mut state, &system, "cruiser", &defender(), 2);
            let mut table = Table::new();
            // Round-1 AFB [10, 10] kills the fighter and pauses; round 1 then leaves one hit to
            // absorb across two cruisers (the assignment choice); round 2 ends the fight.
            let mut dice = Dice::from_faces([10, 10, 10, 1, 1, 10, 1]);
            let mut rng = GameRng::new(1);
            if stepped {
                let (outcome, asks_before_pause) =
                    stepped_fight(&mut state, &system, &mut table, &mut dice, &mut rng);
                (outcome, state, table.log.clone(), asks_before_pause)
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
                (outcome, state, table.log.clone(), None)
            }
        };

        let (stepped_outcome, stepped_state, stepped_log, stepped_asks_before_pause) = fight(true);
        let (driven_outcome, driven_state, driven_log, _driven_asks_before_pause) = fight(false);
        assert_eq!(stepped_outcome, driven_outcome);
        // The barrage feat must be recorded on both sides — identity alone would also pass if
        // neither side had fired it.
        for state in [&stepped_state, &driven_state] {
            assert!(
                state
                    .player(&attacker())
                    .unwrap()
                    .event_feats
                    .iter()
                    .any(|(feat, _)| *feat == Feat::BarrageTookTheLastFighters),
                "the round-1 barrage feat must be recorded on both sides"
            );
        }
        // M07-023 review Q1: the ordering this test is named for must be asserted, not argued —
        // the pause was consumed with zero choices asked before it, so every recorded ask (the
        // assignment included) came after the pause.
        assert_eq!(
            stepped_asks_before_pause,
            Some(0),
            "the fixture must pause, and no choice may be asked before the barrage pause"
        );
        // The composition P2 names: a choice at the retained frame after the pause. Both sides
        // must have been asked to assign the hit, and their decision sequences must match.
        for log in [&stepped_log, &driven_log] {
            assert!(
                log.records
                    .iter()
                    .any(|r| r.prompt == "assign a hit" && r.player == defender()),
                "the driver must resume into the casualty-assignment choice after the pause"
            );
        }
        assert_eq!(stepped_log, driven_log);
        assert!(stepped_state.identical(&driven_state));
    }

    #[test]
    fn a_driven_combat_continues_after_its_barrage_scoring_pause() {
        let (mut state, system) = arena();
        put(&mut state, &system, "destroyer", &attacker(), 1);
        put(&mut state, &system, "fighter", &defender(), 1);
        put(&mut state, &system, "cruiser", &defender(), 1);
        let mut table = Table::new();
        let mut dice = Dice::from_faces([10, 10, 10, 1]);
        let mut rng = GameRng::new(1);

        let outcome = resolve(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
        )
        .expect("the synchronous wrapper consumes its internal scoring pause");

        assert!(outcome.rounds >= 1);
        assert!(combatants(&state, ContentStore::embedded(), POK, &system).len() <= 1);
        assert!(
            state
                .player(&attacker())
                .unwrap()
                .event_feats
                .iter()
                .any(|(feat, _)| *feat == Feat::BarrageTookTheLastFighters)
        );
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
            &attacker(),
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
            &defender(),
            1,
        )
        .unwrap();

        assert!(
            combatants(&state, ContentStore::embedded(), POK, &system).is_empty(),
            "both fleets were destroyed in the same round"
        );
    }

    #[test]
    fn note_holdings_resolves_production_note_keys_to_seated_issuers() {
        // The production key is note_id(alias, owner_faction): "terraform:titans" is the
        // Titans' copy of Terraform. The issuer is the seat playing that faction — a PlayerId
        // built from the faction name matches no seat and can never fire.
        let mut state = crate::fixtures::game(&["a", "b"]);
        let content = ContentStore::embedded();
        let a = PlayerId::new("a");
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("titans");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("hacan");

        // A hand-held note never counts: the card says "in your play area", and Ceasefire is not
        // a play-area card. With only a held note, b has no issuers at all.
        crate::promissory::take(&mut state, content, &b, "cf:titans");
        let notes = note_holdings(&state);
        assert!(
            notes
                .get(&b)
                .is_none_or(std::collections::BTreeSet::is_empty),
            "a held Ceasefire is not in the play area"
        );

        // b receives a's Terraform: the corpus marks it playArea, so receipt puts it faceup in
        // b's play area — and the issuer must resolve to a's seat.
        crate::promissory::take(&mut state, content, &b, "terraform:titans");
        let notes = note_holdings(&state);
        assert_eq!(
            notes.get(&b),
            Some(&std::collections::BTreeSet::from([a.clone()])),
            "the seated Titans player is the issuer of a's note"
        );

        // A play-area note whose owner faction is not seated resolves to no issuer rather than a
        // phantom id.
        crate::promissory::take(&mut state, content, &b, "blood_pact:empyrean");
        let notes = note_holdings(&state);
        assert_eq!(
            notes.get(&b),
            Some(&std::collections::BTreeSet::from([a.clone()]))
        );
    }

    #[test]
    fn space_combat_against_a_seated_note_issuer_records_betray_a_friend() {
        // Betray a Friend: "Win a combat against a player whose promissory note you had in your
        // play area at the start of your tactical action." The winner holds the loser's note,
        // faceup in its play area.
        let hub = crate::fixtures::plain_hub();
        let system = SystemId::new(hub.centre.clone());
        let mut state = crate::fixtures::game(&["a", "b"]);
        let content = ContentStore::embedded();
        let a = PlayerId::new("a"); // titans: the issuer, and the loser
        let b = PlayerId::new("b"); // hacan: holds a's note, and wins
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("titans");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        crate::fixtures::put(&mut state, &system, "cruiser", &a, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b, 1);
        crate::promissory::take(&mut state, content, &b, "terraform:titans");

        let before = before_combat_with_notes(
            &state,
            ContentStore::embedded(),
            POK,
            &system,
            note_holdings(&state),
        );
        let occurrence = state.begin_feat_occurrence();
        let outcome = CombatOutcome {
            winner: Some(b.clone()),
            rounds: 1,
        };

        assert!(note_combat_event_feats(
            &mut state,
            ContentStore::embedded(),
            POK,
            &system,
            &before,
            &outcome,
            occurrence
        ));
        assert!(state.did_at_occurrence(&b, Feat::WonAgainstANoteHolder, occurrence));
    }

    #[test]
    fn a_note_received_after_the_snapshot_does_not_count_for_betray_a_friend() {
        // The card names the tactical action's start: a note that reaches the play area after
        // the snapshot is not one "you had" then.
        let hub = crate::fixtures::plain_hub();
        let system = SystemId::new(hub.centre.clone());
        let mut state = crate::fixtures::game(&["a", "b"]);
        let content = ContentStore::embedded();
        let a = PlayerId::new("a"); // titans: the issuer, and the loser
        let b = PlayerId::new("b"); // hacan: wins
        state.player_mut(&a).unwrap().faction = ti4_model::id::FactionId::new("titans");
        state.player_mut(&b).unwrap().faction = ti4_model::id::FactionId::new("hacan");
        crate::fixtures::put(&mut state, &system, "cruiser", &a, 1);
        crate::fixtures::put(&mut state, &system, "cruiser", &b, 1);

        let before = before_combat_with_notes(&state, content, POK, &system, note_holdings(&state));
        // The note arrives only after the snapshot.
        crate::promissory::take(&mut state, content, &b, "terraform:titans");
        assert!(state.promissory_faceup.contains("terraform:titans"));

        let occurrence = state.begin_feat_occurrence();
        let outcome = CombatOutcome {
            winner: Some(b.clone()),
            rounds: 1,
        };
        note_combat_event_feats(
            &mut state, content, POK, &system, &before, &outcome, occurrence,
        );
        assert!(
            !state.did_at_occurrence(&b, Feat::WonAgainstANoteHolder, occurrence),
            "the snapshot predates the receipt"
        );
    }
}
