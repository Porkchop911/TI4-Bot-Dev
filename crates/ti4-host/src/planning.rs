//! The data of a player's plan and of what the planner publishes about it.
//!
//! The adapters and the checks of the publication boundary are defined in `ti4-view`. The
//! thread that runs a plan against a disposable engine fork (`PlanningRunner`) is the server's;
//! these are the plain types it publishes and that the wire carries.

use serde::{Deserialize, Serialize};
use ti4_view::planning::audit::{SafePublication, StopReason};

pub use ti4_view::planning::audit::{ASSUMPTION, PlanScope, SECONDARY_ASSUMPTION};
pub use ti4_view::planning::{
    DecisionRecording, RecordedDecision, RecordingDecider, ReplayDecider, ReplayProgress,
    ReplayStatus, ReplayStopReason,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerPlan {
    pub revision: u64,
    /// Explicit script replacement, preserved across checkpoint refresh and recovery.
    #[serde(default)]
    pub reset_revision: u64,
    /// The original movement answers remain the editable draft until a fresh
    /// movement answer replaces them. Refresh/recovery reopens the same editor.
    #[serde(default)]
    pub editing_movement: bool,
    #[serde(default)]
    pub movement_edit_revision: u64,
    /// Start of the retained tail awaiting semantic revalidation.
    #[serde(default)]
    pub revalidation_start: Option<usize>,
    #[serde(default)]
    pub replacing_movement: bool,
    pub base_checkpoint_id: u64,
    pub recorded_decisions: Vec<RecordedDecision>,
    /// Confirmation receipts survive refresh and session restoration with the script.
    #[serde(default)]
    pub recorded_request_ids: Vec<String>,
}

impl PlayerPlan {
    #[must_use]
    pub const fn new(base_checkpoint_id: u64) -> Self {
        Self {
            revision: 0,
            reset_revision: 0,
            editing_movement: false,
            movement_edit_revision: 0,
            revalidation_start: None,
            replacing_movement: false,
            base_checkpoint_id,
            recorded_decisions: Vec::new(),
            recorded_request_ids: Vec::new(),
        }
    }

    pub fn movement_start(&self) -> Option<usize> {
        self.recorded_decisions.iter().position(|decision| {
            decision
                .context
                .as_ref()
                .is_some_and(|context| context.subtype == "movement_step")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptIdentity {
    pub checkpoint_id: u64,
    pub plan_revision: u64,
    pub generation_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    pub recorded_answers: usize,
    pub replayed: usize,
    pub remaining: usize,
    pub completed_steps: usize,
    pub nested_answers_since_checkpoint: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureCategory {
    Preparation,
    Engine,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanningUpdate {
    Preparing,
    SafeOffer(SafePublication),
    SafeStep(SafePublication),
    Stopped {
        reason: StopReason,
        last_safe_publication: Option<SafePublication>,
    },
    Failed(FailureCategory),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningEnvelope {
    pub publication_id: u64,
    pub identity: AttemptIdentity,
    #[serde(default)]
    pub reset_revision: u64,
    #[serde(default)]
    pub editing_movement: bool,
    #[serde(default)]
    pub movement_edit_revision: u64,
    /// Replay offers are informative; only a waiting offer accepts an answer.
    pub awaiting_answer: bool,
    pub assumptions: Vec<String>,
    pub progress: Progress,
    pub recorded_request_ids: Vec<String>,
    #[serde(default)]
    pub recorded_decisions: Vec<RecordedDecision>,
    pub update: PlanningUpdate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionError {
    Retired,
    NotWaiting,
    UnknownOption,
}

