//! The battle calculator: a lean simulator of space and ground combat ([`arena`]) and the odds
//! that its rollouts give ([`odds`]).
//!
//! It needs the content and the model only, so that it can be linked where the training and the
//! advisor cannot (a wasm build).

pub mod arena;
pub mod odds;
