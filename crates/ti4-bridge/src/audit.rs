//! Audit stub.

pub struct Audit;

impl Audit {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement audit")
    }
}
