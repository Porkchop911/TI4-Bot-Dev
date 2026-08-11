//! Bot policy stub.
//!
//! A bot is handed a redacted [`GameState`] — the oracle's `views.view_for` — and asks the
//! real engine to enumerate its legal options, because legality depends on facts the bot is
//! not entitled to compute for itself.

use ti4_model::state::GameState;

pub struct BotPolicy;

impl BotPolicy {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Score a position from the viewpoint the redacted state was built for.
    ///
    /// # Errors
    /// Not implemented yet.
    pub fn evaluate(&self, _view: &GameState) -> Result<f64, anyhow::Error> {
        todo!("M08: implement bot policy evaluation")
    }
}

impl Default for BotPolicy {
    fn default() -> Self {
        Self::new()
    }
}
