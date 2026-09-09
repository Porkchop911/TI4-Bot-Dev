# Improving training for victory points — 2026-09-09

Status: investigation and experiment design; no training recipe or production code changed, no CUDA job started. The 12-map pilot finds u1700 ahead of its starting weights by 0.299 VP against frozen opponents, provisionally; the clearance ranking alone does not establish the VP ranking. Primary objective: **mean candidate victory points at the existing four-round horizon**, with fixed-opponent robustness checks. Opening clearance and waste are diagnostics/guardrails, not substitutes for VP. If the intended objective is full-game wins rather than four-round points, that requires a separately stated horizon and acceptance metric.

## 1. Measured evidence

### Existing training log: little demonstrated VP progress

The recovered `out/stage2-mlp-shaped-resumed.log` is readable. Its 34 report windows range from **3.160 to 3.295 VP**; the first five windows average **3.2494**, the last five **3.2248**. The individual u50 and u1700 windows report 3.207 and 3.278. These are temperature-2.5 self-play measurements on changing training seeds/opponents, not a fixed-opponent evaluation or a statistical test. They do not establish either improvement or regression. They do show that the current ~3.2-VP regime should not be described using the old ~1.49-VP scale.

### Fresh checkpoint screen against fixed opponents

One frozen `crossplay_eval` executable evaluated each candidate against five copies of `stage2-mlp-shaped/checkpoint-473312`. Validation pool, seeds 910000000–910000003, six faction rotations × six candidate positions, four rounds, temperature 0.001 for every seat, 16 Rayon workers. All three checkpoint vocabularies have identical SHA-256 `fa3d6f945988cc9f210fffafff115422c9bf883c077ae8aac8aaf483d1ec41fc`. All 432 games completed without reported truncation or inference errors.

| Candidate | Candidate games | Mean VP | Margin to strongest opponent | Strict VP lead | Round-one clearance | Any waste |
|---|---:|---:|---:|---:|---:|---:|
| Starting checkpoint 473312 | 144 | 3.826 | −1.757 | 10.4% | 91.67% | 2.08% |
| Resumed u50 / 7084 | 144 | 3.847 | −1.722 | 10.4% | 96.53% | 0.69% |
| Resumed u1700 / 241428 | 144 | 3.854 | −1.618 | 11.1% | 91.67% | 0.69% |

Only four independent map-seed clusters: **this screen cannot rank the checkpoints reliably**. The 144 rows are not 144 independent maps. Nonetheless, the 4.86pp clearance gap between u50 and u1700 accompanies only −0.007 VP for u50. Selecting the higher-clearance checkpoint would not establish that we selected the higher-VP policy.

The strict-lead column is not the engine's full-game win metric: ties are excluded and the game is stopped at the prescribed horizon. Likewise, margin against the maximum of five opponents has a negative equal-policy baseline; compare changes against the first row, not against zero.

At u1700 there were 3.26 *non-forced scoring-head offers* per candidate game and 0.64% were declined, versus 3.31 and 3.35% for the starting checkpoint. This is not a census of all possible scores: the wrapper ignores single-option choices, and heads do not enumerate every VP source. It nevertheless weakens the case that indiscriminate refusal to score is the dominant current failure. Investigate how scoring opportunities are created and converted, rather than adding an untested “always score” rule.

### Separate 12-map paired follow-up: some evidence of VP improvement

The follow-up used seeds 910000010–910000021, the same executable/opponent/settings, and one invocation per map to retain cluster-level results: **432 candidate games per checkpoint, 1,296 games total**, with no reported errors or truncations. This is a separate seed cohort from the initial screen, not a replacement of its results.

| Candidate | Mean VP |
|---|---:|
| Starting 473312 | 3.5463 |
| Resumed u50 | 3.7083 |
| Resumed u1700 | 3.8449 |

| Fixed-checkpoint comparison | Mean VP difference | Paired map-bootstrap 95% interval | Maps better / worse |
|---|---:|---:|---:|
| u50 − starting | +0.1620 | [−0.0069, +0.3287] | 8 / 4 |
| u1700 − starting | **+0.2986** | **[+0.0718, +0.5417]** | 9 / 3 |
| u1700 − u50 | +0.1366 | [−0.0509, +0.3241] | 7 / 5 |

Method: 50,000 paired bootstrap resamples of the 12 map seeds, keeping all 36 candidate-position/rotation cases together; RNG seed 20260909. Per-map integer VP totals are recovered unambiguously from 36 games × the tool's three-decimal means (rounding error is <0.018 total VP). Intervals are exploratory, unadjusted for the three comparisons, and have only 12 clusters. The raw analysis also records a sign-flip reference calculation; neither calculation measures training-replicate variance.

This supports **continuing to investigate the mature Stage-2 policy**, rather than declaring it stalled or choosing u50 solely for clearance. It is encouraging evidence for these particular weights against this particular opponent, not proof that the current recipe is optimal or that another run would gain 0.30 VP. The u1700/u50 difference is still unresolved. Confirm on the larger predeclared validation cohort and another opponent before promoting a champion. The disagreement in effect size between the four-map and twelve-map samples demonstrates why small screens cannot size VP gains reliably.

Raw files: `paired-<seed>-<checkpoint>.log`; analysis: `paired-vp-analysis.json`, all under `out/training-review-20260909`.

### Hacan: real opening symptoms, not yet a demonstrated VP bottleneck

A separate fresh `opening_failures` sample used u1700, eight Train-pool seeds 710000000–710000007, six rotations, one round, temperature 0.001: 48 Hacan openings, **11 failures**. Of these, 10 missed planets, 7 missed systems, and 6 missed the unit/composition test; components overlap. Eight failures gained two planets, two gained one; the remaining failure did not belong to those bins. Failed Hacan openings had 1.82 ship-occupied systems versus 2.70 when cleared. None had all ground forces in a single system. These data support examining expansion and composition/attrition, not a blanket “all infantry on one planet” diagnosis.

The diagnostic's printed “1 unit gained” banner is stale: the inspected source calls `opening.units_ok()`, which checks capacity ships and infantry against the current requirement (2 and 3). Do not treat that caption as the current contract. This Train-pool sample is not comparable to the document's 600-seed Validation clearance acceptance run.

Hacan is not automatically the highest-value VP intervention. In the four-seed crossplay screen its u1700 VP is 3.792; L1Z1X is lower at 3.500 despite 100% clearance in that small sample. Neither faction ordering is an acceptance claim. A useful next census should join opening failures to **later VP, objective types and opportunities**, and distinguish unreached targets from unsuccessful delivery/combat losses. Correlation alone would still not prove a repair increases VP.

## 2. Corrections to the review's framing

1. **The 1.54pp range is not a VP noise floor.** `STAGE2_TRANSFERABLE_LESSONS.md` derives it from three Stage-1 training replicates, and explicitly says Stage 2 needs its own study. A three-run range is also not a universal significance threshold. Preserve it as a warning against small single-replicate clearance claims; measure VP evaluation uncertainty and Stage-2 recipe variance in VP units separately.
2. **The joint clearance/waste figures use different pools.** A same-cohort clear-and-zero-waste fraction cannot exceed that cohort's clearance. The supplied u1700 table has 92.52% clear+zero beside 92.12% clearance because the columns come from Train and Validation. They are not one joint acceptance measurement. Measure VP, clearance, incidence, rate and tactical activity on the same evaluation cohort.
3. **84% Hacan does not mathematically prevent 95% table clearance.** Five factions at 100% plus Hacan at 84% average 97.33%; reaching 95% would require the other five to average 97.2%. Hacan is a large practical gap, not an absolute ceiling, and clearance itself is not the requested primary objective.
4. **Self-play mean VP is not intrinsically fixed.** At a four-round cutoff, public/secret scoring can increase for multiple seats; TI4 VP is not a fixed-sum score. Self-play mean VP is useful for the user's absolute-score goal, but changing opponents makes it insufficient as evidence of robust policy improvement. Keep it alongside fixed-opponent VP and a small crossplay panel. The stronger “mean VP is pinned” wording in the crossplay tool's comment is not a general property of this horizon.
5. **Greedy here means the established 0.001-temperature convention.** It is near-greedy sampling, not a proof of exact argmax on every near-tie. Keep the convention fixed for comparability; do not silently replace the sampler. A separate candidate-temperature sweep can diagnose execution mismatch without selecting a recipe on incomparable hot-policy scores.

## 3. The strongest mechanism: the reward still buys non-VP outcomes

With discount 1, `reward::step_rewards` adds differences of the horizon potential, and `returns` takes suffix sums. The horizon potential includes:

`VP + 0.35 × unscored scoreable publics + 0.25 × unscored scoreable secrets + 0.03 × fleet resource value + 0.1 × technologies gained`.

Thus these terms telescope into **final potential minus starting potential**. The final auxiliary potential is not cancelled. This is an algebraic fact about the current code, not a measured policy pathology. The checks `objective_weight < vp_weight` and `secret_weight < vp_weight` protect an immediate score conversion; they do not prove the complete shaped objective prefers the VP-optimal trajectory.

Concrete reward exchange rates, not predicted VP gains:

- One wasted tactical activation costs **5 VP-equivalents** before other changes.
- Clearing round one pays **3**, and the exact formula also subtracts `0.3 × shortfall`; the supplied reward table omits that failure component.
- Another **33⅓ resources of surviving fleet** pays one VP-equivalent; ten technologies pay one. These may reward useful investments, but terminal holdings also have value independent of scoring.
- Strategy monoculture can cost **one full VP-equivalent**. With four plays, 3/4 = 75% escapes the >80% condition, while 4/4 pays the full penalty. There is an incentive to switch one card even if repeating it would score better; whether the current policy encounters such choices remains unmeasured.
- A still-satisfied but unscored objective at the horizon retains shaping value.

Potential-based shaping can preserve optimal policies under its required discounted and terminal conditions; simply calling a finite-horizon difference a “potential” does not establish those conditions. Here the telescoping terminal residue can be seen directly. [Ng, Harada & Russell, 1999](https://ai.stanford.edu/~ang/papers/shaping-icml99.pdf).

Do **not** remove all shaping at once. It helped the policy learn and may still be useful. First perform controlled ablations of the most weakly justified terms, recording their realized per-seat reward contributions. A later VP-only design could cancel the auxiliary terminal residue at the defined horizon while retaining VP reward, but that changes training targets and needs return-level tests. With the current undiscounted Monte Carlo suffix returns, a fully terminal-neutral potential mostly becomes a state-dependent baseline shift; do not promise it will magically solve credit assignment.

## 4. Ranked changes

No proposed recipe has a measured VP lift in this investigation. Numeric VP promises would be invented. The estimates below therefore distinguish mechanisms, pilot success thresholds and actual observations. For every row, a clearance change below the historical 1.54pp range is not persuasive recipe evidence on its own; a VP change must instead be judged against measured VP uncertainty.

| Rank | Change | Evidence / effect estimate | Reproducibility risk | Frozen inference surface risk |
|---|---|---|---|---|
| 1 | Select and stop by held-out VP against fixed opponents; use clearance/waste as same-cohort diagnostics | Removes a demonstrated metric mismatch. The 12-map pilot measures u1700 +0.299 VP versus the starting weights, but cannot yet resolve u1700 versus u50 or estimate recipe variance. Predeclare +0.10 VP as a useful pilot target, **not a predicted lift**. | Low for read-only evaluation; map-cluster pairing and frozen opponents required. | None if existing inference is used unchanged. |
| 2 | Ablate the strategy-diversity reward, then test a smaller round-one bonus separately | Removes incentives worth up to 1 and 3 reward units respectively. Could improve or harm VP; no lift estimate supported. Test VP directly, not a ≥95% opening target. | Each arm intentionally learns different weights; same-arm seeded reproducibility must hold. | No actor-input/layout change; learned checkpoints differ as intended. |
| 3 | Test entropy annealing during late training | Never tested in this project; `--entropy-final 0.25` is already supported. Potential benefit is better exploitation; downside is early fixation. Unknown VP effect. | Low implementation risk; schedule resets on restart must be controlled. | No change to inference math or features at fixed weights. |
| 4 | Audit/remove terminal fleet/tech residue; monitor actual wasted activations before relaxing their penalty | Proven reward mismatch mechanism, not proven behavior. Ablate fleet and tech separately after rank 2. Waste is already small greedily; avoid another large retrofit penalty. | Reward semantics and critic targets change; arm-by-arm tests needed. | No observation changes required. |
| 5 | Add a small frozen-opponent mixture to training, scored against an untouched opponent panel | Pure current-self-play may specialize. A starting 20% historical-opponent fraction is a hypothesis, not a TI4 result or optimal number. Unknown VP lift. | Medium: deterministic opponent selection and only learner-generated PPO records. | Low if all opponent vocabularies match and observations/order remain identical. |
| 6 | Improve credit assignment only after measuring critic quality: GAE pilot with gamma retained at 1 | Current target is Monte Carlo return minus stored value. GAE may reduce variance at a bias cost; no evidence yet that this is the bottleneck. | Medium/high: terminal masks, seat boundaries and frozen advantages must be exact. | Actor surface unchanged; training mathematics intentionally changes. |
| 7 | Persist complete training state in a separate training snapshot | Removes known cold-Adam/resume confounding. No direct VP-gain forecast; primarily buys trustworthy continuation and experiments. | High correctness bar: uninterrupted/resumed optimizer and parameter fingerprints must match. | Avoid modifying the closed inference bundle; use a sibling training-state artifact. |

The 20% historical-opponent starting point has precedent in OpenAI Five, which mixed current and past opponents to address strategy collapse; that is motivation to test, not transferable evidence of a TI4 gain. [OpenAI Five paper, self-play section](https://cdn.openai.com/dota-2.pdf).

GAE explicitly trades variance for bias. Measure explained variance and value error by faction and round before choosing it; current aggregate critic loss alone cannot show whether the value baseline is useful. A blind gamma=0.99 change is especially unattractive when time is counted in hundreds of seat decisions: it would strongly suppress late VP. For example, 0.99^500 ≈ 0.0066. Keep gamma=1 for the stated undiscounted objective in the first GAE experiment. [Schulman et al., GAE](https://arxiv.org/abs/1506.02438).

## 5. A concrete experiment sequence

### A. Finish a VP baseline before another long continuation

Evaluate retained u50, u1700 and starting 473312 on identical validation seeds against the same fixed opponent, then a second frozen opponent. Start with a modest screen; promote survivors to 600 map seeds. Emit **per-map** candidate VP, margin, scoring sources/opportunities, per-round VP, clearance, any-waste, waste/tactical and tactical/seat. Exclude no failed game silently; reject an acceptance run with errors/truncations. Bootstrap paired **map seeds**, carrying all rotations and candidate-seat cases together. Keep the final pool untouched for confirmation after selecting a recipe.

Do not select 34 checkpoints by repeated noisy point estimates without accounting for selection bias. Screen a predeclared sparse set (early/mid/latest), select on validation, and perform a final untouched evaluation. A historical seed base different from training is not a substitute for using the correct pool role.

### B. First controlled PPO pilots

Use u1700 as a fixed common research starting point; this does not declare it the VP champion. All arms cold-start Adam identically because the current snapshot cannot restore it. Run three rollout-seed replicates per arm, same seed blocks across arms, and compare against a contemporaneous control. Preserve four rounds, temperature 2.5, learning rate 3e-4, all other coefficients and update budget.

| Arm | Only change relative to current recipe | What it tests |
|---|---|---|
| Control | None | New Stage-2 VP variance and continuation baseline |
| D | `--strategy-diversity-weight 0` | Whether card diversity is taking priority over VP |
| E | `--entropy-final 0.25` | Whether late exploration pressure prevents consolidation |
| O, second wave | `--r1-bonus 1` | Whether opening reward is too expensive relative to VP |

A **300-update screening budget** per arm/replicate costs roughly 1.5 hours at the observed ~18 s/update; control+D+E across three replicates is roughly **13.5 GPU-hours plus evaluation**, not a few minutes. Run sequentially on the single GPU. This is a proposed bounded experiment, not a job launched by this review. A flat 300-update result is a screening result, not proof that a recipe cannot improve after longer training. Extend only promising/stable arms to a common longer budget.

Seed and schedule bookkeeping must use the actual number of completed updates/last seed from the run, not an inferred checkpoint filename. Persist the entire command, source/executable hashes, starting tensor hashes, seeds, temperatures, opponent assignments and schedule position. All arms must share restart semantics. Do not compare a resumed cold-Adam arm against an uninterrupted control and call the difference a reward effect.

Promote based on paired VP improvement and replicated consistency. The +0.10 VP pilot target is a practical decision threshold, not a significance threshold; require uncertainty estimates and report every replicate. If the user ultimately wants wins, choose a corresponding full-game primary metric instead of changing the reward to VP margin by default. Worst-faction VP and activity are guardrails; worst-faction clearance should not override a robust increase in the requested VP objective without an explicit reason.

### C. Only then escalate the algorithm

For historical opponents, initially reserve a small deterministic fraction of games for a fixed opponent set; continue collecting PPO records **only for the current learner's seats**. Old opponents' behavior records are not on-policy learner data. Fix and rotate learner seats, retain deterministic harvest order, and avoid training against the same panel used for final acceptance.

For GAE, preserve gamma=1 initially, sweep lambda only after the critic diagnostic, and treat round transitions within the four-round game as nonterminal. The four-round boundary is terminal for this stated task; it is a truncation requiring different treatment if the target changes to full-game return. Changing episode/seat boundaries incorrectly would bias every advantage.

For resumability, save Adam moments/step, global update, seed position, entropy schedule, RNG state or deterministic derivation metadata, reward/settings, model and source identity atomically beside the unchanged inference bundle. Old checkpoints cannot recover lost Adam state retroactively. Test split-versus-uninterrupted training on the supported device; prove identical fixed-weight inference via ordered choice/event/final-state hashes as a separate gate.

## 6. Deprioritized or ruled out

- More Stage-1 training or a larger r1_shaping sweep as the default next move: those optimize opening behavior; VP benefit has not been shown. Hacan failures remain worth diagnosing, but fixing their clearance percentage is not evidence of more points.
- “Always score” rules, action masks, new observation features, vocabulary/head/network changes: not compatible with the frozen policy surface. The scoring-offer census also gives no case for a blanket override.
- Another unanchored behavior-cloning pass or counterfactual action relabeling: prior project failures stand; no new supporting evidence here.
- A new large waste penalty: the known inactivity failure and already-low greedy waste argue against it. If relaxing the existing penalty is tested later, use a small continuation ablation with VP and activity, not a new zero-update ramp claim based on this mature policy.
- GPU inference/batched kernels as a learning fix: out of scope for increasing VP per game and still constrained. Earlier runtime results remain useful hypotheses, but their destroyed raw artifacts cannot be freshly audited and are not evidence for a VP gain.
- Declaring the policy near its ceiling: unsupported. We have not established the VP noise floor, a stable VP champion, the value of the auxiliary rewards, or robustness against varied opponents.

## 7. Scope, provenance and limits

Read-only production review plus new report and bounded CPU evaluations. The supplied approach document and existing dirty training/reward/feature files were not edited. No staging, commits, deletion, worktree operations, new training, optimizer changes, or subagents. No ppo_update.exe was running at the preflight check. Existing CPU evaluation executables were copied to `out/training-review-20260909` and frozen for the comparisons; source inspection guides interpretation, but this review did not rebuild/reconstruct their complete historical build environment.

Raw evidence: `out/training-review-20260909/crossplay-{base,u50,u1700}.log`, `crossplay-summary.json`, `opening-u1700.log`, and `report-series.json`. Existing crossplay inference and its wrapper were used without modification; this review does not claim a new wrapper-identity/hash qualification or a trained-policy A/B. All conclusions about recipe improvements are hypotheses until PPO arms are run. Byte-identical games are appropriate for instrumentation/infrastructure changes at fixed weights; they cannot be required between differently trained policies, whose choices are supposed to improve.

Primary code inspected: `crates/ti4-training/src/reward.rs` (horizon potential, step rewards, suffix returns), `crates/ti4-mlp/examples/ppo_update.rs` (recording, waste returns, schedules), `crates/ti4-mlp/src/ppo.rs` (frozen Monte Carlo advantages), `crates/ti4-mlp/src/bundle.rs` (closed inference artifact list), and the existing crossplay/opening diagnostics. This report is independent investigation evidence, not a migration acceptance or independent review.

## Appendix: durable per-map VP totals

Each cell is total candidate VP over 36 seat/rotation games, not a percentage. These small raw aggregates are retained here so the principal paired analysis survives loss of ignored out/ artifacts.

| Map seed | Starting 473312 | u50 | u1700 |
|---|---:|---:|---:|
| 910000010 | 142 | 146 | 153 |
| 910000011 | 139 | 129 | 126 |
| 910000012 | 111 | 135 | 126 |
| 910000013 | 121 | 111 | 116 |
| 910000014 | 126 | 146 | 161 |
| 910000015 | 140 | 149 | 143 |
| 910000016 | 124 | 139 | 164 |
| 910000017 | 117 | 110 | 124 |
| 910000018 | 113 | 111 | 132 |
| 910000019 | 141 | 149 | 147 |
| 910000020 | 120 | 131 | 115 |
| 910000021 | 138 | 146 | 154 |

Frozen crossplay executable SHA-256: `8736f60fc3bfc28c2b40fceda4357bb856e3dddc7d6f1ad5fdeee2a1f7a166fa`. Input/checkpoint/source/log checksums are in `out/training-review-20260909/manifest.json`.
