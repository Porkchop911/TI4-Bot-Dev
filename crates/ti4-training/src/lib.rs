//! Stage 1/2 learning, promotion, archives, capture.
//!
//! Implemented: `reward` (M10-011), `rollout` (M10-012), `gradient` (M10-013, M10-014) and the
//! training loop in `stage1`, which `stage2` shares. Promotion, archives and Parquet capture
//! remain stubs and are named as such rather than left to look finished.

pub mod archive;
pub mod capture;
pub mod evaluation;
pub mod gradient;
pub mod promotion;
pub mod reward;
pub mod rollout;
pub mod stage1;
pub mod stage2;

pub use archive::*;
pub use capture::*;
pub use evaluation::*;
pub use gradient::*;
pub use promotion::*;
pub use reward::*;
pub use rollout::*;
pub use stage1::*;
