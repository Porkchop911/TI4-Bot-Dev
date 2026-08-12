//! Rotation stub.

pub struct Rotation;

impl Default for Rotation {
    fn default() -> Self {
        Self::new()
    }
}

impl Rotation {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement rotation")
    }
}
