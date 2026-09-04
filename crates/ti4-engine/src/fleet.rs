//! Fleet supply (LRR 37) and capacity (LRR 16).
//!
//! Ported from the oracle's `engine/fleet.py`. Both limits are enforced by the *owner*
//! removing units, so both ask through a [`Table`].

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::units::{UnitType, catalogue};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};

/// The choice kind for removing a unit to get back within a limit.
pub const REMOVE_KIND: &str = "remove";

/// 37.1: non-fighter ships only. 37.1a excludes anything being carried.
#[must_use]
pub fn counts_against_supply(kind: &UnitType<'_>) -> bool {
    kind.is_ship() && !kind.is_fighter() && !kind.consumes_capacity()
}

/// How many non-fighter ships this player may keep in one system.
///
/// The fleet pool is the command tokens in it, capped by any law that caps it — Fleet
/// Regulations holds it to four however many tokens a player has piled up — and then lifted by any
/// faction ability that raises it (Letnev's Armada, +2).
#[must_use]
pub fn limit(state: &GameState, content: &ContentStore, player: &PlayerId) -> i32 {
    let base = state.player(player).map_or(0, |seat| seat.fleet_tokens);
    let capped = crate::laws::fleet_pool_cap(state, base);
    // The law caps first and the ability lifts afterwards, which is the order that lets Letnev's
    // Armada mean something under Fleet Regulations rather than being erased by it.
    crate::faction_abilities::fleet_supply(state, content, player, capped)
}

/// One unit that has not been placed yet, for asking what a placement would leave behind.
///
/// A production placement puts `count` copies of one type at one destination, which is why this is
/// a single arrival rather than a list. `in_space` matters to more than bookkeeping: a ground force
/// in the space area consumes capacity, while the same ground force on a planet consumes none, and
/// a space dock on a planet adds fighter support that changes the answer without ever entering the
/// space area.
#[derive(Debug, Clone, Copy)]
pub struct Arrival<'a> {
    pub kind: UnitType<'a>,
    pub count: i64,
    /// Placed in the space area rather than on a planet.
    pub in_space: bool,
}

/// Fleet supply and transport for one seat in one system, answered together.
///
/// Together because the two limits are not independent: Fighter II moves fighters the capacity
/// cannot hold onto the fleet pool, so a ground force placed in a space area can push fighters out
/// of capacity and consume fleet supply without a single ship being produced. Answering either
/// question on its own gets that case wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Standing {
    /// Non-fighter ships this seat may keep here (37.1).
    pub fleet_limit: i64,
    /// Ships charged against that limit, including the fighters the pool carries.
    pub fleet_charged: i64,
    /// Capacity the ships present provide (16.1).
    pub transport: i64,
    /// Capacity that fighters and ground forces in the space area consume, after a structure's
    /// fighter-only support has excused what it can.
    pub consumed: i64,
    /// Fighters the fleet pool carries instead of capacity (Fighter II).
    pub fighters_charged: i64,
    /// Capacity-consuming units that cannot legally remain here (16.3).
    pub capacity_excess: i64,
}

impl Standing {
    /// Ships this seat could still add before 37.3 takes any off the board.
    ///
    /// Signed on purpose. A negative headroom is a real position -- production places units first
    /// and the limit is enforced afterwards -- and its magnitude is exactly what will be removed.
    #[must_use]
    pub const fn fleet_headroom(&self) -> i64 {
        self.fleet_limit - self.fleet_charged
    }

    /// Ships 37.3 would remove.
    #[must_use]
    pub const fn fleet_excess(&self) -> i64 {
        let over = self.fleet_charged - self.fleet_limit;
        if over < 0 { 0 } else { over }
    }

    /// Capacity still free. Never negative: what does not fit is [`Self::capacity_excess`].
    #[must_use]
    pub const fn capacity_free(&self) -> i64 {
        let free = self.transport - self.consumed;
        if free < 0 { 0 } else { free }
    }
}

/// Fleet supply and transport for one seat in one system, optionally with a unit not yet placed.
///
/// The one arithmetic behind every answer in this module. [`over_supply`], [`over_capacity`] and
/// [`fighters_over_capacity`] are expressed through it, so a preview of what a placement would
/// leave and the enforcement that later removes units cannot drift apart: there is only one
/// calculation to be right or wrong about.
#[must_use]
pub fn standing(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    arriving: Option<Arrival<'_>>,
) -> Standing {
    let types = catalogue(content, sources);
    standing_using(&types, state, content, player, system, arriving)
}

/// [`standing`] for a caller that has already built the catalogue.
///
/// A producer asking what each of its options would leave asks this once per option, and
/// `catalogue` allocates a fresh map every call. Building one map per production choice rather than
/// two per offered unit is the difference between a preview that is free and one that is not.
pub(crate) fn standing_using(
    types: &BTreeMap<&str, UnitType<'_>>,
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    system: &SystemId,
    arriving: Option<Arrival<'_>>,
) -> Standing {
    standing_with(
        types,
        state.board.get(system),
        player,
        i64::from(limit(state, content, player)).max(0),
        arriving,
    )
}

/// [`standing`] with the catalogue built, the board borrowed and the fleet limit already known.
///
/// Borrowed, not cloned: `GameState::system_state` returns a CLONE of the whole system, units and
/// planet units included, and the enforcement path asks this for every occupied seat-and-system
/// pair at every turn end.
///
/// `fleet_limit` is passed in rather than read, because the two capacity callers do not need it and
/// reading it costs a law and an ability lookup each time.
fn standing_with(
    types: &BTreeMap<&str, UnitType<'_>>,
    board: Option<&ti4_model::state::SystemState>,
    player: &PlayerId,
    fleet_limit: i64,
    arriving: Option<Arrival<'_>>,
) -> Standing {
    let space: Vec<UnitType<'_>> = board
        .map(|board| {
            board
                .units_of(player)
                .iter()
                .filter_map(|unit| types.get(unit.type_id.as_str()).copied())
                .collect()
        })
        .unwrap_or_default();
    // A unit landing on a planet never reaches the space area; it can still change the answer by
    // being a structure that supports fighters.
    let landing = arriving.filter(|arrival| !arrival.in_space);
    let arrival = arriving.filter(|arrival| arrival.in_space);
    // What the arrival adds to each sum. Written out rather than routed through a helper, which
    // would have to be generic over the catalogue's lifetime to accept these as functions.
    let (mut new_ships, mut new_transport, mut new_carried, mut new_fighters) = (0, 0, 0, 0);
    if let Some(arrival) = arrival {
        new_transport = arrival.kind.capacity() * arrival.count;
        if counts_against_supply(&arrival.kind) {
            new_ships = arrival.count;
        }
        if arrival.kind.is_fighter() {
            new_fighters = arrival.kind.capacity_cost() * arrival.count;
        } else if arrival.kind.consumes_capacity() {
            new_carried = arrival.kind.capacity_cost() * arrival.count;
        }
    }

    let present = i64::try_from(
        space
            .iter()
            .filter(|kind| counts_against_supply(kind))
            .count(),
    )
    .unwrap_or(i64::MAX)
        + new_ships;
    let transport: i64 = space.iter().map(UnitType::capacity).sum::<i64>() + new_transport;
    let carried: i64 = space
        .iter()
        .filter(|kind| kind.consumes_capacity() && !kind.is_fighter())
        .map(UnitType::capacity_cost)
        .sum::<i64>()
        + new_carried;
    let fighters: i64 = space
        .iter()
        .filter(|kind| kind.is_fighter())
        .map(UnitType::capacity_cost)
        .sum::<i64>()
        + new_fighters;
    let support: i64 = board
        .map(|board| {
            board
                .planet_units
                .values()
                .flatten()
                .filter(|unit| &unit.owner == player)
                .filter_map(|unit| types.get(unit.type_id.as_str()))
                .map(UnitType::fighter_support)
                .sum()
        })
        .map_or(0, |support: i64| support)
        + landing.map_or(0, |arrival| arrival.kind.fighter_support() * arrival.count);

    // 16.3 counts fighters *and* ground forces against one combined total, and 16.3a lets the
    // owner choose which of the excess to remove -- which only means anything if a ground force
    // can be the excess.
    //
    // This used to subtract the ground forces from the transport first and then test the fighters
    // against what was left, so six infantry on a four-capacity carrier reported *no* excess: the
    // subtraction went negative, `max(0)` swallowed it, and there were no fighters to catch it.
    // Ground forces stranded in a space area were therefore never removed.
    //
    // A space dock's fighter support is still fighter-only (16.3, Space Dock II), which is why it
    // cannot simply be added to the transport: it excuses fighters and nothing else.
    let excused = fighters.min(support);
    let consumed = carried + fighters - excused;
    let overflow = (consumed - transport).max(0);
    let upgraded = space
        .iter()
        .chain(arrival.as_ref().map(|arrival| &arrival.kind))
        .any(|kind| kind.is_fighter() && kind.required_technology().is_some());
    let fighters_charged = fighters_charged_to_fleet_pool(upgraded, fighters, excused, overflow);
    Standing {
        fleet_limit,
        // Fighter II from the other side: fighters the capacity cannot hold are ships as far as the
        // fleet pool is concerned, so they are counted here rather than removed there.
        fleet_charged: present + fighters_charged,
        transport,
        consumed,
        fighters_charged,
        capacity_excess: (overflow - fighters_charged).max(0),
    }
}

/// Ships beyond the cap in this system, if any.
#[must_use]
pub fn over_supply(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    over_supply_with(&types, state, content, player, system)
}

/// [`over_supply`] with the unit catalogue already built.
///
/// `catalogue` walks the content store and allocates a fresh `BTreeMap` on every call, and the
/// enforcement path used to call it four or five times for a single seat-and-system pair. With six
/// seats and every system checked at each turn end that was roughly a thousand map builds a turn,
/// all to answer a question that is usually "nothing to do".
fn over_supply_with(
    types: &BTreeMap<&str, UnitType<'_>>,
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let standing = standing_with(
        types,
        state.board.get(system),
        player,
        i64::from(limit(state, content, player)).max(0),
        None,
    );
    usize::try_from(standing.fleet_excess()).unwrap_or(0)
}

/// Capacity-consuming units that cannot legally remain in this space area (16.3).
///
/// Ship capacity carries fighters *and* ground forces together; a space dock's fighter support
/// is a separate, fighter-only exemption, which is why the two are not simply summed.
#[must_use]
pub fn over_capacity(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    let Some(board) = state.board.get(system) else {
        return 0;
    };
    over_capacity_with(&types, board, player)
}

/// [`over_capacity`] with the catalogue already built and the board already borrowed.
///
/// The fleet limit is irrelevant to a capacity answer, so this does not pay to look it up.
fn over_capacity_with(
    types: &BTreeMap<&str, UnitType<'_>>,
    board: &ti4_model::state::SystemState,
    player: &PlayerId,
) -> usize {
    usize::try_from(standing_with(types, Some(board), player, 0, None).capacity_excess).unwrap_or(0)
}

/// Fighters that the fleet pool absorbs instead of capacity (Fighter II).
///
/// > Fighters in excess of your ships' capacity count against your fleet pool.
///
/// Only fighters, and only upgraded ones. A unit upgrade replaces every fighter a player owns at
/// once (90.8), so a seat's fighters are all base or all upgraded and there is no mixed case to
/// apportion. The overflow is taken from the fighters first because the ground forces in it are
/// still ordinary excess -- the card says nothing about them.
const fn fighters_charged_to_fleet_pool(
    upgraded: bool,
    fighters: i64,
    excused_fighters: i64,
    overflow: i64,
) -> i64 {
    if !upgraded {
        return 0;
    }
    let unexcused = fighters - excused_fighters;
    let cap = if unexcused < 0 { 0 } else { unexcused };
    if overflow < cap { overflow } else { cap }
}

/// Fighters this player has in a system that the fleet pool must carry (Fighter II).
#[must_use]
pub fn fighters_over_capacity(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> i64 {
    let types = catalogue(content, sources);
    let Some(board) = state.board.get(system) else {
        return 0;
    };
    standing_with(&types, Some(board), player, 0, None).fighters_charged
}

/// 37.3 and 16.3: the owner chooses and removes units until within the limit.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn enforce(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
) -> Result<usize, IllegalChoice> {
    enforce_seeing(state, content, sources, None, table, player, system)
}

/// Enforce fleet and capacity limits everywhere, for every seat.
///
/// Both `while` loops in [`enforce_seeing`] fall through when a seat is inside its limits, so this
/// is cheap on the overwhelmingly common step where nothing changed and asks no question.
///
/// It exists because the fleet pool can shrink far from any movement: Fleet Regulations caps it,
/// Clandestine Operations returns tokens from it, and a token-cost objective spends from it. Each of
/// those sites lacks a decider, and a limit that is only checked where ships move leaves ships on
/// the board that the rules have already removed.
///
/// **Called at every turn end** from `Game::advance_turn`, which is what makes a produced unit's
/// fleet and capacity aftermath a real consequence rather than a position nobody looks at. Running
/// it enforces limits for every seat in every system, which is what 58.4 says, and it changed
/// long-standing behaviour: eight fixtures set up positions that were legal only because nothing
/// checked. See `plans/BUG_2026-08-29_LEAD_FLEET_SUPPLY.md`.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers a casualty choice with something not offered.
pub fn enforce_everywhere(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
) -> Result<usize, IllegalChoice> {
    let seats: Vec<PlayerId> = state.players.iter().map(|seat| seat.id.clone()).collect();
    let systems: Vec<SystemId> = state.board.keys().cloned().collect();
    let mut removed = 0;
    for player in &seats {
        for system in &systems {
            removed += enforce_seeing(state, content, sources, galaxy, table, player, system)?;
        }
    }
    Ok(removed)
}

/// Enforce fleet and capacity limits while exposing the public position to learned deciders.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
#[allow(
    clippy::too_many_arguments,
    reason = "limit enforcement needs the position, optional map and owning decision table"
)]
pub fn enforce_seeing(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
) -> Result<usize, IllegalChoice> {
    let types = catalogue(content, sources);
    let mut removed = 0;

    // Supply first: removing a carrier can strand fighters, so capacity is judged after.
    while over_supply_with(&types, state, content, player, system) > 0 {
        let candidates: Vec<Unit> = state
            .system_state(system)
            .units_of(player)
            .into_iter()
            .filter(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(counts_against_supply)
            })
            .cloned()
            .collect();
        if candidates.is_empty() {
            break;
        }
        remove_one(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            system,
            &candidates,
            "fleet supply",
        )?;
        removed += 1;
    }

    while state
        .board
        .get(system)
        .is_some_and(|board| over_capacity_with(&types, board, player) > 0)
    {
        let candidates: Vec<Unit> = state
            .system_state(system)
            .units_of(player)
            .into_iter()
            .filter(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(UnitType::consumes_capacity)
            })
            .cloned()
            .collect();
        if candidates.is_empty() {
            break;
        }
        remove_one(
            state,
            content,
            sources,
            galaxy,
            table,
            player,
            system,
            &candidates,
            "capacity",
        )?;
        removed += 1;
    }
    Ok(removed)
}

/// Ask the owner which of `candidates` to remove, offering each distinguishable unit once.
fn remove_one(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    table: &mut Table,
    player: &PlayerId,
    system: &SystemId,
    candidates: &[Unit],
    reason: &str,
) -> Result<(), IllegalChoice> {
    let mut seen = std::collections::BTreeSet::new();
    let mut options = Vec::new();
    for (index, unit) in candidates.iter().enumerate() {
        if !seen.insert((unit.type_id.to_string(), unit.sustained_damage)) {
            continue;
        }
        options.push(ChoiceOption::labelled(
            format!("remove|{index}"),
            REMOVE_KIND,
            format!("remove {}", unit.type_id),
        ));
    }
    let choice = Choice::new(
        player.clone(),
        format!("remove a unit: over {reason} in {system}"),
        options,
    );
    let answer = table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
    let index = answer
        .id
        .strip_prefix("remove|")
        .and_then(|rest| rest.parse::<usize>().ok())
        .unwrap_or(0);
    let doomed = candidates.get(index).unwrap_or(&candidates[0]).clone();
    state
        .system_mut(system)
        .remove(std::slice::from_ref(&doomed));
    Ok(())
}

#[cfg(test)]
mod tests {

    /// Both halves of Fighter II, which only became reachable when unit upgrades started applying.
    ///
    /// "This unit may move without being transported" is a move value and comes from the corpus.
    /// "Fighters in excess of your ships' capacity count against your fleet pool" is the half that
    /// needed writing: such a fighter is not removed by capacity, it is charged to the pool, and it
    /// is removed only if the pool cannot hold it either.
    #[test]
    fn fighter_two_moves_alone_and_spills_onto_the_fleet_pool() {
        let content = ContentStore::embedded();
        let types = catalogue(content, POK);
        let plain = types.get("fighter").copied().expect("a fighter");
        let upgraded = types.get("fighter2").copied().expect("Fighter II");

        // Clause 1 -- "this unit may move without being transported" -- is a move value, and it is
        // in the corpus. Movement offers any ship with a move value, so this half works.
        assert_eq!(plain.move_value(), 0, "a base fighter cannot move itself");
        assert!(upgraded.move_value() > 0, "Fighter II can");

        // Clause 2 -- "fighters in excess of your ships' capacity count against your fleet pool".
        // A fighter is never an ordinary fleet-pool ship; it is charged there only when capacity
        // cannot hold it, which is what the two helpers below measure.
        assert!(!counts_against_supply(&upgraded));

        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "fighter2", &player, 2);

        // Clause 2 now works from the capacity side: with no carrier, neither Fighter II is
        // *removed* -- they are charged to the fleet pool instead.
        assert_eq!(
            over_capacity(&state, content, POK, &player, &system),
            0,
            "an excess Fighter II is not capacity-excess"
        );
        assert_eq!(
            fighters_over_capacity(&state, content, POK, &player, &system),
            2,
            "both are carried by the fleet pool"
        );

        // And from the supply side: three fleet tokens hold them, one does not.
        if let Some(seat) = state.player_mut(&player) {
            seat.fleet_tokens = 3;
        }
        assert_eq!(
            over_supply(&state, content, POK, &player, &system),
            0,
            "a fleet pool of three carries two loose fighters"
        );
        if let Some(seat) = state.player_mut(&player) {
            seat.fleet_tokens = 1;
        }
        assert_eq!(
            over_supply(&state, content, POK, &player, &system),
            1,
            "a fleet pool of one does not, and the excess is removed as a ship would be"
        );

        // A *base* fighter is untouched by any of this: it is removed by capacity, as before.
        let mut plain_state = crate::fixtures::game(&["a"]);
        plain_state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut plain_state, &system, "fighter", &player, 2);
        assert_eq!(
            over_capacity(&plain_state, content, POK, &player, &system),
            2,
            "without the upgrade an unsupported fighter is still excess"
        );
        assert_eq!(
            fighters_over_capacity(&plain_state, content, POK, &player, &system),
            0,
            "and the fleet pool carries nothing for it"
        );
    }

    /// 16.3c's second sentence is **not** enforced: excess is settled before combat, not after.
    ///
    /// `over_capacity` answers correctly -- the assertions below are about the predicate, and they
    /// pass. What is missing is a *caller* after the shooting: `enforce_seeing` runs before combat
    /// (16.3c's first sentence, "do not count against capacity during combat") and after
    /// production, and nowhere else. So a carrier destroyed in combat leaves its fighters and
    /// ground forces standing in the space area, and a stranded ground force can still invade.
    ///
    /// Wiring it at the end of combat was tried and reverted: Crash Landing and three other cards
    /// place or move units *during* combat, from windows that settle after the combat window
    /// closes, and enforcing before those windows resolve removes units the cards are about to
    /// rescue. Getting it right means ordering enforcement after every combat-triggered window,
    /// which is its own change with its own reviewed ordering -- not a line added here.
    ///
    /// Recorded in `engine-rules-audit.md` under Capacity.
    #[test]
    ///
    fn a_dead_carrier_is_reported_as_leaving_excess_behind() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "carrier", &player, 1);
        crate::fixtures::put(&mut state, &system, "fighter", &player, 4);
        assert_eq!(
            over_capacity(&state, content, POK, &player, &system),
            0,
            "four fighters ride a carrier legally"
        );

        // The carrier dies, as it would to combat hits.
        if let Some(here) = state.board.get_mut(&system) {
            let at = here
                .units
                .iter()
                .position(|unit| unit.type_id.as_str() == "carrier")
                .expect("the carrier is there");
            here.units.remove(at);
        }
        assert_eq!(
            over_capacity(&state, content, POK, &player, &system),
            4,
            "and with it gone all four are excess"
        );
    }

    /// 16.3 counts fighters *and* ground forces against the combined capacity.
    ///
    /// One carrier (capacity 4) and six infantry in the space area is two units over. The rule
    /// names both kinds together -- "more fighters and ground forces ... than the total capacity"
    /// -- and 16.3a lets the owner choose which of the excess to remove, which only makes sense if
    /// ground forces can be the excess.
    #[test]
    fn ground_forces_in_space_count_against_capacity() {
        let content = ContentStore::embedded();
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        state.board.entry(system.clone()).or_default();
        crate::fixtures::put(&mut state, &system, "carrier", &player, 1);
        crate::fixtures::put(&mut state, &system, "infantry", &player, 6);

        assert_eq!(
            over_capacity(&state, content, POK, &player, &system),
            2,
            "a carrier carries four, so two of the six infantry cannot stay"
        );
    }
    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    use super::*;
    use crate::setup::start_game;

    fn arena() -> (GameState, SystemId, PlayerId) {
        let player = PlayerId::new("a");
        let state = start_game(
            ContentStore::embedded(),
            std::slice::from_ref(&player),
            POK,
            None,
        )
        .unwrap();
        (state, SystemId::new("18"), player)
    }

    fn put(state: &mut GameState, system: &SystemId, kind: &str, owner: &PlayerId, n: usize) {
        for _ in 0..n {
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new(kind), owner.clone()));
        }
    }

    #[test]
    fn fighters_do_not_count_against_fleet_supply() {
        // 37.1: non-fighter ships only.
        let (mut state, system, player) = arena();
        state.player_mut(&player).unwrap().fleet_tokens = 1;
        put(&mut state, &system, "fighter", &player, 6);
        put(&mut state, &system, "carrier", &player, 1);

        assert_eq!(
            over_supply(&state, ContentStore::embedded(), POK, &player, &system),
            0,
            "one carrier fits a supply of one, and fighters are not counted"
        );
    }

    #[test]
    fn fleet_regulations_tightens_the_supply() {
        let (mut state, system, player) = arena();
        state.player_mut(&player).unwrap().fleet_tokens = 8;
        put(&mut state, &system, "cruiser", &player, 6);
        assert_eq!(
            over_supply(&state, ContentStore::embedded(), POK, &player, &system),
            0,
            "eight tokens hold six ships"
        );

        state.enact_law("regulations", "for");
        assert_eq!(
            over_supply(&state, ContentStore::embedded(), POK, &player, &system),
            2,
            "the law caps the pool at four"
        );
    }

    #[test]
    fn ships_beyond_the_fleet_pool_are_over_supply() {
        let (mut state, system, player) = arena();
        state.player_mut(&player).unwrap().fleet_tokens = 2;
        put(&mut state, &system, "cruiser", &player, 5);

        assert_eq!(
            over_supply(&state, ContentStore::embedded(), POK, &player, &system),
            3
        );
    }

    #[test]
    fn enforcing_supply_removes_until_within_the_limit() {
        let (mut state, system, player) = arena();
        state.player_mut(&player).unwrap().fleet_tokens = 2;
        put(&mut state, &system, "cruiser", &player, 5);
        let mut table = Table::new();

        let removed = enforce(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player,
            &system,
        )
        .unwrap();

        assert_eq!(removed, 3);
        assert_eq!(
            over_supply(&state, ContentStore::embedded(), POK, &player, &system),
            0
        );
    }

    #[test]
    fn fighters_beyond_capacity_are_removed() {
        // 16.3: a carrier holds so many, and the rest cannot stay in space.
        let (mut state, system, player) = arena();
        state.player_mut(&player).unwrap().fleet_tokens = 9;
        put(&mut state, &system, "carrier", &player, 1);
        put(&mut state, &system, "fighter", &player, 9);

        let over = over_capacity(&state, ContentStore::embedded(), POK, &player, &system);
        assert!(over > 0, "nine fighters exceed one carrier");

        let mut table = Table::new();
        enforce(
            &mut state,
            ContentStore::embedded(),
            POK,
            &mut table,
            &player,
            &system,
        )
        .unwrap();
        assert_eq!(
            over_capacity(&state, ContentStore::embedded(), POK, &player, &system),
            0
        );
    }

    #[test]
    fn ground_forces_and_fighters_share_one_hold() {
        // Ship capacity carries both, so troops aboard leave less room for fighters.
        let (mut state, system, player) = arena();
        put(&mut state, &system, "carrier", &player, 1);
        put(&mut state, &system, "fighter", &player, 4);
        let alone = over_capacity(&state, ContentStore::embedded(), POK, &player, &system);

        put(&mut state, &system, "infantry", &player, 2);
        let shared = over_capacity(&state, ContentStore::embedded(), POK, &player, &system);

        assert!(
            shared > alone,
            "troops in the hold squeeze the fighters out"
        );
    }

    #[test]
    fn a_space_dock_supports_fighters_without_using_ship_capacity() {
        // 16.2, and it is fighter-only — which is why dock support is not simply added to
        // transport capacity.
        let (mut state, system, player) = arena();
        put(&mut state, &system, "fighter", &player, 3);
        let unsupported = over_capacity(&state, ContentStore::embedded(), POK, &player, &system);
        assert!(unsupported > 0);

        state
            .system_mut(&system)
            .planet_units
            .entry(ti4_model::id::PlanetId::new("mecatol_rex"))
            .or_default()
            .push(Unit::new(UnitTypeId::new("spacedock"), player.clone()));

        assert!(
            over_capacity(&state, ContentStore::embedded(), POK, &player, &system) < unsupported,
            "the dock took some of them"
        );
    }

    #[test]
    fn an_empty_system_is_within_every_limit() {
        let (state, system, player) = arena();
        assert_eq!(
            over_supply(&state, ContentStore::embedded(), POK, &player, &system),
            0
        );
        assert_eq!(
            over_capacity(&state, ContentStore::embedded(), POK, &player, &system),
            0
        );
    }

    /// The one arithmetic answers what enforcement answers, and an arrival counts as the unit.
    ///
    /// The second half is what every production preview rests on: asking about a unit that has not
    /// been placed must give the same answer as placing it and asking again. Anything less makes a
    /// preview a second implementation of the rule, free to drift from the one that removes units
    /// at the end of the turn.
    #[test]
    fn obs008c2b_standing_matches_enforcement_and_counts_an_arrival_as_a_unit() {
        use super::*;
        use ti4_model::content_types::POK;

        let content = ContentStore::embedded();
        let types = catalogue(content, POK);
        let player = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        let mut state = crate::fixtures::game(&["a"]);
        state.board.entry(system.clone()).or_default();
        // Two Fighter IIs and nothing to carry them: the pool takes them, so both limits are
        // engaged at once and a wrong split between the two cannot pass unnoticed.
        crate::fixtures::put(&mut state, &system, "fighter2", &player, 2);
        if let Some(seat) = state.player_mut(&player) {
            seat.fleet_tokens = 1;
        }

        let now = standing(&state, content, POK, &player, &system, None);
        assert_eq!(
            usize::try_from(now.capacity_excess).unwrap(),
            over_capacity(&state, content, POK, &player, &system),
        );
        assert_eq!(
            now.fighters_charged,
            fighters_over_capacity(&state, content, POK, &player, &system),
        );
        assert_eq!(
            usize::try_from(now.fleet_excess()).unwrap(),
            over_supply(&state, content, POK, &player, &system),
        );
        assert!(
            now.fighters_charged > 0 && now.fleet_excess() > 0,
            "the fixture engages both limits: {now:?}"
        );

        // An infantry that has not been placed yet, against the same infantry placed for real.
        let infantry = types.get("infantry").copied().expect("infantry");
        let previewed = standing(
            &state,
            content,
            POK,
            &player,
            &system,
            Some(Arrival {
                kind: infantry,
                count: 2,
                in_space: true,
            }),
        );
        let mut in_space = state.clone();
        crate::fixtures::put(&mut in_space, &system, "infantry", &player, 2);
        assert_eq!(
            previewed,
            standing(&in_space, content, POK, &player, &system, None),
            "an arrival in the space area counts exactly as the placed unit"
        );

        // And on a planet, where the same arrival consumes nothing but a structure can still
        // change the answer by supporting fighters.
        let dock = types.get("spacedock").copied().expect("space dock");
        let previewed = standing(
            &state,
            content,
            POK,
            &player,
            &system,
            Some(Arrival {
                kind: dock,
                count: 1,
                in_space: false,
            }),
        );
        let mut landed = state.clone();
        crate::fixtures::put_on_planet(&mut landed, &system, &planet, "spacedock", &player, 1);
        assert_eq!(
            previewed,
            standing(&landed, content, POK, &player, &system, None),
            "an arrival on a planet counts exactly as the placed structure"
        );
    }
}

#[cfg(test)]
mod obs_review_super_dreadnought {
    use super::*;
    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;

    /// L1Z1X's Super Dreadnought carries two, and the engine's capacity arithmetic must use that.
    ///
    /// Reported from play as "super dreadnought seems not implemented as 2 capacity". Content gives
    /// `l1z1x_dreadnought` capacityValue 2 against 1 for the generic `dreadnought`, so a wrong
    /// answer here means capacity was read from the base type rather than the faction unit.
    #[test]
    fn a_super_dreadnought_carries_two_fighters() {
        let content = ContentStore::embedded();
        let types = ti4_content::units::catalogue(content, POK);
        assert_eq!(
            types.get("dreadnought").expect("generic").capacity(),
            1,
            "the generic dreadnought carries one"
        );
        assert_eq!(
            types.get("l1z1x_dreadnought").expect("super").capacity(),
            2,
            "the super dreadnought carries two"
        );

        let player = PlayerId::new("a");
        let system = SystemId::new("01");
        let mut state = GameState::new(
            std::slice::from_ref(&player),
            &[],
            std::collections::BTreeMap::new(),
            None,
            0,
        );
        let board = state.system_mut(&system);
        board.units.push(Unit::new(
            UnitTypeId::new("l1z1x_dreadnought"),
            player.clone(),
        ));
        for _ in 0..2 {
            board
                .units
                .push(Unit::new(UnitTypeId::new("fighter"), player.clone()));
        }

        assert_eq!(
            over_capacity(&state, content, POK, &player, &system),
            0,
            "two fighters fit in a super dreadnought"
        );

        state
            .system_mut(&system)
            .units
            .push(Unit::new(UnitTypeId::new("fighter"), player.clone()));
        assert_eq!(
            over_capacity(&state, content, POK, &player, &system),
            1,
            "the third fighter does not fit"
        );
    }
}
