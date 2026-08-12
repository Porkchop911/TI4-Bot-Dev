//! Checkpoint stub.

pub struct Checkpoint;

impl Default for Checkpoint {
    fn default() -> Self {
        Self::new()
    }
}

impl Checkpoint {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M12: implement checkpoint")
    }
}
