//! ti4-model: Typed IDs, state, units, views, and schema contracts.
//!
//! All IDs are newtypes to prevent accidental mixing. State is owned by the engine
//! and not publicly mutable. Views provide redacted access for bots and TTS.

pub mod id;
pub mod state;
pub mod units;
pub mod view;
pub mod content_types;
pub mod factions;

pub use id::*;
pub use state::*;
pub use units::*;
pub use content_types::*;
pub use factions::*;
