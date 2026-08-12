//! Batch simulation stub.

pub struct SimulationBatch;

impl Default for SimulationBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationBatch {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self, _count: i32) -> Result<(), anyhow::Error> {
        todo!("M10: implement batch simulation")
    }
}
