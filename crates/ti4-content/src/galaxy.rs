//! The galaxy: systems, planets, and adjacency.
//!
//! Systems come from the corpus verbatim, so [`System`] is a thin typed view over a record
//! rather than a re-modelling of one. Anomaly flags, wormholes, and planet lists are read
//! straight from the data, which is what keeps "adding Thunder's Edge tiles is a
//! re-extraction, not a code change" true.
//!
//! Adjacency is the six hex neighbours *plus* wormhole pairing: in TI4 two tiles sharing a
//! wormhole kind are adjacent however far apart they sit. It is derived on demand rather
//! than stored, so no cached neighbour list can drift from the placement.
//!
//! Hyperlanes are not modelled. The corpus marks a tile `isHyperlane` but carries no path
//! data, and guessing the paths would produce adjacency that is wrong in a way tests would
//! not catch. [`Galaxy::hyperlanes`] lists them so a caller can see what is excluded.

use std::collections::{BTreeMap, BTreeSet};

use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::hex::Hex;

use crate::loader::ContentStore;
use crate::record::Record;

/// A typed view over a system record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct System<'a> {
    record: &'a Record,
}

impl<'a> System<'a> {
    #[must_use]
    pub const fn new(record: &'a Record) -> Self {
        Self { record }
    }

    #[must_use]
    pub const fn record(&self) -> &'a Record {
        self.record
    }

    #[must_use]
    pub fn id(&self) -> &'a str {
        self.record.id().unwrap_or_default()
    }

    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        self.record.text("name")
    }

    /// Planet ids in this system, in corpus order.
    #[must_use]
    pub fn planets(&self) -> Vec<&'a str> {
        self.record.strings("planets")
    }

    /// Wormhole kinds on this tile, e.g. `ALPHA`. Uppercase, as the corpus writes them.
    #[must_use]
    pub fn wormholes(&self) -> BTreeSet<&'a str> {
        self.record.strings("wormholes").into_iter().collect()
    }

    #[must_use]
    pub fn is_nebula(&self) -> bool {
        self.record.flag("isNebula")
    }

    #[must_use]
    pub fn is_supernova(&self) -> bool {
        self.record.flag("isSupernova")
    }

    #[must_use]
    pub fn is_asteroid_field(&self) -> bool {
        self.record.flag("isAsteroidField")
    }

    #[must_use]
    pub fn is_gravity_rift(&self) -> bool {
        self.record.flag("isGravityRift")
    }

    #[must_use]
    pub fn is_hyperlane(&self) -> bool {
        self.record.flag("isHyperlane")
    }

    /// Thunder's Edge's Entropic Scar, a new kind of anomaly.
    ///
    /// Unit abilities cannot be used by or against anything inside it — Sustain Damage,
    /// Production, Planetary Shield, Space Cannon, Bombardment, Deploy, and Anti-Fighter
    /// Barrage — while *text* abilities are unaffected.
    #[must_use]
    pub fn is_scar(&self) -> bool {
        self.record.flag("isScar")
    }

    #[must_use]
    pub fn is_anomaly(&self) -> bool {
        self.is_nebula()
            || self.is_supernova()
            || self.is_asteroid_field()
            || self.is_gravity_rift()
            || self.is_scar()
    }
}

/// A typed view over a planet record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planet<'a> {
    record: &'a Record,
}

impl<'a> Planet<'a> {
    #[must_use]
    pub const fn new(record: &'a Record) -> Self {
        Self { record }
    }

    #[must_use]
    pub const fn record(&self) -> &'a Record {
        self.record
    }

    #[must_use]
    pub fn id(&self) -> &'a str {
        self.record.id().unwrap_or_default()
    }

    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        self.record.text("name")
    }

    /// The system this planet sits in, or `None` if it is not printed on a tile.
    ///
    /// See [`Self::is_placed_during_play`].
    #[must_use]
    pub fn system_id(&self) -> Option<&'a str> {
        self.record.text("tileId")
    }

    /// Whether this planet has no printed tile and is placed onto one during play.
    ///
    /// Twelve planets in the corpus carry no `tileId`: Mirage arrives from exploration,
    /// Custodia Vigilia from the Custodian relic, and the Thunder's Edge planets
    /// (Avernus, Triad, the Oceans, Illusion, Phantasm, Thunder's Edge itself) from
    /// tokens. Each has a matching record in `tokens`. This is a property of the planet,
    /// not a hole in the data, so it is modelled rather than treated as a broken
    /// reference.
    #[must_use]
    pub fn is_placed_during_play(&self) -> bool {
        self.system_id().is_none()
    }

    #[must_use]
    pub fn resources(&self) -> i64 {
        self.record.int("resources").unwrap_or(0)
    }

    #[must_use]
    pub fn influence(&self) -> i64 {
        self.record.int("influence").unwrap_or(0)
    }

    #[must_use]
    pub fn planet_type(&self) -> Option<&'a str> {
        self.record.text("planetType")
    }

    #[must_use]
    pub fn tech_specialties(&self) -> Vec<&'a str> {
        self.record.strings("techSpecialties")
    }

    #[must_use]
    pub fn is_legendary(&self) -> bool {
        self.record.text("legendaryAbilityName").is_some()
    }

    /// The faction whose homeworld this is, if any.
    #[must_use]
    pub fn homeworld_of(&self) -> Option<&'a str> {
        self.record.text("factionHomeworld")
    }
}

/// The system catalogue in a source scope, keyed by id.
#[must_use]
pub fn all_systems(store: &ContentStore, sources: SourceSet) -> BTreeMap<&str, System<'_>> {
    store
        .from_sources(ContentType::Systems, sources)
        .filter_map(|r| r.id().map(|id| (id, System::new(r))))
        .collect()
}

/// The planet catalogue in a source scope, keyed by id.
#[must_use]
pub fn all_planets(store: &ContentStore, sources: SourceSet) -> BTreeMap<&str, Planet<'_>> {
    store
        .from_sources(ContentType::Planets, sources)
        .filter_map(|r| r.id().map(|id| (id, Planet::new(r))))
        .collect()
}

/// The planets that sit in a given system.
#[must_use]
pub fn planets_in<'a>(
    store: &'a ContentStore,
    system_id: &str,
    sources: SourceSet,
) -> Vec<Planet<'a>> {
    store
        .from_sources(ContentType::Planets, sources)
        .filter(|r| r.text("tileId") == Some(system_id))
        .map(Planet::new)
        .collect()
}

/// Whether a system holds any faction's homeworld.
#[must_use]
pub fn is_home_system(store: &ContentStore, system_id: &str, sources: SourceSet) -> bool {
    planets_in(store, system_id, sources)
        .iter()
        .any(|p| p.homeworld_of().is_some())
}

/// Something went wrong building a board.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GalaxyError {
    #[error("{rings} rings hold {capacity} tiles, got {requested}")]
    TooManyTiles {
        rings: i32,
        capacity: usize,
        requested: usize,
    },
    #[error("no system {0:?} in the corpus for the requested sources")]
    UnknownSystem(String),
    #[error("system {0:?} was placed twice")]
    DuplicateSystem(String),
}

/// Wormhole kinds that Wormhole Reconstruction links together, whatever their kind.
const LINKED_KINDS: [&str; 2] = ["ALPHA", "BETA"];

/// Systems placed on a hex grid, with adjacency derived rather than stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Galaxy {
    /// Hex to system id.
    placement: BTreeMap<Hex, String>,
    /// System id to hex — the inverse, built once.
    coords: BTreeMap<String, Hex>,
    /// System id to its wormhole kinds, copied out of the corpus at build time so that
    /// adjacency does not need the store threaded through every call.
    wormholes: BTreeMap<String, BTreeSet<String>>,
    /// System ids flagged `isHyperlane`, whose paths this module does not model.
    hyperlanes: BTreeSet<String>,
    /// Laws in play may switch wormholes off entirely or link them all together. Set by
    /// the game rather than read from it, so a `Galaxy` stays a plain value.
    pub wormholes_off: bool,
    pub wormholes_all_linked: bool,
}

impl Galaxy {
    /// Place systems onto a spiral from the centre outwards.
    ///
    /// A placeholder for real map setup (drafting, map templates); enough to exercise
    /// adjacency, movement, and activation.
    ///
    /// # Errors
    /// [`GalaxyError::TooManyTiles`] if the requested rings cannot hold the tiles, or
    /// [`GalaxyError::UnknownSystem`] if a system is not in the corpus for these sources.
    pub fn build(
        store: &ContentStore,
        system_ids: &[&str],
        sources: SourceSet,
        rings: i32,
    ) -> Result<Self, GalaxyError> {
        let spiral = Hex::spiral(rings);
        if spiral.len() < system_ids.len() {
            return Err(GalaxyError::TooManyTiles {
                rings,
                capacity: spiral.len(),
                requested: system_ids.len(),
            });
        }

        let catalogue = all_systems(store, sources);
        let mut placement = BTreeMap::new();
        let mut coords = BTreeMap::new();
        let mut wormholes = BTreeMap::new();
        let mut hyperlanes = BTreeSet::new();

        for (hex, &id) in spiral.iter().zip(system_ids) {
            let system = catalogue
                .get(id)
                .ok_or_else(|| GalaxyError::UnknownSystem(id.to_owned()))?;
            if coords.contains_key(id) {
                // Silently keeping the last placement leaves `placement` and `coords`
                // disagreeing, which puts the tile in two places at once and moves
                // everything after it one step round the spiral.
                return Err(GalaxyError::DuplicateSystem(id.to_owned()));
            }
            placement.insert(*hex, id.to_owned());
            coords.insert(id.to_owned(), *hex);
            wormholes.insert(
                id.to_owned(),
                system.wormholes().into_iter().map(str::to_owned).collect(),
            );
            if system.is_hyperlane() {
                hyperlanes.insert(id.to_owned());
            }
        }

        Ok(Self {
            placement,
            coords,
            wormholes,
            hyperlanes,
            wormholes_off: false,
            wormholes_all_linked: false,
        })
    }

    /// System ids on the board, in placement order (centre outwards).
    #[must_use]
    pub fn system_ids(&self) -> Vec<&str> {
        self.placement.values().map(String::as_str).collect()
    }

    /// The hex a system sits on.
    #[must_use]
    pub fn coord_of(&self, system_id: &str) -> Option<Hex> {
        self.coords.get(system_id).copied()
    }

    /// The system on a hex, if any.
    #[must_use]
    pub fn system_at(&self, hex: Hex) -> Option<&str> {
        self.placement.get(&hex).map(String::as_str)
    }

    /// Systems flagged as hyperlanes, whose paths are not modelled here.
    #[must_use]
    pub fn hyperlanes(&self) -> Vec<&str> {
        self.hyperlanes.iter().map(String::as_str).collect()
    }

    /// Neighbouring systems, including wormhole pairs (LRR: Adjacency).
    ///
    /// Returns an empty set for a system that is not on this board.
    #[must_use]
    pub fn adjacent(&self, system_id: &str) -> BTreeSet<&str> {
        let Some(here) = self.coords.get(system_id) else {
            return BTreeSet::new();
        };
        let mut neighbours: BTreeSet<&str> = here
            .neighbours()
            .into_iter()
            .filter_map(|n| self.system_at(n))
            .collect();
        if !self.wormholes_off {
            neighbours.extend(self.wormhole_partners(system_id));
        }
        neighbours.remove(system_id);
        neighbours
    }

    #[must_use]
    pub fn are_adjacent(&self, a: &str, b: &str) -> bool {
        self.adjacent(a).contains(b)
    }

    /// Hex distance between two systems, ignoring wormholes.
    ///
    /// Wormholes make two tiles *adjacent* without making them close, so this is a
    /// geometric measure and not a movement range. Movement asks [`Self::adjacent`].
    #[must_use]
    pub fn distance(&self, a: &str, b: &str) -> Option<i32> {
        Some(self.coord_of(a)?.distance(self.coord_of(b)?))
    }

    fn wormhole_partners(&self, system_id: &str) -> BTreeSet<&str> {
        let Some(kinds) = self.wormholes.get(system_id) else {
            return BTreeSet::new();
        };
        if kinds.is_empty() {
            return BTreeSet::new();
        }

        let links_everything =
            self.wormholes_all_linked && kinds.iter().any(|k| LINKED_KINDS.contains(&k.as_str()));

        self.wormholes
            .iter()
            .filter(|(other, other_kinds)| {
                other.as_str() != system_id
                    && if links_everything {
                        other_kinds
                            .iter()
                            .any(|k| LINKED_KINDS.contains(&k.as_str()))
                    } else {
                        !other_kinds.is_disjoint(kinds)
                    }
            })
            .map(|(other, _)| other.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::FULL;

    fn store() -> &'static ContentStore {
        ContentStore::embedded()
    }

    /// Mecatol at the centre, ringed by six ordinary systems.
    fn small() -> Galaxy {
        Galaxy::build(
            store(),
            &["18", "19", "20", "21", "22", "23", "24"],
            FULL,
            3,
        )
        .unwrap()
    }

    // -- systems and planets from data ---------------------------------------------

    #[test]
    fn mecatol_rex_is_read_from_data() {
        let systems = all_systems(store(), FULL);
        let mecatol = systems["18"];
        assert_eq!(mecatol.name(), Some("Mecatol Rex"));
        assert_eq!(mecatol.planets(), vec!["mr"]);
        assert!(!mecatol.is_anomaly());
    }

    #[test]
    fn anomaly_flags_are_read_from_data() {
        let systems = all_systems(store(), FULL);
        assert!(systems["41"].is_gravity_rift());
        assert!(systems["41"].is_anomaly());
        assert_eq!(systems["39"].wormholes(), BTreeSet::from(["ALPHA"]));
    }

    #[test]
    fn the_corpus_supplies_every_anomaly_kind() {
        let systems = all_systems(store(), FULL);
        assert!(systems.values().any(System::is_nebula));
        assert!(systems.values().any(System::is_supernova));
        assert!(systems.values().any(System::is_asteroid_field));
        assert!(systems.values().any(System::is_gravity_rift));
        assert!(systems.values().any(System::is_scar), "Thunder's Edge");
    }

    #[test]
    fn planets_carry_their_economy() {
        let planets = all_planets(store(), FULL);
        let abyz = planets["abyz"];
        assert_eq!((abyz.resources(), abyz.influence()), (3, 0));
        assert_eq!(abyz.planet_type(), Some("HAZARDOUS"));
        assert_eq!(abyz.system_id(), Some("38"));
    }

    #[test]
    fn mecatol_is_worth_six_influence() {
        let planets = all_planets(store(), FULL);
        let mecatol = planets["mr"];
        assert_eq!((mecatol.resources(), mecatol.influence()), (1, 6));
        assert_eq!(mecatol.homeworld_of(), None);
    }

    #[test]
    fn a_home_system_is_recognised_by_its_planets() {
        // Jord is Sol's homeworld and sits in system 01.
        assert!(is_home_system(store(), "01", FULL));
        assert!(
            !is_home_system(store(), "18", FULL),
            "Mecatol is nobody's home"
        );
        let jord = planets_in(store(), "01", FULL);
        assert_eq!(jord.len(), 1);
        assert_eq!(jord[0].id(), "jord");
        assert_eq!(jord[0].homeworld_of(), Some("sol"));
    }

    #[test]
    fn every_printed_planet_names_a_system_that_exists() {
        let systems = all_systems(store(), FULL);
        for (id, planet) in all_planets(store(), FULL) {
            if let Some(tile) = planet.system_id() {
                assert!(
                    systems.contains_key(tile),
                    "{id} sits on unknown tile {tile}"
                );
            }
        }
    }

    #[test]
    fn twelve_planets_are_placed_during_play_rather_than_printed_on_a_tile() {
        let placed: Vec<&str> = all_planets(store(), FULL)
            .into_iter()
            .filter(|(_, p)| p.is_placed_during_play())
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            placed,
            vec![
                "avernus",
                "custodiavigilia",
                "illusion",
                "mirage",
                "ocean1",
                "ocean2",
                "ocean3",
                "ocean4",
                "ocean5",
                "phantasm",
                "thundersedge",
                "triad",
            ]
        );
        // Each arrives on a token, so the token catalogue must know about it.
        assert!(store().get(ContentType::Tokens, "mirage").is_some());
        assert!(store().get(ContentType::Tokens, "avernus").is_some());
    }

    #[test]
    fn planets_in_a_system_excludes_the_ones_placed_during_play() {
        // Mirage has no tile, so it must not appear in any system's planet list until an
        // exploration puts it there.
        for system in all_systems(store(), FULL).keys() {
            let ids: Vec<&str> = planets_in(store(), system, FULL)
                .iter()
                .map(Planet::id)
                .collect();
            assert!(
                !ids.contains(&"mirage"),
                "mirage should not be printed on {system}"
            );
        }
    }

    // -- adjacency -----------------------------------------------------------------

    #[test]
    fn centre_is_adjacent_to_its_whole_ring() {
        assert_eq!(
            small().adjacent("18"),
            BTreeSet::from(["19", "20", "21", "22", "23", "24"])
        );
    }

    #[test]
    fn adjacency_is_symmetric() {
        let galaxy = small();
        for a in galaxy.system_ids() {
            for b in galaxy.adjacent(a) {
                assert!(galaxy.are_adjacent(b, a), "{a} -> {b} but not back");
            }
        }
    }

    #[test]
    fn a_system_is_not_adjacent_to_itself() {
        let galaxy = small();
        for id in galaxy.system_ids() {
            assert!(!galaxy.adjacent(id).contains(id));
        }
    }

    #[test]
    fn ring_neighbours_are_adjacent_to_each_other_but_not_across() {
        let galaxy = small();
        assert!(galaxy.are_adjacent("19", "20"));
        assert!(
            !galaxy.are_adjacent("19", "22"),
            "opposite side of the ring"
        );
    }

    #[test]
    fn wormholes_make_distant_systems_adjacent() {
        // Tile 39 and Lodor (26) are the base game's alpha pair; distance is irrelevant.
        let galaxy = Galaxy::build(
            store(),
            &["18", "19", "20", "21", "22", "23", "39", "26"],
            FULL,
            3,
        )
        .unwrap();
        assert!(galaxy.distance("39", "26").unwrap() > 1);
        assert!(galaxy.are_adjacent("39", "26"));
    }

    #[test]
    fn a_beta_wormhole_does_not_pair_with_an_alpha() {
        let galaxy = Galaxy::build(
            store(),
            &["18", "19", "20", "21", "22", "23", "39", "40"],
            FULL,
            3,
        )
        .unwrap();
        let systems = all_systems(store(), FULL);
        assert_eq!(systems["40"].wormholes(), BTreeSet::from(["BETA"]));
        assert!(!galaxy.are_adjacent("39", "40"));
    }

    #[test]
    fn wormhole_reconstruction_links_every_alpha_and_beta() {
        let mut galaxy = Galaxy::build(
            store(),
            &["18", "19", "20", "21", "22", "23", "39", "40"],
            FULL,
            3,
        )
        .unwrap();
        assert!(!galaxy.are_adjacent("39", "40"));
        galaxy.wormholes_all_linked = true;
        assert!(galaxy.are_adjacent("39", "40"), "the law links them");
        assert!(galaxy.are_adjacent("40", "39"), "and symmetrically");
    }

    #[test]
    fn switching_wormholes_off_leaves_only_hex_adjacency() {
        let mut galaxy = Galaxy::build(
            store(),
            &["18", "19", "20", "21", "22", "23", "39", "26"],
            FULL,
            3,
        )
        .unwrap();
        assert!(galaxy.are_adjacent("39", "26"));
        galaxy.wormholes_off = true;
        assert!(!galaxy.are_adjacent("39", "26"));
        assert!(
            galaxy.are_adjacent("18", "19"),
            "hex neighbours are unaffected"
        );
    }

    #[test]
    fn a_wormhole_with_no_partner_on_the_board_is_adjacent_to_nothing_extra() {
        let galaxy = Galaxy::build(store(), &["18", "39"], FULL, 3).unwrap();
        assert_eq!(galaxy.adjacent("39"), BTreeSet::from(["18"]));
    }

    #[test]
    fn a_system_off_the_board_has_no_neighbours() {
        assert!(small().adjacent("99").is_empty());
        assert_eq!(small().distance("18", "99"), None);
    }

    #[test]
    fn placement_follows_the_spiral_from_the_centre() {
        let galaxy = small();
        assert_eq!(galaxy.coord_of("18"), Some(Hex::ORIGIN));
        assert_eq!(galaxy.system_at(Hex::ORIGIN), Some("18"));
        for ring in ["19", "20", "21", "22", "23", "24"] {
            assert_eq!(galaxy.distance("18", ring), Some(1));
        }
    }

    #[test]
    fn building_more_tiles_than_fit_fails_loudly() {
        let ids: Vec<&str> = std::iter::repeat_n("18", 40).collect();
        let err = Galaxy::build(store(), &ids, FULL, 1).unwrap_err();
        assert!(
            matches!(
                err,
                GalaxyError::TooManyTiles {
                    rings: 1,
                    capacity: 7,
                    requested: 40
                }
            ),
            "{err}"
        );
    }

    #[test]
    fn placing_a_system_twice_fails_loudly() {
        // Keeping the last placement would leave the tile in two places at once and
        // shift every later tile one step round the spiral.
        let err = Galaxy::build(store(), &["18", "19", "18"], FULL, 3).unwrap_err();
        assert_eq!(err, GalaxyError::DuplicateSystem("18".to_owned()));
    }

    #[test]
    fn building_with_an_unknown_system_fails_loudly() {
        let err = Galaxy::build(store(), &["18", "nonesuch"], FULL, 3).unwrap_err();
        assert_eq!(err, GalaxyError::UnknownSystem("nonesuch".to_owned()));
    }

    #[test]
    fn hyperlanes_are_listed_rather_than_silently_treated_as_ordinary_tiles() {
        let hyperlane = all_systems(store(), FULL)
            .into_iter()
            .find(|(_, s)| s.is_hyperlane())
            .map(|(id, _)| id)
            .expect("the corpus has hyperlane tiles");
        let galaxy = Galaxy::build(store(), &["18", hyperlane], FULL, 3).unwrap();
        assert_eq!(galaxy.hyperlanes(), vec![hyperlane]);
    }

    #[test]
    fn a_galaxy_is_deterministic() {
        assert_eq!(small(), small());
        assert_eq!(small().system_ids(), small().system_ids());
    }
}
