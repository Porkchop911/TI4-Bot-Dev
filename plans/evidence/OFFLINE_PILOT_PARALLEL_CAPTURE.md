# OFFLINE-PILOT-PARALLEL-CAPTURE — evidence

Operator-requested change (2026-09-13), outside the M00–M13 table: run the offline self-play
pilot capture (`crates/ti4-mlp/examples/capture_offline_pilot.rs`) on the fastest possible number
of workers instead of one.

## Normative sources

- `AGENTS.md` accuracy rules (determinism must not depend on thread scheduling; no speculative
  abstractions).
- Existing parallel-capture precedent: `crates/ti4-mlp/examples/build_positive_corpus.rs`
  (`actor.inference_copy()` per chunk, `Rc` created inside the worker closure) and
  `ti4_training::rollout::play_rotated_save54_pool_batch_with_workers` (worker-count seam).
- Engine HEAD `4d7f08c`, worktree dirty (codex's in-flight BUG-001/PPO files preserved untouched;
  none of them are committed by this package except the dev-dependency additions that the offline
  corpus examples require — see "Scope changes").
- Historical Python reference: not inspected.

## What changed

`capture_offline_pilot.rs` only (plus the evidence/state files and the example's dev-deps):

1. **Plans are precomputed on the main thread.** The faction deck / policy-offset state machine is
   a sequential function of `game_index`; it now runs before any game starts, so every worker
   receives exactly the inputs the old sequential loop would have given that game.
2. **Games play on rayon's global pool** (one thread per logical processor — 32 on this machine).
   `--workers N` pins a dedicated pool of exactly N threads (the same testable seam the PPO
   rollout code uses); no flag means "fastest possible".
3. **Actors never cross threads by reference.** `tch::Tensor` is `Send` but not `Sync`, so each
   worker chunk owns deep inference copies (`Actor::inference_copy()`) made on the main thread and
   wrapped in `Rc` inside the worker closure — byte-for-byte the `build_positive_corpus` pattern.
   Chunk count is bounded by the worker count, so tensor memory is O(workers), not O(games).
4. **Per-game zstd frames.** Each worker writes its own game's records to
   `staging/decisions-<game_index:06>.jsonl.zst` and `staging/games-<game_index:06>.jsonl.zst`;
   the main thread concatenates those frames in game order into the published shards. No
   uncompressed record crosses a thread boundary, so memory stays bounded at any corpus size
   (the 240-game block codex captured single-threaded is ~35 GB uncompressed).

## Determinism argument and proof

Per-game inputs are pure functions of `game_index`/`seed_base`; each game runs on one thread with
its own seeded RNG streams; libtorch intra-op threads are pinned to 1 process-wide, so no tensor
reduction order depends on scheduling. The decoded shards must therefore be identical at any
worker count — and they are:

| run | command (all `--games` as noted) | workers | wall clock | decisions sha256 | games sha256 |
|---|---|---|---|---|---|
| A | `--out out/tmp-par-a-w1 --games 12 --workers 1` | 1 | 39.7 s | `bace70e3ef8c…c3bcce9` | `13ac415015ed…f1c24246ab` |
| B | `--out out/tmp-par-b-w32 --games 12` (default) | 32 | 9.3 s | identical to A | identical to A |
| D | `--out out/tmp-par-d-w8 --games 12 --workers 8` | 8 | — | identical to A | — |

Full hashes: decisions `bace70e3ef8c1219a3037d3b2a61e7fcb0033238e9cf756ed6bf11a01c3bcce9`,
games `13ac415015edc63f8dee042bf4248ef72811a0028272ce84e7dd08f1c24246ab` (runs A, B and D).
All three runs: 12 games / 18520 decisions.

Batch-size stability (chunking path with >1 game per worker): run C `--out out/tmp-par-c-64g
--games 64` on 32 workers finished in **29.6 s** (sequential estimate ≈ 64 × 3.3 s ≈ 211 s, i.e.
~7× wall-clock including startup, actor copies and the validation read-back) with 97911 decisions;
its first 18520 decoded decision lines and first 12 game-metadata lines are byte-identical to runs
A/B.

## Checks (exact results)

- `cargo build --release -p ti4-mlp --example capture_offline_pilot` — clean.
- `rustfmt --edition 2024 --check crates/ti4-mlp/examples/capture_offline_pilot.rs` — clean after
  formatting.
- `cargo clippy --release -p ti4-mlp --example capture_offline_pilot` — **no new warnings**. Five
  pre-existing pedantic/all warnings remain in this file, all on code this package did not change:
  `unused_self` (RecordingDecider::observation), Option `map/unwrap_or_else` (`git()` helper),
  `too_many_lines` (`policy()`, body unchanged apart from parameter names) and `type_complexity`
  (the per-game record-handle type, moved verbatim from the old loop into `play_game`).
- `cargo test -p ti4-mlp` — lib 99 passed / 0 failed; integration binaries 3+3+2+1+2 passed /
  0 failed.
- Smoke run (`--games 2 --workers 1`, out/tmp-parallel-smoke): published 2 games / 3149 decisions;
  `validate_offline_corpus.rs --corpus … --decisions 256` **passed** (256 non-forced decisions,
  1381 reconstructed options, finite one-hot BC mean NLL 0.316882) — proving the multi-frame shard
  reconstructs the MLP's exact sparse inputs and forward passes.
- `python tools/filter_offline_corpus.py --source out/tmp-par-b-w32 --destination
  out/tmp-par-selected` **passed** (retained 7/12 games, 11374 decisions) — proving the Python
  selector consumes the new shard format.

## Scope changes (recorded, not silent)

- **Shard byte layout**: published shards are now multi-frame zstd streams (one frame per game,
  concatenated in game order). Decoded content is identical to what sequential capture produced;
  raw bytes differ from a single-frame encoding. Concatenated frames are standard zstd and every
  reader this pipeline uses walks them transparently — verified above with the Rust decoder
  (`validate_decisions` inside the example, `validate_offline_corpus.rs`) and Python's
  `zstandard.stream_reader` (filter tool).
- **Manifest**: additive `workers: usize` field on the `ti4-offline-selfplay-v1` manifest so a
  corpus records how it was scheduled. All other fields unchanged; shard hashes are recorded as
  before.
- **New CLI flag** `--workers N` (positive integer). Default is rayon's global pool = all logical
  processors, i.e. the fastest possible scheduling on this machine.
- **Committed dev-dependencies**: `serde`, `zstd`, `chrono` for `ti4-mlp` (Cargo.toml + lock) —
  added by codex in the working tree specifically to build these offline-corpus examples; without
  them the committed example does not compile on a clean checkout.

## Known differences / caveats

- The existing `E:/ti4-corpus/pilot-v1` corpus was generated with an earlier engine state: the
  worktree has since gained uncommitted changes to `choice.rs`/`progress.rs`, so per-game decision
  counts for identical seeds differ from pilot-v1 (e.g. game 0: 1350 then, 1750 now). That drift
  predates this package; a comparison against pilot-v1 would be invalid and was not performed.
- Test corpora `out/tmp-par-{a-w1,b-w32,c-64g,d-w8}` and `out/tmp-par-selected` are retained under
  the gitignored `out/` for re-verification; delete freely.
