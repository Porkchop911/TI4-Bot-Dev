//! Stage 1/2 learning, promotion, archives, capture.
//!
//! Implemented: `reward` (M10-011), `rollout` (M10-012), `gradient` (M10-013, M10-014) and the
//! training loop in `stage1`, which `stage2` shares. Promotion, archives and Parquet capture
//! remain stubs and are named as such rather than left to look finished.

/// Every training binary allocates far more than it computes, so the allocator is a first-order
/// choice rather than a detail.
///
/// Measured on this workload (30 games per run, trained profiles, `FULL`, four rounds, release,
/// single-threaded): the Windows system allocator averages 17.89 ms per training game, mimalloc
/// 9.46 ms — **1.89x**. The same measurement with `-C target-cpu=native` moves nothing (+1%),
/// which is the evidence that this is an allocation-bound workload and not a compute-bound one.
/// Threads make the gap wider, not narrower: mimalloc's per-thread heaps remove exactly the
/// contention 32 rollout workers create.
///
/// Declared in the library rather than in each of the fourteen examples, because it is a property
/// of the workload and not of any one entry point. A `#[global_allocator]` is program-wide, so
/// every binary linking this crate — examples, tests, benches — gets it.
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

pub mod archive;
pub mod capture;
pub mod evaluation;
pub mod gradient;
pub mod ppo;
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
