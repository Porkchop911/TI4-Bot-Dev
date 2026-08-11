//! Rules stub.

pub struct Rules;

impl Rules {
    pub fn new() -> Self { Self }

    pub fn validate_legality(&self) -> Result<(), anyhow::Error> {
        todo!("M04-M06: implement rules validation")
    }
}
