//! Feature extraction stub.

use ti4_model::view::BotView;

pub struct FeatureExtractor;

impl FeatureExtractor {
    pub fn new() -> Self { Self }

    pub fn extract(&self, _view: &BotView) -> Result<Vec<f64>, anyhow::Error> {
        todo!("M08-M09: implement feature extraction")
    }
}
