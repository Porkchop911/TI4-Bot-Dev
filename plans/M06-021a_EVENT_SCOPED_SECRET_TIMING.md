# M06-021a — Event-scoped secret-objective timing correction

## Package details

| Field | Value |
|---|---|
| ID | M06-021a1 (child of M06-021a) |
| Milestone | M06 — General rules |
| Depends | M06-021 independent tier-C finding |
| Branch | `wp/m06-021a-event-scoped-secret-timing` |
| Permission class | P1 |
| Review tier | C — timing, legality, and hidden information |

## Objective

Implement the typed occurrence model and event-scoring semantics required by the
parent correction. M06-021a2 will wire each engine emitter and exact pause point.

## Atomic split

The original M06-021a row spans six source modules and two distinct risks, exceeding
the package standard's normal atomic limit. Its acceptance criterion is preserved as:

| Child | Scope | Dependency | Completion boundary |
|---|---|---|---|
| M06-021a1 | occurrence IDs/scopes, secret eligibility, scoring-window caps/sequencing | M06-021 finding | model-level semantics and focused tests; no emitter wiring |
| M06-021a2a | tactical pause orchestration plus space cannon, anti-fighter barrage, and space-combat emitters | M06-021a1 | exact tactical substep ordering and focused tests |
| M06-021a2b | bombardment, control-loss, pass, and agenda emitters | M06-021a2a | parent end-to-end tests and fresh independent tier-C review |

## Normative sources and scope

- FFG *Living Rules Reference 2.0*, rule 61.7: any number of objectives may be
  scored during an action turn or agenda phase; at most one may be scored during
  or after each combat; space and ground combat are separate occurrences.
- Printed timing/requirements in the embedded secret-objective content.
- Accepted Rust contracts for atomic choice application, stable IDs, replay
  determinism, and typed redacted views.
- `plans/evidence/M06-021.md` records the review finding. Historical Python code
  is not an acceptance source and will not be inspected for this package.

## Scoped access

```text
Writable paths:
  crates/ti4-model/src/state.rs
  crates/ti4-engine/src/secrets.rs
  crates/ti4-engine/src/objectives.rs
  plans/M06-021a_EVENT_SCOPED_SECRET_TIMING.md
  plans/evidence/M06-021a1.md
  plans/EXECUTION_STATE.md
Read-only external paths: none
Network access: none (the official source was reviewed during plan review)
Processes/ports: bounded Cargo test/lint commands only; no ports
Generated artifacts: Cargo target output only
Destructive actions: none
External-state changes: none
```

## Invariants and non-goals

- A triggering fact is tied to its concrete occurrence; stale facts never create
  a later offer. This child exposes the typed boundary; M06-021a2 supplies events.
- One secret objective per player may be scored during/after a combat occurrence.
  Space and ground combat do not share that limit.
- Eligible non-combat action/agenda objectives are offered sequentially until no
  eligible objective remains or the player declines.
- M06-021a2b will make Become a Martyr trigger only on losing control of a
  home-system planet; this child does not wire that event.
- Opponents' held secrets remain inaccessible; replay and option IDs remain
  deterministic; rejected transitions remain atomic.
- Do not expand secret coverage, change public-objective scoring, alter authored
  bot behavior, or add objective-progress features in this package.

## Tests and evidence

Add rules-traced unit tests for occurrence matching and attribution, combat caps,
sequential non-combat scoring, and decline semantics. Existing state-view,
ordering, and illegal-choice tests remain the governing evidence for unaffected
redaction, determinism, and atomicity contracts. M06-021a2a adds tactical
pause/ordering tests; M06-021a2b adds remaining emitter and replay integration
tests. Run formatting, focused tests, affected-crate tests, the
workspace suite, and linting (strict for `ti4-model`; record existing unrelated
engine warnings). `plans/evidence/M06-021a1.md` must record exact
commands/results and the official rule source. The independent tier-C review
closes the parent in a2.

## Definition of done

All child invariants and tests pass; no historical Python comparison is claimed;
the scoped changes are committed as M06-021a1 only. M06-021a2a/a2b remain required
for the parent behavior and independent tier-C review.
