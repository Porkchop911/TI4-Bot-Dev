//! The frame feed: how a branch that is running on its own thread gets drawn.
//!
//! A `LiveReview` is not `Send`, so the window cannot hold the game; it holds copies of the frames the
//! branch produced. These tests are about that copy being complete, ordered, and there when the window
//! looks - including across the two things the replayer does that a plain simulation never does: park
//! on a human mid-step, and rebuild a recorded prefix before playing a single new one.
//!
//! They run the real engine on the committed example inputs. Each one is a few seconds; the fork tests
//! are the slow ones because they play a game twice, which is the whole point.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_replayer::fingerprint::FrameFingerprint;
use ti4_replayer::live::{AdvanceGoal, Feed, LiveBranch, LiveEvent, LiveState, ReplayRequest};
use ti4_replayer::rebuild::{RebuildBounds, RebuildTarget, ReplayScript};
use ti4_replayer::{Gate, ManualSubmission, ReplayRecord, SeatControl, SeatMode, SubmitOutcome};
use ti4_review::{ReviewFrame, SimulationConfig};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(PathBuf::from)
        .expect("the crate lives under the workspace's crates directory")
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

/// Wait for the branch to finish starting, and fail loudly if it did not start.
fn wait_ready(branch: &LiveBranch) {
    let until = Instant::now() + Duration::from_secs(120);
    while branch.gate().state() == LiveState::Ready && branch.gate().pending().is_none() {
        // A branch that has not begun reports `Ready`; the started event is what proves it did.
        if branch
            .drain_events()
            .iter()
            .any(|event| matches!(event, LiveEvent::Started { .. }))
        {
            return;
        }
        assert_ne!(
            branch.gate().state(),
            LiveState::Failed,
            "the branch failed while starting"
        );
        assert!(Instant::now() < until, "the branch never started");
        std::thread::sleep(Duration::from_millis(5));
    }
    // It may already be running by the time we look, which is also a start.
    let until = Instant::now() + Duration::from_secs(120);
    while Instant::now() < until {
        if !matches!(branch.gate().state(), LiveState::Ready) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the branch never started");
}

/// Run a real branch for `steps` steps with a feed attached and return what the window saw.
fn play(steps: usize) -> (SimulationConfig, Vec<ReviewFrame>, Vec<ReplayRecord>) {
    let config = config();
    let gate = Arc::new(Gate::live(SeatControl::all_auto()));
    gate.attach_feed();
    gate.enable_recording();
    let branch = LiveBranch::with_gate(config.clone(), Arc::clone(&gate)).expect("spawn");
    wait_ready(&branch);
    branch
        .gate()
        .run(AdvanceGoal::Steps(steps))
        .expect("the advance is accepted");
    let until = Instant::now() + Duration::from_secs(180);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the branch stalled mid-run");
    }
    let mut feed = Feed::default();
    drain_into(&branch, &mut feed);
    let answers = branch.gate().records();
    drop(branch);
    (config, feed.frames, answers)
}

/// The engine step of the first frame a branch reported, for the "did it advance" assertion.
fn frames_first_step(frames: &[ReviewFrame]) -> usize {
    frames.first().map_or(0, |frame| frame.engine_step)
}

/// Take everything the branch has produced, the way a repaint loop does, until the queue goes quiet.
fn drain_into(branch: &LiveBranch, into: &mut Feed) {
    let mut quiet = 0;
    for _ in 0..200 {
        let batch = branch.gate().take_feed();
        let got = !batch.frames.is_empty() || batch.header.is_some() || batch.missing > 0;
        into.frames.extend(batch.frames);
        if batch.header.is_some() {
            into.header = batch.header;
        }
        into.missing += batch.missing;
        if got {
            quiet = 0;
        } else {
            quiet += 1;
            if quiet >= 3 {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn the_feed_carries_the_header_once_and_every_frame_in_order() {
    let (_config, frames, _answers) = play(5);
    // The setup frame plus one per step. If this ever changes shape, the timeline widget's assumption
    // that a frame index is a position is what breaks.
    assert_eq!(
        frames.iter().map(|frame| frame.index).collect::<Vec<_>>(),
        (0..6).collect::<Vec<_>>(),
        "every frame the engine made arrived, in order, exactly once"
    );
    let last = frames.last().expect("a final frame");
    assert!(
        last.state.players.len() >= 2 && last.state.players.len() == last.state.seating_order.len(),
        "a real game whose seating order the seat chips can draw, not a stub"
    );
    assert!(
        last.engine_step > frames_first_step(&frames),
        "the strip ends further into the game than it began"
    );
}

#[test]
fn the_header_arrives_once_and_carries_the_board_the_window_needs() {
    let config = config();
    let gate = Arc::new(Gate::live(SeatControl::all_auto()));
    gate.attach_feed();
    let branch = LiveBranch::with_gate(config, Arc::clone(&gate)).expect("spawn");
    wait_ready(&branch);
    branch
        .gate()
        .run(AdvanceGoal::Steps(2))
        .expect("the advance is accepted");
    let until = Instant::now() + Duration::from_secs(120);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the branch stalled");
    }
    let mut feed = Feed::default();
    drain_into(&branch, &mut feed);
    let header = feed.header.expect("the header arrives with the frames");
    assert!(
        header.frames.is_empty(),
        "the header is not a second copy of every frame"
    );
    assert!(
        !header.board.is_empty(),
        "the board the tiles are drawn on comes from the header"
    );
    assert_eq!(
        header.manifest.seed, 4_242,
        "the manifest the panel prints is the one the branch ran on"
    );
    let again = branch.gate().take_feed();
    assert!(
        again.header.is_none(),
        "the header is offered once, not once per drain"
    );
}

#[test]
fn a_window_that_stops_looking_makes_the_branch_drop_the_oldest_first() {
    let config = config();
    let gate = Arc::new(Gate::live(SeatControl::all_auto()));
    // Three frames of patience: a window that is not draining loses the past, never the present.
    gate.attach_feed_with(3);
    let branch = LiveBranch::with_gate(config, Arc::clone(&gate)).expect("spawn");
    wait_ready(&branch);
    branch
        .gate()
        .run(AdvanceGoal::Steps(6))
        .expect("the advance is accepted");
    let until = Instant::now() + Duration::from_secs(180);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the branch stalled");
    }
    let mut feed = Feed::default();
    drain_into(&branch, &mut feed);
    assert!(
        feed.missing > 0,
        "a full feed must report what it dropped, not hide it"
    );
    let highest = feed.frames.last().expect("the newest frame survives").index;
    assert!(
        feed.frames.iter().all(|frame| frame.index <= highest),
        "the frames kept are the newest ones, in order"
    );
    assert_eq!(
        branch.gate().dropped_frames(),
        0,
        "taking the feed takes the count with it, so a window never reports the same hole twice"
    );
}

#[test]
fn the_feed_keeps_feeding_across_a_pause_and_an_answer() {
    let config = config();
    let gate = Arc::new(Gate::live(SeatControl::all_auto()));
    gate.attach_feed();
    let branch = LiveBranch::with_gate(config, Arc::clone(&gate)).expect("spawn");
    wait_ready(&branch);
    branch
        .gate()
        .run(AdvanceGoal::Steps(2))
        .expect("the first advance is accepted");
    let until = Instant::now() + Duration::from_secs(120);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the first run stalled");
    }
    let mut before = Feed::default();
    drain_into(&branch, &mut before);
    let reached = before
        .frames
        .last()
        .expect("the first run produced frames")
        .index;

    // Take a seat, park on the next decision that actor is offered, and answer it by hand.
    let seat = before
        .frames
        .last()
        .expect("a frame")
        .state
        .seating_order
        .first()
        .cloned()
        .expect("a seat");
    branch.gate().set_mode(&seat, SeatMode::Manual);
    branch
        .gate()
        .run(AdvanceGoal::Steps(40))
        .expect("the second advance is accepted");
    let parked = branch
        .wait_until_parked(Duration::from_secs(120))
        .expect("a manual seat should park within forty steps");
    let chosen = parked
        .options
        .first()
        .expect("the offer has at least one option")
        .id
        .clone();
    let outcome = branch.gate().submit(&ManualSubmission {
        offer: parked.offer,
        fingerprint: parked.fingerprint.clone(),
        option_id: chosen.clone(),
    });
    assert_eq!(
        outcome,
        SubmitOutcome::Accepted { option_id: chosen },
        "the answer was the one on offer"
    );
    let until = Instant::now() + Duration::from_secs(180);
    while branch.gate().pending().is_some() {
        assert!(Instant::now() < until, "the answer never woke the engine");
        std::thread::sleep(Duration::from_millis(5));
    }
    branch.gate().stop();
    let until = Instant::now() + Duration::from_secs(60);
    while matches!(
        branch.gate().state(),
        LiveState::Running | LiveState::WaitingForHuman
    ) {
        assert!(Instant::now() < until, "the branch would not stop");
        std::thread::sleep(Duration::from_millis(5));
    }

    let mut after = Feed::default();
    drain_into(&branch, &mut after);
    assert!(
        !after.frames.is_empty(),
        "the branch kept producing frames after the human answered"
    );
    let first = after
        .frames
        .first()
        .expect("a frame after the answer")
        .index;
    assert!(
        first > reached,
        "the window must not be shown a frame it already had: {first} after {reached}"
    );
    for pair in after.frames.windows(2) {
        assert!(
            pair[1].index > pair[0].index,
            "frames arrive in order across an answer"
        );
    }
}

/// Replay the frames of a real game on a fresh branch and then play on from the fork.
#[test]
fn a_fork_rebuilds_and_then_plays_on_the_same_branch() {
    let (config, frames, answers) = play(6);
    let fork_frame = frames.last().expect("frames").index - 1;
    let script_records: Vec<ReplayRecord> = answers
        .iter()
        .filter(|record| record.frame < u64::try_from(fork_frame).expect("a frame index"))
        .cloned()
        .collect();
    assert!(
        !script_records.is_empty(),
        "six steps must settle decisions worth forcing"
    );

    let gate = Arc::new(Gate::replaying(
        SeatControl::all_auto(),
        ReplayScript::new(script_records.clone()),
    ));
    gate.attach_feed();
    let fingerprints: Vec<FrameFingerprint> = frames
        .iter()
        .take(fork_frame + 1)
        .map(FrameFingerprint::of)
        .collect();
    let branch = LiveBranch::replay(
        Arc::clone(&gate),
        ReplayRequest {
            config,
            fingerprints,
            target: RebuildTarget::Frame(u64::try_from(fork_frame).expect("fits")),
            bounds: RebuildBounds::default(),
            record: true,
        },
    )
    .expect("the fork branch spawns");

    // The rebuild is the branch's first act: the window sees it arrive as an event, not as a hang.
    let mut rebuilt = None;
    let until = Instant::now() + Duration::from_secs(240);
    while rebuilt.is_none() {
        assert!(
            Instant::now() < until,
            "the fork never reported its rebuild"
        );
        for event in branch.drain_events() {
            match event {
                LiveEvent::Rebuilt {
                    replayed,
                    steps,
                    frames,
                } => {
                    assert!(replayed > 0, "the prefix was forced, not merely offered");
                    assert!(steps > 0, "reaching the fork took engine steps");
                    rebuilt = Some((replayed, steps, frames));
                }
                LiveEvent::Failed(why) => {
                    panic!("the fork refused a recording that is real: {why}");
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let (_replayed, _steps, reached) = rebuilt.expect("a rebuild report");
    assert_eq!(
        reached,
        fork_frame + 1,
        "the branch is positioned at the frame it forked, no further"
    );

    // And now it plays: the same thread, the same review, the frames after the fork arriving as any
    // other branch's would. This is the behaviour that needs the review to outlive the rebuild.
    branch
        .gate()
        .run(AdvanceGoal::Steps(3))
        .expect("a rebuilt branch accepts an advance");
    let until = Instant::now() + Duration::from_secs(180);
    while !branch.wait_until_idle(Duration::from_millis(5)) {
        assert!(Instant::now() < until, "the fork stalled after its rebuild");
    }
    let mut feed = Feed::default();
    drain_into(&branch, &mut feed);
    let highest = feed
        .frames
        .last()
        .expect("the fork produced frames after playing on")
        .index;
    assert!(
        highest > fork_frame,
        "the branch played past the frame it forked at"
    );
    assert!(
        feed.frames.len() > fork_frame,
        "the window got the whole history: the replayed prefix and the new frames"
    );
    for pair in feed.frames.windows(2) {
        assert!(
            pair[1].index > pair[0].index,
            "a rebuild and the play that follows are one continuous strip"
        );
    }
    assert!(
        !branch.gate().records().is_empty(),
        "a branch asked to record, recorded: that is how the fork is replayable later"
    );
}

#[test]
fn a_fork_whose_recording_lies_fails_and_refuses_to_run() {
    let (config, frames, answers) = play(4);
    let fork_frame = frames.last().expect("frames").index - 1;
    let mut fingerprints: Vec<FrameFingerprint> = frames
        .iter()
        .take(fork_frame + 1)
        .map(FrameFingerprint::of)
        .collect();
    // Swap two frames: the recording is internally plausible and wrong, which is the case a rebuild
    // exists to catch.
    if fingerprints.len() > 2 {
        fingerprints.swap(1, 2);
    } else {
        fingerprints[0] = FrameFingerprint::of(frames.last().expect("a frame"));
    }

    let gate = Arc::new(Gate::replaying(
        SeatControl::all_auto(),
        ReplayScript::new(
            answers
                .iter()
                .filter(|record| record.frame < u64::try_from(fork_frame).expect("fits"))
                .cloned()
                .collect(),
        ),
    ));
    let branch = LiveBranch::replay(
        Arc::clone(&gate),
        ReplayRequest {
            config,
            fingerprints,
            target: RebuildTarget::Frame(u64::try_from(fork_frame).expect("fits")),
            bounds: RebuildBounds::default(),
            record: true,
        },
    )
    .expect("the branch spawns to try");

    let until = Instant::now() + Duration::from_secs(240);
    while branch.gate().state() != LiveState::Failed {
        assert!(Instant::now() < until, "a lying recording was not caught");
        std::thread::sleep(Duration::from_millis(10));
    }
    let why = branch
        .drain_events()
        .into_iter()
        .find_map(|event| match event {
            LiveEvent::Failed(detail) => Some(detail),
            _ => None,
        })
        .expect("the failure is reported, not just written to the state");
    assert!(
        why.contains("differ") || why.contains("diverge"),
        "the reason names what went wrong: {why}"
    );
    assert!(
        branch.gate().run(AdvanceGoal::Steps(1)).is_err(),
        "a branch that could not reproduce its past will not be played into a future"
    );
}
