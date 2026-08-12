//! Import stub.

pub struct Import;

impl Default for Import {
    fn default() -> Self {
        Self::new()
    }
}

impl Import {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement import")
    }
}
