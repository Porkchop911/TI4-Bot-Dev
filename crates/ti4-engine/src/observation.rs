//! Activity on a disposable execution, without retaining its unknown results.
//!
//! Rules still run normally. A caller can reject the copy before publishing it.
//! Temporary RNG clones share this handle; a new game fork drops the binding.

use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct ExecutionObservation(Arc<AtomicU8>);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObservedActivity {
    pub randomness: bool,
    pub hidden_information: bool,
    pub unsupported_participation: bool,
    pub unsupported_segment: bool,
}

impl ExecutionObservation {
    pub fn randomness(&self) {
        self.0.fetch_or(1, Ordering::SeqCst);
    }
    pub fn hidden_information(&self) {
        self.0.fetch_or(2, Ordering::SeqCst);
    }
    pub fn unsupported_participation(&self) {
        self.0.fetch_or(4, Ordering::SeqCst);
    }
    pub fn unsupported_segment(&self) {
        self.0.fetch_or(8, Ordering::SeqCst);
    }

    #[must_use]
    pub fn activity(&self) -> ObservedActivity {
        Self::read(self.0.load(Ordering::SeqCst))
    }

    /// The activity since the last call, for a caller that asks after each decision of a game
    /// that goes on (the undo of a seat on the server). A latch that is taken is clear again.
    #[must_use]
    pub fn take(&self) -> ObservedActivity {
        Self::read(self.0.swap(0, Ordering::SeqCst))
    }

    const fn read(bits: u8) -> ObservedActivity {
        ObservedActivity {
            randomness: bits & 1 != 0,
            hidden_information: bits & 2 != 0,
            unsupported_participation: bits & 4 != 0,
            unsupported_segment: bits & 8 != 0,
        }
    }
}
