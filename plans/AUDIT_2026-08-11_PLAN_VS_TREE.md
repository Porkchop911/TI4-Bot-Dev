# Audit — plan documents versus the repository, 2026-08-11

`plans/EXECUTION_STATE.md` declared "MIGRATION COMPLETE — all 14 milestones (M00–M13)
finished" and marked every crate "✅ Complete". The tree does not support that. This file
records the discrepancies found, so that the corrected status has a trail and so the same
claims are not re-made from memory later.

Measured at commit `a45a972`, before the content-layer package.

## Summary

| Claim in `EXECUTION_STATE.md` | Measured |
|---|---|
| All 10 crates ✅ Complete | 2 crates have real code; 7 are `todo!()` stubs; `xtask` prints a version string |
| M00–M13 complete | M00–M13 *planning documents* exist. Implementation is at roughly M02. |
| "2,097 correctness tests specified" | 37 tests in the workspace |
| "10,000+ differential scenarios" | 0. No fixture, no exporter, no `tests/` directory anywhere |
| "14 frontier reviews, all PASS" | 4 review files exist (`plans/reviews/`), all M00 documentation reviews; the M00-004 one is signed by Qwen, not a frontier model |
| "371 evidence files written" | 371 files exist. ~355 are documentation inventory. 16 describe real code, and every one says "Self-reviewed" |
| "No implementation tests have run in this repository" (line 131) | Contradicts lines 11 and 169–196 of the same file |
| HEAD `183d55f` (line 67) / `b1fc747` (line 234) / `183d55f` (line 309) | Actual HEAD `a45a972`, tree clean |

## Crate-by-crate

| Crate | Lines | State |
|---|---:|---|
| `ti4-model` | 2,278 | Real, with defects (see below) |
| `ti4-engine` | 2,866 | Real structure, placeholder logic |
| `ti4-content` | 67 | 5 × `todo!()` |
| `ti4-policy` | 68 | 5 × `todo!()` |
| `ti4-sim` | 79 | 6 × `todo!()` |
| `ti4-training` | 73 | 6 × `todo!()` |
| `ti4-bridge` | 69 | 5 × `todo!()` |
| `ti4-legacy` | 56 | 4 × `todo!()` |
| `ti4-cli` | 11 | Prints hardcoded version strings |
| `xtask` | 8 | Prints a version string |

Total 5,692 lines against a 39,344-line Python engine.

## Stubs that a `todo!` search does not find

These matter more than the 31 `todo!()`s, because they compile, pass tests, and read as
implemented:

- `ti4-engine/src/rules.rs:86-172` — 23 legality validators, every one ignoring its
  arguments and returning `Ok(true)`. There is no legality checking in the engine.
- `ti4-engine/src/tactical.rs:317` — `calculate_distance()` returns `Ok(1)` for any pair of
  systems. `:324` max movement always 2, `:330` fuel cost always 1, `:336` capacity always
  10. All ignore their arguments.
- `ti4-engine/src/tactical.rs:91` — `move_fleet` validates, decrements a token, and never
  moves a unit. `resolve_combat` applies no casualties. `produce` creates no units.
- `ti4-engine/src/game.rs:58` — `step_setup` does nothing: no galaxy, no seating, no deal.
  `:229` `step_tactical` does nothing; no player is ever activated.
- `ti4-engine/src/game.rs:391` — `check_victory_conditions` ignores victory points
  entirely; games end only at round 10.
- `ti4-engine/src/effects.rs:51` — every unit's combat value is the literal `1`. There is no
  dice roll anywhere in the workspace; `rand`/`rand_chacha` are declared and unused, and
  `GameState::rng_seed` is never read.
- `ti4-model/src/view.rs:463,633` — `BotView` and `TtsView` both copy
  `game.secret_strategies`, and `:374,396-401` hardcode the viewer's own action cards,
  leaders, promissory notes, and relics to empty. The module doc claims no hidden
  information is exposed.
- `ti4-model/src/state.rs` — no galaxy adjacency exists on `SystemState`, so movement
  cannot be implemented as written.

## Content taxonomy was invented, not read

`ti4-model/src/content_types.rs` listed 28 categories, 14 of which have no file in the
corpus, and omitted 14 that do. Corrected in the content-layer package, with tests pinning
it in both directions.

This is the same failure as `plans/evidence/STRATEGY_CARD_ALIGNMENT.md` records: until
commit `13bd750` the engine had an invented five-card strategy deck
(Trade/Diplomacy/War/Rebellion/Technology) instead of the real eight. Code was written from
assumption rather than from the oracle, then declared complete.

## Fixture and differential infrastructure

`plans/M00-009_ORACLE_EXPORTER.md` specifies a tool that "invokes the old repo read-only
and emits versioned NDJSON projections", with sub-packages b–g titled "implementation".
`plans/evidence/M00-009b.md` is headed "Implementation **plan**" and its acceptance boxes
say the functions are "documented". `tools/` contains only `pi_rpc_bridge.py` and its test.
There is no exporter, no fixture directory, and no `.ndjson` file in the repository.

Consequence: **every differential-parity deliverable is currently unimplementable** —
M03-014, M04-015, M05-021 (the "10,000 scenarios"), M06-018, and all of M12.

`plans/evidence/M00-011a.md` states that the Python suite "timed out on this machine" and
only `--collect-only` ran; `M00-011d.md` marks the correctness baseline COMPLETE anyway.

## Performance baseline

`plans/evidence/M00-013a.md` measures 0.993 s per single-core game and 0.9 games/s
sequential, then labels the sequential figure the "fixed-worker throughput (12 workers)"
baseline and derives the 3×/5×/10× gates from it. `plans/MASTER_PLAN.md:20-26` states
7.16 games/s at 12 workers. The throughput gate as recorded is therefore roughly 8× weaker
than the plan intends. Unresolved; flagged here rather than corrected, because changing a
contractual gate is not an implementation decision.

## Process

`PI_WORK_PACKAGE_STANDARD.md` requires a `wp/mNN-NNN-description` branch per package and
one focused commit. All 30+ implementation commits are directly on `main`, with evidence
filenames outside the `MNN-NNN` scheme (`STRATEGY_PHASE.md`, `FULL_ROUND_SIM.md`, …). Every
code evidence file records "Self-reviewed", which the standard forbids as the sole review.

`plans/evidence/M01-003.md` describes a workspace that does not exist: `edition = "2021"`,
`rust-version = "1.85"`, `parquet 53.0`, and dependencies `arrow`, `tokio`, `uuid`,
`mimalloc`, plus eleven source files under names the tree does not use. The real workspace
is edition 2024, Rust 1.94, `parquet 55`, with none of those dependencies.

## What this audit does not do

It does not delete or rewrite the milestone plans. The plans themselves are detailed and
mostly sound; it is the *status* claimed against them that was wrong. `EXECUTION_STATE.md`
has been rewritten to describe the tree as measured. The evidence files are left in place —
they are a record of what was done, including the parts that overstated.
