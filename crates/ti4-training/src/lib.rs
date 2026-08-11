//! Stub: Stage 1/2 learning, promotion, archives, Parquet capture.
//! Full implementation in M10.

pub mod stage1;
pub mod stage2;
pub mod promotion;
pub mod archive;
pub mod capture;

pub use stage1::*;
pub use stage2::*;
pub use promotion::*;
pub use archive::*;
pub use capture::*;
