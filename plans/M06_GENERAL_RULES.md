# M06 — General rules

## Goal

Port current non-faction rules families without silently expanding or shrinking implementation coverage.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
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

## Exit gate

All applicable non-faction rules tests pass, registry coverage agrees exactly, and payment/scoring
operations are atomic under property and mutation testing.

