//! M11-007 — the decoder against real captures.
//!
//! `tests/golden/hexsummary_captures.json` holds nine board summaries taken from live six-player
//! games, carried over from the historical repository's fixtures at its pinned commit. They are
//! checked in because that repository is read-only and pinned and cannot be regenerated here, and
//! because a decoder written from an encoder needs at least one board it did not design itself.
//!
//! What these add over the unit tests is *coverage the author did not choose*: 47 systems per
//! board, hyperlanes, anomaly markers in space, planets held by token alone, and mid-combat
//! positions with two colours in one space region.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ti4_bridge::hexsummary::{Board, Piece, decode};
use ti4_bridge::mapping::galaxy_from_board;

fn captures() -> BTreeMap<String, String> {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "tests",
        "golden",
        "hexsummary_captures.json",
    ]
    .iter()
    .collect();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    serde_json::from_str(&text).expect("the capture corpus is valid JSON")
}

fn decoded() -> BTreeMap<String, Board> {
    captures()
        .into_iter()
        .map(|(name, summary)| {
            let board =
                decode(&summary).unwrap_or_else(|error| panic!("{name} did not decode: {error}"));
            (name, board)
        })
        .collect()
}

#[test]
fn every_real_capture_decodes() {
    let boards = decoded();
    assert_eq!(boards.len(), 9, "the corpus lost a capture");
    for (name, board) in &boards {
        assert!(
            board.systems.len() >= 30,
            "{name} decoded to only {} systems, which is not a six-player table",
            board.systems.len()
        );
    }
}

#[test]
fn no_capture_produces_a_piece_with_an_impossible_count() {
    // A sticky-count bug does not fail to parse; it silently multiplies a fleet. A board where
    // some stack claims dozens of dreadnoughts is the shape that mistake takes.
    for (name, board) in decoded() {
        for system in &board.systems {
            for region in std::iter::once(&system.space).chain(system.planets.iter()) {
                for stack in &region.stacks {
                    let ceiling = match stack.piece {
                        // The plastic in the box, per colour: fighters and infantry come in
                        // large supplies, everything else does not.
                        Piece::Fighter | Piece::Infantry => 16,
                        Piece::OwnerToken | Piece::CommandToken => 1,
                        _ => 8,
                    };
                    assert!(
                        stack.count >= 1 && stack.count <= ceiling,
                        "{name} tile {}: {:?} x{} exceeds what a player owns",
                        system.tile,
                        stack.piece,
                        stack.count
                    );
                }
            }
        }
    }
}

#[test]
fn a_capture_of_a_fresh_six_player_setup_has_six_home_fleets_and_no_tokens() {
    // Setup is the one board whose contents are known in advance, which makes it the only capture
    // that can check *what* was decoded rather than only that decoding succeeded.
    let boards = decoded();
    let setup = boards.get("setup_6p_pok").expect("the setup capture");

    let occupied: Vec<&_> = setup
        .systems
        .iter()
        .filter(|system| !system.space.stacks.is_empty())
        .collect();
    assert_eq!(
        occupied.len(),
        6,
        "a fresh table has exactly six occupied systems, the home systems"
    );

    for system in occupied {
        let colours = system.space.occupiers();
        assert_eq!(
            colours.len(),
            1,
            "tile {} has more than one colour at setup",
            system.tile
        );
        assert!(
            system.command_tokens().is_empty(),
            "tile {} carries a command token before the first activation",
            system.tile
        );
    }
}

#[test]
fn control_claimed_records_the_token_the_capture_is_named_for() {
    let boards = decoded();
    let claimed = boards.get("control_claimed").expect("the capture");
    let tokens: usize = claimed
        .systems
        .iter()
        .flat_map(|system| system.planets.iter())
        .filter(|planet| planet.owner_token().is_some())
        .count();
    assert!(
        tokens > 0,
        "a capture taken after a planet was claimed must show at least one owner token"
    );
}

#[test]
fn every_capture_survives_being_decoded_twice_identically() {
    // Cheap, and it is the property a sticky-state bug breaks first: state left over between runs
    // would make the second decode differ from the first.
    for (name, summary) in captures() {
        let first = decode(&summary).expect("decodes");
        let second = decode(&summary).expect("decodes");
        assert_eq!(first, second, "{name} decoded differently the second time");
    }
}

#[test]
fn every_real_capture_places_as_a_board_rather_than_a_heap() {
    // The conversion from the mod's doubled-height grid to axial is one line, and a wrong one
    // still produces valid-looking coordinates -- they are just scattered. Connectivity is the
    // property that separates the right line from a plausible wrong one, and it is only
    // meaningful on a whole capture, where the tiles between any two are all present.
    let store = ti4_content::ContentStore::embedded();
    for (name, summary) in captures() {
        let board = decode(&summary).expect("decodes");
        let mapped = galaxy_from_board(store, &board, ti4_model::content_types::FULL)
            .unwrap_or_else(|error| panic!("{name} did not map: {error}"));

        let placed: Vec<String> = mapped
            .galaxy
            .system_ids()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        assert!(
            placed.len() >= 30,
            "{name} placed only {} tiles",
            placed.len()
        );

        // The wormhole nexus is exempt, and correctly so: it is placed outside the main galaxy
        // and reached only through wormholes, so on its locked face -- gamma only, with nothing
        // else carrying gamma -- it touches nothing at all. That is the board, not a bad
        // conversion.
        let stranded: Vec<&String> = placed
            .iter()
            .filter(|id| !id.starts_with("82"))
            .filter(|id| mapped.galaxy.adjacent(id).is_empty())
            .collect();
        assert!(
            stranded.is_empty(),
            "{name}: {stranded:?} touch nothing, so the grid conversion is wrong"
        );

        // Mecatol sits at the centre of every real table -- but not always as tile 18. These
        // captures use tile 112, the alternate printing (`mrte`, alias `mr2`), so the invariant
        // has to be about the planet rather than the tile number.
        let centre = mapped
            .galaxy
            .system_at(ti4_model::hex::Hex::ORIGIN)
            .unwrap_or_else(|| panic!("{name} has no tile at the origin"));
        let catalogue = ti4_content::galaxy::all_systems(store, ti4_model::content_types::FULL);
        let mecatol = catalogue[centre].planets().into_iter().any(|id| {
            ti4_content::galaxy::planet(store, id, ti4_model::content_types::FULL)
                .and_then(|planet| planet.name())
                .is_some_and(|planet| planet == "Mecatol Rex")
        });
        assert!(
            mecatol,
            "{name} has tile {centre} at the origin, which is not a Mecatol printing"
        );

        // The mod spells tile 1 without the corpus's leading zero, and single-digit tiles are
        // home systems. If the normalisation regressed they land in `unknown_tiles` and a seat
        // starts nowhere -- so no single-digit tile may ever be reported unknown.
        let lost_homes: Vec<&String> = mapped
            .unknown_tiles
            .iter()
            .filter(|tile| tile.len() == 1 && tile.chars().all(|c| c.is_ascii_digit()))
            .collect();
        assert!(
            lost_homes.is_empty(),
            "{name} lost single-digit tiles {lost_homes:?}; the zero-padding normalisation broke"
        );
        let single_digit = board
            .systems
            .iter()
            .filter(|system| system.tile.len() == 1)
            .count();
        assert!(
            single_digit > 0,
            "{name} has no single-digit tile, so it cannot exercise the normalisation"
        );
    }
}
