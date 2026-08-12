//! Capture stub.

pub struct Capture;

impl Default for Capture {
    fn default() -> Self {
        Self::new()
    }
}

impl Capture {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement capture")
    }
}
