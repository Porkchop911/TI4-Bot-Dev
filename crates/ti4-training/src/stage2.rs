//! Stage 2 training stub.

pub struct Stage2Training;

impl Default for Stage2Training {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage2Training {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement Stage 2 training")
    }
}
