//! Stage 1 training stub.

pub struct Stage1Training;

impl Default for Stage1Training {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage1Training {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement Stage 1 training")
    }
}
