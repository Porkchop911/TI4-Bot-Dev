# Where an MLP PPO update's time actually goes

Measured 2026-09-08 against `out/checkpoints/stage2-mlp-shaped` (the stage-2 shaped run, stopped at
update 3,600) on a 32-core machine with one RTX 3090.

> ## CORRECTED 2026-09-08 — §3 was wrong, and §5's ranking followed it
>
> **This document originally claimed the engine was 86.1% of rollout and named it the optimisation
> target. That is false.** The figure came from misreading `game_cost`, which times only the
> *candidate* seat's decider (`game_cost.rs:262-284` gives the other five seats throwaway counters)
> and prints the remaining wall clock beside it. That residual is not the engine: with six seats
> sharing one network it is mostly the other five seats' inference.
>
> An all-seat measurement (`engine_cost`, wrapping all six deciders and timing `Game::step` by
> phase) puts it the other way round: **policy ~82%, residual ~18%**, and on the direct per-step
> breakdown **engine ≈17.45% of engine-plus-policy**, i.e. roughly **10.5% of training wall time**
> rather than the ~52% this document asserted.
>
> Source: `plans/ENGINE_OPTIMIZATION_2026-09-08.md`, which also carries 108 identical
> choice/event/final-state hashes as trajectory evidence and five interleaved paired repetitions.
>
> **What survives:** §1 (rollout/optimise split), §2 (the wall-clock reconciliation and the hidden
> `Batch::freeze` cost), and §4 (why an update got slower). Those are independent of the bad
> attribution. **What does not:** every claim in §3 about the engine dominating, and the ordering in
> §5. The corrected ranking puts **inference first**, not the engine.

Every number below is measured.

## 0. The headline

| layer | share | note |
|---|---:|---|
| **rollout** (self-play, CPU, 32 threads) | **60.1%** of wall | of which ~82% is policy inference and ~17% the engine (CORRECTED — see the banner) |
| **optimise** (PPO epochs, CUDA) | **34.5%** of wall | scales almost exactly with batch size |
| **batch assembly** (`Batch::freeze`) | **4.4%** of wall | invisible in the log — see §2 |
| report + checkpoint blocks | 0.02% | negligible, measured |

Two things are worth knowing before reading further:

1. **The `total` printed per update is not wall clock.** `ppo_update.rs:1451` prints
   `rollout_time + optimise_time`. Anything between the two timers is unreported.
2. **Inside rollout, policy inference dominates the engine, not the other way round.** ~82% of a
   rollout is the six seats' forward passes; the engine is ~17%. This *is* an inference-bound
   trainer, on CPU. (The original text asserted the reverse; see the banner.)

## 1. What the log reports, over 3,605 updates

Parsed from `out/stage2-mlp-shaped.log`:

| phase | mean | median | min | max | share of reported total |
|---|---:|---:|---:|---:|---:|
| rollout | 10.65s | 10.70s | 2.20s | 30.50s | **63.6%** |
| optimise | 6.09s | 6.20s | 1.80s | 10.90s | **36.4%** |
| total | 16.75s | 16.90s | — | — | 100% |

The 63.6 / 36.4 split is remarkably stable — it moves from 61.6/38.4 in the first 300 updates to
64.3/35.7 in the last 300 and never leaves that band.

## 2. Reconciling with the clock

| | |
|---|---:|
| wall clock (21:19:04 Sep 7 → 15:02:23 Sep 8) | 63,799s (17.722 h) |
| sum of reported `total` | 60,372s (16.770 h) |
| **unaccounted** | **3,427s (5.4%)** |

To find out what the unaccounted time is, two runs from the same checkpoint with identical
arguments and only the update count differing:

| run | updates | Σ reported | wall | gap |
|---|---:|---:|---:|---:|
| A | 10 | 181.4s | 190.02s | 8.62s |
| B | 20 | 371.8s | 388.23s | 16.43s |

Solving the two equations gives **0.78s per update** of unaccounted time and **0.81s** of one-off
process startup.

**What that 0.78s is.** The only code between `rollout_time = rolled.elapsed()`
(`ppo_update.rs:1301`) and `let optimised = Instant::now()` (`ppo_update.rs:1391`) is the
demonstration-replay block and `Batch::freeze` (`ppo_update.rs:1383`). This run passed no
`--demo-per-update`, which defaults to 0, so the replay block is skipped entirely. The 0.78s is
therefore essentially all batch assembly — **4.4% of wall time that the log never shows.**

**Report and checkpoint blocks are not the answer.** A separate A/B — 10 updates at
`--report-every 100` (no reports) against 10 updates at `--report-every 1` (ten reports, each
writing a 23 MB bundle and reloading it to verify) — differed by **2.73s total**, i.e. **0.27s per
report block**. Across the real run's 36 reports that is under 10 seconds.

**Residual.** 0.78 × 3,605 = 2,816s predicted against 3,427s measured, leaving ~611s (10 min)
unexplained over 17.7 hours. The likely cause is contention I created myself: several CPU-only
evaluations (`clearance_eval`, `build_positive_corpus`, `behaviour_report`, `game_cost`) ran
concurrently with training and stole cores from the rollout threads. That is a measurement
artifact of this session, not a property of the trainer.

## 3. Inside rollout: policy, not the engine (SECTION CORRECTED)

**What this section originally said was an artefact of the instrument.** `game_cost` shares its
timing counters only with the candidate seat (`game_cost.rs:262-284`); the other five seats get
counters that are created and dropped. Subtracting that one seat's decider time from the game's wall
clock therefore leaves *five more seats' inference* sitting inside a bucket labelled "engine". With
six seats on one network, roughly five sixths of policy time was booked as rules work — which is the
size of the error.

`engine_cost` wraps all six deciders and times `Game::step` by phase. Same bundle, 4 rounds,
`checkpoint-473312`:

| | time | share |
|---|---:|---:|
| inside all six deciders | 263.5s | **81.8%** |
| everything else (rules, setup, diagnostics) | 58.6s | **18.2%** |

322.1s wall, 369,774 decisions — exactly six times the 61,629 the candidate-only run counted, which
is itself the tell.

On the direct per-step breakdown, with diagnostic overhead removed, the engine is
`8.68 / (8.68 + 41.08) = **17.45%**` of engine-plus-policy. Carried onto the 60.1% rollout share
that is roughly **10.5% of training wall time**, not the ~52% this document first claimed. The
transfer is an assumption: a sequential greedy evaluation is not 32-thread PPO at temperature 2.5.

**So the trainer is inference-bound, on CPU.** A smaller trunk, batched forward passes or
quantisation attacks ~82% of 60% of wall — roughly half of training. The engine is the minority
cost.

Full method, paired repetitions and the trajectory-hash evidence:
`plans/ENGINE_OPTIMIZATION_2026-09-08.md`.

### Candidate-seat decider cost by head

Unaffected by the correction above — this table always was one seat's decider time, measured
directly rather than by subtraction.

| head | calls | total | per call |
|---|---:|---:|---:|
| trade | 14,983 | 11.34s | 756.92µs |
| **activation** | 1,508 | 6.79s | **4.50ms** |
| turn | 10,609 | 5.36s | 505.48µs |
| cargo | 4,187 | 2.40s | 572.44µs |
| production | 2,880 | 2.00s | 696.06µs |
| payment | 3,989 | 1.64s | 410.00µs |
| movement | 3,029 | 1.58s | 521.99µs |
| tokens | 4,023 | 1.54s | 381.74µs |
| other | 1,671 | 1.49s | 892.74µs |
| secondary | 4,061 | 1.41s | 347.05µs |
| agenda | 1,913 | 1.31s | 682.34µs |
| landing | 2,625 | 1.30s | 495.89µs |
| ability | 2,833 | 1.25s | 441.09µs |
| development | 612 | 0.72s | 1.18ms |
| strategy | 937 | 0.45s | 483.66µs |
| scoring | 908 | 0.36s | 401.23µs |
| exploration | 528 | 0.14s | 266.51µs |
| combat | 327 | 0.14s | 428.40µs |
| transit | 6 | 0.01s | 1.50ms |

Two different problems here. **`trade` is the largest total purely on call volume** — 14,983 calls
at an unremarkable 757µs. **`activation` is the expensive one per call at 4.50ms, six times the
median head**, which is consistent with it choosing over the largest option set (every activatable
system). `development` (1.18ms) and `transit` (1.50ms) are also slow per call but too rare to
matter.

### Parallelism

Rollout is rayon-parallel over 32 threads, split one chunk per thread rather than one job per
thread: each chunk carries an owned `Actor` copy because `tch::Tensor` is `Send` but not `Sync`, so
per-job copies would allocate 96 actors instead of one per core (`ppo_update.rs:1234-1248`). With
`SEEDS_PER_UPDATE = 16` and 6 rotations, that is 96 games over 32 threads — 3 games per thread.

## 4. Why an update got slower: it is batch size, not inefficiency

| updates | decisions | rollout | optimise | total | roll% | **µs/decision** |
|---|---:|---:|---:|---:|---:|---:|
| 0–300 | 96,439 | 6.99s | 4.37s | 11.36s | 61.6% | **117.8** |
| 600–900 | 122,804 | 9.64s | 5.58s | 15.21s | 63.3% | 123.9 |
| 1200–1500 | 136,203 | 10.59s | 6.16s | 16.76s | 63.2% | 123.0 |
| 1800–2100 | 138,196 | 11.01s | 6.33s | 17.35s | 63.5% | 125.5 |
| 2400–2700 | 140,779 | 11.46s | 6.46s | 17.93s | 64.0% | 127.3 |
| 3300–3600 | 147,549 | 12.83s | 7.12s | 19.96s | 64.3% | **135.3** |

Update time grew **+76%** (11.36s → 19.96s) while decisions per update grew **+53%** (96,439 →
147,549). Cost per decision rose only **+15%** (117.8 → 135.3 µs).

Correlation with decisions per update: **optimise 0.931**, **rollout 0.823**. The optimiser is very
nearly linear in batch size, as expected for a fixed epoch and minibatch schedule.

So the slowdown is mostly the policy learning to *do more*: `tactical/seat` rose from 4.7 at update
0 to 6.2–6.4 by the end, games run longer, and more decisions are recorded per game. That is
training working, not the trainer degrading. The residual +15% per decision is small and unexplained
— plausibly cache pressure from larger batches, or longer games producing more expensive positions
for the engine to resolve.

## 5. What to optimise, in order

Reordered after the §3 correction. The original list led with the engine on the strength of the bad
attribution; on the corrected numbers it is the minority cost.

1. **CPU inference — ~82% of rollout, roughly half of training wall.** Six seats each run a forward
   pass per decision, sequentially within a game. This is where the time is. Candidates, none yet
   measured: batching forward passes across the seats or games resident on a rollout thread, a
   smaller trunk, or quantisation. Note `ti4_tensor::inference_device()` is hard-coded to
   `Device::Cpu` under §7.1, so a GPU inference path is a spec change, not just an implementation.
2. **`Batch::freeze` — 4.4% of wall, and currently invisible.** `total` silently excludes it, which
   is how it went unnoticed. Timing and printing it is cheap and makes the log honest.
3. **The engine — ~17% of rollout, ~10.5% of wall.** Real but secondary. The supply-catalogue fix in
   `ENGINE_OPTIMIZATION_2026-09-08.md` already takes ~15% of engine time, worth ~1.4–1.6% of
   training; further engine work has a low ceiling.
4. **Not the optimiser.** 34.5% of wall, already on CUDA, and linear in batch size.
5. **Not checkpointing.** 0.27s per report block.
6. **Not the `activation` head's option construction.** Its next-decision gap is 0.05s against 7.04s
   spent *inside* activation deciders — the cost is the forward pass, which is item 1, not the
   engine building the option set.

## 6. Artifacts

```
out/stage2-mlp-shaped.log            3,605 updates of per-phase timings
out/timing-noreport.log              A: 10 updates, no reports  (wall 190.02s)
out/timing-report.log                B: 10 updates, 10 reports  (wall 192.75s)
out/timing-20.log                    C: 20 updates, no reports  (wall 388.23s)
out/game-cost-u3600.log              decider/engine split and per-head costs
crates/ti4-mlp/examples/ppo_update.rs:1229-1451   the two timers and what lies between them
crates/ti4-mlp/examples/game_cost.rs              the decider/engine instrument
```
