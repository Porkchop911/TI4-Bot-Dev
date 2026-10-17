//! Who sits in each seat of a running game. The table is set up in `ti4-host` (`setup`); the
//! update of a game carries its seats, so they are here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_model::id::PlayerId;

/// Who sits in a seat of a running game. It is public: a host tells it to every viewer with the
/// game, so a page names a seat by its player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatFact {
    /// The name of the player at the table.
    pub nickname: String,
    /// Who plays the seat: `human`, or a kind of `ti4_host::setup::bot_kind`.
    pub kind: String,
}

/// The kind of a seat that a person plays ([`SeatFact::kind`]).
pub const HUMAN_KIND: &str = "human";

/// The seats of a running game by the id of their player ([`SeatFact`]).
pub type TableSeats = BTreeMap<PlayerId, SeatFact>;
