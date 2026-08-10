# Rust rewrite master plan

## Decision

Build a new repository at `D:\Projects\ti4-engine-rs`. Preserve public behavior and persisted
artifacts while redesigning internals idiomatically in Rust. Keep the Python repository unchanged
and use it as a differential oracle until cutover.

The scope is everything present on `codex/fully-learned-policy` at `37061c5`, including the rules
engine, current implemented content, documented gaps, bots, fully learned policy schemas, Stage 1
and Stage 2 training, simulation tools, artifact formats, and TTS bridge.

## Baseline

| Measure | Observed value |
|---|---:|
| Engine Python modules / lines | 74 / 39,344 |
| Total source and tooling lines | 75,865 |
| Python tests collected | 2,097 |
| Branch-specific files / additions | 70 / about 15,000 lines |
| Four-round game, one core | 1.053 s |
| Four-round throughput, 12 workers | 7.16 games/s |
| Decisions per game | 1,355 |
| Scoring cost | 0.777 ms/decision |
| Live-game clone | 13.49 ms |
| Tactical action | 2.36 ms median |

M00 must remeasure these figures before they become contractual.

## Compatibility policy

Preserve:

- legal-choice sets, stable option IDs, state transitions, and event ordering;
- documented deterministic behavior and replay semantics;
- content JSON and manifests;
- policy profiles and checkpoint schemas 2–5;
- JSON, JSON.GZ, Parquet, map-pool, baseline, telemetry, and surrogate artifacts;
- TTS HTTP endpoints and command/telemetry JSON;
- implemented behavior and explicit `unimplemented` registries.

Internal Rust layouts may differ. Existing artifacts may use a checked translation command where
direct loading would permanently constrain the Rust design. Every compatibility surface is marked
`exact`, `semantic`, `translated`, `intentional-change`, or `not-applicable` in M00's ledger.

Native Rust games use a pinned, versioned RNG. Legacy Python seeds are converted into an explicit
entropy/replay stream; they do not force all future games to use Python's RNG behavior.

## Workspace architecture

| Crate | Responsibility |
|---|---|
| `ti4-model` | Typed IDs, state, units, views, schema contracts |
| `ti4-content` | Corpus loading, validation, provenance, content hashes |
| `ti4-engine` | Choices, timing, legal actions, rules, effects, game loop |
| `ti4-policy` | Scored bots, feature extraction, learned policy inference |
| `ti4-sim` | Maps, replay, batches, rotations, benchmarks |
| `ti4-training` | Stage 1/2 learning, promotion, archives, Parquet capture |
| `ti4-bridge` | HTTP, TTS commands, import, reconcile, audit |
| `ti4-legacy` | Python artifact and replay conversion |
| `ti4-cli` | Supported executable entry points |
| `xtask` | Fixture, validation, and repository maintenance commands |

Rules state is owned by the engine and not publicly mutable. IDs are newtypes. Iteration that can
affect behavior uses deterministic ordering. Content is immutable and shared. The engine emits a
canonical event stream. Rollout snapshots use measured copy-on-write or structural sharing rather
than copying a Python object graph literally.

## Performance gates

Measured on the same Windows host and workload:

| Metric | Required | Target | Stretch |
|---|---:|---:|---:|
| Single-core four-round game | 3x | 5x | 10x |
| Fixed-worker throughput | 3x | 5x | 10x |
| Policy scoring | 3x | 6x | 12x |
| Snapshot/fork | 3x | 5x | 10x |
| Stage 2 training throughput | 2x | 4x | 8x |
| Peak memory per worker | no regression | 40% lower | 60% lower |

No performance result counts unless semantic parity tests for that workload pass.

## Quality gates

- Windows is the supported runtime platform.
- Workspace test coverage: at least 85%; legality/timing/payment/replay modules: at least 95%.
- Mutation score: at least 90% in critical modules and 80% elsewhere.
- No unresolved critical or high dependency vulnerability.
- No unreviewed `unsafe`.
- Deterministic hash equality across repeated native runs.
- 10,000 differential tactical scenarios and 1,000 complete-game soak runs.
- Every external parser has malformed, oversized, and fuzzed inputs.
- Checkpoint writes are atomic and crash-recovery tested.

## Milestone gates

1. M00 freezes scope and the oracle.
2. M01 creates a reproducible workspace.
3. M02 establishes content and state.
4. M03 establishes choices, timing, and replay.
5. M04 completes generic games.
6. M05 completes the tactical action.
7. M06 ports general rules.
8. M07 closes current faction/TE parity.
9. M08 ports authored bots.
10. M09 ports learned inference.
11. M10 ports simulation and training.
12. M11 replaces the Python bridge.
13. M12 proves parity, safety, and speed.
14. M13 qualifies and cuts over the workload.

Each milestone requires a frontier-model exit review. Completion means passing evidence, not merely
the presence of code.

## Completion definition

The Rust engine is ready only when the scope ledger is closed, legacy artifacts import, applicable
behavior is equivalent, known gaps are accurate, security gates pass, minimum speedups are measured,
the representative workload soaks successfully, and rollback to the frozen Python engine has been
tested.

