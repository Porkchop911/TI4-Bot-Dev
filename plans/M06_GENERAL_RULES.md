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
| M06-023 | Remaining position and bought-cost progress | 022 | Accepted Rust predicates/payment planner | Six public and seventeen secret position families plus greatest exactly-affordable scaled cost for ten bought objectives; unavailable map state remains distinct. |
| M06-024 | Reopened frontier critical review (accepted) | 021a2b–023 | — | F1 issuer-resolution fix with four red-first regression tests; F2 escalated to M06-025; J1 instrumentation run recorded; independence limitation recorded. |
| M06-025 | Play-area note scoring for baf and sb (accepted) | 024, 021a, 023 | Printed card text; corpus `playArea` field | Content-driven face-up model over the eleven play-area notes; baf/sb count play-area notes only with M06-021a timing and M06-023 deduplication preserved; K1 issuer-from-key fix. |

## Exit gate

All applicable non-faction rules tests pass, registry coverage matches the accepted Rust scope,
event-scoped objective timing satisfies the named official rules, and payment/scoring operations are
atomic under property and mutation testing. M06-024 has no unresolved finding.

### Closure record (2026-08-21)

**M06 is closed.** Both reopened packages are accepted by independent Tier-C review (Claude Opus 5,
distinct from all implementers of the reviewed code):

- M06-024: `plans/M06-024_OPEN_REVIEW_ITEMS.md` — F1 accept (red-before-green-after reproduced
  independently), F2 confirmed and escalated, J1 resolved by one instrumented 150-game run
  (`crates/ti4-training/examples/feat_activation_probe.rs`: baf live end-to-end at 313 records / 11
  scores; fwp 21 and bam 48 records with zero scores consistent with rare alignment, full scoring
  loops proven by unit tests), independence limitation recorded.
- M06-025: `plans/M06-025_OPEN_REVIEW_ITEMS.md` — accept. L1 (eight faction play-area notes cannot
  fire under D11's six-faction roster; standing re-check condition for any future roster widening)
  and L2 (M06-023's measured sb gain of 91% was counting hand-held notes the card excludes) recorded
  in evidence; L3 resolved by comment.
- Final verification: engine 839 + 5 doctests; workspace 18 suites / 1,312 passed / 0 failed,
  deterministic across two runs; exhaustive payment campaigns pass; Clippy clean on all touched
  files (three documented pre-existing warnings elsewhere); rustfmt and `git diff --check` clean.
- **Independence limitation (carried per adjudicator's instruction):** the same frontier reviewer
  reviewed M06-021a…024 at package level and then adjudicated this exit; F2 was found by the
  implementer, not the reviewer. No second ever-independent adjudicator was available in this
  session; recorded here rather than left implicit.
- **Known-difference ledger:** baf/sb now count play-area notes only — downstream VP/clearance
  numbers are non-comparable until re-baselined (pre-M06-025 baseline mean VP per seat 2.935;
  post-M06-025 probe run 2.958 on the same protocol). The eight faction play-area notes outside
  D11's roster are untestable until a future package widens it (standing condition in
  `plans/evidence/M06-025.md`).
- No command run by any M06 package wrote to the historical Python reference.

**Next ready package:** M07-019 (post-M06 faction/TE integration revalidation; dependencies
M06-024 accepted and M07-018 part of the accepted M07 baseline).
