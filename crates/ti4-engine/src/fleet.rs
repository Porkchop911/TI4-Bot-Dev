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
    over_supply_with(&types, state, content, sources, player, system)
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
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let Some(board) = state.board.get(system) else {
        return 0;
    };
    let present = board
        .units_of(player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(counts_against_supply)
        })
        .count();
    // Fighter II again, from the other side: fighters the capacity cannot hold are ships as far as
    // the fleet pool is concerned, so they are counted here rather than removed there.
    let carried_fighters =
        usize::try_from(fighters_over_capacity_with(types, board, player).max(0)).unwrap_or(0);
    let _ = (content, sources);
    (present + carried_fighters)
        .saturating_sub(usize::try_from(limit(state, content, player).max(0)).unwrap_or(0))
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
/// Same reason as [`over_supply_with`]: this runs for every occupied seat-and-system pair at every
/// turn end, and rebuilding the catalogue and cloning the system there dominated the cost of
/// enforcing the rule at all.
fn over_capacity_with(
    types: &BTreeMap<&str, UnitType<'_>>,
    board: &ti4_model::state::SystemState,
    player: &PlayerId,
) -> usize {
    let space = board.units_of(player);

    // Resolved once rather than looked up per sum.
    let held: Vec<UnitType<'_>> = space
        .iter()
        .filter_map(|unit| types.get(unit.type_id.as_str()).copied())
        .collect();

    let transport: i64 = held.iter().map(UnitType::capacity).sum();
    let support: i64 = board
        .planet_units
        .values()
        .flatten()
        .filter(|unit| &unit.owner == player)
        .filter_map(|unit| types.get(unit.type_id.as_str()))
        .map(UnitType::fighter_support)
        .sum();
    let carried: i64 = held
        .iter()
        .filter(|kind| kind.consumes_capacity() && !kind.is_fighter())
        .map(UnitType::capacity_cost)
        .sum();
    let fighters: i64 = held
        .iter()
        .filter(|kind| kind.is_fighter())
        .map(UnitType::capacity_cost)
        .sum();

    // 16.3 counts fighters *and* ground forces against one combined total, and 16.3a lets the
    // owner choose which of the excess to remove -- which only means anything if a ground force
    // can be the excess.
    //
    // Fighter II: "fighters in excess of your ships' capacity count against your fleet pool." Those
    // fighters are not capacity-excess at all; they become fleet-pool ships, and `over_supply`
    // charges them there. Ground forces in the overflow are still excess, because the card speaks
    // only of fighters.
    //
    // This used to subtract the ground forces from the transport first and then test the fighters
    // against what was left, so six infantry on a four-capacity carrier reported *no* excess: the
    // subtraction went negative, `max(0)` swallowed it, and there were no fighters to catch it.
    // Ground forces stranded in a space area were therefore never removed.
    //
    // A space dock's fighter support is still fighter-only (16.3, Space Dock II), which is why it
    // cannot simply be added to the transport: it excuses fighters and nothing else.
    let excused_fighters = fighters.min(support);
    let overflow = (carried + fighters - excused_fighters - transport).max(0);
    let to_fleet_pool = fighters_charged_to_fleet_pool(&held, fighters, excused_fighters, overflow);
    usize::try_from((overflow - to_fleet_pool).max(0)).unwrap_or(0)
}

/// Fighters that the fleet pool absorbs instead of capacity (Fighter II).
///
/// > Fighters in excess of your ships' capacity count against your fleet pool.
///
/// Only fighters, and only upgraded ones. A unit upgrade replaces every fighter a player owns at
/// once (90.8), so a seat's fighters are all base or all upgraded and there is no mixed case to
/// apportion. The overflow is taken from the fighters first because the ground forces in it are
/// still ordinary excess -- the card says nothing about them.
fn fighters_charged_to_fleet_pool(
    held: &[UnitType<'_>],
    fighters: i64,
    excused_fighters: i64,
    overflow: i64,
) -> i64 {
    let upgraded = held
        .iter()
        .any(|kind| kind.is_fighter() && kind.required_technology().is_some());
    if !upgraded {
        return 0;
    }
    overflow.min((fighters - excused_fighters).max(0))
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
    fighters_over_capacity_with(&types, board, player)
}

/// [`fighters_over_capacity`] with the catalogue already built and the board already borrowed.
///
/// Borrowed, not cloned: `GameState::system_state` returns a CLONE of the whole system, units and
/// planet units included, and this used to be called several times per check.
fn fighters_over_capacity_with(
    types: &BTreeMap<&str, UnitType<'_>>,
    board: &ti4_model::state::SystemState,
    player: &PlayerId,
) -> i64 {
    let held: Vec<UnitType<'_>> = board
        .units_of(player)
        .iter()
        .filter_map(|unit| types.get(unit.type_id.as_str()).copied())
        .collect();
    let transport: i64 = held.iter().map(UnitType::capacity).sum();
    let support: i64 = board
        .planet_units
        .values()
        .flatten()
        .filter(|unit| &unit.owner == player)
        .filter_map(|unit| types.get(unit.type_id.as_str()))
        .map(UnitType::fighter_support)
        .sum();
    let carried: i64 = held
        .iter()
        .filter(|kind| kind.consumes_capacity() && !kind.is_fighter())
        .map(UnitType::capacity_cost)
        .sum();
    let fighters: i64 = held
        .iter()
        .filter(|kind| kind.is_fighter())
        .map(UnitType::capacity_cost)
        .sum();
    let excused_fighters = fighters.min(support);
    let overflow = (carried + fighters - excused_fighters - transport).max(0);
    fighters_charged_to_fleet_pool(&held, fighters, excused_fighters, overflow)
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
/// **Not yet called from the game loop.** Running it every step enforces limits for every seat in
/// every system continuously, which is arguably what 58.4 says — and it changes long-standing
/// behaviour broadly: eight existing fixtures set up positions that were legal only because nobody
/// looked. That is a behavioural change large enough to move the `ti4-sim` baseline, so it belongs
/// in its own reviewed change rather than riding along with a targeted fix. See
/// `plans/BUG_2026-08-29_LEAD_FLEET_SUPPLY.md`.
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
    while over_supply_with(&types, state, content, sources, player, system) > 0 {
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
