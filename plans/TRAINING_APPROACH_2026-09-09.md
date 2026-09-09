# The current training approach — basis for an improvement review

Written 2026-09-09 for Astra, as the starting point for finding improvements. It describes what is
actually being run, what has been measured, what has already failed, and where the open questions
are. Every number is measured; where something is an assumption or a single replicate, it says so.

Read `plans/INFERENCE_OPTIMIZATION_2026-09-08.md` and `plans/ENGINE_OPTIMIZATION_2026-09-08.md`
(both yours) alongside this — they cover cost, this covers *what is being learned and how well*.

---

## 1. What is being trained

An MLP policy for Twilight Imperium 4, by PPO self-play, six seats of the same network.

**Stage 2** is the only line that matters now. It optimises victory points over a 4-round horizon,
with a small opening term. Stage 1 (one round, opening-gate reward) still exists in the code and is
described in §6, but §5 shows it has been superseded.

### The exact command currently in use

```powershell
$env:LIBTORCH = "…\out\libtorch-2.9.1-cu128-20260908"   # CUDA build, own CARGO_TARGET_DIR
.\target-cuda\release\examples\ppo_update.exe `
  --bundle out\checkpoints\stage2-mlp-shaped\checkpoint-473312 `
  --stage 2 --rounds 4 `
  --temperature 2.5 --movement-entropy 0.05 --entropy-final 1 `
  --learning-rate 3e-4 --waste-penalty 5 `
  --fleet-weight 0.03 --tech-weight 0.1 --strategy-diversity-weight 1.0 `
  --seed-base 2400057600 --updates 6400 --report-every 50 `
  --device cuda --out out\checkpoints\stage2-mlp-shaped-resumed
```

### The reward, term by term

| term | value | what it pays for |
|---|---:|---|
| `vp_weight` | 1.0 | a victory point |
| `objective_weight` | 0.35 | a *satisfied* public objective, taken back when scored |
| `secret_weight` | 0.25 | the same for a secret |
| `r1_bonus` | 3.0 | crossing the round-one opening bar |
| `r1_shaping` | 0.1 | the Stage-1 potential, **only on transitions with both ends inside round one** |
| `clearance_weight` | 0 | off |
| `high_vp_bonus` | 0 | off |
| `fleet_weight` | 0.03 | potential over fleet value in resources |
| `tech_weight` | 0.1 | per technology beyond setup |
| `strategy_diversity_weight` | 1.0 | terminal monoculture penalty, >80% of ≥3 plays |
| waste penalty | 5, all six factions | per wasted tactical activation, applied to returns |

Constraints the reward validates and why they exist (`crates/ti4-training/src/reward.rs`):
`objective_weight < vp_weight` and `secret_weight < vp_weight`, or standing next to a point pays
better than taking it. Every opening component is capped at its bar, so production or territory
beyond the gate cannot farm shaping. Rewards are potential *differences*, so losing ground is a
negative step rather than a smaller positive one.

`r1_shaping` is deliberately a tenth of Stage-1 magnitude: at Stage-1 magnitudes the opening
potential swamps a ~1.49-point game and Stage 2 quietly becomes Stage 1 again.

### Optimiser and sampling

PPO clip 0.2, 4 epochs, minibatch 4096, value 0.5, entropy 0.01/0.1 with movement 0.05, Adam at
3e-4, acting temperature 2.5, 16 seeds × 6 rotations = 96 games per update.

**`--entropy-final 1` means entropy annealing is off.** The schedule is
`scale = entropy_final.mul_add(progress, 1.0 - progress)`, which is identically 1 when
`entropy_final` is 1 — its default. No run in this project's history has annealed entropy. Whether
it should is an open question, not a decision.

---

## 2. How results are measured — get this right first

There are two acceptance measurements, both **greedy** at `--temperature 0.001`, 600 seeds ×
6 rotations = **21,600 seat-games**:

| metric | tool | pool |
|---|---|---|
| **clearance** — fraction of seat-games crossing the round-one bar | `clearance_eval` | Validation (`out/pools/full_np8_12_holdout.json`) |
| **waste** — per-faction clear / any-waste / tactical-per-seat | `build_positive_corpus` | Train |

Never conflate the two waste numbers: **any-waste** is *incidence* (seat-games with ≥1 wasted
activation); **waste/tactical** is *rate* (fraction of individual actions wasted). **`clear+zero`**
— cleared and wasted nothing — is the joint metric and the one worth optimising.

**`tactical/seat` is a mandatory diagnostic.** A waste penalty has a degenerate solution: stop
acting. Never report a waste improvement without it.

### Two traps that have already cost this project real time

**In-training report tables are not acceptance evidence.** They are self-play at temperature 2.5 on
the training pool. Measured gap, twice: 84.17% in-training vs **91.20%** greedy; 84.51% in-training
vs **92.12%** greedy. They also scramble the per-faction ordering — factions that look like the
laggards in-training are frequently not.

**`game_cost`'s decider/engine split is not what it appears** (you found this; it is recorded here
so nobody repeats it). It shares timing counters only with the candidate seat, so the residual
beside it contains five other seats' inference. `game_cost.rs` has since been relabelled and its
header points at `engine_cost`.

### The noise floor — the single most important number for an improvement review

From three identical-recipe replicates (`plans/STAGE2_TRANSFERABLE_LESSONS.md`):

- **table-level clearance: 1.54 pp spread (σ ≈ 0.80 pp)**
- **per-faction: up to 5.39 pp**

Most differences in §3 are inside this. Do not propose a change on the strength of a sub-1.54pp
table difference from one replicate, and do not diagnose a faction on less than ~5pp.

---

## 3. Where the policy actually is

All greedy, acceptance convention.

| checkpoint | clearance | any-waste | clear+zero | tactical/seat |
|---|---:|---:|---:|---:|
| `stage2-mlp-shaped-resumed/checkpoint-7084` (u50) | **93.15% ± 0.34** | 0.33% | **93.15%** | 2.679 |
| `stage2-mlp-shaped-resumed/checkpoint-241428` (u1700) | 92.12% ± 0.36 | **0.10%** | 92.52% | 2.629 |
| `stage2-mlp-shaped/checkpoint-473312` (u3600, previous run) | — | — | — | — |
| previous run u3400 | 92.31% ± 0.36 | 0.26% | 92.35% | 2.679 |
| stage-2 from blank, no shaping, u2000 | 91.20% ± 0.38 | 0.54% | 91.44% | 2.788 |
| stage-1 champion `blank-waste-mine-p5/59540` | 93.40% ± 0.33 | 2.31% | 91.36% | 3.000 |

Per faction at u1700:

| faction | clearance | any-waste |
|---|---:|---:|
| letnev | 96.89% ± 0.57 | 0.31% |
| jolnar | 96.44% ± 0.60 | 0.08% |
| l1z1x | 94.83% ± 0.72 | 0.17% |
| sol | 91.00% ± 0.93 | 0.03% |
| xxcha | 90.03% ± 0.98 | 0.03% |
| **hacan** | **83.53% ± 1.21** | 0.00% |

**The original target was ≥95% clearance and ≤0.5% any-waste jointly.** Waste is met with margin
and has been for some time. Clearance is ~2–3pp short at table level, and three factions now clear
95% individually.

**The binding constraint is now one faction.** Hacan at 83.53% is 6.5pp below the next-worst and the
only faction under 90%; that exceeds the 5.39pp per-faction floor, so it is probably real. Its
any-waste is 0.00% — it is not wasting, it is failing to cross the bar. Table clearance cannot reach
95% while one faction sits at 84%.

---

## 4. What has already been tried and failed

Do not re-propose these without new evidence.

1. **Behaviour cloning on clean trajectories.** 18,750 greedy clearing zero-waste demonstrations;
   cross-entropy improved monotonically (0.8044 → 0.7719) while clearance fell 90.76 → 89.73% and
   any-waste rose 5.08 → 8.15% over 7 epochs, every epoch. Classic distribution shift.
2. **A large waste penalty retrofitted onto a trained policy.** Waste became excellent, clearance
   collapsed, Letnev worst.
3. **Continuing fixed-penalty PPO past its sweet spot.** Reproduced twice; the corrected-reward arm
   went 90.25% at u1600 to 89.02% at u2050.
4. **Cross-game forward batching** (your measurement): 7,797 of 11,120 logits differ bitwise,
   max 6.1e-5. Surface-affecting, therefore ineligible — see §7.
5. **Blaming a faction's lag on a reward term.** Jol-Nar sat under 5% clearance for ~1,100 updates
   and then reached 96%+. Two mechanistic explanations were proposed and both were wrong. See
   `plans/jolnar-lags-early-in-stage2` reasoning in the memory and §8.

---

## 5. The result that reframes the whole approach

**Stage separation may not be earning its cost.** A stage-2 run *from blank weights* — no stage-1
warm start — reached 91.20% clearance and 0.54% any-waste, while a dedicated stage-1 run under the
same reward reached 90.25% / 3.55%. The stage-2 run beat the dedicated stage-1 run at stage 1's own
metric.

The original argument for the split was that round-4 VP is too noisy to learn from at blank weights
(σ ≈ 1.4 per player-game). That is refuted at n=1.

Open: how much of this is `r1_shaping 0.1` doing? The stage-2 reward still carries opening shaping,
so the finding may be "shaping-always-on beats a curriculum" rather than "stage 1 is unnecessary". A
sweep of `r1_shaping` (0.1 → 0.2 → 0.4) inside a stage-2 run would separate them. Full argument in
`plans/REPORT_2026-09-07_STAGE_SEPARATION_QUESTION.md`.

---

## 6. Known structural properties worth exploiting

- **In stage 1, `tactical/seat` is pinned at exactly 3.000** across all twelve measured arms,
  including a badly regressed one. A seat spends all three tactic tokens every time, so the
  "reduce waste by acting less" degenerate solution is effectively unavailable there. In stage 2 it
  *is* available and was taken: `tactical/seat` collapsed 4.733 → 0.076 by update 23 and took ~400
  updates to recover. Any waste-penalty work should account for that asymmetry.
- **The waste penalty from update zero was the only thing that ever improved clearance and waste
  together.** Retrofitting it does not work (§4.2).
- **Adam state is not in a checkpoint.** `INFERENCE_FILES` is a closed five-name list — trunk,
  readout, value, embedding, slots. Every "resume" therefore restarts the optimiser cold. This is a
  real confound in every continuation result in this project, including the current run.
- **Seeds must be advanced on resume.** `base = seed_base + 16 * update`. Reusing a base replays the
  exact maps and openings already trained on; `ppo_update.rs:76-78` documents it.

---

## 7. Hard constraints on any proposal

**The checkpoint surface is frozen.** Do not change what a seat observes, which options are offered,
their ids, their order, or `BTreeMap` iteration order. Any of those invalidates every trained
checkpoint. Numerical changes to the forward pass can move sampled actions even when mathematically
equivalent — treat them as surface-affecting until hashes prove otherwise.

**Determinism.** Seeded RNG and reproducible decision fingerprints are load-bearing. Prove
byte-identical games with ordered choice/event/final-state hashes, as you did for the supply patch.

**CPU inference is a specification constraint**, not an oversight —
`ti4_tensor::inference_device()` is hard-coded to `Device::Cpu` under §7.1. A GPU inference path is
a spec change requiring approval.

**One GPU, shared tree.** Check `tasklist` for `ppo_update.exe` before any CUDA job. The tree has
uncommitted work from several sessions; check `git status` before touching a file.

**Never `git worktree remove --force`.** It followed a junction and destroyed `out/` on 2026-09-08.
Do not use recursive force-deletes on scratch locations that have ever had a link into live data.

---

## 8. Open questions — the useful targets

1. **Hacan.** The binding constraint on the 95% goal. 83.53% clearance, 0.00% waste — it is not
   wasting, it is not crossing the bar. What is it failing at? `failed_openings` / `opening_failures`
   restricted to Hacan would say which component of the gate (planets, systems, unit composition)
   misses, without speculating about reward terms. **This is the highest-value open question.**
2. **Entropy annealing has never been used** (`--entropy-final 1` is the default and every run used
   it). Whether a decaying exploration bonus helps late-run clearance is untested.
3. **`r1_shaping` sweep** — separates "shaping beats a curriculum" from "stage 1 is unnecessary"
   (§5).
4. **Optimiser state in the bundle.** Every continuation restarts Adam cold. Adding optimiser state
   would remove a confound that has muddied at least four results — but it changes the bundle
   format, so it needs a plan.
5. **The waste penalty's early collapse.** 400 updates of a 6,400-update run were spent climbing out
   of near-zero activity. A penalty ramp or a Lagrangian/CMDP formulation with a learned multiplier
   might avoid it. Note any-waste is already 0.10%, so the constraint is slack — the question is
   whether the *transient* can be avoided, not whether the final waste can improve.
6. **Selection by worst faction rather than table mean.** With Hacan at 83.5% and Letnev at 96.9%,
   the table mean hides the thing that actually blocks the target.

---

## 9. Current state and artifacts

Training is **paused** at update 1,700 of 6,400, resumable from
`out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428`.

```
out/checkpoints/stage2-mlp-shaped/checkpoint-473312        u3600, previous run, committed to git
out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-7084  u50   93.15% / 0.33%
out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428 u1700 92.12% / 0.10%
  … 34 checkpoints at 50-update intervals in between
out/pools/full_np8_12_{train,holdout,final}.json           regenerated, verified against the
                                                           durable manifest
out/vocabulary/                                            generation fa3d6f94…, reconstructed
```

**A caveat you need.** On 2026-09-08 `out/` was destroyed (my error, recorded in §7). Recovered:
all three pools regenerate bit-for-bit from `plans/evidence/MLP-ARTIFACTS.md`; the vocabulary came
back from a git-committed checkpoint's `slots.json`; `checkpoint-473312` was in git. **Not
recovered:** every intermediate checkpoint of the previous run, the stage-1 champion `59540`,
`stage2-blank-p5/240504`, all training and eval logs, and **your own frozen profiling executables
and raw evidence** for the inference report. Numbers quoted from those runs in §3 and §4 come from
this session's records rather than from re-readable logs.

libtorch was re-downloaded; the CPU manifest was re-pinned with the owner's approval and a CUDA
distribution `libtorch-2.9.1-cu128-20260908` was added with its own manifest. The two builds use
separate `CARGO_TARGET_DIR`s because each stages its own DLLs beside the binary.

---

## 10. What would make a good answer

A ranked set of changes with, for each: the measured evidence, an estimated effect **stated against
the 1.54pp noise floor**, the risk to determinism and to the checkpoint surface kept separate, and
an honest line between what was measured and what is extrapolated. Your last two reports were the
right shape.

If the honest finding is that the recipe is near its ceiling and the remaining gap is one faction's
opening play, say that — it is a more useful result than a marginal coefficient change.
