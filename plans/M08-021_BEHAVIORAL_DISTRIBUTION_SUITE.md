# M08-021 — Behavioral distribution suite (F-M08-017-1 requirement)

## Preparation status

Dependency-safe specification only. Scoped by the operator's disposition of F-M08-017-1
(2026-08-22, adopting the M08-017 frontier review's recommendation as-is): **this suite is
required before M08-019 closes.** Begin only after M08-017 and M08-020 are accepted — the
baseline must be built on corrected ground-combat behavior (KD-2), or it would bake a known
deviation into every future comparison.

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots |
| Depends | accepted M08-017, M08-020; blocks M08-019 (hard ordering) |
| Permission class | P1 |
| Review tier | B (independent Qwen + milestone integration test); escalates to frontier if the bounds methodology or a re-baseline is in question |

## Why this exists

The authored bot (`ScoredBot`) is the **comparison baseline** the learned policy is measured
against; the programme's promotion gates are mean-VP differences measured against it. Today, a
silent change to the bot — or to engine behavior it plays against — invalidates every cross-time
VP comparison (including the MLP Phase 8 ablation) with nothing to detect it: M08-015 was never
built (M08-017 Part 4). Determinism pins catch *run-to-run* drift; this suite catches
*version-to-version* behavioral drift.

## Objective and deliverable

A paired-seed behavioral-distribution suite for the authored bot, in `ti4-sim` (which owns
`play()`, batch infrastructure, and `GameResult`):

1. **Fixed seed set** — a named, committed list of seeds (not regenerated per run), large enough
   that the metrics below are stable to one game's noise; size justified by the baseline run's
   observed variance.
2. **Metrics**, computed from `GameResult` and decision logs:
   - **Action mix** — frequency distribution over choice kinds / action types taken;
   - **VP pace** — victory points per round trajectory (per seat, aggregated);
   - **Completion** — fraction of games reaching a clean end state without error or horizon
     cutoff;
   - **Faction differentiation** — spread of behavior/VP across the six seated factions.
3. **Baseline + bounds** — the first run records the baseline in `plans/evidence/M08-021.md` with
   its exact protocol (seeds, content scope, horizon, roster). Statistical bounds are derived from
   that run by a stated method (e.g., bootstrap confidence intervals over seeds per metric) and
   **approved at review**. The suite's test asserts the current tree's metrics fall within the
   recorded bounds; out-of-bounds is a failure to diagnose, not a reason to re-baseline.
4. **Re-baseline discipline** — bounds may be changed only by a versioned process: record old and
   new values side by side in evidence, state the semantic cause (which package changed what),
   and get review approval. This is the same discipline as M06's 2.935 → 2.958 comparability note.

## Required behavior and tests

- The suite runs green on the current tree with bounds from its own baseline run.
- A mutation check proves it is load-bearing: a deliberate, scoped behavioral change to `ScoredBot`
  (e.g., one scorer constant) moves at least one metric out of bounds or visibly shifts the
  distribution; revert restores green. The mutant and its effect are recorded in evidence.
- Determinism precondition re-asserted inside the suite: two runs from the same seed set produce
  identical per-seed results before any comparison (so a flaky bound can never hide an engine
  nondeterminism regression).

## Scoped access (declared at start, before any finding exists)

```text
Writable paths:
  crates/ti4-sim/src/            (new module or test file for the suite; no changes to play()/GameResult semantics unless a demonstrated need is recorded)
  plans/M08-021_BEHAVIORAL_DISTRIBUTION_SUITE.md
  plans/evidence/M08-021.md
  plans/EXECUTION_STATE.md
Read-only supporting paths:
  crates/ti4-policy/src/bot.rs, crates/ti4-sim/src/run.rs
  plans/evidence/M08-017.md, plans/KNOWN_DIFFERENCES.md
Network/process needs: bounded Cargo test commands only (suite runs are CPU-bound game batches)
Generated artifacts: Cargo target output and bounded ignored baseline logs only
External-state effects/destructive actions: none
```

## Non-goals

No learned-policy comparison in this package (that is the MLP plan's job, built on this
baseline); no re-implementation of waived rows 014/016; no engine behavior changes.
