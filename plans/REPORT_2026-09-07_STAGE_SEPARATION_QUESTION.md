# Does the Stage-1 / Stage-2 split still earn its cost?

Report for external review. Repo: `ti4-engine-rs`, commit `c7c4e23`, 2026-09-07.
Every number below is measured, and names the log it came from.

---

## 0. What the two stages are

This project trains an MLP policy to play Twilight Imperium 4 through a PPO self-play loop.
Training has been split into two stages since the beginning.

**Stage 1 — "the opening".** One round of play (`Horizon::opening()`, `rounds: 1`). The objective
is a dense, near-noise-free gate on round-one facts: *gain 3 planets, hold 3 systems, gain the
opening unit composition (2 capacity ships + 3 ground forces)*. Reward is a potential difference
over exactly those components, each capped at its bar, plus a large `clear_bonus` (22.0) for
crossing it. The stated rationale (`crates/ti4-training/src/reward.rs:8-13`) is that round-4
victory points have sigma ~= 1.4 per player-game and are mostly interaction, so from blank weights
they are very nearly pure noise — a search against them selects on luck long before it selects on
play.

**Stage 2 — "the points".** Four rounds. Reward is `vp 1 | objective 0.35 | secret 0.25`, plus
opening shaping kept deliberately small: `r1_bonus 3`, `r1_shaping 0.1`. The shaping applies **only
to transitions with both ends inside round one**, and is a tenth of stage-1 magnitude specifically
so that "Stage 2 does not quietly become Stage 1 again"
(`crates/ti4-training/src/reward.rs:26-28`).

The intended workflow has always been: train stage 1 to a high opening bar, then warm-start stage 2
from it.

**The question: is that split still justified?** A stage-2 run from blank weights just produced a
better stage-1 policy than the dedicated stage-1 pipeline did.

---

## 1. Measurement conventions (please hold these fixed when reasoning)

Two acceptance measurements, both **greedy** at `--temperature 0.001`, 600 seeds x 6 rotations =
**21,600 seat-games**:

| metric | tool | pool |
|---|---|---|
| **clearance** — fraction of seat-games crossing the round-one bar | `clearance_eval` | Validation (`out/pools/full_np8_12_holdout.json`) |
| **waste** — per-faction clear / any-waste / tactical-per-seat | `build_positive_corpus` | Train |

Two waste numbers that must never be conflated:

- **any-waste** — *incidence*: fraction of seat-games with **at least one** wasted activation.
- **waste/tactical** — *rate*: fraction of individual tactical actions that were wasted.

A third, **`clear+zero`**, is the joint metric: fraction of seat-games that both cleared the bar and
wasted nothing. It is the one that actually matters and the one this report leans on.

**`tactical/seat` is the mandatory diagnostic.** A waste penalty has a known degenerate solution:
stop taking tactical actions at all. Any waste improvement must be read next to this number.

**In-training report tables are NOT acceptance evidence.** They are self-play at the acting
temperature (2.5 here), on the training pool, at the training horizon. Section 4 shows exactly how
badly they mislead.

**Noise floor**, from three identical-recipe replicates (`plans/STAGE2_TRANSFERABLE_LESSONS.md`):
table-level clearance **1.54 pp** spread (sigma ~= 0.80 pp); per-faction up to **5.39 pp**. Do not
treat a 2 pp table difference from a single replicate as a real effect.

---

## 2. Where stage 1 stands after a long, dedicated effort

Context: an observation-contract rework moved bundles to schema 7. `ti4_mlp::bundle::read` hard-
requires `schema == 7` with no migration path, so all 12 previous champions are permanently
unloadable and everything had to be retrained from blank.

Requested target: **>= 95% clearance and <= 0.5% any-waste, jointly.**

Every stage-1 arm measured at the acceptance convention:

| arm | updates | clearance (Validation) | any-waste | waste/tactical | tactical/seat | clear+zero |
|---|---:|---:|---:|---:|---:|---:|
| `blank-waste-mine-p5/checkpoint-59540` — best asset, **old** reward | 1600 | **93.40% +-0.33** | 2.31% | 0.008 | 3.000 | **91.36%** |
| `blank-corrected-p5/checkpoint-56984` — **corrected** reward | 1600 | 90.25% +-0.40 | 3.55% | 0.012 | 3.000 | 88.22% |
| `blank-corrected-p5/checkpoint-74168` — same arm, run out | 2050 | 89.02% +-0.42 | 2.60% | 0.009 | 2.997 | 87.17% |
| hybrid `control-s1` u25 | 25 | 92.28% +-0.36 | 4.28% | 0.014 | 3.000 | 89.06% |
| hybrid `control-s1` u50 | 50 | 91.65% +-0.37 | 4.78% | 0.016 | 3.000 | 87.81% |
| hybrid `control-s1` u75 | 75 | 90.78% +-0.39 | 6.78% | 0.023 | 3.000 | 86.14% |
| hybrid `control-s1` u100 | 100 | 81.66% +-0.52 | 9.19% | 0.031 | 3.000 | 77.98% |
| hybrid `aux-s1` u25 | 25 | 91.80% +-0.37 | 2.62% | 0.009 | 3.000 | 89.66% |
| hybrid `aux-s1` u50 | 50 | 93.77% +-0.32 | 5.98% | 0.020 | 2.999 | 88.49% |
| hybrid `aux-s1` u75 | 75 | 91.41% +-0.37 | 2.51% | 0.008 | 2.999 | 89.97% |
| hybrid `aux-s1` u100 | 100 | 90.22% +-0.40 | 4.30% | 0.014 | 2.997 | 87.25% |
| hybrid `correct-aux-s1` u25 | 25 | 92.99% +-0.34 | 2.55% | 0.008 | 3.000 | 91.14% |

**The target was never hit.** Best clearance ever recorded on this architecture is 94.97% (a
schema-6 champion, now unloadable); best waste separately is 1.46% at 93.23% clearance. Nothing
reached 95% and 0.5% together. Ceiling behaviour: clearance saturates around 93-94% and any-waste
refuses to go below ~2.3%.

**Note one structural fact in that table: `tactical/seat` is 3.000 in every single stage-1 arm** —
including the badly-regressed 81.66% one. In the one-round horizon a seat spends all three tactic
tokens, always. The degenerate "reduce waste by acting less" escape hatch is effectively closed in
stage 1: waste there is purely a *targeting-quality* problem. Remember this for section 5.

Failed stage-1 approaches, for completeness:

1. Plain PPO from blank, then behavior cloning -> BC lowered waste, plateaued clearance at 89-90%.
2. Large waste penalty retrofitted onto an already-trained policy -> waste excellent, clearance
   collapsed (severe Letnev failure).
3. Fixed-penalty PPO continued past the ~1600-update sweet spot -> clearance *and* waste regressed.
   Reproduced again this week: the corrected-reward arm went 90.25% at u1600 to 89.02% at u2050.
4. BC on 18,750 greedy clearing zero-waste demonstrations from the best waste-aware policy ->
   cross-entropy improved monotonically (0.8044 -> 0.7719) while clearance fell 90.76 -> 89.73% and
   any-waste rose 5.08 -> 8.15% over 7 epochs, monotonically, every epoch. Classic distribution
   shift; abandoned.

### 2b. One confound you need, because it affects section 4

The stage-1 potential's unit term was corrected mid-effort. The change is still uncommitted
(`git diff crates/ti4-training/src/reward.rs`, +53/-23):

```rust
// before: a dense proxy paying for the FIRST gained unit and no more
let units = capped(progress.units_gained, /* UNIT_SHAPING_CAP */ 1);
... + unit_weight * units + ...

// after: pays for the real composition bar
let capacity_ships = capped(progress.capacity_ships, requirement.capacity_ships); // cap 2
let infantry       = capped(progress.infantry,       requirement.infantry);       // cap 3
... + unit_weight * (capacity_ships + infantry) + ...
```

With `expansion_weight 2.0`, `unit_weight 1.0`, the maximum potential moves from
`2.0*(3+3) + 1.0*1 = 13.0` to `2.0*(3+3) + 1.0*(2+3) = 17.0`. **The unit term's share of maximum
potential goes from 1/13 to 5/17** — the shaping now pays roughly five times more for production,
relative to expansion, than it used to.

Isolating control (identical recipe, penalty 5, only the reward differs):
**93.40% -> 90.25% clearance, 2.31% -> 3.55% any-waste.** About 3.2 pp clearance cost, roughly 2x
the noise floor, but **a single replicate** — not yet replicated across seeds.

**The binary used for every run in sections 3 and 4 was built at 08:47 on 2026-09-06, after the
reward edit at 08:44.** So every number below uses the corrected reward.

---

## 3. The new run: Stage 2 from blank weights, 2000 updates

Deliberately naive: no stage-1 warm start, no curriculum, waste penalty on from update zero.

```
ppo_update --bundle out/checkpoints/blank-fa3d6f9-shared \
  --stage 2 --rounds 4 \
  --temperature 2.5 --movement-entropy 0.05 --entropy-final 1 \
  --learning-rate 3e-4 --waste-penalty 5 \
  --seed-base 2100000000 --updates 2000 --report-every 100 \
  --device cuda --out out/checkpoints/stage2-blank-p5
```

Header as run: `reward Two (victory points) | 4 round(s) per game`;
`opening r1 bonus 3 | r1 shaping 0.1 | clearance 0 | high-VP 0`;
`points vp 1 | objective 0.35 | secret 0.25`; waste penalty 5 on all six factions;
PPO clip 0.2, 4 epochs, minibatch 4096, value 0.5, entropy 0.01/0.1 (movement 0.05);
16 seeds x 6 rotations = 96 games/update. About 13.8 s/update, ~7.7 h wall clock. Ran to completion.

### 3a. In-training self-play (T = 2.5, training pool, 4-round games) — NOT acceptance evidence

| upd | clearance | mean VP | table VP total |
|---:|---:|---:|---:|
| 100 | 10.74% | 1.220 | ~7 |
| 200 | 55.54% | 2.208 | ~13 |
| 300 | 61.41% | 2.398 | ~14 |
| 400 | 64.31% | 2.589 | ~16 |
| 500 | 70.28% | 2.755 | ~17 |
| 600 | 74.27% | 2.881 | ~17 |
| 700 | 79.24% | 2.911 | ~17 |
| 800 | 80.15% | 2.904 | ~17 |
| 900 | 82.62% | 2.980 | ~18 |
| 1000 | 83.05% | 2.992 | ~18 |
| 1100 | 83.17% | 2.944 | ~18 |
| 1200 | 82.84% | 2.981 | ~18 |
| 1300 | 82.31% | 3.005 | ~18 |
| 1400 | 82.15% | 2.961 | ~18 |
| 1500 | 82.58% | 2.992 | ~18 |
| 1600 | 81.53% | 3.042 | ~18 |
| 1700 | 82.55% | 3.060 | ~18 |
| 1800 | 84.49% | 3.014 | ~18 |
| 1900 | 83.87% | 3.042 | ~18 |
| 2000 | **84.17%** | **3.069** | ~18 |

VP is genuine movement here, not a zero-sum artifact: the pool becomes actually contended only
around ~25 table points, and this run tops out near 18. It was still climbing at u2000
(3.014 -> 3.042 -> 3.069), i.e. **not converged**.

Read naively, that table says "stage 2 reaches only 84% clearance, ~9 pp worse than dedicated
stage 1." **That reading is wrong.** See section 4.

### 3b. The waste penalty caused a total activation shutdown, then the policy climbed out

`tactical/seat` over the run:

| update | 0 | 5 | 10 | 20 | **23** | 40 | 60 | 80 | 100 | 200 | 395 | 2000 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| tactical/seat | 4.733 | 1.892 | 0.375 | 0.095 | **0.076** | 0.448 | 0.795 | 2.097 | 2.293 | 3.238 | 5.043 | ~6.2-6.5 |
| waste/tactical | 0.821 | 0.528 | 0.194 | — | — | — | — | — | 0.031 | 0.013 | — | 0.004-0.010 |

The penalty found the degenerate solution within ~23 updates and drove tactical actions to
essentially zero. The VP term then dragged the policy back out, but **it took roughly 400 updates
(~1.5 h of the 7.7 h budget) to return to normal activity levels.** By the end, `tactical/seat` is
6.2-6.5 over four rounds with `waste/tactical` 0.004-0.010 — acting *more* and wasting *less*. The
non-degenerate outcome, reached the expensive way.

This failure mode is available in stage 2 (passing is an option) and effectively unavailable in
stage 1 (`tactical/seat` pinned at 3.000 across all twelve measured arms).

---

## 4. The result that prompts the question

`out/checkpoints/stage2-blank-p5/checkpoint-240504` — the stage-2 policy, 2000 updates — measured at
the **stage-1 acceptance convention**, against the best dedicated stage-1 assets:

|  | stage-2 from blank, u2000 | stage-1 champion `59540` | stage-1 corrected-reward `56984` |
|---|---:|---:|---:|
| trained on | VP, 4 rounds, **no stage-1 warm start** | opening gate, 1 round | opening gate, 1 round |
| reward version | corrected | **old** | corrected |
| clearance (Validation, greedy) | **91.20% +-0.38** | 93.40% +-0.33 | 90.25% +-0.40 |
| any-waste (Train, greedy) | **0.54%** | 2.31% | 3.55% |
| waste/tactical | **0.002** | 0.008 | 0.012 |
| tactical/seat | 2.788 | 3.000 | 3.000 |
| **clear+zero (joint)** | **91.44%** | 91.36% | 88.22% |

Per faction:

| faction | s2-blank clearance | s2-blank any-waste | 59540 clearance | 59540 any-waste |
|---|---:|---:|---:|---:|
| hacan | 87.47% +-1.08 | 1.08% | 92.22% | 1.78% |
| jolnar | 90.44% +-0.96 | 0.47% | 89.47% | 0.56% |
| l1z1x | 94.69% +-0.73 | 0.53% | 96.86% | 1.44% |
| letnev | 93.83% +-0.79 | 0.17% | 93.39% | 0.64% |
| sol | 86.97% +-1.10 | 0.86% | 92.81% | **7.17%** |
| xxcha | 93.78% +-0.79 | 0.11% | 94.83% | 2.28% |
| **table** | **91.20% +-0.38** | **0.54%** | **93.40%** | **2.31%** |

Four things stand out.

1. **The in-training table understated greedy clearance by 7 pp** (84.17% at T=2.5 vs 91.20%
   greedy). It also got the *per-faction ordering wrong*: in-training, Jol-Nar (78.28%) and Xxcha
   (78.78%) looked like the laggards; greedy they are 90.44% and 93.78%, and the real laggards are
   Sol (86.97%) and Hacan (87.47%).
2. **Under the same corrected reward, the stage-2 run beat the dedicated stage-1 run at the
   stage-1 metric**: 91.20% vs 90.25% clearance, 0.54% vs 3.55% any-waste. The like-for-like
   comparison favours *not* separating.
3. **any-waste 0.54% is effectively the 0.5% target** — a number the dedicated stage-1 pipeline
   never approached in any arm (best ever 1.46%; the champion 2.31%). Sol in particular went from
   7.17% to 0.86%.
4. **On the joint metric the two are indistinguishable**: 91.44% vs 91.36%. The stage-2 policy
   trades ~2 pp of clearance for ~1.8 pp of waste and lands in the same place — but it gets there
   with 4x less waste, which is the axis that was stuck.

**Honest caveat on point 3.** `tactical/seat` is 2.788, *below* the 3.000 that every stage-1 arm
shows, so the stage-2 policy does take ~7% fewer tactical actions in the round-one horizon. The
mandatory diagnostic is mildly triggered. Arithmetic against the degenerate explanation: removing
0.212 actions per seat at the champion's waste rate (0.008) would account for only ~0.0017 of
waste/seat, while observed waste/seat fell 0.023 -> 0.005, a drop of 0.018 — an order of magnitude
more than reduced activity alone can explain. So the policy appears to be preferentially dropping
the *wasteful* actions rather than acting less across the board, and clearance is preserved either
way. This is arithmetic, not a controlled experiment.

**Other caveats.** Single replicate on each arm; the noise floor is 1.54 pp at table level, so the
91.20 vs 93.40 gap (2.2 pp) is only ~1.4x the floor and the 91.20 vs 90.25 gap (0.95 pp) is *inside*
it. The runs are not compute-matched: the stage-2 run had 2000 updates against the stage-1 arms'
1600, and ~4x the per-game rollout cost (4 rounds vs 1), consuming ~7.7 h against ~2 h.

---

## 5. The question

**Given the above, does it still make sense to keep Stage 1 as a separate training stage?**

Concretely, please weigh in on:

1. **Does the split still pay for itself?** The original argument was that round-4 VP is too noisy
   to learn from at blank weights. This run refutes that empirically at n=1: a stage-2 run *from
   blank* learned both VP (1.220 -> 3.069, still climbing) and a better-than-dedicated opening. Is
   one result enough to retire the split, or does the noise argument still hold in a regime this
   run never reached — longer horizons, stronger opponents, more of the VP pool contested?

2. **How much of the win is `r1_shaping 0.1` doing?** The stage-2 reward still carries opening
   shaping (`r1_bonus 3`, `r1_shaping 0.1`, applied only to intra-round-one transitions). So this is
   not "stage 2 alone" — it is "stage 2 with a tenth-strength stage-1 term always on". Is the real
   finding that **shaping-always-on beats stage-separation**, i.e. that a curriculum was the wrong
   mechanism for something a shaping term does better? If so, is the right next move to sweep
   `r1_shaping` upward (0.1 -> 0.2 -> 0.4) inside a stage-2 run rather than to keep a stage-1 phase
   at all?

3. **The waste-penalty shutdown.** Stage 2 paid ~400 updates (~20% of the run) climbing out of a
   near-total activation collapse (`tactical/seat` 4.733 -> 0.076 -> recovery). Stage 1
   structurally cannot suffer this, since `tactical/seat` sits at exactly 3.000 across all twelve
   arms. Two readings:
   - *keep stage 1* as the safe venue for waste shaping, then warm-start stage 2; or
   - *drop stage 1* and fix the collapse directly — penalty ramp/warm-up, or a Lagrangian/CMDP
     formulation where the waste constraint's multiplier is learned rather than fixed at 5.

   Which is the better use of the compute? If the Lagrangian, what constraint level and dual
   learning rate would you set, given any-waste is already at 0.54%?

4. **The corrected reward.** The unit-shaping fix (section 2b) costs ~3.2 pp of stage-1 clearance at
   n=1, moving the unit term from 1/13 to 5/17 of maximum potential. But the *same* corrected reward
   is what the stage-2 run used, and it reached 91.20%. Does that suggest the correction is fine and
   the old reward's 93.40% was partly a stage-1-specific overfit — i.e. the old shaping bought
   clearance by underweighting production in a way that only pays inside a one-round horizon?
   Revert, retune (`unit_weight` below 1.0 to restore the old ratio), or keep it?

5. **Acceptance by faction.** The table mean hides real spread: Sol 86.97% and Hacan 87.47% against
   L1Z1X 94.69%. Should selection gate on the worst faction rather than the table mean, and if so at
   what level, given the per-faction noise floor is up to 5.39 pp?

6. **What is the single cheapest experiment that settles the split?** Our candidate: run stage 2
   from blank twice more with different `seed-base` values, for a 3-replicate estimate of the
   91.20% / 0.54% result; and in parallel run one stage-2 arm warm-started from `59540` on the same
   seed budget. If warm-started stage 2 does not beat from-blank stage 2 on VP *and* clearance, the
   split has no remaining justification. Is there a cheaper or more decisive design?

---

## 6. What to explicitly not assume

- Do not treat any in-training report table as an acceptance number (section 3a vs section 4 shows a
  7 pp gap and a scrambled per-faction ordering).
- Do not treat a sub-1.54 pp table-level clearance difference from one replicate as a real effect.
- Do not treat a waste improvement as real without reading `tactical/seat` next to it.
- All inference in this project is CPU-only by design (`ti4_tensor::inference_device()` is hard-
  coded to `Device::Cpu`); only gradient steps use CUDA, and there is a single shared GPU. Compute
  proposals should account for that.

## 7. Artifacts

```
out/stage2-blank-p5.log                               full 2000-update training log
out/checkpoints/stage2-blank-p5/checkpoint-240504     the stage-2 policy evaluated in section 4
out/eval-stage2blank-p5-u2000-clearance.log           91.20% +-0.38 Validation greedy
out/eval-stage2blank-p5-u2000-waste.log               0.54% any-waste, clear+zero 91.44%
out/checkpoints/blank-waste-mine-p5/checkpoint-59540  stage-1 champion (old reward)
out/eval-guided-baseline-u0-waste.log                 59540 waste: 2.31%, clear+zero 91.36%
out/checkpoints/blank-corrected-p5/checkpoint-56984   stage-1, corrected reward, u1600
out/eval-corrp5-s1-u1600-clearance.log                90.25% +-0.40
out/eval-corrp5-s1-u1600-waste.log                    3.55% any-waste, clear+zero 88.22%
out/eval-corrp5-s1-u2050-*.log                        89.02% / 2.60% — same arm run out to u2050
out/blank-corrected-p5.log                            corrected-reward stage-1 training log
crates/ti4-training/src/reward.rs                     reward definition (unit-term fix uncommitted)
crates/ti4-training/src/rollout.rs:38-80              Horizon::opening() / ::rounds()
plans/HANDOVER_2026-09-06_STAGE1_CONFOUNDS.md         the three confounds behind the stage-1 history
plans/STAGE2_TRANSFERABLE_LESSONS.md                  noise-floor replicate study
```
