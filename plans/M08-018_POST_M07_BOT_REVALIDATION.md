# M08-018 — Post-M07 authored-bot observation and legality revalidation

## Preparation status

Dependency-safe specification only. Do not begin until M07-020, M08-017, and M08-020 are
accepted.

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots |
| Depends | accepted M07-020, M08-017, and M08-020 (hard ordering: bot revalidation must run against the corrected ground-combat behavior so no baseline is built on the known F-M07-019-1 deviation) |
| Permission class | P1 |
| Review tier | B |
| Compatibility | accepted Rust authored-bot legality, determinism, and redaction contracts |

## Objective

Prove every authored bot remains legal, deterministic, and correctly redacted when occurrence-
scoped secret scoring creates nested choices, fixing only demonstrated downstream regressions.

## Scoped access

```text
Writable paths:
  crates/ti4-policy/src/bot.rs
  crates/ti4-policy/src/features.rs
  crates/ti4-policy/src/lib.rs
  crates/ti4-engine/src/choice.rs
  plans/M08-018_POST_M07_BOT_REVALIDATION.md
  plans/evidence/M08-018.md
  plans/EXECUTION_STATE.md
Read-only supporting paths:
  crates/ti4-engine/src/{game,objectives,secrets}.rs
  plans/evidence/M07-020.md
Network/process needs: bounded Cargo test/lint/replay commands only
Generated artifacts: Cargo target output and bounded ignored replay logs only
External-state effects/destructive actions: none
```

Source edits require a focused red regression. Findings outside the declared paths become scoped
blocking child packages rather than broad cleanup.

## Required invariants and tests

- Every authored bot chooses only an option present in the current legal `Choice`, including
  repeated offers in unlimited action/agenda occurrences and retained combat windows.
- A bot can decline/score and resume without stale cached options, duplicate decisions, or head
  misclassification.
- Observations and explanations expose no opponent secret alias, exact private eligibility, hidden
  note relation, or private payment detail beyond established typed views.
- Identical state/choice/seed inputs yield identical rankings, selected IDs, explanations, and
  replay hashes regardless of map/hash iteration order.
- Existing authored scores and ordering outside the new nested windows remain unchanged unless a
  reviewed regression test proves the old result illegal.
- Add representative action, agenda, combat, decline, and no-eligible-secret cases for every
  authored bot configuration in accepted scope.
- Run policy/engine redaction and legality suites, deterministic repeated runs, full workspace,
  Clippy, and `git diff --check`.

## Non-goals and definition of done

Do not tune weights, add learned features, change objective progress, expand bot/content scope, or
claim Python parity. Completion requires accepted dependencies, passing required suites, resolved
independent Tier-B findings, unchanged authorized observations, complete evidence, and a focused
commit containing only demonstrated fixes and tests.
