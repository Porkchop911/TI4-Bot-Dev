# Compact handover — Phase 9, tenth batch (2026-09-02)

## Objective
Phase 9 rules-verification, tenth batch: Expedition, Exploration, Game Board, Game Round,
Hyperlanes, Influence, Initiative Order — read each topic's official LRR text against the
engine, fix defects found, re-baseline, and hand over a clean tree.

## Normative source versions
- Official rules text: tirules2.com (fetched per topic: `/R_exploration`, `/R_expedition`,
  numeric slugs 39/40/44/47/48). No historical Python reference was consulted this batch.
- Behaviour baseline: **v28** (re-baselined this batch from v27; no bound breached).

## Active milestone/package
- **Phase 9 verification, tenth batch — COMPLETE.** Committed at `6b7cbf4` on
  `wp/r01-review-viewer-contract`.

## Status and completed acceptance criteria
- **Exploration (LRR 35): VERIFIED** after two fixes. **Game Board (39), Game Round (40),
  Influence (47), Initiative Order (48): VERIFIED.** **Expedition (TE): PARTIAL** (end-of-turn
  timing modelled as a turn-consuming component action; sixth-slice TE placement ABSENT).
  **Hyperlanes (44): PARTIAL** (standing 6.4/44 gap re-confirmed, no new issues).
- **Three defects fixed**, all on the Dark Energy Tap / frontier path:
  1. `note_arrival` explored a frontier token on *any* arrival (no permission check); it now
     only announces `SHIP_MOVED`.
  2. The DET trigger (tactical action ends on a frontier token with 1+ of your ships) had no
     code at all; it now fires in `close_tactical`, the one place every tactical action ends —
     so a fleet already parked on the token explores when its owner acts there.
  3. DET's retreat relaxation (adjacent systems with no other players' units, even without own
     presence) is the *union* with 78.7c for holders only — the technology waives only the
     own-presence clause, and its "units" is stricter than 78.7c's "ships".
- **Recorded open (separate packages, not fixed):** 35.8a exploration-deck reshuffle (no
  exploration discard pile exists), 35.3 simultaneous-exploration order (fixed order, not
  asked).
- Four new engine tests pin the behaviour. Engine suite now **1,094**.

## Current branch and HEAD
- Branch `wp/r01-review-viewer-contract`, HEAD `6b7cbf4` (tenth batch) on `ce66438` (ninth).

## Working-tree state
- Clean. The only untracked paths are `sample.html` and `sample.ti4review.json` — pre-existing
  byproducts that are never staged.

## Tests last run and exact results
- Clippy, `RUSTFLAGS=-D warnings`, all five core crates (`ti4-model`, `ti4-content`,
  `ti4-engine`, `ti4-policy`, `ti4-sim`), `--all-targets`: clean.
- `cargo test -p ti4-engine`: **1,094 passed, 0 failed**.
- `cargo test -p ti4-policy`: **189/189 passed** (the nested-window campaign passes on the
  re-verified `NESTED_WINDOW_SEEDS`).
- `cargo test -p ti4-sim` (release, `LIBTORCH=D:/Projects/ti4-engine-rs/out/libtorch-2.9.1-cpu`):
  **52/52** against the v28 bounds.

## Compatibility evidence
- `engine-rules-audit.md`: topic rows, +5 defect rows (3 fixed, 2 open), counter now
  "twenty-seven defects… nineteen fixed, eight open", totals 0 wrong / 5 absent / 33 partial /
  41 verified / 11 ok / 1 OOS / 18 unverified.
- `plans/evidence/M08-021.md`: v28 section with the v27→v28 raw side-by-side.

## Decisions made and rationale
- **Re-baselined v27 → v28** through the versioned process: the DET/frontier fixes change POK
  sim behaviour, and every `now` value stayed inside the v27 intervals, so this is a
  re-derivation, not a repair of a breach. `faction_differentiation` shifted most, [0.452,
  1.047] → [0.490, 1.071]; `vp_pace` [0.406, 0.460] → [0.397, 0.455].
- **Policy campaign seeds re-verified**, not the criterion relaxed: the DET/frontier fixes
  shifted every campaign trajectory off the six hand-picked `NESTED_WINDOW_SEEDS` (the ~3%
  mid-window scorer re-offer event is rare). A scan of the reserved range 7787-7999 at
  rotation 0 yielded seven seeds (7793, 7850, 7864, 7893, 7907, 7924, 7992); a follow-up
  pass over rotations 1-2 found 7850 re-offers there as well. This is the "new seeds" remedy
  the test's own comment documents — a fixture update, not a Phase 9 rule change.

## Open review findings or blockers
- None blocking. Carried open items remain the recorded exploration gaps (35.8a, 35.3) and the
  standing Hyperlanes/6.4 content-authoring package.

## Next exact action/command
- Begin Phase 9, **eleventh batch** (next seven alphabetically): Leader Sheet, Leaders,
  Legendary Planets, Mecatol Rex, Mechs, Modifiers, Move. Fetch each topic's LRR text, read
  every numbered sub-rule against the code, fix any defect (failing test first), then run the
  full gate (clippy `-D warnings` on the five core crates; engine; policy; sim vs v28) and
  re-baseline only if engine behaviour changed.

## Files to read first after compaction
- This file, then `plans/EXECUTION_STATE.md` ("Current position", newest section first),
  `engine-rules-audit.md` (topic table + defect ledger), `plans/SCOPED_PERMISSIONS.md`.