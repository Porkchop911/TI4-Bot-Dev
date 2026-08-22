# M07-019 — Post-M06 faction and Thunder's Edge revalidation

## Status

**Accepted 2026-08-22** (independent Tier B, Claude Opus 5 — see
`plans/M07-019_OPEN_REVIEW_ITEMS.md`; all corrections M1–M3 applied and recorded in its Resolution
section). Both dependencies were met: M06-024 (and M06-025) accepted at closure commit `b721a9a`;
M07-018 is part of the accepted M07 baseline on this branch's history. Branch:
`wp/m07-019-post-m06-revalidation` from `b721a9a`. This package does not reopen or expand the
accepted faction/content scope. Deliverable: four nested-window regression tests in `game.rs`
(test module only, +588/−0) plus three recorded findings (F-M07-019-1 structures-as-ground-
defenders, escalated to frontier; F-M07-019-2 phantom-round dice consumption after a total-wipe fwp
pause; F-M07-019-3 `event_feats` missing from state equality, scoped as child M07-021). No
demonstrated M06 regression required a source fix. Evidence: `plans/evidence/M07-019.md`.

| Field | Value |
|---|---|
| Milestone | M07 — Factions and Thunder's Edge |
| Depends | accepted M06-024 (closure `b721a9a`) and M07-018 (accepted M07 baseline) |
| Permission class | P1 |
| Review tier | B (independent Qwen plus milestone integration tests; any finding that changes timing/legality semantics escalates to frontier per AGENTS.md) |
| Compatibility | accepted Rust faction/TE contracts and official objective timing |

## Objective

Revalidate the accepted faction and Thunder's Edge behavior after occurrence-scoped secret windows
and objective-progress queries, fixing only demonstrated integration regressions in timing-stack,
effect-scope, legal-choice, replay, or redaction boundaries.

## Scoped access

```text
Writable paths:
  crates/ti4-engine/src/faction_abilities.rs
  crates/ti4-engine/src/game.rs
  crates/ti4-engine/src/timing.rs
  crates/ti4-engine/src/thunders_edge.rs
  crates/ti4-policy/src/bot.rs
  plans/M07-019_POST_M06_REVALIDATION.md
  plans/evidence/M07-019.md
  plans/EXECUTION_STATE.md
Read-only supporting paths:
  crates/ti4-engine/src/{combat,invasion,objectives,secrets}.rs
  plans/evidence/M06-024.md
Network/process needs: bounded Cargo test/lint/replay commands only
Generated artifacts: Cargo target output and bounded ignored replay logs only
External-state effects/destructive actions: none
```

Do not touch a listed source file unless a focused red test demonstrates a regression there. A
finding outside these paths becomes a scoped child package and blocks completion.

## Required invariants and tests

- Faction/TE reactions nested around event-secret scoring resume at the exact retained timing frame;
  no effect is skipped, repeated, or moved across space cannon, barrage, combat, invasion, pass, or
  agenda boundaries.
- Sequence-scoped effects expire by their established combat/production/activation identity even
  when an inner scoring window is declined, exhausted, or rejects an invalid choice.
- Every authored faction/TE decider receives only legal stable options and authorized observations;
  held secret aliases and another player's occurrence eligibility remain redacted.
- Direct and stepped tactical APIs produce equivalent faction/TE state and canonical replay hashes.
- Existing implemented/partial/unimplemented registries and counts remain byte-for-byte unchanged.
- Add nested-window focused regressions for at least one combat reaction, one invasion/control
  reaction, one production/activation-scoped effect, and one TE timing path.
- Run all faction, TE, timing, replay, observation-redaction (the `choice::redacted_for` tests and
  the model view tests — there is no suite named "redaction"), engine, policy, and workspace suites
  plus Clippy and `git diff --check`.

## Known traps

- The M06 event-scoped windows pause tactical resolution mid-combat (space cannon, barrage,
  combat occurrence) — a faction reaction that was in flight when the window opened must resume at
  the exact retained frame; the failure mode is a skipped or doubled effect, not a crash.
- `timing.rs` frames are identity-bearing: an effect scoped to one combat/production/activation
  identity must not survive a declined or exhausted inner scoring window under a new identity.
- Redaction is enforced at the observation boundary (typed views), so a regression shows up as a
  leaked alias in a decider's options, not as an illegal choice being accepted.
- Replay hashes compare canonical projections: any new state field written by a faction/TE path
  must be part of the projection or the direct-vs-stepped equivalence test fails.
- Do not "fix" a red test by widening a registry or adding content — that is scope expansion and
  blocks completion; it becomes a scoped child package instead.

## Non-goals and evidence

Do not implement new faction or TE content, change registries, tune bot weights, add policy features,
or claim Python parity. `plans/evidence/M07-019.md` records exact base/head commits, demonstrated
regressions (including none), tests, replay hashes, reviewer/findings, and the unchanged scope ledger.

## Definition of done

Both dependencies are accepted; required nested-window and affected suites pass; every demonstrated
regression is fixed without scope expansion; registry/redaction/replay invariants hold; independent
Tier-B review findings are resolved; evidence is complete; and only scoped paths are committed.
