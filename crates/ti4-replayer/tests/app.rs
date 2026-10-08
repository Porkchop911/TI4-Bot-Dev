//! The replayer's application state, tested with no window and no engine.
//!
//! Every interaction a reviewer has with a live branch - flipping a seat, clicking an option,
//! pausing, forking from a frame, closing the window while a rebuild runs - is decided in
//! [`ReplayApp`] and can therefore be tested against a scripted stand-in for the branch. The engine
//! itself is exercised in `live.rs` and `rebuild.rs`; what matters here is that the app never
//! invents an affordance, never forks by accident, and never goes silent about a button that does
//! nothing.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_model::id::PlayerId;
use ti4_replayer::app::{
    BranchHandle, PlayBlock, RebuildOutcome, RebuildStatus, ReplayApp, ReplaySettings,
    SETTINGS_PATH, SetupDefaults,
};
use ti4_replayer::control::{
    ManualSubmission, ModeEffect, OfferId, PendingManualChoice, SeatControl, SeatMode,
    SubmitOutcome,
};
use ti4_replayer::live::{AdvanceGoal, FrameTick, LiveError, LiveState, Snapshot};
use ti4_replayer::project::{MAX_TOTAL_FRAMES, ReplayInputs, SourceTimeline, Verification};
use ti4_replayer::{BranchId, ReplayerProject};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";
const SEATS: [&str; 6] = ["seat0", "seat1", "seat2", "seat3", "seat4", "seat5"];

// ---------------------------------------------------------------------------
// A branch the app can be pointed at without a game running.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct State {
    live: Option<LiveState>,
    seats: SeatControl,
    pending: Option<PendingManualChoice>,
    submissions: Vec<ManualSubmission>,
    runs: Vec<AdvanceGoal>,
    pauses: usize,
    stops: usize,
}

/// A branch the app can be told about. Cloning hands the test a second handle onto the same state,
/// so it can see what the app asked for.
#[derive(Clone, Default)]
struct Fake {
    state: Arc<Mutex<State>>,
    /// What the next submission should answer instead of the truth.
    refuse: Arc<Mutex<Option<SubmitOutcome>>>,
}

impl Fake {
    fn park(&self, choice: PendingManualChoice) {
        let mut state = self.state.lock().expect("unlocked");
        state.live = Some(LiveState::WaitingForHuman);
        state.seats.set_mode(&choice.actor, SeatMode::Manual);
        state.pending = Some(choice);
    }

    fn refuse_next(&self, outcome: SubmitOutcome) {
        *self.refuse.lock().expect("unlocked") = Some(outcome);
    }

    fn counts(&self) -> (usize, usize, usize) {
        let state = self.state.lock().expect("unlocked");
        (state.runs.len(), state.pauses, state.stops)
    }
}

impl BranchHandle for Fake {
    fn snapshot(&self) -> Snapshot {
        let state = self.state.lock().expect("unlocked");
        Snapshot {
            state: state.live.unwrap_or_default(),
            seats: state.seats.clone(),
            pending: state.pending.clone(),
        }
    }

    fn set_mode(&self, seat: &PlayerId, mode: SeatMode) -> ModeEffect {
        let mut state = self.state.lock().expect("unlocked");
        let previous = state.seats.mode(seat);
        state.seats.set_mode(seat, mode);
        // The gate hands a parked choice to the policy when its own seat goes back to Auto.
        if previous == SeatMode::Manual
            && mode == SeatMode::Auto
            && state
                .pending
                .as_ref()
                .is_some_and(|pending| &pending.actor == seat)
        {
            let choice = state
                .pending
                .take()
                .expect("the choice that made this a release")
                .fingerprint;
            state.live = Some(LiveState::Running);
            return ModeEffect::ReleasedBot { previous, choice };
        }
        ModeEffect::Changed { previous }
    }

    fn submit(&self, submission: &ManualSubmission) -> SubmitOutcome {
        let mut state = self.state.lock().expect("unlocked");
        state.submissions.push(submission.clone());
        if let Some(refusal) = self.refuse.lock().expect("unlocked").take() {
            return refusal;
        }
        let option_id = match state.pending.take() {
            None => return SubmitOutcome::NoPendingChoice,
            Some(pending) if pending.fingerprint != submission.fingerprint => {
                let current = pending.fingerprint.clone();
                state.pending = Some(pending);
                return SubmitOutcome::Stale { current };
            }
            Some(_) => {
                state.live = Some(LiveState::Running);
                submission.option_id.clone()
            }
        };
        SubmitOutcome::Accepted { option_id }
    }

    fn run(&self, goal: AdvanceGoal) -> Result<(), LiveError> {
        let mut state = self.state.lock().expect("unlocked");
        state.runs.push(goal);
        state.live = Some(LiveState::Running);
        Ok(())
    }

    fn pause(&self) {
        // A pause is a request: the branch keeps running until it reaches a step boundary.
        self.state.lock().expect("unlocked").pauses += 1;
    }

    fn stop(&self) {
        self.state.lock().expect("unlocked").stops += 1;
    }

    fn delegate_pending(&self, offer: OfferId) -> Result<PlayerId, LiveError> {
        let mut state = self.state.lock().expect("unlocked");
        // Mirrors the gate: only the named occurrence may be delegated, and a mismatch consumes
        // nothing.
        if state.pending.as_ref().map(|pending| pending.offer) != Some(offer) {
            return Err(LiveError::NotRunnable(LiveState::Ready));
        }
        let pending = state
            .pending
            .take()
            .ok_or(LiveError::NotRunnable(LiveState::Ready))?;
        state.live = Some(LiveState::Running);
        Ok(pending.actor)
    }
}

/// An engine choice, wrapped the way the gate wraps one before it reaches a panel.
fn choice(seat: &str, frame: u64, ask: u32) -> PendingManualChoice {
    let engine = Choice::new(
        PlayerId::new(seat),
        "Activate which planet?",
        vec![
            option("planet:iod", "planet", "Iod"),
            option("skip", "skip", "Skip"),
        ],
    );
    PendingManualChoice::new(BranchId::SOURCE, frame, ask, None, &engine)
        .expect("a two option choice")
}

fn option(id: &str, kind: &str, label: &str) -> ChoiceOption {
    ChoiceOption {
        id: id.to_owned(),
        kind: kind.to_owned(),
        label: label.to_owned(),
        payload: BTreeMap::new(),
        preview: None,
    }
}

fn submission_for(pending: &PendingManualChoice, option: usize) -> ManualSubmission {
    ManualSubmission {
        offer: pending.offer,
        fingerprint: pending.fingerprint.clone(),
        option_id: pending.options[option].id.clone(),
    }
}

// ---------------------------------------------------------------------------
// A project with a proven source branch and a frame timeline to look at.
// ---------------------------------------------------------------------------

fn inputs() -> ReplayInputs {
    ReplayInputs {
        checkpoint: CHECKPOINT.to_owned(),
        map_pool: MAP_POOL.to_owned(),
        seed: 4_242,
        rotation: 1,
        profile_table: "learner".to_owned(),
        temperature: 0.5,
        diplomacy: false,
        lineup: None,
    }
}

fn verification() -> Verification {
    Verification {
        matches: true,
        engine_commit: "abcdef1234567890".to_owned(),
        engine_matches: true,
        content: "ti4-content-test".to_owned(),
    }
}

/// A project whose source branch has `frames` frames and has been reproduced. Reproduction is not
/// faked casually: in the app only the rebuild code sets it, and a test that wants an unproven
/// branch asks for one explicitly.
fn project(frames: usize) -> ReplayerProject {
    let mut project = ReplayerProject::new(
        inputs(),
        SourceTimeline {
            session: "out/reviews/session.r01.json".to_owned(),
            session_sha256: "a".repeat(64),
            checkpoint_sha256: "b".repeat(64),
            map_pool_sha256: "c".repeat(64),
            engine_commit: Some("abcdef1234567890".to_owned()),
            content_sha256: Some("ti4-content-test".to_owned()),
            frames: u64::try_from(frames).unwrap_or(u64::MAX),
            factions: SEATS.iter().map(|seat| format!("faction-{seat}")).collect(),
        },
    );
    project.branches[0].frames = u64::try_from(frames).unwrap_or(u64::MAX);
    project.branches[0].verified = true;
    project
}

fn ticks(frames: usize) -> Vec<FrameTick> {
    (0..frames)
        .map(|index| FrameTick {
            index,
            engine_step: index * 2,
            round: u32::try_from(index / 20 + 1).unwrap_or(u32::MAX),
            phase: "status".to_owned(),
            active: None,
            decisions: index % 3,
            finished: false,
            error: None,
        })
        .collect()
}

fn app_with(frames: usize) -> ReplayApp<Fake> {
    let project = project(frames);
    let app = ReplayApp::new(
        project,
        verification(),
        Path::new("out/replays/test.r02.json"),
    );
    let mut app = app;
    app.attach(Fake::default(), ticks(frames));
    app
}

// ---------------------------------------------------------------------------
// Seats
// ---------------------------------------------------------------------------

#[test]
fn six_seats_toggle_without_disturbing_each_other() {
    let mut app = app_with(10);
    for seat in SEATS {
        assert_eq!(app.seats().mode(&PlayerId::new(seat)), SeatMode::Auto);
    }
    for seat in SEATS {
        assert_eq!(app.toggle_seat(&PlayerId::new(seat)), SeatMode::Manual);
    }
    let seats = app.seats();
    for seat in SEATS {
        assert_eq!(
            seats.mode(&PlayerId::new(seat)),
            SeatMode::Manual,
            "{seat} stayed manual through the other five toggles"
        );
    }
    assert_eq!(app.toggle_seat(&PlayerId::new("seat2")), SeatMode::Auto);
    let seats = app.seats();
    assert_eq!(seats.mode(&PlayerId::new("seat1")), SeatMode::Manual);
    assert_eq!(seats.mode(&PlayerId::new("seat3")), SeatMode::Manual);
    // The intent is remembered on the branch, so the next run of this branch starts the same way.
    let branch = app
        .project()
        .branch(BranchId::SOURCE)
        .expect("the source branch");
    assert_eq!(branch.seats.len(), SEATS.len());
}

#[test]
fn a_seat_returned_to_auto_lets_the_policy_answer_the_parked_choice() {
    let mut app = app_with(10);
    let fake = Fake::default();
    fake.park(choice("seat1", 4, 0));
    app.attach(fake, ticks(10));
    assert!(app.pending().is_some(), "a manual seat is waiting");
    app.set_seat_mode(&PlayerId::new("seat1"), SeatMode::Auto);
    assert!(
        app.pending().is_none(),
        "the panel closes when the seat goes back to the policy"
    );
    assert!(
        app.notice()
            .unwrap_or_default()
            .contains("goes to the policy"),
        "and the app says so: {:?}",
        app.notice()
    );
}

// ---------------------------------------------------------------------------
// Answering
// ---------------------------------------------------------------------------

#[test]
fn an_answer_closes_the_panel_and_a_second_click_is_refused() {
    let mut app = app_with(10);
    let fake = Fake::default();
    let pending = choice("seat1", 4, 0);
    fake.park(pending.clone());
    app.attach(fake, ticks(10));
    assert_eq!(app.pending().expect("parked").actor, PlayerId::new("seat1"));
    assert_eq!(
        app.submit(&submission_for(&pending, 0)),
        SubmitOutcome::Accepted {
            option_id: "planet:iod".to_owned()
        }
    );
    assert!(
        app.pending().is_none(),
        "the panel shows the answer, not the buttons"
    );
    // The same click arriving twice must not look like a second decision.
    assert_eq!(
        app.submit(&submission_for(&pending, 1)),
        SubmitOutcome::Duplicate,
        "the app refuses on its own even before asking the branch"
    );
}

#[test]
fn a_click_aimed_at_a_choice_that_moved_is_refused_and_the_panel_redraws() {
    let mut app = app_with(10);
    let fake = Fake::default();
    let old = choice("seat1", 4, 0);
    let fresh = choice("seat2", 6, 1);
    fake.park(old.clone());
    app.attach(fake.clone(), ticks(10));
    fake.refuse_next(SubmitOutcome::Stale {
        current: fresh.fingerprint.clone(),
    });
    // Pretend the branch moved between the click and the answer.
    fake.park(fresh.clone());
    assert_eq!(
        app.submit(&submission_for(&old, 0)),
        SubmitOutcome::Stale {
            current: fresh.fingerprint.clone()
        },
        "the refusal reaches the caller intact"
    );
    assert_eq!(
        app.pending().expect("the panel redraws").fingerprint,
        fresh.fingerprint,
        "and the app is showing the choice that is actually pending"
    );
}

#[test]
fn a_seat_can_let_the_policy_answer_just_this_once() {
    let mut app = app_with(10);
    let fake = Fake::default();
    let parked = choice("seat3", 9, 0);
    fake.park(parked.clone());
    app.attach(fake, ticks(10));
    assert_eq!(
        app.delegate_pending(parked.offer).expect("one-shot"),
        PlayerId::new("seat3")
    );
    assert!(app.pending().is_none());
    assert!(app.notice().unwrap_or_default().contains("from the policy"));
}

#[test]
fn run_pause_and_stop_reach_the_branch_and_nothing_else() {
    let mut app = app_with(10);
    let fake = Fake::default();
    app.attach(fake.clone(), ticks(10));
    app.advance(AdvanceGoal::Steps(3)).expect("accepted");
    app.pause();
    let (runs, pauses, stops) = fake.counts();
    assert_eq!(runs, 1, "one goal was asked for");
    assert_eq!(pauses, 1, "one pause was asked for");
    assert_eq!(
        stops, 0,
        "nothing stopped the branch behind the reader's back"
    );
    assert_eq!(
        app.live_state(),
        Some(LiveState::Running),
        "a pause is a request; the branch is still running until the boundary"
    );
    app.detach();
    assert_eq!(fake.counts().2, 1, "detaching stops the branch");
}

#[test]
fn an_answer_with_nothing_running_is_simply_refused() {
    let mut app = app_with(10);
    app.detach();
    let pending = choice("seat1", 4, 0);
    assert_eq!(
        app.submit(&submission_for(&pending, 0)),
        SubmitOutcome::NoPendingChoice
    );
    assert!(
        app.notice()
            .unwrap_or_default()
            .contains("Nothing is running")
    );
}

// ---------------------------------------------------------------------------
// Navigation and the branch tree
// ---------------------------------------------------------------------------

#[test]
fn looking_through_history_does_not_branch_anything() {
    let mut app = app_with(40);
    let before = app.project().clone();
    for index in [0, 7, 39, 12, 12, 0] {
        app.select_frame(index);
        assert_eq!(app.viewed(BranchId::SOURCE), Some(index));
    }
    app.previous_frame();
    app.next_frame();
    app.go_to_tip();
    assert_eq!(app.viewed(BranchId::SOURCE), Some(39));
    assert!(app.at_tip(), "the tip is marked as such");
    app.select_frame(3);
    assert!(!app.at_tip(), "and looking back in time is not the tip");
    assert_eq!(
        app.project(),
        &before,
        "no navigation keypress may create or alter a branch"
    );
}

#[test]
fn the_tree_shows_what_a_branch_has_proved() {
    let mut app = app_with(30);
    let nodes = app.tree();
    assert_eq!(nodes.len(), 1);
    assert!(
        nodes[0].playable,
        "a reproduced source branch can be played"
    );
    assert_eq!(nodes[0].frames, 30);
    assert!(nodes[0].fork_frame.is_none(), "the source has no fork");

    // An unproven branch of the same shape is inspectable and not playable.
    app.project_mut().branches[0].verified = false;
    let nodes = app.tree();
    assert!(!nodes[0].verified);
    assert!(
        !nodes[0].playable,
        "an unproven branch is readable, not playable"
    );
}

#[test]
fn switching_branches_stops_the_running_one_and_keeps_seen_frames() {
    let mut app = app_with(20);
    app.select_frame(5);
    let mut project = app.project().clone();
    let child = project
        .fork(
            BranchId::SOURCE,
            5,
            Some("what if".to_owned()),
            vec![ti4_replayer::SeatSetting {
                player: "seat1".to_owned(),
                mode: SeatMode::Manual,
            }],
        )
        .expect("a fork");
    let mut app = ReplayApp::new(project, verification(), Path::new("p.r02.json"));
    app.attach(Fake::default(), ticks(20));
    assert!(app.handle().is_some());
    assert!(app.select_branch(child), "the fork is a real branch");
    assert!(
        app.handle().is_none(),
        "switching view does not keep a branch running"
    );
    assert_eq!(app.current(), child);
    assert!(!app.at_tip(), "a branch with no frames seen has no tip yet");
}

// ---------------------------------------------------------------------------
// Play from this frame
// ---------------------------------------------------------------------------

#[test]
fn play_is_refused_until_the_inputs_and_the_branch_are_proven() {
    // Inputs on disk are not what the project recorded.
    let mut app = ReplayApp::new(
        project(20),
        Verification {
            matches: false,
            engine_commit: "x".to_owned(),
            engine_matches: true,
            content: "ti4-content-test".to_owned(),
        },
        Path::new("out/replays/edited.r02.json"),
    );
    app.attach(Fake::default(), ticks(20));
    app.select_frame(10);
    let blocked = app.play_check().expect_err("unverified inputs block Play");
    assert!(matches!(blocked, PlayBlock::InputsUnverified(_)));
    assert!(blocked.tooltip().contains("checkpoint"));
    assert!(
        app.plan_play(vec![]).is_err(),
        "and the plan is refused, not merely discouraged"
    );

    // Verified inputs, branch never reproduced.
    let mut app = app_with(20);
    app.select_frame(10);
    app.project_mut().branches[0].verified = false;
    let blocked = app
        .play_check()
        .expect_err("an unproven branch cannot be forked from");
    assert_eq!(blocked, PlayBlock::BranchUnverified);
    assert!(blocked.tooltip().contains("reproduced"));

    // A running branch cannot fork: Play is about recorded history.
    let mut app = app_with(20);
    app.select_frame(10);
    app.advance(AdvanceGoal::Steps(1)).expect("accepted");
    let blocked = app.play_check().expect_err("a running branch blocks Play");
    assert!(matches!(blocked, PlayBlock::BranchRunning(_)));
    assert!(blocked.tooltip().contains("pause or stop"));

    // A terminal frame has nothing after it.
    let mut app = app_with(20);
    let mut ticks = ticks(20);
    ticks[19].finished = true;
    app.attach(Fake::default(), ticks);
    app.go_to_tip();
    assert_eq!(
        app.play_check().expect_err("the end is the end"),
        PlayBlock::TerminalFrame
    );

    // Every refusal says something a reader can act on.
    let blocks = [
        PlayBlock::Rebuilding,
        PlayBlock::NoFrame,
        PlayBlock::TooManyBranches { limit: 128 },
        PlayBlock::FrameBudget {
            total: MAX_TOTAL_FRAMES,
            limit: MAX_TOTAL_FRAMES,
        },
        PlayBlock::Refused("no parent".to_owned()),
    ];
    for block in blocks {
        assert!(block.tooltip().len() > 20, "{block:?} is too quiet");
    }
}

#[test]
fn play_forks_at_the_viewed_frame_and_asks_for_the_prefix() {
    let mut app = app_with(50);
    app.select_frame(20);
    let plan = app
        .plan_play(vec![])
        .expect("a proven branch may be forked");
    assert_eq!(plan.parent, BranchId::SOURCE);
    assert_eq!(plan.frame, 20);
    assert_ne!(plan.child, plan.parent);
    assert!(app.project().children_of(plan.child).is_empty());
    assert_eq!(
        app.rebuild_status(),
        RebuildStatus::Running {
            branch: plan.child,
            frame: 20
        }
    );
    // The parent's future survives untouched: same length, same answers, still verified.
    let parent = app.project().branch(plan.parent).expect("the parent");
    assert_eq!(parent.frames, 50);
    assert!(parent.verified);
    let children = app.project().children_of(BranchId::SOURCE);
    assert_eq!(children, vec![plan.child]);
}

#[test]
fn a_finished_rebuild_becomes_the_view_and_the_parent_stays_whole() {
    let mut app = app_with(50);
    app.select_frame(20);
    let plan = app.plan_play(vec![]).expect("forked");
    let mut more = ticks(15);
    for (offset, tick) in more.iter_mut().enumerate() {
        tick.index = 20 + offset;
    }
    app.finish_rebuild(
        RebuildOutcome {
            frames: more,
            replayed: 7,
        },
        plan.child,
    );
    assert_eq!(app.current(), plan.child);
    assert!(app.at_tip(), "the rebuild lands the reader on the live tip");
    assert_eq!(app.frames(plan.child).len(), 15);
    let child = app.project().branch(plan.child).expect("the child");
    assert!(child.verified, "a reproduced branch is a proven branch");
    assert_eq!(child.frames, 15);
    let parent = app.project().branch(plan.parent).expect("the parent");
    assert_eq!(
        parent.frames, 50,
        "the original future is still there to look at"
    );
    assert_eq!(
        app.rebuild_status(),
        RebuildStatus::Done {
            branch: plan.child,
            replayed: 7
        }
    );
    // And the reader can go back to the parent at any time.
    assert!(app.select_branch(plan.parent));
    assert_eq!(app.frames(plan.parent).len(), 50);
}

#[test]
fn a_refused_rebuild_leaves_no_branch_behind() {
    let mut app = app_with(50);
    app.select_frame(30);
    let plan = app.plan_play(vec![]).expect("forked");
    assert_eq!(app.project().branches.len(), 2);
    app.fail_rebuild(plan.child, "the record did not match the ask at frame 12");
    assert_eq!(
        app.project().branches.len(),
        1,
        "a branch that cannot be reproduced must not sit in the tree"
    );
    assert!(matches!(app.rebuild_status(), RebuildStatus::Failed(_)));
    assert_eq!(app.current(), BranchId::SOURCE);
    // Play is available again: the refusal did not leave the app busy.
    app.select_frame(30);
    app.plan_play(vec![]).expect("Play is not stuck");
}

#[test]
fn closing_the_window_while_rebuilding_is_safe_and_honest() {
    let mut app = app_with(50);
    app.select_frame(40);
    let plan = app.plan_play(vec![]).expect("forked");
    let in_flight = app.close();
    assert_eq!(
        in_flight,
        RebuildStatus::Running {
            branch: plan.child,
            frame: 40
        },
        "the caller is told what was still running, so it can ask about saving"
    );
    assert_eq!(
        app.project().branches.len(),
        1,
        "the half-built fork is gone"
    );
    assert_eq!(app.rebuild_status(), RebuildStatus::Idle);
    assert!(app.handle().is_none());
    // Closing twice is not an event.
    assert_eq!(app.close(), RebuildStatus::Idle);
}

#[test]
fn cancelling_a_rebuild_stops_it_looking_like_a_branch() {
    let mut app = app_with(50);
    app.select_frame(1);
    let plan = app.plan_play(vec![]).expect("forked");
    app.cancel_rebuild();
    assert_eq!(app.rebuild_status(), RebuildStatus::Idle);
    assert_eq!(app.project().branches.len(), 1);
    assert!(app.project().branch(plan.child).is_none());
}

// ---------------------------------------------------------------------------
// Settings live somewhere else
// ---------------------------------------------------------------------------

#[test]
fn the_replayer_never_reads_or_writes_the_reviewers_settings() {
    assert_eq!(SETTINGS_PATH, "out/replays/replayer-settings.json");
    assert!(
        !SETTINGS_PATH.contains("reviews"),
        "the reviewer keeps out/reviews/reviewer-settings.json"
    );
    let dir = std::env::temp_dir().join(format!("ti4-r02-007a-settings-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a temporary settings directory");
    let reviewer = dir.join("out/reviews/reviewer-settings.json");
    fs::create_dir_all(reviewer.parent().expect("a parent")).expect("out/reviews");
    fs::write(&reviewer, b"{\"reviewer\":true}").expect("a reviewer settings file");

    let path = dir.join(SETTINGS_PATH);
    let settings = ReplaySettings {
        window_width: 1234.0,
        last_project: Some("out/replays/what-if.r02.json".to_owned()),
        last_branch: Some(3),
        setup: SetupDefaults {
            checkpoint: "out/checkpoints/run-11/checkpoint-602331/slots.json".to_owned(),
            map_pool: "out/pools/full_np8_12_holdout.json".to_owned(),
            seed: "007".to_owned(),
            rotation: 4,
            profile_table: "Accepted".to_owned(),
            temperature: 0.05,
            diplomacy: true,
            lineup: ["sol", "ghost", "keleresx", "argent", "naalu", "crimson"]
                .map(str::to_owned)
                .to_vec(),
        },
        ..ReplaySettings::default()
    };
    settings.save(&path).expect("settings save");
    let restored = ReplaySettings::load(&path);
    assert_eq!(restored, settings);
    // The table you last played at is what the setup form opens with, seed and rotation included - and
    // a seed typed with a leading zero stays typed that way, because it is text and not a number.
    assert_eq!(restored.setup.seed, "007");
    assert_eq!(restored.setup.rotation, 4);
    assert_eq!(restored.setup.profile_table, "Accepted");
    assert!(restored.setup.diplomacy);
    assert_eq!(
        fs::read(&reviewer).expect("the reviewer's file is still readable"),
        b"{\"reviewer\":true}",
        "saving replayer settings must not touch the reviewer's"
    );

    // A file written before the setup group existed still loads, and keeps what it does know.
    fs::write(
        &path,
        b"{\"window_width\":1.0,\"last_project\":\"out/replays/a.r02.json\"}",
    )
    .expect("write");
    let older = ReplaySettings::load(&path);
    assert!((older.window_width - 1.0).abs() < f32::EPSILON);
    assert_eq!(
        older.last_project.as_deref(),
        Some("out/replays/a.r02.json")
    );
    assert_eq!(older.setup, SetupDefaults::default());

    // A settings file from a future version is ignored rather than trusted.
    fs::write(&path, b"{\"window_width\":1.0,\"hologram\":\"yes\"}").expect("write");
    assert_eq!(ReplaySettings::load(&path), ReplaySettings::default());
    // And an unreadable one too.
    fs::write(&path, b"not json at all").expect("write");
    assert_eq!(ReplaySettings::load(&path), ReplaySettings::default());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn frames_arriving_from_a_live_branch_extend_the_view_without_losing_position() {
    let mut app = app_with(10);
    app.select_frame(4);
    let mut more = ticks(14);
    more.drain(..10);
    app.record_frames(&more);
    assert_eq!(app.frames(BranchId::SOURCE).len(), 14);
    assert_eq!(
        app.viewed(BranchId::SOURCE),
        Some(4),
        "reading a decision at frame 4 is not interrupted by the next frame arriving"
    );
    app.go_to_tip();
    app.record_frames(&ticks(3));
    assert_eq!(
        app.frames(BranchId::SOURCE).len(),
        14,
        "a replay of frames already seen does not rewind or duplicate the timeline"
    );
    assert!(app.at_tip());
}

/// The other half of the rule above: a reader who is *watching* follows.
///
/// Before this, `at_tip` was asked after the new frames had already been appended, so it answered
/// "no" every time a frame arrived and the view never moved again. On a table started in the window
/// that meant the board, the player sheets and the step panel all stayed on frame zero for the whole
/// game, while the timeline counted up beside them.
#[test]
fn a_reader_at_the_tip_is_carried_along_by_the_branch() {
    let mut app = app_with(10);
    app.go_to_tip();
    assert!(app.at_tip());
    let mut arriving = ticks(14);
    arriving.drain(..10);
    app.record_frames(&arriving);
    assert_eq!(
        app.viewed(BranchId::SOURCE),
        Some(13),
        "watching the tip means watching what the branch does next"
    );
    assert!(app.at_tip());
    // And one more batch, because the bug was in how the question was asked, not in the first answer.
    let mut again = ticks(16);
    again.drain(..14);
    app.record_frames(&again);
    assert_eq!(app.viewed(BranchId::SOURCE), Some(15));
}

/// The project counts frames too, and `fork` refuses a frame past the end of the parent - so a
/// branch whose count is never updated cannot be forked from at all. A table started in the window
/// begins at zero frames recorded, which is how "Play from this frame" came to answer "branch-0 ends
/// at frame 0" for every frame of a game in progress.
#[test]
fn frames_arriving_grow_the_branch_the_project_knows_about() {
    let mut app = app_with(0);
    assert_eq!(
        app.project()
            .branch(BranchId::SOURCE)
            .expect("source")
            .frames,
        0
    );
    app.record_frames(&ticks(12));
    assert_eq!(
        app.project()
            .branch(BranchId::SOURCE)
            .expect("source")
            .frames,
        12,
        "the project's count follows the frames the branch has actually produced"
    );
    app.select_frame(7);
    app.plan_play(Vec::new())
        .expect("a frame the branch has reached can be forked from");
}

/// Two consecutive asks that look identical are two questions, not one answered twice.
///
/// `ChoiceFingerprint` excludes the frame and the ask ordinal on purpose, so it cannot separate
/// them: a seat six units over capacity is asked "remove a unit: over capacity in 14" six times in
/// one engine step, each time with the single option `remove|0` and no context. Suppressing the
/// panel on the fingerprint alone hid every ask after the first — the window said "No seat is
/// waiting" while the engine sat parked inside `ask`, a frozen game with nothing on screen. What
/// identifies an instance is `(fingerprint, frame, ask)`.
#[test]
fn an_identical_looking_ask_at_the_next_ordinal_is_a_new_question() {
    let mut app = app_with(10);
    let fake = Fake::default();
    app.attach(fake.clone(), ticks(10));

    let first = choice(SEATS[0], 7, 1);
    fake.park(first.clone());
    let offered = app.pending().expect("the first ask is shown");
    assert_eq!(offered.ask, 1);
    assert_eq!(
        app.submit(&submission_for(&first, 0)),
        SubmitOutcome::Accepted {
            option_id: first.options[0].id.clone()
        }
    );
    assert!(
        app.pending().is_none(),
        "the answered offer is not shown again while it is still the pending one"
    );

    // The same offer, one ordinal later: same actor, prompt, options and context, so the same
    // fingerprint. It is still a question nobody has answered.
    let second = choice(SEATS[0], 7, 2);
    assert_eq!(
        second.fingerprint, first.fingerprint,
        "the fixture has to reproduce the collision, or this test proves nothing"
    );
    fake.park(second.clone());
    let shown = app
        .pending()
        .expect("an ask at the next ordinal must reach the panel");
    assert_eq!(shown.ask, 2);
    assert_eq!(
        app.submit(&submission_for(&second, 0)),
        SubmitOutcome::Accepted {
            option_id: second.options[0].id.clone()
        },
        "and it must be answerable, not refused as a duplicate"
    );

    // A genuine double click on the offer still on screen is still refused.
    assert_eq!(
        app.submit(&submission_for(&second, 0)),
        SubmitOutcome::Duplicate
    );
}
