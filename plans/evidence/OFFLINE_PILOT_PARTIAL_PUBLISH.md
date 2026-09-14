# Offline pilot: safe partial publication and BC continuation

Date: 2026-09-14. Branch `codex/fix-six-faction-leaders`.

## Production input

- stopped staging: `E:\ti4-corpus\vponly-single-236464-20260914.staging-59548`
- requested games: 332,768; actually completed before the stop: 158,755
- complete retained pairs observed before publication: 100,492 (`good` 97,387, `bad` 21,
  `random` 3,084); incomplete final parts are excluded by the validator
- generation base commit: `6938019d690bf728f381bab4e203331982ebbf4d`, dirty worktree (the
  single-checkpoint changes were committed six minutes after the executable build)
- generation executable SHA-256:
  `be6895cc64c77e38a23dc9b3a941bcfe191db669453348e4df2201961ce16b70`
- generation map-pool SHA-256:
  `106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df`

## Correctness changes

`capture_offline_pilot --publish-staging` now requires actual/planned counts, explicit policy
mode, and generation commit/binary provenance. It verifies an optional generation-time map-pool
digest rather than silently attributing publication-time state to generation.

Every complete part pair is decoded on the requested Rayon worker pool. Every decision is
deserialized as `CapturedDecision`, loss-alignment checked, matched to its game/seat/faction/policy
metadata, and checked for contiguous per-seat decision indices. Duplicate game indices across
buckets, conflicting checkpoint digests, seed/round disagreement, impossible counts, bucket/reason
mismatch, and falsely labelled failed games refuse publication. Truncated kill artifacts are
reported and excluded.

Recovery builds shards in a new sibling `*.publishing-<pid>` directory, verifies exact SHA-256s,
writes root and scoped manifests, then atomically renames that directory. The original stopped
staging is never modified, so failure remains retryable. `games_played` is the actual completed
count and `games_planned` records the original request.

Future single-checkpoint capture advances the temperature offset by one game. With six seats and
three temperatures, every seat now sees every temperature instead of the former `+6 mod 3 = 0`
pinning. The stopped corpus necessarily retains that historical seat/temperature confound and the
pre-leader-fix behavior; its continuation checkpoint is therefore experimental and must be
evaluated before promotion.

## Verification

- `cargo test -p ti4-mlp --example capture_offline_pilot -- --test-threads=4`: 20 passed.
- Tests use structurally valid `CapturedDecision` records and cover loss alignment, full partial
  scan/reconstruction, truncation exclusion, corrupt-record refusal, cross-bucket duplicate
  refusal, atomic publication with preserved source parts, exact shard hashes, honest manifest
  counts/provenance, and complete seat/temperature coverage.
- Workspace-wide `cargo clippy ... -D warnings` remains blocked by pre-existing warnings in
  `ti4-mlp/src/bot.rs`, `ti4-mlp/src/ppo.rs`, and `ti4-mlp/src/lib.rs`; none is in this package.

## Committed launcher

`scripts/publish_and_train_stopped_corpus.ps1` rebuilds the publisher, publishes with 32 validation
workers and pinned provenance, rebuilds the CUDA trainer, then calls the authenticated
`scripts/train_offline_corpus.ps1` boundary. Training consumes `good + random` (not `bad`) and
continues from `D:\Projects\ti4-engine-rs\out\offline-bc-v2-20260913-from-318956` for five full
epochs at batch 4096, micro-batch 2048, learning rate 3e-5, with 32 parallel parser workers.
Run-plan JSON, logs, executable hash, checkpoint hash, corpus-manifest hashes, and output checkpoint
are preserved beside `out\offline-bc-v3-20260914-from-bcv2`.
