//! Content provenance stub.

pub struct Provenance;

impl Provenance {
    pub fn new() -> Self { Self }

    pub fn compute_hashes(&self) -> Result<(), anyhow::Error> {
        todo!("M02: implement provenance hashing")
    }
}
