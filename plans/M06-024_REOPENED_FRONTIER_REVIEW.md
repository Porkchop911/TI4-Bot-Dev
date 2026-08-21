# M06-024 — Reopened M06 frontier critical review

## Status

Review started 2026-08-21 on branch `wp/m06-024-reopened-frontier-review` (cut from `bfcdb73`).
All three dependencies are accepted and committed; their commits form the exact review frontier.
Mechanical verification campaigns run under this package; final independent Tier-C adjudication
requires a frontier-model reviewer distinct from any implementer of the reviewed code.

| Field | Value |
|---|---|
| Milestone | M06 — General rules |
| Depends | accepted M06-021a2b, M06-022, and M06-023 |
| Permission class | P1 |
| Review tier | C — timing, scoring, payments, and hidden information |
| Compatibility | accepted Rust rules and package contracts; Python parity not applicable |

## Objective

Independently determine whether reopened M06 satisfies its event-timing and objective-progress
contracts, resolve every actionable finding, and close the milestone only with reproducible rules,
property, mutation, lint, and workspace evidence.

## Exact review frontier

- Base: `92edea4`, which contains the accepted M06-020 frontier and historical M06-021 package.
- Accepted additions already fixed: M06-021a1/a2a/a2b commit `5d027e8` and M06-022 commit
  `d58622c`.
- Head (recorded before review began): accepted M06-023 commit `bfcdb73`.
- Exact review range: `92edea4..bfcdb73` — three commits: `5d027e8` (M06-021a1/a2a/a2b),
  `d58622c` (M06-022), `bfcdb73` (M06-023).
- Normative sources: the package specifications, accepted Rust scoring/payment predicates, and FFG
  *Living Rules Reference 2.0* rule 61.7 plus the printed objective timings named by M06-021a.
- Historical Python is neither a source nor an acceptance oracle and must not be inspected.

Before review, record exact base/head commits and verify every dependency evidence file names its
commands, results, reviewer, resolved findings, and commit.

## Scoped access

```text
Writable paths before a finding exists:
  plans/M06-024_REOPENED_FRONTIER_REVIEW.md
  plans/M06-024_OPEN_REVIEW_ITEMS.md
  plans/evidence/M06-024.md
  plans/EXECUTION_STATE.md
  plans/M06_GENERAL_RULES.md
Read-only review frontier:
  crates/ti4-model/src/state.rs
  crates/ti4-engine/src/combat.rs
  crates/ti4-engine/src/game.rs
  crates/ti4-engine/src/invasion.rs
  crates/ti4-engine/src/objectives.rs
  crates/ti4-engine/src/payment.rs
  crates/ti4-engine/src/production.rs
  crates/ti4-engine/src/secrets.rs
Read-only external paths: none
Network/process needs: bounded Cargo test/lint/property/mutation commands only
Generated artifacts: bounded ignored test/mutation output with command and hash recorded
External-state effects/destructive actions: none
```

Review fixes may touch a source path only after the ledger declares that exact path writable for a
documented finding. Any finding whose complete fix exceeds the atomic review-fix scope must become
a recorded child package and block the exit gate; the review must never silently shrink or waive it.

## Required independent review questions

### Known carried finding

- M06-023 review H1 found that the `WonAgainstANoteHolder` combat and invasion emitters compare a
  promissory-note owner faction with `PlayerId`. Verify both space and ground Betray-a-Friend paths
  resolve production-format `note_id(alias, faction)` keys through the seated faction owner. This
  is a pre-existing M06-021 defect outside M06-023 and is blocking unless fixed and regression-tested.

### Timing and scoring

- Does every action/agenda secret read only its exact `FeatOccurrence`, with no stale position or
  turn-level fallback?
- Do space cannon and bombardment use non-combat unlimited scoring, while barrage plus its space
  combat share one cap and every defended planet receives a separate ground-combat cap?
- Can every eligible non-combat secret be scored sequentially until decline, while a player can
  never score twice in one combat occurrence even across multiple pauses?
- Do retained windows resume at the exact substep, and do synchronous and stepped APIs agree?
- Are invalid choices, failed awards, and overflow/refusal paths atomic and visibly erroneous?

### Progress and payment

- Does every mapped count expose exact integer `have`/non-zero `threshold`, and is legality derived
  from that same result rather than a parallel predicate?
- Are distinct/max/per-colour/per-trait families reduced with the specified identity and without
  duplicate inflation? Do map-dependent families return unavailable without a map?
- For each bought objective, is reported progress the greatest exactly affordable scaled cost under
  the existing disjoint payment planner, including `AllThree` overlap traps?
- Are all progress queries observationally pure, deterministic, and unavailable for unknown aliases?

### Hidden information and persistence

- Do public/observer views reveal no held secret identity, occurrence eligibility, payment plan, or
  private note relation beyond the acting player's established boundary?
- Are occurrence counters/ledgers serialized compatibly, compared where determinism requires, and
  replay-stable across equivalent runs?

## Required campaigns

- Focused tests named by M06-021a, M06-022, and M06-023.
- Exhaustive small-state payment properties, including greatest-`k` maximality and disjoint-plan
  counterexamples.
- Event-order/replay/atomicity properties for every occurrence type and repeated scoring window.
- Full `cargo test --workspace --quiet` and all doctests.
- Strict Clippy for `ti4-model`; engine Clippy with every warning classified as pre-existing,
  fixed, or blocking.
- `git diff --check`, scope-ledger reconciliation, and deterministic repeated-test comparison.
- Bounded mutation checks for changed legality/timing/payment decision boundaries. Surviving
  mutations in a critical boundary are blocking unless an equivalent assertion is demonstrated.

## Evidence and finding protocol

`plans/evidence/M06-024.md` records the reviewer identity/model, independence, exact frontier,
commands and results, every finding with severity and source location, each fix/rejection rationale,
rerun evidence, mutation survivors, and final disposition. The implementer cannot be the sole
reviewer. A reviewer reports findings; fixes remain auditable and are independently rechecked.

No aggregate rollout rate establishes timing or payment correctness. A bounded end-to-end run may
support reachability evidence, but decision-boundary tests and properties remain authoritative. No
performance claim belongs to this package.

## Definition of done

All dependencies are accepted and committed; an independent frontier reviewer has reviewed the
exact frontier; every actionable finding and critical mutation survivor is resolved and rechecked;
all required campaigns pass; scope/test/known-difference ledgers are reconciled; evidence names the
final clean commit; and M06 is marked complete with no unresolved finding. Only then may M07-019 or
any M09 addition that depends on the reopened M06 gate begin.
