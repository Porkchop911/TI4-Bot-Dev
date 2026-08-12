//! Map generation stub.

pub struct MapGenerator;

impl Default for MapGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl MapGenerator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn generate(&self, _seed: u64) -> Result<(), anyhow::Error> {
        todo!("M10: implement map generation")
    }
}
