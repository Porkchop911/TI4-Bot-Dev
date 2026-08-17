# Focused plan: engine efficiency — and a correction to its premise

Date 2026-08-17. Supersedes the compute sections of
`plans/PLAN_2026-08-17_ALGORITHM_ARENA_AND_COMPUTE.md` and Part 3 of
`plans/ANALYSIS_2026-08-17_LEARNING_AND_COMPUTE.md`.

---

## 0a. Headline (measured after the first draft of this plan)

**`mimalloc` as the global allocator: 1.89× on the whole training path, single-threaded, one line.**

```
system allocator : 17.39 / 17.83 / 18.46 ms per training game   (mean 17.89)
mimalloc         :  9.26 /  9.71 /  9.41 ms per training game   (mean  9.46)
```

30 games per run, trained profiles, `FULL`, 4 rounds, release. Measured single-threaded, so the
32-thread figure should be **better**, not worse: the system allocator's contention is what mimalloc's
per-thread heaps remove.

A second measurement makes the diagnosis general:

```
-C target-cpu=native (Zen 5, AVX-512) :  22.71 -> 22.48 ms/game   (+1%, i.e. nothing)
```

Together these say the workload is **allocation- and pointer-chasing-bound, not compute-bound**.
That determines which whole *classes* of optimisation are worth pursuing here, independent of any
algorithm choice:

| Pays | Does not pay |
|---|---|
| Better allocator | SIMD / vectorisation |
| Fewer, flatter allocations | Instruction selection (`target-cpu`) |
| Contiguous structures (`Vec`) over node-based (`BTreeMap`) | GPU offload |
| Interning strings to integers | |

This is the general answer to "which efficiency work survives an algorithm change": the ones in
the left column do, because the bottleneck is a property of the data layout, not of the learning
rule.

**Revised split, re-measured under mimalloc** (the §0 table below was taken under the system
allocator, i.e. under the configuration this plan now recommends replacing):

| Component | System allocator | **Under mimalloc** |
|---|---|---|
| Engine | 2.83 ms (10.2%) | **1.16 ms (11.9%)** |
| Policy — features + scoring | 15.14 ms (54.6%) | **5.85 ms (59.7%)** |
| Trajectory recording | 9.77 ms (35.2%) | **2.78 ms (28.4%)** |
| Total | 27.73 ms | **9.80 ms** |

Every slice gets faster; the ratio is preserved. The engine remains ~12%, the policy remains the
target. The conclusion of §0 stands.

---

## 0. The measurement that changes this plan

I was asked for an engine efficiency plan on the shared premise that the engine is ~68% of
training compute. **I measured it before planning against it, and the premise is wrong.**

Method: standalone probe outside the repo, release build, single-threaded, 6 seats, 4 rounds,
`FULL` sources, Rust varied maps, 12 games per configuration. Engine cost isolated with random
deciders (`Seats::Random`, no policy, no recording); policy cost isolated by the difference
between `play_rotated_batch` (recording) and `play_rotated_batch_evaluation` (no recording).

| Component | BLANK profiles | **TRAINED profiles** (`run_pure_u5000.json`, 286,431 weights) |
|---|---|---|
| Engine | 2.83 ms (34.9%) | **2.83 ms (10.2%)** |
| Policy — features + scoring | 1.84 ms (22.7%) | **15.14 ms (54.6%)** |
| Trajectory recording | 3.44 ms (42.4%) | **9.77 ms (35.2%)** |
| **Total per game** | 8.11 ms | **27.73 ms** |

**The engine is ~10% of a real training game. Policy plus recording is ~90%.**

Note the scaling: going from blank to trained weights takes the policy slice from 1.84 ms to
15.14 ms — **8.2×** — with no change to the engine. That is `BTreeMap<String, f64>` lookup cost
growing with map size and string-comparison depth, exactly the mechanism flagged earlier. It also
means the cost grows as training continues.

### 0.1 This agrees with the project's own instrumentation once the arithmetic is redone

`STAGE2-SCHEDULING-WAVES.md` records instrumented `consider()` costs of 644 core-s (features) +
117 core-s (scoring) = 761 core-s over a 25-update run, and a measured 0.62 core-s/game.

```
learning games in that run   = 25 updates x 96 games = 2,400
learning-phase game cost     = 2,400 x 0.62          = 1,488 core-s
policy share of LEARNING     = 761 / 1,488           = 51%
```

The doc's "~32%" comes from dividing by ~11.3k games — the whole run including **boundary
evaluation games**, which are evaluation-only and (since F15) record no trajectories. That mixes
two different workloads in one denominator. Corrected, the project's own data already said policy
≥ 51% of learning cost, and my measurement (which additionally counts recording, and uses a much
larger trained weight table) says ~90%.

### 0.2 Caveats, stated plainly

- Single-threaded. At 32 threads, allocator contention inflates the allocation-heavy slices
  (policy, recording) more than the engine, so the engine share likely falls further, not rises.
- Rust varied maps, not the `save52` pool used in real runs; a larger board raises engine cost
  somewhat.
- 12 games per configuration — enough for a 10%-vs-68% call, not for tuning.
- My absolute per-game figure (27.7 ms) is far below the recorded 0.62 core-s/game. That gap needs
  reconciling (board size, thread contention, or an error in the original figure) — but it does not
  affect the *ratio*, which is what this plan turns on.

### 0.3 What survives from the earlier reasoning

The durability argument was right; my mapping of it onto the code was wrong. **Feature
construction and trajectory encoding are needed by every learned arm** — REINFORCE, PPO, ExIt,
behaviour cloning all build features and all do credit assignment. They are just as
algorithm-agnostic as the simulator is. The safest-investment principle holds; it points at a
different file.

---

## 1. Engine work still worth doing

At 10% of the game, the engine cannot be the main lever. These are on the list because they are
**cheap and provably durable**, not because they are large. Ceiling on the whole section: ~5% of
training wall-clock.

| # | Change | Evidence | Effort |
|---|---|---|---|
| E-1 | **Memoize `galaxy::all_planets`** behind a `OnceLock` per `(SourceSet)` | 1,102 ns/call measured. `production::planet_value` (`production.rs:80`) rebuilds the entire 110-planet map to read **one** planet, and is called from an O(n²) loop in `payment_options` (each planet's guard sums `max_face_value` over every other planet) | 1 h |
| E-2 | **Memoize `units::catalogue`** the same way | 2,786 ns/call at `FULL`, builds a 104-entry `BTreeMap` per call, **91 call sites** in `ti4-engine` including per-decision paths | 1 h |
| E-3 | **Memoize `galaxy::planets_in`** per `(system, SourceSet)` | 2,327 ns/call, 14 sites, called per commit-loop iteration in `invasion.rs` | 1 h |
| E-4 | **Hoist the O(n²) guard in `payment_options`** — `max_face_value` over all planets is recomputed inside the per-planet loop; compute the vector once and subtract | `production.rs`, `payment` is a live head (61 decisions/game in the T5 trace) | 2 h |

**Acceptance criterion for all four: byte-identical decision traces.** These are pure speedups, so
`single_game_trace` on a fixed seed must produce an identical trace before and after. That is
mechanical, not a judgement call, and it is what makes this work permanent.

## 2. Where the compute actually is — and it is equally durable

| # | Change | Why it is the real target | Effort |
|---|---|---|---|
| P-1 | **Intern feature names → `u32`; weights → dense `Vec<f32>`** | The 8.2× blank→trained scaling *is* this cost. Needed by every learned arm. Removes the growth-with-training problem entirely. | 3–5 d |
| P-2 | **Compact trajectory encoding** — `Vec<(u32,f32)>` not `BTreeMap<String,BTreeMap<String,f64>>` | 35% of a training game. Note: **do not remove** the trajectory — PPO/ExIt need it (see §0.3 of the arena plan). Shrink it. | 2 d |
| P-3 | **Intern option ids and labels** | 226 ns per `ChoiceOption` × ~7,000 options/game ≈ 1.6 ms. Sits on the engine/policy boundary: labels exist only to be tokenized by the feature builder, so this fixes both slices at once. **Highest-leverage single change in either list.** | 2 d |
| P-4 | **Hoist prompt tokenization out of the per-option loop** | `tokens(&choice.prompt)` runs once per *option*; up to 37 options per decision | 2 h |
| P-5 | **mimalloc global allocator** | **MEASURED 1.89×** (§0a). Do this first, before anything else in either list. | 1 h |

### 2.1 Other agnostic wins, measured or sized

| Change | Measured | Verdict |
|---|---|---|
| `-C target-cpu=native` | +1% | **Skip.** Not compute-bound. |
| Profile deep-copy — `Arc::new(profile.clone())` per batch call (7 sites in `rollout.rs`) | 28.7 ms per call for 286k weights, ~1% of an update | Trivial fix, low value. Do it while nearby. |
| Redundant per-seed setup — `map_filler` (0.287 ms) + `start_game_seeded` (0.050) + `build_board` (0.012) recomputed identically for all 6 rotations of a seed | 1.74 ms wasted per seed, ~1% | Trivial fix (cache per seed), low value. |
| **ID interning** — `PlayerId`/`SystemId`/`PlanetId`/`UnitTypeId` are all `String` newtypes; every `Unit` carries two heap allocations | not isolated | **Defer.** It is the right *class* of fix, but it is a workspace-wide refactor and mimalloc already captures much of the same cost for one line. Re-measure after P-5 before committing. |
| **Checkpoint I/O** — 32–37 MB JSON per checkpoint, ~38 s per run recorded in `STAGE2-SCHEDULING-WAVES.md` | not re-measured | Worth doing. `bincode` is already a workspace dependency. Survives every algorithm. |
| **Utilisation 63% → 85%** | — | Survives. Re-test `RAYON_NUM_THREADS=16` **after** P-5 — the SMT penalty may have been allocator contention all along. |
| **PGO** | untested | Cheap to try, but expect modest given the target-cpu result. |

## 3. Order of work

0. **P-5 (mimalloc) alone, first, and re-measure everything after it.** It is one line for 1.89×,
   and it moves the baseline that every other item is judged against.
1. **E-1, E-2, E-3, P-4** — one session. All are hours, all are mechanical, all verifiable by
   identical-trace. Re-measure with the probe afterwards.
2. **P-3** (option interning) — the boundary fix, biggest single win.
3. **P-1, P-2** — the two structural changes, in that order.
4. **E-4** — cheap, do it whenever.

Deliberately **not** in this plan: pruning dominated trade offers. It changes the offered option
set, which is a modelling decision with oracle-parity consequences, not a speedup. It belongs in a
separate package with a parity review.

## 4. What to measure, and when to stop

Re-run the probe (engine / policy / recording split, blank **and** trained profiles) after each
numbered item. Two stopping conditions:

- **Engine work stops when the engine slice is below 5%** of a trained-profile training game. At
  that point further engine effort cannot repay itself regardless of how cheap it is.
- **P-1 succeeds if the blank→trained policy ratio drops below 2×.** If interning does not flatten
  that 8.2× scaling, the diagnosis is wrong and the cost is somewhere else — re-profile before
  continuing.

Also worth resolving early: the 27.7 ms vs 0.62 core-s/game discrepancy. Run the probe against the
`save52` pool and at 32 threads. If the gap is thread contention, P-5 is worth more than estimated;
if it is board size, E-1/E-2/E-3 are worth more than estimated. Either answer changes the ordering
above, and it is an hour of work.
