# Offline pilot: partial publish of a killed capture run

Date: 2026-09-14. Branch `codex/fix-six-faction-leaders`, base commit `09571a1`.

## Motivation

The operator will kill the running 332,768-game single-checkpoint capture
(`vponly-single-236464-20260914`) before it finishes and wants to train a BC student from
whatever corpus data exists at that point. A killed run leaves per-game zstd parts under
`<out>.staging-<pid>/` but no published corpus, so `offline_bc train-raw-parallel` (which
requires a valid `manifest.json`) cannot consume it directly.

## Design

New mode in `crates/ti4-mlp/examples/capture_offline_pilot.rs`:

```text
capture_offline_pilot --publish-staging <staging-dir> [--out <corpus-dir>] \
    --checkpoint <ckpt-dir> --games N [--workers N] [--map-pool path]
```

- **Scan**: every `good|bad|random|failed` part pair (`decisions-<idx>.jsonl.zst` +
  `games-<idx>.jsonl.zst`) is fully decoded and validated. A truncated or corrupt part
  (kill mid-write) is *excluded with a report*, never silently dropped; a decodable but
  inconsistent part (record count mismatch, bucket/reason disagreement, seed-base mismatch)
  **refuses the publish** — inconsistency means the run's invariants broke, not that one game
  was unlucky.
- **Reconstruct**: `GameOutcome` is rebuilt from decoded metadata: VPs from
  `SeatMetadata.final_progress.victory_points`, retention reason by re-running the pure
  `decide_retention(table_vp, max_faction_vp, game_seed)` and checking it agrees with the
  part's bucket folder, seed base as `game_seed - game_index`, rounds/checkpoint manifests/
  policy mode from seat metadata.
- **Assemble**: shared `assemble_and_publish` (also used by the normal path) concatenates
  parts in game order per bucket, runs the byte-exactness sha256 gates, writes
  `retention.jsonl`, root + scoped bucket manifests, then renames staging to the corpus dir.
  For partial publish, parts are kept until all gates pass (`keep_parts: true`) so a failed
  publish is retryable; they are deleted only after success.
- **Manifest honesty**: `games` = complete games found (not planned), `games_played` =
  planned total from `--games`, `workers` = `Option<usize>` (new) — the killed run's worker
  count is recorded when given, `null` otherwise; `policy_mode` and checkpoint manifests are
  derived from seat metadata so vocabulary provenance matches the training checkpoint.

Supporting changes: `Manifest.workers: Option<usize>`, `GameOutcome: Debug + Clone`,
`concatenate_parts(…, keep_parts)`. Normal-run behavior is unchanged (it passes
`keep_parts = false`; all three training buckets already exist in staging).

## Verification so far

- **Unit tests 18/18** (`cargo test -p ti4-mlp --example capture_offline_pilot`), including:
  partial scan/reconstruction from synthetic parts; truncated-part exclusion with report;
  inconsistent-part refusal (record-count mismatch); full partial publish producing a corpus
  whose manifests, shard hashes, and retention sidecar all validate.
- **Clippy**: clean for the new code (one pre-existing `unused self` warning at line ~286).
- **Normal-path regression with real data**: rebuilt debug binary ran 20 single-checkpoint
  games end-to-end → `published 13/20 games / 22883 decisions`, assembly + integrity check
  passed, manifest complete (smoke corpus under gitignored `out/tmp-partial-smoke`).
- **Newline-counting assumption on real records**: decoded the smoke corpus shards and
  compared newline counts to summed metadata `decision_count` — exact match in every bucket
  (`good`: 21031 = 21031, `random`: 1852 = 1852, `bad`: empty). Holds by construction anyway:
  each record is one compact `serde_json` line (control chars escaped) + `\n`.

## Not yet done (recorded honestly)

- **Real killed-run e2e smoke**: the debug smoke run finished before it could be killed;
  truncation mechanics are covered synthetically by unit tests, and the operator's actual
  kill of the 332k run is the production case. If a publish ever reports exclusions or
  refuses, inspect its per-part report lines first.
- **Release rebuild** of `capture_offline_pilot.exe` is blocked while the running capture
  holds the exe lock (Windows); the launcher script performs it after the kill.

## Launcher

`out/train_single_ckpt.ps1` (gitignored): verifies the capture process is dead, globs
`E:\ti4-corpus\vponly-single-236464-20260914.staging-*`, rebuilds the release capture binary,
publishes to `E:\ti4-corpus\vponly-single-236464-20260914-partial` (checkpoint 236464,
planned games 332768), rebuilds `offline_bc` in `target-cuda` against cu128 libtorch, then
trains from the published `good + random` buckets with v2's recipe (`--workers 32`,
`--micro-batch 2048`, defaults epochs 5 / lr 3e-5 / batch 4096) into
`out/offline-bc-single-<date>-from-236464`. Logs to `out/train-single-ckpt.log`.
