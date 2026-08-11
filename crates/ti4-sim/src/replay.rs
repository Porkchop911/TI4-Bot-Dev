//! Replay stub.
//!
//! Replay in the oracle is driven by a *decision* log, not an event log: replaying a game
//! means feeding the same answers back to the same choice points. The entry type belongs to
//! M03-005 (`DecisionLog`) and does not exist yet, so this stub does not name one — an
//! invented type here is how the previous engine ended up modelling a different game.

pub struct Replay;

impl Replay {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not implemented yet.
    pub fn record(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement replay recording, on top of the M03-005 decision log")
    }

    /// # Errors
    /// Not implemented yet.
    pub fn replay(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement replay playback")
    }
}

impl Default for Replay {
    fn default() -> Self {
        Self::new()
    }
}
