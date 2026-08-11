//! Scoring stub.

use ti4_model::*;
use ti4_model::view::BotView;
use std::collections::HashMap;

pub struct Scorer;

impl Scorer {
    pub fn new() -> Self { Self }

    pub fn score(&self, _view: &BotView) -> Result<HashMap<PlayerId, f64>, anyhow::Error> {
        todo!("M08: implement scoring")
    }
}
