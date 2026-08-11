//! Batch simulation stub.

use ti4_model::*;

pub struct SimulationBatch;

impl SimulationBatch {
    pub fn new() -> Self { Self }

    pub fn run(&self, _count: i32) -> Result<(), anyhow::Error> {
        todo!("M10: implement batch simulation")
    }
}
