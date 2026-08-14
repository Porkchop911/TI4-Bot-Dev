//! Fleet supply (LRR 37) and capacity (LRR 16).
//!
//! Ported from the oracle's `engine/fleet.py`. Both limits are enforced by the *owner*
//! removing units, so both ask through a [`Table`].

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
/// Regulations holds it to four however many tokens a player has piled up. Faction abilities
/// that raise it are still unimplemented.
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
    let present = state
        .system_state(system)
        .units_of(player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(counts_against_supply)
        })
        .count();
    present.saturating_sub(usize::try_from(limit(state, content, player).max(0)).unwrap_or(0))
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
    let board = state.system_state(system);
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

    let room = (transport - carried).max(0) + support;
    usize::try_from((fighters - room).max(0)).unwrap_or(0)
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
    while over_supply(state, content, sources, player, system) > 0 {
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

    while over_capacity(state, content, sources, player, system) > 0 {
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
