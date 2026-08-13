//! Authored bots: redacted observation, valuation, and scored choice (M08).
//!
//! `view` and `valuation` are implemented (M08-001, M08-002). The scored bot itself, feature
//! extraction and learned inference remain stubs and are named as such rather than left to look
//! finished.

pub mod bot;
pub mod features;
pub mod learned;
pub mod scoring;
pub mod valuation;
pub mod view;

pub use bot::*;
pub use features::*;
pub use learned::*;
pub use scoring::*;
