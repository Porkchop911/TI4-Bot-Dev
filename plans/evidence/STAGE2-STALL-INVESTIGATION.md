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

## T6 — single-game per-decision differential (root cause of cross-engine divergence)

**Method.** One game, seed 83000001 rotation 0, 4-round oracle horizon, map pool
`save52_e400_n8192.json.gz`, both engines in greedy mode (`--greedy-temperature 0.0001`) with full
per-option raw-feature dumps:

- Rust: new example `crates/ti4-training/examples/single_game_trace.rs` (TraceBot records faction,
  per-bot idx, prompt, option id/kind sets, scores, head routing, blind/seeing path; optional
  `--full-features` raw bucket dump and `--dump-head <head> --dump-out <path>` weight dump).
- Python: `out/diff_py_game.py` monkey-patches each bot's `_sample` at instance level (oracle repo
  byte-untouched, verified with git status before/after); `--table <key>` selects the checkpoint
  profile table.

**Finding 1 — the apparent idx0 score divergence was a harness artifact (wrong tables).**
Checkpoint `stage1_pg_six_to5000_20260810.json` (final_update=3050, run_complete=False) carries two
profile tables: `profiles` = last **accepted** champion snapshot; `learner_profiles` = **live learner**
weights at u3050. Oracle resume semantics (`train_stage1_policy_gradient.py:1247-1248`): accepted comes
from `profiles`, live training state from `learner_profiles`. Rust's production loader
(`stage2_training.rs load_start`) implements the same mapping (learner = `learner_profiles` → fallback
`profiles` → `accepted`; champion = `accepted` → fallback `profiles`). The first differential run used
`profiles` on the Python side but `learner_profiles` on the Rust side — two different u3050 snapshots.
All 80 strategy-head weights per faction differ between the tables (e.g. xxcha `option:pok2diplomacy`:
accepted +0.0946 vs learner +0.4819).

**Finding 2 — with identical tables, scoring is exact.** Re-running both sides on `learner_profiles`:

- All six factions' idx0 (strategy card choice): option sets identical (shared-pool draft order),
  raw feature vectors identical bucket-for-bucket (21 buckets/option), scores equal to full precision.
- Across every aligned common decision for all six factions: **max |score delta| = 0.000000**, zero
  choice mismatches, zero blind-path decisions in the Rust trace.
- Weights loaded by Rust (`--dump-head strategy`) are bit-equal to the file's `learner_profiles` table;
  head routing agrees (strategy_card → head "strategy" both sides); score formula is a plain dot product
  on both sides (Python `sum(weighted.values())`, Rust `Head::score_vector`).

**Finding 3 — residual divergence is in decision *surface labels*, not extraction.** Feature extractors
are faithful mirrors: Rust `explicit_option_features` ↔ Python `_NamedFeatureExtractor`; tokenizer
identical (`[a-z0-9]+` on lowercased text); for an equivalent decision (same prompt, same option ids,
same state) the feature vectors are byte-equal (`feature_diffs=0`). But each engine's game layer labels
equivalent situations differently, and those labels hash into feature names:

| surface | Python | Rust |
|---|---|---|
| player identity in prompts/options | faction name ("transaction with hacan", "reaction:l1z1x:...") | seat id ("transaction with seat3", "reaction:seat5:...") |
| option-id vocabulary, same prompt "spend 3 influence for a command token" | `no` / `yes` (48 occurrences) | `buy` / `decline` (24) |
| leadership secondary offer | repeated `no/yes` purchase prompts | `decline/follow` then purchase prompts |
| reaction events observed | GROUND_FORCE_COMMITTED, INVASION_STARTED, ACTION_CARD_PLAYED, SUSTAIN_DAMAGE_USED, ...after | SHIP_MOVED, PLAYER_PASSED, PRODUCTION_USED, SPACE_COMBAT_STARTED, PLANET_CONTROL_GAINED, ...After |

Consequence: shared checkpoints look up different weights for equivalent situations → systematically
different scores from the first labeled decision onward → cascading game divergence (py 1868 vs rust
1048 decisions; prompt-class counts differ by 3-5x on action phases and trades). This fully explains the
T5 pilot's systematic per-faction metric deltas: Rust training optimizes against its own surface while
the Python checkpoint weights were trained on Python's surface.

**Open item (rules-level, separate from scoring):** the reaction-event taxonomies differ in more than
naming — each side observed event classes the other never emitted. Needs a dedicated hook-set audit to
rule out missing/extra reaction windows before any parity claim about full game dynamics.

**Decision required (operator/frontier).** To make Rust stage-2 comparable to or resuming-from the
Python oracle chain, one canonical decision surface must be adopted: (a) align Rust game-layer labels to
the Python surface (faction names, `no/yes` ids, event naming) — restores cross-engine parity and direct
checkpoint reuse; (b) standardize on the Rust surface and retrain from scratch in Rust only — abandons
cross-comparability with existing Python artifacts; (c) canonicalize player identity at feature time in
both extractors — a representation change to both pipelines. This materially changes public training
behavior and is not decided by existing plans.

**Artifacts.** `out/rust_ff_83000001.json`, `out/py_ff_learn_83000001.json` (full-feature greedy traces),
`out/diff_decisions.py --at <faction> <idx>` (bucket-level feature diff), `out/rust_loaded_strategy.json`
(weight dump). Oracle repo unchanged.

## T6b — reaction-window taxonomy audit (read-only)

Both engines register action-card reaction windows in exactly one table keyed by the corpus's
printed window text (Python `engine/reactions.py WINDOWS`, Rust `reactions.rs window_table()`);
leaders/technology use separate standing-modifier mechanisms on both sides, verified by grep.
Script: `out/audit_reaction_windows.py`; tables dumped to `out/reaction_window_tables.json`.

**Counts:** Python 38 aliases; Rust 29.

| class | count | detail |
|---|---|---|
| missing in Rust (cards can never react) | 10 | windows on UNIT_DESTROYED (after ×1, when ×1), GROUND_FORCE_COMMITTED after, SUSTAIN_DAMAGE_USED (after + when), GROUND_DICE_ROLLED after, RETREAT_ANNOUNCED after, defender "Announce Retreats step" variant of COMBAT_ROUND_STARTED, HITS_ASSIGNING before-assignment ×2 |
| extra in Rust (reaction opportunity Python never offers) | 1 | "When 1 or more of your units use PRODUCTION" → PRODUCTION_USED |
| name-only remaps (fire at the same game point?) | 3 aliases / 4 rows | SPACE_COMBAT_ENDED↔SPACE_COMBAT_WON; INVASION_STARTED↔INVASION_BEGAN ×2 — emit-site parity still to verify in Phase 2 |
| **semantic divergence** | 1 | "When another player chooses a strategy card during the strategy phase": Python `Relation.WHEN` (fires before completion), Rust `After` (fires after resolution) — different observable/mutable state at the reaction point |

This audit explains every event-taxonomy difference observed in T6 traces and bounds Phase 2:
close 10 missing windows, remove or gate the 1 extra, align 3-4 event names, fix 1 WHEN/AFTER
relation. The WHEN/AFTER fix changes when a reaction fires relative to resolution — legality/API
boundary territory → frontier review tier per AGENTS.md before implementation.

**Phase plan for surface alignment (operator-approved option (a), 2026-08-15):**
- Phase 1: label/vocabulary alignment only (player identity in prompts/options, `no/yes` vs
  `buy/decline`, prompt phrasing). Mechanical; verified by re-running the T6 differential to full
  structural agreement.
- Phase 2: reaction-window set alignment per this audit (own package; WHEN/AFTER row gets frontier
  review first).

## Phase 1 — surface alignment: label inventory and package split

Inventory script `out/inventory_labels.py` (traces: py_ff_learn_83000001 vs rust_ff_83000001)
canonicalises player identity to `<p>` per side and groups prompts. Result: 77 prompt classes only in
Python, 37 only in Rust, 32 shared-text rows with differing option sets (mostly state-cascade after
early divergence — not surface bugs), 13 fully shared. The genuine label divergences, by class:

| id | class | Python surface | Rust surface |
|---|---|---|---|
| P1-a | trade offers | `"{faction} gives {offer} for {demand} -- accept?"`, ids (accept, counter, **refuse**) | `"seatN offers — accept?"` (no offer detail), ids (accept, counter, decline) |
| P1-b | influence/resource bidding | "pay N more influence" / "pay N more resources" | "pay N" |
| P1-c | ground-commit + ready/retreat surface | "commit ground forces in {sys}", ids `commit\|n\|planet` + done_committing; "ready a planet"; "let another player replenish commodities" (done, factions) | "commit ground forces", ids `land\|n\|planet` + decline; "ready an exhausted planet" (+decline); "grant free trade replenishment" (decline, seats) |
| P1-d | reaction option ids / prompt identity | `reaction:{faction}:{EVENT}:after` (lowercase relation), prompt "after {EVENT}" with faction id | `reaction:seatN:{EVENT}:After`, seat id |
| P1-e | speaker choice + seat-id prompts | "who becomes speaker", faction-name options; "signal jamming: whose token goes into N" (factions) | "choose the new speaker", seat ids; seats in other prompts |
| P1-f | misc wording | "--" hyphens ("over the hand limit -- discard one of 8"); leadership purchase loop structure to verify against Rust 'follow' gate | em-dash "—"; `pok1leadership secondary` (decline/follow) then buy/decline prompts |

Event-name remaps (INVASION_STARTED↔INVASION_BEGAN etc.) and the window-set gaps stay in Phase 2.
Each P1-x is its own branch + commit, verified by crate tests; final verification re-runs the T6
differential after all sub-packages land.

**P1-a scope (this package):** trade-offer prompt text + offer-detail formatting + refuse/decline
vocabulary in Rust `transactions.rs` to match Python's exact strings and id set.

## P1-a — trade-surface label alignment (implemented)

**Scope.** Rust `transactions.rs` (+ one call site in `game.rs`, wiring test): align the
decision surface of transactions to the Python oracle's strings, ids and payloads. No legality,
timing or state-semantics changes; no note-ID identity change (residual R1 stays for P1-a2);
no new option shapes (action-card trades stay out until P1-a3).

**Changes.**
- `Terms::describe()`: fixed order "N trade goods", "M commodities", "K relic fragments", raw
  promissory id; empty → "nothing" (oracle `Terms.describe`).
- `Offer::describe(state)`: "{faction} gives {given} for {received}" — faction name, not seat.
- Open option ids: `trade|{seat}` → `component|trade|{faction}` (matches the crate-wide
  `component|{source}|` convention and Python's player-is-faction identity). `opens_with`
  reverse-maps faction→seat via seating order; tables with duplicate faction names are outside
  the oracle's expressible games (its player *is* its faction) — deterministic first-seat
  resolution documented in code.
- Answer prompt: "{offer.describe()} -- accept?" (em dash → hyphen); options now exactly
  [accept, refuse, counter] with ids/kinds/labels matching Python (`refuse` carries
  DECLINE_KIND so decline routing is unchanged).
- Oracle `_priced` parity: every offer option and the accept option carry `net` / `their_net`
  (receiver-side worth − giver-side cost) via new `Terms::worth_to_receiver` / `cost_to_giver`,
  with per-note pricing (`NOTE_WORTH` table, support-prefix 4.0/3.0, trade-agreement alias).
  Prices use the existing flat note *transfer* rules elsewhere; only feature payloads are added.
- Prompt "transaction with {faction}" for the propose stage.

**Verification.**
- `cargo fmt -p ti4-engine` clean; `cargo test -p ti4-engine`: 756 + 5 doctests pass (was 752);
  new tests: terms formatting, open-id/faction naming + round-trip resolution, propose prompt,
  answer ids/kinds/order, net/their_net payloads. Clippy `--all-targets` clean.
- `cargo check --workspace --all-targets` clean; `cargo test -p ti4-training`: 98 pass.
- T6 differential re-run (seed 83000001 rot 0, greedy temp 0.0001, full features, same learner
  table both sides): rust now 1152 decisions (was 1048); **max_score_gap = 0.000000 on every
  faction's common prefix**; trade surface structurally identical:
  - open ids: `component|trade|{hacan,jolnar,l1z1x,letnev,sol,xxcha}` — same set both engines;
    kind `open_transaction` both.
  - propose prompts: "transaction with {faction}" both (py 297 / rust 131 such decisions).
  - answer prompts: "{faction} gives … for … -- accept?" format identical both.
  - answer ids: {accept, refuse, counter} both (rust previously had `decline`).
- Residuals at this seed are out of P1-a scope and expected: state-cascade option-set drift after
  first choice divergence; Rust's blind decline/follow secondary path where Python inlines the
  prompt (P1-b/P1-d + Phase 2 windows); note IDs still seat-keyed (R1 → P1-a2).

**Artifacts.** `out/rust_ff_83000001_p1a.json` (new rust trace), `out/trace_p1a_raw.txt`.

## P1-a2 — promissory-note identity alignment (implemented)

**Objective.** The trade surface now matches the oracle's labels for goods/commodities/fragments
(P1-a), but every note id Rust mints embeds a *seat* (`cf:seat3`, `support:seat3`) where the
oracle embeds the owner's *faction name* (`cf:hacan`, `support:hacan` — oracle player.id **is**
its faction). Note ids flow into option ids, labels, payloads and the state projection, so until
they match, every note-bearing decision bucket differs between engines.

### Gaps found while scoping (all verified against oracle source, read-only)

- **G1 — deal timing.** `seated()` in `rollout.rs` calls `start_game_seeded` (which deals notes
  via `setup.rs:128`) *before* assigning real factions to seats. Every Rust training/eval game
  therefore dealt only the four generic notes (`cf ps ta an`) — no faction notes at all, while
  the oracle's `promissory.deal` runs on fully-seated players and deals `FACTION_NOTES`
  (letnev→war_funding, hacan→convoys, jolnar→ra, sol→ms, l1z1x→ce, xxcha→favor; verified the
  Rust content corpus carries exactly these aliases). Fix: re-deal after faction assignment in
  `seated()` (idempotent — no note has moved yet at setup). This changes real-game state (5+
  notes/player) and is a parity restoration, recorded here rather than silently applied.
- **G2 — FACEUP set.** Oracle `FACEUP = {"an", "convoys"}`; Rust had `["an"]`. In Rust, received
  Trade Convoys never entered the play area, so `reaches_anyone` could never fire (dead card) and
  a held convoys note stayed re-offerable. Fix: add `"convoys"` to the const. This *requires* the
  faceup-exclusion below or Rust would newly offer notes Python refuses to.
- **G3 — `available_notes` filters.** Oracle excludes faceup notes ("already played, doing its
  work where it sits") and withholds Alliance until the owner's commander is unlocked (the note
  conveys nothing before then). Rust had neither filter. Fix: port both; needs `content`, so
  `ContentStore` is threaded through `available_notes` → `can_pay` → `why_illegal` →
  `TradeWindow::resolve`/`offer_from`.

### In scope (one atomic commit)

1. **Identity:** note and support ids embed the owner's faction name. Pure formatters take a
   name; new helpers `promissory::faction_name(state, player)` and `promissory::seat_of(state,
   name)` (first match in seating order — same deterministic rule as P1-a's `opens_with`).
   **Duplicate-faction caveat:** two seats sharing one faction mint colliding note ids; the
   earlier seat shadows the later. The oracle cannot express such a table (its player.id is its
   faction), so these remain Rust-only scaffolding — documented, not error-handled.
2. **G1 re-deal** in `rollout.rs::seated` after faction assignment.
3. **G2 FACEUP += convoys.**
4. **G3 filters** + content threading (mechanical signature updates only).
5. Tests: existing note tests moved to distinct-faction scaffolds (same pattern as the P1-a
   wiring fix); new tests for faction-keyed deal, re-deal in seated games, convoys faceup,
   faceup-exclusion and alliance gating.

### Out of scope (documented residuals)

- Per-note pn pricing + `:price` id suffix + price parsing → **P1-a3** (oracle prices
  `int(round(worth))` per note; Rust keeps flat NOTE_PRICE=2 for now). Labels/ids therefore still
  differ on ra/an/convoys/ta offers at this seed.
- `ac{}` action-card trade shape → P1-a3.
- Note events (PROMISSORY_RECEIVED/RETURNED, CEASEFIRE_USED…) and the traded-goods-for-note stat
  reconciliation → Phase 2 / separate package.
- **Observed, deferred:** `secrets.rs::holds_a_rivals_note` looks up the *full* note id in the
  content store (`content.get(PromissoryNotes, "cf:x")`) where records are keyed by alias — the
  faction comparison can never resolve for any id form. Pre-existing; affects one secret-objective
  scoring check, not the trade surface. Needs its own reviewed fix.

### Permission class / bounds (SCOPED_PERMISSIONS)

Local-repo writes only: `crates/ti4-engine/src/{promissory,transactions,game}.rs`,
`crates/ti4-training/src/rollout.rs`, tests within those files; evidence + execution state.
No network, no external processes beyond cargo; oracle repo read-only (already re-verified).

### Implementation summary (as scoped above)

- `promissory.rs`: `FACEUP = ["an", "convoys"]`; ids minted from faction names via new helpers
  `faction_name` / `seat_of` (first match in seating order); `deal()` keys by seat faction;
  `available_notes(state, content, player)` rewritten to oracle form — faceup exclusion, Alliance
  commander-unlock gate, and the *removal* of Rust's engine-local ownership filter (oracle parity
  G3b: holding is what makes a note offerable; a lent-out note stays re-offerable by its holder);
  `trade_agreement_worth` keys off the embedded name ("generic" → flat default).
- `transactions.rs`: `content: &ContentStore` threaded through `can_pay` / `why_illegal` /
  `resolve` / `TradeWindow::resolve`; `offer_from` takes `state` so the `ss` branch can mint
  faction-keyed support ids. Call sites verified contained to the crate (no external callers).
- `game.rs`: `step_trade` passes `self.content`.
- `rollout.rs`: re-deal after faction assignment in `seated()` (G1) — real games now deal the
  five-note hands the oracle deals; scaffolding games unchanged (idempotent at setup).
- Tests: note/support tests moved to distinct-faction scaffolds (a→hacan, b→jolnar + re-deal,
  same pattern as P1-a's wiring fix); new coverage for faction-keyed ids, re-deal in seated
  games, convoys faceup, faceup exclusion, Alliance gating, lent-out note re-offerability.
- Incidental hygiene (pre-existing warnings fixed so both crates stay clippy-clean at all-targets):
  duplicate `#[expect]` in `rollout.rs::play_with_deciders`, `.iter()` no-op, and six mechanical
  lints in the `single_game_trace` example. No behavior changes.

### Verification

- `cargo fmt -p ti4-engine -p ti4-training` clean; `cargo test -p ti4-engine`: **758 lib + 5
  doctests pass** (was 756+5); `cargo test -p ti4-training`: **98 pass**; `cargo clippy -p
  ti4-engine -p ti4-training --all-targets`: zero warnings; `cargo check --workspace
  --all-targets` clean.
- T6 differential re-run (same harness as P1-a: checkpoint
  `D:/Projects/ti4-engine/out/stage1_pg_six_to5000_20260810.json`, seed 83000001 rot 0, rounds 4,
  greedy temp 0.0001, --full-features, map pool save52_e400_n8192):

  **Protocol fix (recorded for all future diffs):** the Python trace must be run with
  `--table learner_profiles`. Rust's `single_game_trace` loads the *learner* table first; the
  script's default (`profiles` = accepted/champion) scores every decision differently, which
  initially masqueraded as a P1-a2 regression (hacan idx0 structural mismatch, score gaps up to
  ~3.9 on "common" decisions). The confusion was fully investigated and closed: the oracle is
  deterministic in-process and across processes (`PYTHONHASHSEED` has no effect; `six_player_game`
  uses a fixed board, not the map pool) — only the table flag differed.

- With the correct table (rust 1136 decisions / py 1868):
  - **max_score_gap = 0.000000 on every faction's common prefix; zero choice mismatches** — the
    identity change is scoring-neutral exactly as intended.
  - Residual structural divergences are all in previously-logged classes:
    - `component|leader|hacanagent` / `xxchaagent` present in Python action-phase options, absent
      from Rust (**finding F1 below**).
    - Rust blind `decline`/`follow` secondaries where Python inlines the prompt (P1-b/P1-d +
      Phase 2 reaction windows). One diff-script artifact noted: its `prompt_text_mismatches`
      counter also counts the first structurally-divergent decision itself; verified not a real
      prompt-text divergence within any common prefix.
- Rust-vs-Rust (P1-a trace vs P1-a2 trace): zero score/choice mismatches on every common prefix;
  first divergences are exactly the intended surface changes — e.g. hacan idx48 "transaction with
  jolnar": old build offered `pnan:seat3` (seat-keyed Alliance note), new build offers
  `pncf:hacan` (faction-keyed; `an` now gated behind commander unlock, which Rust rollouts never
  reach — see F1). Later per-faction divergences (note ids in offer tails at jolnar@81,
  letnev@79, sol@103) are the seat→faction id rename; a combat-reaction mismatch at l1z1x@92 /
  xxcha@65 is cascade from the earlier fork, not an unintended surface change (verified: all
  preceding decisions identical on both builds).
- Oracle repo re-verified unchanged after the run (`git -C D:/Projects/ti4-engine status --short`
  shows only the pre-existing untracked `docs/POLICY_GRADIENT_HANDOVER.md`; commit still 37061c5).

### Finding F1 — Rust rollouts deploy no leaders (Phase 2 gap, confirmed by differential)

`crates/ti4-engine/src/leaders.rs` is complete (`deploy`, `check_unlocks`) and `game.rs:1683`
calls `check_unlocks` every turn, but `leaders::deploy` is only called from tests — never in real
games or rollouts (Python calls `_leaders_mod.arm(game)` at creation). Consequence: Rust seats
start with empty leader maps, commanders never unlock, so the new Alliance gate permanently
withholds `an` notes from Rust learners while Python champions offer them from round 3+; and
`component|leader|{faction}agent` options are absent from every Rust action phase (confirmed in
the differential above: present for hacan/xxcha in Python, missing in Rust). Fixing this adds a
whole option class (Commander Agent actions) to the surface — Phase 2 scope, not P1-a2.

### Artifacts

`out/rust_ff_83000001_p1a2.json`, `out/py_ff_learn_83000001_p1a2.json` (learner table),
`out/trace_p1a2_rust.log`, `out/trace_p1a2_py.log`. Determinism probes kept for the record:
`out/hashprobe.py`, `out/double_run.py`.

## P1-a3 — per-note trade pricing (implemented)

**Objective.** Every note Rust mints into an offer id is flat-priced at `NOTE_PRICE = 2`
(`pncf:hacan`, "sell cf:hacan for 2 trade goods") where the oracle prices each note by its own
worth: option id `pn{note}:{price}` with `price = int(round(_note_worth(note)))` — and that call
passes **no game**, so a Trade Agreement takes the flat `WORTH["ta"] = 2.5`, not its live value,
for pricing purposes (live worth still flows through `_priced`'s net/their_net in both engines;
verified formulas identical). Labels: "sell {note} for {price} trade goods".

**Prices the oracle actually produces** (WORTH verified line-by-line against Rust's NOTE_WORTH —
identical table): ra→4, an/convoys→3, ta→2.5→**2**, ce/ms/favor/war_funding→2, ps/cf→1.5→2. The
two `.5` values are exact banker's-rounding cases: Python `round(1.5) = round(2.5) = 2` (half to
even), while Rust `f64::round` is half-away-from-zero (`3`). A naive port would price `ta` at 3
and every ps/cf sale identically — the ta divergence changes both option ids and what the deal
asks for, so it must be reproduced exactly: helper `py_round_half_even`.

**Scope (one atomic commit).**
1. `note_option_price(note)` in transactions.rs: no-game worth (support→4.0 for completeness,
   ta→flat 2.5 row, else NOTE_WORTH row or 1.5 default) → half-to-even integer price.
2. offer_options pn branch: guard becomes `price > 0 && their_goods >= price`; id
   `pn{note}:{price}`; label with the live price. (Payload {note, alias} unchanged.)
3. offer_from pn branch: parse `rpartition(':')` → note + price; unrecognised forms return None
   (oracle would raise on a bare `pnfoo`; Rust's Option form is the safe equivalent).
4. Delete `NOTE_PRICE` (all three use sites replaced); keep NOTE_WORTH/note_worth/note_cost as-is.
5. Tests: per-note prices incl. the banker's cases (ta→2, cf/ps→2 via 1.5, ra→4, an/convoys→3),
   partner-affordability guard at the live price, id parse round-trip, malformed → None; update
   existing note-offer tests whose expected ids gain `:2`.

**Out of scope / reclassified.**
- ~~`ac{}` action-card trades are oracle-inert.~~ — **RETRACTED, corrected below in the
  post-commit audit.** The original claim read only the declaration line
  (`TRADES_ACTION_CARDS: set[str] = set()` in `faction_abilities/__init__.py`) and missed
  `engine/faction_abilities/hacan.py:48`, which runs `fa.TRADES_ACTION_CARDS.add(ARBITERS)` at
  import time; the content corpus then makes it live, because hacan's record carries
  `abilities: ["master_of_trade", "guild_ships", "arbiters"]`. Action-card trades are a real,
  firing oracle surface for any table containing Hacan — see **F3** below. No retraction of code:
  P1-a3 shipped only pricing, which is unaffected.
- **F2 (Phase 2 gap):** the oracle's 81.3 status-phase draw (one action card per player in
  initiative order, +1 with Neural Motivator) is never called from Rust's `finish_status_phase`;
  Rust draws only via exploration/strategy-card effects. Consequence: seat hands diverge over a
  game and "play an action card" actions appear in Python action phases but not Rust ones — the
  same wired-but-never-called class as F1 (leaders). Out of P1-a3; needs its own reviewed package.

**Permission class / bounds.** Local writes: `crates/ti4-engine/src/transactions.rs` (+ tests),
evidence, execution state. No network; oracle repo read-only (re-verified clean this session).

**Implementation.** `crates/ti4-engine/src/transactions.rs` only:
- `note_option_price(note)`: no-game worth (support→4.0, else NOTE_WORTH row by alias or 1.5
  default — the ta row is flat 2.5 because there is no game argument), rounded with
  `py_round_half_even`, which reproduces Python's banker's rounding on exact halves
  (`round(2.5) = 2`; Rust's half-away-from-zero would price a Trade Agreement at 3).
- offer_options pn branch: per-note guard `price > 0 && their_goods >= price`, id
  `pn{note}:{price}`, label "sell {note} for {price} trade goods". Payload unchanged.
- offer_from pn branch: price parsed from the *last* colon (the note itself is alias:faction);
  an unpriced legacy form parses to no deal instead of inventing a price (oracle would raise).
- `NOTE_PRICE` deleted; all three use sites replaced. `_priced` net/their_net formulas were
  verified identical to the oracle before and after — live worth (incl. per-owner TA) still flows
  through them, only ids/labels use the flat-row price.

**Tests.** Four new tests + one updated: `note_option_prices_follow_the_oracle_table` (all ten
aliases incl. both banker's cases and the support row), `note_sales_carry_their_own_price_in_id_and_label`
(exact id set in BTreeMap order + label text), `note_sales_require_the_partner_to_afford_the_live_price`
(ra at 4 with a 3-TG partner: absent; at 4: present — the case a flat price got wrong),
`a_priced_note_id_parses_back_into_the_same_deal` (round-trip + unpriced form → None).

**Verification.** fmt clean; ti4-engine 762 lib + 5 doctests pass (+4 new); ti4-training 98 pass;
clippy all-targets zero warnings on both crates; workspace check clean.

**T6 differential (seed 83000001 rot 0, rounds 4, temp 0.0001, full features, checkpoint
stage1_pg_six_to5000_20260810). Protocol incident found and closed first:** the initial Rust run
passed `--greedy` where the example's flag is `--greedy-temperature`; unknown flags are silently
ignored, so that trace ran at native profile temperatures (turn head 1.0) and was not comparable —
detected from the per-decision `head.temperature` metadata recorded by the trace. Protocol note:
always verify a new trace's temperature before diffing. The corrected run is on record:

- py-vs-rust (`out/py_ff_learn_83000001_p1a2.json`, table learner_profiles, vs
  `out/rust_ff_83000001_p1a3.json`): **max_score_gap = 0.000000 and 0 choice mismatches within the
  common prefix for all six factions** (hacan/xxcha break at idx=1 on F1 leader components;
  jolnar/l1z1x/letnev on Rust blind decline/follow secondary windows — P1-b/Phase-2; sol's one
  prompt mismatch is the known first-divergence counting artifact).
- rust-vs-rust p1a2→p1a3: identical scores and choices through every common prefix; first intended
  forks are exactly the priced note ids (hacan@48 `pncf:hacan` → `pncf:hacan:2`; jolnar@81, letnev@79
  cascade from trade states); l1z1x identical through its whole common run.
- Note-id vocabulary: all 16 distinct priced ids Rust mints in this game are a subset of the
  oracle's 26; every price matches the oracle table exactly (cf/ps/ta/ms/ce/favor/war_funding→2,
  ra→4, convoys/an→3 — ta at 2 from the flat row, never its live value). py-only ids are all
  explained: l1z1x/xxcha offer states unreached due to the idx=1 cascade forks (F1/windows), and
  `pnan:*` notes which Python can offer only after commander unlocks that Rust rollouts lack (F1).

**Post-commit audit (operator prompt: "action card trades are a Hacan ability") — F3 recorded.**
Re-inspected the oracle read-only and found the "oracle-inert" claim above was wrong on two
independent grounds:
- `hacan.py:48` adds `"arbiters"` to `TRADES_ACTION_CARDS` at import; `has()` resolves it through
  the faction record's `abilities`, which for alias `hacan` includes `"arbiters"`. So
  `trades_action_cards(game, hacan_seat)` is **true**.
- Mechanics (transactions.py): legality gate 94.3 accepts a card-bearing offer iff *either* party
  has Arbiters and each side holds the card it offers; proposal adds exactly one option when either
  party has the ability, the proposer holds cards, and the partner holds ≥1 trade good:
  id `ac{card}:1` with `card = sorted(hand)[0]`, label "sell the action card {card} for 1 trade
  good", payload `{"action_card": card}`. Settlement transfers the card on both sides (Terms
  already carries it in the oracle).
- **Measured frequency:** in the T6 game itself (`out/py_ff_learn_83000001_p1a2.json`) ac options
  appeared **19 times** across l1z1x/xxcha/letnev/sol trade windows (all Hacan partners) and were
  **chosen 0 times** — a structural-surface divergence that reshapes option sets but did not steer
  this game's outcomes.
- Rust state: `Terms` has no `action_card` field; no parse, gate, or proposal exists. Oracle source
  comment (kept verbatim in the audit trail): before this option shape was added, "Hacan …
  finished last of six with 1.30 points and no wins in thirty games" — the oracle authors added it
  precisely because Hacan's trade identity never fired.
- **F3 (Phase-1 surface gap):** implement the ac{} shape — `Terms.action_card`, 94.3 legality,
  proposal option, parse/settlement — as sub-package **P1-a4**. Interaction with F2: Rust hands
  stay thin without the status-phase draw, so P1-a4 alone narrows but does not close the gap;
  full parity needs both (F2 is a game-flow change and gets its own reviewed package).

**Residuals.** F2 (status-phase action-card draw never called in Rust) and F3 (ac{} trade shape)
as above. Next Phase-1 sub-packages: P1-a4 (action-card trades, per the operator's correction) or
P1-b (`no`/`yes` answer vocabulary + blind secondaries); ordering to be confirmed.
