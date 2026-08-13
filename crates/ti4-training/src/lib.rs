//! Stage 1/2 learning, promotion, archives, capture.
//!
//! `reward` is implemented (M10-011): the returns the two stages optimise. The rollout loop,
//! promotion, archives and capture remain stubs and are named as such rather than left to look
//! finished.

pub mod archive;
pub mod capture;
pub mod promotion;
pub mod reward;
pub mod rollout;
pub mod stage1;
pub mod stage2;

pub use archive::*;
pub use capture::*;
pub use promotion::*;
pub use reward::*;
pub use rollout::*;
pub use stage1::*;
pub use stage2::*;
