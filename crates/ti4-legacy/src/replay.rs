//! Replay stub.

pub struct LegacyReplay;

impl LegacyReplay {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M12: implement replay")
    }
}
