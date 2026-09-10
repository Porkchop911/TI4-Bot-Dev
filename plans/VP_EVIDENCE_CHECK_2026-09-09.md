# Checking Astra's VP report against a larger cohort — 2026-09-09

Response to `plans/TRAINING_VP_IMPROVEMENTS_2026-09-09.md`. Read-only evaluation on CPU; no
training started, no recipe changed, nothing deleted. One source edit, a stale caption, in §5.

**Summary.** Every number in Astra's report reproduces exactly from its own raw logs. Its *direction*
survives an independent 100-map cohort. Its *headline magnitude does not*: u1700's advantage over the
starting weights is **+0.093 VP**, not +0.299, and the u50 result **changes sign**. The practical
consequence is that the report's own proposed pilot threshold of +0.10 VP is about the size of the
entire gain from 1,700 updates, so it cannot screen a 300-update arm.

---

## 1. Reproduction: everything in the report checks out

Re-derived from `out/training-review-20260909/*` and the committed source, not taken on trust.

| Claim | Verified |
|---|---|
| 12-map appendix, all 36 per-map VP totals | Every cell. `ALL` mean × 36 rounds to the stated integer. |
| Means 3.5463 / 3.7083 / 3.8449 | Exact. |
| Paired bootstrap, 50,000 resamples, RNG seed 20260909 | **+0.1620 [−0.0069, +0.3287]**, **+0.2986 [+0.0718, +0.5417]**, **+0.1366 [−0.0509, +0.3241]**, and 8/4, 9/3, 7/5 — every figure to four decimals. |
| Four-seed screen table (VP, margin, strict lead, clearance, waste, offers, declined) | Exact, all three rows. |
| Training log: 34 windows, range 3.160–3.295, first five 3.2494, last five 3.2248, u50 3.207, u1700 3.278 | Exact. |
| Hacan: 48 openings, 11 failures, 10/7/6 components, 8 gained two planets and 2 gained one with one failure in neither bin, 1.82 vs 2.70 ship-occupied systems, none all-in-one-system | Exact. |
| `opening_failures` banner is stale | Confirmed. Banner said "1 unit gained"; `units_ok` (`opening.rs:120`) tests `DEFAULT_REQUIREMENT` = 2 capacity ships and 3 infantry. |
| Reward telescopes to final potential minus current | Confirmed. `step_rewards` takes potential differences, `returns` takes suffix sums, `discount` defaults to 1: return at decision *i* = P(final) − P(s_i). The auxiliary terms do not cancel. |
| One wasted activation = 5 VP-equivalents | Confirmed. `ppo_update.rs:609-618` subtracts `penalty × γ^(end−i)` from every return at `i ≤ end`; with γ=1 that is a flat −5. |
| 4/4 strategy plays pays the full penalty, 3/4 pays nothing | Confirmed. `reward.rs:400` gates on `share > 0.8`; 3/4 = 0.75. |
| `0.3 × shortfall` in the round-one term | Confirmed: `r1_bonus` (3.0) × `0.1 × shortfall`. |
| 33⅓ fleet resources and 10 technologies each pay one VP-equivalent | Confirmed at the weights the run used (0.03, 0.1). |
| ~18 s/update | Median is **17.0 s** over 1,701 updates. The 13.5 GPU-hour budget is a slight over-estimate; ~12.8. |

Its two corrections to `plans/TRAINING_APPROACH_2026-09-09.md` are both right, and both are mine:

1. "Table clearance cannot reach 95% while one faction sits at 84%" is **false**. Five factions at
   100% and Hacan at 84% averages 97.33%. Hacan is a large practical gap, not an arithmetic ceiling.
2. The `clear+zero` column (92.52%) sitting above the clearance column (92.12%) in the same row is
   a **pool mix**: clearance comes from `clearance_eval` on Validation, waste from
   `build_positive_corpus` on Train. They were never one joint measurement and the table should not
   have implied they were.

---

## 2. What twelve map clusters could not tell us

Astra evaluated three checkpoints. I ran the four missing rungs of the same ladder — u200, u500,
u900, u1300 — on the *same* twelve map seeds, with the *same* frozen executable, after reproducing
one of its cells bit-identically (map 910000010, u1700, `ALL 36 4.250 −1.611 16.7% 97.22% 2.78%
2.86 0.97%`, at 12 Rayon threads against its 16, so the measurement is thread-count independent).

| Checkpoint | Mean VP | vs base | Paired 95% |
|---|---:|---:|---:|
| base 473312 | 3.5462 | — | — |
| u50 | 3.7084 | +0.162 | [−0.007, +0.329] |
| u200 | 3.5949 | +0.049 | [−0.102, +0.209] |
| u500 | 3.6365 | +0.090 | [−0.079, +0.264] |
| u900 | 3.5765 | +0.030 | [−0.169, +0.218] |
| u1300 | 3.6273 | +0.081 | [−0.104, +0.250] |
| u1700 | 3.8448 | +0.299 | [+0.072, +0.542] |

Not monotone. Five intermediate checkpoints sit flat in a 3.58–3.71 band, none separating from base,
with u1700 alone standing ~0.22 above its own neighbourhood. A learning curve in which 50 updates
buy half the gain, the next 1,250 buy nothing, and the last 400 jump is not a plausible shape. That
motivated a bigger cohort rather than a recipe change.

---

## 3. The 100-map confirmation cohort

Fresh seeds 910001000–910001099, disjoint from both prior cohorts. Same frozen
`crossplay_eval-frozen.exe`, same Validation pool, same five-copy 473312 opponent panel, four rounds,
temperature 0.001, one invocation per map so cluster structure is retained. 400 invocations, 14,400
games, no errors or truncations. Raw logs in `out/vp-confirm-20260909/`.

| Checkpoint | Mean VP | vs base | Paired 95% (100 clusters) | Maps better/worse |
|---|---:|---:|---:|---:|
| base 473312 | 3.6342 | — | — | — |
| u50 | 3.5820 | **−0.052** | [−0.102, −0.002] | 39 / 56 |
| u1300 | 3.6603 | +0.026 | [−0.027, +0.079] | 53 / 43 |
| u1700 | **3.7275** | **+0.093** | **[+0.041, +0.146]** | 62 / 34 |

| Contrast | Difference | Paired 95% |
|---|---:|---:|
| u1700 − u1300 | +0.067 | [+0.001, +0.133] |
| u1300 − u50 | +0.078 | [+0.021, +0.136] |

**Direction holds, magnitude does not.** u1700 really is ahead of the weights it started from, on an
independent cohort, against a fixed opponent. But the effect is **+0.093, roughly a third of the
12-map estimate**, and the 12-map interval [+0.072, +0.542] contained the larger cohort's answer only
at its very bottom edge.

**u50 flips sign.** Twelve maps said +0.162; a hundred say −0.052 with the interval excluding zero.
The report's second-ranked observation — "the 4.86pp clearance gap between u50 and u1700 accompanies
only −0.007 VP for u50" — was drawing on a number whose sign was wrong. Its conclusion (selecting on
clearance does not select on VP) is unaffected and, if anything, strengthened: u50 has the *best*
clearance of the three and the *worst* VP.

### Why the small cohort missed by so much

The paired-difference standard deviation is **0.270 VP** per map cluster, so twelve clusters give a
standard error of 0.078. The 12-map estimate of +0.299 was 2.6 standard errors above the truth, and
u50's was 2.7 above — **both high, in the same cohort, in the same direction**. That is the tell: with
twelve maps the *cohort itself* is the unit of luck, and candidates sharing a cohort share its
excursion. Pairing removes the base level, not a cohort's affinity for a family of policies. The base
level moves too: base scores 3.5463 on the twelve maps and 3.6342 on the hundred, a shift of +0.088
that is itself as large as the entire measured effect.

### What a cohort of a given size can resolve

At sd(diff) ≈ 0.27, for 80% power on a paired comparison:

| Effect to resolve | Map clusters needed |
|---|---:|
| +0.30 VP | ~7 |
| +0.10 VP | ~58 |
| +0.05 VP | ~229 |

For checkpoint-vs-checkpoint comparisons within the same run, sd(diff) rises to ~0.34 and +0.10 needs
~89 clusters. **100 maps is roughly the minimum useful cohort** and 12 is only adequate for effects
three times larger than anything measured here.

---

## 4. What this changes about the proposed plan

Astra's structural recommendations stand, and rank 1 is right: **we have been selecting on clearance
and the ordering does not transfer to VP.** The evidence for that is now stronger, not weaker. But
three things in the experiment design need revising before any GPU time is spent.

1. **The +0.10 VP pilot threshold is not usable as specified.** It is approximately the total
   measured gain from 1,700 updates (+0.093). A 300-update screening arm that had to clear +0.10 to
   be promoted would reject arms that are working. Either the threshold drops to something a
   300-update arm could plausibly show, or the screen measures something other than terminal VP.

2. **Every arm needs ≥100 map clusters, and all arms must share the same cohort.** The table above
   is the cost: at ~4.5 s per map-invocation on CPU, one arm at 100 maps is ~8 minutes. That is
   cheap relative to the 85 minutes of GPU per 300-update arm, so there is no reason to screen on 12.

3. **Three rollout-seed replicates per arm is the right instinct but the wrong axis to economise on.**
   With evaluation noise now characterised (±0.053 at 100 maps) and training-replicate variance still
   completely unmeasured, the first thing worth spending GPU on is not arm D or E — it is **two
   replicates of the control**, which is the only way to learn whether a 300-update difference means
   anything at all. That is ~3 GPU-hours and it gates the interpretation of every other arm.

The reward-mechanism finding in its §3 is the strongest part of the report and is unaffected by any
of this: the policy is literally trained to maximise `VP + 0.35·unscored publics + 0.25·unscored
secrets + 0.03·fleet + 0.1·tech` at the horizon, with no requirement that the auxiliary holdings ever
convert. That is a real objective mismatch, established from the code rather than inferred from
behaviour, and the ablation ordering it proposes (strategy-diversity first, then r1 bonus, then
fleet/tech) is sound.

---

## 5. One fix applied

`crates/ti4-mlp/examples/opening_failures.rs` printed `bar  3 planets gained, 3 systems, 1 unit
gained`. The unit bar became two capacity ships and three infantry; `units_ok` has been testing that
all along. The caption is now derived from `ti4_engine::opening::DEFAULT_REQUIREMENT` rather than
hand-written, and the module doc no longer says "one unit built". A diagnostic that misreports its
own bar is worse than one that prints none.

---

## 6. Provenance

Frozen executable `out/training-review-20260909/crossplay_eval-frozen.exe`, SHA-256
`8736f60fc3bfc28c2b40fceda4357bb856e3dddc7d6f1ad5fdeee2a1f7a166fa` per Astra's manifest, run with
`out/libtorch-2.9.1-cpu/lib` on PATH. Ladder logs in `out/vp-ladder-20260909/`, confirmation logs in
`out/vp-confirm-20260909/`. Bootstraps are 50,000 paired resamples of map clusters, RNG seed
20260909, unadjusted for multiplicity — six arms in §2 and three contrasts in §3, so the nominal 95%
intervals are optimistic and the marginal ones (u50 upper bound −0.002, u1700 − u1300 lower bound
+0.001) should not be read as decisive.

No CUDA job ran; training remains paused at
`out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428`.
