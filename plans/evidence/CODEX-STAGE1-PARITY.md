# CODEX-STAGE1-PARITY — meaningful Stage-1 comparison repair

| Field | Value |
|---|---|
| Branch | `codex/stage1-parity-fixes` |
| Oracle | `D:\Projects\ti4-engine`, commit `37061c5`, read-only |
| Permission | P1; Rust source/tests/docs only |
| External writes | none |
| Network | none |
| Generated artifacts | Cargo build output only |

## Objective

Make the Rust Stage-1 diagnostic reproduce the successful Python representation, faction panel,
training math, and rotation unit; prevent timing diagnostics from claiming semantic parity.

## Changed behavior

- schemas 3, 4, and 5 can load collision-free named profiles;
- blank schema-4 profiles are sparse and valid;
- new Stage-1 training defaults to schema-4 explicit features and learning rate 0.03;
- structured board features are emitted for explicit inference;
- a faction-keyed rotated rollout and gradient accumulator prevent cross-faction credit;
- `FactionPlan::python_reference()` fixes the reference configuration;
- `stage1_parity` loads Python checkpoints and reports/gates per-faction results;
- the old throughput tool no longer treats successful execution as semantic parity.

## Compatibility

The schema-2 extractor, hash function, bucket count, and blank hashed profile are unchanged. The new
path is additive. Existing schema-2 tests remain the compatibility guard.

## Verification

Final checks:

- focused package suites: passed (751 engine, 102 policy, and 93 training tests);
- `cargo fmt --all --check`: passed;
- `cargo clippy --workspace --lib --bins --tests -- -D warnings`: passed;
- `cargo clippy -p ti4-training --example stage1_parity -- -D warnings`: passed;
- `python -m py_compile tools/benchmark_training.py tools/bench_generation.py`: passed;
- `git diff --check`: passed;
- strict solved-profile invocation: passes for all three factions on the shared 96-game/faction
  pool panel after the execution/feature fixes described below.

The full all-target Clippy command is blocked by the pre-existing uncommitted
`stage1_curve.rs` change (`manual_is_multiple_of`); that user-owned file was deliberately not
altered as part of this package. The focused test set covers:

- named weights read without hashing;
- sparse blank explicit profiles;
- real-board structured features;
- full faction/seat rotation;
- exact reference configuration;
- named-weight growth from blank training;
- direct loading and evaluation of the Python schema-4 checkpoint.

## Known differences

See `docs/STAGE1_PARITY_COMPARISON.md`. Solved-profile transfer is a required gate precisely because
map, legal-choice, prompt/payload, and content-window parity are not yet complete.

## Measured diagnostics

- Solved Python table, 96 Rust seat-games/faction: Hacan 0.000 clearance / 1.26 planets;
  Jol-Nar 0.000 / 0.11; Letnev 0.010 / 0.80. Transfer gate fails.
- Blank Rust table after 50 repaired updates: Hacan 0.20 planets, Jol-Nar 0.00, Letnev 0.16.
  Hacan and Letnev move immediately; Jol-Nar still diverts into units (1.81).
- Before factual payloads were added to movement/cargo/landing/production choices, the same solved
  Hacan profile gained 0.02 planets. After the payload repair it gained 1.26.
- Follow-up cargo-window repair: Jol-Nar rose from 0.04 to 1.62 planets on a
  24-seat-game/faction panel. The trace proved that an underfilled capacity-four carrier loaded all
  three available units but never sailed. Cargo feature overlap then rose from about 69% to 98%.
- The strategy-card blocker is closed in the working tree: all eight primaries and secondaries are
  invoked by the driver, with Leadership/Hacan/Jol-Nar cost substitutions and Thunder's Edge
  replacements. Focused strategy tests pass. The solved panel still needs a fresh measurement.
- The map blocker is closed by loading the exact Python map-pool artifact.
- Post-card solved panel, 96 seat-games/faction: Hacan 0.094 clearance / 1.34 planets / 1.36
  units; Jol-Nar 0.010 / 1.53 / 2.23; Letnev 0.125 / 1.28 / 1.55. The 0.80 transfer gate still
  fails, but the prior near-zero-units symptom is gone.
- Final same-pool solved panel after learned-window closure: Hacan `0.865`, Jol-Nar `0.865`,
  Letnev `0.823`; the per-faction
  `0.80` transfer gate passes. Corresponding planet/system/unit means are Hacan
  `3.74/2.97/2.96`, Jol-Nar `2.95/2.97/2.39`, Letnev `2.84/2.97/2.40`.
- Causal trace fixes: runtime FULL-source propagation; authoritative printed technology choices;
  Sling Relay; Thunder's Edge expeditions; Gravleash; Gravity Drive's `move_gd` contract; Transit
  Diodes as a free start-of-turn choice; faction-specific production catalogue; and Python's
  distance-zero `target:reachable` observation.
- Learned-window closure: stateful nested action-card, agenda, reaction, exploration, payment,
  casualty, fleet, and timing choices now receive `Observed`; Integrated Economy, Orbital Drop's
  destination/DEPLOY, Peace Accords, Psychoarchaeology, Chaos Mapping, Predictive Intelligence,
  and Bio-Stims are wired as learned decisions.
- Residual, explicitly not claimed as complete parity: unsupported content/timing windows,
  cross-engine event ordering, and RNG consumption still differ.
