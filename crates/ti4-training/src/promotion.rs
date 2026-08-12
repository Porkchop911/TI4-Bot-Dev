//! Promotion stub.

pub struct Promotion;

impl Default for Promotion {
    fn default() -> Self {
        Self::new()
    }
}

impl Promotion {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement promotion")
    }
}
