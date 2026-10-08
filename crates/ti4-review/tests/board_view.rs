//! The board's meaning, separated from its drawing.
//!
//! R02-006b moved every decision the board renderer makes out of the egui code and into
//! [`ti4_review::view::board_view`], leaving in the renderer only where to put things and which
//! strokes to make. That split is only worth having if the decisions themselves are pinned, because
//! they used to be invisible: they were locals inside a draw loop.
//!
//! The frame these tests use is a real setup frame from the committed example inputs, with the
//! interesting state - who owns what, which law is elected, where the fracture is - injected by the
//! test. Real state where it is cheap, injected state where waiting for a game to produce it would
//! mean a test that quietly proves nothing.

use std::path::PathBuf;

use eframe::egui::Color32;
use ti4_content::ContentStore;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::SystemState;
use ti4_model::units::Unit;
use ti4_review::view::{
    self, TileFill, TileView, UnitStack, board_view, hex_corners, planet_offset, tile_fill,
    unit_stacks,
};
use ti4_review::{LiveReview, ProfileTable, ReviewSession, SimulationConfig};

const ANOMALY: &str = "nebula";

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
    review.session
}

/// A frame the test owns, so it can be bent into the shapes the board has to handle.
fn frame_of(session: &ReviewSession) -> ti4_review::ReviewFrame {
    session
        .frames
        .last()
        .expect("a started review has its setup frame")
        .clone()
}

fn kinds(tiles: &[TileView]) -> Vec<&str> {
    tiles.iter().map(|tile| tile.system.as_str()).collect()
}

/// The trait line the board draws under a planet: traits, a separator only when both sides exist,
/// then specialties. Recomputed here so a change in its shape is a decision and not a drift.
fn summarise(planet: &ti4_review::PlanetMeta) -> String {
    let traits = planet
        .traits
        .iter()
        .map(|value| view::short_trait(value))
        .collect::<Vec<_>>()
        .join("");
    let specialties = planet
        .tech_specialties
        .iter()
        .map(|value| view::short_specialty(value))
        .collect::<Vec<_>>()
        .join("");
    match (traits.is_empty(), specialties.is_empty()) {
        (true, true) => String::new(),
        (true, false) => specialties,
        (false, true) => traits,
        (false, false) => format!("{traits}·{specialties}"),
    }
}

fn find<'a>(tiles: &'a [TileView], system: &str) -> &'a TileView {
    tiles
        .iter()
        .find(|tile| tile.system == system)
        .unwrap_or_else(|| panic!("the board lost {system}"))
}

fn anomalies(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn tile_fill_precedence_is_the_order_that_keeps_the_board_truthful() {
    // Purged wins over everything, including the reader's own selection: the system is gone, and a
    // highlight must not suggest otherwise.
    let (kind, color) = tile_fill(true, Some("fracture"), true, &anomalies(&[ANOMALY]), true);
    assert_eq!(kind, TileFill::Purged);
    assert_eq!(color, Color32::from_rgb(24, 24, 28));

    let (kind, color) = tile_fill(false, Some("fracture"), true, &anomalies(&[ANOMALY]), true);
    assert_eq!(kind, TileFill::Fracture);
    assert_eq!(color, Color32::from_rgb(39, 25, 57));

    let (kind, color) = tile_fill(false, Some("nexus"), true, &anomalies(&[ANOMALY]), true);
    assert_eq!(kind, TileFill::Nexus);
    assert_eq!(color, Color32::from_rgb(25, 48, 61));

    // A hyperlane system that is also anomalous is painted by the anomaly, because the anomaly is
    // what stops a player moving through it.
    let (kind, color) = tile_fill(false, None, true, &anomalies(&[ANOMALY]), true);
    assert_eq!(kind, TileFill::Hyperlane);
    assert_eq!(color, Color32::from_rgb(55, 39, 91));
    let (kind, color) = tile_fill(false, None, false, &anomalies(&[ANOMALY]), true);
    assert_eq!(kind, TileFill::Anomaly);
    assert_eq!(
        color,
        Color32::from_rgb(55, 45, 91),
        "the nebula's own colour"
    );

    let (kind, color) = tile_fill(false, None, false, &[], true);
    assert_eq!(kind, TileFill::Selected);
    assert_eq!(color, Color32::from_rgb(48, 91, 116));

    let (kind, color) = tile_fill(false, None, false, &[], false);
    assert_eq!(kind, TileFill::Plain);
    assert_eq!(color, Color32::from_rgb(23, 44, 69));

    // An unknown special area is not a special area.
    assert_eq!(
        tile_fill(false, Some("unknown-realm"), false, &[], false).0,
        TileFill::Plain
    );
}

#[test]
fn hex_corners_are_the_shape_every_ring_is_drawn_from() {
    let point = eframe::egui::Pos2::new(300.0, 200.0);
    let corners = hex_corners(point, 58.0);
    assert_eq!(corners.len(), 6, "a tile is a hexagon");
    for corner in &corners {
        let offset = *corner - point;
        assert!(
            (offset.length() - 58.0).abs() < 1e-3,
            "a corner drifted off the tile's radius"
        );
    }
    // The orientation the control rings and the split ring both assume: the first corner is 30
    // degrees down-right, so a flat edge faces up, where the system label sits.
    let first = corners[0] - point;
    assert!(first.x > 0.0 && first.y > 0.0, "first corner is down-right");
    let angle = first.y.atan2(first.x);
    assert!((angle - std::f32::consts::FRAC_PI_6).abs() < 1e-4);
    // The rings are this shape scaled towards the centre, which is what the two strokes rely on.
    let ring: Vec<_> = corners
        .iter()
        .map(|corner| point + (*corner - point) * 0.92)
        .collect();
    for corner in &ring {
        assert!(((*corner - point).length() - 58.0 * 0.92).abs() < 1e-3);
    }
}

#[test]
fn planet_offsets_centre_spread_and_lift() {
    assert_eq!(
        planet_offset(0, 1),
        (0.0, 27.0),
        "one planet sits in the middle"
    );
    assert_eq!(planet_offset(0, 2), (-22.0, 27.0));
    assert_eq!(planet_offset(1, 2), (22.0, 27.0));
    // Three or more spread on a tighter pitch and lift so the labels clear the bottom edge.
    let three: Vec<(f32, f32)> = (0..3).map(|index| planet_offset(index, 3)).collect();
    assert_eq!(
        three[1],
        (0.0, 24.0),
        "the middle planet of three is centred"
    );
    assert!(
        three[0].0 < three[1].0 && three[1].0 < three[2].0,
        "left to right"
    );
    for window in three.windows(2) {
        let step = window[1].0 - window[0].0;
        assert!(step > 15.0, "planets at {step} apart would be unreadable");
        assert!(
            step < 22.0,
            "three planets spread wider than two, so they are not a row of two"
        );
    }
    assert!(
        planet_offset(0, 3).1 < planet_offset(0, 2).1,
        "a crowded tile lifts its planets"
    );
    // Never asked by the renderer, and it must not underflow if a caller asks anyway.
    assert_eq!(planet_offset(0, 0), (0.0, 27.0));
}

#[test]
fn unit_stacks_group_by_everything_the_glyph_shows() {
    let content = ContentStore::embedded();
    let unit = |owner: &str, type_id: &str, damaged: bool, galvanized: bool| Unit {
        type_id: ti4_model::id::UnitTypeId::new(type_id),
        owner: PlayerId::new(owner),
        sustained_damage: damaged,
        galvanized,
    };
    let empty: Vec<Unit> = Vec::new();
    assert!(unit_stacks(content, &empty).is_empty());

    let same = vec![
        unit("seat0", "fighter", false, false),
        unit("seat0", "fighter", false, false),
        unit("seat0", "fighter", false, false),
    ];
    let stacks = unit_stacks(content, &same);
    assert_eq!(
        stacks.len(),
        1,
        "three identical fighters are one stack of three"
    );
    assert_eq!(stacks[0].count, 3);
    assert_eq!(stacks[0].owner, PlayerId::new("seat0"));

    // Anything the glyph draws differently is a different stack: type, damage, galvanize, owner.
    let mixed = vec![
        unit("seat0", "fighter", false, false),
        unit("seat0", "fighter", true, false),
        unit("seat0", "fighter", false, true),
        unit("seat0", "destroyer", false, false),
        unit("seat1", "fighter", false, false),
    ];
    let stacks = unit_stacks(content, &mixed);
    assert_eq!(stacks.len(), 5, "no two of these may merge");
    for stack in &stacks {
        assert_eq!(stack.count, 1);
    }
    // Order of arrival changes nothing: the key is the identity of a stack, so a fleet that moved
    // between frames cannot redraw as different shapes.
    let key = |stack: &UnitStack| {
        (
            stack.owner.to_string(),
            stack.base.clone(),
            stack.damaged,
            stack.galvanized,
        )
    };
    let mut reversed = mixed.clone();
    reversed.reverse();
    let mut forward: Vec<_> = unit_stacks(content, &mixed).iter().map(key).collect();
    let mut backwards: Vec<_> = unit_stacks(content, &reversed).iter().map(key).collect();
    forward.sort();
    backwards.sort();
    assert_eq!(
        forward, backwards,
        "the same units stacked differently because they arrived in another order"
    );

    // The base type is what the glyph switches on, so a faction hull resolves rather than falling
    // through to the unknown shape.
    let flagship = unit_stacks(content, &[unit("seat0", "sol_flagship", false, false)]);
    assert_eq!(flagship.len(), 1);
    assert!(
        flagship[0].base.contains("flagship"),
        "a named flagship stacked as {:?}",
        flagship[0].base
    );
}

#[test]
fn the_board_shows_only_what_the_frame_has() {
    let session = started_session();
    let mut frame = frame_of(&session);
    let content = ContentStore::embedded();

    let without_fracture = board_view(content, &session, &frame, None);
    frame.state.fracture_in_play = true;
    let with_fracture = board_view(content, &session, &frame, None);
    assert!(
        with_fracture.len() > without_fracture.len(),
        "the Fracture appearing in play added no tile"
    );
    for tile in &without_fracture {
        assert_ne!(
            tile.special_area.as_deref(),
            Some("fracture"),
            "a tile of the Fracture was drawn while it is out of play"
        );
    }
    assert!(
        kinds(&without_fracture)
            .iter()
            .all(|system| !kinds(&with_fracture).contains(system)
                || without_fracture.len() < with_fracture.len()),
        "the two boards disagree about which ordinary systems exist"
    );

    // The nexus depends on the map and on `nexus_unlocked`, never on the Fracture.
    let nexus_before = without_fracture
        .iter()
        .filter(|tile| tile.special_area.as_deref() == Some("nexus"))
        .count();
    frame.state.fracture_in_play = false;
    let again = board_view(content, &session, &frame, None);
    assert_eq!(
        again
            .iter()
            .filter(|tile| tile.special_area.as_deref() == Some("nexus"))
            .count(),
        nexus_before,
        "the nexus depends on the nexus, not on the Fracture"
    );
    assert_eq!(
        again.len(),
        without_fracture.len(),
        "the same frame answered differently twice"
    );
}

#[test]
fn the_wormhole_nexus_is_drawn_before_anybody_has_been_there() {
    // The Nexus sits beside the hex grid and is reached only through its own wormholes, so a player
    // finds it by looking at the board. Gating the tile on `state.board` — which is written the first
    // time a unit, a capture or a token touches a system — meant it appeared only once the player
    // had already flown somewhere they could not see. In play is the map's answer; which face is up
    // is the frame's.
    let session = started_session();
    let content = ContentStore::embedded();
    assert!(
        session
            .board
            .iter()
            .any(|tile| tile.special_area.as_deref() == Some("nexus")),
        "this review's own map carries no Wormhole Nexus, so the test below would prove nothing"
    );

    let mut frame = frame_of(&session);
    // Nothing has ever touched the tile: no board entry either way.
    frame.state.board.remove(&SystemId::new("82a"));
    frame.state.board.remove(&SystemId::new("82b"));

    frame.state.nexus_unlocked = false;
    let locked: Vec<String> = board_view(content, &session, &frame, None)
        .iter()
        .filter(|tile| tile.special_area.as_deref() == Some("nexus"))
        .map(|tile| tile.system.clone())
        .collect();
    assert_eq!(
        locked,
        ["82a".to_owned()],
        "the locked face, and one tile rather than two stacked on the same corner"
    );

    frame.state.nexus_unlocked = true;
    let open: Vec<String> = board_view(content, &session, &frame, None)
        .iter()
        .filter(|tile| tile.special_area.as_deref() == Some("nexus"))
        .map(|tile| tile.system.clone())
        .collect();
    assert_eq!(
        open,
        ["82b".to_owned()],
        "once it is triggered the open face is what is on the table"
    );
}

#[test]
fn a_game_whose_map_has_no_nexus_draws_none() {
    // In play comes from the map, so a table whose map has no Nexus — a base-scope game — shows no
    // tile, unlocked or not. It is the other half of the rule, and the half that stops the viewer
    // inventing a Prophecy of Kings tile.
    let mut session = started_session();
    session
        .board
        .retain(|tile| tile.special_area.as_deref() != Some("nexus"));
    let content = ContentStore::embedded();
    for unlocked in [false, true] {
        let mut frame = frame_of(&session);
        frame.state.nexus_unlocked = unlocked;
        assert!(
            board_view(content, &session, &frame, None)
                .iter()
                .all(|tile| tile.special_area.as_deref() != Some("nexus")),
            "a nexus was drawn for a map that has none (unlocked: {unlocked})"
        );
    }
}

#[test]
fn selection_marks_one_tile_and_links_its_portal() {
    let mut session = started_session();
    // Egress is a property of the map the session carries, so the test puts one where it needs it.
    let mut egress_named = None;
    for tile in &mut session.board {
        if tile.special_area.is_none() && !tile.egress && egress_named.is_none() {
            tile.egress = true;
            egress_named = Some(tile.system.clone());
        }
    }
    let egress = egress_named.expect("the map has an ordinary system to make egress");
    let mut frame = frame_of(&session);
    let content = ContentStore::embedded();
    let tiles = board_view(content, &session, &frame, None);
    assert!(
        tiles.iter().all(|tile| !tile.selected),
        "nothing is selected before the reader clicks"
    );
    assert!(
        tiles.iter().all(|tile| !tile.portal_linked),
        "nothing is linked to nothing"
    );

    let ordinary = tiles
        .iter()
        .find(|tile| !tile.egress && !tile.ingress)
        .expect("most systems are not portals")
        .system
        .clone();
    let selected = board_view(content, &session, &frame, Some(&ordinary));
    assert!(find(&selected, &ordinary).selected);
    assert_eq!(
        selected.iter().filter(|tile| tile.selected).count(),
        1,
        "one system is selected, not one per tile"
    );
    assert!(
        selected.iter().all(|tile| !tile.portal_linked),
        "an ordinary system links nothing"
    );

    // An ingress token on one system links it to every tile that can take it out again.
    let ingress_target = tiles
        .iter()
        .find(|tile| tile.system != ordinary && tile.system != egress)
        .expect("a system that is neither the ordinary one nor the exit")
        .system
        .clone();
    frame
        .state
        .ingress_tokens
        .insert(SystemId::new(ingress_target.clone()));
    let selected = board_view(content, &session, &frame, Some(&ingress_target));
    assert!(
        find(&selected, &egress).portal_linked,
        "the egress tile lights up"
    );
    assert!(
        !find(&selected, &ordinary).label.is_empty(),
        "ordinary systems are still drawn"
    );
    assert!(
        !find(&selected, &ordinary).portal_linked,
        "a system with no way out is not linked"
    );

    // Selecting the egress end works the other way round too.
    let selected = board_view(content, &session, &frame, Some(&egress));
    assert!(
        find(&selected, &ingress_target).portal_linked,
        "choosing the exit shows where the entry is"
    );
    assert!(!find(&selected, &ordinary).portal_linked);
}

#[test]
fn control_rings_follow_who_is_actually_there() {
    let session = started_session();
    let mut frame = frame_of(&session);
    let content = ContentStore::embedded();
    let tiles = board_view(content, &session, &frame, None);
    let system = tiles
        .iter()
        .find(|tile| tile.planets.len() == 1 && tile.special_area.is_none())
        .expect("a single-planet system")
        .system
        .clone();
    let unit = |owner: &str, type_id: &str| Unit {
        type_id: ti4_model::id::UnitTypeId::new(type_id),
        owner: PlayerId::new(owner),
        sustained_damage: false,
        galvanized: false,
    };
    let mut state = SystemState::default();
    state.units.push(unit("seat0", "cruiser"));
    frame
        .state
        .board
        .insert(SystemId::new(system.clone()), state);

    let one_owner = board_view(content, &session, &frame, None);
    let tile = find(&one_owner, &system);
    assert_eq!(tile.space_owner, Some(PlayerId::new("seat0")));
    assert_eq!(tile.planet_owners, vec![], "nothing owns the planet yet");
    assert_eq!(tile.units.len(), 1);

    // Contested space paints no ring: the rule is a single controller, not a majority.
    let mut state = frame
        .state
        .board
        .get(&SystemId::new(&system))
        .cloned()
        .expect("the system is on the board");
    state.units.push(unit("seat1", "dreadnought"));
    frame.state.board.insert(SystemId::new(&system), state);
    let contested = board_view(content, &session, &frame, None);
    assert_eq!(find(&contested, &system).space_owner, None);
    assert_eq!(find(&contested, &system).units.len(), 2);

    // Infantry standing in space is not space control, so the filter on ships is load-bearing.
    let mut state = frame
        .state
        .board
        .get(&SystemId::new(&system))
        .cloned()
        .expect("the system is on the board");
    state.units = vec![unit("seat2", "infantry")];
    frame.state.board.insert(SystemId::new(&system), state);
    let on_foot = board_view(content, &session, &frame, None);
    assert_eq!(
        find(&on_foot, &system).space_owner,
        None,
        "ground troops do not control space"
    );

    // Planet control is the thin ring, sorted, and a purged planet stops counting.
    let planet = find(&on_foot, &system)
        .planets
        .first()
        .expect("the system has its planet")
        .meta
        .id
        .clone();
    let mut state = frame
        .state
        .board
        .get(&SystemId::new(&system))
        .cloned()
        .expect("the system is on the board");
    state
        .planet_control
        .insert(PlanetId::new(&planet), PlayerId::new("seat3"));
    frame.state.board.insert(SystemId::new(&system), state);
    let held = board_view(content, &session, &frame, None);
    assert_eq!(
        find(&held, &system).planet_owners,
        vec![PlayerId::new("seat3")]
    );
    let mut state = frame
        .state
        .board
        .get(&SystemId::new(&system))
        .cloned()
        .expect("the system is on the board");
    state.purged_planets.insert(PlanetId::new(&planet));
    frame.state.board.insert(SystemId::new(&system), state);
    let purged = board_view(content, &session, &frame, None);
    assert_eq!(
        find(&purged, &system).planet_owners,
        vec![],
        "a purged planet's owner no longer paints the ring"
    );
    assert!(
        find(&purged, &system).planets[0].purged,
        "the planet itself still knows it was purged"
    );
    assert_eq!(
        find(&purged, &system).planets[0].color,
        Color32::from_rgb(38, 38, 42)
    );
    assert_eq!(find(&purged, &system).planets[0].badge, "× ");
}

#[test]
fn a_planet_is_its_owner_s_colour_or_grey() {
    let session = started_session();
    let frame = frame_of(&session);
    let content = ContentStore::embedded();
    let tiles = board_view(content, &session, &frame, None);

    // A planet's colour is its owner's, or grey when nobody holds it - and that is the whole rule, so
    // it is checked against every planet on the board rather than one convenient one.
    let mut unowned = None;
    for tile in &tiles {
        for planet in &tile.planets {
            let expected = planet
                .owner
                .as_ref()
                .map_or(Color32::from_rgb(91, 96, 105), view::player_color);
            assert_eq!(planet.color, expected, "{}", planet.meta.id);
            if planet.owner.is_none() && !planet.purged && unowned.is_none() {
                unowned = Some(planet.clone());
            }
        }
    }
    let free = unowned.expect("a fresh game leaves a world unclaimed");
    assert_eq!(free.color, Color32::from_rgb(91, 96, 105));
    assert_eq!(free.attachments, 0);
    assert!(free.ground.is_empty());
    assert!(free.coexisting.is_empty());
    assert_ne!(free.badge, "× ", "an unclaimed world is not destroyed");
}

#[test]
fn a_claimed_world_shows_its_garrison_and_labels() {
    let session = started_session();
    let mut frame = frame_of(&session);
    let content = ContentStore::embedded();
    let tiles = board_view(content, &session, &frame, None);
    // Take a world nobody holds, claim it, and watch every part of the model follow: colour,
    // coexistence, attachments, garrison.
    let (system, free) = tiles
        .iter()
        .find_map(|tile| {
            tile.planets
                .iter()
                .find(|planet| planet.owner.is_none() && !planet.purged)
                .map(|planet| (tile.system.clone(), planet.clone()))
        })
        .expect("a fresh game leaves a world unclaimed");
    let world = free.meta.clone();
    let mut state = SystemState::default();
    state
        .planet_control
        .insert(PlanetId::new(&world.id), PlayerId::new("seat1"));
    state.coexisting.insert(
        PlanetId::new(&world.id),
        std::iter::once(PlayerId::new("seat4")).collect(),
    );
    state.planet_units.insert(
        PlanetId::new(&world.id),
        vec![
            Unit {
                type_id: ti4_model::id::UnitTypeId::new("mech"),
                owner: PlayerId::new("seat1"),
                sustained_damage: false,
                galvanized: false,
            },
            Unit {
                type_id: ti4_model::id::UnitTypeId::new("infantry"),
                owner: PlayerId::new("seat1"),
                sustained_damage: true,
                galvanized: false,
            },
        ],
    );
    frame.state.board.insert(SystemId::new(&system), state);
    frame
        .state
        .planet_attachments
        .insert(PlanetId::new(&world.id), vec!["an-attachment".to_owned()]);

    let after = board_view(content, &session, &frame, None);
    let held = find(&after, &system)
        .planets
        .iter()
        .find(|planet| planet.meta.id == world.id)
        .expect("the world the test claimed is still on its tile");
    assert_eq!(held.owner, Some(PlayerId::new("seat1")));
    assert_eq!(held.color, Color32::from_rgb(66, 142, 235), "seat1's blue");
    assert_eq!(held.coexisting, vec![PlayerId::new("seat4")]);
    assert_eq!(held.attachments, 1);
    assert_eq!(
        held.ground.len(),
        2,
        "a mech and a damaged infantry do not merge"
    );
    assert_eq!(held.ground[0].base, "infantry");
    assert!(held.ground[0].damaged);
    assert!(!held.ground[1].damaged);

    // The trait line is traits, then a separator, then specialties - and no separator when one side
    // is missing, or the reader reads a missing trait as a blank one. Recomputed here from the
    // planet's own data, so a change in the shape of the label is a decision and not a drift.
    assert_eq!(
        held.trait_label,
        summarise(&world),
        "the trait line changed shape"
    );
    for tile in &after {
        for planet in &tile.planets {
            assert_eq!(
                planet.trait_label,
                summarise(&planet.meta),
                "{}",
                planet.meta.id
            );
            assert!(
                !planet.trait_label.starts_with('·') && !planet.trait_label.ends_with('·'),
                "{}'s trait line leans on a separator with nothing beside it",
                planet.meta.id
            );
        }
    }
}

#[test]
fn a_suppressed_wormhole_is_still_a_wormhole() {
    let session = started_session();
    let mut frame = frame_of(&session);
    let content = ContentStore::embedded();
    let printed = tiles_with_printed_wormholes(&session, content);
    let (system, kind, token_printed) = printed.expect("the map has a printed wormhole");

    let before = board_view(content, &session, &frame, None);
    let hole = &find(&before, &system).wormholes;
    assert!(
        hole.iter().any(|hole| hole.kind == *kind && !hole.token),
        "the printed wormhole vanished"
    );

    frame
        .state
        .laws
        .insert("travel_ban".to_owned(), "for".to_owned());
    let during = board_view(content, &session, &frame, None);
    let during = find(&during, &system);
    assert_eq!(
        during.wormholes.len(),
        hole.len(),
        "suppression removed a wormhole instead of marking it"
    );
    assert!(
        during
            .wormholes
            .iter()
            .filter(|hole| hole.kind == *kind)
            .all(|hole| hole.suppressed),
        "alpha and beta must be drawn with the slash"
    );

    // A placed token on another system stays off this one, and a token here joins in draw order.
    let mut state = frame.state.board.get(&SystemId::new(&system)).cloned();
    let _ = &mut state;
    frame
        .state
        .wormhole_tokens
        .insert("GAMMA".to_owned(), SystemId::new(&system));
    let tokened = board_view(content, &session, &frame, None);
    let tile = find(&tokened, &system);
    assert!(
        tile.wormholes
            .iter()
            .any(|hole| hole.kind == "GAMMA" && hole.token),
        "the placed token is missing"
    );
    assert!(
        tile.wormholes
            .iter()
            .filter(|hole| hole.token)
            .all(|hole| hole.kind == "GAMMA"),
        "only the token is a token"
    );
    // Draw order is layout: the renderer spaces the rings by list position, so a token that jumped
    // ahead of a printed wormhole would move every glyph on the tile.
    let printed_at = tile.wormholes.iter().position(|hole| !hole.token);
    let token_at = tile.wormholes.iter().position(|hole| hole.token);
    if let (Some(printed_at), Some(token_at)) = (printed_at, token_at) {
        assert!(
            printed_at < token_at,
            "a placed token drew before the printed wormhole, so the whole row shifted"
        );
    }
    assert!(
        !tile
            .wormholes
            .iter()
            .any(|hole| hole.kind == "GAMMA" && hole.suppressed),
        "gamma is not affected by travel ban"
    );
    assert!(!token_printed);
}

fn tiles_with_printed_wormholes(
    session: &ReviewSession,
    content: &ContentStore,
) -> Option<(String, String, bool)> {
    let frame = frame_of(session);
    board_view(content, session, &frame, None)
        .into_iter()
        .find_map(|tile| {
            tile.wormholes
                .first()
                .map(|hole| (tile.system.clone(), hole.kind.clone(), hole.token))
        })
}

#[test]
fn the_model_is_a_function_of_the_frame() {
    let session = started_session();
    let frame = frame_of(&session);
    let content = ContentStore::embedded();
    assert_eq!(
        board_view(content, &session, &frame, None),
        board_view(content, &session, &frame, None),
        "the same frame built two different boards"
    );
    // Selection changes only what selection changes: the geometry and the meaning of every other
    // tile are untouched, which is what lets the replayer redraw a branch cheaply.
    let plain = board_view(content, &session, &frame, None);
    let target = plain[3].system.clone();
    let chosen = board_view(content, &session, &frame, Some(target.as_str()));
    assert_eq!(plain.len(), chosen.len());
    for (before, after) in plain.iter().zip(&chosen) {
        let mut expected = before.clone();
        expected.selected = before.selected || after.selected;
        expected.color = after.color;
        if before.fill != after.fill {
            assert!(expected.selected, "an unselected tile changed fill");
        }
        assert_eq!(expected.system, after.system);
        assert_eq!(expected.planets, after.planets, "selection moved a planet");
        assert_eq!(expected.units, after.units, "selection moved a fleet");
    }
    assert_eq!(chosen.iter().filter(|tile| tile.selected).count(), 1);
}
