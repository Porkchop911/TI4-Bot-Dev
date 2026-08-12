//! Archive stub.

pub struct Archive;

impl Default for Archive {
    fn default() -> Self {
        Self::new()
    }
}

impl Archive {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn save(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement archive save")
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn load(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement archive load")
    }
}
