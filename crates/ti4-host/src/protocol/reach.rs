//! The "who can go here" query (system overview): which ships on the board could move into a
//! system if it were activated now. Read-only and ephemeral: nothing is persisted, logged,
//! broadcast or added to snapshots, and the answer goes only to the connection that asked.
//!
//! Older servers answer the unknown `query_reach` message with a malformed message error (the
//! protocol version is not bumped, as for the other additive messages), which a client treats as
//! "not available".

use serde::{Deserialize, Serialize};
use ti4_engine::reach_query::ReachOrigin;

/// Longest system id a `query_reach` may carry.
pub const MAX_SYSTEM_ID_BYTES: usize = 32;

/// Why a reach query was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ReachRefusal {
    /// The session is replaying, finished or failed, or has no map to search.
    Busy,
    /// The system is not on this game's map.
    UnknownSystem,
    /// Too many requests on this connection.
    RateLimited,
}

/// The answer to a `query_reach`: ships that could move into `system_id`, "as of now".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReachIntoMsg {
    pub protocol_version: u16,
    pub game_id: String,
    pub request_id: u64,
    pub system_id: String,
    /// The game version of the position the answer was computed from. Not a stale-version
    /// envelope field: the client compares it with the version it shows.
    pub as_of_version: u64,
    /// Ships grouped by owner seat (seating order) and origin system. Empty when refused.
    pub origins: Vec<ReachOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<ReachRefusal>,
}
