# M08 — Authored bots

## Goal

Port the existing scored bot, planning surfaces, and explainability without weakening legal-information boundaries.

## Work packages

Python parity is no longer an acceptance criterion. The historical evidence files for rows
001–017 were **superseded by the M08-017 re-execution** (`plans/evidence/M08-017.md`): 7 rows
delivered, 2 partial, 7 absent — see Scope dispositions below. Rows 018–019 revalidate bot
legality/observation handling after the M06/M07 event-window correction; row 021 is required
before M08-019 closes per the operator's disposition of F-M08-017-1.

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
| M08-019 | Reopened frontier exit review | 018, 021 | — | Resolve hidden-information, legality, determinism and downstream-regression findings before M09. |
| M08-020 | Ground-combat structure legality (F-M07-019-1 fix) | M07-020,M08-017; blocks 018 | LRR 49; M07-019 evidence, M07-020 adjudication | Structure-only planets fall without resistance (no spurious ground combat); structures die on control transfer; Assimilate-after-pause coverage load-bearing. Hard ordering before 018 so revalidation and all downstream baselines run against corrected behavior. |
| M08-021 | Behavioral distribution suite (F-M08-017-1 requirement) | 017, 020; blocks 019 | Programme comparability requirement (M08-017 frontier review S3); M08-015 original row text | Paired-seed comparison of the authored bot: action mix, VP pace, completion, faction differentiation within approved statistical bounds established by a recorded baseline run and review approval. Must complete before 019 closes so the exit gate's "paired-seed behavior remains within approved statistical bounds" clause is met on real evidence. Prep spec: `plans/M08-021_BEHAVIORAL_DISTRIBUTION_SUITE.md`. |
| M08-022 | Ground-force predicate vs corpus flag (M08-020 review T1) | 020; **hard-ordered before any D11 roster widening** (does not block 018/021 — Titans is off-roster, so the defect is structurally dormant today) | Corpus `isGroundForce` flag vs LRR ground-force definition; M08-020 review T1 (`plans/M08-020_OPEN_REVIEW_ITEMS.md`) | `UnitType::is_ground_force()` agrees with the corpus flag for every record, with an explicit recorded decision on the two unflagged Naaz space mechs (union semantics recommended); red-first test for Hel-Titan I; no classification change for any unit of a roster faction. Prep spec: `plans/M08-022_TITANS_PDS_GROUND_FORCE_PREDICATE.md`. Ledger entry KD-5. |

## Scope dispositions (operator decision, 2026-08-22)

F-M08-017-1 was escalated by the M08-017 frontier review with a complete recommendation; the
operator **adopted it as-is** (option c hybrid). This section is the M08 scope ledger for rows
001–016. Full reasoning: `plans/M08-017_OPEN_REVIEW_ITEMS.md` §S3.

| Row | Disposition | Rationale |
|---|---|---|
| 008 tactical plans | **Cancelled** | No consumer in MLP Phases 2–8 — inherited oracle-port scope; would degrade the `bc_capacity` diagnostic (multi-turn plans cannot be expressed by a per-option scorer); cost of three spec/implement/review cycles. Case against recorded fairly: a stronger baseline makes Phase 8's claim stronger. |
| 010 faction profiles | **Cancelled** | Same no-consumer rationale; `learned::Profile` is M09-track and must not be counted here (F-M08-017-3/S2). |
| 013 experimental capabilities | **Cancelled** | No referent without 008/010 — opt-in/configuration scaffolding for them. |
| 009 opening features | No action | Misattributed, not missing: content lives on the M09 track (`progress.rs`, M09-011/M09-012). |
| 012 serialization | **Deferred** (implementer's discretion per recommendation) | Trivial either way; no consumer in the tree needs serialized decisions. Any future package that needs them adds `Serialize` + test in its own commit. |
| 014 bot differential choices | **Waived with reason** | The 112 behavioral tests plus choice- and game-level determinism pins cover the practical regression risk; golden rankings would mostly re-pin what determinism already pins. |
| 015 behavioral distribution suite | **Required before M08-019 closes → scoped as M08-021** | The authored bot is the comparison baseline every cross-time VP measurement depends on, including the MLP Phase 8 ablation; without a paired-seed distribution pin, a silent bot change invalidates all of them. |
| 016 performance benchmark | **Waived with reason** (dead dependency already removed per S1) | M00-012's microbenchmark protocol and the MLP plan's D19 CPU/CUDA gates define the throughput measurements that actually matter; a separate bot-level regression budget would measure something nothing is gated on. |

The withdrawn justification (the "no heuristics, straight learning" constraint) does not appear in
this ledger: it was wrong — the authored bot is architecturally isolated from training — and its
withdrawal is recorded in the review file.

## Exit gate

Authored bots select only legal actions from authorized observations, explanations reconcile, and
paired-seed behavior remains within approved statistical bounds; M08-019 has no unresolved finding.
