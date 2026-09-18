# Training performance optimization: execution guide

Date: 2026-09-11. Owner request: prepare detailed instructions while training continues; execute
benchmarks only after it pauses. This document is the only edit made for this request. No agents
were spawned, no profiling was attached and no training was interrupted.

## 1. Objective, authority and boundaries

Improve end-to-end training throughput without degrading learning. The ultimate metric is VP gained
per wall-clock hour on fixed evaluation workloads. Hardware utilization is explanatory telemetry,
not the acceptance target. A faster optimizer that changes samples, gradients or exploration is a
different learning experiment and must be labeled as such.

Work one numbered package at a time. Do not infer permission to stop, resume, replace or overwrite the
user's trainer/checkpoints. The expected pause in roughly 30 minutes is not evidence it has stopped.
Verify process state before every benchmark. No CUDA benchmarks or heavy CPU work while it runs.
Do not launch subagents unless separately requested. Follow AGENTS.md and preserve other work.

Two tracks:

- **Track A: preserve behavior.** Timing, input preparation, buffer reuse and bounded CPU preparation
  overlap, with unchanged CPU rollout inference and PPO sample/update order.
- **Track B: explicitly behavior-affecting research.** GPU rollout inference, changed batch shapes,
  mixed precision, changed minibatch size, fused arithmetic or asynchronous rollout. CPU inference is
  currently required by the specification. Design and diagnose this track, but get explicit authority
  for a GPU-inference specification exception before implementing/running that prototype. Discussion
  of its potential is not production cutover authorization.

Do not change reward weights, observation semantics, option IDs/order, vocabulary, BTreeMap order,
model width/depth, game horizon, RNG consumption or training temperature as a performance fix.
Do not regenerate fixtures to accommodate a mismatch. Never reset/stash/clean, stage or commit under
this handoff. Use the existing repository checkout and existing `target-cuda` only. Do not create
worktrees, repository copies, alternate build-target folders, or duplicate libtorch/CUDA installations,
inside or outside the repository. Previous duplication consumed roughly 80 GB. Ask the user before
creating any output folder, even inside the repository; this document does not grant that approval.
Reuse an existing user-approved output location and avoid overwriting unrelated files. No recursive
cleanup of worktrees or artifact directories; older sessions had junction-related data loss. Report
disk-space issues instead of deleting or duplicating installations.

## 2. Exact observed baseline

Read-only inspection found a clean `main` checkout at
`0bd2e754cc96a872c9aae41cee51f9fcbf5579a6`. Recheck this; it is not a durable promise of cleanliness.

| Item | Observed value |
|---|---|
| Repository | `D:\Projects\ti4-engine-rs` (the configured C: directory is not the working repository) |
| Live trainer PID | 28324 at inspection; rediscover by executable path, never assume PID persists |
| Executable | `out/blank-shaped-4layers/ppo_update.exe` |
| Executable SHA-256, verified | `c9d29c39bf707f520bedec832cbf75eb3f65d6eb8b0475849662f158f75cce2a` |
| Output/log | `out/blank-shaped-4layers/continue/train.out.log` |
| Resume source | `out/blank-shaped-4layers/checkpoints/checkpoint-33616` |
| Architecture | schema 8, width 256, capacity 20480, shared critic, two residual blocks after trunk |
| GPU | RTX 3090, 24,576 MiB total; about 8,435 MiB device-wide use during sampling |
| CPU | 32 logical processors |
| Rollout | 16 seeds × six rotations = 96 games/update; four rounds; temperature 2.5 |
| PPO | four epochs; minibatch 4096; learning rate 0.0003; CUDA optimizer |
| Seed base | 2400204000 |
| Checkpoint cadence | 100 updates; continuation optimizer counter restarted from zero |

Read both `out/blank-shaped-4layers/provenance.txt` and `continue/provenance.txt`, and the actual
process command line. Command flags observed:

```text
--stage 2 --rounds 4 --temperature 2.5 --movement-entropy 0.05 --entropy-final 1
--learning-rate 3e-4 --seed-base 2400204000 --updates 10000 --report-every 100
--device cuda --waste-penalty 1 --fleet-weight 0.03 --tech-weight 0.1
--strategy-diversity-weight 1 --r1-bonus 2 --fleet-hoard-penalty 1
--zero-fleet-penalty 10 --trade-goods-hoard-weight 0.1 --styx-bonus 1
```

This is provenance, NOT a command to relaunch the long production run. Use small explicit workloads
and distinct filenames in an existing user-approved output folder for tests. If the trainer creates
output/checkpoint subfolders automatically, obtain approval for that exact folder layout and bounded
artifact size before launching it. Do not select a checkpoint that is still being written.

### Live measurements (observations, not benchmark results)

Sampling around 20:24–20:25 local time:

- Rollout: trainer used roughly 94–97% of total logical CPU capacity; GPU mostly 1–11% utilization.
- Optimization: trainer used roughly 3% of total CPU capacity (about one logical core); GPU reported
  48–62% utilization and typically 270–277 W after ramp-up. GPU memory-busy readings were about
  30–40%; this counter is not percentage of peak bandwidth. GPU counters are device-wide.
- A recent 20-update log window averaged 14.04 s rollout and 7.53 s optimization, approximately
  21.57 s combined. Later updates 259–261 showed 12.8–13.8 s rollout and 7.0–7.1 s optimization.
- `Batch::freeze` occurs before the optimizer timer; printed total adds only rollout and optimizer
  times. Copying, assembly and reporting can add time outside those counters. Measure true wall time.

A busy single CPU core during optimization is consistent with host preparation or submission limits,
but does not prove either. Low utilization may also reflect short kernels, memory behavior or waits.
Do not convert 60% GPU utilization into a promised 40% speedup.

### Prior results that constrain proposals

Read `INFERENCE_OPTIMIZATION_2026-09-08.md` and `OPTIMIZATION_ATTEMPT_2026-09-09.md`.
The older model's 32-worker profile attributed 46.63% of decider time to actor feature extraction,
5.75% to vocabulary conversion, 9.25% to critic features and 6.37% to progress/recording.
These are summed worker times, not additive end-to-end wall shares. Dense hidden matmul was only
2.48% of decider time; embedding reduction was 18.48%. The current residual model needs a new profile.

An earlier cross-game batching prototype failed bitwise logits. Read its evidence before repeating it.
Moving forward computation to GPU does not move CPU feature extraction or the engine automatically.
Our conversational suggestion that GPU inference might be the larger opportunity is a hypothesis,
not established by the old inference-residual percentage.

The older optimization report used equal decision counts as an equivalence check. That is insufficient:
different choices can produce equal counts. Require ordered choices, events and final-state hashes.
It also observed post-gradient CUDA divergence; repeatability must be characterized before using
bitwise training comparisons as an acceptance gate.

## 3. Relevant code and findings already established

Inspect the recorded commit, not just whichever source happens to be checked out:

| File / symbols | What to inspect |
|---|---|
| `crates/ti4-mlp/examples/ppo_update.rs` | CPU actor copies, rollout chunks, record assembly, `Batch::freeze`, optimizer timing and report totals |
| `crates/ti4-mlp/src/ppo.rs` | `Batch::freeze`, minibatch scoring, epoch shuffle, constant transfers, padded option layout, `drain_epoch` |
| `crates/ti4-mlp/src/lib.rs` | `logits_mixed_parts`, critic `value_batch`, actor/device copying |
| `crates/ti4-tensor/src/lib.rs` | `gather_reduce_batch`: canonical order, flattening, indices/offsets/weights transfers |
| `crates/ti4-mlp/src/distill.rs` | Adam: global gradient norm, host scalar read, clipping, per-parameter operations |
| `crates/ti4-mlp/src/bot.rs` | feature extraction, vocabulary projection, probabilities, seeded sampling, critic and PPO records |

Verified against the recorded source: each minibatch walks CPU records to form sparse option data,
heads/rows, slot indices, chosen indices, temperatures, advantages, behavior log-probabilities, entropy
coefficients and critic inputs/returns. Tensor construction/upload happens inside that path.
The same immutable rollout data is revisited over four differently shuffled epochs.

Telemetry already stays on-device until the end of each epoch. Do not propose redoing that optimization.
Adam deliberately reads one global gradient-norm scalar per step; it both determines clipping and
rejects nonfinite gradients before updating parameters. Do not remove or postpone that rejection to
hide synchronization time. Likewise, changing reduction order or using a fused optimizer is not a
data-layout-only change.

## 4. Package A0 — freeze provenance and establish repeatability

Before work, inspect process state, git status, source HEAD, library paths and the completed checkpoint
manifest. The known PID is only a hint. Confirm no competing CPU benchmarks/builds or CUDA job.
Do not kill a process to make a benchmark possible; report that the machine is occupied.

Use the existing checkout at `D:\Projects\ti4-engine-rs` and its existing `target-cuda` build
directory. Do not create another checkout, worktree or target directory. Ask the user before creating
any output folder; use an existing approved location where possible. Obtain a concrete artifact-size
bound and check free space before generating replay batches, traces or checkpoints. Avoid commands
that implicitly create unapproved subfolders.

If source is dirty, record its provenance with diffs and hashes, including relevant untracked source
files, in the approved output location. Do not copy the repository or its runtimes to capture source
state. Do not discard others' changes or compare against a clean commit with different behavior.
Never use `git_commit` in a bundle as the sole proof of the binary's source. Benchmark baseline and
variant sequentially using the same build directory; preserve each executable under a distinct name
in the approved location and record its source provenance. If this would require reverting or
overwriting someone else's work, stop and report the conflict.

Point `LIBTORCH` explicitly at the existing CUDA libtorch installation and use existing `target-cuda`.
Do not download, extract, copy or install another libtorch/CUDA runtime for this task. CPU and CUDA
builds can stage different DLLs beside executables; verify the DLLs actually used, not just the
environment variable. Reuse the existing compatible DLL location rather than copying a runtime per
variant. Do not overwrite the live trainer or its DLLs, and do not build concurrently with another
user of `target-cuda`. Record executable/DLL/source/bundle/pool hashes, command line, resolved settings,
CPU/GPU, drivers, toolchain and working-tree state.

Reject unknown flags in any new diagnostic harness. Verify production run headers reflect every
requested setting; the historical trainer silently ignored some unknown options.

Choose one completed checkpoint for the whole initial investigation. Restore the same initial weights
and Adam moments/counter for every optimizer A/B. If the production bundle lacks optimizer state,
use identically initialized fresh Adam for both sides, label this limitation, and also test a replay
after several warm-up steps. Never compare a warm optimizer against a cold restart.

Repeat unchanged baseline rollout twice with the same inputs. Record per-game ordered-choice,
ordered-event and final-state hashes, chosen probabilities, critic values and PPO record ordering.
Repeat unchanged baseline optimization on an identical captured batch to determine which results are
bitwise repeatable. Capture actual tensors/state, not only rounded losses.

**Exit:** reproducible input manifest, frozen baseline and documented repeatability envelope. If baseline
diverges, investigate before attributing divergence to a patch. Do not claim an exact training-preserving
optimization if the available comparison cannot demonstrate it.

## 5. Package A1 — measure the full pipeline

Add opt-in diagnostic timings, with instrumentation disabled by default. Measure:

1. Whole update wall time, CPU inference copies and worker setup.
2. Rollout wall time; nested actor features, vocabulary, actor forward, sampling, critic features,
   critic forward and PPO record creation. Cover all six seats and retain summed-vs-wall distinction.
3. Merge/return calculation, freeze, allocation/drop time where material.
4. PPO CPU packing, host-to-device copies, actor/critic kernels, backward, Adam and telemetry.
5. Reporting/checkpoint time separately from ordinary updates.

Use CUDA events or a CPU/CUDA timeline to attribute asynchronous GPU work. Host elapsed time around
a CUDA call usually measures enqueue time, not completed computation. Avoid inserting a synchronize
after every operation: it changes the pipeline. Use a few diagnostic synchronization boundaries and
one proper completion boundary for timed replay; measure uninstrumented performance separately.

Check whether NVIDIA Nsight Systems or an equivalent profiler is available. `nsys` was not found on
PATH during inspection; this does not prove it is not installed. Do not install large tools or change
system settings opportunistically. If unavailable, use bounded event/host timing and state the limits.

After the trainer pauses, capture one representative 96-game rollout with recording enabled. Keep
ragged option counts, sparse lengths, critic inputs, recorded temperatures and full behavior data.
Replay the frozen batch through four epochs. Report real options vs padded cells and worst-option
count per minibatch; do not sort decisions by size to improve padding, since that changes SGD batches.

**Exit:** a budget that reconciles with full wall time, an uninstrumented baseline and one identified
largest avoidable cost. If forward is a small rollout share, lower GPU-inference priority accordingly.

## 6. Package A2 — eliminate repeated immutable preparation

Implement only if A1 shows meaningful host packing or transfer cost.

Start with reusable CPU buffers and cached immutable metadata. Cache per-decision canonical sparse
inputs, option counts, heads, rows, chosen IDs, temperatures, fixed behavior statistics and critic
inputs. Build each epoch's minibatches in the EXACT baseline shuffle order. Account for settings that
vary by update, especially entropy coefficients. Preserve f64-to-f32 conversion points, duplicate
feature handling, stable order and padding layout.

Do not cache learned embeddings, logits, probabilities, critic values produced by the current model,
or autograd graphs across optimizer steps. Weights change after each minibatch. Fixed behavior values
from rollout are a different category and must remain the recorded ones.

Prefer caching ragged data over densifying the entire rollout. Estimate memory before allocating:
feature entries × (index bytes + value bytes), offsets/metadata, critic data, padding, gradients and
activation peaks. The observed 8.4 GiB device use is not a guarantee that 16 GiB is safely available.

First compare buffers/tensors bit-for-bit with the baseline on varied option counts, empty sparse
inputs, duplicates, mixed heads/factions and partial minibatches. Then compare multiple optimizer
steps including losses, gradients, weights and Adam state. Retain malformed-input rejection tests.

**Exit:** verified preparation equivalence plus an isolated replay speedup. No end-to-end speedup
claim yet. Stop if copying or allocations merely move elsewhere and full replay does not improve.

## 7. Package A3 — overlap CPU preparation with GPU computation

Implement after A2, only if the timeline still shows host preparation gaps.

Use one preparation worker and a bounded queue of two minibatches. The worker prepares immutable
CPU data; one existing consumer owns GPU work and Adam. Preserve deterministic epoch/minibatch order
and the baseline order of parameter updates. Do not share mutable tensors unsafely to bypass tch's
Send/Sync restrictions. Have explicit end-of-epoch, cancellation, worker-error and channel-close paths;
errors must propagate, not deadlock or silently drop samples. Join workers on normal exit and failure.

Benchmark this separately from pinned-memory/asynchronous-transfer changes. If it wins, test pinned
buffers and supported asynchronous H2D copies as a follow-up. Use explicit stream/event dependencies,
buffer lifetime ownership and completion before reuse; `non_blocking` alone does not prove overlap.
Do not fabricate unsupported tch stream APIs. Bound memory and check actual overlap in a timeline.

**Exit:** unchanged input/update sequence, failure behavior and semantic gates, plus a full replay win.

## 8. Package A4 — optional GPU input cache

Only if repeated H2D transfers remain material, consider retaining immutable rollout inputs on GPU
and assembling unchanged minibatches there. Keep the baseline arithmetic, batch shapes, sparse
canonicalization and reduction order. GPU indexing/copying may cost more than the avoided transfer;
measure it. If gather implementation changes kernel selection or accumulation order, treat this as
Track B until exact evidence demonstrates otherwise.

Use a capacity bound with a clear fallback to the baseline path; no unbounded per-update tensor
retention. Exercise multiple consecutive updates to find leaks and allocator high-water growth.

## 9. Track B design — batched GPU inference across games

This is a separate research proposal, not a required implementation step after A4. Obtain the
specification exception described in section 1 before running it.

Compare: CPU baseline; GPU per-decision inference (overhead control); cross-game batched GPU inference.
Use a frozen policy snapshot for an entire rollout, then normal PPO optimization. No stale-policy
rollouts and no simultaneous weight mutation while inference requests are outstanding.

The engine/feature extractor stays on CPU. A request contains stable game/seat/decision IDs, actor
inputs, head/faction metadata and, if included, the critic inputs the baseline already permits. Do
not give the actor private critic-only information. Include PPO recording and actual behavior
probabilities/values in measurements; policy-only evaluation understates training work.

Different games provide independent requests; six sequential seats within a game do not. Keep RNG
streams with their games and sample in each game's original decision order. A central worker owns
the GPU actor; responses are matched by IDs and final rollout records are harvested in canonical
order, never arrival order.

Avoid a queue that waits for a full batch after every producer is blocked. Specify deterministic
partial-batch flushing, end-of-rollout handling and error/cancellation behavior. Test one active game,
fewer games than target batch size, uneven game lengths, early finishes and inference failures. Wall
clock timeout batching can change grouping with thread scheduling; if grouping changes numerics,
that also changes reproducibility. Either demonstrate grouping-invariant results or explicitly
design a deterministic grouping scheme and measure its latency cost.

Start with small target batches (e.g. 8, 16 and 32 pending decisions) and report actual decision AND
option counts per batch, queue wait, feature time, H2D/forward/D2H time, rollout wall and CPU worker
occupancy. Do not assume the 96-game job list means 96 games are simultaneously submitting requests.
Study the current Rayon chunking and ownership before selecting a cooperative scheduling approach.

First compare CPU vs GPU logits/probabilities, chosen actions, critic values and records. Repeat
GPU runs under scheduling variation. Numerical equivalence in real arithmetic is not bitwise
equivalence, and a tiny probability change can change the entire game. The old CPU batching prototype
already had this issue. If hashes differ, record first divergence and evaluate as a new execution
surface with repeatability and paired VP tests; never conceal it using a tolerance on final VP.

GPU inference should proceed beyond a prototype only if complete rollout throughput improves enough
to justify the engineering and the new surface passes the agreed learning/reproducibility gates.

## 10. Benchmark and adoption protocol

Keep profiler traces separate from performance measurements. Warm up both frozen variants equally.
Alternate paired A/B order over at least five measurements; use the same captured batch and initial
optimizer state for replay. Report all samples, median, spread and paired differences. If noisy,
diagnose contention or extend the bounded sample; do not cherry-pick the best run.

After an isolated win, run matched 96-game, four-epoch full updates with identical starting state,
weights, seed and settings. Capture 10–20 full updates per condition if repeatability and runtime
permit, plus a checkpointing boundary. If training diverges, fixed-batch replay can isolate optimizer
performance, but evolving rollouts are different workloads and must not be presented as exact A/B.

Report true seconds/update, decisions/second, games/hour, preparation/rollout/optimization shares,
CPU/GPU memory peaks and allocation/transfer metrics where measured. Pair timing with the semantic
results. Counts alone do not establish equivalence. Track A adoption requires unchanged behavior
within a demonstrated baseline; unresolved numerical divergence requires review as Track B.

For any behavior-changing arm, test learning against a continued baseline with identical starting
checkpoint, optimizer initialization, seeds/settings and compute accounting. Use independent training
replicates and paired map-cluster evaluation. No automatic claim that faster training improves VP.
Do not promote or resume production from benchmark scratch outputs.

### Planning arithmetic, not predicted gains

Using only 14.04 s rollout + 7.53 s optimization, excluding unreported overhead:

| Hypothetical improvement | Combined time | Throughput gain |
|---|---:|---:|
| Optimization becomes 5.0 s | 19.04 s | about 13% |
| Optimization time halves | 17.81 s | about 21% |
| Rollout time falls 20% | 18.76 s | about 15% |

Do not add these gains or extrapolate GPU utilization directly. Removing 20% of rollout time is not
the same as accelerating forward computation by 20%. Estimate attainable gains from A1's new budget.

## 11. Stop conditions and required handoff

Stop the current experiment and report if: another trainer starts; input/source provenance is unclear;
hashes differ unexpectedly; baseline repeatability is unresolved; unknown flags are ignored;
nonfinite checks weaken; batch order changes; memory grows across updates; threads fail to join;
or an optimization requires a specification/surface change beyond its track.

Do not abandon useful read-only analysis because a benchmark is waiting, but do not claim a speedup
from contended measurements. Do not keep training an arm whose only established result is higher GPU
utilization. Failed prototypes are useful results when the cause and evidence are recorded.

Each package's report must include:

1. Problem measured, hypothesis, scope and exact changed files.
2. Frozen source/executable/library/checkpoint/pool hashes and full resolved configuration.
3. Commands, resource bounds, raw output paths and all timing samples.
4. Input tensor, gradient/optimizer and ordered-game equivalence evidence as applicable.
5. Before/after full wall time, memory, uncertainty and learning-behavior limitations.
6. Verdict: adopt candidate, reject, inconclusive, or requires surface approval; next exact action.

Immediate next action after the user's pause: rediscover processes, select a completed checkpoint,
and execute A0 followed by A1. Do not jump straight to batching or asynchronous rollouts.
