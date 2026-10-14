//! Deterministic reconstruction: rebuild a branch by replaying it, never by installing a snapshot.
//!
//! A frame's `state` is a full `GameState`, and putting one into a running `Game` is not possible
//! even when the fields line up — `Game` also carries open transaction windows, `prepared_turn_seq`,
//! the event sequence and the galaxy, none of which a frame records faithfully enough to restore. So
//! "play from this frame" means *replaying* to it: rebuild from the same checkpoint, map pool, seed,
//! rotation, profile table and temperature, and repeat the exact engine-step and decision prefix.
//!
//! Two rules make the replay land where the original did:
//!
//! 1. **The policy is invoked once per prefix choice and its answer is thrown away.** Sampling
//!    advances the policy's RNG, so a prefix that skipped the call would leave later Auto play
//!    drawing from a different stream than the game being branched. The engine only ever sees the
//!    recorded answer.
//! 2. **Every frame is checked, not trusted.** Each rebuilt frame is digested with
//!    [`crate::fingerprint::FrameFingerprint`] and compared against the recorded digest before the
//!    branch is considered usable. A mismatch, a prefix that does not match what the engine is asking
//!    for, a policy failure underneath a replayed choice, an engine error frame, a bound or a
//!    cancellation all produce a typed [`RebuildError`] and **no** playable branch.
//!
//! The prefix itself is [`ReplayRecord`]s (R02-001), the authoritative record of settled decisions —
//! never the reviewer's decision trace, which also carries synthetic fleet-package and plan-stop rows
//! that were never engine decisions.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use ti4_engine::choice::Choice;
use ti4_model::id::PlayerId;
use ti4_review::{
    AdvanceUnit, LiveReview, MAX_COMMAND_STEPS, MAX_FRAMES, ReviewFrame, SimulationConfig,
};

use crate::control::{ChoiceFingerprint, ReplayRecord, SeatControl};
use crate::fingerprint::{FrameFingerprint, first_difference};
use crate::live::Gate;

/// How far a rebuild must go before it is ready for play.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebuildTarget {
    /// Reproduce frames `0..=index`; frame 0 is a legitimate branch point.
    Frame(u64),
    /// Replay every recorded frame, to wherever the original got to.
    End,
}

impl RebuildTarget {
    #[must_use]
    pub const fn frame_index(self) -> Option<u64> {
        match self {
            Self::Frame(index) => Some(index),
            Self::End => None,
        }
    }
}

/// The limits a rebuild may not exceed. Inherited from the reviewer unless overridden.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RebuildBounds {
    /// Engine steps before the rebuild is refused.
    pub max_steps: usize,
    /// Frames before the rebuild is refused.
    pub max_frames: usize,
}

impl Default for RebuildBounds {
    fn default() -> Self {
        Self {
            max_steps: MAX_COMMAND_STEPS,
            max_frames: MAX_FRAMES,
        }
    }
}

/// What the prefix said about one ask, and what to do about it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PrefixAnswer {
    /// Answer with this recorded option, invoking the policy once underneath and discarding that.
    Replay { option_id: String },
    /// The prefix is over; ordinary Auto/Manual control resumes.
    Done,
    /// The prefix does not match what the engine is asking for.
    Mismatch(Mismatch),
}

/// Which part of the prefix disagreed with the engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MismatchKind {
    /// The engine asked a different seat than the record names.
    Actor,
    /// Same seat, different question.
    Prompt,
    /// The ordered offered ids differ.
    Options,
    /// The typed decision context differs.
    Context,
    /// The recorded answer is not on offer.
    Chosen,
    /// Everything above looked right but the identity digest differs.
    Identity,
}

impl std::fmt::Display for MismatchKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Actor => "actor",
            Self::Prompt => "prompt",
            Self::Options => "options",
            Self::Context => "context",
            Self::Chosen => "chosen",
            Self::Identity => "identity",
        })
    }
}

/// A prefix choice that is not the choice the engine is asking about.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Mismatch {
    /// Which record in the script (0-based).
    pub index: usize,
    pub kind: MismatchKind,
    /// Frame the rebuilt engine was on when it disagreed.
    pub frame: u64,
    /// Which ask within that frame.
    pub ask: u32,
    pub actor: PlayerId,
    pub detail: String,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "prefix record {} (frame {}, ask {}) for {}: {:?} — {}",
            self.index, self.frame, self.ask, self.actor, self.kind, self.detail
        )
    }
}

/// The recorded decisions a rebuild must reproduce, in the order the engine asked them.
#[derive(Clone, Debug, Default)]
pub struct ReplayScript {
    records: Vec<ReplayRecord>,
    next: usize,
}

impl ReplayScript {
    #[must_use]
    pub fn new(records: Vec<ReplayRecord>) -> Self {
        Self { records, next: 0 }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// How many prefix choices have been replayed.
    #[must_use]
    pub fn applied(&self) -> usize {
        self.next
    }

    #[must_use]
    pub fn finished(&self) -> bool {
        self.next >= self.records.len()
    }

    #[must_use]
    pub fn records(&self) -> &[ReplayRecord] {
        &self.records
    }

    /// Validate the engine's current ask against the next record.
    ///
    /// Checks run from the coarse identity down so the diagnostic names the field a human should
    /// look at, and the digest is checked last as the catch-all: actor, prompt, ordered option ids,
    /// typed context, then the recorded answer being on offer, then the fingerprint.
    pub(crate) fn answer_for(&mut self, seat: &PlayerId, choice: &Choice) -> PrefixAnswer {
        let Some(index) = self
            .next
            .checked_mul(1)
            .filter(|_| self.next < self.records.len())
        else {
            return PrefixAnswer::Done;
        };
        let record = &self.records[index];
        let mismatch = |kind: MismatchKind, detail: String| -> PrefixAnswer {
            PrefixAnswer::Mismatch(Mismatch {
                index,
                kind,
                frame: record.frame,
                ask: record.ask,
                actor: record.actor.clone(),
                detail,
            })
        };
        if &record.actor != seat {
            return mismatch(
                MismatchKind::Actor,
                format!("engine asked {seat}, record says {}", record.actor),
            );
        }
        if record.prompt != choice.prompt {
            return mismatch(
                MismatchKind::Prompt,
                format!("{:?} vs {:?}", short(&record.prompt), short(&choice.prompt)),
            );
        }
        let offered: Vec<String> = choice.ids().into_iter().map(str::to_owned).collect();
        if record.offered != offered {
            return mismatch(
                MismatchKind::Options,
                format!(
                    "{} option(s) recorded, {} offered: {:?} vs {:?}",
                    record.offered.len(),
                    offered.len(),
                    head(&record.offered),
                    head(&offered),
                ),
            );
        }
        let context = choice
            .context
            .as_ref()
            // Display-only fields (the reaction trigger) are not part of what a replay binds.
            .and_then(|context| serde_json::to_value(context.without_display_fields()).ok());
        if record.context != context {
            return mismatch(
                MismatchKind::Context,
                format!(
                    "recorded {} vs offered {}",
                    describe(record.context.as_ref()),
                    describe(context.as_ref())
                ),
            );
        }
        if choice.option(&record.chosen).is_none() {
            return mismatch(
                MismatchKind::Chosen,
                format!("{:?} is not on offer", record.chosen),
            );
        }
        let fingerprint = ChoiceFingerprint::from_choice(choice);
        if fingerprint != record.fingerprint {
            return mismatch(
                MismatchKind::Identity,
                format!(
                    "{} vs {}",
                    fingerprint.as_hex(),
                    record.fingerprint.as_hex()
                ),
            );
        }
        self.next += 1;
        PrefixAnswer::Replay {
            option_id: record.chosen.clone(),
        }
    }
}

/// Why a rebuild refused to produce a branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RebuildError {
    /// The prefix did not match what the engine asked. No branch is produced.
    Diverged(Mismatch),
    /// A rebuilt frame is not the frame it reproduces.
    FrameMismatch { index: usize, detail: String },
    /// The policy underneath a replayed choice failed, so the RNG could not be realigned.
    PolicyFailed {
        frame: u64,
        actor: PlayerId,
        detail: String,
    },
    /// The engine recorded an error on a replayed frame.
    EngineFailed { index: usize, error: String },
    /// The target frame does not exist in the recorded run.
    TargetUnavailable { wanted: u64, recorded: usize },
    /// A limit was hit; a partial rebuild is not returned.
    BoundsExceeded {
        steps: usize,
        frames: usize,
        max_steps: usize,
        max_frames: usize,
    },
    /// Cancelled between steps.
    Cancelled,
    /// The reviewer could not be started at all.
    Setup(String),
}

impl std::fmt::Display for RebuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Diverged(mismatch) => write!(formatter, "rebuild diverged: {mismatch}"),
            Self::FrameMismatch { index, detail } => {
                write!(formatter, "rebuilt frame {index} differs: {detail}")
            }
            Self::PolicyFailed {
                frame,
                actor,
                detail,
            } => {
                write!(
                    formatter,
                    "policy failed replaying for {actor} at frame {frame}: {detail}"
                )
            }
            Self::EngineFailed { index, error } => {
                write!(formatter, "engine error on rebuilt frame {index}: {error}")
            }
            Self::TargetUnavailable { wanted, recorded } => write!(
                formatter,
                "target frame {wanted} is beyond the {recorded} recorded frame(s)"
            ),
            Self::BoundsExceeded {
                steps,
                frames,
                max_steps,
                max_frames,
            } => write!(
                formatter,
                "rebuild hit its bounds after {steps} step(s) and {frames} frame(s) \
                 (max {max_steps} steps, {max_frames} frames)"
            ),
            Self::Cancelled => write!(formatter, "rebuild cancelled"),
            Self::Setup(reason) => write!(formatter, "rebuild could not start: {reason}"),
        }
    }
}

impl std::error::Error for RebuildError {}

/// A rebuild that reproduced its target and may be played.
pub struct Rebuilt {
    /// The reviewer, parked exactly on the target frame. Not `Send`: use it on the thread that
    /// rebuilt it, which is what a live branch does.
    pub review: LiveReview,
    /// Frames reproduced, including frame 0.
    pub frames: usize,
    /// Prefix choices replayed.
    pub replayed: usize,
    /// Engine steps taken.
    pub steps: usize,
    /// How many times the policy was invoked underneath the replayed choices.
    pub policy_calls: usize,
}

/// Rebuild a branch on the calling thread.
///
/// `recorded` holds the fingerprint of every recorded frame, index by index; only the indices it
/// covers are checked, so a caller may hand over the frames it has. `cancel` is polled between
/// steps, which is the smallest unit the plan allows for a cancellable rebuild.
///
/// # Errors
/// Any variant of [`RebuildError`]. On error nothing is returned: a partial branch is the failure
/// mode this function exists to prevent.
pub fn rebuild(
    config: &SimulationConfig,
    seats: SeatControl,
    script: ReplayScript,
    recorded: &[FrameFingerprint],
    target: RebuildTarget,
    bounds: RebuildBounds,
    cancel: &AtomicBool,
) -> Result<Rebuilt, RebuildError> {
    let gate = Arc::new(Gate::replaying(seats, script));
    rebuild_with_gate(&gate, config, recorded, target, bounds, &|| {
        cancel.load(Ordering::Relaxed)
    })
}

/// Rebuild on a gate the caller already owns.
///
/// This is the whole of [`rebuild`] with the gate handed in, and the reason it exists is that a live
/// branch is one thread that must not stop for a conversation with the window: the branch that
/// replays a prefix is the branch that then plays on, because the `LiveReview` it produces is not
/// `Send` and cannot be handed to a second thread. So the replayer builds the gate first, rebuilds
/// through it here, and keeps driving the same review through the same gate - taking over from frame
/// N with the seats the operator chose, on the policy stream the prefix left aligned.
///
/// The gate must be one built by [`Gate::replaying`] for the script being reproduced. Cancellation is
/// a predicate rather than a flag so that a branch can cancel on shutdown as well as on request.
///
/// # Errors
/// As [`rebuild`].
pub fn rebuild_with_gate(
    gate: &Arc<Gate>,
    config: &SimulationConfig,
    recorded: &[FrameFingerprint],
    target: RebuildTarget,
    bounds: RebuildBounds,
    cancel: &(dyn Fn() -> bool + Sync),
) -> Result<Rebuilt, RebuildError> {
    let hook = |seat: &PlayerId,
                policy: Box<dyn ti4_engine::choice::Decider>|
     -> Box<dyn ti4_engine::choice::Decider> {
        Box::new(crate::decider::ControlledDecider::new(
            policy,
            seat.clone(),
            Arc::clone(gate),
        ))
    };
    let mut review = LiveReview::start_with_control(config, &hook)
        .map_err(|error| RebuildError::Setup(error.to_string()))?;

    let wanted = recorded.len();
    if let Some(index) = target.frame_index()
        && index >= wanted as u64
    {
        return Err(RebuildError::TargetUnavailable {
            wanted: index,
            recorded: wanted,
        });
    }

    let mut steps = 0_usize;
    let mut validated = 0_usize;
    loop {
        if cancel() {
            return Err(RebuildError::Cancelled);
        }
        let frames = review.session.frames.len();
        if frames > bounds.max_frames || steps > bounds.max_steps {
            return Err(RebuildError::BoundsExceeded {
                steps,
                frames,
                max_steps: bounds.max_steps,
                max_frames: bounds.max_frames,
            });
        }
        // Every frame produced so far is checked before anything else can depend on it.
        for frame in &review.session.frames[validated..] {
            if let Some(error) = frame.error.as_ref() {
                return Err(RebuildError::EngineFailed {
                    index: frame.index,
                    error: error.clone(),
                });
            }
            if let Some(expected) = recorded.get(frame.index) {
                let actual = FrameFingerprint::of(frame);
                if &actual != expected {
                    return Err(RebuildError::FrameMismatch {
                        index: frame.index,
                        detail: recorded_frame_difference(frame, expected),
                    });
                }
            }
        }
        validated = frames;

        let last_index = frames.saturating_sub(1) as u64;
        let done = match target {
            RebuildTarget::Frame(index) => last_index >= index,
            RebuildTarget::End => last_index + 1 >= wanted as u64 || review.is_terminal(),
        };
        if done {
            break;
        }
        gate.begin_frame(frames as u64);
        review.advance(AdvanceUnit::Step, 1);
        steps += 1;

        if let Some(mismatch) = gate.divergence() {
            return Err(RebuildError::Diverged(mismatch));
        }
        if let Some(failure) = gate.policy_failure() {
            return Err(failure);
        }
    }

    let frames = review.session.frames.len();
    Ok(Rebuilt {
        review,
        frames,
        replayed: gate.replayed(),
        steps,
        policy_calls: gate.policy_calls(),
    })
}

/// A rebuilt frame that does not match its recorded digest.
///
/// The original frame is not available here — only its digest — so the detail is the two digests plus
/// the rebuilt frame's own coarse identity. When the caller can supply both frames,
/// [`crate::fingerprint::first_difference`] names the field.
fn recorded_frame_difference(frame: &ReviewFrame, expected: &FrameFingerprint) -> String {
    let actual = FrameFingerprint::of(frame);
    format!(
        "rebuilt {} vs recorded {} (engine step {}, round {}, phase {:?}, active {:?}, {} decision(s))",
        actual.as_hex(),
        expected.as_hex(),
        frame.engine_step,
        frame.round,
        frame.phase,
        frame.active,
        frame.decision_count,
    )
}

/// Same as [`rebuild`], but also compares two frames field by field when they disagree, for callers
/// that still hold the original frames.
#[must_use]
pub fn describe_frame_difference(rebuilt: &ReviewFrame, original: &ReviewFrame) -> Option<String> {
    first_difference(rebuilt, original)
}

fn short(text: &str) -> String {
    let mut out: String = text.chars().take(60).collect();
    if out.chars().count() < text.chars().count() {
        out.push('…');
    }
    out
}

fn head(ids: &[String]) -> Vec<&str> {
    ids.iter().map(String::as_str).take(4).collect()
}

fn describe(context: Option<&serde_json::Value>) -> String {
    match context {
        None => "no context".to_owned(),
        Some(value) => {
            let text = value.to_string();
            format!("context of {} byte(s)", text.len())
        }
    }
}
