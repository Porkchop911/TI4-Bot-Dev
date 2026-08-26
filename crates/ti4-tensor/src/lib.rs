//! The bounded CPU tensor adapter (M09-025).
//!
//! # Scope
//!
//! Enough surface for M09-026 to build the batched MLP actor on, and nothing more. There is no
//! model here: no layers, no heads, no readouts, no training. Every function this crate gains
//! beyond the floor is a function M09-026's review has to take on trust, so the floor is where it
//! stops.
//!
//! # CPU inference, and only CPU inference
//!
//! MLP plan §7.1 is explicit that CUDA is never an inference *backend* on this branch: every action
//! for rollout, validation and evaluation is selected by the deterministic CPU path.
//!
//! Until M10-037 that was enforced by the crate having no way to name another device at all. It now
//! can — §7.1's one permitted switch moves the model and Adam state to CUDA for forward, backward
//! and update between CPU rollouts — so the guarantee is carried by [`inference_device`], which
//! takes no parameters and returns `Cpu`, and by [`OptimizerDevice`], which is the only thing that
//! can say otherwise and applies only to an optimiser step.
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

/// The pinned `tch` version this crate is built against.
///
/// Recorded in every checkpoint manifest (MLP plan §4.4) so a bundle carries the binding it was
/// produced under. Kept in step with the workspace dependency by
/// `the_pinned_versions_match_the_build`.
pub const TCH_VERSION: &str = "0.22.0";

/// The pinned libtorch version, matching `plans/artifacts/libtorch-2.9.1-cpu.manifest.json`.
pub const LIBTORCH_VERSION: &str = "2.9.1";

/// Anything that stopped a tensor operation.
#[derive(Debug, Error)]
pub enum TensorError {
    /// CUDA was requested for the optimiser and the linked libtorch has none.
    #[error("CUDA was requested but the linked libtorch has no CUDA device")]
    NoCuda,
    /// A sparse vector's index and value slices disagree in length.
    #[error("{indices} indices against {values} values")]
    Ragged { indices: usize, values: usize },
    /// A column index is outside the allocated capacity.
    #[error("column {column} is outside a table of {capacity} rows")]
    OutOfRange { column: i64, capacity: i64 },
    /// A feature value was NaN or infinite.
    #[error("feature value {value} is not finite")]
    NotFinite { value: f32 },
    /// A tensor could not be converted to host values.
    #[error("tensor conversion failed: {0}")]
    Conversion(String),
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
    /// Whether the linked libtorch has a usable CUDA device.
    ///
    /// Recorded, not forbidden. Before M10-037 this was asserted false, because the branch linked a
    /// CPU-only libtorch and a build that silently acquired CUDA could have changed what produced a
    /// decision. That is now a deliberate configuration, so the guarantee moved to
    /// [`inference_device`] — a strictly stronger statement, since it holds whether or not CUDA is
    /// present — and this field's job is to make a manifest say which device a number came from.
    pub cuda: bool,
    /// CUDA devices visible.
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
    // libtorch's thread counts and RNG are process-global, and cargo runs a binary's tests in
    // parallel threads of one process. Without this lock, one test's configuration and another's
    // read interleave and the gate becomes scheduler-dependent rather than deterministic — which
    // is not a gate (F-M09-025-2, harness note).
    let _serialised = CONFIG_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    tch::set_num_threads(1);
    pin_interop_threads();
    tch::manual_seed(seed);

    let reported = backend();
    // **Both** counts. `pin_interop_threads` swallows libtorch's refusal so a second call cannot
    // take the process down, and an earlier version then returned `Ok` regardless — a swallowed
    // failure reported as successful configuration, which is exactly the shape §7.2 warns about
    // (F-M09-025-2).
    for (setting, got) in [
        ("intra-op threads", reported.intra_op_threads),
        ("inter-op threads", reported.inter_op_threads),
    ] {
        if got != 1 {
            return Err(TensorError::NotEnforced {
                setting,
                wanted: 1,
                got: i64::from(got),
            });
        }
    }
    Ok(reported)
}

/// Where an optimiser step may run.
///
/// # Why this exists as a type rather than a flag someone remembers
///
/// MLP plan §7.1 is categorical: *"CUDA is never an inference backend in this branch. Every action
/// for rollout, validation and evaluation is selected by the deterministic CPU path. The only
/// switch is `--optimizer-device cpu|cuda`: after CPU rollouts produce a fixed batch, the model and
/// Adam state may move to CUDA for forward/backward/update and return to CPU before the next
/// decision."*
///
/// So there are two devices in play and exactly one of them is negotiable. Naming the negotiable
/// one keeps the other from drifting: [`inference_device`] is a function with no parameters and one
/// possible answer, and every tensor a decision is built from goes through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerDevice {
    /// Everything on CPU. The default, and the only setting that needs no gate.
    Cpu,
    /// Forward, backward and the Adam update on CUDA, between CPU rollouts.
    ///
    /// Selected only after M10-037's gate: fixed-batch gradient agreement with CPU, repeatability,
    /// and at least a 10% median end-to-end improvement whose paired 95% bootstrap interval has a
    /// lower bound above zero. Otherwise M10-037 closes as a measured no-op.
    Cuda,
}

impl OptimizerDevice {
    /// The libtorch device this names.
    ///
    /// # Errors
    /// [`TensorError::NoCuda`] if CUDA was asked for and the linked libtorch has none. Refused
    /// rather than silently falling back: a run that believed it was on CUDA and quietly was not
    /// would report a device in its manifest that never executed a gradient.
    pub fn resolve(self) -> Result<Device, TensorError> {
        match self {
            Self::Cpu => Ok(Device::Cpu),
            Self::Cuda if tch::Cuda::is_available() => Ok(Device::Cuda(0)),
            Self::Cuda => Err(TensorError::NoCuda),
        }
    }
}

/// The device every decision is computed on, always.
///
/// Not a setting. §7.1 admits no CUDA inference backend, and the whole determinism argument in §7.2
/// rests on the CPU path being the only one that ever selects an action.
#[must_use]
pub const fn inference_device() -> Device {
    Device::Cpu
}

/// Serialises every global libtorch configuration change.
static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

/// The same table on a chosen device, for an optimiser that runs off the CPU.
pub fn zeros_table_on(capacity: i64, width: i64, device: Device) -> Tensor {
    Tensor::zeros([capacity, width], (Kind::Float, device))
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
    if let Some(bad) = values.iter().copied().find(|value| !value.is_finite()) {
        // A NaN has no place in a total order and none in a logit; an infinity poisons every sum
        // downstream. Refused here rather than propagated into a softmax that would return a
        // plausible-looking distribution.
        return Err(TensorError::NotFinite { value: bad });
    }
    if columns.is_empty() {
        // An empty vector is a legal input — a decision whose every feature was out of vocabulary.
        // It contributes the zero row, not an error and not a panic.
        return Ok(Tensor::zeros([width], (Kind::Float, table.device())));
    }

    // Sorting by column alone is not enough. Rust's sort is stable, so duplicate columns kept the
    // *caller's* order — and f32 addition is not associative, so a caller handing one column
    // large-positive, large-negative and small contributions in a different order got a different
    // sum (F-M09-025-3). Duplicates are explicitly legal, so the order among them is part of the
    // contract, not an accident of the sort.
    //
    // The tie-break is the value's bit pattern under a total order. Non-finite values are rejected
    // above, so nothing incomparable reaches it, and `-0.0` and `+0.0` get a defined relative order
    // rather than comparing equal and falling back to caller order.
    let pairs = ordered_pairs(columns, values, capacity)?;
    let sorted_columns: Vec<i64> = pairs.iter().map(|(column, _)| *column).collect();
    let sorted_values: Vec<f32> = pairs.iter().map(|(_, value)| *value).collect();

    // Indices and scales are built on the host and moved once, so the device follows the table
    // rather than being assumed. An index on a different device than the tensor it selects from is
    // an error in libtorch, not a silent copy.
    let device = table.device();
    let index = Tensor::from_slice(&sorted_columns).to_device(device);
    let scale = Tensor::from_slice(&sorted_values)
        .unsqueeze(1)
        .to_device(device);
    let rows = table.index_select(0, &index) * scale;
    Ok(rows.sum_dim_intlist([0i64].as_slice(), false, Kind::Float))
}

/// One option's `(column, value)` pairs in the fixed reduction order.
///
/// Split out of [`gather_reduce`] so the batched path shares exactly the same ordering rule rather
/// than reimplementing it — the two must agree bit for bit, and the cheapest way to guarantee that
/// is for there to be one implementation.
fn ordered_pairs(
    columns: &[i64],
    values: &[f32],
    capacity: i64,
) -> Result<Vec<(i64, f32)>, TensorError> {
    if columns.len() != values.len() {
        return Err(TensorError::Ragged {
            indices: columns.len(),
            values: values.len(),
        });
    }
    if let Some(&bad) = columns
        .iter()
        .find(|column| **column < 0 || **column >= capacity)
    {
        return Err(TensorError::OutOfRange {
            column: bad,
            capacity,
        });
    }
    if let Some(bad) = values.iter().copied().find(|value| !value.is_finite()) {
        return Err(TensorError::NotFinite { value: bad });
    }
    let mut pairs: Vec<(i64, f32)> = columns
        .iter()
        .copied()
        .zip(values.iter().copied())
        .collect();
    pairs.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| total_order_key(left.1).cmp(&total_order_key(right.1)))
    });
    Ok(pairs)
}

/// Every option of one decision gathered in a single pass: `[options, width]`.
///
/// # Why this exists
///
/// MLP plan §4.3: "batch every option of a decision into one forward pass ... Batching within a
/// decision is not optional." The hidden layer was already batched; the **gather** was not.
///
/// # What actually costs, measured rather than assumed
///
/// M09-029 profiled one decision and the gather was 206 us of 252. It is neither dispatch-bound nor
/// FLOP-bound but **latency**-bound: the input table is `[16384, width]`, far larger than L2, and a
/// decision fetches hundreds of 1 KiB rows from random offsets in it. Halving the width barely moved
/// the total, which is the signature of a per-row cost rather than a per-byte one.
///
/// So the lever is the number of rows fetched. Measured over 40,000 real decisions: a mean of 6.2
/// options and **528.8 gathered rows of which only 131.9 are distinct** — every row is fetched about
/// four times, once per option that mentions it.
///
/// So the lever is the number of rows fetched, and this aggregates duplicate columns *within* each
/// option first — §4.3's "aggregate duplicate feature names first" — then hands one flat index list,
/// one offset list and one weight list to a fused `embedding_bag`, which walks the indices once and
/// accumulates straight into each option's output row.
///
/// # What this used to say, and why it changed
///
/// Two earlier shapes are recorded here because their measurements are the reason for the current
/// one, and because the rustdoc went on describing the first of them after it had been replaced.
///
/// The first gathered each distinct row once into a `[distinct, width]` block and combined it with
/// an `[options, distinct] x [distinct, width]` matmul. That is fine at inference scale and becomes
/// a 34-million-float combination matrix for a training minibatch — measured at 0.11x, slower than
/// not batching at all. The second replaced the matrix with a segment sum, which materialises a
/// `[total_entries, width]` intermediate instead: about 268 MB for a 512-decision micro-batch, and
/// it made a CPU epoch time out. `embedding_bag` materialises neither.
///
/// # Determinism
///
/// Duplicate columns *within* an option are aggregated in the fixed order [`ordered_pairs`] defines,
/// which is the contract F-M09-025-3 established and the reason that order exists. The accumulation
/// `embedding_bag` performs across an option's entries is unspecified, as `sum_dim_intlist` and the
/// matmul were before it, and identical for a fixed build, device and thread count. On CUDA its
/// **backward** uses atomics and is not reproducible run to run; see
/// `plans/M10-032_DETERMINISM_FINDING.md`.
///
/// # Errors
/// As [`gather_reduce`]: ragged input, an out-of-range column, or a non-finite value.
pub fn gather_reduce_batch(
    table: &Tensor,
    options: &[(&[i64], &[f32])],
) -> Result<Tensor, TensorError> {
    let (capacity, width) = {
        let size = table.size();
        (size[0], size[1])
    };
    let rows = i64::try_from(options.len()).unwrap_or(0);

    // Per option, in the fixed order, with duplicates summed as they are met.
    //
    // An earlier version also accumulated every column into a `distinct` vector and sorted and
    // deduplicated it. That was left over from a gather that really did index distinct rows; the
    // fused `embedding_bag` below walks the index list directly and never used the result. Its only
    // remaining consumer was the emptiness test, which is a fold over the aggregates rather than a
    // sort of every column in the batch — for a 4,096-decision minibatch that is hundreds of
    // thousands of elements sorted, twice per minibatch, four times per update, to answer a
    // question with a yes/no answer. Reported by an independent review of the training throughput.
    let mut per_option: Vec<Vec<(i64, f32)>> = Vec::with_capacity(options.len());
    let mut empty = true;
    for (columns, values) in options {
        let pairs = ordered_pairs(columns, values, capacity)?;
        let mut aggregated: Vec<(i64, f32)> = Vec::with_capacity(pairs.len());
        for (column, value) in pairs {
            match aggregated.last_mut() {
                Some((last, sum)) if *last == column => *sum += value,
                _ => aggregated.push((column, value)),
            }
        }
        empty &= aggregated.is_empty();
        per_option.push(aggregated);
    }

    if empty {
        // Every option was empty — a decision whose every feature was out of vocabulary. The zero
        // rows are the answer, not an error: the same contract as `gather_reduce`.
        return Ok(Tensor::zeros([rows, width], (Kind::Float, table.device())));
    }

    // One host-to-device move per gather, not per option: the flat buffers are assembled on the
    // host and transferred once. Per-decision transfers would dominate a GPU run.
    let device = table.device();

    // One fused embedding-bag: gather, scale and reduce in a single kernel.
    //
    // # Why not do it by hand
    //
    // Both hand-rolled shapes hit a wall. A dense `[options, distinct]` combination matrix is fine
    // at inference scale (6.2 options against 131.9 distinct rows) and becomes 34 million floats for
    // a training micro-batch — measured at 0.11x, slower than not batching. Replacing it with a
    // segment sum removes that matrix but materialises a `[total_entries, width]` intermediate
    // instead, which for a 512-decision micro-batch is around 268 MB and made a CPU epoch time out.
    //
    // `embedding_bag` materialises neither. It walks the index list once and accumulates straight
    // into the output row, which is precisely the "embedding-bag calculation, not a materialized
    // `[N, V_cap]` tensor" §4.3 asks for.
    //
    // `mode = 0` is sum. `sparse = false` keeps a dense gradient, because the Adam here indexes
    // `.grad()` as an ordinary tensor; a sparse gradient is a later change that has to go with an
    // optimiser that understands one.
    let mut flat: Vec<i64> = Vec::new();
    let mut weights: Vec<f32> = Vec::new();
    let mut offsets: Vec<i64> = Vec::with_capacity(options.len());
    for aggregated in &per_option {
        offsets.push(i64::try_from(flat.len()).unwrap_or(0));
        for (column, value) in aggregated {
            flat.push(*column);
            weights.push(*value);
        }
    }

    // An empty bag is legal — a decision whose every feature was out of vocabulary — but
    // `embedding_bag` will not accept an empty index list at all, so that case is handled above.
    let indices = Tensor::from_slice(&flat).to_device(device);
    let offsets = Tensor::from_slice(&offsets).to_device(device);
    let per_sample = Tensor::from_slice(&weights).to_device(device);
    let (out, _, _, _) = Tensor::embedding_bag(
        table,
        &indices,
        &offsets,
        false,
        0,
        false,
        Some(&per_sample),
        false,
    );
    Ok(out)
}

/// A dense `[n, a] × [a, b]` product, for the hidden layer and the readouts.
pub fn matmul(left: &Tensor, right: &Tensor) -> Tensor {
    left.matmul(right)
}

/// A total order over `f32` bit patterns, for breaking ties among duplicate columns.
///
/// Flip the sign bit for non-negative values and invert everything for negative ones: the
/// resulting `u32` orders exactly as the float does, and unlike `partial_cmp` it is *total*, which
/// is what a sort comparator needs. Non-finite values never reach it.
const fn total_order_key(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 == 0 {
        bits ^ 0x8000_0000
    } else {
        !bits
    }
}

/// A tensor's values as `f32`.
///
/// Fallible on purpose. An earlier version returned `unwrap_or_default()`, so an unsupported dtype
/// or a failed conversion produced an empty vector that every downstream assertion accepted as a
/// legitimately empty tensor (F-M09-025-4). A helper documented for assertions and evidence is the
/// last place a silent empty result belongs.
///
/// # Errors
/// [`TensorError::Conversion`] carrying the underlying reason.
pub fn to_vec(tensor: &Tensor) -> Result<Vec<f32>, TensorError> {
    Vec::<f32>::try_from(tensor.contiguous().view([-1]))
        .map_err(|error| TensorError::Conversion(error.to_string()))
}

/// [`to_vec`], panicking on a conversion failure. For tests and diagnostics only.
///
/// # Panics
/// If the conversion fails — loudly, which is the point.
#[must_use]
pub fn to_vec_or_panic(tensor: &Tensor) -> Vec<f32> {
    to_vec(tensor).expect("tensor converts to f32 values")
}

#[cfg(test)]
mod emptiness_tests {
    use super::*;

    fn table() -> Tensor {
        // Row `i` is the constant `i`, so a gathered sum is legible as arithmetic rather than noise.
        let width = 4i64;
        let capacity = 8i64;
        let mut values: Vec<f32> = Vec::new();
        for row in 0..capacity {
            for _ in 0..width {
                values.push(f32::from(u8::try_from(row).expect("a small fixture row")));
            }
        }
        Tensor::from_slice(&values).view([capacity, width])
    }

    #[test]
    fn a_batch_in_which_every_option_is_empty_gathers_zero_rows() {
        // The emptiness test that replaced the sorted `distinct` vector. Its contract is the same
        // as `gather_reduce`'s: a decision whose every feature fell out of vocabulary is answered
        // with zero rows, not an error.
        let table = table();
        let empty: [i64; 0] = [];
        let weights: [f32; 0] = [];
        let options: Vec<(&[i64], &[f32])> = vec![
            (empty.as_slice(), weights.as_slice()),
            (empty.as_slice(), weights.as_slice()),
        ];
        let out = gather_reduce_batch(&table, &options).expect("an empty batch is legal");
        assert_eq!(out.size(), vec![2, 4]);
        let values = to_vec(&out).expect("readable");
        assert!(
            values.iter().all(|value| *value == 0.0),
            "an all-empty batch produced {values:?}"
        );
    }

    #[test]
    fn an_empty_option_keeps_its_own_row_and_leaves_its_neighbours_alone() {
        // The risk in folding emptiness per option rather than over the whole batch is an
        // off-by-one in which a zero row displaces a real one. The empty option sits in the middle
        // precisely so that a shift in either direction shows up.
        let table = table();
        let empty: [i64; 0] = [];
        let no_weights: [f32; 0] = [];
        let first: [i64; 2] = [1, 2];
        let first_weights: [f32; 2] = [1.0, 1.0];
        let last: [i64; 1] = [5];
        let last_weights: [f32; 1] = [2.0];
        let options: Vec<(&[i64], &[f32])> = vec![
            (first.as_slice(), first_weights.as_slice()),
            (empty.as_slice(), no_weights.as_slice()),
            (last.as_slice(), last_weights.as_slice()),
        ];
        let out = gather_reduce_batch(&table, &options).expect("a mixed batch is legal");
        let values = to_vec(&out).expect("readable");
        assert_eq!(out.size(), vec![3, 4]);
        // Row 1 + row 2 = 1 + 2 = 3 in every column.
        assert!(
            values[0..4].iter().all(|value| (value - 3.0).abs() < 1e-6),
            "{values:?}"
        );
        // The empty option: zeros, and still in position one.
        assert!(values[4..8].iter().all(|value| *value == 0.0), "{values:?}");
        // Row 5 scaled by 2 = 10.
        assert!(
            values[8..12]
                .iter()
                .all(|value| (value - 10.0).abs() < 1e-6),
            "{values:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: i64 = 20_260_821;

    #[test]
    fn inference_stays_on_cpu_whether_or_not_cuda_is_linked() {
        // This assertion used to be `!backend.cuda` — CUDA must be *absent*. That was the right
        // tripwire while the branch linked a CPU-only libtorch: a build that silently acquired one
        // could produce a different decision, and M09-025 wanted that to fail loudly.
        //
        // M10-037 links a CUDA build on purpose, so absence is no longer the invariant. The
        // invariant §7.1 actually states is that CUDA never selects an action, and that is what is
        // checked here instead — a **stronger** claim than the old one, because it still holds in
        // the configuration the old test simply could not run in.
        assert_eq!(inference_device(), Device::Cpu);

        // And the tensors a decision is actually built from land there.
        let table = zeros_table(64, 8);
        assert_eq!(table.device(), Device::Cpu);
        let gathered = gather_reduce(&table, &[1, 2], &[1.0, 1.0]).expect("gather");
        assert_eq!(
            gathered.device(),
            Device::Cpu,
            "a gather escaped to another device"
        );
        let batched =
            gather_reduce_batch(&table, &[(&[1, 2][..], &[1.0, 1.0][..])]).expect("batch");
        assert_eq!(
            batched.device(),
            Device::Cpu,
            "a batched gather escaped to another device"
        );
    }

    #[test]
    fn the_optimizer_device_refuses_cuda_it_does_not_have() {
        // Fail-closed rather than falling back: a run that believed it was on CUDA and quietly was
        // not would record a device in its manifest that never executed a gradient.
        assert_eq!(
            OptimizerDevice::Cpu.resolve().expect("cpu resolves"),
            Device::Cpu
        );
        match OptimizerDevice::Cuda.resolve() {
            Ok(device) => assert!(
                tch::Cuda::is_available(),
                "CUDA resolved to {device:?} on a build without it"
            ),
            Err(TensorError::NoCuda) => assert!(!tch::Cuda::is_available()),
            Err(other) => panic!("wrong error: {other}"),
        }
    }

    #[test]
    fn the_backend_reports_what_is_linked() {
        // Recorded, not forbidden. A manifest that says which device was available is what makes a
        // later comparison between a CPU run and a CUDA run interpretable.
        let backend = backend();
        assert_eq!(backend.cuda, tch::Cuda::is_available());
        assert_eq!(
            backend.cuda_devices,
            usize::try_from(tch::Cuda::device_count()).unwrap_or(0)
        );
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
        // Non-vacuity is deliberately **not** checked here by mutating the thread count. It is a
        // process-global setting and this process is shared with every other test in the binary,
        // so a temporary change races anything that reads it. `tests/interop.rs` does it in a
        // process of its own.
        assert_eq!(reported.inter_op_threads, 1, "both counts are enforced");
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

        let first = to_vec_or_panic(&gather_reduce(&table, &columns, &values).expect("reduces"));
        let second = to_vec_or_panic(&gather_reduce(&table, &columns, &values).expect("reduces"));
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

        let forward = to_vec_or_panic(&gather_reduce(&table, &columns, &values).expect("reduces"));
        let mut reversed: Vec<(i64, f32)> = columns
            .iter()
            .copied()
            .zip(values.iter().copied())
            .collect();
        reversed.reverse();
        let reversed_columns: Vec<i64> = reversed.iter().map(|(c, _)| *c).collect();
        let reversed_values: Vec<f32> = reversed.iter().map(|(_, v)| *v).collect();
        let backward = to_vec_or_panic(
            &gather_reduce(&table, &reversed_columns, &reversed_values).expect("reduces"),
        );

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
        let summed =
            to_vec_or_panic(&gather_reduce(&table, &[1, 1], &[0.5, 0.25]).expect("reduces"));
        assert_eq!(summed, vec![1.5, 1.5, 1.5]);
    }

    #[test]
    fn duplicate_contributions_are_summed_in_a_canonical_order() {
        // F-M09-025-3. Sorting by column alone left duplicates in caller order, and f32 addition
        // is not associative: a large positive, a large negative and a small value summed in a
        // different order give a different last bit, and a softmax over near-tied logits turns a
        // last bit into a different action. Every permutation must give bit-identical output.
        configure_deterministic(SEED).expect("configured");
        let table = zeros_table(4, 3);
        let mut row = table.narrow(0, 1, 1);
        let _ = row.fill_(1.0);

        let contributions: [f32; 3] = [1.0e7, -1.0e7, 0.125];
        let permutations = [
            [0usize, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        let mut results = Vec::new();
        for order in permutations {
            let values: Vec<f32> = order.iter().map(|i| contributions[*i]).collect();
            let reduced = gather_reduce(&table, &[1, 1, 1], &values).expect("reduces");
            results.push(to_vec_or_panic(&reduced));
        }
        // Non-vacuity: these values really are order-sensitive in f32. Summed left to right the
        // small term is lost against the large ones; the canonical order is what makes the answer
        // one answer rather than whichever the caller happened to produce.
        let naive_forward = (contributions[0] + contributions[1]) + contributions[2];
        let naive_reverse = (contributions[2] + contributions[1]) + contributions[0];
        assert!(
            (naive_forward - naive_reverse).abs() > 0.0,
            "the fixture is not order-sensitive, so the test proves nothing"
        );

        for (index, result) in results.iter().enumerate() {
            assert_eq!(
                *result, results[0],
                "permutation {index} produced a different sum"
            );
        }
    }

    #[test]
    fn a_failed_conversion_is_distinguishable_from_an_empty_tensor() {
        // F-M09-025-6. Making `to_vec` fallible only matters if a failure is distinguishable from a
        // legitimately empty tensor, so both sides are exercised — and what the underlying
        // conversion actually rejects is recorded rather than assumed.
        configure_deterministic(SEED).expect("configured");

        // The real failure mode in `tch`: a rank-2 tensor cannot become a flat vector.
        let rank_two = Tensor::from_slice(&[1.0f32, 2.0, 3.0, 4.0]).view([2, 2]);
        let raw = Vec::<f32>::try_from(rank_two.shallow_clone());
        assert!(
            raw.is_err(),
            "the underlying conversion accepted a rank-2 tensor"
        );

        // `to_vec` flattens first, so it succeeds on the same tensor — which is what makes that
        // `.contiguous().view([-1])` load-bearing rather than decorative.
        assert_eq!(
            to_vec(&rank_two).expect("to_vec flattens"),
            vec![1.0, 2.0, 3.0, 4.0]
        );

        // And a genuinely empty tensor converts to an empty vector: the value a failure used to be
        // silently confused with.
        let empty = Tensor::from_slice(&[] as &[f32]);
        assert_eq!(
            to_vec(&empty).expect("an empty tensor converts"),
            Vec::<f32>::new()
        );

        // A dtype mismatch is *not* a failure: `tch` converts i64 to f32. Recorded because it is
        // the case the finding suggested, and asserting it would have been wrong.
        assert_eq!(
            to_vec(&Tensor::from_slice(&[1_i64, 2])).expect("tch converts kinds"),
            vec![1.0, 2.0]
        );
    }

    #[test]
    fn a_non_finite_feature_value_is_refused() {
        // A NaN has no place in a total order and none in a logit; an infinity poisons every sum
        // downstream. Both are refused rather than propagated into a softmax that would return a
        // plausible-looking distribution.
        configure_deterministic(SEED).expect("configured");
        let table = zeros_table(8, 4);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(
                matches!(
                    gather_reduce(&table, &[1], &[bad]),
                    Err(TensorError::NotFinite { .. })
                ),
                "{bad} was accepted"
            );
        }
        // And a finite value still passes, so the guard is not rejecting everything.
        assert!(gather_reduce(&table, &[1], &[1.5]).is_ok());
    }

    #[test]
    fn an_empty_vector_contributes_the_zero_row() {
        // Every feature out of vocabulary is a legal position, not an error and not a panic.
        configure_deterministic(SEED).expect("configured");
        let table = Tensor::rand([16, 8], (Kind::Float, Device::Cpu));
        let empty = gather_reduce(&table, &[], &[]).expect("an empty vector is legal");
        assert_eq!(to_vec_or_panic(&empty), vec![0.0; 8]);
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
        assert_eq!(to_vec_or_panic(&table), vec![0.0; 64 * 16]);
        let reduced = gather_reduce(&table, &[63], &[1.0]).expect("reduces");
        assert_eq!(
            to_vec_or_panic(&reduced),
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
