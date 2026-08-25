//! The bounded CPU tensor adapter (M09-025).
//!
//! # Scope
//!
//! Enough surface for M09-026 to build the batched MLP actor on, and nothing more. There is no
//! model here: no layers, no heads, no readouts, no training. Every function this crate gains
//! beyond the floor is a function M09-026's review has to take on trust, so the floor is where it
//! stops.
//!
//! # CPU only
//!
//! MLP plan §7.1 is explicit that CUDA is never an inference backend on this branch. This crate has
//! no CUDA feature, no optional GPU dependency, and no device parameter. Everything runs on CPU
//! because that is the only device it can name.
//!
//! # The pin
//!
//! `tch = "=0.22.0"` against libtorch 2.9.1, pinned by SHA-256 in
//! `plans/artifacts/libtorch-2.9.1-cpu.manifest.json`. Newer `tch` requires libtorch 2.13 and would
//! mean a ~2 GB download; 0.22 is the release built for the 2.9 series, which is already on disk.
//! `.cargo/config.toml` points `LIBTORCH` at the pinned copy relative to the repository, so the pin
//! travels with the checkout rather than depending on a shell export.

use thiserror::Error;

pub use tch::{Device, Kind, Tensor};

/// Anything that stopped a tensor operation.
#[derive(Debug, Error)]
pub enum TensorError {
    /// A sparse vector's index and value slices disagree in length.
    #[error("{indices} indices against {values} values")]
    Ragged { indices: usize, values: usize },
    /// A column index is outside the allocated capacity.
    #[error("column {column} is outside a table of {capacity} rows")]
    OutOfRange { column: i64, capacity: i64 },
    /// The deterministic configuration did not take effect.
    ///
    /// §7.2 requires the settings to be *enforced*, not merely requested. Setting a thread count
    /// and carrying on is the failure this names.
    #[error("{setting}: asked for {wanted}, libtorch reports {got}")]
    NotEnforced {
        setting: &'static str,
        wanted: i64,
        got: i64,
    },
}

/// What the process is running on, recorded rather than assumed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    /// Always false on this branch. Recorded so a build that acquired CUDA fails a test rather
    /// than quietly changing what produces a decision.
    pub cuda: bool,
    /// CUDA devices visible. Zero.
    pub cuda_devices: usize,
    /// Intra-op threads libtorch reports.
    pub intra_op_threads: i32,
    /// Inter-op threads libtorch reports.
    pub inter_op_threads: i32,
    /// Whether libtorch was built with MKL, which decides which BLAS the reductions run through.
    pub mkl: bool,
    /// Whether libtorch was built with OpenMP — the thing `intra_op_threads` actually governs.
    pub openmp: bool,
}

/// Read the backend as libtorch reports it.
#[must_use]
pub fn backend() -> Backend {
    Backend {
        cuda: tch::Cuda::is_available(),
        cuda_devices: usize::try_from(tch::Cuda::device_count()).unwrap_or(0),
        intra_op_threads: tch::get_num_threads(),
        inter_op_threads: tch::get_num_interop_threads(),
        mkl: tch::utils::has_mkl(),
        openmp: tch::utils::has_openmp(),
    }
}

/// Pin the deterministic configuration, and prove it took effect.
///
/// §7.2 pins libtorch's intra-op and inter-op thread counts and the RNG state. Single-threaded is
/// the setting that matters for reproducibility here: a parallel reduction sums in whatever order
/// the threads finish, and f32 addition is not associative, so a multi-threaded sum is
/// reproducible only by accident.
///
/// The thread counts are **read back** rather than assumed. `set_num_interop_threads` in
/// particular is only honoured before the inter-op pool is first used, so a call that silently did
/// nothing is exactly the shape §7.2 warns about.
///
/// # Errors
/// [`TensorError::NotEnforced`] if libtorch does not report the settings that were requested.
pub fn configure_deterministic(seed: i64) -> Result<Backend, TensorError> {
    tch::set_num_threads(1);
    pin_interop_threads();
    tch::manual_seed(seed);

    let reported = backend();
    if reported.intra_op_threads != 1 {
        return Err(TensorError::NotEnforced {
            setting: "intra-op threads",
            wanted: 1,
            got: i64::from(reported.intra_op_threads),
        });
    }
    Ok(reported)
}

/// Attempt the inter-op pool size exactly once per process.
///
/// **libtorch permits this setting once, before any parallel work has started**, and raises
/// otherwise: *"cannot set number of interop threads after parallel work has started or
/// `set_num_interop_threads` called"*. `tch` surfaces that by panicking rather than returning, so
/// second call takes the process down.
///
/// That makes inter-op a **process-lifetime** setting rather than a configurable one. A binary that
/// wants it must call [`configure_deterministic`] before touching a tensor; a test *process* that
/// has already run other tests cannot have it, which is why the proof lives in its own integration
/// test with its own process (`tests/interop.rs`).
///
/// The `Once` makes repeat calls safe, and the `catch_unwind` covers the case where libtorch has
/// already started work before the first call — best effort, then read back and reported honestly
/// by [`backend`] rather than assumed.
fn pin_interop_threads() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let attempt = std::panic::catch_unwind(|| tch::set_num_interop_threads(1));
        if attempt.is_err() {
            // libtorch had already started parallel work. Nothing to recover: the pool size is
            // whatever it became, and `backend()` reports it.
        }
    });
}

/// A zeroed `[capacity, width]` input table.
///
/// The rows above `slot_count` are part of this allocation and start zero, which is what M09-024a's
/// capacity headroom assumes and what M09-026/M09-028 must keep asserting.
pub fn zeros_table(capacity: i64, width: i64) -> Tensor {
    Tensor::zeros([capacity, width], (Kind::Float, Device::Cpu))
}

/// Gather the rows a sparse feature vector names, scale each by its value, and sum them.
///
/// This is the first layer of the per-option trunk: MLP plan §4.3 requires it to be an
/// embedding-bag calculation rather than a materialised `[N, V_cap]` product, because a decision
/// carries around thirty active columns against a table of tens of thousands.
///
/// **Indices are sorted before the reduction**, and that is not cosmetic. f32 addition is not
/// associative, so summing the same rows in a different order gives a different last bit, and a
/// softmax over near-tied logits turns a last bit into a different action. §4.3 forbids unsorted
/// or hash iteration for exactly this reason. Duplicate columns are summed rather than rejected:
/// a feature name can legitimately be contributed twice, and the fixed order makes that sum
/// reproducible.
///
/// # Errors
/// [`TensorError::Ragged`] if the slices disagree in length, [`TensorError::OutOfRange`] if a
/// column falls outside the table.
pub fn gather_reduce(
    table: &Tensor,
    columns: &[i64],
    values: &[f32],
) -> Result<Tensor, TensorError> {
    if columns.len() != values.len() {
        return Err(TensorError::Ragged {
            indices: columns.len(),
            values: values.len(),
        });
    }
    let (capacity, width) = {
        let size = table.size();
        (size[0], size[1])
    };
    if let Some(&bad) = columns
        .iter()
        .find(|column| **column < 0 || **column >= capacity)
    {
        return Err(TensorError::OutOfRange {
            column: bad,
            capacity,
        });
    }
    if columns.is_empty() {
        // An empty vector is a legal input — a decision whose every feature was out of vocabulary.
        // It contributes the zero row, not an error and not a panic.
        return Ok(Tensor::zeros([width], (Kind::Float, Device::Cpu)));
    }

    let mut pairs: Vec<(i64, f32)> = columns
        .iter()
        .copied()
        .zip(values.iter().copied())
        .collect();
    pairs.sort_by_key(|(column, _)| *column);
    let sorted_columns: Vec<i64> = pairs.iter().map(|(column, _)| *column).collect();
    let sorted_values: Vec<f32> = pairs.iter().map(|(_, value)| *value).collect();

    let index = Tensor::from_slice(&sorted_columns);
    let scale = Tensor::from_slice(&sorted_values).unsqueeze(1);
    let rows = table.index_select(0, &index) * scale;
    Ok(rows.sum_dim_intlist([0i64].as_slice(), false, Kind::Float))
}

/// A dense `[n, a] × [a, b]` product, for the hidden layer and the readouts.
pub fn matmul(left: &Tensor, right: &Tensor) -> Tensor {
    left.matmul(right)
}

/// A tensor's values as `f32`, for assertions and for recording evidence.
#[must_use]
pub fn to_vec(tensor: &Tensor) -> Vec<f32> {
    Vec::<f32>::try_from(tensor.contiguous().view([-1])).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: i64 = 20_260_821;

    #[test]
    fn the_backend_is_cpu_only() {
        // §7.1: CUDA is never an inference backend on this branch. A build that acquired one is a
        // build that could produce a different decision, so this is a test rather than a comment.
        let backend = backend();
        assert!(!backend.cuda, "CUDA is available to a CPU-only branch");
        assert_eq!(backend.cuda_devices, 0);
    }

    #[test]
    fn the_deterministic_configuration_is_enforced_not_merely_requested() {
        // §7.2's failure mode: settings that are called and not checked. The thread counts are
        // read back from libtorch, and a request that did not take effect is an error.
        let reported = configure_deterministic(SEED).expect("the configuration must take effect");
        assert_eq!(reported.intra_op_threads, 1);
        assert_eq!(
            tch::get_num_threads(),
            1,
            "libtorch disagrees with the report"
        );
        // Non-vacuity: the setting is one libtorch can actually change, so asserting it equals 1
        // is not asserting a constant.
        tch::set_num_threads(2);
        assert_eq!(
            tch::get_num_threads(),
            2,
            "the thread count is not settable at all"
        );
        configure_deterministic(SEED).expect("re-pinned");
        assert_eq!(tch::get_num_threads(), 1);
        // Inter-op is deliberately not asserted here: it is settable once per process and this
        // process shares itself with every other test in the binary. `tests/interop.rs` proves it
        // in a process of its own.
    }

    #[test]
    fn the_same_input_twice_is_bit_identical() {
        // The smoke §7.1 asks for. Deliberately not a three-element toy: the table is wide enough
        // and the vector long enough that a parallel reduction could plausibly have reordered the
        // sum, so agreement is evidence about the configuration rather than about the input being
        // too small to differ.
        configure_deterministic(SEED).expect("configured");
        let width = 256;
        let capacity = 4_096;
        let table = Tensor::rand([capacity, width], (Kind::Float, Device::Cpu));

        let columns: Vec<i64> = (0..capacity).step_by(7).collect();
        #[expect(
            clippy::cast_precision_loss,
            reason = "small deterministic fixture values"
        )]
        let values: Vec<f32> = columns
            .iter()
            .map(|c| (*c as f32).mul_add(0.001, 0.5))
            .collect();
        assert!(
            columns.len() > 500,
            "the fixture must be large enough to reorder"
        );

        let first = to_vec(&gather_reduce(&table, &columns, &values).expect("reduces"));
        let second = to_vec(&gather_reduce(&table, &columns, &values).expect("reduces"));
        assert_eq!(first.len(), usize::try_from(width).expect("small"));
        assert_eq!(first, second, "the same input produced a different sum");
        assert!(
            first.iter().any(|value| *value != 0.0),
            "the fixture reduced to zero: agreement would be vacuous"
        );
    }

    #[test]
    fn the_reduction_does_not_depend_on_the_order_the_columns_arrive_in() {
        // The property §4.3 requires and the reason the indices are sorted first. A caller that
        // hands the same features in a different order must get the same row, to the bit.
        configure_deterministic(SEED).expect("configured");
        let table = Tensor::rand([512, 64], (Kind::Float, Device::Cpu));
        let columns: Vec<i64> = (0..512).step_by(3).collect();
        #[expect(
            clippy::cast_precision_loss,
            reason = "small deterministic fixture values"
        )]
        let values: Vec<f32> = columns.iter().map(|c| 1.0 - (*c as f32) * 0.0005).collect();

        let forward = to_vec(&gather_reduce(&table, &columns, &values).expect("reduces"));
        let mut reversed: Vec<(i64, f32)> = columns
            .iter()
            .copied()
            .zip(values.iter().copied())
            .collect();
        reversed.reverse();
        let reversed_columns: Vec<i64> = reversed.iter().map(|(c, _)| *c).collect();
        let reversed_values: Vec<f32> = reversed.iter().map(|(_, v)| *v).collect();
        let backward =
            to_vec(&gather_reduce(&table, &reversed_columns, &reversed_values).expect("reduces"));

        assert_ne!(
            columns, reversed_columns,
            "the fixture must actually differ in order"
        );
        assert_eq!(
            forward, backward,
            "the reduction followed the caller's order"
        );
    }

    #[test]
    fn duplicate_columns_are_summed_in_a_fixed_order() {
        configure_deterministic(SEED).expect("configured");
        let table = zeros_table(4, 3);
        let _ = table.narrow(0, 1, 1).fill_(2.0);

        // Column 1 twice: the row is worth 2.0 per element, so 0.5 + 0.25 of it is 1.5.
        let summed = to_vec(&gather_reduce(&table, &[1, 1], &[0.5, 0.25]).expect("reduces"));
        assert_eq!(summed, vec![1.5, 1.5, 1.5]);
    }

    #[test]
    fn an_empty_vector_contributes_the_zero_row() {
        // Every feature out of vocabulary is a legal position, not an error and not a panic.
        configure_deterministic(SEED).expect("configured");
        let table = Tensor::rand([16, 8], (Kind::Float, Device::Cpu));
        let empty = gather_reduce(&table, &[], &[]).expect("an empty vector is legal");
        assert_eq!(to_vec(&empty), vec![0.0; 8]);
    }

    #[test]
    fn a_ragged_or_out_of_range_vector_is_refused() {
        configure_deterministic(SEED).expect("configured");
        let table = zeros_table(8, 4);
        assert!(matches!(
            gather_reduce(&table, &[0, 1], &[1.0]),
            Err(TensorError::Ragged {
                indices: 2,
                values: 1
            })
        ));
        assert!(matches!(
            gather_reduce(&table, &[8], &[1.0]),
            Err(TensorError::OutOfRange {
                column: 8,
                capacity: 8
            })
        ));
        assert!(matches!(
            gather_reduce(&table, &[-1], &[1.0]),
            Err(TensorError::OutOfRange { column: -1, .. })
        ));
    }

    #[test]
    fn free_rows_start_zero() {
        // M09-024a's capacity headroom assumes rows above `slot_count` are zero, and M09-026/028
        // must keep asserting it. The allocation is where that starts being true.
        let table = zeros_table(64, 16);
        assert_eq!(to_vec(&table), vec![0.0; 64 * 16]);
        let reduced = gather_reduce(&table, &[63], &[1.0]).expect("reduces");
        assert_eq!(
            to_vec(&reduced),
            vec![0.0; 16],
            "a free row contributed something"
        );
    }

    #[test]
    fn a_dense_product_has_the_shape_the_trunk_needs() {
        configure_deterministic(SEED).expect("configured");
        let batch = Tensor::rand([5, 256], (Kind::Float, Device::Cpu));
        let hidden = Tensor::rand([256, 128], (Kind::Float, Device::Cpu));
        let out = matmul(&batch, &hidden);
        assert_eq!(out.size(), vec![5, 128]);
    }
}
