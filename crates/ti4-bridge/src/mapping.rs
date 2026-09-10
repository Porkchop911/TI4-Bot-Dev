//! M11-009 — building the engine's galaxy from the map that is actually on the table.
//!
//! Without this, reconciliation compares against the wrong board. [`ti4_content::galaxy::Galaxy`]
//! can lay tiles out on a spiral of its own, and a game built that way need not contain the system
//! the players are fighting over at all — asked whether Xxcha can activate tile 69, the honest
//! answer would be that there is no tile 69 on *its* map. The engine has to be playing the same
//! game before it can referee one.
//!
//! The mod already reports where every tile is, so this converts its grid into the engine's.
//!
//! # The two coordinate systems
//!
//! The mod divides a tile's world position by `5.25` across and `3.03` down. That ratio is √3, the
//! giveaway for a hex grid, and what comes out is **doubled-height** coordinates: neighbours sit at
//! `(±1, ±1)` and `(0, ±2)`, never at `(±1, 0)`. The engine uses axial `(q, r)`. So:
//!
//! ```text
//! q = x
//! r = (y - x) / 2
//! ```
//!
//! `y - x` is always even on a real board, which makes it a free integrity check: an odd value is
//! not a hex position at all, and means something upstream is wrong.
//!
//! That conversion has unusually good ground truth. A carrier has move 1, and a captured game shows
//! Xxcha moving a carrier from tile 14 to tile 69 — so those two tiles *are* adjacent, whatever any
//! formula says. The test at the bottom of this file is that move.
//!
//! # The corpus zero-pads and the mod does not
//!
//! The mod writes tile `1`; the corpus calls it `01`. Nine tiles differ this way — and they are
//! exactly the **home systems**, so a lookup that does not normalise loses every seat's starting
//! position and reports a board where nobody lives anywhere. The table's spelling is kept for
//! reporting and the corpus's is used for placement, because everything downstream of here keys on
//! the corpus id.
//!
//! # Tiles the corpus has never heard of
//!
//! Real boards carry them: the captured six-player game includes 901 through 907. They are dropped
//! and **reported**, never guessed at. A galaxy containing a system with no planets, no anomalies
//! and no wormholes would not fail — it would quietly answer movement questions wrongly, which is
//! worse than admitting the gap.

use ti4_content::ContentStore;
use ti4_content::galaxy::{Galaxy, GalaxyError};
use ti4_model::content_types::SourceSet;
use ti4_model::hex::Hex;

use crate::hexsummary::Board;

/// Why a table position could not become a galaxy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MappingError {
    /// A grid position no hex could occupy.
    #[error("({x}, {y}) is not a hex position: y - x must be even")]
    NotOnTheLattice { x: i32, y: i32 },
    /// The same tile appeared twice, or the corpus refused a placement.
    #[error("building the galaxy: {0}")]
    Galaxy(String),
}

impl From<GalaxyError> for MappingError {
    fn from(error: GalaxyError) -> Self {
        Self::Galaxy(error.to_string())
    }
}

/// Doubled-height grid coordinates to the engine's axial pair.
///
/// # Errors
/// [`MappingError::NotOnTheLattice`] when `y - x` is odd, which no hex position can be.
pub fn to_hex(x: i32, y: i32) -> Result<Hex, MappingError> {
    if (y - x).rem_euclid(2) != 0 {
        return Err(MappingError::NotOnTheLattice { x, y });
    }
    Ok(Hex::new(x, (y - x) / 2))
}

/// The inverse, for putting an engine position back on the table.
#[must_use]
pub fn to_grid(hex: Hex) -> (i32, i32) {
    (hex.q, hex.q + 2 * hex.r)
}

/// The wormhole nexus, which the mod spells `82` and the corpus splits by face.
///
/// `82a` is the locked face and carries only a gamma wormhole; `82b` is open and carries alpha and
/// beta as well. The board summary reports neither — it gives the tile number and the pieces on it,
/// and the faces differ in *wormholes*, which the summary never encodes. So the face cannot be read
/// off the table.
///
/// The locked face is assumed because it is the setup state, and the ambiguity is reported as
/// [`Mapped::assumed_nexus_locked`] rather than swallowed: an open nexus is adjacent to every alpha
/// and beta wormhole on the board, so guessing wrong does not lose a tile, it silently rewires
/// movement across the whole galaxy.
pub const NEXUS_TILE: &str = "82";
/// The face assumed for [`NEXUS_TILE`] when the table cannot say.
pub const NEXUS_LOCKED_FACE: &str = "82a";

/// The corpus's id for a tile the mod named, if it carries one.
///
/// Tries the mod's spelling first, then the nexus face, then the corpus's zero-padded form —
/// padding is last and conditional, because a three-digit tile must not become four and a tile
/// whose id is not a number at all must not be mangled into one.
pub fn corpus_id<'a>(
    catalogue: &'a std::collections::BTreeMap<&'a str, ti4_content::galaxy::System<'a>>,
    tile: &str,
) -> Option<&'a str> {
    if let Some((id, _)) = catalogue.get_key_value(tile) {
        return Some(id);
    }
    if tile == NEXUS_TILE
        && let Some((id, _)) = catalogue.get_key_value(NEXUS_LOCKED_FACE)
    {
        return Some(id);
    }
    if tile.len() == 1 && tile.chars().all(|c| c.is_ascii_digit()) {
        let padded = format!("0{tile}");
        if let Some((id, _)) = catalogue.get_key_value(padded.as_str()) {
            return Some(id);
        }
    }
    None
}

/// A galaxy built from the table, and an honest account of what was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapped {
    /// The engine's board, containing exactly the tiles the corpus knows.
    pub galaxy: Galaxy,
    /// Tiles on the table that the content corpus does not carry, in table order.
    ///
    /// Reported rather than dropped silently: a caller reconciling against this galaxy needs to
    /// know that a piece of the table is missing from it, because movement answers near the gap
    /// will be wrong rather than merely absent.
    pub unknown_tiles: Vec<String>,
    /// The wormhole nexus was placed on its locked face because the table could not say which.
    ///
    /// See [`NEXUS_TILE`]. If the nexus is actually open, every alpha and beta wormhole on the
    /// board is adjacent to it and this galaxy answers movement wrongly.
    pub assumed_nexus_locked: bool,
}

/// Build the engine's galaxy from a decoded board.
///
/// Hyperlane faces are ignored here — the tile is placed at its position, and the paths a hyperlane
/// opens are not modelled by [`Galaxy`] at all (it tracks them only as a flag). Movement across
/// hyperlanes is therefore *not* answered correctly yet, which is a gap worth naming rather than a
/// thing this function quietly gets right.
///
/// # Errors
/// [`MappingError::NotOnTheLattice`] if any tile is at an impossible position, and
/// [`MappingError::Galaxy`] if the same tile is on the table twice.
pub fn galaxy_from_board(
    store: &ContentStore,
    board: &Board,
    sources: SourceSet,
) -> Result<Mapped, MappingError> {
    let catalogue = ti4_content::galaxy::all_systems(store, sources);

    let mut placements: Vec<(&str, Hex)> = Vec::new();
    let mut unknown_tiles = Vec::new();
    for system in &board.systems {
        let hex = to_hex(system.x, system.y)?;
        if let Some(id) = corpus_id(&catalogue, &system.tile) {
            placements.push((id, hex));
        } else {
            unknown_tiles.push(system.tile.clone());
        }
    }

    let assumed_nexus_locked = board.systems.iter().any(|system| system.tile == NEXUS_TILE);

    let galaxy = Galaxy::placed(store, &placements, sources)?;
    Ok(Mapped {
        galaxy,
        unknown_tiles,
        assumed_nexus_locked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hexsummary::decode;
    use ti4_model::content_types::FULL;

    fn store() -> &'static ContentStore {
        // Already a shared static inside the loader; this is just the shorter name for it.
        ContentStore::embedded()
    }

    #[test]
    fn the_two_coordinate_systems_round_trip() {
        for (x, y) in [(0, 0), (2, 0), (-1, -5), (3, 1), (-3, 3), (1, 7)] {
            let hex = to_hex(x, y).expect("a lattice position");
            assert_eq!(to_grid(hex), (x, y), "({x}, {y})");
        }
    }

    #[test]
    fn a_position_off_the_lattice_is_refused_rather_than_rounded() {
        // The check is free and it is the only signal that something upstream mangled a position.
        // Rounding would put a tile one step from where it is, which reads as a legal board.
        for (x, y) in [(0, 1), (2, -1), (-1, 0)] {
            assert_eq!(
                to_hex(x, y),
                Err(MappingError::NotOnTheLattice { x, y }),
                "({x}, {y})"
            );
        }
    }

    #[test]
    fn negative_coordinates_convert_the_same_way() {
        // `(y - x) / 2` truncates towards zero in Rust and floors in Python. On even differences
        // they agree, which is why the lattice check has to come first.
        assert_eq!(to_hex(-1, -5).expect("lattice"), Hex::new(-1, -2));
        assert_eq!(to_hex(-3, 3).expect("lattice"), Hex::new(-3, 3));
        assert_eq!(to_hex(3, -3).expect("lattice"), Hex::new(3, -3));
    }

    #[test]
    fn the_ground_truth_move_comes_out_adjacent() {
        // A carrier has move 1, and a captured game shows Xxcha moving a carrier from tile 14 to
        // tile 69. Those tiles are adjacent on the real table, whatever any formula says. This is
        // the only test here that could not have been written from the conversion alone.
        let board = decode("14+2-4Wf2r;Wipso;Wio,69+2-2GtWc2f1t;Wio;Wio")
            .expect("two tiles from a real capture");
        let mapped = galaxy_from_board(store(), &board, FULL).expect("a galaxy");
        assert!(
            mapped.galaxy.are_adjacent("14", "69"),
            "tile 14 and tile 69 must be adjacent: a carrier moved between them"
        );
    }

    #[test]
    fn tiles_the_corpus_does_not_carry_are_reported_and_not_invented() {
        // The captured six-player board includes 901 through 907. A galaxy holding a system with
        // no planets, no anomalies and no wormholes answers movement wrongly rather than failing.
        let board = decode("18+0+0,901-2+6;,902+2+6;").expect("one real tile and two invented");
        let mapped = galaxy_from_board(store(), &board, FULL).expect("a galaxy");
        assert_eq!(
            mapped.unknown_tiles,
            vec!["901".to_owned(), "902".to_owned()]
        );
        assert_eq!(mapped.galaxy.system_ids(), vec!["18"]);
    }

    #[test]
    fn a_real_capture_places_every_tile_the_corpus_knows() {
        let summary = "1-3-1Py2c3f5i;Ps,6-2+4Ycd3f5i;Yps,10-1-5Bcdfy3i;Bs;,12+1+5Rdf2ci;Rs2p;,\
                       14+2-4Wf2r;Wipso;Wio,16+3+1G2cfi;Gis;;Gi,18+0+0;,19-3+1;,21+2+0;,26-1-3;;,\
                       69+2-2GtWc2f1t;Wio;Wio,901-2+6;,902+2+6;";
        let board = decode(summary).expect("a real capture");
        let mapped = galaxy_from_board(store(), &board, FULL).expect("a galaxy");

        assert_eq!(
            mapped.unknown_tiles,
            vec!["901".to_owned(), "902".to_owned()]
        );
        assert_eq!(
            mapped.galaxy.system_ids().len(),
            11,
            "every tile the corpus carries must be placed"
        );
        assert!(
            mapped.galaxy.coord_of("01").is_some(),
            "the mod's tile 1 is the corpus's 01, and it is a home system"
        );

        // Mecatol is at the centre of a real table, and comes out at the origin.
        assert_eq!(mapped.galaxy.coord_of("18"), Some(Hex::ORIGIN));

        // Whether the placement is a *board* rather than a heap is checked against the whole
        // captures in `tests/hexsummary_golden.rs`; this summary is a hand-picked subset, so the
        // tiles between these are missing and most of them correctly touch nothing.
    }

    #[test]
    fn a_tile_on_the_table_twice_is_refused() {
        // Two placements of one id would leave the galaxy's two indexes disagreeing, putting a
        // tile in two places at once.
        let board = decode("18+0+0,18+2+0").expect("a board that names 18 twice");
        let error =
            galaxy_from_board(store(), &board, FULL).expect_err("one tile cannot be in two places");
        assert!(matches!(error, MappingError::Galaxy(_)), "{error}");
    }
}

#[cfg(test)]
mod nexus_tests {
    use super::*;
    use crate::hexsummary::decode;

    #[test]
    fn the_wormhole_nexus_is_placed_on_its_locked_face_and_says_so() {
        // The mod writes `82`; the corpus splits it into `82a` (gamma only) and `82b` (alpha, beta
        // and gamma). The summary reports pieces, never wormholes, so the face is unreadable.
        let board = decode("18+0+0;,82+6-2;;").expect("Mecatol and the nexus");
        let mapped = galaxy_from_board(
            ContentStore::embedded(),
            &board,
            ti4_model::content_types::FULL,
        )
        .expect("a galaxy");

        assert!(
            mapped.galaxy.coord_of(NEXUS_LOCKED_FACE).is_some(),
            "the nexus must be on the board, not dropped as an unknown tile"
        );
        assert!(
            !mapped.unknown_tiles.contains(&NEXUS_TILE.to_owned()),
            "tile 82 is a real system and must not be reported missing"
        );
        assert!(
            mapped.assumed_nexus_locked,
            "the assumption must be declared: an open nexus rewires movement galaxy-wide"
        );
    }

    #[test]
    fn a_board_without_the_nexus_claims_no_assumption() {
        let board = decode("18+0+0;").expect("just Mecatol");
        let mapped = galaxy_from_board(
            ContentStore::embedded(),
            &board,
            ti4_model::content_types::FULL,
        )
        .expect("a galaxy");
        assert!(!mapped.assumed_nexus_locked);
    }
}
