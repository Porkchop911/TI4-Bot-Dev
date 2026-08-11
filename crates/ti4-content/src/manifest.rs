//! Content manifest stub.

pub struct ContentManifest;

impl ContentManifest {
    pub fn new() -> Self { Self }

    pub fn load(&self) -> Result<(), anyhow::Error> {
        todo!("M02: implement manifest loading")
    }
}
