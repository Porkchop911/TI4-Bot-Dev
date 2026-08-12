//! Audit stub.

pub struct Audit;

impl Default for Audit {
    fn default() -> Self {
        Self::new()
    }
}

impl Audit {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement audit")
    }
}
