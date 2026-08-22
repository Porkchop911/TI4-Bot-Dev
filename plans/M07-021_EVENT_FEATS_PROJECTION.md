# M07-021 — `event_feats` state-equality projection (child of M07-019 review M2)

## Status

**Accepted 2026-08-22** (independent Tier B, Claude Opus 5 — see
`plans/M07-021_OPEN_REVIEW_ITEMS.md`; scope extension into `combat.rs` approved; N1/N2 resolved in
its Resolution section). Dependency met: M07-019 accepted and committed at `c034549`. Branch:
`wp/m07-021-event-feats-projection` from `c034549`. **Must complete before the M07-020 exit
review** so the milestone's equivalence invariant holds on the field M06 introduced. Option A
implemented (comparison added); red-first evidence, the diagnosed test-harness completion, and the
scope-extension declaration are in `plans/evidence/M07-021.md`. Coverage limit per review N1:
the invariant holds on `event_feats` for fights that do not pause — follow-up scoped as M07-022,
a dependency of M07-020.

| Field | Value |
|---|---|
| Milestone | M07 — Factions and Thunder's Edge |
| Depends | accepted M07-019 (finding F-M07-019-3 / review M2) |
| Permission class | P1 |
| Review tier | B (equivalence/projection semantics; escalates to frontier if the chosen option changes replay-hash behavior beyond adding a compared field) |

## Objective

Close the gap recorded in `plans/evidence/M07-019.md` §F-M07-019-3: `Player.event_feats`
(`state.rs:399`) — the M06 field that gates `did_at_occurrence` and therefore secret-scoring
eligibility — is not compared in `Player::PartialEq`, while the rest of the occurrence model
(`GameState.scored_feat_occurrences`, `feat_occurrence_seq`) is. Two states differing only in
`event_feats` compare equal, so the direct-vs-stepped equivalence invariant cannot fail on feat
evidence alone.

## Decision to make (recorded at implementation)

- **Option A (recommended):** add `event_feats` to the `Player::PartialEq` comparison — consistent
  with the rest of the occurrence model and with the field's own doc comment carrying no exclusion
  marker.
- **Option B:** keep it excluded but add the explicit `// Not compared.` marker plus a recorded
  reason in the `impl PartialEq for Player` doc comment (which already enumerates deliberate
  exclusions).

Choose A unless implementation shows a concrete reason the comparison breaks an accepted contract;
record the choice and rationale in evidence.

## Scoped access

```text
Writable paths:
  crates/ti4-model/src/state.rs        (the PartialEq impl / marker + focused test)
  plans/M07-021_EVENT_FEATS_PROJECTION.md
  plans/evidence/M07-021.md
  plans/EXECUTION_STATE.md
Read-only verification frontier:
  crates/ti4-engine/**                 (equivalence and replay suites)
Network/process needs: bounded Cargo test/lint commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

**Scope extension declared during implementation:** `crates/ti4-engine/src/combat.rs` (test module
only). The projection change exposed that `a_stepped_combat_matches_the_driven_one`'s stepped
harness omitted the post-combat feat bookkeeping both real drivers perform, so it could not pass
without completing that harness. The edit is test-only (+17/−1), changes no assertion (the
`identical()` comparison stands as written), and touches no engine behavior. Declared per the
M06-025 precedent; reviewer to adjudicate.

## Required checks

- A focused red-first test: two states differing only in `event_feats` must compare unequal under
  Option A (or the marker + reason must exist and a test must pin the documented exclusion under
  Option B).
- Full workspace suite before/after; if any existing equivalence or replay test depended on the old
  omission, diagnose rather than regenerate fixtures.
- Clippy/fmt on touched files; `git diff --check`.

## Definition of done

The projection decision is implemented and pinned by a focused test; all campaigns pass; evidence
records the choice, rationale, and exact results; independent review per tier B resolves; only
scoped paths commit. M07-020 may then begin with the equivalence invariant intact on `event_feats`.
