# OFFLINE-PILOT-BUCKETED-ZSTD — per-reason bucket folders, zstd output

## Objective

Operator decision (2026-09-13): the plain-JSONL capture format made 32k-scale generation
disk-write-bound on E: (a mechanical HDD; ~140 MB of uncompressed records per retained game vs
~900 KB zstd frames), slowing a 32,768-game run from ~97 min to an estimated 6–7 h. Revert capture
output to zstd and publish retained games into folders by retention reason — `good/`, `bad/`,
`random/` (plus `failed/` only when a game fails) — so each quality class is directly consumable by
the CUDA pipeline (`offline_bc pack --corpus <root>/<bucket>`).

## Normative sources

- Operator instruction: "b but make sure the files land in folders by rentention reason, (just
  good/bad/random)" and "also do 200k" (2026-09-13).
- Retention rule `vp-threshold-v1` with canonical inclusive standout threshold (`>= 6`, confirmed
  same day; see `OFFLINE_BC_PLAIN_JSONL_INPUT.md`, "Retention-rule semantics").
- `AGENTS.md`: determinism must not depend on thread scheduling or hash-map iteration; one focused
  commit per atomic package.

## What changed (single file: `crates/ti4-mlp/examples/capture_offline_pilot.rs`)

- Restored `JsonlZstdWriter` (zstd level 9, byte-faithful to the pre-`412f706` implementation);
  parts are again `decisions-NNNNNN.jsonl.zst` / `games-NNNNNN.jsonl.zst`; final shards per bucket
  are `decisions.jsonl.zst` / `games.jsonl.zst`.
- New `bucket_for(reason)`: Standout|StrongTable → `good`, WeakTable → `bad`, RandomControl →
  `random`, FailedGame → `failed`. The three training buckets are pre-created at staging setup;
  `failed/` is created lazily by a worker only when a game actually fails, so it appears in the
  published corpus iff failures occurred.
- Retained games write their parts into `<staging>/<bucket>/`; assembly concatenates each bucket's
  frames in game order with the same running-sha256 byte-exactness gate as before (now per shard).
- Empty buckets publish a valid zero-record zstd frame (`zstd::stream::encode_all(&b""[..], 9)`),
  so every corpus has the same shape and every folder decodes cleanly.
- Manifest: `storage_encoding = "zstd"`; `shards` keyed by path relative to the corpus root
  (e.g. `good/decisions.jsonl.zst`); new additive field `buckets: {bucket → {games, decisions}}`.
  Each training bucket folder additionally gets a scoped `manifest.json` (same schema; games /
  records / shards / retention_breakdown / policy_families restricted to that bucket) so `offline_bc
  pack`, which refuses corpora without a manifest, accepts any bucket directly. `failed/` has no
  manifest: its partial decisions are visibility artifacts, never training data.
- `retention.jsonl` sidecar unchanged (one line per played game; the reason string already
  determines the bucket).

## Commands and results

- `cargo build --release -p ti4-mlp --example capture_offline_pilot` — clean, no warnings.
- `cargo test --release -p ti4-mlp --example capture_offline_pilot` — **13 passed** (12 prior + new
  `buckets_map_reasons_to_folders`).
- Clippy: warning count identical to pre-change baseline (verified by stashing and diffing the
  normalized warning lists; one transient `collapsible_if` from my own nested if was fixed with a
  let-chain, edition 2024 / rustc 1.94).
- Determinism + layout: two 12-game runs at seed base 9000001 (`--workers 32` → `out/tmp-bucket-a`,
  `--workers 1` → `out/tmp-bucket-b`). Both published **5/12 games, 7509 decisions**. Every data
  shard is byte-identical across worker counts:
  - `good/decisions.jsonl.zst` sha256 `5dc04ca6…3e`, `good/games.jsonl.zst` `883c6c0a…ef36f`
  - `bad/decisions.jsonl.zst` `37243bce…dd3c`, `bad/games.jsonl.zst` `eb00b05d…737f`
  - `random/*` both `6fb85438…cbd` (the deterministic empty frame — no random-control games in this
    sample)
  - `retention.jsonl` identical.
- Manifests differ only in the legitimate `workers` and `created_utc` fields (verified by semantic
  diff after dropping `created_utc`). Top-level buckets: good 3 games/4993 decisions, bad
  2/2516, random 0/0; scoped bucket manifests carry matching per-bucket shards/breakdowns.
- CUDA pipeline entry point: `offline_bc pack --corpus out/tmp-bucket-a/good --out
  out/tmp-pack-bucket --checkpoint …/checkpoint-318956` → **published** (train.ti4bc.zst 1,514,453 B;
  validation header-only at this corpus size).

## Benchmark effect

Restores the CPU-bound regime of run #1: per-retained-game write volume drops from ~140 MB plain to
~900 KB zstd (this data compresses ~150× under level 9 — verified by recompressing a decoded game
frame: 121 MB → 821 KB). Expected 32k runtime back to ~1.7 h; final corpus ~8–10 GB instead of
~1.4 TB.

## Unresolved differences / scope notes

- `failed/` is a fourth folder beyond the operator's "just good/bad/random": it exists only when a
  game fails (zero failures in every run so far) and keeps engine-failure visibility without
  contaminating training buckets. Recorded here as an intentional, minimal deviation.
- Old flat corpora (`pilot-retained-32k-20260913`, `six-faction-block-0001-complete`, `pilot-v1`)
  are untouched and remain readable by the dual-format packer.

## Source versions

Engine HEAD at commit time: branch `wp/offline-pilot-streaming-retention` (parent `d19fe72`).
Historical Python reference: not inspected.
