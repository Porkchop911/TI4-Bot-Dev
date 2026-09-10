# Engine optimization investigation — 2026-09-08

Status: investigation complete; one optimization implemented and measured. **Integration qualification is blocked by unrelated shared-tree failures** (details below). Nothing staged or committed.

The main finding is an attribution error: the engine is about **17% of this rollout workload, not 86%**. The small supply-count change reduces measured engine time by **14–15%**, worth an estimated **1.4–1.6% of total training wall time**, not the ~7–8% implied by the original engine share. This is an extrapolation; no PPO training A/B was run.

## Scope and permissions

Operator-requested performance investigation, independent of migration milestones. P1: new profiling example, clean supply helper if justified, its tests, this report, and bounded ignored logs/executables under `out/`. No network, GPU workload, staging, commits, branch changes, or edits to other sessions' modified files. The task's configured C: checkout is empty; the actual repository is the D: path explicitly supplied by the operator. Writes/builds use approved sandbox escalation for that path.

The initial read found `game.rs`, `ppo_update.rs`, training rollout, and many other engine files modified. They remain untouched. `supply.rs` was clean. A later status also showed another session modifying `combat.rs`; frozen executable comparisons and source manifests are required for performance attribution.

## Correction to the premise

`game_cost.rs` accumulates `spent`, `calls`, and head totals only for `index == candidate_seat`. Other five seats are unwrapped unless tracing, and tracing gives each opponent independent, discarded counters. Subtracting candidate time from game wall therefore includes the other five policies, setup, and audit work. The reported 86.1% is not an engine measurement; 52% of training time does not follow.

A new all-seat wrapper using the original audit runner completed the exact requested workload: checkpoint-473312, four rounds, seeds 900000100..900000105, six rotations, six candidate-seat replays = 216 games. Aggregate: **322.1 s wall, 263.5 s inside all deciders, 58.6 s residual, 369,774 decisions**. Thus **81.8% policy / 18.2% residual**. Residual includes diagnostic hashing/setup/audit overhead and is an upper bound on engine work, not an exclusive engine figure. This instrumentation correction is not a speedup comparison with the historical 296.3 s result.

Raw log: `out/engine-cost-full-20260908.log`. Direct step measurements and optimization validation follow below.

## Measurement design

`engine_cost` preserves checkpoint loading, CPU actors, per-seat streams, temperature 0.001, map pool, factions, rotations, and candidate selection from `game_cost`. It wraps all six deciders. Default runner uses the existing public setup factory, times each `Game::step` by phase at entry, and subtracts complete wrapper time (policy plus diagnostics). It rejects any step error and horizon truncation. `--legacy-audit` retains the old runner for differential checks.

`GAPS`/`HEADGAPS` are intervals associated with the NEXT decision's phase/head. They help localize investigation, but are not exclusive subsystem timings: preceding answer resolution and phase transitions can belong to the interval. Direct `PHASES` contains count, inclusive step wall, and exclusive engine duration. Setup is separate. Timing and Debug-based choice digests never feed game state or RNG. Diagnostic choice hashes are not the engine's versioned fingerprint format and do not hash every observation feature.

## Candidate under test

`supply::held` rebuilds `units::catalogue` inside `base_type_of` for every owned unit visited, including captured units. Production's `will_place -> allowed -> remaining -> held` repeats that for capped build options. Hoist the exact catalogue construction to once per `held` call and use the same exact-key lookup/fallback. This preserves BTreeMap ordering, all traversals, integer counts, source filtering, unknown-id behavior, and RNG/decision interfaces. It avoids global cache invalidation or changing alias resolution.

## Deliberately untouched

- PPO optimizer, network architecture, CUDA batching, rollout chunk scheduling: outside requested scope.
- Checkpoint writes: existing measured cost negligible.
- `Batch::freeze` timing: valuable but `ppo_update.rs` is already dirty, so the requested ownership rule prevents editing it here.
- Production's main `standing_after` already uses analytic `fleet::standing_using`; a claim that every main build option clones the entire state would be false. Separate `produce_one`/integrated-economy paths still clone, but their cost is not yet measured.


## Direct engine breakdown

Same bundle, horizon, seed range, map pool and faction rotations; `--only-seat 0` avoids the five redundant replays when candidate and opponent are the same checkpoint. This is **36 distinct seed/rotation games**, not 216 independent games. On the initial 216-replay run, all six replays of each seed/rotation had identical choice/event/state hashes. That run counted 369,774 calls, exactly six times the original report's candidate-only 61,629.

The shared tree changed `combat.rs` between the initial exploratory build and the frozen baseline build. Consequently, the later baseline has 61,784 calls and differs in 15/36 trajectories from the earlier executable. **Do not compare those executables as an optimization A/B.** Instead, direct-vs-legacy validation uses the same frozen baseline executable, and supply before/after uses separately frozen executables differing only in supply source at build time.

All following phase numbers use the frozen baseline and its direct runner:

| Phase at step entry | Steps | Inclusive step time (s) | Engine excluding complete wrappers (s) | Share of engine |
|---|---:|---:|---:|---:|
| Action | 40,905 | 45.3655 | 8.18772 | 94.32% |
| Status | 2,740 | 2.2546 | 0.23325 | 2.69% |
| Agenda | 2,216 | 2.0129 | 0.20781 | 2.39% |
| Strategy | 1,007 | 0.4655 | 0.05200 | 0.60% |
| **Total** | **46,868** | **50.0985** | **8.68078** | **100%** |

Policy time is 41.0756 s; wrapper overhead is approximately 0.3421 s. Setup is separately 0.0640 s. Outer game wall is 50.2046 s. Phase entry classification includes automatic transitions executed within that step; it does not claim that every instruction in an Action step is an action rule. Unlike a decider-gap profile, it also includes choice-free steps.

Removing diagnostic overhead gives engine fraction `8.68078 / (8.68078 + 41.07563) = 17.45%`. If that ratio transfers to the 60.1%-rollout training run, engine share of training is **10.49%**. Transfer is an assumption: sequential low-temperature evaluation does not reproduce 32-thread PPO scheduling, training temperatures, or trajectory capture. The original 52% estimate is unsupported regardless.

The baseline's largest intervals associated with the next decision are turn 2.8077 s, production 1.6702 s, trade 0.9247 s, ability 0.5771 s, movement 0.4648 s, and secondary 0.4362 s. These are useful localization signals, not exclusive function costs. In particular, an interval can include resolution of the previous choice.

## Implemented change and controlled results

Only production code changed: `crates/ti4-engine/src/supply.rs`, `held`. It constructs the exact same source-filtered `BTreeMap` once per call and uses borrowed base-type strings, instead of rebuilding the map and allocating a result string for each owned unit. Traversal, integer addition, capture ownership and unknown-id fallback are unchanged. No cross-call cache, alias lookup substitution, reordering, policy feature, option, observation, RNG or concurrency change was introduced.

### All 36 unique games

| Metric | Baseline | Optimized | Reduction |
|---|---:|---:|---:|
| Exclusive engine time | 8.68078 s | 7.47647 s | **13.87%** |
| Action engine time | 8.18772 s | 6.98179 s | 14.73% |
| Policy time | 41.07563 s | 41.36124 s | increased 0.70%, illustrating noise |
| Outer wall | 50.20461 s | 49.28962 s | 1.82% |
| Next-production intervals | 1.67020 s | 0.67043 s | 59.86% |
| Next-payment intervals | 0.39323 s | 0.24946 s | 36.56% |

The narrow source intervention and concentration in production/payment intervals support catalogue rebuilding as a real cost. The 1.2043 s engine reduction is **2.42% of uninstrumented rollout time**, hence **1.45% of training time** using the historical rollout fraction. This estimates only the isolated engine benefit, without attributing policy timing noise to the patch.

### Five interleaved repetitions

Protocol fixed before these repetitions: one warmup game per executable, then five pairs of the same six seeds, rotation 0, candidate seat 0, four rounds. Order A/B, B/A, A/B, B/A, A/B; one process at a time, default affinity, CPU inference, 32 logical processors, Rust 1.94.1, release thin LTO/codegen-units=1. No PPO process was present on checks before runs. Two idle cargo processes were visible at one sampling point, with no rustc or training process; no exclusive-machine reservation was made. These are bounded single-thread game measurements, not a parallel-training benchmark.

| Pair | Baseline engine (s) | Optimized engine (s) | Engine reduction | Baseline wall (s) | Optimized wall (s) |
|---|---:|---:|---:|---:|---:|
| 1 | 1.37379 | 1.15505 | 15.92% | 7.92064 | 7.55138 |
| 2 | 1.35958 | 1.15096 | 15.34% | 7.80662 | 7.55063 |
| 3 | 1.34833 | 1.15330 | 14.46% | 7.77783 | 7.50959 |
| 4 | 1.35568 | 1.19283 | 12.01% | 7.80822 | 7.75277 |
| 5 | 1.35185 | 1.14707 | 15.15% | 7.74888 | 7.52398 |

Paired median engine reduction **15.15%**; paired median outer-wall reduction **3.28%**, range 0.71–4.66%. Engine mean ± sample SD: baseline **1.35785 ± 0.00985 s**, optimized **1.15984 ± 0.01868 s**. Outer wall mean ± sample SD: baseline **7.81244 ± 0.06519 s**, optimized **7.57767 ± 0.09950 s**. Thus the engine improvement is consistent across repetitions, while the much larger policy share makes total wall more variable. Do not present 3.28% × rollout fraction as a precise training gain; the engine-only estimate around 1.4–1.6% is better supported.

## Ranked remaining opportunities

Ranking weights measured scope and implementation risk. Only rank 1 has an implemented A/B; other numeric ranges are explicitly engineering scenarios, not promised savings. Do not add them together.

| Rank | Slow work and reason | Proposed change | Estimated total-training saving | Determinism / surface risk |
|---|---|---|---|---|
| 1 — implemented | Supply counting repeatedly constructs a full catalogue per owned unit, multiplied by capped build candidates | Hoist exact catalogue once per count; borrow base-type text | **1.4–1.6%**, extrapolated from measured engine savings | Low; exact corpus/source/location differential test and game hash evidence below |
| 2 | Turn-associated intervals: 2.8077 s. `Game::action_options` calls many independent availability generators; some derive repeated immutable content and player facts | Profile those generators next; share catalogue/derived facts **within a single choice construction**, retaining all ids/order/previews | **0.3–1.0% scenario** if 10–30% of the associated interval is removed; attribution alone does not establish feasibility | Low–medium for local immutable reuse; high for cross-turn caching because ownership, laws and temporary flags change. `game.rs` is dirty: do not edit here |
| 3 — secondary scope | `Batch::freeze` canonicalizes/validates sparse options; source shows per-option temporary pair allocation and sorting. Historical 4.4% wall attribution has not been independently measured here | First time freeze and print complete update wall; then investigate reusable scratch capacity while preserving the exact column/value-bit total order and fold order | Timer itself **0%**; **0.4–1.1% scenario** for a later 10–25% freeze reduction | Timing low; canonicalization medium because f32 duplicate-fold order is load-bearing. `ppo_update.rs` and `ppo.rs` are dirty, so neither edited |
| 4 | `step_trade` clones the galaxy on each negotiation decision; next-trade intervals total 0.9247 s | Borrow the galaxy across disjoint state/table fields if the borrow structure permits; measure before attempting larger changes | **0.05–0.2% scenario**, assuming 5–20% of those intervals is avoidable; clone share itself unmeasured | Low if exact borrow substitution; no fewer trade options/negotiations. `game.rs` dirty |

Other catalogue construction in production's `placements`, `producers`, and `buildable_for` is visible, but the successful supply patch already reduces next-production intervals to 0.6704 s. Further reuse is a smaller follow-up, not a second independent claim on the original 1.6702 s.

Not pursued: reducing offered options, removing negotiations, changing observation features, or changing BTreeMap iteration order. These violate the checkpoint surface constraint. Activation's next-decision gap is only 0.0543 s versus 7.0425 s inside activation deciders; the measurement does not support targeting activation option generation as the dominant engine cost. The corrected policy share warrants revisiting inference attribution in a separate task, but no network/optimizer work was done here.

## Preservation evidence and validation limits

- The new supply regression test passed **before** the optimization against the original implementation. It compares expected per-unit resolution across every corpus id, unknown ids, BASE/POK/DEFAULT source scopes, two owners plus an absent owner, space, planets, and captures.
- After optimization, `cargo test -p ti4-engine -j 4`: **1,239 unit tests passed**; integration inventory **3 passed, 1 failed**. The missing inventory entries are `thunders_edge::choose_system` and `choose_placer` (Choice), `fracture::after_breakthrough_gained` (Choice/AskObserved), and `entropic_scars::resolve_status_start` (Choice/AskObserved). These files and the registry were already modified by others; none is in this change. No fixture was regenerated or weakened. **The required full-suite gate is not green.**
- Same frozen baseline executable, direct runner vs `--legacy-audit`: **36/36 ordered-choice hashes, 36/36 event hashes, 36/36 final-state hashes identical**. This verifies the replacement runner against the existing public audit path.
- Baseline vs optimized, all unique games: the same **108 hashes identical**, 61,784 decisions on each side. All five repeated pairs also match all hashes for their six games. This is trajectory evidence, not merely equal scores/winners.
- Diagnostic choices hash the full Debug choice and result sequence, including option ordering. They are not `fingerprint.rs`'s versioned serialized fingerprints and omit observation feature vectors. The change cannot affect observations when its returned integer count is identical; the corpus differential test checks that helper contract. No policy/observation code was edited.
- The profiler's six-seat seeded-answer/error test passed against the exact release dependencies used by the frozen benchmark. A normal Cargo test was attempted but blocked by later unrelated edits; a standalone `rustc --test` invocation selected dependencies through the successful example's Cargo fingerprints and used thin LTO. Exact arguments are retained in `out/engine-cost-wrapper-rustc-args-20260908.json`; add `-C lto=thin -C opt-level=3 -C codegen-units=1`, as used by the successful run. Result: **1 passed**.
- Later shared-tree edits introduced `game.rs:477` using nonexistent `ctx.galaxy` on `Resolving`; this blocked fresh Cargo validation. Do not interpret that error as a supply regression. The other session subsequently resolved the compile error: strict `cargo clippy -p ti4-engine --lib -- -D warnings` passed. Final validation: the rerun passed **1,241 engine unit tests**, with the same inventory integration failure (3 passed/1 failed); **all 5 doc tests passed**. The normal `cargo test -p ti4-mlp --example engine_cost -j 4` now also passed (1 test), after the shared compile error was fixed.
- Strict profiler lint checking against its exact frozen dependencies passed with all/pedantic warnings denied. The normal MLP Clippy command stops earlier on **11 existing library findings** in other files (including `ppo.rs` and `lib.rs`); none was fixed here. Scoped rustfmt and `git diff --check` are clean. Independent read-only review examined profiler attribution/setup and led to removing print/hash overhead from step buckets and surfacing errors. The supply patch has no independent review in this task; per the user's follow-up, no additional subagents were started. No merge/integration approval is claimed.

## Reproduction and artifacts

Build environment matches the user request: set LIBTORCH to the repository's `out/libtorch-2.9.1-cu128`, LIBTORCH_BYPASS_VERSION_CHECK=1, prepend its lib directory to PATH, and set GIT_COMMIT from `git rev-parse HEAD`. Actors loaded by this example perform CPU inference. The benchmark executables are frozen under `out/` so subsequent shared edits cannot change an already-running comparison.

```powershell
cargo build --release -p ti4-mlp --example engine_cost -j 4
# Full original workload (216 replays):
./target/release/examples/engine_cost.exe --bundle out/checkpoints/stage2-mlp-shaped/checkpoint-473312 --rounds 4 --seeds 6
# Every unique game (36), used for direct before/after:
./target/release/examples/engine_cost.exe --bundle out/checkpoints/stage2-mlp-shaped/checkpoint-473312 --rounds 4 --seeds 6 --only-seat 0
# Same-runner validation: append --legacy-audit.
# Six-game repeated subset: append --only-rotation 0.
```

Baseline was frozen at 16:58:16 local; optimized at 17:02:31. The source manifest at baseline and pre-optimized-build comparison showed **only supply.rs changed**. `space_stations.rs` was first observed changed at 17:02:38, after the optimized executable existed, and was subsequently edited again. It was not part of either frozen optimization variant. Later `game.rs` edits are also outside them. Exploratory-vs-baseline combat changes are explicitly excluded from speedup comparisons above.

Raw files: `out/engine-cost-full-20260908.log`, `engine-cost-step-{baseline,optimized}-20260908.log`, `engine-cost-legacy-baseline-20260908.log`, `engine-cost-pair-{1..5}-{baseline,optimized}-20260908.log`, corresponding warmup logs, `engine-cost-summary-20260908.json`, `engine-cost-pairs-summary-20260908.json`, and `engine-cost-source-baseline-20260908.json`. Validation logs use the same `engine-cost-` prefix. Executables/logs remain ignored and uncommitted; no cleanup of shared outputs was performed. Checksums are in `out/engine-cost-artifact-manifest-20260908.json`.

The next safe action is to let the other session finish its engine/registry edits, rerun the full engine suite and scoped Clippy, and independently review the small supply change before integrating it. The report and frozen measurements remain useful even while that integration gate is blocked. Profiler-only final cleanup added checked timer subtraction and documentation/lint annotations after benchmarking; no engine implementation or choice forwarding changed in that cleanup, and its normal Cargo test passed afterward.
