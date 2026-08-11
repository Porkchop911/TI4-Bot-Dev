//! Archive stub.

pub struct Archive;

impl Archive {
    pub fn new() -> Self { Self }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement archive save")
    }

    pub fn load(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement archive load")
    }
}
