//! Stub: choices, timing, legal actions, rules, effects, and game loop.
//! Full implementation in M03-M06.

pub mod choice;
pub mod timing;
pub mod rules;
pub mod effects;
pub mod game;
pub mod phase;
pub mod tactical;

pub use choice::*;
pub use timing::*;
pub use rules::*;
pub use effects::*;
pub use game::*;
pub use phase::*;
pub use tactical::*;
