# M08 — Authored bots

## Goal

Port the existing scored bot, planning surfaces, and explainability without weakening legal-information boundaries.

## Work packages

Rows 001–017 retain their historical evidence. Python parity is no longer an
acceptance criterion. Rows 018–019 revalidate bot legality/observation handling
after the M06/M07 event-window correction.

| ID | Package | Depends | Historical source / normative context | Deliverable and acceptance test |
|---|---|---|---|---|
| M08-001 | Policy observation API | M07 | `views.py`, bot game proxy | Typed authorized observation; compile/API design prevents direct hidden-state access. |
| M08-002 | Base valuation | 001 | `valuation.py` | Item and state value primitives with golden component tests. |
| M08-003 | Score component schema | 001,002 | `bots.py` | Named factual/utility components, deterministic aggregation, no accidental parameter harvesting. |
| M08-004 | Choice scoring dispatcher | 003 | `bots.py` | Route all current choice kinds; unknown kind has explicit safe behavior. |
| M08-005 | Tactical scoring | 004 | bots/movement/production helpers | Activation, movement, cargo, landing, combat, production scoring. |
| M08-006 | Economy/development scoring | 004 | bots/technology/strategy | Spend, trade, research, token, strategy choices. |
| M08-007 | Objective planning | 004 | bots/objectives/secrets | Public/secret focus, partial progress, spend reservation, scoring schedule. |
| M08-008 | Tactical plans | 005,007 | `tactical_plans.py` | Plan construction, reservations, completion, invalidation, opt-in flags. |
| M08-009 | Opening features | 005–007 | `opening.py` | Round-one progress/shortfall facts and faction-neutral calculations. |
| M08-010 | Faction profiles | 003–009 | guides/baseline JSON | Load, validate, and apply existing profile weights without mutating fixtures. |
| M08-011 | Sampling/temperature | 004 | `ScoredBot.choose` | Stable seeded sampling, shortlist behavior, all legality checks. |
| M08-012 | Explanations | 003–011 | score breakdown/explain tools | Components reproduce final score and can be serialized for diagnostics. |
| M08-013 | Experimental capabilities | 007–010 | planning/capability tests | Preserve opt-in/off-by-default status and configuration compatibility. |
| M08-014 | Bot differential choices | 004–013 | bot/guide tests | Golden logits/rankings or bounded tolerance for representative choice sets. |
| M08-015 | Behavioral distribution suite | 010–014 | sim baselines | Paired seeds compare action mix, VP pace, completion, faction differentiation. |
| M08-016 | Bot performance benchmark | 014 | benchmark tool | Per-decision and game costs recorded; regression budget enforced. |
| M08-017 | Frontier information/review gate | 001–016 | — | Review hidden information, parameter leakage, determinism, and statistical acceptance. |
| M08-018 | Post-M07 bot observation/legality revalidation | M07-020,M08-017,M08-020 | Accepted Rust bot contracts | New nested scoring choices remain legal, deterministic and redacted for every authored bot; full affected suites pass. |
| M08-019 | Reopened frontier exit review | 018 | — | Resolve hidden-information, legality, determinism and downstream-regression findings before M09. |
| M08-020 | Ground-combat structure legality (F-M07-019-1 fix) | M07-020,M08-017; blocks 018 | LRR 49; M07-019 evidence, M07-020 adjudication | Structure-only planets fall without resistance (no spurious ground combat); structures die on control transfer; Assimilate-after-pause coverage load-bearing. Hard ordering before 018 so revalidation and all downstream baselines run against corrected behavior. |

## Exit gate

Authored bots select only legal actions from authorized observations, explanations reconcile, and
paired-seed behavior remains within approved statistical bounds; M08-019 has no unresolved finding.
