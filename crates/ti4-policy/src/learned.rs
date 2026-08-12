//! Learned policy inference stub.

pub struct LearnedPolicy;

impl Default for LearnedPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl LearnedPolicy {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn load(&self, _path: &str) -> Result<(), anyhow::Error> {
        todo!("M09: implement learned policy loading")
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn infer(&self, _features: &[f64]) -> Result<Vec<f64>, anyhow::Error> {
        todo!("M09: implement learned policy inference")
    }
}
