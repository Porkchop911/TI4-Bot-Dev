//! ti4-model: Typed IDs, state, units, views, and schema contracts.
//!
//! All IDs are newtypes to prevent accidental mixing. State is owned by the engine
//! and not publicly mutable. Views provide redacted access for bots and TTS.

pub mod content_types;
pub mod hex;
pub mod id;
pub mod schema;
pub mod state;
pub mod units;
pub mod view;

pub use content_types::*;
pub use hex::Hex;
pub use id::*;
pub use schema::*;
pub use state::*;
pub use units::*;
