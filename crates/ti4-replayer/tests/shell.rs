//! The whole window's journey, without the window.
//!
//! `crates/ti4-replayer/src/gui.rs` cannot be clicked in a test: it needs a display, and the operator
//! who clicks it is not a CI machine. But every decision a click takes is a call into the library, and
//! the sequence is fixed: open a session, adopt it as a project, look at a frame, fork, rebuild, take a
//! seat, answer, run on, save, reopen. This test walks that path against the real engine on the
//! committed inputs - including the file round trip - so that when the window misbehaves the answer is
//! either "the pixels" or "here is the line", and not "somewhere in the design".
//!
//! What it does not do is prove the thing looks right. That is the operator's smoke pass, and this file
//! says so rather than implying otherwise.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use ti4_model::id::PlayerId;
use ti4_replayer::app::{RebuildOutcome, ReplayApp};
use ti4_replayer::fingerprint::FrameFingerprint;
use ti4_replayer::gui::{seat_control, seat_settings, seats_from};
use ti4_replayer::live::{AdvanceGoal, Feed, LiveBranch, LiveEvent, LiveState, ReplayRequest};
use ti4_replayer::project::ReplayInputs;
use ti4_replayer::rebuild::{RebuildBounds, RebuildTarget};
use ti4_replayer::store::{self, Store};
use ti4_replayer::{
    BranchId, Gate, ManualSubmission, ReplayerProject, SeatControl, SeatMode, SubmitOutcome,
    load_project, save_project,
};
use ti4_review::{ReviewFrame, SimulationConfig, load_session, save_session};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "ti4-r02-shell-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary directory");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives under the workspace's crates directory")
        .to_path_buf()
}

fn config() -> SimulationConfig {
    let root = workspace_root();
    SimulationConfig {
        checkpoint: root.join(CHECKPOINT),
        map_pool: root.join(MAP_POOL),
        seed: 4_242,
        rotation: 1,
        table: ti4_review::ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    }
}

/// Record a short real game and return it as a session file would be: frames plus settled answers.
fn record(
    steps: usize,
) -> (
    SimulationConfig,
    Vec<ReviewFrame>,
    Vec<ti4_replayer::ReplayRecord>,
) {
    let config = config();
    let gate = Arc::new(Gate::live(SeatControl::all_auto()));
    gate.attach_feed();
    gate.enable_recording();
    let branch = LiveBranch::with_gate(config.clone(), Arc::clone(&gate)).expect("spawn");
    let until = Instant::now() + Duration::from_secs(120);
    while branch.gate().state() != LiveState::Ready {
        assert_ne!(branch.gate().state(), LiveState::Failed, "start failed");
        assert!(Instant::now() < until, "the branch never became ready");
        std::thread::sleep(Duration::from_millis(5));
    }
    branch
        .gate()
        .run(AdvanceGoal::Steps(steps))
        .expect("advance accepted");
    let until = Instant::now() + Duration::from_secs(180);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the branch stalled");
    }
    let answers = branch.gate().records();
    let session = branch.into_session().expect("the session comes back");
    (config, session.frames, answers)
}

/// Open a short real recording exactly as the window does: write it as R01 writes it, adopt it as a
/// project, verify its inputs, and load its frames into the store.
///
/// The temporary directory is deliberately leaked: the session file stays open-able by whatever the
/// caller does next, and the machine's temp area is where it belongs.
fn opened() -> (ReplayApp<Arc<Gate>>, Store, SimulationConfig) {
    let temp = TempDir::new("opened");
    let (config, frames, _settled) = record(6);
    let session_path = temp.path.join("recorded.ti4review.json");
    let mut session = ti4_review::LiveReview::start(&config)
        .expect("a starting table")
        .session;
    session.frames = frames;
    save_session(&session_path, &session).expect("write the session file");

    let loaded = load_session(&session_path).expect("reload what was written");
    let inputs = ReplayInputs::from_manifest(&loaded.manifest);
    assert_eq!(inputs.seed, config.seed, "the seed is the recording's own");
    let root = workspace_root();
    let project = ReplayerProject::import_with(&session_path, &loaded, &inputs, &root, None, None)
        .expect("the session becomes a project");
    let verification = project
        .verify_inputs(&root)
        .expect("the inputs are where the recording said they were");
    assert!(verification.matches, "a fresh import hashes clean");
    let mut store = Store::new(3);
    store.import(BranchId::SOURCE, loaded);
    assert!(
        store.drawable(BranchId::SOURCE),
        "an import is always drawable"
    );
    // Exactly what `gui.rs::open_session` does: the reducer is told about the frames too, or its
    // timeline has nothing to move.
    let mut app = ReplayApp::new(project, verification, &session_path);
    app.record_frames(&store.ticks(BranchId::SOURCE));
    std::mem::forget(temp);
    (app, store, config)
}

/// Press Play at `viewed` and wire the result back the way `gui.rs` does: build the request from the
/// stored frames, rebuild on a branch thread, take the feed, finish the rebuild, attach the gate.
fn play_at(
    app: &mut ReplayApp<Arc<Gate>>,
    store: &mut Store,
    config: &SimulationConfig,
    viewed: usize,
) -> (Arc<Gate>, LiveBranch, usize) {
    app.select_frame(viewed);
    let plan = app
        .plan_play(seat_settings(&app.seats()))
        .expect("Play is allowed on a verified recording");
    assert_eq!(plan.frame, u64::try_from(viewed).expect("fits"));
    let fingerprints: Vec<_> = store
        .frames(plan.parent)
        .iter()
        .take(viewed + 1)
        .map(FrameFingerprint::of)
        .collect();
    let gate = Arc::new(Gate::replaying_interactive(
        seat_control(&app.seats()),
        plan.script,
    ));
    gate.attach_feed();
    let branch = LiveBranch::replay(
        Arc::clone(&gate),
        ReplayRequest {
            config: config.clone(),
            fingerprints,
            target: RebuildTarget::Frame(u64::try_from(viewed).expect("fits")),
            bounds: RebuildBounds::default(),
            record: true,
        },
    )
    .expect("the fork branch spawns");
    let mut reported = None;
    let until = Instant::now() + Duration::from_secs(240);
    while reported.is_none() {
        assert!(Instant::now() < until, "the rebuild never reported");
        for event in branch.drain_events() {
            if let LiveEvent::Rebuilt {
                replayed, frames, ..
            } = event
            {
                reported = Some((replayed, frames));
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let (replayed, rebuilt) = reported.expect("a rebuild report");
    let mut drained = Feed::default();
    for _ in 0..50 {
        let batch = gate.take_feed();
        let got = !batch.frames.is_empty() || batch.header.is_some();
        drained.frames.extend(batch.frames);
        if batch.header.is_some() {
            drained.header = batch.header;
        }
        if !got {
            break;
        }
    }
    store.apply(plan.child, drained);
    let ticks: Vec<_> = store.frames(plan.child).iter().map(store::tick).collect();
    app.finish_rebuild(
        RebuildOutcome {
            frames: ticks,
            replayed,
        },
        plan.child,
    );
    assert_eq!(
        app.current(),
        plan.child,
        "a reproduced fork becomes what is being looked at"
    );
    app.attach(Arc::clone(&gate), app.frames(plan.child).to_vec());
    (gate, branch, rebuilt)
}

#[test]
fn a_recording_opens_with_the_game_and_navigation_is_not_branching() {
    let (mut app, store, _config) = opened();
    let seats = seats_from(store.frames(BranchId::SOURCE), &app.seats());
    assert!(seats.len() >= 2, "the example table has more than one seat");
    let first = seats[0].clone();
    assert_eq!(app.seats().mode(&first), SeatMode::Auto);
    assert_eq!(
        app.toggle_seat(&first),
        SeatMode::Manual,
        "the chip the window draws is the app's answer, not a widget's memory"
    );

    let branches = app.tree().len();
    app.select_frame(2);
    app.previous_frame();
    app.next_frame();
    app.select_frame(4);
    assert_eq!(app.tree().len(), branches, "looking is not branching");
    assert_eq!(app.project().branches.len(), branches);
    assert_eq!(
        app.viewed(BranchId::SOURCE),
        Some(4),
        "the timeline position the slider shows is the app's"
    );
}

#[test]
fn play_forks_proves_the_prefix_and_leaves_the_parent_whole() {
    let (mut app, mut store, config) = opened();
    let parent_frames = store.len(BranchId::SOURCE);
    let (_gate, _branch, rebuilt) = play_at(&mut app, &mut store, &config, 4);
    assert_eq!(
        store.len(app.current()),
        rebuilt,
        "the window holds exactly the frames the rebuild proved"
    );
    assert!(
        app.tree()
            .iter()
            .any(|node| node.id == app.current() && node.playable),
        "only a reproduced fork becomes playable"
    );
    let tree = app.tree();
    let source = tree
        .iter()
        .find(|node| node.id == BranchId::SOURCE)
        .expect("the recording");
    assert_eq!(
        source.frames, parent_frames,
        "the branch that was forked kept every frame it had"
    );
}

#[test]
fn a_hand_answered_decision_survives_the_save() {
    let (mut app, mut store, config) = opened();
    // Take a seat before the fork, so the fork inherits it - the only way a replayer branch can be
    // asked anything at all.
    let seats = seats_from(store.frames(BranchId::SOURCE), &app.seats());
    let first = seats[0].clone();
    app.set_seat_mode(&first, SeatMode::Manual);
    let (gate, branch, _rebuilt) = play_at(&mut app, &mut store, &config, 4);
    let current = app.current();
    assert_eq!(
        branch.gate().snapshot().seats.mode(&first),
        SeatMode::Manual,
        "the branch thread was told which seat the operator took"
    );

    app.advance(AdvanceGoal::Steps(40))
        .expect("a rebuilt branch accepts a command");
    let parked = branch
        .wait_until_parked(Duration::from_secs(180))
        .expect("the seat set to Manual is asked within forty steps");
    assert_eq!(
        parked.actor, first,
        "the seat that was set to Manual is the one asked"
    );
    assert!(
        app.pending().is_some(),
        "the choice panel is open while the branch is parked"
    );
    let option = parked.options[0].id.clone();
    let outcome = app.submit(&ManualSubmission {
        offer: parked.offer,
        fingerprint: parked.fingerprint.clone(),
        option_id: option.clone(),
    });
    assert!(
        matches!(outcome, SubmitOutcome::Accepted { .. }),
        "the answer was on offer"
    );
    assert!(
        app.pending().is_none(),
        "the panel closes when the answer is accepted"
    );
    assert_eq!(
        app.submit(&ManualSubmission {
            offer: parked.offer,
            fingerprint: parked.fingerprint,
            option_id: option.clone(),
        }),
        SubmitOutcome::Duplicate,
        "a second click is refused by the app, which is what disables the widget"
    );

    stop_and_drain(&gate, &mut store, current);
    assert!(
        store.len(current) > 5,
        "the branch played past the frame it forked at"
    );

    let held = u64::try_from(store.len(current)).expect("a frame count that fits");
    store::fold_answers(app.project_mut(), current, &gate.records(), held)
        .expect("fold the branch's own answers into the project");
    assert_eq!(
        gate.dropped_records(),
        0,
        "every settled decision was written down: {}",
        gate.last_record_failure().unwrap_or_default()
    );
    let answers = app
        .project()
        .branch(current)
        .expect("the fork")
        .answers
        .clone();
    assert!(
        answers
            .iter()
            .any(|record| record.chosen == option && record.actor == first),
        "the hand-answered decision is stored on the branch that answered it: {answers:?}"
    );
    assert!(
        answers.iter().all(|record| record.frame >= 4),
        "a branch stores only from its fork onwards; the rest belongs to its parent"
    );

    let root = workspace_root();
    let saved = TempDir::new("saved");
    let project_path = saved.path.join("evening.r02.json");
    save_project(&project_path, app.project()).expect("save the project");
    let reopened = load_project(&project_path).expect("read it back");
    assert_eq!(reopened.branches.len(), 2, "the recording and the fork");
    assert!(
        reopened
            .branch(current)
            .expect("the fork")
            .playable(&reopened.verify_inputs(&root).expect("still verifiable")),
        "a reproduced branch stays playable across a save and a reopen"
    );
    assert!(
        !reopened
            .replay_script(current)
            .expect("a script for the fork")
            .is_empty(),
        "the fork's prefix is reproducible from the file alone"
    );
}

/// Let the answer wake the engine, stop the branch, and take the frames it played meanwhile - the
/// sequence the window performs when the operator presses Stop.
fn stop_and_drain(gate: &Arc<Gate>, store: &mut Store, branch: BranchId) {
    let until = Instant::now() + Duration::from_secs(120);
    while gate.pending().is_some() {
        assert!(Instant::now() < until, "the answer never woke the engine");
        std::thread::sleep(Duration::from_millis(5));
    }
    gate.stop();
    let until = Instant::now() + Duration::from_secs(60);
    while matches!(
        gate.state(),
        LiveState::Running | LiveState::WaitingForHuman
    ) {
        assert!(Instant::now() < until, "the branch would not stop");
        std::thread::sleep(Duration::from_millis(5));
    }
    store.apply(branch, gate.take_feed());
}

/// The seat helpers the window depends on, tested where the window cannot be.
#[test]
fn the_chips_come_from_the_game_and_the_modes_survive_the_trip() {
    let seat = PlayerId::new("p2");
    let mut control = SeatControl::all_auto();
    control.set_mode(&seat, SeatMode::Manual);
    let settings = seat_settings(&control);
    assert_eq!(settings.len(), 1, "only what differs from Auto is stored");
    let rebuilt = seat_control(&SeatControl::from_changes([("p2", SeatMode::Manual)]));
    assert_eq!(rebuilt.mode(&seat), SeatMode::Manual);
    // With no frame to ask, the chips are still the whole table - not just the seat somebody has
    // already touched, which is what left the operator with a single chip to click.
    let chips = seats_from(&[], &control);
    assert_eq!(
        chips.first(),
        Some(&seat),
        "the seat already taken is the first chip, not buried at the end"
    );
    for index in 0..6 {
        let name = format!("seat{index}");
        assert!(
            chips.iter().any(|chip| chip.as_str() == name),
            "{name} has to be offered before the table has produced a frame, or it cannot be taken"
        );
    }
    let mut sorted = chips.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        chips.len(),
        "no seat appears twice on the bar"
    );
}
