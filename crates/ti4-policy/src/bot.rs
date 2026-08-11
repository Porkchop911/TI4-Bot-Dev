//! Bot policy stub.

use ti4_model::view::BotView;

pub struct BotPolicy;

impl BotPolicy {
    pub fn new() -> Self { Self }

    pub fn evaluate(&self, _view: &BotView) -> Result<f64, anyhow::Error> {
        todo!("M08: implement bot policy evaluation")
    }
}
