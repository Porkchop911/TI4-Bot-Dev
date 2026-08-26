# Training throughput analysis — 2026-08-26

## Scope

This is a non-invasive observation of Claude's live `run-001` PPO training. No source,
configuration, checkpoint, or running-process state was changed during this review.

Observed command:

```text
cargo run --release -p ti4-mlp --example ppo_update -- \
  --updates 3000 --report-every 100 --device cuda \
  --bundle out/checkpoints/mlp-critic/checkpoint-shared-1 \
  --out out/checkpoints/run-001
```

The executable was PID 37544 (`target-cuda/release/examples/ppo_update.exe`) on an
RTX 3090. Measurements below are snapshots, not a controlled benchmark. Other desktop
processes were using the GPU, so the phase analysis uses Windows' process-specific GPU
engine counter where possible rather than attributing all `nvidia-smi` utilization to
training.

## Observed throughput

At a 64-update snapshot of `out/train_run001.log`:

| Quantity | Mean | Observed range |
|---|---:|---:|
| Decisions/update | 134,769 | 124,693–146,764 |
| Rollout/update | 4.808 s | 4.2–5.4 s |
| Optimizer/update | 7.350 s | 6.8–8.2 s |
| Total/update | 12.156 s | 11.2–13.5 s |

At this rate, 3,000 updates take approximately 10.13 hours. Optimizer work accounts for
about 60.5% of update time and rollout for about 39.5%.

The workload alternates between two readily distinguishable phases:

| Phase | Process CPU | Process GPU | Memory behavior | Interpretation |
|---|---:|---:|---:|---|
| Parallel rollout | commonly 84–91%; sampled phase mean 75.8% | normally 0% | working set rises toward 6.5 GiB | CPU-bound simulation and feature extraction |
| PPO optimizer | commonly 2.9–3.3%; sampled phase mean 5.2% | steady intervals about 35–41% | about 6.6 GiB working set / 13.0 GiB private | GPU is fed by roughly one host core and is under-saturated |

The sampled phase means contain boundary samples, which explains rollout's non-zero
10.1% GPU mean and optimizer's lower 31.3% GPU mean. Away from boundaries the separation
is clean: rollout uses the CPUs and no GPU; optimization uses one host core and only a
fraction of the GPU. Approximately 4.3 GiB of 24 GiB VRAM was occupied, leaving substantial
capacity headroom. Total-GPU samples also showed an `EACefSubProcess` consuming GPU time;
that is external contention, but it does not explain the training process's own low GPU
occupancy.

## Findings and recommended order

### 1. Profile a short clone before changing the optimizer

Run one or two updates under Nsight Systems (or equivalent CUDA timeline tooling), while
leaving this long run untouched. Add/collect separate timings for:

- host construction of minibatch vectors and ragged indices;
- host-to-device copies;
- actor sparse gather and trunk;
- critic forward pass;
- backward;
- Adam step.

The live signature suggests gaps between relatively small kernels, memory-bound sparse
gathers, or synchronous host-to-device preparation—not an RTX 3090 compute ceiling. A
timeline will distinguish these before a large refactor is selected.

### 2. Share one inference actor among the six bots in a game

`examples/ppo_update.rs::play_one` currently calls `actor.inference_copy()` for each of the
six `MlpBot`s. The outer rollout code already creates another inference copy for each Rayon
chunk. This can mean hundreds of deep model copies per update and plausibly explains much
of the 6–13 GiB process-memory footprint and phase-boundary allocation churn.

Keep a worker-local immutable actor and share it among that worker's six seats. Because the
tensor-backed actor is not `Sync`, this should be a same-thread ownership design (for
example, an `Rc`-style game-local handle), not an `Arc` shared across Rayon workers. This
preserves behavior and should reduce copies, allocation traffic, and memory pressure.

### 3. Canonicalize sparse options once when freezing a PPO batch

Every call to `ti4_tensor::gather_reduce_batch` invokes `ordered_pairs`, aggregates duplicate
columns, and builds/sorts/deduplicates a `distinct` vector. PPO revisits the same frozen
decisions for four epochs, so invariant sparse structure is repeatedly rebuilt.

Canonicalize each `SparseOption` once at batch freeze and retain the deterministic ordering
and duplicate-summation rules. The current `distinct` vector is subsequently used only to
test emptiness; the nearby documentation still describes a distinct-row gather that the
present embedding-bag implementation no longer performs. Correcting that mismatch and
removing the redundant sort/dedup are safe candidates after a focused equivalence test.

### 4. Prepack invariant minibatch data and reduce repeated transfers

`ppo.rs::score_minibatch` reconstructs host-side `parts`, `heads`, row indices, ragged gather
metadata, padding/chosen indices, behavior values, advantages, entropy coefficients, critic
rows, and returns on every minibatch of every epoch, then creates and transfers tensors.
Most underlying values are fixed for the update; only the shuffled selection changes.

Store a canonical packed representation when `Batch` is frozen, upload invariant tensors
once per update where practical, and select shuffled rows/segments without rebuilding the
semantic data. A producer thread with pinned double buffers could prepare the next batch
while the GPU executes the current one, provided operation and optimizer ordering remain
unchanged. This directly targets the one-host-core/underfed-GPU pattern.

### 5. Improve rollout tail balance without sharing actors across threads

The current rollout partitions 96 jobs into static chunks using
`jobs.len().div_ceil(rayon::current_num_threads())`. On a 32-thread host this is three games
per worker. Variable game lengths can leave a tail in which only a few cores remain busy.

A worker loop that owns one non-`Sync` actor and pulls the next job from a shared queue would
retain one actor per worker while dynamically balancing games. First record per-worker finish
times; if the tail is small, actor-copy removal is the higher-priority change.

### 6. Remove avoidable environmental contention

For controlled throughput runs, close GPU-accelerated launchers/browser surfaces such as the
observed EA subprocess, avoid simultaneous builds, and use the machine's performance power
profile. This is the only immediate no-code opportunity. Its benefit is probably modest,
because the process-specific counter independently shows training at only about 35–41% GPU.

## Changes requiring an explicit protocol decision

These may improve throughput but are not implementation-only optimizations:

- **Larger minibatches.** VRAM headroom suggests 8,192 or 16,384 decisions may raise GPU
  occupancy and reduce the current number of optimizer steps. It also changes Adam update
  frequency and therefore the fixed PPO protocol; benchmark only under a pre-registered
  revision.
- **Mixed precision.** This may benefit the 3090 but changes numerical behavior and requires
  stability and equivalence gates.
- **Overlapping next-update rollout with optimization.** Ordinary overlap would collect the
  next trajectories under a stale policy and violate the current on-policy contract. Do not
  treat it as a free pipeline optimization.
- **Changing epoch count.** This changes the learning algorithm, not merely throughput.

## Throughput ceilings

Using the observed 4.808 s rollout and 7.350 s optimizer means:

| Hypothetical improvement | New update time | Overall speedup |
|---|---:|---:|
| Optimizer 30% faster | 10.462 s | 1.16x |
| Optimizer 2x faster | 8.483 s | 1.43x |
| Rollout 2x faster | 9.754 s | 1.25x |
| Both phases 2x faster | 6.079 s | 2.00x |

Accordingly, optimizer feeding is the first throughput target, but rollout allocation and
load balancing must also improve to go materially beyond roughly 1.4x overall.

## Suggested measurement gate for each optimization

Use an idle machine and a committed binary, run the same fixed seeds and checkpoint, retain
per-phase timings, and compare at least 30 updates after warm-up. Alongside wall time, verify
identical rollout fingerprints and decision counts for semantics-preserving changes. For
optimizer packing changes, also compare losses, parameter deltas, and optimizer state under
the repository's accepted numerical tolerance. Measure process-specific GPU utilization;
total desktop GPU utilization is not sufficient evidence.

## Follow-up after `944bc53`

The first optimization pass reduced the measured update mean from 12.10 s to 9.72 s. The new
split is approximately 3.99 s rollout plus 5.73 s optimization. The following are the next
semantics-preserving candidates, ordered by implementation effort and confidence.

### A. Finish canonicalizing sparse options at batch freeze — easiest likely win

The removed global `distinct` sort was valuable, but `gather_reduce_batch` still calls
`ordered_pairs` for every option. It copies and sorts the same policy and critic columns every time
the frozen batch is revisited across four epochs.

At `Batch::freeze`, validate, deterministically sort, and duplicate-fold every policy option and
critic vector once. Give the batched gather a checked canonical fast path that appends already
sorted unique pairs directly to its flat embedding-bag arrays. Retain the general public path for
untrusted callers. Verify bit-identical gather output, gradients, losses, and parameter deltas.

This mostly removes host work rather than adding GPU work, but that is precisely useful when the
GPU is being paced by one host core. It is smaller and safer than redesigning the whole minibatch
format.

### B. Split `prepare_minibatch` from GPU scoring and prepare one batch ahead

Host packing and GPU execution are currently serialized inside `score_minibatch`. A bounded
single-item producer can construct the next minibatch's vectors while the current minibatch runs
forward, backward, and Adam. Consumption must remain in the fixed shuffled order, so this does not
alter optimizer semantics.

Start with ordinary owned buffers and measure. This alone may hide most remaining sort/vector work.
Only then add pinned staging tensors; `tch 0.22` exposes `Tensor::pin_memory` and
`Tensor::to_device_(..., non_blocking, ...)`, but pinned buffers must remain alive until their
asynchronous copy completes.

### C. Consolidate the minibatch uploads

In shared-critic mode, one minibatch currently performs approximately sixteen separate
`Tensor::from_slice(...).to_device(device)` paths:

- three for policy embedding-bag indices, offsets, and weights;
- two for per-option head and faction rows;
- three for padding/gather/chosen metadata;
- three for behavior log-probability, advantage, and entropy coefficient;
- three for critic embedding-bag indices, offsets, and weights;
- one critic row tensor and one return tensor.

At roughly 33 minibatches × 4 epochs, this is about 2,100 host-to-device copy calls per update.
Pack integer metadata into one or a few contiguous staging tensors and floating constants into
another, upload once, then obtain typed/narrowed views on device. The total byte count remains, but
copy setup and host stalls fall sharply. A more complete version packs an entire epoch, uploads each
field once, and narrows per-minibatch ranges; that is higher payoff but more engineering.

Pinned/non-blocking transfer alone is not enough if the code immediately waits on the same work.
It belongs with producer prepacking and retained staging-buffer lifetimes.

### D. Remove the host-built padded mask

The PPO path uploads both a full rectangle of gather indices and a full Boolean padding rectangle.
The distillation path already demonstrates the alternative shape: allocate a device rectangle
filled with `-inf` and `index_copy` only real logits into their slots. Entropy can use `xlogy` or an
on-device `where(probability > 0, p * log_p, 0)` so zero-probability padding contributes exactly
zero without uploading a mask.

This removes one large host vector, one transfer, and the gather-then-masked-fill sequence. It needs
explicit tests for real-logit underflow, padded entropy, gradients through `index_copy`, and NaN
refusal so the optimization does not hide genuine non-finite values.

### E. Avoid materializing the zero-padded identity matrix

`trunk_mixed` gathers a 16-wide identity embedding, allocates a zero tensor for the remaining
240 columns, concatenates the two into `[options, 256]`, and then adds the full matrix. For a large
policy option batch this is tens of megabytes of zeros and memory traffic per forward pass.

Compute the first 16 preactivation columns with `x + identity + b1`, the remaining columns with
`x + b1`, concatenate once, and apply ReLU. Preserve the existing floating-point addition order in
the first 16 columns and test exact forward/gradient equivalence. This is a compact experiment, but
benchmark it: extra narrow/cat dispatches can offset the bandwidth saved.

### F. Small, easy host cleanups

These are unlikely to move wall time individually but are safe to bundle behind measurements:

- reserve exact or estimated capacity for `parts`, `heads`, `rows`, embedding-bag `flat`, and
  `weights`;
- precompute f32 behavior values, advantages, returns, entropy coefficients, and validated row/head
  indices at batch freeze instead of converting them four times;
- reuse the per-decision faction-row tensor between the policy metadata and critic path where the
  layout permits it;
- stop reconstructing shallow parameter-handle vectors on every Adam step if a retained validated
  handle list can preserve the leaf/gradient checks.

### Why the GPU cannot simply work during rollout

With the post-change split, optimization occupies only about 59% of wall time. Even if optimizer
GPU utilization were 100%, whole-run GPU utilization would therefore top out near 59% while rollout
remains CPU-only. The observed approximately 40% optimizer utilization predicts only about 24%
whole-run utilization, matching the apparent low duty cycle.

Keeping the GPU active during rollout would require either centrally batching inference across
many concurrently paused games or collecting the next update under a stale policy while the current
one optimizes. The first conflicts with the current CPU-inference boundary and is a substantial
scheduler redesign; the second changes on-policy PPO semantics. Neither is an easy throughput fix.

### Expected ceiling from this round

At 3.99 s rollout and 5.73 s optimization, making the optimizer 30% faster produces approximately
8.40 s/update (1.16x overall); halving optimizer time produces approximately 6.86 s/update (1.42x).
Further gains then require rollout improvement too.
