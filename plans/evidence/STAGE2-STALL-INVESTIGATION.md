# Stage-2 stall investigation — evidence (T0 forensics, T1 high-resolution evaluation)

Date: 2026-08-14. Branch `codex/stage1-parity-fixes`. Safepoint commit `66fd234`, tag
`safepoint/stage2-stall-baseline` (rollback point for this investigation; created before any new
edit).

## Objective and scope

Explain why Stage-2 training appears stalled — the frozen champion in every recorded checkpoint,
no promotions across ~1,100 updates — and test the proposed remedies against real artifacts. The
user excluded failure mode B (process actually dead) and C (wall-clock stall); the observed mode is
A: alive process, flat progress. Scope of this package: diagnosis + instrumentation + two
differential experiments (T1 evaluation-only at higher resolution; T2 learning-rate differential).
No rule changes, no optimizer redesign yet.

## T0 — checkpoint forensics (read-only)

Artifacts inspected under `out/`: `stage2_from_stage1.json` (update 4600), `stage2_s2p1.json`
(5000), `stage2_s2p2.json` (5700, lr 0.03, n=8 validation), `stage2_lr_test.json` (5200).

Findings:

- The accepted champion table is byte-stable across every recorded boundary of the lineage; it
  equals the bootstrap state from `stage2_from_stage1`.
- Candidate aggregate gains over the champion at n=8 boundaries: +0.46, −0.15, +0.33, +0.69,
  +0.68, +0.39, +0.13, +0.67, +0.69, +0.50, +0.14 — mean ≈ +0.4, no upward trend over ~1,100 updates.
- Per-faction clearance "regressions" exceeding the 0.03 veto tolerance appeared at most n=8
  boundaries (worst l1z1x down to about −0.167). Flagged as suspect: 48 games per faction panel is
  low resolution for a 0.03 tolerance on a ratio metric.
- Learner profile vs champion: ~87k cells, ~80% differ by >1e-6, mean |Δ| ≈ 0.0014, max ≈ 0.31 —
  dense small movement consistent with a random walk around the bootstrap rather than directional
  drift.
- Telemetry cross-check on per-update weight movement (`telemetry.update_norm` summed over a
  block's updates): s2p1/s2p2 blocks at lr=0.03 average ≈0.034/update; the `stage2_lr_test.json`
  blocks look ~half in raw totals only because that run used 50-update blocks (`every=50`) — per
  update they are identical, so no hidden step-size change occurred there. The T2 run confirms the
  scaling directly: at lr=0.01 movement is ≈0.010–0.015/update (one third), i.e.
  `update_norm ∝ learning_rate` with no clip-saturation regime in sight (`gradient.rs::apply`: 
  `delta = lr · shrink · value / actions`, and T2's steps are well below any saturation bound).

## Instrumentation added to `stage2_training.rs`

Safepoint-preserved behavior; gate boolean logic unchanged (`acceptable_stage_two_table` is now a
thin wrapper over the new clause reporter, and all pre-existing example tests pass unmodified):

- `failed_stage_two_clauses(...)`: names every violated gate clause in check order (clearance veto
  per faction, VP veto per faction, aggregate margin, paired σ evidence) with actual numbers.
- Console: every boundary that promotes nothing now prints one line per rejecting clause.
- `--eval-only` + `--eval-out <path>`: evaluates candidate and champion panels on identical seeds
  (paired), prints a per-faction table plus the gate verdict, writes a JSON sidecar, trains
  nothing. No checkpoint mutation.
- `--learning-rate <x>`: overrides `FactionPlan::stage_two_reference()` step size; recorded in the
  checkpoint `arguments` map and printed at startup so runs are reproducible from their own
  documents.

Unit test added: `the_gate_explain_which_clause_vetoes_a_candidate` — a table failing all four
clauses names all four; a clean table explains nothing (proving wrapper equivalence).

Verification: `cargo test -p ti4-training --example stage2_training` → **11 passed, 0 failed**
(10 pre-existing + 1 new), on the release-instrumented binary path.

## T1 — evaluation-only at n=32 against the frozen 5700 state

Command (release build):

```
cargo run -q --release -p ti4-training --example stage2_training -- \
  --checkpoint out/stage2_s2p2.json --validation-seeds 32 --eval-only \
  --eval-out out/eval_t1_5700_n32.json \
  --map-pool D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz
```

Result (wall time 24 s; artifact `out/eval_t1_5700_n32.json`):

| faction | cand VP | acc VP | ΔVP | cand clr | acc clr | Δclr |
|---|---|---|---|---|---|---|
| sol | 2.031 | 2.047 | −0.016 | 0.8073 | 0.7656 | +0.0417 |
| letnev | 2.042 | 1.969 | +0.073 | 0.8333 | 0.8281 | +0.0052 |
| xxcha | 1.943 | 1.802 | +0.141 | 0.7917 | 0.8021 | −0.0104 |
| hacan | 1.854 | 1.797 | +0.057 | 0.8229 | 0.7500 | +0.0729 |
| jolnar | 2.094 | 2.146 | −0.052 | 0.8385 | 0.8542 | −0.0156 |
| l1z1x | 1.927 | 1.943 | −0.016 | 0.7604 | 0.7708 | −0.0104 |

Paired evidence: **gain +0.188, SE 0.180, samples 32** → 2σ detection threshold 0.361.
Gate verdict: `FAIL — aggregate margin (0.1875 ≤ 0.30); sigma evidence (0.1875 < 2×SE)`.

Conclusions:

1. **No veto fires at n=32.** The worst clearance *regression* is −0.016 (l1z1x, xxcha) against a
   0.03 tolerance; the worst VP regression is −0.052 (jolnar) against 0.15. The repeated n=8
   "clearance regressions" (down to ≈−0.17 for l1z1x) were measurement noise at 48 games per
   faction panel, not real erosion. The per-faction veto is not structurally deadlocking promotion
   at adequate resolution — it was rejecting noisy panels.
2. **The true aggregate gain of this learner over the champion is +0.19 ± 0.18 (SE)** —
   statistically indistinguishable from zero, and below both the margin bar (0.30) and the noise
   floor (0.36). The n=8 mean (+0.4) was inflated by small-panel seed luck.
3. **Revised primary diagnosis: the stall is an optimization problem (≈zero drift after 1,100
   updates), not a gate miscalibration.** At sufficient panel resolution the gate's refusal of this
   state is correct; there is nothing real to promote yet.

## T2 — learning-rate differential run (complete)

Hypothesis: lr=0.03 with clip=1.0 saturates per-head movement near the step cap each update, so
batches act as noisy O(1) perturbations (random walk). Halving the step to lr=0.01 should reduce
per-update jitter; if drift is directionally present at all, cumulative paired gain vs champion
should trend upward across boundaries instead of hovering at ~+0.2±noise.
(Outcome note: the clip-saturation premise did not hold — see T0 correction and displacement
table below; movement scales linearly with lr.)

Design: identical resume state (`out/stage2_from_stage1.json` @4600), identical seed schedule and
map pool as the recorded baseline lineage; only `--learning-rate 0.01` differs, plus n=32 boundary
panels (the T1 finding that n=8 is under-resolved for these tolerances). 1000 updates with a
boundary every 100 → absolute update positions 4700…5600 align one-for-one with the recorded lr=0.03
blocks in `stage2_s2p2.json`, enabling direct per-update comparison.

Command (background, log `out/logs/t2_lr001.log`):

```
cargo run -q --release -p ti4-training --example stage2_training -- \
  --checkpoint out/stage2_from_stage1.json \
  --out out/stage2_test_lr001_n32.json \
  --updates 1000 --every 100 --learning-rate 0.01 \
  --validation-seeds 32 --confirmation-seeds 32 \
  --map-pool D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz
```

Run finished 2026-08-14 20:34 WEDT: `1000 updates in 3308.7s (3.309 s/update)`, 0 errors, 0
zero-movement updates. Binary built ~19:43 from this tree; later edits to the example were cosmetic
only (rustfmt + inline format args), so results are valid.

Per-boundary candidate-vs-champion aggregate VP gain at matching absolute updates:

| update | baseline n=8 (lr .03) | T2 n=32 gain ± SE (lr .01) |
|-------:|----------------------:|---------------------------:|
| 4700   | +0.458 | −0.271 ± 0.202 |
| 4800   | −0.146 | −0.198 ± 0.178 |
| 4900   | +0.333 | −0.203 ± 0.159 |
| 5000   | +0.688 | −0.135 ± 0.177 |
| 5100   | +0.688 | −0.276 ± 0.198 |
| 5200   | +0.396 | −0.162 ± 0.186 |
| 5300   | +0.125 | −0.109 ± 0.203 |
| 5400   | +0.667 | −0.021 ± 0.197 |
| 5500   | +0.688 | +0.000 ± 0.189 |
| 5600   | +0.500 | −0.094 ± 0.174 |

- Every T2 boundary gain is within ≈1.4σ of zero; mean over the ten boundaries ≈ **−0.15**, i.e.
  statistically indistinguishable from no improvement. A weak late-run drift toward ~0 (from −0.28)
  stays below per-boundary noise and is not actionable evidence.
- The baseline n=8 column averages +0.43, but T1 showed the same lineage's final state has a true
  gain of only +0.19 at n=32: the fixed 8-seed panels over-read this candidate lineage by
  ≈+0.2–0.5 (the same seeds are re-used at every boundary evaluation, so their per-game bias is not
  averaged out across boundaries). The baseline's apparent "hovering around +0.4" was largely panel
  noise.
- Promotions: **none in either run** — T2 rejected at every boundary (aggregate margin and σ
  evidence; occasional `l1z1x` clearance vetoes, e.g. −0.062 to −0.038 vs the 0.03 tolerance,
  which are real even at n=32).

Weight displacement from the shared champion (`out/stage2_from_stage1.json` @4600), per faction,
Σ|Δ| over common cells:

| faction | T2 (lr .01, 1000 upd) | s2p2 (lr .03, 1100 upd) |
|--------:|----------------------:|------------------------:|
| hacan   | 5.87 (mean\|Δ\| 4.0e-4) | 16.29 (1.1e-3) |
| jolnar  | 4.98 (3.2e-4)          | 14.42 (9.4e-4) |
| l1z1x   | 5.21 (3.7e-4)          | 16.64 (1.2e-3) |
| letnev  | 6.35 (4.8e-4)          | 17.54 (1.3e-3) |
| sol     | 7.06 (4.4e-4)          | 19.58 (1.2e-3) |
| xxcha   | 6.08 (4.7e-4)          | 16.05 (1.2e-3) |

Ratio T2/baseline ≈ **0.31–0.38** (mean ~0.36), matching the total step-budget ratio
(lr×updates = 10/33 = 0.30; random-walk lr√N ratio 0.32) — displacement is proportional to the
step budget with no super-linear signal accumulation.

## Conclusion (evidence-backed)

The Stage-2 stall (mode A: alive, flat progress) is a **zero-signal optimization problem**, not a
gate bug or mis-tuned threshold:

1. Both learning rates produce directionless exploration: ~75–83% of weight cells move in every
   update block, but net displacement after 1000+ updates leaves paired table-VP gain within ±1.4σ
   of zero against the champion. The gate's rejections are all correct; there is genuinely nothing
   to promote.
2. Root cause candidate: the four-round horizon compresses outcomes into near-ties (≈1–2 VP spread,
   ≈1.3 scoring decisions per faction-game), so centered REINFORCE credit carries almost no
   directional information about play quality, and entropy-regularized steps behave as a random
   walk around the bootstrap.
3. The n=8 validation panels compounded the appearance of a deadlock: they produced noise-driven
   clearance/VP vetoes at most boundaries (T1 showed none fire for the same state at n=32) and
   over-read candidate gains by ≈+0.2–0.5 because the fixed seed set's per-game bias repeats at
   every boundary.
4. Learning rate is not a useful lever here: halving it halves weight displacement but does not
   create drift (T2). Keep n≥32 boundary panels and σ-based promotion — those are now validated as
   well calibrated; the missing ingredient is reward signal, not gate strictness.

## T3: Python oracle Stage-2 parity audit (read-only, 2026-08-14)

Inspected `D:/Projects/ti4-engine/out/stage2_pg_six_c_20260810.json` (latest lineage segment,
`final_update` 3500, resumed from `stage2_pg_six_b`, which resumed from `stage2_pg_six`) and its
tools source. The oracle's Stage-2 **did** make promotions: u3100 was a schema-migration
re-acceptance (all six factions), then real isolated per-faction promotions at **sol@u3350** and
**xxcha@u3450** — each requiring an aggregate table gain > 0.30 on two independent n=32 panels.
So under the oracle configuration, promotion-grade improvement does emerge around u≈3350–3450,
while the Rust lineage shows zero net drift through ~5700 updates of nominally identical setup.

Parity audit result (Rust vs Python `tools/train_stage1_policy_gradient.py`):

| Dimension | Python oracle | Rust | Verdict |
|---|---|---|---|
| Horizon | 4 rounds | 4 rounds | equal |
| lr / entropy / clip | 0.03 / 0.01 / 1.0 | same defaults | equal |
| Training batch per update | 16 seeds × 6 rotations = 96 seat-games | `train_seeds=16` × 6 = 96 | equal |
| Map pool | `save52_e400_n8192.json.gz`, varied maps | same file (T1/T2) | equal |
| Boundary panels | n=32 validation + n=32 confirmation, fixed seeds per run | same defaults | equal |
| Gate tolerances | margin 0.05/faction, VP veto 0.15, clearance veto 0.03 | identical | equal |
| Reward/returns | potential-difference reward + round-1 bonus/shaping, undiscounted suffix sums | `reward.rs` is a verified port (golden tests against oracle `_returns`) | equal |
| Update law | centered returns, scale = sqrt(var) or 1, mean-gradient clip via shrink, entropy term | line-level equivalent in `gradient.rs::apply()` | equal |
| Weight movement | telemetry u3401..3500: ≈4.6–6.8 Σ\|Δ\| per head /100 updates | s2p2: ≈3.0–4.7 /100 updates | same order; earlier suspected ~7× step-size gap resolved |
| Trajectory capture | every decision (`progress_interval=8` only throttles progress-snapshot recomputation) | every decision | equal |

Remaining Rust deviations — all make Rust **stricter** than the oracle, none explain zero drift
(no T1/T2 boundary came near these thresholds):

1. Extra paired-σ clause in `acceptable_stage_two_table` (default 2σ; `--accept-sigmas 0`
   restores the exact oracle gate).
2. Isolated-path `faction_improved` is VP-only; Python's `better()` uses a clearance tiebreak.
3. Evaluation cadence: Rust default every 100 updates vs oracle eval_every 50 (half the
   promotion chances per update).
4. No per-game wall-clock cap (Python converts >30 s games into counted stalls, ≈0 in those
   runs) — immaterial for this data.

Also confirmed: Python's isolated fallback path applies the **same** aggregate margin clause as
the assembled path, so that is not a Rust-specific gap; and fixed validation seeds per run mean
the oracle has the same correlated-panel property as Rust (see below).

## Panel decorrelation (`--panel-step`, opt-in)

Motivation: every boundary re-measuring the same fixed panel makes all boundary gain estimates
correlated — cross-boundary trend tests are statistically invalid, and one noisy panel's vetoes
repeat forever (exactly what T1 showed for n=8; a property Python shares). New flag on
`stage2_training.rs`: `--panel-step N` starts each boundary k's validation/confirmation seed
block at `base + k·N`, so adjacent panels are disjoint when N ≥ panel size. Default 0 keeps the
historical fixed-panel behavior bit-for-bit (regression-tested).

Wiring: `first_seed_for_boundary(base, step, index)` helper (unit-tested for both cases); bootstrap
comparison is boundary 0 on the base seed; per-boundary first seeds are recorded in each history
entry (`validation_first_seed`, `Option` with serde default so old checkpoints load unchanged) and
`panel_step` is stored in checkpoint arguments. Smoke run `out/smoke_panel_step.json`
(`--updates 50 --every 25 --train-seeds 1 --validation-seeds 2 --panel-step 3`) shows history
seeds 96000000 / 96000003 / 96000006 for boundaries 0/1/2 as expected.

**Pairing-contract bug caught in the T4 launch (attempt 1, killed at ~u4729):** `GainEvidence::paired`
matches candidate and champion measurements by shared source seed. With stepping on, each fresh
candidate panel shares no seeds with the champion's bootstrap measurement, so every boundary
reported `gain=+0.000, samples=0` (visible in `out/logs/t4_oracle_parity_attempt1_brokenpairing.log`
and preserved artifact `out/stage2_t4_attempt1_brokenpairing.json`) and the aggregate margin clause
could never fire — the run was mathematically incapable of promoting. Fix: when stepping is on, the
loop re-measures the current incumbent on each boundary's fresh validation **and** confirmation
panel before any paired comparison (one extra 2×192-game evaluation per boundary); default mode is
untouched. The oracle gets comparability for free from its fixed per-run panels; Rust pays explicit
re-measurement instead. T4 relaunched with the fix under an unchanged pre-registered decision rule.

## Open questions for follow-up packages

- If T2 had shown drift at lr=0.01, the fix would have been a step-size schedule/decay plus
  keeping n≥32 boundary panels. It did not, so the priority moves to reward signal (below).
- The isolated-faction fallback is gated by the same aggregate margin clause in **both**
  implementations (T3 confirmed this on the Python side), so a single improved faction needs an
  implausibly large one-faction gain there; sequential per-faction training or a per-faction
  archive remains a structural option if joint training cannot clear the bar.

## Next experiments (revised after T3)

1. **T4: oracle-parity run** — resume @4600 from `out/stage2_from_stage1.json`, ~3500 updates,
   exact oracle configuration: `--every 50` (oracle eval cadence), `--accept-sigmas 0` (drop the
   extra σ clause to match the Python gate exactly), n=32 validation+confirmation panels, and
   `--panel-step ≥ 32` so each boundary's gain is an independent sample. Success criterion:
   at least one promotion by u≈3500, mirroring the oracle's sol@u3350 / xxcha@u3450. Failure
   under identical settings means the gap is implementation-level (game features or rollout
   behavior), not hyperparameters — escalate to a frontier-model differential diagnosis instead of
   tuning further. Cost ≈ 3500 × ~3.3 s/update ÷ parallelism ≈ 3–4 h.
2. More train seeds per update (e.g. 64 vs 16) to cut gradient variance — 4× cost, combine with
   less frequent boundary evaluation.
3. If both still show zero drift, the reward itself needs re-examination (e.g., outcome-only
   credits over the full horizon instead of per-decision centered returns).

Deprioritized by operator: `--rounds 8` ("plays too many rounds"); kept on file as a fallback if
T4 fails and game-length compression is implicated.

## Python retest (control run, completed 02:54 WEDT)

The operator redirected the investigation: instead of a Rust-bootstrap experiment, re-run the
Python pipeline itself from its own stage-1 champions with the latest Python stage-2 settings.
Launched ~01:27 WEDT (PID 65312 + ~30 worker processes), completed at u3550 in ~87 min wall,
`run_complete=True`. All outputs in this repo (`out/py_retest_stage2_pychamp.json`, logs, surrogate
snapshots); the oracle repo was verified byte-untouched via `git status` before and after.

Config = verbatim latest segment settings (seed 74000000, train_seeds 16, val/conf/audit seeds 32,
eval_every 50, lr 0.03 / entropy 0.01 / clip 1.0, gate tolerances 0.05/0.03/0.15/0.10, game_seconds
30, save52 pool), resumed from `stage1_pg_six_to5000_20260810.json` (u3050) with `--updates 500`.

### Reproduction verdict vs the original a->b->c chain

- Gate decisions identical at **9 of 10** boundaries: u3100 assembled (all six), u3150-u3300
  rejected, **u3350 isolated sol**, rest rejected. Single flip: the original promoted
  **xxcha @ u3450**; this run did not.
- Trajectory comparison (candidate metrics per boundary): **bit-identical through u3150**, then
  per-faction deltas of +/-0.06..0.22 emerge at u3200 and wander (sums -0.60..+0.28). Onset at one
  discrete point indicates run-to-run nondeterminism in the Python pipeline itself, most plausibly
  wall-clock `game_seconds=30` abandonment (a game over budget is dropped as stalled; which games
  exceed 30s depends on machine load) plus hash-seed/parallel-reduction ordering. The original chain
  ran under August load; this run ran idle at night.

### Forensics of the u3450 xxcha flip (out/forensic_xxcha_u3450.py, .json)

Reconstructed each run's exact isolated-xxcha swap table from surrogate snapshots
(accepted@u3450 = learner-table@u3100 + sol<-learner-sol@u3350; xxcha swapped in), measured both on
the identical validation panel:

| clause (limit) | original (promoted) | retest (rejected) |
|---|---|---|
| aggregate gain >= +0.300 | **+0.474** pass | **+0.016 FAIL** |
| worst faction VP regression >= -0.15 | -0.021 pass | **-0.172 (sol) FAIL** |
| worst clearance regression >= -3pp | -2.08pp pass | -2.08pp pass |

The retest's swap table had drifted enough by u3450 that xxcha's addition no longer moved the table:
total gain collapsed from +0.47 to +0.02 and sol tripped the VP veto. So the original promotion was a
comfortable call in its own run; the retest rejection is also comfortable in its own -- the flip came
from compounded profile drift, not from either decision being razor-thin against its own measurements.

### Start vs final (requested report tables)

Starting profile = stage-1 champion @u3050 (horizon-1 trained), measured on the identical 4-round
validation panel by out/eval_starting_profile.py -> out/py_retest_starting_panel.json:

| faction | start VP / clr | final ACCEPTED vp / clr (u3550) | dVP | dClr | final LEARNER vp / clr | learner dVP |
|---|---|---|---|---|---|---|
| sol    | 1.240 / 83.9% | 2.135 / 92.7% | +0.895 | +8.8pp  | 2.172 / 88.5% | +0.932 |
| letnev | 1.542 / 15.6% | 1.865 / 28.6% | +0.323 | +13.0pp | 2.161 / 34.4% | +0.619 |
| xxcha  | 1.380 / 69.3% | 1.901 / 79.7% | +0.521 | +10.4pp | 2.135 / 80.7% | +0.755 |
| hacan  | 1.411 / 67.7% | 1.958 / 79.2% | +0.547 | +11.5pp | 1.964 / 72.9% | +0.553 |
| jolnar | 1.198 / 60.9% | 1.932 / 68.2% | +0.734 | +7.3pp  | 2.276 / 65.6% | +1.078 |
| l1z1x  | 1.271 / 71.4% | 1.771 / 71.4% | +0.500 | +0.0pp  | 1.870 / 71.4% | +0.599 |
| SUM    | 8.042         | 11.562           | **+3.52** |       | 12.582          | **+4.54** |

Promotion timeline: u3100 assembled (all six) -> u3350 isolated sol. Original chain additionally had
xxcha @u3450 (see flip above).

### Interpretation

- Most of the stage-2 "progress" is a **horizon reorientation jump in the first 50 updates**: the
  starting profile was trained at horizon 1, so it is handicapped on full 4-round games; +2.9 total VP
  by u3100 (assembled promotion) does most of the visible work in both runs identically.
- After that, improvement is slow and gate decisions are sensitive to run-to-run noise: xxcha's u3450
  attempt flipped under timing/hash nondeterminism. The old system works, reproduces its main results,
  and its promotions were real -- but the pipeline is not bit-reproducible across runs (wall-clock
  game abandonment + parallel reduction order). Rust's deterministic engine should be cleaner here;
  verify whether any wall-clock abandonment exists in the Rust trainer.
- T4 comparability caveat: Rust T4 resumed from a stage-2 lineage resume point, i.e. already past its
  own reorientation jump, so "zero promotions over +2100 updates" is not directly comparable to
  Python's "+300/+400 from raw stage-1 champions". The decisive differential experiment now is: run
  the Rust stage-2 trainer **from this same Python stage-1 champion file** (schema-compat check first)
  with T4-equivalent settings and compare boundary-by-boundary against both Python runs.

Artifacts: out/py_retest_stage2_pychamp.json (+ _surrogate/), out/logs/py_retest_stage2.log,
out/py_retest_starting_panel.json, out/eval_starting_profile.py, out/forensic_xxcha_u3450.{py,json},
out/compare_trajectories.py.

## T5 differential experiment (Rust from Python champion) — pilot +50 updates, completed 08:10 WEDT

**Setup.** `stage2_training.exe` (HEAD incl. new opt-in training-seed-stream flags) resumed the oracle's own
stage-1 champion file (`D:/Projects/ti4-engine/out/stage1_pg_six_to5000_20260810.json`, u3050, schema 4 — loaded
directly, no conversion), trained +50 updates on the oracle's exact seed stream
(`--train-seed-base 74000000 --train-seed-stride 10000`), T4-equivalent settings (horizon 4, lr 0.03, entropy
0.01, clip 1.0, train seeds 16, fixed panels at validation-first-seed 83M / confirmation-first-seed 88M, n=32,
save52 pool, `--accept-sigmas 0` for Python gate parity). Output: `out/t5_pilot_u3100.json`; log
`out/logs/t5_pilot.log`.

**Pre-flight (`--eval-only`, same seeds):** Rust measured the starting table at sol 1.302/93.2%, letnev
1.594/70.8%, xxcha 1.141/81.8%, hacan 1.010/91.7%, jolnar 1.010/71.9%, l1z1x 1.005/92.2% (vp/clearance).
Standalone Python re-measure of the same table/seeds (`out/py_retest_starting_panel.json`) gave sol
1.240/83.9%, letnev 1.542/15.6%, xxcha 1.380/69.3%, hacan 1.411/67.7%, jolnar 1.198/60.9%, l1z1x
1.271/71.4% — already divergent (letnev clearance 70.8 vs 15.6).

**u3100 boundary comparison** (Rust pilot vs the two Python chains, which recorded bit-identical u3100):

| faction | Rust vp / clr    | Python vp / clr  | dv     | dclr      |
|---------|------------------|------------------|--------|-----------|
| sol     | 1.672 / 93.2%    | 1.536 / 90.6%    | +0.136 | +2.6pp    |
| letnev  | 1.953 / 75.5%    | 1.901 / 29.7%    | +0.052 | **+45.8pp** |
| xxcha   | 1.849 / 82.8%    | 1.943 / 81.3%    | -0.094 | +1.6pp    |
| hacan   | 1.714 / 91.2%    | 1.766 / 77.6%    | -0.052 | +13.6pp   |
| jolnar  | 1.370 / 68.2%    | 2.005 / 60.9%    | **-0.635** | +7.3pp |
| l1z1x   | 1.609 / 85.9%    | 1.818 / 74.0%    | -0.209 | +12.0pp   |

Sum VP 10.167 (Rust) vs 10.969 (Python). Panel SE at n=192 is ~0.05-0.08 VP / ~1.5-1.9pp, so jolnar and the
clearance gaps are many sigma — systematic per-faction play divergence, not noise.

**Gate decision matched:** both Rust and Python promoted the full assembled six-faction table at u3100.

**Ruled out:** head routing (Rust `head()` falls back to the schema-4 `other` head for split-successor heads;
the oracle's 4→5 migration is designed behaviour-preserving); metric definitions (both engines average own final
VP over non-stalled games and cleared-fraction); seed stream (banner confirmed base/stride match).

**Remaining suspects, in order:** faction-specific feature extraction differences; map/board generation or
map-selection-from-seed differences; sampling/RNG differences. Next decisive step: single-source-seed
per-decision diff between the engines to localize board-vs-scoring divergence.

**Code added for T5 (opt-in, default-preserving):** `FactionPlan.train_seed_stride` + example flags
`--train-seed-base`, `--train-seed-stride`; effective values recorded in checkpoint arguments; banner line.
98 crate tests pass; clippy clean; release binary rebuilt.
