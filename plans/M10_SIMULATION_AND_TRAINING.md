# M10 — Simulation and training

## Goal

Maintain the accepted simulation/map/training workload and add deterministic MLP corpus capture,
distillation, PPO, resume, promotion, optional optimizer acceleration, and sealed evaluation.

## Work packages

Rows 001–030 retain the historical source labels under which they were executed.
For rows 031 onward, `docs/MLP_PLAN.md` revision 5, accepted Rust training math,
and pre-registered data/gate manifests are normative; Python parity is not an
acceptance criterion.

| ID | Package | Depends | Normative source / context | Deliverable and acceptance test |
|---|---|---|---|---|
| M10-001 | GameResult/Batch schemas | M07–M09 | `engine/sim.py` | Success/failure/ending/timing/actions/VP structures; failures are counted, not hidden. |
| M10-002 | Save 52 board | M04 | `sim.save_52_galaxy` | Exact placement, seats, rotations, content/source constraints. |
| M10-003 | Save 54 board | M04 | `sim.save_54_galaxy` | Three-player layout and faction/home configuration. |
| M10-004 | Map variation | 002,003 | variation/balance functions | Deterministic tile-seed variation and measured balance score. |
| M10-005 | Map-pool reader | 004 | `MapPool`, JSON.GZ pools | Stream/load existing pools, validate slots/arrangements/hash, deterministic draw. |
| M10-006 | Map-pool builder | 004 | `tools/build_map_pool.py` | Parallel deterministic generation; seating-independent outer maps. |
| M10-007 | Single-game runner | 001–005 | `sim.play` | Seed/rotation/horizon/policy inputs and structured failure capture. |
| M10-008 | Parallel batch runner | 007 | `sim.run` | Bounded workers, deterministic result ordering, cancellation/error handling. |
| M10-009 | Rotation/report suites | 008 | rotation/report tools | Counterbalanced panels, action mix, differentiation, stable machine-readable report. |
| M10-010 | Training config/schema | M09 | training schema/config tools | Stage, reward, panels, learning parameters, versions, compatibility validation. |
| M10-011 | Return calculation | 010 | Stage 1/2 trainer | Stage 1 capped potential; Stage 2 VP/objective/R1 shaping; golden trajectory returns. |
| M10-012 | Advantage centering | 011 | trainer reducer | Per-faction/head centers, variance, deterministic reduction. |
| M10-013 | Policy gradients | 011,012 | REINFORCE implementation | Sparse gradients, entropy, clipping, update; numerical oracle fixtures. |
| M10-014 | Worker sufficient statistics | 013 | worker gradient reduction | Reduced and raw paths agree within declared floating tolerance. |
| M10-015 | Persistent worker pool | 008,014 | trainer worker lifecycle | Profiles serialized once per batch, all workers eligible, clean shutdown/recovery. |
| M10-016 | Validation panels | 010,015 | trainer validation | Fixed varied maps, isolated faction swaps, assembled table evaluation. |
| M10-017 | Promotion/confirmation | 016 | promotion tests | Stage-specific acceptance, regression vetoes, lazy confirmation, champion isolation. |
| M10-018 | Learner/champion resume | 010,017 | resume logic | Failed promotion does not roll back learner or change champion; uninterrupted equivalence test. |
| M10-019 | Telemetry | 012–018 | training telemetry | Counts, moments, entropy, norms, KL estimator labels, bounded live tail. |
| M10-020 | Atomic checkpoints | 018,019 | JSON checkpoint writer | Crash-safe replace, checksums, schema/version, recovery from interrupted temp file. |
| M10-021 | Surrogate snapshots | 016–020 | JSON.GZ snapshots | Immutable candidate fingerprint, panels, maps, outcomes, promotion, telemetry. |
| M10-022 | Decision corpus schema | M09 | `DECISION_CORPUS.md` | Typed one-row-per-option representation and validation. |
| M10-023 | Parquet writer | 022 | `decision_capture.py` | Worker shards, zstd/dictionaries, aligned feature lists, forced-decision filtering. |
| M10-024 | Capture sampling | 023 | capture fraction/every | Whole-episode deterministic sampling and update cadence. |
| M10-025 | Training archive layout | 019–024 | training archive/index/lineage | Manifests, generations, `_SUCCESS`, cases/mutations/profiles, lineage validation. |
| M10-026 | Existing artifact import | 005,020–025 | existing out/data artifacts | Representative Python pools/checkpoints/snapshots/Parquet/archive read successfully. |
| M10-027 | Stage 1 smoke | 010–026 | Stage 1 tests | Small deterministic update/evaluate/resume run with expected metrics. |
| M10-028 | Stage 2 smoke | 010–026 | Stage 2 tests | Four-round update/evaluate/promote/resume run and objective progress cache checks. |
| M10-029 | Training performance suite | 007–028 | benchmark/docs | Games/s, updates/s, memory, serialization, capture overhead versus M00 baseline. |
| M10-030 | Frontier math/artifact review | 010–029 | — | Independent training math, numerical, crash safety, schema, and performance review. |
| M10-031 | Fixed teacher corpus | M09-030,M10-030 | MLP plan §6.1 | P2 deterministic 128-seed × six-rotation capture of actor/critic vectors, legal sets, teacher probabilities and accepted four-round returns; seed-cluster split, checksummed ≤10 GiB shards, forced-choice filtering and no final data. |
| M10-032 | Multi-teacher factual distillation | 031 | MLP plan §6.1 | Fixed temperature 1.0, predeclared imitation/gameplay gates, at most two fixed DAgger rounds, zero-init feature extensions. |
| M10-033 | Critic warm-up and fallback | 032 | MLP plan §6.2 | Train only disjoint critic rows/value head with actor logits bit-identical; one bounded separate-critic retry; batch-mean fallback chosen before PPO and shared by ablations. |
| M10-034 | MLP PPO and Adam | 033 | MLP plan §6.3 | Advantages detached/frozen from behavior values across four epochs; numerical gradients, entropy, actor/critic loss, deterministic reduction tests. |
| M10-035 | Schema-6 training resume extension | 034 | MLP plan §§4.4,4.6 | Optimizer tensors/config, data/vocabulary/RNG cursors, manifest-last recovery and uninterrupted-vs-resumed equivalence. |
| M10-036 | Shared-model promotion gate | 035 | MLP plan §6.4 | Fresh seed-cluster validation/confirmation panels, aggregate merit, twelve multiplicity-corrected guards, synthetic decisions and one bounded real boundary. |
| M10-036a | Fixed optimizer selection | 036 | MLP plan §6.3 | Six 200-update validation-only pilots, guarded/confirmed selection, then a fixed 1,000-update factual smoke with ≥2 promotions; freeze for every ablation. |
| M10-037 | Optional CUDA optimizer gate | 036a | MLP plan §§7.1–7.2 | P2/D: CPU rollouts only; fixed-batch gradient agreement/repeatability and ≥10% end-to-end median gain with positive paired 95% CI, else no CUDA merge. |
| M10-038 | Three-run ablation and sealed final | 036a–037 (CUDA pass or recorded no-op) | MLP plan §§1,6.5,7 | P2/D: three fixed 10,000-update runs within 72h/100 GiB bounds; freeze artifacts/analysis; one resumable fixed-seed final campaign; clustered CI and fixed success bar. |
| M10-039 | Reopened frontier exit review | 031–038 | — | Two independent tier D passes reconcile math, artifacts, data leakage, determinism, performance, ablations and final claims. |

## Exit gate

Rust can resume supported linear and schema-6 training, preserve learner/champion semantics, emit
valid checkpoints/snapshots/archives/decision shards, and train/evaluate the three MLP ablations
without validation/final leakage. Optional CUDA has passed or been omitted by its fixed gate, the
one-shot final result is reported without threshold changes, and M10-039 has no unresolved finding.
