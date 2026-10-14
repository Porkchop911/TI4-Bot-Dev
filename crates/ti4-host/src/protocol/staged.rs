//! The wire types of the staged work of a seat: what a seat has staged and not sent (the staged
//! movement, landings and production, the private draft of a tactical action, and the draft of a
//! secondary). The server keeps it for the seat, so every tab and every device of the seat has
//! the same staged work; the state and its persistence are the server's
//! (`ti4_server::session::staged`). The values are the client's; the server stores and forwards
//! them, and reads only a draft of a secondary that is ready, to send it when the game asks the
//! seat (`ti4_server::session::ready_secondary`).
//!
//! Revisions count up for the whole seat, so the newest message of a seat is the one with the
//! highest `revision`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The largest value of one slot of `PutStaged`, as JSON: room for a draft with its undo
/// history, under the size of a whole message.
pub const MAX_STAGED_VALUE_BYTES: usize = 48 * 1024;

/// One kind of staged work of a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagedSlot {
    /// The staged movement, landings or production of the open step.
    Movement,
    /// The private draft of a tactical action, with its undo history.
    Plan,
    /// The draft of a secondary.
    Secondary,
}

/// The value of one slot and the revision of the seat when it was written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedValue {
    pub revision: u64,
    /// `None` when the slot was cleared.
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

/// What the seat of a connection has staged. Sent only to the connections of that seat that
/// asked for it (`ClientMessage::Subscribe::staged`): after the snapshot, after every write of
/// any of them, and to a connection whose write was stale. Carries no game version on purpose: a
/// stale-version filter must never drop it; the newest message is the one with the highest
/// `revision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatStagedMsg {
    pub protocol_version: u16,
    pub game_id: String,
    /// The revision of the newest write of any slot (0: never written).
    pub revision: u64,
    /// The slots that were ever written; a cleared slot has no value.
    pub slots: BTreeMap<StagedSlot, StagedValue>,
    /// The connection whose write this is, so that it can skip its own echo. Absent in the
    /// message after the snapshot and in the answer to a stale write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer: Option<u64>,
    /// The id of the connection that is sent this message, in the message after its snapshot
    /// only: the `writer` of its own writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<u64>,
}

impl SeatStagedMsg {
    /// The staged work of a seat, with the newest `revision` of any slot, without `writer` or
    /// `connection`.
    #[must_use]
    pub fn new(game_id: &str, revision: u64, slots: BTreeMap<StagedSlot, StagedValue>) -> Self {
        Self {
            protocol_version: super::PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            revision,
            slots,
            writer: None,
            connection: None,
        }
    }
}
