//! `ti4-view` turns engine state into the redacted views a client is shown.
//!
//! It depends on the model, the content and the engine only, so the server and the wasm build
//! both use it.

#![allow(clippy::missing_panics_doc, clippy::missing_errors_doc)]

pub mod action_cards;
pub mod agendas;
pub mod components;
pub mod exploration;
pub mod map;
pub mod maps;
pub mod planning;
pub mod projection;
pub mod reaction;
pub mod reaction_modes;
pub mod scenario;
pub mod score;
pub mod seats;
pub mod secondary;
pub mod secret_objectives;
pub mod status;
pub mod tactical;
pub mod view;
