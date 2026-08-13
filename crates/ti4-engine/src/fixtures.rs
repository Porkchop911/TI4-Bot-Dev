//! Shared test fixtures.
//!
//! Every combat, movement and invasion module grew its own `arena`, `put` and `plain_systems`.
//! Beyond the duplication that cost a real bug twice: the one-ring geometry trap in
//! [`Hub::across`] was rediscovered independently in `movement.rs` and `tactical.rs`, because
//! each had written its own fixture and neither knew what the other had learned.
//!
//! Compiled unconditionally so sibling crates can use the same board; see the note on the module
//! declaration in `lib.rs` for why a cargo feature cannot do that job.

use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, System};
use ti4_model::content_types::POK;
use ti4_model::id::{PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::setup::start_game;

/// A seated game with the given players.
///
/// # Panics
/// If setup refuses these players.
#[must_use]
pub fn game(players: &[&str]) -> GameState {
    let seats: Vec<PlayerId> = players.iter().map(|name| PlayerId::new(*name)).collect();
    start_game(ContentStore::embedded(), &seats, POK, None).expect("setup succeeds")
}

/// Ordinary systems — no anomaly, no hyperlane — taken from the corpus in a stable order.
#[must_use]
pub fn plain_systems(count: usize) -> Vec<String> {
    ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
        .iter()
        .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
        .map(|(id, _)| (*id).to_owned())
        .take(count)
        .collect()
}

/// A system of one anomaly kind, chosen by property rather than by id.
///
/// # Panics
/// If the corpus has no such system, or `kind` is not one of the four names.
#[must_use]
pub fn a_system_where(kind: &str) -> String {
    ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
        .iter()
        .find(|(_, system)| match kind {
            "nebula" => system.is_nebula(),
            "supernova" => system.is_supernova(),
            "asteroid field" => system.is_asteroid_field(),
            "gravity rift" => system.is_gravity_rift(),
            other => unreachable!("unknown anomaly kind {other}"),
        })
        .map(|(id, _)| (*id).to_owned())
        .expect("the corpus has one")
}

/// A planet the corpus actually places, with the system holding it.
///
/// # Panics
/// If the corpus has no placed planet.
#[must_use]
pub fn a_placed_planet() -> (SystemId, PlanetId) {
    ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
        .iter()
        .find(|(_, planet)| planet.system_id().is_some() && !planet.is_placed_during_play())
        .map(|(id, planet)| {
            (
                SystemId::new(planet.system_id().unwrap_or("18")),
                PlanetId::new(*id),
            )
        })
        .expect("the corpus has a placed planet")
}

/// Planets in no faction's home system.
#[must_use]
pub fn non_home_planets(count: usize) -> Vec<String> {
    ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
        .iter()
        .filter(|(_, planet)| planet.homeworld_of().is_none() && !planet.is_placed_during_play())
        .map(|(id, _)| (*id).to_owned())
        .take(count)
        .collect()
}

/// Put `count` units of a kind into a system's space area.
pub fn put(state: &mut GameState, system: &SystemId, kind: &str, owner: &PlayerId, count: usize) {
    for _ in 0..count {
        state
            .system_mut(system)
            .units
            .push(Unit::new(UnitTypeId::new(kind), owner.clone()));
    }
}

/// Put `count` units of a kind onto a planet.
pub fn put_on_planet(
    state: &mut GameState,
    system: &SystemId,
    planet: &PlanetId,
    kind: &str,
    owner: &PlayerId,
    count: usize,
) {
    for _ in 0..count {
        state
            .system_mut(system)
            .planet_units
            .entry(planet.clone())
            .or_default()
            .push(Unit::new(UnitTypeId::new(kind), owner.clone()));
    }
}

/// A one-ring map: `centre` surrounded by six `outer` systems.
///
/// **The ring is itself a route.** Two opposite outer systems are two apart *through the centre*
/// but three apart *around the ring*, so a test that blocks the centre must use a move value of
/// 2 — a larger value lets the detour succeed and the test passes for the wrong reason.
pub struct Hub {
    pub galaxy: Galaxy,
    pub centre: String,
    pub outer: Vec<String>,
}

impl Hub {
    /// The outer system directly across the centre from `from`.
    ///
    /// Two apart is **not** enough to identify it: ring positions two seats round are also two
    /// apart, by a route that never touches the centre. The opposite tile is the one whose
    /// *only* shared neighbour is the centre, which is what makes the centre a real bottleneck.
    ///
    /// This distinction was rediscovered independently in two modules before the fixture moved
    /// here. Both wrong versions passed the eye test.
    ///
    /// # Panics
    /// If the map is not a full one-ring hub.
    #[must_use]
    pub fn across(&self, from: &str) -> String {
        let neighbours_of = |id: &str| -> std::collections::BTreeSet<String> {
            self.galaxy
                .adjacent(id)
                .into_iter()
                .map(ToOwned::to_owned)
                .collect()
        };
        let from_neighbours = neighbours_of(from);
        self.outer
            .iter()
            .find(|other| {
                other.as_str() != from
                    && self.galaxy.distance(from, other) == Some(2)
                    && &from_neighbours & &neighbours_of(other)
                        == std::collections::BTreeSet::from([self.centre.clone()])
            })
            .cloned()
            .expect("every outer system has one opposite")
    }
}

/// Build a hub whose tiles are `ids`, the first at the centre and the rest around it.
///
/// # Panics
/// If the galaxy cannot be built from these ids.
#[must_use]
pub fn hub_from(ids: &[String]) -> Hub {
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let galaxy = Galaxy::build(ContentStore::embedded(), &refs, POK, 1).expect("a valid map");
    Hub {
        galaxy,
        centre: ids[0].clone(),
        outer: ids[1..].to_vec(),
    }
}

/// A hub whose centre is `centre_id` and whose ring is ordinary systems.
#[must_use]
pub fn hub_with_centre(centre_id: &str) -> Hub {
    let mut ids = vec![centre_id.to_owned()];
    ids.extend(
        plain_systems(8)
            .into_iter()
            .filter(|id| id != centre_id)
            .take(6),
    );
    hub_from(&ids)
}

/// A hub whose centre is ordinary and whose first ring seat is `outer_id`.
#[must_use]
pub fn hub_with_outer(outer_id: &str) -> Hub {
    let plain: Vec<String> = plain_systems(9)
        .into_iter()
        .filter(|id| id != outer_id)
        .collect();
    let mut ids = vec![plain[0].clone(), outer_id.to_owned()];
    ids.extend(plain[1..6].iter().cloned());
    hub_from(&ids)
}

/// An ordinary hub.
#[must_use]
pub fn plain_hub() -> Hub {
    hub_with_centre(&plain_systems(1)[0])
}

/// Silence "unused" for helpers not every module needs.
#[allow(
    dead_code,
    reason = "a shared fixture module is used a piece at a time"
)]
const fn _all_used(_: Option<&System<'_>>) {}
