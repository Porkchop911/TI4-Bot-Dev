//! Map generation stub.

pub struct MapGenerator;

impl MapGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, _seed: u64) -> Result<(), anyhow::Error> {
        todo!("M10: implement map generation")
    }
}
