use super::*;
use ti4_tensor::{TensorError, total_order_key};
pub fn gather_profiled(
    profile: &Rc<RefCell<Profile>>,
    table: &Tensor,
    options: &[(&[i64], &[f32])],
) -> Result<Tensor, TensorError> {
    let started = Instant::now();
    let (capacity, width) = {
        let size = table.size();
        (size[0], size[1])
    };
    let rows = i64::try_from(options.len()).unwrap_or(0);

    // Straight into the embedding-bag arrays, in the fixed order, with duplicates summed as they
    // are met.
    //
    // An earlier version built a `Vec<Vec<(i64, f32)>>` of aggregates and then copied it into the
    // flat arrays. That is one heap allocation per option — about 24,000 per PPO minibatch — plus
    // another for the sorted pairs, to produce data that is written once and read once.
    //
    // It also sorted every option on every visit. PPO revisits a frozen batch four times per
    // update, and `Batch::freeze` now leaves each option's columns strictly increasing, so the
    // sorted path below simply appends. The unsorted fallback is kept in full for callers that
    // have not canonicalised — inference reaches here straight from the projection — and reuses one
    // scratch buffer instead of allocating per option.
    let total: usize = options.iter().map(|(columns, _)| columns.len()).sum();
    let mut flat: Vec<i64> = Vec::with_capacity(total);
    let mut weights: Vec<f32> = Vec::with_capacity(total);
    let mut offsets: Vec<i64> = Vec::with_capacity(options.len());
    let mut scratch: Vec<(i64, f32)> = Vec::new();

    for (columns, values) in options {
        offsets.push(i64::try_from(flat.len()).unwrap_or(0));
        let start = flat.len();

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

        // Strictly increasing means already in `ordered_pairs` order with no duplicates left to
        // fold, so the aggregate is the input. Checking that is one linear pass over data the
        // validation above has already walked; sorting to discover the same thing is not.
        let sorted = columns.windows(2).all(|pair| pair[0] < pair[1]);
        if sorted {
            flat.extend_from_slice(columns);
            weights.extend_from_slice(values);
            continue;
        }

        scratch.clear();
        scratch.extend(columns.iter().copied().zip(values.iter().copied()));
        scratch.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| total_order_key(left.1).cmp(&total_order_key(right.1)))
        });
        for (column, value) in scratch.iter().copied() {
            // Fold only within this option: `start` is where its entries begin, so the last
            // element of a *previous* option can never be mistaken for a duplicate.
            if flat.len() > start && flat[flat.len() - 1] == column {
                let last = weights.len() - 1;
                weights[last] += value;
            } else {
                flat.push(column);
                weights.push(value);
            }
        }
    }

    if flat.is_empty() {
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
    // into the output row.

    // An empty bag is legal — a decision whose every feature was out of vocabulary — but
    // `embedding_bag` will not accept an empty index list at all, so that case is handled above.
    profile
        .borrow_mut()
        .add("gather_prepare", started.elapsed());
    let started = Instant::now();
    let indices = Tensor::from_slice(&flat).to_device(device);
    let offsets = Tensor::from_slice(&offsets).to_device(device);
    let per_sample = Tensor::from_slice(&weights).to_device(device);
    profile
        .borrow_mut()
        .add("tensor_construct", started.elapsed());
    let started = Instant::now();
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
    profile.borrow_mut().add("embedding_bag", started.elapsed());
    Ok(out)
}
