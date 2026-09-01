# The MLP temperature regime

Written 2026-09-01, after looking for this and failing to find it written down anywhere.

`MlpBot` samples `p = softmax(s / temperature)` and defaults to `1.0`. Six tools override that
default, each with a different value, and the values are not arbitrary — they encode three
different jobs. Until now that knowledge lived only in six `map_or` calls and a handful of `//!`
paragraphs, which is why it was possible to run training at the wrong setting and to look for the
regime in `plans/` and conclude it did not exist.

## The three temperatures

| Temperature | Job | Where it is the default |
|---|---|---|
| **0.25** | **Measuring a trained policy.** Near-greedy: read what the policy actually believes, with sampling noise damped so a number reflects the weights rather than the draw. | `space_station_reliance`, `failed_openings`, `empty_activations` |
| **1.0** | **Training, and the distillation student.** Every run to date; pinned for distillation. | `ppo_update` (implicit until 2026-09-01), `opening_failures`, `opening_plan` |
| **2.5 – 3.5** | **Searching for alternatives.** Deliberately hot, to find lines the policy assigns too little probability to. | `rescue_imitation` (2.5), `opening_reachability` (2.5; run at 3.5 in `out/reach_hard.log`) |

## Why each value is what it is

**0.25 — evaluation.** A clearance figure sampled at 1.0 measures the policy *and* the dice. At
0.25 the softmax is sharp enough that the reported number is close to what the policy would do if
asked to commit, which is the quantity a champion comparison wants. This is the setting the
space-stations audit used to establish that 6.2% of measured clearances rested on an illegal move.

**1.0 — training, historically.** Every run before 2026-09-01 used it, and `ppo_update` had no way
to use anything else. The reason usually given is exploration: PPO is on-policy, the batch is
whatever the policy sampled, and sharpening the distribution narrows what the update can learn
from. That is a real effect but a matter of degree, and it is **not** what the 0.25 runs measured.

What they measured was a defect. `MlpBot` sampled at `softmax(s / T)` and recorded that as the
behaviour probability; the optimiser scored `logits.log_softmax(...)` with **no division**, at
`ppo.rs:437` and `ppo.rs:628`. So `pi_new` and `pi_behaviour` were different functions, every
importance ratio in every batch was wrong, and PPO's clip bounded a quantity that meant nothing.
At `T = 1.0` the division is by one, so the omission was invisible for every run that had ever
been made. Fixed 2026-09-01: `Step` now records its temperature, both scoring paths divide by it,
and two tests hold the invariant `pi_new / pi_behaviour == 1` under unchanged weights at 1.0, 0.25
and 2.5 — one per scoring path, each probed against its own path's reversion.

**So the 0.25 collapse below is not evidence that low-temperature training is bad.** It is evidence
that the flag was unsound. Whether 0.25 trains well is now an open question that can be asked
honestly, and has not been.

`student_temperature` is separately pinned to exactly `1.0` in the schema-6 bundle manifest and
checked by `teacher_corpus::validate` — a distilled bundle captured at any other value is refused.

**2.5–3.5 — search.** `opening_reachability` asks whether a failed opening was *clearable*, by
replaying the position with one seat sampling hot and everyone else on their original stream. The
answer is a lower bound: finding a line proves the position was winnable, failing to find one
proves only that this search did not. `rescue_imitation` then clones the **first divergent
decision** from those rescues and nothing else — because a 2.5 trajectory is mostly *worse* than
the champion's and cleared anyway, so cloning all of it teaches the policy to be more random.

## Exploration during training is not the temperature knob

This is the part that is easiest to get wrong, and I did.

Every PPO run before the 2026-09-01 engine work — 018, 019, 020, 021, 022, 023 — was invoked with
`--movement-entropy 0.05`, five times the 0.01 default, and run-018 additionally set
`--entropy-final 0.05`. None of them passed a temperature, because `ppo_update` had no
`--temperature` flag until 2026-09-01.

So training exploration was tuned through the **entropy coefficient on the movement head**, not
through temperature. The invocations are recorded in `out/train_run0*.log`, which is the only place
this was written down.

| Run | Invocation |
|---|---|
| 018 | `--movement-entropy 0.05 --entropy-final 0.05` |
| 019 | `--movement-entropy 0.05` |
| 020 | `--movement-entropy 0.05` |
| 021–023 | header shows `entropy 0.01/0.1 (movement 0.05)` |

## What happened when this was not followed

`run-026` and `run-027` trained at `--temperature 0.25` with the default `movement-entropy 0.01`
— the evaluation temperature, with the training exploration knob left at its untuned default, which
is pushing hard in the opposite direction from the established regime.

These runs all predate the ratio fix, so their 0.25 columns show a broken optimiser, not a
temperature. The 1.0 columns are unaffected — division by one.

| | run-025 (1.0, 0.01) | run-026 (0.25, 0.01) | run-028 (1.0, **0.05**) |
|---|---|---|---|
| clearance @ update 50 | 28.61% | **0.01%** | see run |
| `\|log r\|` | 0.049 | 0.117 | 0.044 |
| clipped | 4.5% | 14.1% | 3.8% |
| lowest head entropy | 0.53 | **0.002** (movement, by update 39) | 0.43 |

The movement head reached entropy 0.002 by update 39 — effectively deterministic, from a blank
model, before any evidence justified it. run-030, resumed at 0.25 from the run-028 champion, fell
from 88% to 2.61% over 650 updates with 43 heads at exactly 0.0000 entropy, while `|log r|` read
0.071 and 6.8% clipped — both perfectly healthy numbers, both computed *from the broken ratio*.
That is the shape of this defect: the statistic that would have caught it was derived from it.

It did produce one thing of value. A livelock in the component-action path (four sites returning
`Ok(())` on a failed action without consuming the turn) had been costing roughly one self-play game
in twenty thousand at temperature 1.0. At 0.25 the policy picks the same failing option every time,
so it surfaced within 50 updates and was fixed in `3a4566c`. **Lowering the temperature did not
cause that bug; it removed the randomness hiding it.** A near-greedy run is a good way to find
livelocks, and a bad way to train.

## Rules of thumb

- **Training**: temperature 1.0, `--movement-entropy 0.05`.
- **Measuring a champion**: temperature 0.25.
- **Searching for what a policy is missing**: 2.5 or hotter, and clone only the first divergent
  decision.
- **Training**: 1.0 with `--movement-entropy 0.05` is the only setting with a clean result behind
  it (run-028, 91.3% measured at 0.25 on held-out seeds). Anything else is untested, not refuted.
- **Never** set a sampling temperature on a PPO run without confirming the recorded behaviour
  probabilities *and the optimiser's scoring* use the same value. Confirming only the bot side is
  what produced the 2026-09-01 defect; the bot side was correct throughout.
