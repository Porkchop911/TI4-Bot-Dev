# Stage-2 trainer scheduling: rollout waves, pipeline overlap, and speed attribution

| Field | Value |
|---|---|
| Branch | `codex/stage1-parity-fixes` |
| Date | 2026-08-16 |
| Oracle | `D:\Projects\ti4-engine`, read-only (commit `37061c5`) |
| External writes | none (all artifacts under this repo's gitignored `out/`) |

## Objective

Operator requirements for the Stage-2 push: (1) clear Stage 2 with learned heads, (2) be at
least 5x faster than a comparable Python process on the same workload, (3) no prolonged CPU idle
during runs. This package addresses requirement (3) and measures requirement (2); it adds two
opt-in scheduling mechanisms to `stage2_training` and records the full speed attribution for this
machine.

## Findings from measurement (all on this machine: Ryzen 9 9950X, 32 logical cores, 94 GB RAM)

### Cost model per 100 updates (train-seeds=16, save52 pool, horizon 4, real gate)

| Component | Wall time | Notes |
|---|---|---|
| Learning (96 games/update x 100) | ~320-360 s | Rayon `par_iter` per update; apply() cost measured ~0 |
| Boundary at u+100 | ~140-190 s | validation panel 192 games + (rejection: up to 6 x 192 isolated fallback) or (+192 confirmation on promotion) |
| Checkpoint load + save + pool load | ~38 s | measured by I/O-only probe (`out/probe_io*`) |

### Hot-path attribution (temporary instrumentation, removed after measurement)

Instrumented `LearnedBot::consider()` with atomic nanosecond counters over a 25-update run
(5.8M decisions): feature construction = 644 core-s, scoring = 117 core-s. Against the measured
per-game cost of ~0.62 core-s/game (Rust) the policy side is **~32% of game time**; the engine
(legality generation, state mutation, reactions) is the remaining ~68%. Consequence: policy-side
micro-optimization alone cannot reach a 5x wall-time gain; the levers are scheduling utilization
and total work.

### Utilization diagnosis (the "spotty CPU" complaint)

Pure-learning probe (`--updates 100 --every 1000`, one block, boundary only at the end), 3 s CPU
sampler:

| Run | Learning wall | Avg cores in learning phase | Utilization |
|---|---|---|---|
| depth=1 (reference) | 357.3 s | 16.7 / 32 | **52%** |
| `--rollout-depth 4` | 293.0 s | 20.1 / 32 | **63%** |
| `--rollout-depth 8` | 305.5 s | 19.3 / 32 | 60% |

Root cause: game lengths vary (agenda endings, reaction chains). Each update's fixed 96-game
batch ends with straggler games while half the pool idles; `apply()` between updates is ~free, so
the idle is pure scheduling loss. Depth=4 is the sweet spot; depth=8 regresses slightly
(staleness variance + wave overhead).

### Python comparables (same champion start u3050, same seed base 74M/stride 10k, save52 pool)

| Process | Workload | Wall | Per-game single-threaded |
|---|---|---|---|
| Python `workers=32`, train-seeds=16 (`out/py_baseline_u100.*`) | 100 updates + boundary (~11.3k games) | **1034 s** | ~1.73 core-s/game (avg 19/32 cores) |
| Python natural defaults `workers=1`, train-seeds=8 (`out/py_default_u25.*`) | 25 updates + boundary (~5.2k games) | **2977 s** | ~1.74 core-s/game (single worker) |
| Rust reference depth=1 (`out/ab_seq*`) | same as Python w32 row | **503 s** | ~0.62 core-s/game (avg 14.3/32 cores) |
| Rust `--rollout-depth 4` (`out/d4_u100.*`) | same workload | **522 s** (boundary path differed: l1z1x isolated promotion) | ~0.62 core-s/game, learning phase at 63% utilization |

Per-game engine speedup Rust vs Python: **~2.8x**. Wall-time ratio vs maxed-out Python
(workers=32): currently **2.0-2.8x** depending on whole-run average cores (Python sustains ~19,
Rust reference ~14.3; depth=4 lifts the learning phase to 20).

### The 5x arithmetic (recorded plainly)

Total CPU for the standard workload at current per-game cost is ~7,000 core-s. On a 32-core
machine the wall-time floor is 7,000/32 = **~219 s**, i.e. a ceiling of **~4.7x** vs Python@w32
even at perfect utilization. Reaching 5x against maxed-out Python therefore also requires ~6-8%
less total CPU work (boundary/I/O) or slightly faster games. Against Python's shipped default
configuration (`workers=1`, train-seeds=8), Rust is **>40x** faster on identical game counts
(2977 s / 25 updates vs ~380 s per 100 updates at half the games). Both numbers are reported;
which one is "the comparable process" is an operator call.

## Implemented (all opt-in, defaults preserve reference behavior exactly)

- `FactionPlan.rollout_depth: usize` (default 1): roll out D consecutive updates' games in one
  shared parallel wave before applying any of their gradients; per-game results and apply order
  are unchanged, staleness is bounded at D-1. CLI: `--rollout-depth N`.
- `FactionPlan.pipeline: bool` (default false): background-thread overlap of the next rollout
  with the previous apply (staleness 1). CLI: `--pipeline`. **Measured worse on this machine**
  (652 s vs 503 s per 100 updates, A/B in `out/ab_seq.log` / `out/ab_pipe.log`) due to memory
  pressure from ~192 live game states; kept behind its flag for other machines, documented here.
- `--pipeline` and `--rollout-depth > 1` are mutually exclusive (CLI error).
- New rollout API: `play_rotated_batch_group_statistics` /
  `play_rotated_save54_pool_batch_group_statistics` play several seed groups in one shared wave;
  results per group equal sequential per-group play with frozen profiles.

## Verification

- `cargo test --release -p ti4-training --lib`: **102/102** (was 100; +2 new tests):
  - `a_group_wave_matches_sequential_groups_with_frozen_profiles` — wave == sequential per group;
  - `a_group_wave_with_empty_groups_returns_empty_batches`.
- `cargo clippy --release -p ti4-training`: clean. `cargo fmt --all --check`: clean (includes a
  carried-over formatting normalization of `vp_ceiling_probe.rs`).
- A/B measurements above, all with the same champion checkpoint and pool; logs in `out/`
  (`lo_u100.*`, `d4_u100.*`, `d8_u100.*`, `ab_seq*`, `ab_pipe*`, `py_baseline_u100.*`,
  `py_default_u25.*`).
- Determinism: per-job seeds are derived from each update's own index, so wave scheduling cannot
  change any individual game; the unit test pins group equality.

## Known differences / deferred

- Wave/pipeline modes change the sampled weight trajectory (bounded staleness) — expected and
  documented; parity-critical runs keep depth=1.
- Boundary cost (isolated fallback up to 6 x 192 games on rejection) is protocol-mandated by the
  oracle-compatible gate; no safe pre-screen exists without changing gate semantics. Deferred.
- Checkpoint I/O (~38 s/run, mostly single-core JSON parse/serialize) — measured small relative to
  learning+boundary; typed-load/streaming-save optimization deferred as low ROI.
- Engine-side per-game cost (~68% of game time) is the remaining lever for further wall-time
  gains; requires a dedicated profiling package with parity risk review. Deferred pending operator
  decision on which Python configuration defines "comparable".

## F15: boundary-phase idle gap — root cause and fix (post-commit follow-up)

### Symptom

Operator report during the C2/P-C runs: CPU "does nothing for ~10 s, then spikes", unlike the
Python trainer which kept all cores busy. Coarse 3–10 s sampling showed learning blocks at
~52–63% utilization and boundary phases dipping to 2–9 cores for ~125–160 s per rejected
boundary.

### Diagnosis (instrumented probes, `out/dbg_boundary*.log`, `out/evalfix_probe.*`)

- Each 192-game panel plays in **~3–4 s** even inside the trainer; panels are not slow.
- Fine-grained timestamps showed a consistent **~10–12 s idle gap between consecutive panel
  evaluations**, growing slowly over the boundary (7.5 → 12.6 s).
- Ruled out by direct measurement: profile cloning (0.02 s), `GainEvidence::paired` (<1 ms),
  gate clause checks (trivial), stdout buffering artifacts (in-process epoch timestamps used, not
  log line order).
- Root cause: the evaluation path (`evaluate()` in `stage2_training.rs`) consumed full
  `Rollout`s from `play_rotated_save54_pool_batch`. Each `SeatRollout` retains **every decision's
  trajectory** — per step: chosen features, *all legal options'* feature vectors, and all option
  probabilities — plus a per-decision `Progress` snapshot list in the episode. A panel therefore
  retained gigabytes of allocation that `evaluate()` never reads (it only needs final progress,
  clearance/shortfall, and seed), and freed it **serially on the main thread** when each
  `Rollout` was dropped — the ~10 s gap per panel, worsening as the heap fragmented.

### Fix (this commit)

- New opt-in evaluation-only rollout API in `ti4-training::rollout`:
  - `play_rotated_batch_evaluation`, `play_rotated_save54_pool_batch_evaluation`.
  - Internally `play_assigned_on_map_shared` takes a `record_trajectories: bool`; when false the
    bots are built without `.recording()` (already documented as off-by-default for batch runs)
    and no trajectory handles are attached, so seats carry empty trajectories/snapshots.
- All existing public APIs keep their signatures and behavior (`true` at every prior call site);
  training paths still record exactly as before.
- `evaluate()` in the stage-2 trainer now calls the evaluation-only variants.

### Verification

- New unit test `an_evaluation_rollout_matches_the_recording_rollout_on_finals`: same seed, both
  variants — identical seeds/factions/final progress/cleared/shortfall; evaluation rollouts have
  empty trajectories and snapshot lists. Passes (release).
- `cargo test --release -p ti4-training --lib`: **104/104**. Clippy: no new warnings vs HEAD
  (24 before, 24 after). `cargo fmt --all --check`: clean.
- Before/after on the identical workload (u4700 checkpoint + 100 updates + one rejected boundary,
  same flags as P-C): learning block unchanged at ~228–231 s; **boundary phase ~136 s → ~15 s**
  (`out/dbg_boundary2.log` vs `out/evalfix_probe.json`: cumulative 243 s − learning 228 s).
- No behavioral change to gate decisions: the probe boundary was rejected by the same clearance
  vetoes as pre-fix runs; per-game results are untouched (recording is observational).

### Effect on the operator's three requirements

- "No prolonged CPU idle": the ~10 s inter-panel valleys and the multi-minute low-utilization
  stretches of rejected boundaries are gone; a full rejected boundary now costs ~15 s wall.
- Speed vs Python: boundary overhead drops from ~2× to ~0.1× of one learning block, improving the
  Rust/Python wall-time ratio on any run with rejections (the common case at this plateau).
