//! Feature extraction stub.

use ti4_model::state::GameState;

pub struct FeatureExtractor;

impl FeatureExtractor {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not implemented yet.
    pub fn extract(&self, _view: &GameState) -> Result<Vec<f64>, anyhow::Error> {
        todo!("M08-M09: implement feature extraction")
    }
}

impl Default for FeatureExtractor {
    fn default() -> Self {
        Self::new()
    }
}
