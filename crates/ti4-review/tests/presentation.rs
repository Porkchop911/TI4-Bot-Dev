//! Snapshots for the presentation layer both reviewers share.
//!
//! R02-006a moved this code out of `gui.rs` without changing it, and these tests are the reason that
//! claim can be checked rather than asserted. They pin the values the two apps promise each other -
//! the seat palette, the abbreviations, the anomaly and wormhole styling, and the hex geometry - so
//! that a later re-tone is a deliberate edit to one file plus these expectations, and an accidental
//! drift in either application is a test failure here.
//!
//! Nothing here opens a window. The drawing functions take an `egui::Painter`, which needs a live
//! context, so what is pinned about them is the data they draw from: the style and geometry they are
//! handed, and the label they build.

use std::path::PathBuf;

use eframe::egui::{Color32, Pos2, Vec2};
use ti4_content::ContentStore;
use ti4_model::content_types::ContentType;
use ti4_model::id::{FactionId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::units::Unit;

use ti4_review::view;
use ti4_review::{LiveReview, ProfileTable, ReviewSession, SimulationConfig};

/// The colour of each seat, and of anything that is not a seat.
#[test]
fn the_seat_palette_is_pinned() {
    let expected: [(u8, u8, u8); 6] = [
        (224, 66, 66),
        (66, 142, 235),
        (242, 198, 56),
        (54, 184, 116),
        (173, 103, 224),
        (238, 126, 49),
    ];
    assert_eq!(
        view::SEAT_COLORS.len(),
        expected.len(),
        "one colour per seat"
    );
    for (colour, (r, g, b)) in view::SEAT_COLORS.iter().zip(expected) {
        assert_eq!(*colour, Color32::from_rgb(r, g, b));
    }
    let mut distinct = view::SEAT_COLORS.to_vec();
    distinct.sort_by_key(Color32::to_array);
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        expected.len(),
        "two seats share a colour, so a board cannot be read at a glance"
    );
    assert_eq!(view::NEUTRAL_COLOR, Color32::from_rgb(166, 174, 184));
    // The panel text is black on a light fill: colour marks a seat only as a swatch.
    assert_eq!(view::PANEL_TEXT, Color32::BLACK);
    assert_eq!(view::PANEL_FILL, Color32::from_rgb(246, 247, 250));
}

#[test]
fn a_seat_maps_to_a_colour_and_wraps_past_six() {
    for (index, colour) in view::SEAT_COLORS.iter().enumerate() {
        assert_eq!(
            view::player_color(&PlayerId::new(format!("seat{index}"))),
            *colour,
            "seat {index} must keep its colour"
        );
    }
    // A table larger than the palette reuses it rather than inventing a colour. Pinned so that a
    // seven-seat experiment is a visible decision and not a silent collision.
    assert_eq!(
        view::player_color(&PlayerId::new("seat6")),
        view::SEAT_COLORS[0]
    );
    assert_eq!(view::seat_index(&PlayerId::new("seat6")), Some(0));
}

#[test]
fn anything_that_is_not_a_seat_is_neutral() {
    for id in ["cargo", "seat", "seatx", "seat-1", "", "Seat1"] {
        let player = PlayerId::new(id);
        assert_eq!(
            view::player_color(&player),
            view::NEUTRAL_COLOR,
            "{id:?} is not a seat and must not borrow one's colour"
        );
    }
}

#[test]
fn trait_and_specialty_abbreviations_are_pinned() {
    assert_eq!(view::short_trait("cultural"), "C");
    assert_eq!(view::short_trait("hazardous"), "H");
    assert_eq!(view::short_trait("industrial"), "I");
    assert_eq!(view::short_trait("CULTURAL"), "C", "matching ignores case");
    assert_eq!(view::short_trait("none"), "·", "an unknown trait is a dot");
    assert_eq!(view::short_trait(""), "·");

    assert_eq!(view::short_specialty("biotic"), "G");
    assert_eq!(view::short_specialty("green"), "G");
    assert_eq!(view::short_specialty("cybernetic"), "Y");
    assert_eq!(view::short_specialty("yellow"), "Y");
    assert_eq!(view::short_specialty("propulsion"), "B");
    assert_eq!(view::short_specialty("blue"), "B");
    assert_eq!(view::short_specialty("warfare"), "R");
    assert_eq!(view::short_specialty("red"), "R");
    assert_eq!(
        view::short_specialty("unit upgrade"),
        "T",
        "the rest are general"
    );
    assert_eq!(view::short_specialty(""), "T");
    assert_eq!(view::short_specialty("CYBERNETIC"), "Y");
    // The first rule seen wins, so a specialty that names two colours reads as the first one.
    assert_eq!(view::short_specialty("red biotic"), "G");
}

#[test]
fn anomaly_styles_and_their_precedence_are_pinned() {
    let kinds =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    let expected = [
        ("entropic scar", (48, 24, 64), "╳ SCAR"),
        ("supernova", (105, 38, 20), "✹ NOVA"),
        ("gravity rift", (50, 28, 82), "◉ RIFT"),
        ("nebula", (55, 45, 91), "☁ NEBULA"),
        ("asteroid field", (67, 61, 52), "✦ ASTEROIDS"),
    ];
    for (kind, (r, g, b), label) in expected {
        let (color, text) = view::anomaly_style(&kinds(&[kind]))
            .unwrap_or_else(|| panic!("{kind} should style as an anomaly"));
        assert_eq!(color, Color32::from_rgb(r, g, b), "{kind} colour");
        assert_eq!(text, label, "{kind} label");
    }
    assert!(
        view::anomaly_style(&kinds(&[])).is_none(),
        "no anomaly, no styling"
    );
    assert!(view::anomaly_style(&kinds(&["nebula"])).is_some());
    assert!(
        view::anomaly_style(&kinds(&["dust belt"])).is_none(),
        "an unknown kind is not an anomaly"
    );
    // A tile with several anomalies shows one, in this order, because the worst one is the one that
    // decides what a player can do there.
    let (color, label) = view::anomaly_style(&kinds(&[
        "nebula",
        "supernova",
        "entropic scar",
        "gravity rift",
        "asteroid field",
    ]))
    .expect("a scar is an anomaly");
    assert_eq!(label, "╳ SCAR");
    assert_eq!(color, Color32::from_rgb(48, 24, 64));
    let (_, label) = view::anomaly_style(&kinds(&["asteroid field", "gravity rift"]))
        .expect("a rift is an anomaly");
    assert_eq!(
        label, "◉ RIFT",
        "the more restrictive of two anomalies wins"
    );
}

#[test]
fn wormhole_glyphs_are_pinned() {
    let expected = [
        ("alpha", (225, 82, 82), "α"),
        ("ALPHA", (225, 82, 82), "α"),
        ("beta", (72, 190, 111), "β"),
        ("gamma", (230, 184, 68), "γ"),
        ("delta", (100, 154, 238), "δ"),
    ];
    for (kind, (r, g, b), symbol) in expected {
        let (color, glyph) = view::wormhole_style(kind);
        assert_eq!(color, Color32::from_rgb(r, g, b), "{kind} colour");
        assert_eq!(glyph, symbol, "{kind} glyph");
    }
    let (color, glyph) = view::wormhole_style("omicron");
    assert_eq!(color, Color32::LIGHT_GRAY, "an unknown wormhole is grey");
    assert_eq!(glyph, "?", "and is labelled rather than guessed at");
}

// Geometry is f32 business, and the only cast is a polygon's side count.
#[allow(clippy::cast_precision_loss)]
#[test]
fn polygons_are_regular_and_rotate() {
    let center = Pos2::new(120.0, 80.0);
    let radius = 10.0;
    for sides in [3, 4, 5, 6] {
        let points = view::polygon(center, radius, sides, 0.0);
        assert_eq!(points.len(), sides);
        for point in &points {
            let offset = *point - center;
            assert!(
                (offset.length() - radius).abs() < 1e-3,
                "a vertex of a {sides}-gon left the circle: {}",
                offset.length()
            );
        }
        // Vertices are evenly spaced, in one direction, starting from the offset angle.
        let angles: Vec<f32> = points
            .iter()
            .map(|point| {
                let offset = *point - center;
                offset.y.atan2(offset.x)
            })
            .collect();
        let step = std::f32::consts::TAU / sides as f32;
        for (index, angle) in angles.iter().enumerate().skip(1) {
            // `atan2` reports in (-pi, pi], so a vertex that crosses due west steps backwards.
            let mut delta = angle - angles[index - 1];
            while delta < -std::f32::consts::PI {
                delta += std::f32::consts::TAU;
            }
            assert!(
                (delta - step).abs() < 1e-3,
                "vertex {index} of a {sides}-gon is one step of {step} away, not {delta}"
            );
        }
    }
    // Rotating by exactly one vertex step moves every point to its neighbour, which is how the board
    // draws the same glyph at a different orientation.
    let square = view::polygon(center, radius, 4, 0.0);
    let rotated = view::polygon(center, radius, 4, std::f32::consts::FRAC_PI_2);
    for (index, point) in rotated.iter().enumerate() {
        let next = (index + 1) % 4;
        assert!(
            (*point - square[next]).length() < 1e-3,
            "a quarter turn did not land on the next vertex"
        );
    }
    // The unit glyphs depend on these two specific shapes being what they look like: a fighter is a
    // triangle whose first vertex points up, a dreadnought a flat-topped hexagon.
    let fighter = view::polygon(Pos2::ZERO, 5.5, 3, -std::f32::consts::FRAC_PI_2);
    assert!(fighter[0].y < -5.0, "a fighter's nose points up");
    let hexagon = view::polygon(Pos2::ZERO, 5.5, 6, 0.0);
    assert!(
        hexagon[0].x > 5.0 && hexagon[0].y.abs() < 1e-3,
        "a dreadnought's first vertex is due east"
    );
    assert!(view::polygon(center, radius, 0, 0.0).is_empty());
    assert!(
        (view::polygon(center, radius, 1, 0.0)[0] - (center + Vec2::new(radius, 0.0))).length()
            < 1e-3
    );
}

#[test]
fn a_unit_reads_as_its_base_type() {
    let content = ContentStore::embedded();
    let unit = |type_id: &str| Unit {
        type_id: UnitTypeId::new(type_id),
        owner: PlayerId::new("seat0"),
        sustained_damage: false,
        galvanized: false,
    };
    // A unit the corpus knows resolves to its base type, which is the key the glyph is drawn from.
    assert_eq!(view::unit_base(content, &unit("fighter")), "fighter");
    assert_eq!(view::unit_base(content, &unit("destroyer")), "destroyer");
    assert_eq!(view::unit_base(content, &unit("cruiser")), "cruiser");
    assert_eq!(view::unit_base(content, &unit("carrier")), "carrier");
    assert_eq!(
        view::unit_base(content, &unit("dreadnought")),
        "dreadnought"
    );
    // A faction-specific hull falls back to its base rather than to the full id, or the board would
    // draw every flagship as an unknown blob.
    let flagship = view::unit_base(content, &unit("sol_flagship"));
    assert!(
        flagship == "flagship" || flagship == "sol_flagship",
        "a named flagship resolved to {flagship:?}"
    );
    assert_eq!(
        view::unit_base(content, &unit("no_such_unit_type")),
        "no_such_unit_type",
        "an unknown id is shown as itself"
    );
}

#[test]
fn content_labels_show_the_name_then_the_id() {
    let content = ContentStore::embedded();
    assert_eq!(
        view::content_label(content, ContentType::Technologies, "no_such_tech"),
        "no_such_tech",
        "an id the corpus does not have is shown as the id"
    );
    // Every category the reviewer can label has at least one named record, and a named record reads
    // as `Name [id]` so the reader can always see what the label really is.
    let mut labelled = 0;
    let mut checked = 0;
    for category in [
        ContentType::Planets,
        ContentType::Technologies,
        ContentType::ActionCards,
        ContentType::PublicObjectives,
    ] {
        let records = content.records(category);
        assert!(
            !records.is_empty(),
            "the corpus has no {category:?} records"
        );
        for record in records.iter().take(40) {
            let Some(id) = record.id() else { continue };
            let id = id.to_owned();
            let label = view::content_label(content, category, id.as_str());
            checked += 1;
            assert!(
                label == id || label.ends_with(&format!("[{id}]")),
                "{category:?} label {label:?} neither names {id} nor falls back to its id"
            );
            if label != id {
                labelled += 1;
                assert!(
                    label.contains('[') && label.ends_with(']'),
                    "{category:?} label {label:?} is not `name [id]`"
                );
                assert!(
                    !label.eq_ignore_ascii_case(&id),
                    "the name of {id} was the id itself, so labelling gained nothing"
                );
            }
        }
    }
    assert!(checked > 100, "only {checked} records were checked");
    assert!(
        labelled > 100,
        "only {labelled} of {checked} records labelled by name, which suggests the lookup is broken"
    );
}

/// Walk up to the workspace root, which is where the committed reviewer inputs live.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir
            .join("examples/reviewer/checkpoint-473312/slots.json")
            .is_file()
        {
            return dir;
        }
        assert!(
            dir.pop(),
            "no workspace root carries the committed reviewer inputs"
        );
    }
}

/// The planet ids of a merged tile, in the order the tile will draw them.
fn ids_of(merged: &[ti4_review::PlanetMeta]) -> Vec<&str> {
    merged.iter().map(|planet| planet.id.as_str()).collect()
}

/// A real board and a real planet catalog, from the committed example inputs. Frame zero is enough:
/// the interesting input - which planets a game has placed - is injected by the tests rather than
/// waited for, so the case is exercised instead of depending on how far a run happened to get.
fn started_session() -> ReviewSession {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let review = LiveReview::start(&config).expect("the committed example inputs start a review");
    let session = review.session;
    assert!(!session.board.is_empty(), "a review starts with a board");
    assert!(
        !session.planet_catalog.is_empty(),
        "a review carries the planet catalog"
    );
    session
}

#[test]
fn planets_merge_onto_a_tile_without_duplicates() {
    let session = started_session();
    let mut frame = session
        .frames
        .last()
        .expect("a started review has its setup frame")
        .clone();
    let tile = session
        .board
        .iter()
        .find(|tile| !tile.planets.is_empty())
        .expect("some tile prints a planet");
    let printed: Vec<String> = tile
        .planets
        .iter()
        .map(|planet| planet.id.clone())
        .collect();
    let unprinted = session
        .planet_catalog
        .iter()
        .find(|planet| !printed.contains(&planet.id))
        .expect("the catalog holds planets no tile prints")
        .clone();
    let expected = printed
        .iter()
        .map(String::as_str)
        .chain([unprinted.id.as_str()])
        .collect::<Vec<_>>();
    let ids = ids_of;

    frame.state.placed_planets.insert(
        PlanetId::new(unprinted.id.clone()),
        SystemId::new(tile.system.clone()),
    );
    assert_eq!(
        ids(&view::planets_for_tile(&session, &frame, tile)),
        expected,
        "printed planets come first, then what the game placed"
    );

    // Placing a planet the tile already prints must not list it twice, or the reader counts a world
    // that is not there.
    frame.state.placed_planets.insert(
        PlanetId::new(printed[0].clone()),
        SystemId::new(tile.system.clone()),
    );
    assert_eq!(
        ids(&view::planets_for_tile(&session, &frame, tile)),
        expected,
        "a planet the tile already prints is not listed twice"
    );
}

#[test]
fn a_tile_shows_its_own_system_and_always_the_same_answer() {
    let session = started_session();
    let mut frame = session
        .frames
        .last()
        .expect("a started review has its setup frame")
        .clone();
    let tile = session
        .board
        .iter()
        .find(|tile| !tile.planets.is_empty())
        .expect("some tile prints a planet");
    let before = view::planets_for_tile(&session, &frame, tile);

    let other = session
        .board
        .iter()
        .find(|other| other.system != tile.system)
        .expect("the board has more than one system");
    frame.state.placed_planets.insert(
        PlanetId::new("a-planet-of-another-system"),
        SystemId::new(other.system.clone()),
    );
    assert_eq!(
        view::planets_for_tile(&session, &frame, tile),
        before,
        "another system's planets are not this tile's business"
    );
    assert_eq!(
        view::planets_for_tile(&session, &frame, tile),
        before,
        "the same frame merged differently twice"
    );

    // Every tile answers, including the empty ones, and never with the same planet twice.
    for board_tile in &session.board {
        let merged = view::planets_for_tile(&session, &frame, board_tile);
        let mut unique = merged
            .iter()
            .map(|planet| planet.id.as_str())
            .collect::<Vec<_>>();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            merged.len(),
            unique.len(),
            "tile {} repeats a planet",
            board_tile.system
        );
        assert!(
            merged.len() >= board_tile.planets.len(),
            "tile {} lost a printed planet",
            board_tile.system
        );
    }
}

// ---------------------------------------------------------------------------
// Naming a system
// ---------------------------------------------------------------------------

/// The engine names a system by the number printed on its tile: `activate 43`. Exact, and unreadable
/// unless you already know the map. These two functions are what the replayer spells instead.
#[test]
fn a_system_is_named_by_its_tile_and_what_is_inside_it() {
    let session = started_session();
    let populated = session
        .board
        .iter()
        .find(|tile| tile.planets.len() > 1)
        .expect("some tile prints more than one planet");
    let label = view::system_label(&session, &populated.system);
    assert!(
        label.starts_with(&format!("{} · ", populated.system)),
        "the tile number stays first, because that is what the engine says: {label}"
    );
    for planet in &populated.planets {
        assert!(
            label.contains(planet.label.as_str()),
            "every planet in the system is named: {label}"
        );
    }

    let empty = session
        .board
        .iter()
        .find(|tile| tile.planets.is_empty())
        .expect("some tile prints no planet at all");
    assert_eq!(
        view::system_label(&session, &empty.system),
        format!("{} · {}", empty.system, empty.label),
        "an empty system still has a printed name worth reading"
    );

    assert_eq!(
        view::system_label(&session, "not-a-tile"),
        "not-a-tile",
        "a system this board does not have is returned untouched"
    );
}

/// The rewrite only touches numbers that name a tile of *this* board, and only where a number stands
/// on its own - otherwise `produce 2x fighter for 1` would grow a planet list.
#[test]
fn only_numbers_that_name_a_tile_are_rewritten() {
    let session = started_session();
    let tile = session
        .board
        .iter()
        .find(|tile| !tile.planets.is_empty())
        .expect("some tile prints a planet");
    let system = tile.system.clone();
    let named = view::system_label(&session, &system);

    assert_eq!(
        view::annotate_systems(&session, &format!("activate {system}")),
        format!("activate {named}")
    );
    assert_eq!(
        view::annotate_systems(&session, &format!("commit ground forces in {system}")),
        format!("commit ground forces in {named}")
    );
    // Attached on either side is not a tile reference.
    assert_eq!(
        view::annotate_systems(&session, &format!("sol_{system}")),
        format!("sol_{system}")
    );
    assert_eq!(
        view::annotate_systems(&session, &format!("{system}x")),
        format!("{system}x")
    );
    let untouched = "produce 2x fighter for 1 · give 0 for 1";
    assert_eq!(
        view::annotate_systems(&session, untouched),
        untouched,
        "numbers that name no tile are numbers"
    );
    assert_eq!(view::annotate_systems(&session, ""), "");
}

/// A seat reads as the faction it is playing, not as a number the reader has to remember.
#[test]
fn a_seat_is_named_by_the_faction_it_is_playing() {
    let session = started_session();
    let frame = session
        .frames
        .last()
        .expect("a started table has at least one frame");
    let content = ContentStore::embedded();
    assert!(
        !frame.state.players.is_empty(),
        "a started table seats somebody"
    );
    let named = frame
        .state
        .players
        .iter()
        .filter(|seat| !seat.faction.as_str().is_empty())
        .count();
    assert!(
        named > 0,
        "a frame after setup knows who is playing what, or this test is vacuous"
    );
    for seat in &frame.state.players {
        let label = view::seat_name(frame, &seat.id, content);
        assert!(
            label.starts_with(seat.id.as_str()),
            "the seat keeps its id, because it is also a stable address: {label}"
        );
        if seat.faction.as_str().is_empty() {
            assert_eq!(
                label,
                seat.id.as_str(),
                "no faction yet reads as the bare id"
            );
            continue;
        }
        let name = ti4_content::factions::get(content, seat.faction.as_str())
            .and_then(|f| f.name())
            .expect("a started table plays factions this content knows");
        assert!(label.contains(name), "{label} should name {name}");
    }
    // A faction this build does not know keeps its raw id in parentheses. Inventing a friendly name for
    // an unknown id in a view layer is how a wrong fact gets printed politely.
    let mut odd = frame.clone();
    odd.state.players[0].faction = FactionId::new("not_a_faction");
    let who = odd.state.players[0].id.clone();
    assert_eq!(
        view::seat_name(&odd, &who, content),
        format!("{who} (not_a_faction)")
    );
}

/// Attachments are named, and an id the content does not know is shown as that id rather than hidden.
#[test]
fn attachments_are_named_not_counted() {
    let content = ContentStore::embedded();
    let named = view::attachment_names(
        &["biotic".to_owned(), "no_such_attachment".to_owned()],
        content,
    );
    assert_eq!(named.len(), 2);
    assert_eq!(named[0], "Biotic Research Facility");
    assert_eq!(
        named[1], "no_such_attachment",
        "an attachment the content cannot name must still appear, by id"
    );
}

/// The planet rider adds up what the engine would charge, and exhausting a planet moves only the ready
/// half of each total.
#[test]
fn planet_totals_split_ready_from_total() {
    let session = started_session();
    let frame = session
        .frames
        .last()
        .expect("a started table has at least one frame");
    let content = ContentStore::embedded();
    let seat = frame.state.players[0].id.clone();
    let owned: Vec<PlanetId> = frame
        .state
        .controlled_planets(&seat)
        .into_iter()
        .map(|(_, planet)| planet.clone())
        .collect();
    assert!(
        !owned.is_empty(),
        "a seated player starts with a home system"
    );

    let fresh = view::planet_totals(&session, frame, &seat, content);
    assert_eq!(fresh.planets, owned.len());
    assert_eq!(
        fresh.ready,
        owned.len(),
        "nothing is exhausted at the start"
    );
    assert_eq!(fresh.resources_ready, fresh.resources_total);
    assert_eq!(fresh.influence_ready, fresh.influence_total);
    assert!(
        fresh.resources_total > 0,
        "a home system is worth something"
    );

    let mut spent = frame.clone();
    spent.state.exhausted_planets.insert(owned[0].clone());
    let after = view::planet_totals(&session, &spent, &seat, content);
    assert_eq!(after.ready, owned.len() - 1);
    assert_eq!(after.resources_total, fresh.resources_total);
    assert_eq!(after.influence_total, fresh.influence_total);
    assert!(
        after.resources_ready + after.influence_ready
            < fresh.resources_ready + fresh.influence_ready
    );

    let lines = view::planet_totals_lines(&after);
    assert_eq!(
        lines[0],
        format!(
            "Resources {} ready / {}",
            after.resources_ready, after.resources_total
        )
    );
    assert!(lines[1].starts_with("Influence "));
}

#[test]
fn planet_totals_lines_name_traits_and_ready_specialties() {
    let mut totals = view::PlanetTotals::default();
    totals.traits.insert("cultural".into(), 2);
    totals.traits.insert("industrial".into(), 1);
    totals.specialties.insert("warfare".into(), (0, 1));
    let lines = view::planet_totals_lines(&totals);
    assert_eq!(lines[2], "Traits cultural 2 · industrial 1");
    assert_eq!(lines[3], "Specialties warfare 0/1 (ready/total)");
    assert_eq!(
        view::planet_totals_lines(&view::PlanetTotals::default()).len(),
        2,
        "no traits or specialties, no empty lines"
    );
}

/// Engine text names a seat alone; the reader always sees the seat with its faction, whether the
/// session carries frames (R01) or is a frame-less shell (the replayer).
#[test]
fn a_seat_in_engine_text_is_named_with_its_faction() {
    let session = started_session();
    let content = ContentStore::embedded();
    let frame = session.frames.last().expect("a started table has frames");
    let seat2 = PlayerId::new("seat2");
    let named = view::seat_name(frame, &seat2, content);
    assert_ne!(named, "seat2", "the fixture seats a faction at seat2");

    assert_eq!(
        view::annotate_seats(&session, "seat2 offers a deal to seat0."),
        format!(
            "{named} offers a deal to {}.",
            view::seat_name(frame, &PlayerId::new("seat0"), content)
        )
    );
    // Not a seat token: glued to a word, or followed by more identifier.
    assert_eq!(view::annotate_seats(&session, "myseat2"), "myseat2");
    assert_eq!(view::annotate_seats(&session, "seat2x"), "seat2x");
    assert_eq!(view::annotate_seats(&session, "seats"), "seats");
    assert_eq!(view::annotate_seats(&session, "seat"), "seat");
    // Named once, never twice.
    let once = view::annotate_seats(&session, "seat2 passes");
    assert_eq!(view::annotate_seats(&session, &once), once);

    let mut shell = session.clone();
    shell.frames.clear();
    assert_eq!(
        view::annotate_seats(&shell, "seat2 passes"),
        once,
        "the manifest names the seat when the session carries no frames"
    );
}

/// UI-05: the policy's numbers are named for what they are, and the legend says what they are not.
#[test]
fn policy_numbers_are_named_and_never_a_win_chance() {
    assert_eq!(
        view::policy_odds(Some(-11.591), Some(0.006)),
        " · logit -11.59 · bot picks 0.6%"
    );
    assert_eq!(
        view::policy_odds(None, None),
        "",
        "a human seat's options carry no numbers"
    );
    assert_eq!(view::policy_odds(None, Some(0.25)), " · bot picks 25.0%");
    let legend = view::policy_odds_legend(0.5);
    assert!(legend.contains("temperature 0.5"));
    assert!(legend.contains("Not a chance of winning"));
}

/// OP-03: a reviewed game carries its dice on the frames, combat rolls say whose they are, and the
/// line a reader sees names the seat with its faction and counts the hits.
#[test]
fn combat_dice_reach_the_frames_with_their_roller() {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let mut review = LiveReview::start(&config).expect("the example table starts");
    let content = ContentStore::embedded();
    for _ in 0..20_000 {
        let frame = review.step_once().clone();
        if let Some(roll) = frame
            .rolls
            .iter()
            .find(|roll| roll.reason == "space combat" || roll.reason == "ground combat")
        {
            let seat = roll.by.clone().expect("a combat roll names its roller");
            let line = view::roll_line(&frame, roll, content);
            assert!(
                line.starts_with(&view::seat_name(&frame, &PlayerId::new(&seat), content)),
                "{line}"
            );
            assert!(line.contains(&format!("→ {} hit", roll.hits())), "{line}");
            let shown = view::rolls_for_action(&review.session.frames, &frame);
            assert!(
                shown.contains(roll),
                "the step's own roll is on the Dice list"
            );
            return;
        }
        if frame.finished {
            break;
        }
    }
    panic!("the example game rolled no combat dice in 20 000 steps");
}

/// UI-03: an attachment says what it does, from its own record, and nothing when the record is
/// silent.
#[test]
fn attachments_say_what_they_do() {
    let content = ContentStore::embedded();
    assert_eq!(
        view::attachment_effect("dysonsphere", content).as_deref(),
        Some("+2 resources, +1 influence")
    );
    assert_eq!(
        view::attachment_effect("biotic", content).as_deref(),
        Some("gives the biotic specialty")
    );
    assert_eq!(view::attachment_effect("dmz", content), None);
    assert_eq!(
        view::attachment_labels(&["miningworld".to_owned()], content),
        vec!["Mining World (+2 resources)".to_owned()]
    );
}

/// UI-01: this round's draws, most recent first, with who, where, which card and what came of it.
#[test]
fn exploration_draws_are_listed_for_the_round() {
    let session = started_session();
    let content = ContentStore::embedded();
    let mut frame = session.frames.last().expect("a frame").clone();
    let seat = frame.state.players[0].id.clone();
    let planet = PlanetId::new("mehar_xull");
    frame.state.exploration_log = vec![
        ti4_model::state::ExplorationRecord {
            round: frame.state.round,
            player: seat.clone(),
            deck: "HAZARDOUS".to_owned(),
            card: "mw".to_owned(),
            planet: Some(planet),
            outcome: "attached:miningworld".to_owned(),
        },
        ti4_model::state::ExplorationRecord {
            round: frame.state.round.saturating_sub(1),
            player: seat,
            deck: "CULTURAL".to_owned(),
            card: "ds".to_owned(),
            planet: None,
            outcome: "discarded".to_owned(),
        },
    ];
    let lines = view::exploration_lines(&session, &frame, content);
    assert_eq!(lines.len(), 1, "only this round: {lines:?}");
    assert!(lines[0].contains("hazardous"), "{}", lines[0]);
    assert!(
        lines[0].contains("Mining World → attached Mining World (+2 resources)"),
        "{}",
        lines[0]
    );
}

/// The replayer states each seat's remaining negotiations (2 per action, 6 per round).
#[test]
fn a_seat_shows_its_negotiation_budget() {
    let session = started_session();
    let mut frame = session.frames.last().expect("a frame").clone();
    let seat = frame.state.players[0].id.clone();
    assert_eq!(
        view::negotiation_budget(&frame, &seat),
        "negotiations left: 2 of 2 this action, 6 of 6 this round"
    );
    frame.state.note_negotiation(&seat);
    frame.state.negotiations_this_round.insert(seat.clone(), 6);
    assert_eq!(
        view::negotiation_budget(&frame, &seat),
        "negotiations left: 0 of 2 this action, 0 of 6 this round",
        "the round's limit caps the action's"
    );
}

/// A gravity-rift die is a survival roll, not a hit roll: 1-3 destroys the ship (41.2), so a 7
/// must read as the ship surviving. Reported 2026-09-23 as "gravity rift · hits on 4+: 7 → 1 hit".
#[test]
fn a_rift_die_says_whether_the_ship_survived() {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let review = LiveReview::start(&config).expect("the example table starts");
    let frame = review.session.frames.last().expect("a first frame");
    let content = ContentStore::embedded();
    let rift = |face: u32| ti4_engine::dice::Roll {
        reason: "gravity rift".to_owned(),
        faces: vec![face],
        hits_on: Some(4),
        rerolled: std::collections::BTreeSet::new(),
        by: None,
    };
    let survived = view::roll_line(frame, &rift(7), content);
    assert!(
        survived.ends_with("survives on 4+: 7 → survives"),
        "{survived}"
    );
    let lost = view::roll_line(frame, &rift(2), content);
    assert!(lost.ends_with("survives on 4+: 2 → destroyed"), "{lost}");
}

/// A vote names only its outcomes, so the agenda under vote is found from the engine's reveal event
/// and shown as a card. Reported 2026-09-23: a human voting could not see the agenda.
#[test]
fn the_agenda_under_vote_is_found_and_shown() {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    let review = LiveReview::start(&config).expect("the example table starts");
    let base = review.session.frames.last().expect("a first frame").clone();
    let at = |index: usize, events: &[&str]| {
        let mut frame = base.clone();
        frame.index = index;
        frame.phase = ti4_model::state::Phase::Agenda;
        frame.new_events = events.iter().map(|event| (*event).to_owned()).collect();
        frame
    };
    let frames = vec![
        at(0, &["AGENDA_REVEALED:revolution"]),
        at(1, &[]),
        at(2, &["AGENDA_RESOLVED:revolution:for"]),
    ];
    assert_eq!(
        view::current_agenda(&frames, &frames[1]).as_deref(),
        Some("revolution")
    );
    assert_eq!(view::current_agenda(&frames, &frames[2]), None, "resolved");
    let card = view::agenda_card(ContentStore::embedded(), "revolution");
    assert!(card[0].contains("Anti-Intellectual Revolution"), "{card:?}");
    assert!(card.iter().any(|line| line.starts_with("For:")), "{card:?}");
    assert!(
        card.iter().any(|line| line.starts_with("Against:")),
        "{card:?}"
    );
}
