//! The two shared sheets, actually painted.
//!
//! Every other test in this crate pins the *data* a panel draws from, because the drawing itself
//! needs a context. It does not need a window, though: egui will run a whole frame headlessly, which
//! is enough to catch what the data tests cannot - a panic in a widget, a nested layout that never
//! closes, and two widgets claiming the same id (which is how a collapsing header silently opens its
//! neighbour). R02-007's decision panel grew per-option headers, so that last one is not theoretical.

use std::path::PathBuf;

use eframe::egui;
use ti4_review::panels::{self, SystemNaming};
use ti4_review::{LiveReview, ProfileTable, ReviewFrame, ReviewSession, SimulationConfig};

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir
            .join("examples/reviewer/full_np8_12_holdout.json")
            .is_file()
        {
            return dir;
        }
        assert!(dir.pop(), "no workspace root carries the reviewer inputs");
    }
}

/// A real board, a real frame, and a real set of decisions: the example table, advanced far enough
/// that the decision sheet has options, features and events to draw.
fn played() -> (ReviewSession, ReviewFrame) {
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
    let mut review = LiveReview::start(&config).expect("the committed inputs start a review");
    review.advance(ti4_review::AdvanceUnit::Step, 40);
    let frame = review
        .session
        .frames
        .iter()
        .rev()
        .find(|frame| !frame.decisions.is_empty())
        .cloned()
        .expect("forty steps settle at least one decision");
    (review.session, frame)
}

/// Run one headless frame with `draw` in it, and report anything egui drew a complaint about.
///
/// egui does not panic on a duplicate widget id; it paints a fire label over the offender and
/// carries on, which in a window looks like a collapsing header opening the wrong section. Reading
/// the frame back out is the only way a test sees it.
fn paint(draw: impl FnMut(&mut egui::Ui)) -> Vec<String> {
    let mut draw = draw;
    let context = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 1000.0),
        )),
        ..egui::RawInput::default()
    };
    let output = context.run_ui(input, |ui| {
        egui::ScrollArea::vertical().show(ui, &mut draw);
    });
    let mut complaints = Vec::new();
    for clipped in &output.shapes {
        collect_complaints(&clipped.shape, &mut complaints);
    }
    complaints
}

/// The marker egui paints beside a widget whose id was already used this frame.
const ID_CLASH: char = '\u{1f525}';

fn collect_complaints(shape: &egui::epaint::Shape, into: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(text) => {
            let painted = text.galley.text();
            if painted.contains(ID_CLASH) {
                into.push(painted.to_owned());
            }
        }
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_complaints(shape, into);
            }
        }
        _ => {}
    }
}

#[test]
fn the_players_sheet_paints() {
    let (session, frame) = played();
    let problems = paint(|ui| {
        panels::players_sheet(ui, &panels::Sheets::whole(&session), &frame);
    });
    assert!(problems.is_empty(), "{problems:?}");
}

/// The shape the replayer actually hands these sheets: a session header with `frames` emptied, and the
/// branch's frames in a list beside it. Its store does that on purpose - an R01 recording is hundreds of
/// megabytes and a branch tree must not multiply one - and a sheet that reached into `session.frames`
/// anyway opened a live table with "range end index 0 out of range for slice of length 0" and then with
/// "the len is 0 but the index is 39". Nothing but painting this catches it: every data test in the
/// crate passes with a session that carries its frames.
#[test]
fn the_sheets_paint_from_a_shell_and_a_separate_frame_list() {
    let (session, frame) = played();
    let frames = session.frames.clone();
    let mut shell = session;
    shell.frames = Vec::new();
    let tile = shell.board.first().map(|tile| tile.system.clone());
    let sheets = panels::Sheets {
        header: &shell,
        frames: &frames,
    };
    // One paint per sheet, as in the window: they live in side panels of their own, and two
    // scroll areas sharing an id space would be a fault of the test rather than of the sheet.
    let players = paint(|ui| panels::players_sheet(ui, &sheets, &frame));
    assert!(players.is_empty(), "players sheet: {players:?}");
    let decisions = paint(|ui| {
        panels::decision_sheet(
            ui,
            &sheets,
            &frame,
            tile.as_deref(),
            SystemNaming::TileAndPlanets,
        );
    });
    assert!(decisions.is_empty(), "decision sheet: {decisions:?}");
}

/// And with no history at all - the first frame of a fork, its list still arriving - the two rows that
/// need a previous frame are dropped and everything else still paints. A sheet may lose a row; it may
/// not take the window with it.
#[test]
fn the_sheets_paint_when_the_history_is_empty() {
    let (session, frame) = played();
    let mut shell = session;
    shell.frames = Vec::new();
    let tile = shell.board.first().map(|tile| tile.system.clone());
    let sheets = panels::Sheets {
        header: &shell,
        frames: &[],
    };
    let players = paint(|ui| panels::players_sheet(ui, &sheets, &frame));
    assert!(players.is_empty(), "players sheet: {players:?}");
    let decisions = paint(|ui| {
        panels::decision_sheet(ui, &sheets, &frame, tile.as_deref(), SystemNaming::TileOnly);
    });
    assert!(decisions.is_empty(), "decision sheet: {decisions:?}");
}

#[test]
fn the_decision_sheet_paints_in_both_spellings() {
    let (session, frame) = played();
    for naming in [SystemNaming::TileOnly, SystemNaming::TileAndPlanets] {
        let tile = session.board.first().map(|tile| tile.system.clone());
        let problems = paint(|ui| {
            panels::decision_sheet(
                ui,
                &panels::Sheets::whole(&session),
                &frame,
                tile.as_deref(),
                naming,
            );
        });
        assert!(problems.is_empty(), "{naming:?}: {problems:?}");
    }
}

/// The check above is only worth having if it catches a clash. This is one.
#[test]
fn a_duplicate_id_is_seen_by_this_test() {
    let problems = paint(|ui| {
        for index in 0..2 {
            egui::CollapsingHeader::new(format!("header {index}"))
                .id_salt("the-same-salt-twice")
                .show(ui, |ui| {
                    ui.label("body");
                });
        }
    });
    assert!(
        !problems.is_empty(),
        "two widgets sharing an id must be reported, or the paint tests prove nothing"
    );
}
