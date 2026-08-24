# M09-019 — Post-rules baseline/profile and feature inventory

## Status

**M09-019a accepted by fresh Tier-D pass-1 recheck (2026-08-24, reviewed tip `1a06ca9`).** Both
findings from the original `7ccae2e` review are resolved: checkpoint identity is verified against
the same bytes deserialized, and empty/failed panels cannot publish success. Focused **4/0**,
ti4-sim **36/0**, lint/format/diff clean, and the real panel remains byte-identical. M09-019b is
now dependency-ready and retains the row's required Tier-D pass 2.

**M09-019a complete and accepted.**
Base commit `9a83223` (M09-018 accepted). Branch `wp/m09-019-post-rules-baseline-profile`. Split
into M09-019a and M09-019b **before implementation** per AGENTS.md; the parent acceptance
criterion is preserved across both children.

**M09-019b Tier-D pass 2 review of `624d91c`: changes required (2026-08-24).** The focused and
workspace tests pass, but the timing campaign does not conform to the fixed M00 protocol and the
feature inventory is not yet the exact per-family pin the package claims. Findings
F-M09-019b-1..7 are recorded in `plans/M09-019_OPEN_REVIEW_ITEMS.md`. Row 019 remains open.

M09-019a result: r6 champions (`final10000.json` `accepted` map, sha256 verified against §10)
played 30 fixed seeds on the validation-role pool (sha256 verified) at a 4-round horizon —
**30/30 error-free, 0 completed, mean VP per seat 2.700/2.467/2.167/2.600/2.600/2.533,
33,825 decisions**; panel output byte-identical across three runs; both inputs' checksums
unchanged before/after (non-overwrite proof). Evidence: `plans/evidence/M09-019.md`.

## Milestone row (normative)

> | M09-019 | Post-rules baseline/profile and feature inventory | M08-019,M09-018 | M00 protocol; MLP plan §§2,7 | P2 r6 validation re-baseline plus bounded profile with raw samples and two independent frontier reviews; no optimization bundled. |

Dependencies met: M08-019 accepted (`aa15a39`), M09-018 accepted (`9a83223`).

## Why this package exists (normative sources)

- MLP plan §8 risk table: "Prior VP numbers become non-comparable after M06-021a. **M09-019
  re-baselines r6 on the corrected engine before any MLP comparison.**" The surviving r6 champion
  (`out/stage2_r6/`) was trained and measured against the pre-rules engine; every later number in
  this branch (shadow gates M09-029, distillation comparisons M10) must be read against a baseline
  measured on the current tree.
- MLP plan Phase 2 exit: "Re-baseline engine/feature/model time … Reproducible profile evidence
  exists." The pre-rules timing figures are stale after the M06–M08 rework (event-scoped secret
  scoring, invasion legality, canonical choice-option ordering all changed per-decision work).
- §10 artifact manifest: every corpus/panel command validates artifact role and checksum before
  starting. The seed-777 `full_np8_12_holdout.json` has logical role **validation** (its filename
  says holdout); the sealed final pool does not exist yet (M09-020) and must not be used here.

## Scope

### M09-019a — r6 validation re-baseline (learned-seat runner + bounded panel)

1. **Learned seats.** `ti4-sim::run` gains a way to seat learned profiles: each player is answered
   by the `LearnedBot` of that faction's profile, with per-seat derived seed streams using the same
   discipline as `Seats::Scored` (independent stream per seat, derived from the game seed). No
   change to existing `Random`/`Scored` behavior.
2. **Bounded validation panel.** The r6 champion envelope (`out/stage2_r6/policy.json`, six schema-4
   profiles) is loaded, validated per faction, and played across a fixed seed set on the current
   post-rules engine against the **validation-role** pool `out/pools/full_np8_12_holdout.json`
   (sha256 prefix `aba33c81aa04cefb`, verified before every run). Bounded horizon; per-game metrics
   (VP, completion) and aggregate baseline numbers recorded.
3. **Non-overwrite proof.** sha256 of `out/stage2_r6/policy.json` and
   `out/stage2_r6/final10000.json` recorded before and after the panel; asserted unchanged. The
   post-rules baseline is a *measurement*, stored as evidence — it never rewrites pre-rules weights.
4. **Determinism.** Same seed twice → identical results (all `GameResult` fields except wall-clock
   `seconds`).

### M09-019b — Bounded profile with raw samples + feature inventory

1. **M00 protocol timing re-baseline** of engine/feature/model time on the post-rules tree: 10
   warmups, ≥30 timed samples, monotonic elapsed nanoseconds, predeclared variance thresholds,
   semantic gate (workload ran the shape it was asked for), raw samples preserved in `out/` and
   summarized with variance in evidence. No optimization bundled — measurement code only; no engine
   or policy change is made to improve a number.
2. **Feature inventory.** Catalog of the current feature families in `ti4-policy::features`
   (extractor → head mapping, factual vs hashed), committed as an evidence table with a pinning
   test so rows 021–023 can show diffs against it.

## Permission class

**P2 — bounded panel/profiler output.** Runs simulations and profilers; writes measurement outputs
to gitignored `out/` (task-specific directories, deleted or retained per the artifact policy);
commits plans/evidence only. No network, no new dependencies, no external state effects. The two
r6 checkpoints are read-only inputs; their checksums are asserted unchanged.

## Review tier

**D — performance evidence.** Two independent frontier reviews: one over M09-019a (baseline
methodology + determinism), one over M09-019b (timing protocol + inventory). Both must be resolved
before M09-019 closes; the parent row is not accepted on a single pass.

## Non-goals

- No optimization of any kind (the row says "no optimization bundled").
- No changes to engine legality, feature construction, or inference numerics — measurement only.
- No use of final-role data (does not exist yet; M09-020 seals it).
- No archiving of checkpoints into Git (M09-020 owns durable fixtures).
- No schema-6/MLP work (rows 025+).

## Writable paths (parent, per child)

- **M09-019a:** `crates/ti4-sim/src/run.rs` (learned seats), `crates/ti4-sim/src/baseline.rs`
  (new panel module), `crates/ti4-sim/src/lib.rs` (+2 registration lines), plans files.
- **M09-019b:** `crates/ti4-sim/src/profile.rs` (new timing module) or extension of `baseline.rs`,
  `crates/ti4-policy/src/features.rs` (**test module only**, inventory pinning test), lib.rs,
  plans files.

Any path not listed here requires a declared scope extension before the edit.
