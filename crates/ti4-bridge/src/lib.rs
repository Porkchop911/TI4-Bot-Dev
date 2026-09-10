//! The Tabletop Simulator bridge.
//!
//! `wire` (M11-001), `hexsummary` (M11-007), `mapping` (M11-009), `import` (M11-010),
//! `commands` (M11-014/015) and `client` are real.
//!
//! There is no HTTP *server* here and there will not be one. `bridge/server.py` is vendored from
//! the historical repository and kept: standard-library only, loopback-bound, engine-free, and
//! proven by real games. It satisfies M11-002 through M11-005, and `client` is how Rust talks to
//! it. The remaining stubs are the engine-coupled half: command builders (M11-014/015), telemetry
//! import (M11-010), reconciliation (M11-013) and audit (M11-012).

pub mod audit;
pub mod client;
pub mod commands;
pub mod hexsummary;
pub mod http;
pub mod import;
pub mod mapping;
pub mod reconcile;
pub mod tts;
pub mod wire;

pub use audit::*;
pub use client::{BridgeClient, ClientError, DEFAULT_ADDRESS};
pub use http::*;
pub use import::*;
pub use reconcile::*;
pub use tts::*;
pub use wire::*;

// `hexsummary` is not re-exported flat: its `Region`, `System` and `Board` are the *table's*
// view, and colliding them with the engine's same-named concepts is exactly the confusion M11-012
// exists to keep visible.
