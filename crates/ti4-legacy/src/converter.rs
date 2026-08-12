//! Converter stub.

pub struct Converter;

impl Default for Converter {
    fn default() -> Self {
        Self::new()
    }
}

impl Converter {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M12: implement converter")
    }
}
