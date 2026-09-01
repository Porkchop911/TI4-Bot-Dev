# Experiment: what does the PPO sampling temperature actually do?

Designed 2026-09-01, after the ratio defect in `debcecb` made the question askable for the first
time. Nothing in this document has been run yet; results go in `## Results` at the bottom, and the
design above it is not to be edited to match them.

## The question

`MlpBot` samples `p = softmax(s / T)`. Every PPO run in this project's history used `T = 1.0`,
because `ppo_update` had no other option until 2026-09-01. Two runs at `T = 0.25` then collapsed,
and the collapse was diagnosed — correctly — as a defect: the optimiser scored `softmax(s)` against
a behaviour recorded at `softmax(s / T)`, so the importance ratio compared two different functions
and PPO's clip bounded nothing. Fixed in `debcecb`.

With the ratio correct, run-031 (0.25, resumed from the run-028 champion) no longer collapses. It
decays gently instead: 91.79% at update 10 down to an 81–86% band, where run-030 on the same seeds
went to 2.61%. So the collapse is explained. **What temperature does to training is not.**

The three specific questions:

1. Does training at `T < 1` degrade the policy, or was run-031's decay just "further stage-1
   training from a converged checkpoint degrades it at any temperature"? **There is no control.**
   This is the gap that makes run-031 uninterpretable on its own.
2. Does `T > 1` help? Hotter sampling is the standard exploration lever and this project has never
   tried it in training — only in `opening_reachability` and `rescue_imitation`, which search rather
   than train.
3. Is 1.0 actually the right default, or merely the untested one?

## Why the obvious version of this experiment is wrong

Three confounds, and the second is the one that would have wrecked the result silently.

### 1. The in-run clearance table is not a measurement

`ppo_update` tallies clearance from the rollouts the update was computed from: training maps,
training seeds, and **sampled at the training temperature**. A policy scores higher at 0.25 than at
1.0 because the softmax is sharper. Reading a sweep off those tables compares the measuring
instrument, not the policies.

*Handled by:* `crates/ti4-mlp/examples/clearance_eval.rs`, written for this experiment. Fixed
temperature regardless of how the bundle trained, the Validation pool, a held-out seed range at
900000000 (every training run to date consumed 650000000 upward), and the same
`play_with_decider_factory` / `DEFAULT_REQUIREMENT` path `ppo_update` tallies, so the bar is not
redefined. 2,160 seat-games in 3.7s alongside a running training job.

**Every arm is measured at `T = 0.25`, on the same seeds, whatever it trained at.** One instrument,
one scale.

### 2. Temperature is also a learning rate

This is not obvious and it is the reason `--learning-rate` was added to `ppo_update`.

The optimiser now scores `log softmax(s / T)`. Differentiating with respect to the logits:

```
d/ds  log softmax(s / T)_a  =  (1 / T) (e_a - p)
```

The gradient reaching the logits is scaled by `1/T`. Training at 0.25 applies **4x** the gradient
of training at 1.0; training at 2.5 applies **0.4x**. A naive sweep therefore varies exploration and
effective learning rate together, and if the arms differ, the experiment cannot say which one did
it.

*Handled by:* two arm families.

- **Naive** arms hold the learning rate at the default `3e-4`. This is what an operator actually
  gets from typing `--temperature`, so it is the practically useful answer.
- **Compensated** arms scale the learning rate by `T` (`3e-4 * T`), cancelling the `1/T` to first
  order and leaving exploration as the only difference. This is the scientifically clean answer.

Adam complicates the cancellation — it normalises by a running second moment, so a uniform gradient
rescaling is partly absorbed, and the compensation is approximate rather than exact. It is still far
better than not compensating, and the approximation is stated here rather than assumed away.

### 3. The entropy bonus does not mean the same thing at every temperature

The loss carries `- coefficient * H(p)`, and `p` is the tempered distribution. A sharp distribution
has low entropy, so at 0.25 the bonus has more room to push and pushes harder; at 2.5 the
distribution is already flat and the bonus does almost nothing. The coefficients (0.01 general, 0.10
strategy, 0.05 movement) were tuned at `T = 1.0`.

*Not fully handled, deliberately.* Compensating for it would mean rescaling three coefficients by a
factor with no principled form, which is a second free parameter dressed as a control. Instead it is
**measured**: every arm logs its lowest head entropy per update, and an arm whose clearance falls
while head entropy climbs is being flattened by the bonus rather than by exploration. Run-031 shows
the shape to look for — entropy 0.105 → 0.25, then a plateau, with clearance still drifting down,
which is what ruled the bonus out as the whole story there.

## Design

**Fixed across every arm.** Changing any of these invalidates the comparison.

| | |
|---|---|
| start | `out/checkpoints/run-028/checkpoint-60672` — the champion, 91.3% held-out |
| seed base | 650000000 (identical rollout seeds in every arm) |
| updates | 500 (see below) |
| stage / rounds | 1 / 1 |
| movement entropy | 0.05 (the setting behind every good run: 018–023, 028) |
| entropy final | 1 (no annealing, so entropy is not a second moving part) |
| device | cuda optimiser, CPU rollouts (7.1) |
| report cadence | 10 |
| commit | recorded in each manifest via `GIT_COMMIT` |

**The arms.**

| arm | T | learning rate | what it asks |
|---|---|---|---|
| A-025 | 0.25 | 3e-4 | near-greedy training, as an operator gets it |
| A-050 | 0.5 | 3e-4 | is the effect monotone below 1? |
| **A-100** | **1.0** | **3e-4** | **the control.** Identical to run-031 but at the historical default |
| A-150 | 1.5 | 3e-4 | mildly hot |
| A-250 | 2.5 | 3e-4 | the `opening_reachability` search temperature, used to train |
| C-025 | 0.25 | 7.5e-5 | A-025 with the `1/T` gradient scaling cancelled |
| C-250 | 2.5 | 7.5e-4 | A-250 with the same cancellation |

A-100 is the arm that makes run-031 interpretable, and it is the one to run first if only one is
run. Without it, "0.25 decays" and "any further training from this checkpoint decays" are the same
observation.

Note that A-100 is *not* a re-run of run-028: it resumes from run-028's own output. A converged
policy given 900 more updates at its own temperature may well decay, and if it does, that is the
finding and it applies to every other arm equally.

**Measurement.** After each arm, for the initial bundle and every checkpoint:

```
clearance_eval --bundle <checkpoint> --temperature 0.25 --seeds 400 --seed-base 900000000
```

400 seeds x 6 rotations = 2,400 games, and 6 seats each = 14,400 seat-games per point, a 95% half-width of about ±0.5 points at
these rates. The half-width is printed; treat a gap no larger than it as no gap. It assumes seat
independence, which is not quite true because six seats share a map, so it is a lower bound.

**Why 300 updates and not 900.** The pilot settles this. run-031 ran 900 and reached its final
level by update 200:

| after | clearance at 0.25, held out |
|---|---|
| start | 92.49% ±0.43 |
| 100 | 86.96% ±0.55 |
| 200 | 83.19% ±0.61 |
| 300 | 84.17% ±0.60 |
| 500 | 84.30% ±0.59 |
| 900 | 83.91% ±0.60 |

Updates 200 to 900 are seven hundred updates whose every reading sits inside the others' intervals.
**500** leaves comfortable margin past that plateau, with room for an arm that travels more slowly
per update than the pilot did, and still costs well under half of 900.

The arm this could shortchange is **A-250**, whose 0.4x gradient scaling makes it travel less per
update, so a flat A-250 at 300 updates is ambiguous between "hot does nothing" and "hot moves
slowly". C-250 is precisely the arm that resolves that, which is another reason not to drop the
compensated family. If both hot arms are still visibly moving at 300, extend those two rather than
lengthening every arm: `-Arms A-250,C-250 -Updates 1500`.

Checkpoints are measured every 5th (a point per 50 updates) plus the final one.

**Replicates.** `-Replicates n` runs each arm `n` times, differing **only** in the rollout seed
base (shifted by `r * 100000000`). This matters more than it looks: `clearance_eval`'s half-width is
sampling error in the *evaluation* and says nothing about run-to-run variation in the training,
which is the larger of the two and the one that decides whether a two-point gap between arms means
anything. One replicate gives a between-arm difference with no sense of what a within-arm difference
looks like. Default is 1; the first pass runs at 1 and a second replicate is the right follow-up for
any pair that lands close.

**Cost.** ~15 min of training per arm (500 updates at ~1.85s) plus ~4 min of measurement (11 points
at ~20s). Seven arms is about **2h15** per replicate. Arms run sequentially: they contend for the
same GPU and all CPU cores.

## What would count as an answer

- **"0.25 is bad for training"** requires A-025 to end materially below A-100 on the same
  instrument, *and* C-025 to agree. If A-025 is worse and C-025 is not, the finding is about
  learning rate, not temperature, and the honest statement is that `--temperature 0.25` is bad
  because of what it does to the gradient.
- **"Hot helps"** requires A-150 or A-250 to end materially *above* A-100. Given that A-100 starts
  at a converged champion, "above" is a strong claim and "does not decay while A-100 does" would
  already be informative.
- **"1.0 is merely the untested default"** is the outcome where several arms land inside each
  other's intervals. That is a real result and would mean the temperature knob does not matter much
  once the ratio is correct — which, given that the whole 0.25 story so far was a bug, is a live
  possibility that should not be argued away.

A null result is reported as a null result.

## Threats to validity, stated up front

- **One starting checkpoint.** Everything here is conditional on run-028's champion. A
  from-scratch sweep would be a stronger experiment and costs far more; if the results are close,
  that is the follow-up rather than a re-reading of these.
- **Approximate compensation.** As above: Adam partly absorbs a uniform gradient rescale.
- **The entropy bonus is measured, not controlled.**
- **Held out, but not sealed.** The Validation pool has already informed architecture and
  thresholds, per `artifacts.rs`. The sealed `Final` pool is reserved for M10-038 and is not touched
  here.
- **Seat independence in the interval.** A lower bound, as noted.
- **Vocabulary drift.** The 11,147-slot vocabulary was generated at `b0ad876`, before this
  session's engine work; five new cards land in OOV slots. This affects every arm identically, so
  it does not threaten the comparison, but it does cap the absolute numbers.

## How to run it

`scripts/temperature_sweep.ps1` runs the arms in order and evaluates each checkpoint, writing
`out/sweep/<arm>.log` and `out/sweep/<arm>.eval`. It refuses to start if `GIT_COMMIT` is unset,
because a checkpoint manifest without a commit cannot be traced back to the code that made it.

Run A-100 first and read it before committing three more hours:

```powershell
./scripts/temperature_sweep.ps1 -Arms A-100
```

## Results

### Pilot: run-031, which is A-025 in all but name

Run before this document, from the same checkpoint, the same seeds, the same 900 updates, the same
`--movement-entropy 0.05`, at temperature 0.25 and the default learning rate. The only difference
from a formal A-025 is that `--learning-rate` did not exist yet, and A-025 does not set it. Log:
`out/mlp_stage1_run031.log`; checkpoints under `out/checkpoints/run-031`.

Measured by `clearance_eval` at 0.25, 400 seeds, Validation pool:

| | clearance | mean VP |
|---|---|---|
| start (`run-028/checkpoint-60672`) | **92.49% ±0.43** | 0.071 |
| after 900 updates (`checkpoint-25652`) | **83.91% ±0.60** | 0.133 |

**−8.58 points**, roughly eight times the combined half-widths. Naive 0.25 training degrades the
policy, and the size of it is not in doubt.

What the pilot **cannot** say, and the reason the sweep exists:

- Whether 1.0 would degrade it too. A-100 is the control and has not been run. A converged policy
  given 900 more updates may decay at any temperature.
- Whether the cause is exploration or the 4x effective learning rate. C-025 separates them.

One observation worth carrying into the sweep: **mean VP rose while clearance fell**, 0.071 to
0.133. The stage-1 reward carries a clear bonus of 22 alongside VP terms, so a policy that trades
opening clearance for victory points is moving *up* its reward and *down* the bar the reward exists
to serve. If that pattern appears across arms it is a statement about the reward, not the
temperature, and it belongs in a different investigation.

### The sweep

Not yet run.
