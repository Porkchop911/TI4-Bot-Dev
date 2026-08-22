# M07-022 — Stepped-vs-driven equivalence across scoring pauses (child of M07-021 review N1)

## Preparation status

Dependency-safe preparation only. Begin after M07-021 is accepted and committed; **must complete
before the M07-020 exit review** (it is now a dependency of it), because the milestone's
equivalence invariant currently holds on `event_feats` only for fights that do not pause.

| Field | Value |
|---|---|
| Milestone | M07 — Factions and Thunder's Edge |
| Depends | accepted M07-021 (finding N1 / review of the completed harness) |
| Permission class | P1 |
| Review tier | B (equivalence semantics; escalates to frontier if a pause-path divergence is found in engine behavior rather than in the test harness) |

## Objective

Close the coverage limit recorded in `plans/evidence/M07-021.md` §"Coverage limit recorded per
review N1": `a_stepped_combat_matches_the_driven_one`'s stepped branch consumes choices but not
scoring pauses, so a fight whose round-1 barrage fires a feat leaves the loop unresolved and the
harness panics at `.expect("the fight resolved")`. The direct-vs-stepped equivalence invariant must
be verified **across** the M06 pause path, not only outside it.

## Deliverables

1. **Pause consumption in the stepped harness.** Mirror `resolve()`'s loop exactly: while no
   outcome, either answer a pending choice or consume a scoring occurrence and drive automatic
   transitions (`take_scoring_occurrence()` + `settle_open()`), then complete with the same feat
   bookkeeping both production consumers perform.
2. **A pausing fixture.** Extend (or add a sibling test for) the equivalence comparison with a
   fight whose round-1 Anti-Fighter Barrage fires a feat and pauses — e.g. the shape of
   `a_driven_combat_continues_after_its_barrage_scoring_pause` (destroyer×1 vs fighter×1 +
   cruiser×1, faces `[10, 10, 10, 1]`) — so both sides are driven through the pause and their final
   states compared with `identical()`.
3. **N2 structural fix (adopted per M07-021 evidence §"N2 disposition").** Factor the completion
   bookkeeping (`before_combat` snapshot → `note_combat_event_feats` on `combat_occurrence()`) into
   one helper that both `resolve()` and the harness call, so a third copy-drift is structurally
   impossible. The Game driver's deliberate difference (`before_combat_with_notes`, carrying
   promissory notes for `baf`) stays as-is; the helper takes the snapshot as a parameter or the
   consumers keep their own snapshots — record which and why in evidence.

## Scoped access

```text
Writable paths:
  crates/ti4-engine/src/combat.rs        (harness + resolve() loop + shared helper, test module for the fixture)
  plans/M07-022_STEPPED_EQUIVALENCE_ACROSS_PAUSES.md
  plans/evidence/M07-022.md
  plans/EXECUTION_STATE.md
Read-only verification frontier:
  crates/ti4-engine/src/game.rs          (the stepped production consumer at game.rs:216 — reference only)
  crates/ti4-model/**                    (equality semantics, unchanged)
Network/process needs: bounded Cargo test/lint commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

If implementation shows the pause-path divergence lives in engine behavior (not the harness), stop
and escalate to frontier per tier — do not fix engine semantics inside this package.

## Required checks

- The new pausing-fixture comparison passes with both sides driven through the pause; a red-first
  probe demonstrating that the old harness shape panics/stalls on it is recorded in evidence.
- `resolve()` behavior unchanged: all existing combat tests pass unmodified (the helper refactor
  must be behavior-preserving — if any test's dice history or outcome changes, diagnose before
  proceeding).
- Full workspace suite ×2 (determinism), Clippy/fmt on touched files, `git diff --check`.

## Definition of done

Equivalence is verified across the M06 pause path; the bookkeeping helper makes copy-drift
structurally impossible for the synchronous API and its test replica; all campaigns pass; evidence
records the red-first probe, the helper design decision, and exact results; independent review per
tier B resolves; only scoped paths commit. M07-020 may then begin with the equivalence invariant
holding on `event_feats` inside and outside the pause.
