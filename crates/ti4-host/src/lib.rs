//! `ti4-host`: everything between the engine and a client that does not depend on how the
//! process is hosted. The online server (`ti4-server`, threads and tokio) and the browser
//! (`ti4-wasm`, a worker) both build on it, so the wire messages, the event log and the
//! history rules cannot drift apart.
//!
//! No tokio, no threads spawned, no system clock: time and nonces are injected ([`clock`]).
//! `cargo check -p ti4-host --target wasm32-unknown-unknown` is part of the contract.

#![allow(clippy::missing_panics_doc, clippy::missing_errors_doc)]

pub mod batch;
pub mod card_set;
pub mod clock;
pub mod journal;
pub mod map;
pub mod maps;
pub mod planning;
pub mod preset;
pub mod projection;
pub mod protocol;
pub mod replay;
pub mod rng_force;
pub mod setup;
pub mod state_update;
pub mod step_snapshot;

pub use protocol::*;
