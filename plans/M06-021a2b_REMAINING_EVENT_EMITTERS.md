# M06-021a2b — Remaining event emitters and parent integration

## Package details

| Field | Value |
|---|---|
| ID | M06-021a2b (child of M06-021a2) |
| Milestone | M06 — General rules |
| Depends | M06-021a2a tactical pauses and its focused checks |
| Branch | `wp/m06-021a-event-scoped-secret-timing` |
| Permission class | P1 |
| Review tier | C — timing, scoring legality, hidden information |

## Objective

Replace every remaining turn-scoped action/agenda secret emitter with one exact occurrence,
pause at its printed timing when a secret is scoreable, and remove the retired turn-scoped path.

## Normative sources

- FFG *Living Rules Reference 2.0*, 42, 49, 61.7, and printed secret timing/requirements.
- M06-021a occurrence/scoring contract and Tier-C review findings F2, F4, F6.

## Scoped access

```text
Permission class: P1
Writable paths:
  crates/ti4-engine/src/game.rs
  crates/ti4-engine/src/invasion.rs
  crates/ti4-engine/src/objectives.rs
  crates/ti4-engine/src/secrets.rs
  crates/ti4-model/src/state.rs
  plans/M06-021a2b_REMAINING_EVENT_EMITTERS.md
  plans/M06-021a_OPEN_REVIEW_ITEMS.md
  plans/evidence/M06-021a2b.md
  plans/EXECUTION_STATE.md
Read-only external paths: none
Network access: none
Processes/ports: bounded Cargo test/lint commands only; no ports
Generated artifacts: Cargo target output only
Destructive actions: none
External-state changes: none
```

## Invariants and non-goals

- Each bombardment step receives a separate non-combat occurrence with unlimited scoring.
- Each defended planet's ground combat receives its own `OnePerPlayer` occurrence.
- A home-planet control loss, last pass, and each agenda outcome receive separate unlimited
  action/agenda occurrences with exact owner attribution.
- A failed event-secret award cannot fall through to public-objective award or leave that player
  eligible forever (F4).
- Retire `LegacyTurn`, the turn feat ledger, and `Game::advance_turn` event opening only after the
  final emitter has migrated (F6).
- No changes to rules outcomes, dice, public-objective status scoring, or redaction semantics.

## Tests and evidence

Add focused tests for bombardment, one-per-planet ground combat, home-control loss, last pass,
agenda attribution, occurrence isolation, failed award atomicity, replay determinism, and the
absence of legacy action scoring. Run scoped formatting, focused tests, affected crates, workspace,
and lint. Record exact commands/results and the parent Tier-C review disposition.

## Definition of done

All production secret-event emitters are occurrence-scoped; no runtime legacy event path remains;
the a2a/a2b integration checks and fresh parent Tier-C review pass; all findings are resolved or
explicitly recorded for a later package.
