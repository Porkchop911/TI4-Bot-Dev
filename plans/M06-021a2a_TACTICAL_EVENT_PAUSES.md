# M06-021a2a — Tactical combat event pauses

## Package details

| Field | Value |
|---|---|
| ID | M06-021a2a (child of M06-021a2) |
| Milestone | M06 — General rules |
| Depends | M06-021a1 implementation and verification |
| Branch | `wp/m06-021a-event-scoped-secret-timing` |
| Permission class | P1 |
| Review tier | C — timing and scoring legality; bundled parent review in M06-021a2b |

## Objective

Pause a tactical action immediately after each implemented space-cannon offense,
anti-fighter barrage, and completed space combat when that occurrence makes a
secret scoreable. Resume the exact next tactical substep only after that window
closes.

## Normative sources

- FFG *Living Rules Reference 2.0*, rule 61.7: at most one objective during or
  after each combat; space combat is distinct from ground combat; any number may
  be scored during an action turn.
- Printed secret-objective timing/requirements for *Turn Their Fleets to Dust*,
  *Fight with Precision*, and the completed-space-combat secrets.
- M06-021's Tier-C finding and M06-021a1's typed occurrence/scoring contracts.

## Scoped access

```text
Permission class: P1
Writable paths:
  crates/ti4-engine/src/game.rs
  crates/ti4-engine/src/combat.rs
  crates/ti4-engine/src/objectives.rs
  plans/M06-021a_EVENT_SCOPED_SECRET_TIMING.md
  plans/M06-021a2a_TACTICAL_EVENT_PAUSES.md
  plans/evidence/M06-021a2a.md
  plans/EXECUTION_STATE.md
  plans/M06_GENERAL_RULES.md
  docs/MLP_PLAN.md
Read-only external paths: none
Network access: none
Processes/ports: bounded Cargo test/lint commands only; no ports
Generated artifacts: Cargo target output only
Destructive actions: none
External-state changes: none
```

## Invariants and non-goals

- A pause stores the exact occurrence and resumes exactly once, without skipping
  or replaying a tactical substep.
- Space cannon uses unlimited action-event scoring. Anti-fighter barrage and completed space
  combat share one combat occurrence and use `OnePerPlayer`, preserving the barrage timing while
  preventing two scores for the same combat.
- No legacy turn-scoped action scoring opens from `Game::advance_turn` after a
  tactical action.
- M06-021a2b owns ground combat, bombardment, control-loss, pass, agenda, replay
  integration, and the parent Tier-C review. Its occurrence granularity is fixed:
  each defended planet's ground combat is one combat occurrence, while each
  bombardment step is a separate non-combat occurrence with unlimited scoring.
- No change to dice, combat casualties, initiative order, or secret redaction.

## Tests and evidence

Write failing focused tests first for: AFB pause before ordinary space-combat
dice; space-combat score cap and resume into invasion/production; and
space-cannon occurrence attribution. Run scoped formatting, focused tests,
affected-crate tests, workspace tests, and lint. Record exact commands/results,
remaining emitter gaps, and review status in `plans/evidence/M06-021a2a.md`.

## Definition of done

The three tactical trigger families are occurrence-scoped and paused at their
printed substep; focused and affected checks pass; no legacy tactical trigger
opens at turn advance; changes are ready for the bundled parent Tier-C review in
M06-021a2b.
