//! Space station control (Thunder's Edge).
//!
//! A space station is listed in its system's `planets` array and carries a planet card, but it is
//! not a planet and it is not taken like one. The rules:
//!
//! > **2.** A player gains control of a space station when they are the only player with units in
//! > that space station's system.
//! >
//! > **2a.** If another player moves ships into that system, they will not gain control of that
//! > space station unless they win the resulting space combat.
//! >
//! > **2b.** If the player who controls the space station moves their ships out of the system, they
//! > retain control of that space station until another player moves ships in.
//!
//! Control is therefore a *function of occupancy*, re-evaluated whenever occupancy changes, rather
//! than an event fired by an invasion. The three clauses collapse into one rule with one exception:
//!
//! - exactly one player has units in the system → that player controls every station in it;
//! - anyone else → control is left exactly as it was.
//!
//! That single "leave it alone" branch covers both 2a and 2b. Two players present is a contested
//! system whose combat has not yet resolved, so the previous holder keeps it (2a). Nobody present
//! is the holder having moved out, so the holder keeps it (2b). Re-running this after combat
//! removes the loser's units is what finally transfers it, with no combat-specific code.
//!
//! [`reconcile`] is idempotent and cheap: it looks only at systems that actually contain a station,
//! of which the corpus has four.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;

/// The stations printed in one system, if any.
#[must_use]
pub fn stations_in(content: &ContentStore, sources: SourceSet, system: &SystemId) -> Vec<PlanetId> {
    ti4_content::galaxy::system(content, system.as_str(), sources).map_or_else(Vec::new, |tile| {
        tile.planets()
            .into_iter()
            .filter(|planet| ti4_content::galaxy::is_space_station(content, planet, sources))
            .map(|planet| PlanetId::new(planet.to_owned()))
            .collect()
    })
}

/// Everyone holding a unit anywhere in this system, in space or on a planet.
///
/// "Units", not "ships": rule 2 says units, and a seat that left ground forces on a planet in the
/// system is still present there. Ships alone would hand a station to a passing fleet over the head
/// of the player actually holding the ground.
fn occupants(state: &GameState, system: &SystemId) -> std::collections::BTreeSet<PlayerId> {
    let record = state.system_state(system);
    record
        .units
        .iter()
        .chain(record.planet_units.values().flatten())
        .map(|unit| unit.owner.clone())
        .collect()
}

/// Re-evaluate station control in one system. Returns whether anything changed.
pub fn reconcile(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> bool {
    let stations = stations_in(content, sources, system);
    if stations.is_empty() {
        return false;
    }
    let here = occupants(state, system);
    // Not exactly one occupant: contested (2a) or vacated (2b). Either way the holder keeps it.
    let mut sole = here.into_iter();
    let (Some(owner), None) = (sole.next(), sole.next()) else {
        return false;
    };

    let mut changed = false;
    for station in stations {
        let record = state.system_mut(system);
        if record.planet_control.get(&station) == Some(&owner) {
            continue;
        }
        record.set_control(station, owner.clone());
        changed = true;
    }
    changed
}

/// Re-evaluate station control everywhere on the board. Returns whether anything changed.
///
/// Called after every game step rather than at each of the dozen places a unit can move or die.
/// Occupancy is derived state, so recomputing it is always correct, and doing it in one place means
/// a future movement path cannot forget to.
pub fn reconcile_all(state: &mut GameState, content: &ContentStore, sources: SourceSet) -> bool {
    let systems: Vec<SystemId> = state.board.keys().cloned().collect();
    let mut changed = false;
    for system in systems {
        changed |= reconcile(state, content, sources, &system);
    }
    changed
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;
    use ti4_model::units::Unit;

    use super::*;

    const WATCHTOWER_SYSTEM: &str = "117";
    const WATCHTOWER: &str = "thewatchtower";

    fn ship(owner: &PlayerId) -> Unit {
        Unit::new(ti4_model::id::UnitTypeId::new("carrier"), owner.clone())
    }

    fn setup() -> (GameState, PlayerId, PlayerId, SystemId) {
        let state = crate::fixtures::game(&["a", "b"]);
        (
            state,
            PlayerId::new("a"),
            PlayerId::new("b"),
            SystemId::new(WATCHTOWER_SYSTEM),
        )
    }

    fn holder(state: &GameState, system: &SystemId) -> Option<PlayerId> {
        state
            .system_state(system)
            .planet_control
            .get(&PlanetId::new(WATCHTOWER))
            .cloned()
    }

    #[test]
    fn the_only_player_in_the_system_gains_the_station() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, a, _b, system) = setup();
        state.system_mut(&system).units.push(ship(&a));

        assert!(reconcile(&mut state, content, ALL_SOURCES, &system));
        assert_eq!(holder(&state, &system), Some(a), "rule 2");
    }

    #[test]
    fn a_second_player_arriving_does_not_take_it() {
        // Rule 2a: they gain it only by winning the space combat, which has not happened yet.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, a, b, system) = setup();
        state.system_mut(&system).units.push(ship(&a));
        reconcile(&mut state, content, ALL_SOURCES, &system);

        state.system_mut(&system).units.push(ship(&b));
        reconcile(&mut state, content, ALL_SOURCES, &system);
        assert_eq!(
            holder(&state, &system),
            Some(a),
            "two occupants means the holder keeps it until combat resolves"
        );
    }

    #[test]
    fn winning_the_combat_transfers_it() {
        // The same reconcile call, run once the loser's units are gone. No combat-specific path.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, a, b, system) = setup();
        state.system_mut(&system).units.push(ship(&a));
        reconcile(&mut state, content, ALL_SOURCES, &system);
        state.system_mut(&system).units.push(ship(&b));
        reconcile(&mut state, content, ALL_SOURCES, &system);

        state
            .system_mut(&system)
            .units
            .retain(|unit| unit.owner == b);
        assert!(reconcile(&mut state, content, ALL_SOURCES, &system));
        assert_eq!(holder(&state, &system), Some(b), "rule 2a, resolved");
    }

    #[test]
    fn moving_out_keeps_it() {
        // Rule 2b: control survives an empty system.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, a, _b, system) = setup();
        state.system_mut(&system).units.push(ship(&a));
        reconcile(&mut state, content, ALL_SOURCES, &system);

        state.system_mut(&system).units.clear();
        reconcile(&mut state, content, ALL_SOURCES, &system);
        assert_eq!(
            holder(&state, &system),
            Some(a),
            "an empty system leaves the holder in place"
        );
    }

    #[test]
    fn ground_forces_in_the_system_count_as_presence() {
        // Rule 2 says units, not ships. A seat holding the real planet on a mixed tile is present.
        let content = ti4_content::ContentStore::embedded();
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (a, b) = (PlayerId::new("a"), PlayerId::new("b"));
        let system = SystemId::new("109"); // Bellatrix + Tsion Station

        state.system_mut(&system).planet_units.insert(
            PlanetId::new("bellatrix"),
            vec![Unit::new(
                ti4_model::id::UnitTypeId::new("infantry"),
                a.clone(),
            )],
        );
        state.system_mut(&system).units.push(ship(&b));

        reconcile(&mut state, content, ALL_SOURCES, &system);
        assert_eq!(
            state
                .system_state(&system)
                .planet_control
                .get(&PlanetId::new("tsionstation")),
            None,
            "two players are present, so nobody takes the station"
        );
    }

    #[test]
    fn a_system_with_no_station_is_left_alone() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, a, _b, _s) = setup();
        let plain = SystemId::new("19");
        state.system_mut(&plain).units.push(ship(&a));
        assert!(!reconcile(&mut state, content, ALL_SOURCES, &plain));
        assert!(state.system_state(&plain).planet_control.is_empty());
    }
}
