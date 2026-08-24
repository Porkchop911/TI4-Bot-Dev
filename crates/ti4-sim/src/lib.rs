//! Simulation: maps, replay, batches, rotations, benchmarks.
//!
//! `result` and `run` are implemented (M10-001, M10-007, M10-008). Maps, replay, rotations and
//! the report suites remain stubs and are named as such rather than left to look finished.

pub mod baseline;
pub mod batch;
pub mod behavior;
pub mod benchmark;
pub mod maps;
pub mod profile;
pub mod replay;
pub mod result;
pub mod rotation;
pub mod run;

pub use baseline::*;
pub use batch::*;
pub use behavior::*;
pub use benchmark::*;
pub use maps::*;
pub use profile::*;
pub use replay::*;
pub use result::*;
pub use rotation::*;
pub use run::*;
