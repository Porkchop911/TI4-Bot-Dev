# M09 — Fully learned policy

## Goal

Load and execute existing learned-policy schemas 2–5 with factual features and no authored-heuristic leakage.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
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

## Exit gate

Existing profiles execute in Rust, schema migrations are trustworthy, numerical differences are
bounded, and fully learned inference is demonstrably free of authored utilities.

