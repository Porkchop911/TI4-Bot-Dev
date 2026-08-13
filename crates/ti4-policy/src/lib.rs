//! Policy: the authored bot (M08), and the policy whose utility is entirely learned (M09).
//!
//! Two answers to the same question, kept side by side on purpose. [`bot::ScoredBot`] decides by
//! hand-written constants and is the baseline a learned policy has to beat; [`inference::LearnedBot`]
//! decides by weights fitted from played games and reads nothing authored.
//!
//! Implemented: `view`, `valuation`, `scoring`, `bot` (M08-001 to M08-004, M08-011); `learned`,
//! `features`, `inference` (M09-001 to M09-004, M09-006, M09-013). The training loop that fits a
//! profile lives in `ti4-training`.

pub mod bot;
pub mod features;
pub mod inference;
pub mod learned;
pub mod scoring;
pub mod valuation;
pub mod view;

pub use bot::*;
pub use features::*;
pub use inference::*;
pub use learned::*;
pub use scoring::*;
