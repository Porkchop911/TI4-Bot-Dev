//! Stage 2 shares the loop in [`crate::stage1`].
//!
//! Kept as a module because the plan names two packages, and as one line because the only thing
//! that differs between the stages is what a decision is worth — [`crate::reward::Stage`] decides
//! that, and the loop reads it. Two copies of a training loop is two places for a coefficient to
//! be applied to one stage and forgotten in the other, which is exactly the failure the oracle
//! recorded when a reward argument was added to one call site and not another.

pub use crate::stage1::{Generation, Plan, Run, train};
