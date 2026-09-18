# Offline BC multi-corpus CUDA pipeline

Date: 2026-09-13

Branch: `wp/offline-pilot-streaming-retention`
Scope: operator-authorized preparation of the generated self-play corpus for efficient MLP policy
pretraining.

## Permission and artifact bounds

- Permission class: P1 for source, tests, script, evidence, and local build; P2 only when a full
  training run is explicitly launched.
- Writable source paths: `crates/ti4-mlp/examples/offline_bc.rs`,
  `scripts/train_offline_corpus.ps1`, and this evidence file.
- Read-only external input: published corpora under `E:\ti4-corpus`.
- Generated build output: existing ignored `target/` and `target-cuda/` trees.
- No network access, deletion, corpus mutation, process termination, or remote state change.
- The active generator was not interrupted. Preparation builds used four Cargo jobs.

## Problem

The bucketed generator publishes `good`, `random`, and `bad` as separate authenticated corpora. The
fast `train-raw-parallel` path accepted only one `--corpus`, required an operator-supplied frame
count, did not authenticate capture shards, and therefore could not directly train on `good` plus a
sample of `random`. The older `pack` path would add a full sequential transformation and another
large derived artifact before CUDA could start.

The learner also defaulted to 512 decisions per CUDA micro-batch. The loss already accumulates an
unchanged 4,096-decision batch across micro-batches, so using 2,048 reduces host/kernel launch
overhead without changing the accumulated batch gradient or optimizer-step count.

## Implementation

`train-raw-parallel` now:

1. accepts repeated `--corpus PATH` arguments;
2. refuses every input without a completed `manifest.json`;
3. validates capture schema, observation schema, vocabulary digest, and game counts;
4. resolves each input's exact game and decision shard from the published directory;
5. authenticates each shard from the same in-memory byte buffer used for decoding, avoiding a
   second disk read;
6. obtains independent frame counts from each manifest rather than a manual total;
7. loads all game outcomes, rejects duplicate game/seat identities across inputs, and then decodes
   each decision shard with the global Rayon pool;
8. keeps standout/productive-strong decisions, deterministically samples controls, excludes weak
   decisions, and splits at the game level;
9. reports train and validation composition by bucket, faction, decision head, and policy hash
   before CUDA starts; and
10. records every source corpus path in checkpoint provenance.

Decision shards are processed one corpus at a time. Every shard uses all requested parse workers,
while only one compressed shard occupies the large input buffer at once. Curated samples persist
until training because the current distillation API takes an in-memory sample slice.

`scripts/train_offline_corpus.ps1` is the launch boundary. It validates published paths, refuses
existing outputs, selects the pinned CUDA libtorch, defaults to all logical processors for parsing,
uses `batch=4096` and `micro_batch=2048`, writes an adjacent immutable run plan before launch, tees
the sparse progress log, and retains both files on failure.

Recommended full v1+v2 invocation after v2 publishes:

```powershell
.\scripts\train_offline_corpus.ps1 `
  -Corpus @(
    'E:\ti4-corpus\pilot-retained-32k-20260913',
    'E:\ti4-corpus\pilot-retained-32k-20260913-v2\good',
    'E:\ti4-corpus\pilot-retained-32k-20260913-v2\random'
  ) `
  -Checkpoint 'out\blank-shaped-4layers\shaped-r1bonus3-20260912\checkpoint-318956' `
  -Output 'out\offline-bc-v1-v2-20260913-from-318956' `
  -Workers 32
```

`bad` is intentionally omitted: cloning weak-table decisions would teach their raw actions as
targets. The corpus remains available for later matched-state ranking or negative analysis.

## Checks and results

```text
cargo test --release -p ti4-mlp --example offline_bc -- --nocapture
  5 passed; 0 failed

cargo clippy --release -p ti4-mlp --example offline_bc
  pass; existing workspace warnings remain

cargo clippy --release -p ti4-mlp --example offline_bc -- -D warnings
  blocked by pre-existing ti4-model::GameState struct_excessive_bools

CARGO_TARGET_DIR=target-cuda cargo build --release -p ti4-mlp --example offline_bc
  pass in 19.08 s with four build jobs

target-cuda/release/examples/device_probe.exe
  CUDA available: true; one device; arithmetic result on Cuda(0)
```

Real loader-path validation used the already published 12-game `out/tmp-t12` fixture with four
workers and the CPU-linked binary. It authenticated both shards, decoded 12/12 independent frames,
compiled 16,238 train and 1,372 validation decisions, printed complete composition, and stopped at
the expected `CUDA required` boundary. It wrote no checkpoint. This verifies capture manifests,
checksums, zstd frame discovery, parallel compilation, curation, and game-level splitting; the CUDA
probe separately verifies the prepared CUDA runtime.

A second negative integration invocation supplied the same corpus twice. The executable reported
two manifests/24 frames, read both authenticated game shards, and refused the duplicate identity
`pilot-1026091300-0000` before decision loading or training. This proves repeated arguments are
consumed while preventing accidental double-weighting.

PowerShell parsed `scripts/train_offline_corpus.ps1` successfully. `git diff --check` passed for the
two implementation files.

## Remaining runtime gate

The active v2 generator must finish its assembly, integrity checks, scoped manifests, and atomic
rename before the full command is legal. The full run has not been started by this package. After
publication, inspect the final manifest and composition output before accepting the resulting
checkpoint, and follow training with paired gameplay evaluation; imitation NLL is not evidence of
gameplay improvement.

Independent review remains pending; this package does not claim that its implementer reviewed
itself independently.
