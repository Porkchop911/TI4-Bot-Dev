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

## Open questions for follow-up packages

- If T2 had shown drift at lr=0.01, the fix would have been a step-size schedule/decay plus
  keeping n≥32 boundary panels. It did not, so the priority moves to reward signal (below).
- The isolated-faction fallback is gated by the same aggregate margin clause (0.30 across six
  factions), so a single improved faction needs an implausibly large one-faction gain to be
  promoted on that path; sequential per-faction training or a per-faction archive remains a
  structural option if joint training cannot clear the bar.

## Next experiments (proposed, in priority order)

1. **Longer horizon**: `--rounds 8` (knob exists since the safepoint) doubles game length and the
   VP spread between good and bad play; ~2× rollout cost. Directly attacks root cause #2.
2. More train seeds per update (e.g. 64 vs 16) to cut gradient variance — 4× cost, combine with
   less frequent boundary evaluation.
3. If both still show zero drift, the reward itself needs re-examination (e.g., outcome-only
   credits over the full horizon instead of per-decision centered returns).
