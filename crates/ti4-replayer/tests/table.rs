//! A table started in the window, played, and forked from - against the real engine.
//!
//! `shell.rs` walks the journey that starts from an R01 recording. This one starts from the other
//! door, the one the window opens on: "Load starting table". That path has no recording behind it,
//! and every one of the three things it got wrong was invisible from the recording side.
//!
//! - The project a live table creates starts at zero frames, and `fork` refuses a frame past the end
//!   of its parent, so `Play from this frame` was refused for every frame of a game in progress.
//! - A live branch's answers only reached the project when the project was saved, so a fork planned
//!   before a save replayed a prefix of nothing - which reproduced by luck while every seat was on
//!   Auto, and parked the rebuild forever as soon as a person had answered anything.
//! - The prefix was cut one frame short of the fork, because a decision is stamped with the number
//!   of frames that existed when it was asked: the answers stamped `F` are the ones the step that
//!   *produces* frame `F` consumes.
//!
//! Each test below is one of those, driven through the same calls `gui.rs` makes, in the same order.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_model::id::PlayerId;
use ti4_replayer::app::ReplayApp;
use ti4_replayer::fingerprint::FrameFingerprint;
use ti4_replayer::gui::{seat_control, seat_settings, seats_from};
use ti4_replayer::live::{AdvanceGoal, LiveBranch, LiveEvent, LiveState, ReplayRequest};
use ti4_replayer::project::ReplayInputs;
use ti4_replayer::rebuild::{RebuildBounds, RebuildTarget};
use ti4_replayer::store::{self, Store};
use ti4_replayer::{BranchId, Gate, ManualSubmission, ReplayerProject, SeatControl, SeatMode};
use ti4_review::{ReviewFrame, ReviewSession, SimulationConfig};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";
/// A parked branch waits for this test, not for the CPU, so this is generous but finite.
const WAIT: Duration = Duration::from_secs(240);

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<name> sits directly under the workspace root")
        .to_path_buf()
}

fn config() -> SimulationConfig {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 4_242,
        rotation: 1,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    };
    for path in [&config.checkpoint, &config.map_pool] {
        assert!(
            path.is_file(),
            "table input {} is missing from the checkout",
            path.display()
        );
    }
    config
}

/// The window's `table_header`: wait for the branch to describe its own seating, keeping the frames
/// it publishes meanwhile.
fn header(branch: &LiveBranch) -> (ReviewSession, Vec<ReviewFrame>) {
    let mut frames = Vec::new();
    let until = Instant::now() + WAIT;
    loop {
        let feed = branch.gate().take_feed();
        frames.extend(feed.frames);
        if let Some(header) = feed.header {
            return (header, frames);
        }
        assert_ne!(branch.gate().state(), LiveState::Failed, "the table failed");
        assert!(Instant::now() < until, "the table never described itself");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A table open in the window: the reducer, the running branch, and the frames it has sent.
struct Table {
    app: ReplayApp<Arc<Gate>>,
    branch: LiveBranch,
    store: Store,
}

/// `gui.rs::start_table`, without the window.
fn start_table() -> Table {
    let config = config();
    let inputs = ReplayInputs::of(&config);
    let base = workspace_root();
    let branch = LiveBranch::start(config, SeatControl::all_auto()).expect("the table starts");
    branch.gate().attach_feed();
    branch.gate().enable_recording();
    let (mut shell, frames) = header(&branch);
    shell.frames = frames;
    let project = ReplayerProject::live_table(&inputs, &base, &shell).expect("a live project");
    let verification = project.verify_inputs(&base).expect("the inputs hash clean");
    let mut store = Store::new(3);
    store.import(BranchId::SOURCE, shell);
    let mut app = ReplayApp::new(project, verification, Path::new("this table"));
    app.record_frames(&store.ticks(BranchId::SOURCE));
    let ticks = app.frames(BranchId::SOURCE).to_vec();
    app.attach(Arc::clone(branch.gate()), ticks);
    Table { app, branch, store }
}

/// `gui.rs::poll`, once: drain what the branch has sent and tell the reducer about it.
fn poll(table: &mut Table) {
    let gate = Arc::clone(table.branch.gate());
    let _ = table.branch.drain_events();
    let feed = gate.take_feed();
    if feed.frames.is_empty() && feed.header.is_none() {
        return;
    }
    let target = table.app.current();
    let appended = table.store.apply(target, feed);
    if appended == 0 {
        return;
    }
    let ticks: Vec<_> = table
        .store
        .frames(target)
        .iter()
        .rev()
        .take(appended)
        .rev()
        .map(store::tick)
        .collect();
    table.app.record_frames(&ticks);
}

fn moving(table: &Table) -> bool {
    matches!(
        table.branch.gate().state(),
        LiveState::Running | LiveState::WaitingForHuman
    )
}

fn run_until_idle(table: &mut Table, goal: AdvanceGoal) {
    table.app.advance(goal).expect("the branch takes the goal");
    let until = Instant::now() + WAIT;
    while moving(table) {
        poll(table);
        assert!(Instant::now() < until, "the branch stalled");
        std::thread::sleep(Duration::from_millis(5));
    }
    poll(table);
}

/// Wait for a seat to be asked something, polling the way the window does.
fn wait_for_a_question(table: &mut Table) -> ti4_replayer::PendingManualChoice {
    let until = Instant::now() + WAIT;
    loop {
        poll(table);
        if let Some(pending) = table.app.pending() {
            return pending;
        }
        assert!(moving(table), "the branch stopped without asking anybody");
        assert!(Instant::now() < until, "nobody was asked anything");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Press Play at the frame being viewed and drive the rebuild to its verdict, as `gui.rs` does.
fn play_from_viewed(table: &mut Table) -> Result<(usize, usize), String> {
    let parent = table.app.current();
    let records = table.branch.gate().records();
    let frames = u64::try_from(table.store.len(parent)).expect("fits");
    store::fold_answers(table.app.project_mut(), parent, &records, frames)
        .expect("the branch's own answers belong to the project before it is forked");
    let plan = table
        .app
        .plan_play(seat_settings(&table.app.seats()))
        .map_err(|block| block.tooltip())?;
    let needed = usize::try_from(plan.frame.saturating_add(1)).expect("fits");
    let fingerprints: Vec<FrameFingerprint> = table
        .store
        .frames(parent)
        .iter()
        .take(needed)
        .map(FrameFingerprint::of)
        .collect();
    let config = table
        .app
        .project()
        .inputs
        .simulation_config(&workspace_root())
        .expect("the project names a profile table this build has");
    let gate = Arc::new(Gate::replaying_interactive(
        seat_control(&table.app.seats()),
        plan.script,
    ));
    gate.attach_feed();
    let fork = LiveBranch::replay(
        Arc::clone(&gate),
        ReplayRequest {
            config,
            fingerprints,
            target: RebuildTarget::Frame(plan.frame),
            bounds: RebuildBounds::default(),
            record: true,
        },
    )
    .expect("the fork's thread spawns");
    let until = Instant::now() + WAIT;
    loop {
        for event in fork.drain_events() {
            match event {
                LiveEvent::Rebuilt {
                    replayed, frames, ..
                } => return Ok((replayed, frames)),
                LiveEvent::Failed(why) => return Err(why),
                _ => {}
            }
        }
        // A rebuild that parks is a rebuild whose prefix did not cover the game: it is asking a
        // person to answer a decision that was already answered once. Report it as the failure it
        // is rather than waiting out the timeout.
        if let Some(pending) = gate.pending() {
            return Err(format!(
                "the rebuild parked on {} at frame {} ask {}: {}",
                pending.actor, pending.frame, pending.ask, pending.prompt
            ));
        }
        let _ = gate.take_feed();
        assert!(Instant::now() < until, "the rebuild never reported");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_board_follows_a_table_that_is_playing() {
    let mut table = start_table();
    run_until_idle(&mut table, AdvanceGoal::Steps(8));
    let branch = table.app.current();
    let frames = table.app.frames(branch).len();
    assert!(
        frames > 1,
        "eight steps produce more than the opening frame"
    );
    assert_eq!(
        table.app.viewed(branch),
        Some(frames - 1),
        "the frame on screen is the frame the table has reached"
    );
    assert!(table.app.at_tip());
    // The window's own path from store to sheet, painted. This is the call that ended the first live
    // table: the store hands out a session header with `frames` emptied - one recording is hundreds of
    // megabytes and a branch tree must not multiply it - and the shared sheets read their history out of
    // `session.frames`, which is empty here. Every data test in the workspace passed while the window
    // panicked, because only painting reaches that line.
    let store = &table.store;
    let header = store.session(branch).expect("a live table has a header");
    let frames = store.frames(branch);
    let sheets = ti4_review::panels::Sheets { header, frames };
    assert!(
        header.frames.is_empty(),
        "the store is supposed to keep the frames out of the header, or this test proves nothing"
    );
    for frame in frames.iter().rev().take(3) {
        let output = eframe::egui::Context::default().run_ui(
            eframe::egui::RawInput {
                screen_rect: Some(eframe::egui::Rect::from_min_size(
                    eframe::egui::Pos2::ZERO,
                    eframe::egui::vec2(1200.0, 900.0),
                )),
                ..eframe::egui::RawInput::default()
            },
            |ui| {
                ti4_review::panels::players_sheet(ui, &sheets, frame);
                ti4_review::panels::decision_sheet(
                    ui,
                    &sheets,
                    frame,
                    None,
                    ti4_review::panels::SystemNaming::TileAndPlanets,
                );
            },
        );
        assert!(
            !output.shapes.is_empty(),
            "frame {} painted nothing at all",
            frame.index
        );
    }
    // And a reader who scrolls back is left alone by the next batch.
    table.app.select_frame(1);
    run_until_idle(&mut table, AdvanceGoal::Steps(4));
    assert_eq!(
        table.app.viewed(branch),
        Some(1),
        "reading frame 1 is not interrupted by the game moving on"
    );
}

#[test]
fn a_seat_flipped_while_the_table_runs_is_asked() {
    let mut table = start_table();
    run_until_idle(&mut table, AdvanceGoal::Steps(4));
    let seats = seats_from(table.store.frames(table.app.current()), &table.app.seats());
    assert_eq!(seats.len(), 6, "the example table seats six");
    // Every seat but the first, so the pause cannot be the one the setup form would have made.
    for seat in seats.iter().skip(1) {
        assert_eq!(
            table.app.set_seat_mode(seat, SeatMode::Manual),
            SeatMode::Manual
        );
    }
    table
        .app
        .advance(AdvanceGoal::EndOfGame)
        .expect("the branch runs on");
    let pending = wait_for_a_question(&mut table);
    assert_ne!(
        pending.actor, seats[0],
        "the seat that was flipped mid-game is the one being asked"
    );
    assert_eq!(table.app.seats().mode(&pending.actor), SeatMode::Manual);
    assert_eq!(
        table.branch.gate().fallbacks().unanswered,
        0,
        "no manual ask was answered behind the operator's back"
    );
}

#[test]
fn a_seat_flipped_while_another_is_parked_leaves_that_panel_alone() {
    let mut table = start_table();
    let seats: Vec<PlayerId> = (0..6).map(|n| PlayerId::new(format!("seat{n}"))).collect();
    table.app.set_seat_mode(&seats[0], SeatMode::Manual);
    table
        .app
        .advance(AdvanceGoal::EndOfGame)
        .expect("the branch runs");
    let parked = wait_for_a_question(&mut table);
    assert_eq!(parked.actor, seats[0]);
    table.app.set_seat_mode(&seats[3], SeatMode::Manual);
    assert_eq!(table.app.seats().mode(&seats[3]), SeatMode::Manual);
    let still = table
        .app
        .pending()
        .expect("the open question is still open");
    assert_eq!(
        still.fingerprint, parked.fingerprint,
        "changing another seat never disturbs the choice on screen"
    );
}

#[test]
fn a_table_started_here_can_be_forked_from() {
    let mut table = start_table();
    run_until_idle(&mut table, AdvanceGoal::Steps(10));
    table.branch.gate().stop();
    poll(&mut table);
    let held = table.store.len(table.app.current());
    assert!(held > 2, "ten steps produce frames to fork from");
    table.app.select_frame(held / 2);
    let (replayed, frames) = play_from_viewed(&mut table).expect("the fork rebuilds");
    assert_eq!(
        frames,
        held / 2 + 1,
        "the fork stands exactly at the frame it was made from"
    );
    assert!(
        replayed > 0,
        "the prefix was forced through the policy, not replayed from an empty script"
    );
}

#[test]
fn a_fork_replays_the_answers_a_person_gave() {
    let mut table = start_table();
    let seat0 = PlayerId::new("seat0");
    table.app.set_seat_mode(&seat0, SeatMode::Manual);
    table
        .app
        .advance(AdvanceGoal::EndOfGame)
        .expect("the branch runs");
    // Three answers by hand, always the last option offered, so at least one of them is unlikely to
    // be what the policy would have picked: a prefix that is not replayed will diverge on them.
    for _ in 0..3 {
        let pending = wait_for_a_question(&mut table);
        let option_id = pending
            .options
            .last()
            .expect("a choice with options")
            .id
            .clone();
        let outcome = table.app.submit(&ManualSubmission {
            offer: pending.offer,
            fingerprint: pending.fingerprint.clone(),
            option_id,
        });
        assert!(
            matches!(outcome, ti4_replayer::SubmitOutcome::Accepted { .. }),
            "the engine took the answer: {outcome:?}"
        );
    }
    table.branch.gate().stop();
    let until = Instant::now() + WAIT;
    while moving(&table) {
        poll(&mut table);
        assert!(Instant::now() < until, "the branch would not stop");
        std::thread::sleep(Duration::from_millis(5));
    }
    poll(&mut table);
    let held = table.store.len(table.app.current());
    table.app.select_frame(held - 1);
    let (replayed, frames) = play_from_viewed(&mut table)
        .expect("a branch a person played is a branch that can be forked");
    assert_eq!(frames, held);
    assert!(
        replayed >= 3,
        "every decision of the prefix was forced, the operator's three among them"
    );
}

/// UI-08: a table that was played and stopped can be closed and a second one started in its place.
/// The window used to refuse the second table because Stop ends the thread without detaching it;
/// `gui.rs::close_for_new_table` closes the app, and the next table must then run normally.
#[test]
fn a_second_table_starts_after_the_first_is_stopped_and_closed() {
    let mut first = start_table();
    run_until_idle(&mut first, AdvanceGoal::Steps(2));
    first.branch.gate().stop();
    assert!(
        first.app.handle().is_some(),
        "Stop alone leaves the branch attached - which is what the old guard tripped on"
    );
    let _ = first.app.close();
    assert!(first.app.handle().is_none(), "closing detaches the branch");
    drop(first);

    let mut second = start_table();
    run_until_idle(&mut second, AdvanceGoal::Steps(2));
    assert!(
        !second.store.frames(second.app.current()).is_empty(),
        "the second table plays and sends frames"
    );
    assert_ne!(second.branch.gate().state(), LiveState::Failed);
}
