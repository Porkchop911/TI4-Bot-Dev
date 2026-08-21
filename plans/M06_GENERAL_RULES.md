# M06 — General rules

## Goal

Implement the accepted non-faction rules scope without silent expansion/shrinkage, including exact
official objective timing and exact payment-backed progress semantics.

## Work packages

Rows 001–020 retain the historical source labels under which they were executed.
For rows 021 onward, official rules and accepted Rust specifications are normative;
Python parity is not an acceptance criterion.

| ID | Package | Depends | Normative source / context | Deliverable and acceptance test |
|---|---|---|---|---|
| M06-001 | Generic payment planner | M05 | production/objectives/strategy payments | Atomic multi-currency/disjoint-planet planner with exhaustive small-state properties. |
| M06-002 | Trade economy | 001 | `transactions.py`, trade tests | Commodities, trade goods, replenishment, exchange, transaction validation. |
| M06-003 | Structured transactions | 002 | `transactions.py` | Offers/transfers/promises currently represented; hidden ownership validated. |
| M06-004 | Technology prerequisites | M02 | `technology.py` | Colors, skips, prerequisites, ownership, deterministic legal research set. |
| M06-005 | Research and upgrades | 001,004 | technology/faction tech tests | Payment, gain, unit upgrade, exhaust/ready, events. |
| M06-006 | Exploration decks | M03,M04 | `exploration.py` | Draw, resolve, purge/discard, fragments, attachments; registry coverage preserved. |
| M06-007 | Relics | 006 | `relics.py` | Fragment purge, deck, current relic effects, lifecycle. |
| M06-008 | Action-card lifecycle | M03 | `action_cards.py`, `reactions.py` | Draw/hand limit/play/cancel/discard/timing and implemented effects. |
| M06-009 | Public objectives | 001,M05 | `objectives.py` | All current scoreability predicates, costs, timing, points, reveal decks. |
| M06-010 | Secret objectives | 009 | `secrets.py` | Status/action/agenda/combat timing, limits, score/purge behavior. |
| M06-011 | Victory sources and finish | 009,010 | victory tests/game | Attribution, support points, custodians, target VP, winner timing. |
| M06-012 | Promissory notes | 002,011 | `promissory.py` | Support for the Throne end-to-end and current content-only boundary for others. |
| M06-013 | Agenda voting | 001,M04 | `agenda.py` | Eligible voters, influence payment, abstain, tie, speaker, result. |
| M06-014 | Agenda effects/laws | 013 | `agenda.py`, `laws.py` | Current handlers and exact unimplemented directives; no invented resolution. |
| M06-015 | Leaders lifecycle | M03 | `leaders.py` | Lock/unlock/ready/exhaust/purge and generic timing hooks. |
| M06-016 | Generic reactions | 008,015 | `reactions.py` | Event eligibility, declines, frequency, current general effects. |
| M06-017 | Rule registry ledger | 004–016 | all registries | Implemented/partial/unimplemented counts and names match Python. |
| M06-018 | General differential suite | 001–017 | corresponding test families | Choice/event/state fixtures ported family by family. |
| M06-019 | Payment/parser fuzzing | 001–016 | — | Malformed content and generated payments never panic or partially mutate state. |
| M06-020 | Frontier critical review | 001–019 | — | Review payments, hidden information, scoring, victory, laws, and coverage claims. |
| M06-021 | Feat ledger and fourteen secret paths (implemented; finding open) | 020 | FFG LRR 2.0 §61.7, printed timings | Historical merged implementation and evidence; not accepted until 021a. |
| M06-021a | Event-scoped secret timing correction (accepted; split below) | 021 finding | FFG LRR 2.0 §61.7, printed action/agenda secret timings | Parent acceptance criterion preserved across M06-021a1/a2a/a2b; accepted after a2b and its resolved tier-C review. |
| M06-021a1 | Occurrence model and event-scoring semantics | 021 finding | FFG LRR 2.0 §61.7, printed secret timings | Typed occurrence IDs/scopes, feat matching, combat one-score cap, non-combat sequential scoring, and deterministic unit tests; no emitter wiring. |
| M06-021a2 | Exact event-emitter wiring and integration (parent; split below) | 021a1 | FFG LRR 2.0 §61.7, printed secret timings | Parent acceptance criterion preserved across M06-021a2a/a2b. |
| M06-021a2a | Tactical combat event pauses | 021a1 | FFG LRR 2.0 §61.7, printed combat timings | Pause after space-cannon offense, anti-fighter barrage, and each space-combat occurrence before the next tactical substep; one score per combat; deterministic event-order tests. |
| M06-021a2b | Remaining emitters and parent integration | 021a2a | FFG LRR 2.0 §61.7, printed action/agenda secret timings | Wire bombardment, control loss, pass, and agenda resolution; separate space/ground, multi-agenda, attribution/redaction, replay, atomicity tests; tier-C review. |
| M06-022 | Counting-family objective progress | 021a2b | Accepted Rust scoring predicates | Exact counts expose progress while `satisfied` preserves existing legality. |
| M06-023 | Bespoke and bought-cost progress | 022 | Accepted Rust predicates/payment planner | Count families plus greatest exactly-affordable scaled cost; no heuristic or duplicate-feature summation. |
| M06-024 | Reopened frontier critical review | 021a2b–023 | — | Resolve timing, scoring, payment, hidden-information, property, and workspace findings before M09 additions. |

## Exit gate

All applicable non-faction rules tests pass, registry coverage matches the accepted Rust scope,
event-scoped objective timing satisfies the named official rules, and payment/scoring operations are
atomic under property and mutation testing. M06-024 has no unresolved finding.
