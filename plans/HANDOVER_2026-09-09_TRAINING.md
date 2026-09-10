# Training handover — 2026-09-09

**Author:** Claude Opus 5. **Branch:** `wp/tier-c-review-remediation-obs008c2b-003e1` at `bd54868`.
**GPU:** free. **Nothing is running** except the arm evaluation described in §5, which resumes safely.

The short version: the objective the policy is trained on is **not victory points**, and the
measurement that said otherwise was too small to be trusted. Both are now quantified. Three
experiment arms have finished and their acceptance evaluation is mid-flight.

---

## 1. The conventions, which nothing below violates

- **Acceptance = greedy.** `clearance_eval --temperature 0.001 --seeds 600` on the Validation pool
  (21,600 seat-games) for clearance; `build_positive_corpus` on Train for waste.
- **In-training tables are self-play at T=2.5 and are NOT acceptance evidence.** The measured gap to
  greedy has been ~7pp twice. Never promote on them.
- **Two waste metrics, never conflated:** `any-waste` (incidence) vs `waste/tactical` (rate).
- **`tactical/seat` is mandatory** alongside any waste change: the waste penalty's degenerate
  solution is to stop acting.
- **Stage-1 results report clearance only**, never VP movement.
- One CUDA process at a time on the single 3090. Check `tasklist` first.

---

## 2. What the policy is actually trained on

With `discount = 1`, `returns` is an exact undiscounted suffix sum, so the return at decision *i* is
**exactly** `P(final) − P(s_i)`, and the advantage subtracts the critic's `V(s_i)`. The `−P(s_i)`
half is a deterministic function of the state the critic sees and cancels once fitted.

**Therefore every auxiliary weight does exactly one thing: change what the policy is trained to be
holding when the game is cut off.** None of them helps credit assignment — in a Monte-Carlo suffix
return the intermediate rewards only reach the gradient through their telescoped endpoints.

Measured cost of that, at the weights the run used (census: greedy, 600 games, 4 rounds, Train pool,
`out/vp-confirm-20260909/census-u1700-dirty.log`):

| faction | techs/game | fleet value | tech term | fleet term | monoculture rate | net auxiliary |
|---|---:|---:|---:|---:|---:|---:|
| hacan | 3.71 | 22.28 | 0.371 | 0.668 | 10.3% | **+0.936** |
| jolnar | 4.67 | 16.97 | 0.467 | 0.509 | 26.7% | **+0.709** |
| l1z1x | 1.91 | 24.01 | 0.191 | 0.720 | 5.8% | +0.853 |
| letnev | 1.94 | 26.25 | 0.194 | 0.788 | 0.5% | +0.977 |
| sol | 2.59 | 22.09 | 0.259 | 0.663 | 1.3% | +0.909 |
| xxcha | 1.02 | 24.27 | 0.102 | 0.728 | 0.5% | +0.825 |

**Fleet and tech alone add 0.94 VP-equivalents to a ~3.7-VP objective — a 25% surcharge with nothing
to do with points**, before the unscored public/secret terms, which this tool does not report.

Two asymmetries nobody designed:

- **The tech term is a 4.6× differential subsidy** — Jol-Nar banks 0.467/game, Xxcha 0.102.
- **The monoculture penalty is differential the other way** — Jol-Nar takes warfare 73.7% of its
  picks and trips the >80%-of-≥3-plays condition in 26.7% of games (≈0.27/game); Xxcha and Letnev
  pay ~0.005. Net spread between factions is **0.27 VP-equivalents from shaping alone**, which is
  three times the total gain from 1,700 updates of training (§3).

This is worth holding against the memory that [[jolnar-lags-early-in-stage2]]: the subsidy and the
tax both land hardest on the one faction with a known anomalous learning curve. A mechanism, not a
proof.

---

## 3. The measurement lesson, which matters more than any single number

An earlier 12-map pilot reported u1700 at **+0.299 VP** over its starting weights and u50 at
**+0.162**. An independent **100-map** cohort (fresh seeds 910001000–910001099, same frozen
executable, same five-copy 473312 panel) says:

| checkpoint | mean VP | vs base | paired 95% (100 clusters) |
|---|---:|---:|---:|
| base 473312 | 3.6342 | — | — |
| u50 (7084) | 3.5820 | **−0.052** | [−0.102, −0.002] |
| u1300 (184548) | 3.6603 | +0.026 | [−0.027, +0.079] |
| **u1700 (241428)** | **3.7275** | **+0.093** | **[+0.041, +0.146]** |

**Direction held; magnitude was ~3× overstated and u50 changed sign.** Both 12-map estimates were
~2.6 standard errors high *in the same cohort, in the same direction* — with twelve maps the cohort
itself is the unit of luck, and pairing removes the base level but not a cohort's affinity for a
family of policies. The base level moves too: 3.5463 on the twelve maps, 3.6342 on the hundred.

**Paired-difference sd is 0.270 VP per map cluster.** For 80% power:

| effect | clusters needed |
|---|---:|
| +0.30 | ~7 |
| +0.10 | ~58 |
| +0.05 | ~229 |

Checkpoint-vs-checkpoint within a run rises to sd ≈ 0.34, so +0.10 needs ~89. **100 maps is the
minimum useful cohort; 12 is adequate only for effects three times larger than anything measured.**

Two other facts, both corrections to things I asserted wrongly earlier and later checked:

- **Agendas are seen in training.** `OBS-008f2` built the agenda vote surface deliberately. A stale
  doc comment in `rollout.rs` describing a since-fixed bug says otherwise; do not believe it.
- **The policy plays past its four-round horizon.** 12 maps, same panel, `--rounds 8`: 6.606 VP vs
  3.845 at four rounds (0.826/round vs 0.961). A taper, not a cliff, with no truncations. Only 12
  clusters, so directional.

---

## 4. Selection is the cheapest win available

We have been selecting checkpoints on **clearance**, and the ordering does not transfer: u50 has the
best clearance of the three tested and the worst VP. Any future selection should use the 100-map
paired VP harness (§5). It costs ~8 CPU-minutes per checkpoint and the GPU is idle during it.

---

## 5. The three arms — finished, evaluation in flight

All resumed from `checkpoint-241428`, all on the **same seed block** `2400084816` (the first base the
1,701-update run did not consume: `2400057600 + 16×1701`), 300 updates, checkpoint every 50, one
frozen trainer (`out/arms-20260909/bin/ppo_update-frozen.exe`, sha256 `710B743A89F6…`) whose hash was
re-checked before each arm. Gate before launch: one update from `checkpoint-241428` at seed base
2400084800 reproduced **154,885 decisions**, exactly matching update 1700 of the resumed run.

| arm | change from the current recipe | checkpoints |
|---|---|---|
| `control` | none | `out/checkpoints/arm-control/` |
| `vponly` | `--objective-weight 0 --secret-weight 0 --fleet-weight 0 --tech-weight 0 --strategy-diversity-weight 0 --waste-penalty 1 --r1-bonus 1` | `out/checkpoints/arm-vponly/` |
| `waste1` | `--waste-penalty 1` only | `out/checkpoints/arm-waste1/` |

**In-training (self-play T=2.5, NOT acceptance evidence):**

| arm | VP over last 6 windows | clearance | tactical/seat | waste/seat |
|---|---|---|---:|---:|
| control | 3.229 → 3.245 (flat) | ~85% | 6.410 | 0.059 |
| **vponly** | 3.338 → **3.438** (rising) | ~81% | **7.049** | 0.196 |
| waste1 | 3.300 → 3.277 (flat) | ~85% | 6.837 | 0.168 |

`vponly` separates and it is the predicted mechanism: lower waste penalty → more activity → more VP,
with ~3pp of opening clearance traded away by cutting `r1_bonus`. **`waste1` alone did not separate**,
so if the effect is real it comes from the reward stripping rather than the waste penalty by itself —
the opposite of my earlier bet.

**Acceptance evaluation is running now** and resumes safely (it skips existing logs):

```powershell
powershell -File C:\Users\Niko\.claude\jobs\cbbb6004\tmp\arm_eval.ps1
# or re-create it: frozen crossplay exe, 5x473312 panel, seeds 910001000..910001099,
# --rounds 4 --temperature 0.001, one invocation per map, into out/vp-arms-20260909/
```

Harness stability is verified: the `start` arm re-measured `checkpoint-241428` at **3.7275**,
identical to four decimals to this morning's independent run on the same seeds.

**Read the result against `checkpoint-241428` (3.7275), not against 473312** — every arm resumed
from it. At sd 0.27 and 100 clusters, ±0.05 is resolvable.

---

## 6. What I would do next, in order

1. **Finish the arm evaluation and decide on VP, not clearance.** If `vponly` clears +0.05 over
   `start`, the reward stripping is real and the recipe should change.
2. **Run a control replicate.** `C:\Users\Niko\.claude\jobs\cbbb6004\tmp\control_b.ps1` is written
   and unrun: identical recipe, fresh seed block `2400089616`, ~95 GPU-minutes. **Without it there
   is no estimate of same-recipe variance, so the arms are a screen and not a test.** I recommended
   this before launching the arms and then failed to queue it; it is the single biggest hole.
3. **Decompose `vponly` if it wins.** It changes seven coefficients at once. The combined swing was
   deliberate — single-term ablations cannot clear ±0.05 noise in 300 updates — but a winner needs
   decomposing before it becomes the recipe.
4. **Persist optimizer state.** The schema-7 bundle carries none, so every resume cold-starts Adam.
   That confounds every arm and throws away 1,700 updates of curvature. It must be a *sibling*
   artifact — `INFERENCE_FILES` is a closed five-name list and changing it invalidates every
   checkpoint.
5. **Do not** start GAE, more stage-1 work, or a new waste penalty. See
   `plans/TRAINING_VP_IMPROVEMENTS_2026-09-09.md` §6 and `plans/VP_EVIDENCE_CHECK_2026-09-09.md` §4.

---

## 7. Traps that cost time today

- **`git worktree remove --force` is globally forbidden** — it followed a junction and destroyed
  `out/` on 2026-09-08. See [[never-git-worktree-remove-force]].
- **Never benchmark the working tree.** ~32 files of other sessions' uncommitted work change the
  engine and the observation. Clean checkouts at the two commits, own `CARGO_TARGET_DIR` each,
  compare update 0 only. Three timing numbers were wrong before this was understood.
- **Never run `cargo clippy --fix` with a package-wide scope here.** It rewrote committed shared
  code (`ti4-mlp/src/bot.rs`, the training hot path) as a side effect of fixing an example.
- **`GIT_COMMIT` must be set** or `ppo_update` refuses at checkpoint-write time, after doing the work.
- **Separate `CARGO_TARGET_DIR` for CUDA** (`target-cuda`): build.rs stages libtorch DLLs beside the
  binary and one shared `target/` lets the last build win. An already-built trainer silently lost
  CUDA this way.
- The `behaviour_report` tech/fleet block only exists in the **working tree**, not in a clean
  checkout; and it refuses the Validation pool by role, correctly — it is a Train-pool census.

---

## 8. Artifacts

| Path | What |
|---|---|
| `out/checkpoints/arm-{control,vponly,waste1}/` | 6 checkpoints each, 300 updates |
| `out/arms-20260909/arm-*.log` | Full training logs, command and trainer hash in the header |
| `out/arms-20260909/bin/ppo_update-frozen.exe` | The trainer all three arms used |
| `out/vp-arms-20260909/` | Arm acceptance evaluation (in flight) |
| `out/vp-confirm-20260909/` | 100-map cohort, and the terminal-holdings census |
| `out/vp-ladder-20260909/` | The 12-map intermediate ladder |
| `out/vp-longgame-20260909/` | The 4-vs-8-round probe |
| `out/training-review-20260909/` | Astra's frozen executables and raw logs |
| `plans/VP_EVIDENCE_CHECK_2026-09-09.md` | The 12-map vs 100-map analysis in full |
| `plans/TRAINING_APPROACH_2026-09-09.md` | The recipe as it stood this morning |

Best checkpoint on measured VP remains `out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428`
until the arms are evaluated. It is also what the TTS bridge plays — see
`plans/TTS_BRIDGE_REVIEW_2026-09-09.md` and the reviewer's findings, which are addressed but not
re-reviewed.
