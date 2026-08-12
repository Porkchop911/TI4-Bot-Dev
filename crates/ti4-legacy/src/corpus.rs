//! Corpus stub.

pub struct Corpus;

impl Default for Corpus {
    fn default() -> Self {
        Self::new()
    }
}

impl Corpus {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M12: implement corpus")
    }
}
