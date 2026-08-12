//! Benchmark stub.

pub struct Benchmark;

impl Default for Benchmark {
    fn default() -> Self {
        Self::new()
    }
}

impl Benchmark {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// # Errors
    /// Not yet implemented; this is a stub.
    pub fn run(&self) -> Result<(), anyhow::Error> {
        todo!("M10: implement benchmarks")
    }
}
