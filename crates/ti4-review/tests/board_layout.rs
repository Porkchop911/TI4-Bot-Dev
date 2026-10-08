//! Where the board goes, and only that.
//!
//! R02-006b gave the board its meaning; R02-006e moved the painting of it into `view::draw_board` so
//! the reviewer and the replayer share every stroke. A painter cannot be tested without a window, so
//! what is pinned here is the part that became testable when position was separated from meaning: the
//! layout arithmetic the painter is handed. These numbers used to be four local variables inside
//! `ReviewApp::board`, which meant the replayer could only have matched them by copying them.

// Every number in this file is exact by construction - a literal times a literal, no accumulation -
// so the tests compare exactly. A tolerance would be worse here: it would have to be small enough to
// catch a board that changed by half a pixel, and it would still let one drift by a third. Clippy's
// rule is about float error that these values cannot acquire. Map coordinates are -3..=3, so the
// casts from `i32` are exact for the same reason.
#![allow(clippy::cast_precision_loss, clippy::float_cmp)]

use eframe::egui::{Pos2, Rect, Vec2};
use ti4_content::ContentStore;
use ti4_review::view::{self, BoardLayout, TileView};
use ti4_review::{AdvanceUnit, LiveReview, ProfileTable, ReviewSession, SimulationConfig};

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("the workspace root is two levels above the crate")
        .to_path_buf()
}

fn session() -> ReviewSession {
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
    LiveReview::start(&config)
        .expect("the committed example inputs start a review")
        .session
}

/// A rectangle wide enough that neither clamp bites, so ratios are exact.
const ROOMY: Vec2 = Vec2 {
    x: 1150.0,
    y: 900.0,
};

#[test]
fn the_scale_tracks_the_window_and_clamps_at_both_ends() {
    let rect = Rect::from_center_size(Pos2::ZERO, ROOMY);
    // Exactly the design size: one pixel per unit.
    let layout = BoardLayout::new(rect, ROOMY, false);
    assert!((layout.scale - 1.0).abs() < 1e-6, "{}", layout.scale);
    assert_eq!(layout.radius, 58.0);

    // Half the window, half the board.
    let half = BoardLayout::new(rect, ROOMY * 0.5, false);
    assert!((half.scale - 0.5).abs() < 1e-6);
    assert!((half.radius - 29.0).abs() < 1e-6);

    // A window this small would put the hexes at under half a pixel each; the floor keeps the board
    // legible and lets it overflow instead.
    let tiny = BoardLayout::new(rect, Vec2::new(60.0, 40.0), false);
    assert_eq!(tiny.scale, 0.45);
    let enormous = BoardLayout::new(rect, Vec2::new(9_000.0, 9_000.0), false);
    assert_eq!(enormous.scale, 1.2, "a 4K window must not dwarf the labels");

    // The binding constraint is whichever axis runs out first.
    let narrow = BoardLayout::new(rect, Vec2::new(2_300.0, 450.0), false);
    assert!(
        (narrow.scale - 0.5).abs() < 1e-6,
        "height bound it, not width"
    );
}

#[test]
fn the_fracture_buys_its_band_out_of_the_ring() {
    let rect = Rect::from_center_size(Pos2::ZERO, ROOMY);
    let without = BoardLayout::new(rect, ROOMY, false);
    let with = BoardLayout::new(rect, ROOMY, true);
    assert_eq!(
        with.center.y + 72.0,
        without.center.y,
        "the ring lifts by exactly the band the Fracture is drawn in"
    );
    assert_eq!(with.center.x, without.center.x, "and not sideways");
    assert_eq!(
        with.scale, without.scale,
        "the Fracture does not shrink the board"
    );
}

#[test]
fn ring_tiles_sit_on_the_axial_grid() {
    let rect = Rect::from_center_size(Pos2::ZERO, ROOMY);
    let layout = BoardLayout::new(rect, ROOMY, false);
    let at = |q: i32, r: i32| view::tile_point(&layout, &tile_at(q, r, &layout));
    // A plain tile - no special area - is positioned off the ring centre.
    let origin = at(0, 0);
    assert_eq!(origin, layout.center);
    // One step along q is 126 units right.
    let east = at(1, 0);
    assert_eq!((east - origin), Vec2::new(126.0, 0.0));
    // One step along r is 108 down and half of q's 126 right - the 60° skew the hex grid has.
    let south = at(0, 1);
    assert_eq!((south - origin), Vec2::new(63.0, 108.0));
    // The projection is linear, so spacing is the same anywhere on the board: 126 along a row, 108
    // between rows, and the diagonal neighbours of a hex 125.0 away - within 1% of the row spacing,
    // which is what the hexes are drawn to look like. (It is not a regular projection: 126 and
    // sqrt(63^2 + 108^2) differ by 0.8%, and pinning that is the point, because the reviewer's board
    // was tuned by eye and the replayer must not quietly re-tune it.)
    let centre = at(3, 3);
    let distance =
        |q: i32, r: i32| (view::tile_point(&layout, &tile_at(q, r, &layout)) - centre).length();
    // Two rows down, one column back, is straight down in axial coordinates.
    assert!((distance(2, 5) - 216.0).abs() < 0.001, "two rows down");
}

fn tile_at(q: i32, r: i32, _layout: &BoardLayout) -> TileView {
    TileView {
        q,
        r,
        ..tile_like()
    }
}

/// A tile with every field the painter touches empty, so a geometry test cannot accidentally assert
/// about drawing.
fn tile_like() -> TileView {
    TileView {
        system: String::new(),
        label: String::new(),
        q: 0,
        r: 0,
        special_area: None,
        fill: view::TileFill::Plain,
        color: view::NEUTRAL_COLOR,
        anomaly_label: None,
        space_owner: None,
        planet_owners: Vec::new(),
        selected: false,
        portal_linked: false,
        ingress: false,
        egress: false,
        wormholes: Vec::new(),
        units: Vec::new(),
        command_tokens: Vec::new(),
        token_labels: Vec::new(),
        planets: Vec::new(),
    }
}

#[test]
fn the_two_special_areas_get_their_own_corners() {
    let rect = Rect::from_center_size(Pos2::ZERO, ROOMY);
    let layout = BoardLayout::new(rect, ROOMY, true);
    let mut fracture = tile_at(3, 0, &layout);
    fracture.special_area = Some("fracture".to_owned());
    let fracture_at = view::tile_point(&layout, &fracture);
    assert_eq!(fracture_at.x, rect.center().x, "centred on the band");
    assert!(
        fracture_at.y > layout.center.y,
        "below the ring, which is why the ring lifts"
    );
    assert_eq!(fracture_at.y, rect.bottom() - 61.0);

    let mut nexus = tile_at(0, 0, &layout);
    nexus.special_area = Some("nexus".to_owned());
    let nexus_at = view::tile_point(&layout, &nexus);
    assert_eq!(
        nexus_at.x,
        rect.left() + 72.0,
        "the Nexus keeps the lower-left"
    );
    assert_eq!(nexus_at.y, rect.bottom() - 61.0);
    // And the special area is spelled the way `board_view` spells it, or the tile would be drawn on
    // the ring and the band left empty.
    let _ = &layout;
    assert!(special_areas_are_lowercase());
}

fn special_areas_are_lowercase() -> bool {
    let session = session();
    let frame = session.frames.last().expect("a frame");
    let content = ContentStore::embedded();
    view::board_view(content, &session, frame, None)
        .iter()
        .filter_map(|tile| tile.special_area.as_deref())
        .all(|area| area == area.to_lowercase() && matches!(area, "fracture" | "nexus"))
}

#[test]
fn the_painter_and_the_frame_agree_about_the_fracture() {
    // `draw_board` asks the tiles whether the Fracture is on the board, because it does not get the
    // frame. That is only honest if `board_view` puts the tile there exactly when the frame says the
    // area is in play - across every frame a real review produces, not just one.
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
    let mut live = LiveReview::start(&config).expect("the review starts");
    let frames = live.session.frames.len();
    let content = ContentStore::embedded();
    let mut seen_in_play = 0;
    for index in 0..frames {
        let frame = &live.session.frames[index];
        let tiles = view::board_view(content, &live.session, frame, None);
        let shown = view::fracture_shown(&tiles);
        assert_eq!(
            shown, frame.state.fracture_in_play,
            "frame {index} disagrees about the Fracture"
        );
        seen_in_play += usize::from(shown);
    }
    live.advance(AdvanceUnit::Step, 12);
    assert!(live.session.frames.len() > frames, "the review grew");
    for frame in &live.session.frames[frames..] {
        let tiles = view::board_view(content, &live.session, frame, None);
        assert_eq!(
            view::fracture_shown(&tiles),
            frame.state.fracture_in_play,
            "a mid-game frame disagrees about the Fracture"
        );
    }
    // The example session never brings the Fracture in, so the positive case comes from the frame
    // itself: flip the flag and the tile must appear, because that is the only way `draw_board`
    // learns to title the band it draws below the ring.
    let mut in_play = live.session.frames[frames - 1].clone();
    in_play.state.fracture_in_play = true;
    let tiles = view::board_view(content, &live.session, &in_play, None);
    assert!(
        view::fracture_shown(&tiles),
        "a frame with the Fracture in play must put it on the board"
    );
    assert_eq!(
        seen_in_play, 0,
        "and the example really does start without it"
    );
}

#[test]
fn a_wider_window_moves_the_board_not_the_meaning() {
    // Same frame, two window sizes: tile count, order and every non-positional field must be
    // identical. This is the property the replayer relies on when it draws the board at a different
    // size from the reviewer.
    let session = session();
    let frame = session.frames.last().expect("a frame");
    let content = ContentStore::embedded();
    let tiles = view::board_view(content, &session, frame, None);
    let small = BoardLayout::new(
        Rect::from_center_size(Pos2::ZERO, Vec2::new(700.0, 500.0)),
        Vec2::new(700.0, 500.0),
        true,
    );
    let large = BoardLayout::new(
        Rect::from_center_size(Pos2::ZERO, Vec2::new(1100.0, 850.0)),
        Vec2::new(1100.0, 850.0),
        true,
    );
    // 37 hexes of the example arrangement, plus the Wormhole Nexus beside them. The count was 37
    // until the Nexus was placed at all on a map-pool board (it is off the grid, so the captured
    // geometry it was built from cannot contain it); the extra tile is the fix, and `tile_point`
    // below is what keeps it anchored to the window rather than to a hex.
    assert_eq!(tiles.len(), 38, "the whole example map, not a slice of it");
    let ratio = large.scale / small.scale;
    let mut ring = 0;
    for tile in &tiles {
        let at_small = view::tile_point(&small, tile);
        let at_large = view::tile_point(&large, tile);
        // The two special areas are anchored to the window's edges, not to the ring, so they move
        // with the frame rather than scaling about the centre.
        if let Some(area) = tile.special_area.as_deref() {
            // Both special areas scale the band offset with the window, as every other stroke in
            // the painter does (`- 61.0 * scale`, not `- 61.0`). This expectation used to say
            // otherwise and was dead: no frame in this test had a nexus tile, because no map-pool
            // board had a nexus at all. The assertion is what it always should have been.
            let expected = if area == "nexus" {
                Vec2::new(
                    large.rect.left() + 72.0 * large.scale,
                    large.rect.bottom() - 61.0 * large.scale,
                )
            } else {
                Vec2::new(
                    large.rect.center().x + (tile.q as f32 - 3.0) * 100.0 * large.scale,
                    large.rect.bottom() - 61.0 * large.scale,
                )
            };
            assert_eq!(
                at_large,
                Pos2::new(expected.x, expected.y),
                "{area} is not anchored where the reviewer puts it"
            );
        } else {
            ring += 1;
            assert!(
                ((at_small - small.center) * ratio - (at_large - large.center)).length() < 0.001,
                "{} moved out of proportion",
                tile.system
            );
        }
    }
    assert!(
        ring > 30,
        "most of the map is the ring: {ring} of {}",
        tiles.len()
    );
}
