# OFFLINE-BC-PLAIN-JSONL-INPUT — evidence

Operator-requested change (2026-09-13), outside the M00–M13 table: corpus generation must stop
compressing its output, and the result must be directly consumable by the CUDA pipeline; compute-heavy
steps may not run on a single worker (minimum one per physical core).

## Normative sources

- Operator instruction 2026-09-13 (see above) + "check what codex did".
- Codex's commits already on this branch when work started: `412f706` Write future offline corpora as
  plain JSONL (capture now writes `decisions.jsonl`/`games.jsonl`, manifest gains `storage_encoding`),
  `fce1a54` Parallelize offline corpus packing, `40a67b7` Load offline training frames in parallel,
  `ff76e61` Validate parallel zstd frame boundaries. This package completes the chain: codex changed
  the producer but the consumer (`offline_bc.rs`) still hard-coded `.jsonl.zst`, so future corpora
  would have been unreadable by the CUDA pipeline.
- Engine HEAD `412f706` (branch `wp/offline-pilot-streaming-retention`). Tree clean at start; codex's
  earlier in-flight files had all been committed by him (`1820db0` snapshot).
- Historical Python reference: not inspected.

## What changed

`crates/ti4-mlp/examples/offline_bc.rs` only (plus evidence/state):

1. **Dual-format corpus input.** `files()` now matches `.jsonl` and `.jsonl.zst`; new `is_zstd()`
   sniffs the 4-byte zstd magic; `lines()` (pack-mode games) and pack mode's decisions loop pick the
   reader from the magic. Plain JSONL is the current capture output, zstd frame streams remain for
   existing corpora (`pilot-retained-32k-20260913`, `six-faction-block-0001-complete`, `pilot-v1`).
2. **`train-raw-parallel` on plain corpora.** New `shard()` resolves whichever encoding exists; the
   games shard is parsed as text when plain (with the same record-count guard as the frame count);
   the decisions file is split by new `line_chunks()` into at most `rayon::current_num_threads()`
   line-aligned byte ranges and parse/compiled in parallel — one worker per logical processor, the
   same property the zstd path had via per-frame parallelism. The per-chunk curation (standout/strong
   keep, control subsample by `--control-per-million`, game-id train/validation split) is factored
   into `compile_chunk()` and shared verbatim by both paths. Newline bytes cannot occur inside a
   multi-byte UTF-8 sequence, so chunk boundaries are always valid UTF-8/JSONL splits.
3. **No format change to the packed output.** `train`/`validation.ti4bc.zst` (schema
   `ti4-offline-bc-v1`, magic `TI4BC001`) is what the CUDA pipeline consumes; that contract is
   untouched. The removed compression was at the *corpus* level, per the operator instruction.

## Commands and results

All on the 9950X/RTX-3090 machine, release profile:

```text
cargo build --release -p ti4-mlp --example offline_bc        # clean
rustfmt --edition 2024 crates/ti4-mlp/examples/offline_bc.rs # clean
cargo clippy --release -p ti4-mlp --example offline_bc       # 9 hits, one BELOW the pre-change
                                                             # baseline of 10 (is_multiple_of fix); zero new
```

Functional matrix:

| Test | Command shape | Result |
|---|---|---|
| Plain capture determinism | `capture_offline_pilot --games 12` vs `--workers 1`, same seed base | byte-identical shards (decisions sha256 `dda29f46…0058f24b`, games `ccf1205a…789332071`) — codex's plain-JSONL change preserves the worker-count determinism proof |
| Pack on plain corpus | `offline_bc pack --corpus out/tmp-plain-a` (3 retained games / 5,729 decisions) | published; buckets `{control: 4204, standout: 1284}` |
| Pack on zstd corpus (regression) | same against `out/tmp-t12` and `out/tmp-ret-a` | published; validation shard byte-identical across both input encodings for the same games (`d6951502…df29`) |
| train-raw-parallel on plain corpus | `--corpus out/tmp-plain-a --expected-frames 3` | "parallel-loading 3 game frames", "**parallel-parsing 32 decision chunks**" (one per logical processor), "curated 1410 train and 0 validation decisions"; stops only at CUDA device resolution because this libtorch build links CPU-only torch — the training stage consumes in-memory samples identically for both formats |

## Retention-rule semantic change by codex (recorded, not made by this package)

Codex's snapshot `1820db0` changed the standout condition from strictly-above to inclusive:
`max_faction_vp > 6` → `>= 6`, renaming the boundary test (`exactly_six_vp_is_not_a_standout` →
`exactly_six_vp_is_a_standout`). The operator's stated rule was "more than 6 VP" (strictly above),
which is what commit `77e30cc` implemented and what **the completed 32,768-game corpus used**: its
`retention.jsonl` contains 4,738 games with `max_faction_vp == 6`, none retained as standout. The BC
model `out/offline-bc-32k-20260913-from-318956` was trained from that corpus. Current HEAD therefore
retains a strictly larger set than the published corpora; the rule string `vp-threshold-v1` does not
distinguish the two, but any corpus's `retention.jsonl` reconstructs which semantics it used (games
with max VP == 6 labelled `standout` ⇒ inclusive). **Awaiting operator decision on which semantics is
canonical for future corpora.**

## Head-to-head evaluation requested by operator (2026-09-13)

`crossplay_eval`, 60 seeds × 6 rotations × 6 seats = 2,160 games per direction, greedy candidate vs
frozen benchmark, holdout pool `out/pools/full_np8_12_holdout.json`:

| Candidate (benchmark frozen) | VP | margin | win | cleared | waste | time |
|---|---|---|---|---|---|---|
| `offline-bc-32k-20260913-from-318956` vs checkpoint-318956 table | 3.299 | −1.587 | 10.8% | 83.3% | 14.8% | 326 s |
| `checkpoint-318956` vs offline-bc-32k table | 3.362 | −1.585 | 11.4% | 80.3% | 14.8% | 300 s |

The two policies are indistinguishable (ΔVP 0.06, Δmargin 0.002): the BC student reproduces its
teacher checkpoint-318956 at this horizon; neither beats the other. Both sit in the null region
(margin ≈ −1.59, win ≈ 11% < 1/6 chance) — no regression and no measurable gain from the offline BC
step on this corpus. Full log: `out/eval-bc32k-vs-ckpt318956.log`.

## Known differences / caveats

- The plain decisions shard for a full 32k-scale corpus is ~2× the zstd size (~10 GB vs ~5 GB);
  `train-raw-parallel` reads it whole into RAM (same as its existing zstd behavior) — fine on this
  machine, not portable to small-RAM hosts.
- `line_chunks()` boundary scan is a single O(n) pass (~20–30 s for a 10 GB file); the heavy parse/
  compile step itself runs fully parallel.
- This libtorch build has no CUDA device; actual GPU training of future corpora must run where codex
  trained `offline-bc-32k` (or after a CUDA-enabled torch link).
