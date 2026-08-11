//! Replay stub.

use ti4_model::*;

pub struct Replay;

impl Replay {
    pub fn new() -> Self { Self }

    pub fn record(&self, _event: &EventRecord) -> Result<(), anyhow::Error> {
        todo!("M10: implement replay recording")
    }

    pub fn replay(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement replay playback")
    }
}
