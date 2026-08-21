# M09 — Fully learned policy

## Goal

Load supported learned-policy schemas 2–5 and implement schema-6 CPU MLP inference with factual,
redacted features and no authored-heuristic leakage.

## Work packages

Rows 001–018 retain the historical source labels under which they were executed.
For rows 019 onward, `docs/MLP_PLAN.md` revision 5 and accepted Rust schemas are
normative; Python parity is not an acceptance criterion.

| ID | Package | Depends | Normative source / context | Deliverable and acceptance test |
|---|---|---|---|---|
| M09-001 | Policy artifact envelope | M02,M08 | `learned_policy.py` profiles | Versioned faction/schema/dimensions/heads/temperature metadata and validation. |
| M09-002 | Schema 2 hash function | 001 | `_bucket`, hashed policy | Exact compatible buckets/signs; exhaustive golden vector corpus. |
| M09-003 | Schema 2 factual features | 002 | `HashedLinearPolicy` | Option/prompt/player/board/tactical facts; sparse vectors match fixtures. |
| M09-004 | Schema 2 inference | 002,003 | score/softmax | Scores, probabilities, temperature, and sampling match within tolerance. |
| M09-005 | Explicit sparse layout | 001 | `policy_linear.py` | Stable head/feature indexing, profile-to-vector and vector-to-profile validation. |
| M09-006 | Decision-head router | M08 | `decision_head` | Current precedence and fallback behavior for every observed choice kind. |
| M09-007 | Schema 3 explicit policy | 005,006 | `ExplicitHeadPolicy` | Sparse feature scoring and full legal-set softmax. |
| M09-008 | Schema 3→4 economy migration | 007 | migration/economy split tests | Copy semantics and routing match Python; old checkpoints resume. |
| M09-009 | Schema 4→5 other migration | 007 | other-head split tests | Scoring/agenda/exploration/ability/transit splits and precedence match. |
| M09-010 | Tactical structured features | 003,007 | learned tactical extractors | Origin/destination/route/cargo/unit/invasion/production factual features. |
| M09-011 | Opening progress | M08 | `opening_progress` | Gate facts, caps, shortfall, potential calculations match. |
| M09-012 | Horizon progress | M06 | `horizon_progress` | VP and scoreable-objective factual snapshots with cache invalidation semantics. |
| M09-013 | Trajectory records | 006,011,012 | `trajectory_record` | Legal matrices, probabilities, chosen option, progress, metadata. |
| M09-014 | Heuristic isolation | 003–013 | fully learned tests/review doc | Instrumented tests prove authored score/filter/playbook values cannot enter inference. |
| M09-015 | Existing artifact import | 001–009 | branch checkpoints/profiles | Sample schema 2–5 files validate, migrate, score, and preserve fingerprints as specified. |
| M09-016 | Full-round learned smoke | 010–015 | fully learned tests | Blank and trained policies complete 3p and 6p rounds deterministically. |
| M09-017 | Numerical differential suite | 002–016 | learned-policy tests | Heads, features, logits, probabilities, and selected options meet tolerances. |
| M09-018 | Frontier schema/math review | 001–017 | — | Review hashing, migrations, softmax stability, feature purity, and compatibility. |
| M09-019 | Post-rules baseline/profile and feature inventory | M08-019,M09-018 | M00 protocol; MLP plan §§2,7 | P2 r6 validation re-baseline plus bounded profile with raw samples and two independent frontier reviews; no optimization bundled. |
| M09-020 | Durable baselines and sealed data roles | M08-019,M09-018 | MLP plan §10 | P2 ≤50 MiB compressed fixture policy, checksum manifests, validation role for seed 777, sealed zero-overlap seed-20260822 final pool. |
| M09-021 | Objective policy features | M06-023,M08-019,M09-018 | MLP plan §5.1 | Requirement/progress/met/stage families use scoring sources of truth; legacy factual policy subvector unchanged. |
| M09-022 | Ability decomposition policy features | M08-019,M09-018 | MLP plan §5.3 | Typed ability/start/home/commodity/faction-tech facts separate all 33 selectable seats; unseen identity remains zero. |
| M09-023 | Secret redaction in feature paths | M08-019,M09-018 | Typed redacted views; MLP plan §5.2 | Acting seat sees own secrets; opponents expose public counts only across every feature set. |
| M09-024 | Dense vocabulary and OOV capacity | 019–023 | MLP plan §§4.5,6.1 | P2 replay of the fixed teacher seed schedule; deterministic double-build, reserved OOVs, append-only logical slots, fixed physical capacity and hard migration boundary. |
| M09-025 | CPU libtorch/tch tensor adapter | 019 | MLP plan §§4,7 | Pinned/license/advisory-reviewed P2 dependency; CPU deterministic tensor smoke and bounded adapter tests. |
| M09-026 | Batched MLP actor and readouts | 024,025 | MLP plan §§4.2–4.3 | Depth-2 actor supporting only widths 256/128, shared+zero-init faction residual readouts, schema-4 heads, stable softmax/legal end-to-end play. |
| M09-027 | Canonical critic state and value inference | 026 | MLP plan §§4.1–4.2 | Option/legal-set-free critic namespace; vector/value bit-invariant to legal-set order/content; hidden-info tier C review. |
| M09-028 | Schema-6 inference bundle | 024–027 | MLP plan §§4.4–4.6 | Safe tensor bundle, manifest-last atomic write, bounded validation/recovery, CPU round trip; training resume remains M10-035. |
| M09-029 | CPU MLP game/throughput gate | 028 | M00 protocol; MLP plan §7.1 | P2 deterministic MLP-choice legality smoke plus 5-warmup/20-sample alternating linear-vs-shadow-MLP rollout benchmark; identical decisions/outcomes in timed arms; accept width 256, test fixed width-128 fallback, or stop by fixed bands. |
| M09-030 | Reopened frontier exit review | 019–029 | — | Two independent tier D passes resolve schema, hidden information, numerics, dependency, artifact and performance claims. |

## Exit gate

Existing supported profiles execute in Rust; schema migrations and schema-6 recovery are trustworthy;
hidden information is typed/redacted; CPU MLP inference is deterministic, within its predeclared
throughput band, and demonstrably free of authored utilities; M09-030 has no unresolved finding.
