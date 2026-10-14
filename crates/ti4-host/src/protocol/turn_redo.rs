//! Data transfer objects for TURN REDO (H7): a casual-play "replay my last turn".
//!
//! The game rewinds to the start of the seat's turn (same fixed seed), the seat plays a new turn
//! live, then the rest of the round auto-plays from the other seats' recorded decisions until it
//! reaches the start of the redoing seat's next turn or the first decision that no longer fits.
//! The timeline as it was before the redo is kept as an alternate that can be restored.

use serde::{Deserialize, Serialize};

use crate::protocol::splice::ConflictKind;

/// The one body of `POST /api/games/{id}/turn-redo`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TurnRedoRequest {
    pub expected_version: u64,
    #[serde(flatten)]
    pub command: TurnRedoCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TurnRedoCommand {
    /// Rewind to the start of the seat's last `turns` turns (1 or 2; default 1).
    /// The seat defaults to the requester; naming another seat is host-only.
    Request {
        #[serde(default)]
        seat: Option<String>,
        #[serde(default)]
        turns: Option<u8>,
    },
    /// Replay the other seats' recorded decisions after the new turn (the web client sends this
    /// by itself as soon as the new turn is complete).
    Autoplay,
    /// Bring the timeline from before the redo back, exactly.
    Restore,
    /// Keep the new timeline and drop the saved original.
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRedoStage {
    /// Rewound; the seat is playing the new turn.
    NewTurn,
    /// The rest of the round was auto-played; see the outcome for where it stopped.
    AutoPlayed,
}

/// Why auto-play stopped. No decision of an absent seat is ever guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnRedoStop {
    /// Reached the start of the redoing seat's next turn, which is played live.
    Handoff { seat: String },
    /// A recorded decision no longer fits; the seat the engine now asks decides live.
    Conflict { conflict: TurnRedoConflict },
    /// Every recorded decision after the redone turn fit (the rest of the original history).
    TailExhausted,
}

/// The first recorded decision that no longer fits. Deliberately terse: it names the kind, the
/// seat and the prompt, not the recorded choice or the menu it was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRedoConflict {
    /// Index in the original (pre-redo) history of the decision that did not fit.
    pub original_cursor: usize,
    pub kind: ConflictKind,
    /// The seat that was asked (it decides live now).
    pub seat: String,
    pub prompt: String,
    pub detail: String,
    /// For `reserved_card`: the deck (`action_card`, `secret`, `exploration:<kind>`, ...), the
    /// card id that was reserved for the seat and that seat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipient: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRedoOutcome {
    /// Recorded decisions of other seats that were replayed and kept.
    pub kept: usize,
    /// How many recorded decisions followed the redone turn in the original timeline.
    pub tail_total: usize,
    pub stop: TurnRedoStop,
    /// The seat the engine asks right now, if known.
    pub asking_seat: Option<String>,
}

/// What a client needs to draw the redo UI. `None` in the response means no redo is in flight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRedoStatus {
    /// The seat whose turn is being redone.
    pub seat: String,
    pub requested_by: String,
    /// How many of the seat's turns were rewound (1 or 2).
    pub turns_back: u8,
    /// Redo requests chained onto the same saved original (a second redo keeps the first original).
    pub redo_count: u32,
    pub stage: TurnRedoStage,
    /// Decisions in the saved original timeline.
    pub original_decisions: usize,
    /// Decisions in the live history right after the rewind point (where the new turn begins).
    pub rewound_to: usize,
    /// `new_turn` only: the seat's new turn is complete, so auto-play can run now.
    pub turn_complete: bool,
    /// `auto_played` only: the live history length right after auto-play. The original stays
    /// restorable until a live decision is recorded past this length.
    pub handoff_len: Option<usize>,
    pub outcome: Option<TurnRedoOutcome>,
    /// The viewer is the host or the redoing seat, so may auto-play, restore and keep.
    pub can_control: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRedoStatusResponse {
    pub status: Option<TurnRedoStatus>,
}
