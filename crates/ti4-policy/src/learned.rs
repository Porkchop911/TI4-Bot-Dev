//! Learned policy inference stub.

pub struct LearnedPolicy;

impl LearnedPolicy {
    pub fn new() -> Self { Self }

    pub fn load(&self, _path: &str) -> Result<(), anyhow::Error> {
        todo!("M09: implement learned policy loading")
    }

    pub fn infer(&self, _features: &[f64]) -> Result<Vec<f64>, anyhow::Error> {
        todo!("M09: implement learned policy inference")
    }
}
