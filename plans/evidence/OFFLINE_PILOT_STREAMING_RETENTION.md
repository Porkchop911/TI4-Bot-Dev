# OFFLINE-PILOT-STREAMING-RETENTION — evidence

Operator-requested change (2026-09-13), outside the M00–M13 table: the offline self-play pilot
capture must decide, **at game end while the records are still in memory**, whether to write a
game's frames to disk or abandon it — instead of writing every game and filtering afterwards.

## Normative sources

- Retention rule agreed by operator with codex (2026-09-13), stated as per-game conditions:
  any faction finishes **above 6 VP** → keep; table total **≥ 24 VP** → keep; table total
  **< 10 VP** → keep; otherwise **5% chance** to keep, else discard. Implemented verbatim as rule
  `vp-threshold-v1` (constants `STANDOUT_VP = 6` strictly-above, `STRONG_TABLE_VP = 24`
  inclusive, `WEAK_TABLE_VP = 10` exclusive, coin 5/100).
- `AGENTS.md` accuracy rules: determinism must not depend on thread scheduling; failed runs must
  not become apparent successes.
- Engine HEAD `488c0bc` (branch `wp/offline-pilot-parallel-capture`, parent of this package's
  branch `wp/offline-pilot-streaming-retention`). Worktree dirty with codex's in-flight files,
  preserved untouched and not committed by this package.
- Historical Python reference: not inspected.

## What changed

`crates/ti4-mlp/examples/capture_offline_pilot.rs` only (plus evidence/state):

1. **Streaming retention at game end.** After `game.run`, each worker reads final per-seat VP from
   `game.state.player(p).victory_points` (the same access pattern `vp_sources.rs` uses), sums the
   table total and max faction, and calls `decide_retention(table_vp, max_faction_vp, game_seed)`.
   Retained games write their frames exactly as before; **discarded games write nothing** — their
   records are dropped on the worker thread. Failed games (`completed: false`) are always retained
   with reason `failed_game` so engine failures stay visible in the corpus (previously they were
   recorded too).
2. **Deterministic coin.** The 5% control draw uses `ChaCha8Rng::seed_from_u64(game_seed ^
   RETENTION_COIN_SALT)` — a pure function of the game index, so retention is identical at any
   worker count (no shared RNG, no scheduling dependence).
3. **Loss-alignment gate moved before writing.** The old end-of-run `validate_decisions` re-parsed
   every line of the concatenated shard on one thread (~0.13 ms/decision — 49.7 s of a 240-game
   run, ~1.8 h at 32k games). The same predicate (`is_loss_aligned`: index in range, chosen id
   matches, legal actions numbered from zero) now runs on the in-memory records of each retained
   game *before* its frame is written — microseconds instead of a serial O(N) tail, and an
   unaligned record can never reach disk. Discarded games are not validated (they go nowhere).
4. **Byte-exact assembly check.** `concatenate_frames` now returns the sha256 of the exact bytes it
   wrote; after publishing, each shard's manifest hash must equal that value or the run refuses.
   This replaces the parse-level re-read as the corruption guard: every line in a published shard
   came from a frame whose records passed the gate before writing, and the check proves the file on
   disk is byte-identical to those frames.
5. **Streaming `file_sha`.** The old implementation did `std::fs::read` of the whole shard — an
   OOM at corpus scale (a 32k-game decisions shard is tens of GB; this machine has ~94 GB RAM). It
   now hashes in 1 MiB heap chunks. Required for any large run, with or without retention.
6. **Deterministic failure reporting.** Worker errors carry the failing game's index; the main
   thread reports the smallest-index failure regardless of chunk completion order (previously the
   first error rayon surfaced won a race).
7. **Provenance.** A `retention.jsonl` sidecar is published with every corpus: one line per played
   game (index, seed, table VP, max faction VP, recorded decisions, retained flag, reason) — audit
   trail and calibration data for future threshold tuning. Manifest gains additive fields
   `retention_rule`, `games_played`, `games_retained`, `retention_breakdown`; `games` now means
   "games present in the shards" (retained), consistent with `records["games"]` and shard line
   counts. No corpus published before this change used the new binary, so no migration is needed;
   old corpora keep their old manifests.

## Commands and results

All on the 9950X machine, release profile, engine HEAD as above (worktree dirty with codex's
in-flight files — same caveat as the parallel-capture package).

```text
cargo test --release -p ti4-mlp --example capture_offline_pilot   # 12 new unit tests: all pass
cargo test --release -p ti4-mlp                                    # lib 99 + integration suites: all pass
cargo clippy --release -p ti4-mlp --example capture_offline_pilot  # no new warnings (one pre-existing
                                                                   # warning in the removed serial gate is gone)
rustfmt --edition 2024 crates/ti4-mlp/examples/capture_offline_pilot.rs   # clean
```

Functional run, `--games 12` default pool (seed base 1026091300):

```text
published 2/12 games (retention vp-threshold-v1) / 3979 decisions -> out/tmp-ret-a
  retained: game 2 (standout), game 7 (standout); other 10 discarded with reasons, e.g.
  "discarded: table 10, max faction 3"
```

- `games.jsonl.zst` holds exactly the 2 retained games, indices `[2, 7]`, in order; decisions shard
  decodes to 3979 lines = manifest `records.decisions`; `retention.jsonl` has all 12 played games.
- Codex's validator passes on the retained corpus:
  `cargo run --release -p ti4-mlp --example validate_offline_corpus -- out/tmp-ret-a` →
  "non-forced decisions 2048, reconstructed legal options 12439, finite one-hot BC mean NLL
  2.908345".

Determinism proof (retention included): identical seeds with `--workers 32` vs `--workers 1`:

```text
decisions.jsonl.zst  sha256 038610fbd49b8f8808a16424a8c1317a3070005749dd209476a2bacd2168377a (both)
games.jsonl.zst      sha256 d8a65c34c6bfb8a0a7a97c6d41996f0d63a151e43fe4456fa8f4ad78479af197 (both)
```

Unit tests cover the rule boundaries: `> 6` strictly-above standout, exactly-6 not a standout,
table 24 strong / table 10 not weak, coin determinism per seed, and an empirical ~5% control rate
(20k seeds).

## Expected scale for the 32,768-game run

From codex's existing 240-game block (`E:/ti4-corpus/six-faction-block-0001-complete`, old engine
state; per-seat `final_progress.victory_points`): table VP range 4–25 (median 15), max faction VP
range 2–8. Applying the rule: ~9 standout + ~2 strong + ~11 weak + ~10 random of 240 ≈ **13%
retention** → expect roughly 4,300 retained games / ~6.6 M decisions for 32,768 games.

Size note (corrected during the run): current-engine records are far smaller than pilot-v1's —
the 240-game block's decisions shard is 222 MB (~0.9 MB/game compressed), so expect **~4–5 GB**
for the retained corpus (vs ~35 GB unfiltered), not the ~55 GB initially estimated from
pilot-v1-era record sizes. Actuals land in the corpus manifest and `retention.jsonl`.

## Known differences / caveats

- The rank-based phrasing originally discussed ("top/bottom 10%") was replaced by the operator's
  absolute VP thresholds (24/10) — those are per-game decidable, which is what makes in-stream
  write-or-discard possible. If the distribution shifts as policies improve, recalibrate from any
  corpus's `retention.jsonl` and change the constants (one-line edit, new rule version string).
- The 5% control coin uses rand's `random_range`; reproducibility is guaranteed for a given binary
  build (corpus determinism requirement), not across rand versions.
- `final_progress.victory_points` ("points scored") was used only to *estimate* the retention rate
  from old corpora; the capture itself reads authoritative final seat VP from game state.
