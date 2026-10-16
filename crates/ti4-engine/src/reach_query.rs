//! "Who can go here": every ship on the board that could move INTO a system if that system were
//! activated for its owner, for all seats at once.
//!
//! A read-only view for clients (the system overview). It asks the engine's own reachability
//! search, [`crate::tactical::movable_into`], once per seat, so move values, anomalies, gravity
//! rifts, blockades, wormholes, extra links, Gravity Drive and the Ionian Fuel Refinery, laws and
//! action-card movement effects are exactly what the real movement step would offer. Nothing here
//! owns a decider, a dice or random stream, a decision log or storage, and the position is only
//! read: it cannot change a decision, a hash or a replay.
//!
//! What it assumes, and a client must say so:
//!
//! * **"If activated."** It ignores whose turn it is, whether the seat has a tactic token, and
//!   whether the destination holds the seat's own command token (89.1 forbids activating it; the
//!   ships could still be asked about). Each seat is answered as if it activated the system now.
//! * **Pinned ships stay home.** Ships in a system that holds their owner's own command token are
//!   not offered (58.4c) -- the destination itself excepted, since ships already there do not move.
//! * **Capacity is not checked.** Every ship is listed on its own merits; cargo, fleet pool and
//!   what a carrier could pick up en route are not part of the question.
//!
//! Only public information is read: the units on the board, command tokens, technologies (for
//! Gravity Drive), laws and active effects. No hand, no secret.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

/// One kind of ship in one origin system, with how many of it could arrive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReachShip {
    pub type_id: String,
    pub count: u32,
    /// Of `count`, how many are sustained-damage (they still move; shown for completeness).
    pub damaged: u32,
    /// Of `count`, how many arrive only by spending Gravity Drive's +1.
    pub gravity_drive: u32,
    /// Of `count`, how many arrive only by exhausting the Ionian Fuel Refinery.
    pub ionian: u32,
}

/// One seat's ships in one system that could reach the queried system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReachOrigin {
    pub owner: String,
    pub origin: String,
    pub ships: Vec<ReachShip>,
}

/// Every ship that could move into `destination`, grouped by seat and origin system (seating
/// order, then system id), with ships of one type and boost folded into one row.
#[must_use]
pub fn who_can_reach(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    destination: &SystemId,
) -> Vec<ReachOrigin> {
    let mut found = Vec::new();
    for seat in &state.players {
        let movable =
            crate::tactical::movable_into(state, content, sources, galaxy, &seat.id, destination);
        found.extend(group(&seat.id, &movable));
    }
    found
}

fn group(owner: &PlayerId, movable: &[crate::tactical::Movable]) -> Vec<ReachOrigin> {
    let mut by_origin: BTreeMap<String, BTreeMap<String, ReachShip>> = BTreeMap::new();
    for ship in movable {
        let row = by_origin
            .entry(ship.origin.to_string())
            .or_default()
            .entry(ship.unit.type_id.to_string())
            .or_insert_with(|| ReachShip {
                type_id: ship.unit.type_id.to_string(),
                count: 0,
                damaged: 0,
                gravity_drive: 0,
                ionian: 0,
            });
        row.count += 1;
        row.damaged += u32::from(ship.unit.sustained_damage);
        row.gravity_drive += u32::from(ship.gravity_drive);
        row.ionian += u32::from(ship.ionian);
    }
    by_origin
        .into_iter()
        .map(|(origin, ships)| ReachOrigin {
            owner: owner.to_string(),
            origin,
            ships: ships.into_values().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{game, plain_hub, put};
    use ti4_model::content_types::POK;

    fn pid(id: &str) -> PlayerId {
        PlayerId::new(id)
    }

    fn ask(state: &GameState, hub: &crate::fixtures::Hub, at: &str) -> Vec<ReachOrigin> {
        who_can_reach(
            state,
            ContentStore::embedded(),
            POK,
            &hub.galaxy,
            &SystemId::new(at),
        )
    }

    fn ring_state() -> (GameState, crate::fixtures::Hub) {
        let mut state = game(&["a", "b"]);
        state.board.clear();
        (state, plain_hub())
    }

    #[test]
    fn lists_own_and_enemy_ships_separately_with_counts() {
        let (mut state, hub) = ring_state();
        let near = SystemId::new(hub.outer[0].clone());
        let other = SystemId::new(hub.outer[1].clone());
        put(&mut state, &near, "cruiser", &pid("a"), 2);
        put(&mut state, &near, "destroyer", &pid("a"), 1);
        put(&mut state, &other, "cruiser", &pid("b"), 1);
        let reach = ask(&state, &hub, &hub.centre);
        let mine: Vec<_> = reach.iter().filter(|r| r.owner == "a").collect();
        let theirs: Vec<_> = reach.iter().filter(|r| r.owner == "b").collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].origin, near.to_string());
        let cruisers = mine[0]
            .ships
            .iter()
            .find(|s| s.type_id == "cruiser")
            .unwrap();
        assert_eq!(cruisers.count, 2);
        assert_eq!(mine[0].ships.len(), 2);
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].origin, other.to_string());
    }

    #[test]
    fn a_ship_under_its_owners_command_token_cannot_leave() {
        let (mut state, hub) = ring_state();
        let near = SystemId::new(hub.outer[0].clone());
        put(&mut state, &near, "cruiser", &pid("a"), 1);
        state.system_mut(&near).command_tokens.insert(pid("a"));
        assert!(ask(&state, &hub, &hub.centre).is_empty());
    }

    #[test]
    fn ships_already_in_the_system_are_not_listed_and_blockades_bar_the_way() {
        let (mut state, hub) = ring_state();
        let near = SystemId::new(hub.outer[0].clone());
        let far = SystemId::new(hub.across(near.as_str()));
        put(&mut state, &near, "cruiser", &pid("a"), 1);
        put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &pid("a"),
            1,
        );
        // The cruiser at the centre is already there; the one in the ring arrives.
        assert_eq!(ask(&state, &hub, &hub.centre).len(), 1);
        // From across the hub, a cruiser (move 2) goes through the centre unless an enemy sits there.
        put(&mut state, &far, "cruiser", &pid("a"), 1);
        let across_reach = ask(&state, &hub, &far.to_string());
        assert!(
            across_reach
                .iter()
                .any(|r| r.owner == "a" && r.origin == near.to_string())
        );
        put(
            &mut state,
            &SystemId::new(hub.centre.clone()),
            "cruiser",
            &pid("b"),
            1,
        );
        let blocked = ask(&state, &hub, &far.to_string());
        assert!(
            !blocked
                .iter()
                .any(|r| r.owner == "a" && r.origin == near.to_string())
        );
    }

    #[test]
    fn matches_the_engines_own_movable_into() {
        let (mut state, hub) = ring_state();
        for (i, id) in hub.outer.iter().enumerate() {
            let owner = if i % 2 == 0 { "a" } else { "b" };
            put(
                &mut state,
                &SystemId::new(id.clone()),
                "carrier",
                &pid(owner),
                1 + i,
            );
        }
        let reach = ask(&state, &hub, &hub.centre);
        for seat in ["a", "b"] {
            let engine: usize = crate::tactical::movable_into(
                &state,
                ContentStore::embedded(),
                POK,
                &hub.galaxy,
                &pid(seat),
                &SystemId::new(hub.centre.clone()),
            )
            .len();
            let listed: u32 = reach
                .iter()
                .filter(|r| r.owner == seat)
                .flat_map(|r| r.ships.iter())
                .map(|s| s.count)
                .sum();
            assert_eq!(listed as usize, engine, "seat {seat}");
        }
    }
}
