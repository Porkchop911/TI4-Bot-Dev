//! Deterministic reconstruction: rebuild a branch by replaying it, and prove the target.
//!
//! Every test here runs the real reviewer twice — once to make a recording, once to rebuild from it —
//! and compares frame digests. The prefix a rebuild consumes is the recording branch's own
//! [`ti4_replayer::ReplayRecord`]s, so nothing here reconstructs an offer from the reviewer's trace:
//! what is validated is exactly what the engine asked. Inputs are the committed example bundle and
//! map pool, so there is no skip path.

// Every test here drives a live branch, which only the `host` build has.
#![cfg(feature = "host")]

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use ti4_model::id::PlayerId;
use ti4_replayer::fingerprint::{FrameFingerprint, first_difference};
use ti4_replayer::live::{AdvanceGoal, LiveBranch, LiveState};
use ti4_replayer::rebuild::{
    MismatchKind, RebuildBounds, RebuildError, RebuildTarget, Rebuilt, ReplayScript,
    describe_frame_difference, rebuild,
};
use ti4_replayer::{
    ChoiceFingerprint, ManualSubmission, ReplayRecord, SeatControl, SeatMode, SubmitOutcome,
};
use ti4_review::{ProfileTable, ReviewFrame, SimulationConfig};

const CHECKPOINT: &str = "examples/reviewer/checkpoint-473312/slots.json";
const MAP_POOL: &str = "examples/reviewer/full_np8_12_holdout.json";
const SEED: u64 = 4_242;
const ROTATION: usize = 1;
const TEMPERATURE: f64 = 0.5;
const WAIT: Duration = Duration::from_secs(240);

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<name> sits directly under the workspace root")
        .to_path_buf()
}

fn config() -> SimulationConfig {
    let root = workspace_root();
    let config = SimulationConfig {
        checkpoint: std::env::var_os("TI4_REPLAYER_CHECKPOINT")
            .map_or_else(|| root.join(CHECKPOINT), std::path::PathBuf::from),
        map_pool: std::env::var_os("TI4_REPLAYER_POOL")
            .map_or_else(|| root.join(MAP_POOL), std::path::PathBuf::from),
        seed: SEED,
        rotation: ROTATION,
        table: ProfileTable::Learner,
        temperature: TEMPERATURE,
        diplomacy: false,
        lineup: None,
    };
    for path in [&config.checkpoint, &config.map_pool] {
        assert!(
            path.is_file(),
            "rebuild input {} is missing from the checkout",
            path.display()
        );
    }
    config
}

/// A recording run: every settled decision recorded by the gate, plus the frames it produced.
struct Recording {
    frames: Vec<ReviewFrame>,
    records: Vec<ReplayRecord>,
}

impl Recording {
    /// The frame index just past the `n`th recorded decision, clipped to what exists.
    fn just_past(&self, n: usize) -> u64 {
        let last = self.frames.len().saturating_sub(1) as u64;
        self.records
            .get(n.saturating_sub(1))
            .map_or(last, |record| record.frame.saturating_add(1).min(last))
    }

    fn digests(&self) -> Vec<FrameFingerprint> {
        self.frames.iter().map(FrameFingerprint::of).collect()
    }
}

/// Play `steps` engine steps with the named seats on Manual, recording every settled decision.
fn record(steps: usize, manual: &[&str]) -> Recording {
    record_with(AdvanceGoal::Steps(steps), manual)
}

/// Play until `goal` is satisfied, recording every settled decision.
fn record_with(goal: AdvanceGoal, manual: &[&str]) -> Recording {
    let mut seats = SeatControl::all_auto();
    for name in manual {
        seats.set_mode(&PlayerId::new(*name), SeatMode::Manual);
    }
    let branch = LiveBranch::start(config(), seats).expect("spawn the branch");
    let deadline = Instant::now() + WAIT;
    while branch.gate().state() != LiveState::Ready {
        assert_ne!(
            branch.gate().state(),
            LiveState::Failed,
            "the recording branch failed to start"
        );
        assert!(Instant::now() < deadline, "the branch never became ready");
        std::thread::sleep(Duration::from_millis(5));
    }
    branch.gate().enable_recording();
    branch.gate().run(goal).expect("the advance is accepted");
    let until = Instant::now() + WAIT;
    loop {
        if let Some(pending) = branch.gate().pending() {
            let option_id = pending
                .options
                .first()
                .expect("a panel offers something")
                .id
                .clone();
            assert_eq!(
                branch.gate().submit(&ManualSubmission {
                    offer: pending.offer,
                    fingerprint: pending.fingerprint,
                    option_id: option_id.clone(),
                }),
                SubmitOutcome::Accepted { option_id },
                "answering the panel currently on screen must be accepted"
            );
            continue;
        }
        if branch.wait_until_idle(Duration::from_millis(5)) {
            break;
        }
        assert!(Instant::now() < until, "the recording run stalled");
        std::thread::sleep(Duration::from_millis(2));
    }
    let records = branch.gate().records();
    let session = branch.into_session().expect("the session comes back");
    Recording {
        frames: session.frames,
        records,
    }
}

/// Rebuild and prove every recorded frame, returning how many prefix choices were replayed.
fn rebuild_and_prove(
    source: &Recording,
    script: Vec<ReplayRecord>,
    target: RebuildTarget,
) -> usize {
    let want = source.digests();
    let cancel = AtomicBool::new(false);
    let rebuilt = rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(script),
        &want,
        target,
        RebuildBounds::default(),
        &cancel,
    )
    .expect("the rebuild reproduces its target");
    let expected_frames = match target {
        RebuildTarget::Frame(index) => {
            (usize::try_from(index).expect("a frame index fits") + 1).min(source.frames.len())
        }
        RebuildTarget::End => source.frames.len(),
    };
    assert_eq!(
        rebuilt.frames, expected_frames,
        "the rebuild should stop on the target frame"
    );
    prove(&rebuilt.review.session.frames, &source.frames, &want);
    rebuilt.replayed
}

fn prove(rebuilt: &[ReviewFrame], original: &[ReviewFrame], want: &[FrameFingerprint]) {
    for (index, frame) in rebuilt.iter().enumerate() {
        assert_eq!(
            FrameFingerprint::of(frame),
            want[index],
            "rebuilt frame {index} differs from the recording: {}",
            describe_frame_difference(frame, &original[index])
                .or_else(|| first_difference(frame, &original[index]))
                .unwrap_or_else(|| "field-by-field comparison found nothing".to_owned())
        );
    }
}

fn attempt(
    source: &Recording,
    script: Vec<ReplayRecord>,
    target: RebuildTarget,
) -> Result<usize, RebuildError> {
    let want = source.digests();
    let cancel = AtomicBool::new(false);
    rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(script),
        &want,
        target,
        RebuildBounds::default(),
        &cancel,
    )
    .map(|rebuilt| rebuilt.replayed)
}

/// How one test spoofs a recorded decision.
type Tamper = fn(&mut ReplayRecord);

/// Unwrap the refusal a rebuild owes us. `Rebuilt` holds the reviewer, so `expect_err` is not
/// available; in these tests the error is the whole point.
fn refused(result: Result<Rebuilt, RebuildError>) -> RebuildError {
    match result {
        Ok(_) => panic!("the rebuild should have refused, and produced a branch instead"),
        Err(error) => error,
    }
}

fn bogus_fingerprint() -> ChoiceFingerprint {
    serde_json::from_str(
        "\"sha256:0000000000000000000000000000000000000000000000000000000000000000\"",
    )
    .expect("a fingerprint is a transparent string")
}

#[test]
fn frame_zero_is_a_legitimate_branch_point() {
    let source = record(3, &[]);
    let replayed = rebuild_and_prove(&source, Vec::new(), RebuildTarget::Frame(0));
    assert_eq!(replayed, 0, "frame 0 has no prefix to replay");
}

#[test]
fn an_automatic_prefix_needs_no_records_at_all() {
    // Nothing recorded, nothing forced: plain deterministic replay of the policy, which is the floor
    // every other guarantee here stands on.
    let source = record(14, &[]);
    let replayed = rebuild_and_prove(&source, Vec::new(), RebuildTarget::Frame(13));
    assert_eq!(replayed, 0);
}

#[test]
fn a_recorded_decision_prefix_rebuilds_exactly() {
    let source = record(24, &[]);
    assert!(
        source.records.len() >= 5,
        "24 steps should settle several decisions, got {}",
        source.records.len()
    );
    let prefix: Vec<ReplayRecord> = source.records.iter().take(5).cloned().collect();
    let target = source.just_past(5);
    let replayed = rebuild_and_prove(&source, prefix, RebuildTarget::Frame(target));
    assert_eq!(replayed, 5, "five prefix choices were replayed");
}

#[test]
fn a_prefix_with_several_asks_in_one_frame_rebuilds_exactly() {
    let source = record(120, &[]);
    let mut by_frame: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for record in &source.records {
        *by_frame.entry(record.frame).or_default() += 1;
    }
    let (frame, count) = by_frame
        .iter()
        .filter(|(_, count)| **count > 1)
        .max_by_key(|(_, count)| **count)
        .map(|(frame, count)| (*frame, *count))
        .expect("no frame in 120 steps raised more than one decision");
    let index = source
        .frames
        .iter()
        .position(|candidate| candidate.index as u64 == frame)
        .expect("the frame exists");
    let prefix: Vec<ReplayRecord> = source
        .records
        .iter()
        .filter(|record| record.frame <= frame)
        .cloned()
        .collect();
    let truncated = Recording {
        frames: source.frames[..=index].to_vec(),
        records: source.records.clone(),
    };
    let replayed = rebuild_and_prove(&truncated, prefix.clone(), RebuildTarget::Frame(frame));
    assert_eq!(
        replayed,
        prefix.len(),
        "all {count} ask(s) of frame {frame}, and everything before them, were replayed"
    );
}

#[test]
fn a_forced_prefix_then_auto_play_stays_aligned() {
    // The rule under test: invoke the policy once per prefix choice and throw its answer away. Skip
    // that and the sampling stream shifts, so frames *after* the prefix would drift.
    let source = record(40, &[]);
    assert!(
        source.records.len() >= 6,
        "expected at least six settled decisions in 40 steps, got {}",
        source.records.len()
    );
    let prefix: Vec<ReplayRecord> = source.records.iter().take(4).cloned().collect();
    let target = (source.frames.len() - 1) as u64;
    let want = source.digests();
    let cancel = AtomicBool::new(false);
    let rebuilt = rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(prefix.clone()),
        &want,
        RebuildTarget::Frame(target),
        RebuildBounds::default(),
        &cancel,
    )
    .expect("the rebuild reproduces the whole run");
    assert_eq!(rebuilt.replayed, prefix.len());
    assert!(
        rebuilt.policy_calls >= rebuilt.replayed,
        "the policy was invoked {} time(s) for {} forced choice(s)",
        rebuilt.policy_calls,
        rebuilt.replayed
    );
    prove(&rebuilt.review.session.frames, &source.frames, &want);
}

#[test]
fn a_terminal_target_rebuilds_exactly() {
    // The whole game, twice: every frame including the terminal one, whose digest binds `finished`
    // and the outcome state. This is the most expensive test in the package on purpose - a branch
    // that replays to the end is the strongest statement reconstruction can make.
    let source = record_with(AdvanceGoal::EndOfGame, &[]);
    let last = source.frames.len().saturating_sub(1);
    assert!(
        source.frames[last].finished,
        "the recording should have ended, but the last frame says {:?}",
        source.frames.last().and_then(|frame| frame.error.clone())
    );
    let replayed = rebuild_and_prove(&source, source.records.clone(), RebuildTarget::End);
    assert!(replayed > 0, "a whole game settles decisions");
}

#[test]
fn a_tampered_prefix_is_refused_with_a_typed_diagnostic() {
    let source = record(24, &[]);
    let target = source.just_past(2);
    assert!(target >= 1, "need more than one frame to tamper with");

    let cases: Vec<(&str, Tamper)> = vec![
        ("actor", |record| {
            record.actor = PlayerId::new("seat-not-here");
        }),
        ("prompt", |record| {
            record.prompt = "what did you have in mind?".to_owned();
        }),
        ("options", |record| {
            record.offered.push("x".to_owned());
        }),
        ("context", |record| {
            record.context = Some(serde_json::json!({"tampered": true}));
        }),
        ("chosen", |record| {
            record.chosen = "an-option-nobody-offered".to_owned();
        }),
        ("identity", |record| {
            record.fingerprint = bogus_fingerprint();
        }),
    ];
    for (field, tamper) in cases {
        let mut tampered = source.records.clone();
        tamper(&mut tampered[0]);
        let error = match attempt(&source, tampered, RebuildTarget::Frame(target)) {
            Err(error) => error,
            Ok(replayed) => panic!("a tampered {field} must be refused, replayed {replayed}"),
        };
        let RebuildError::Diverged(mismatch) = &error else {
            panic!("a tampered {field} must diverge, got {error:?}");
        };
        assert_eq!(
            mismatch.index, 0,
            "the tampered record is the first one ({field})"
        );
        let expected = match field {
            "actor" => MismatchKind::Actor,
            "prompt" => MismatchKind::Prompt,
            "options" => MismatchKind::Options,
            "context" => MismatchKind::Context,
            "chosen" => MismatchKind::Chosen,
            _ => MismatchKind::Identity,
        };
        assert_eq!(
            mismatch.kind, expected,
            "a tampered {field} must report {}; it said {:?} ({})",
            expected, mismatch.kind, mismatch.detail
        );
    }
}

#[test]
fn a_tampered_frame_digest_is_refused() {
    let source = record(6, &[]);
    let mut want = source.digests();
    let index = 1.min(want.len() - 1);
    want[index] = FrameFingerprint::from_value(&serde_json::json!({"not": "the frame"}));
    let cancel = AtomicBool::new(false);
    let error = refused(rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(Vec::new()),
        &want,
        RebuildTarget::Frame(2),
        RebuildBounds::default(),
        &cancel,
    ));
    assert!(
        matches!(&error, RebuildError::FrameMismatch { index: got, .. } if *got == index),
        "expected a frame mismatch at {index}, got {error:?}"
    );
}

#[test]
fn a_target_beyond_the_recording_is_refused() {
    let source = record(3, &[]);
    let want = source.digests();
    let cancel = AtomicBool::new(false);
    let error = refused(rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(Vec::new()),
        &want,
        RebuildTarget::Frame(9_000),
        RebuildBounds::default(),
        &cancel,
    ));
    assert!(
        matches!(error, RebuildError::TargetUnavailable { wanted: 9_000, .. }),
        "got {error:?}"
    );
}

#[test]
fn bounds_and_cancellation_produce_no_branch() {
    let source = record(12, &[]);
    let want = source.digests();
    let cancel = AtomicBool::new(false);
    let error = refused(rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(Vec::new()),
        &want,
        RebuildTarget::Frame(11),
        RebuildBounds {
            max_steps: 3,
            max_frames: 1_000_001,
        },
        &cancel,
    ));
    assert!(
        matches!(error, RebuildError::BoundsExceeded { max_steps: 3, .. }),
        "got {error:?}"
    );

    let cancel = AtomicBool::new(true);
    let error = refused(rebuild(
        &config(),
        SeatControl::all_auto(),
        ReplayScript::new(Vec::new()),
        &want,
        RebuildTarget::Frame(11),
        RebuildBounds::default(),
        &cancel,
    ));
    assert!(matches!(error, RebuildError::Cancelled), "got {error:?}");
}

#[test]
fn repeated_rebuilds_are_identical() {
    let source = record(20, &[]);
    let prefix: Vec<ReplayRecord> = source.records.iter().take(3).cloned().collect();
    let target = source.just_past(3);
    let want = source.digests();
    let mut runs = Vec::new();
    for _ in 0..2 {
        let cancel = AtomicBool::new(false);
        let rebuilt = rebuild(
            &config(),
            SeatControl::all_auto(),
            ReplayScript::new(prefix.clone()),
            &want,
            RebuildTarget::Frame(target),
            RebuildBounds::default(),
            &cancel,
        )
        .expect("the rebuild reproduces its target");
        runs.push(
            rebuilt
                .review
                .session
                .frames
                .iter()
                .map(FrameFingerprint::of)
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(runs[0], runs[1], "two rebuilds of one prefix must agree");
    assert_eq!(
        runs[0],
        want[..runs[0].len()],
        "the rebuilds must agree with the run they replay"
    );
}

#[test]
fn a_manual_prefix_rebuilds_from_its_own_records() {
    // A person answered these, so the records carry their provenance; the rebuild replays them as
    // prefix choices, and the frames must still come out identical.
    let all = ["seat0", "seat1", "seat2", "seat3", "seat4", "seat5"];
    let source = record(20, &all);
    assert!(
        source
            .records
            .iter()
            .any(|record| record.provenance == ti4_replayer::Provenance::Human),
        "a manual recording must contain human answers"
    );
    let target = (source.frames.len() - 1) as u64;
    let replayed = rebuild_and_prove(
        &source,
        source.records.clone(),
        RebuildTarget::Frame(target),
    );
    assert!(
        replayed > 0,
        "the manual prefix should have replayed something"
    );
}
