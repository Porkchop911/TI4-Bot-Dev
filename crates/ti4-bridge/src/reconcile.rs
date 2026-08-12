//! Reconcile stub.

pub struct Reconcile;

impl Default for Reconcile {
    fn default() -> Self {
        Self::new()
    }
}

impl Reconcile {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M11: implement reconcile")
    }
}
