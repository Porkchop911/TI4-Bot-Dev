//! Stub: Stage 1/2 learning, promotion, archives, Parquet capture.
//! Full implementation in M10.

pub mod archive;
pub mod capture;
pub mod promotion;
pub mod stage1;
pub mod stage2;

pub use archive::*;
pub use capture::*;
pub use promotion::*;
pub use stage1::*;
pub use stage2::*;
