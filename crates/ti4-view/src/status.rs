//! Turn status, viewer roles, and decision rejection reasons.

use serde::{Deserialize, Serialize};
use ti4_model::id::PlayerId;
use ti4_model::state::Phase;

/// Viewer role determining redaction boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "role", content = "seat", rename_all = "snake_case")]
pub enum ViewerRole {
    /// An authenticated player occupying a specific table seat.
    Player(PlayerId),
    /// A spectator with public-only visibility.
    Spectator,
}

impl ViewerRole {
    /// Returns true if this viewer is the specified acting player.
    #[must_use]
    pub fn is_actor(&self, actor: &PlayerId) -> bool {
        match self {
            Self::Player(p) => p == actor,
            Self::Spectator => false,
        }
    }

    /// Returns the player seat ID if this is a player role.
    #[must_use]
    pub fn seat(&self) -> Option<&PlayerId> {
        match self {
            Self::Player(p) => Some(p),
            Self::Spectator => None,
        }
    }
}

/// Public turn status visible to all participants and spectators.
///
/// Crucially, this communicates which seat is active or making a decision without disclosing
/// private card identities, private reaction windows, or legal options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicTurnStatus {
    /// Normal active turn in the action phase.
    ActiveTurn {
        player: PlayerId,
        phase: Phase,
        round: u32,
    },
    /// A decision is currently pending from a seat.
    ///
    /// `stage` is a generalized description (e.g. "Action Phase", "Tactical Decision", "Agenda Vote", "Timing Window").
    /// It never contains private card names or actor-only options.
    WaitingForDecision {
        seat: PlayerId,
        phase: Phase,
        round: u32,
        stage: String,
    },
    /// A reaction window is open (or looks open). Names no seat: who holds a playable action card
    /// is private, so everybody but the asked seat sees this, and a bluff hold sends the same.
    WaitingForReactions { phase: Phase, round: u32 },
    /// Phase or round transition (e.g. status phase cleanup, strategy card dealing).
    PhaseTransition { phase: Phase, round: u32 },
    /// Terminal game state.
    GameOver { winner: Option<PlayerId> },
}

/// Structured explanation when a choice submission is rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case", deny_unknown_fields)]
pub enum RejectionReason {
    /// State version mismatch between client expectation and server state.
    StaleVersion { expected: u64, current: u64 },
    /// Nonce does not match currently pending choice.
    StaleNonce,
    /// Client is not authenticated for the acting seat.
    UnauthorizedSeat { seat: Option<PlayerId> },
    /// No decision is currently awaiting an answer.
    NoPendingChoice,
    /// Submitted option ID is not in the legal offered options.
    UnknownOption { option_id: String },
    /// Engine validation refused the choice.
    ValidationFailed { message: String },
    /// The seat may not take back its last decision: it lies behind this barrier.
    UndoBarrier { barrier: UndoBarrier },
    /// Another connection of the seat answered this choice already, or the server sent a ready
    /// draft of the seat for it. Sent only to a connection that asked for the staged work of its
    /// seat (`Subscribe::staged`); every other one is told `StaleNonce`. The update that follows
    /// shows the answer.
    AnsweredElsewhere,
}

/// What a seat cannot undo past: after it, the seat knows something that it did not know
/// when it decided, or the game is no longer its own to take back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoBarrier {
    /// Something hidden was shown: a card that was drawn, an exploration, an objective.
    Revealed,
    /// A die was rolled or a deck was shuffled.
    Random,
    /// Another seat decided.
    OtherSeat,
}
