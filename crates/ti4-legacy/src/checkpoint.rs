//! Checkpoint stub.

pub struct Checkpoint;

impl Checkpoint {
    pub fn new() -> Self { Self }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M12: implement checkpoint")
    }
}
