//! Effects stub.

use ti4_model::*;

pub struct Effect {
    pub target: String,
    pub value: i32,
    pub resolved: bool,
}

impl Effect {
    pub fn new(target: String, value: i32) -> Self {
        Self { target, value, resolved: false }
    }

    pub fn resolve(&mut self) {
        self.resolved = true;
    }
}
