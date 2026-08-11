//! Scoring stub.

use std::collections::BTreeMap;

use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

pub struct Scorer;

impl Scorer {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not implemented yet.
    pub fn score(&self, _view: &GameState) -> Result<BTreeMap<PlayerId, f64>, anyhow::Error> {
        todo!("M08: implement scoring")
    }
}

impl Default for Scorer {
    fn default() -> Self {
        Self::new()
    }
}
