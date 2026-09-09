# Situation report — Stage-1 retraining needs design help (2026-09-05)

## Requested outcome

Train a schema-7 Stage-1 MLP policy that, under greedy evaluation, reaches:

- at least **95% clearance**; and
- at most **0.5% of seat-games with any wasted activation**.

The authoritative convention is greedy `--temperature 0.001`, 600 seeds × 6 rotations (21,600
seat-games). Clearance uses the Validation pool; waste uses the Train pool because it is measured
while generating a corpus. Do not compare these numbers to PPO's sampled rollout telemetry.

## Why a fresh policy was required

The OBS-002–012 observation-contract rework moved the bundle to schema 7 and vocabulary generation
`fa3d6f9...` (OOV registry v10). All earlier champions are schema 6 and are deliberately refused
by the loader. There is no schema migration path, so their checkpoints and trajectory corpora
cannot be reused.

## Best result so far

Training PPO from a blank schema-7 bundle with waste penalty 5 present *from update zero* was the
only approach that improved clearance and waste together.

`out/checkpoints/blank-waste-mine-p5/checkpoint-59540` (1,600 PPO updates):

- clearance: **93.40% ± 0.33** greedy;
- any-waste: **2.31%** greedy;
- waste/tactical: **0.008**;
- all six factions cleared above 90%.

It still missed both requested thresholds, but it was clearly better than all other tested arms.

## PPO continuation regressed

The same fixed recipe was continued for 450 updates, publishing:

`out/checkpoints/blank-waste-mine-p5-continued/checkpoint-17636`.

Its greedy Train-pool corpus measurement was:

- clearance: **91.24%**;
- any-waste: **5.19%**;
- waste/tactical: **0.017**;
- 18,750 clearing, zero-waste trajectories available from 21,600 seat-games.

An earlier continuation checkpoint at 200 added updates was also poor: 90.45% ± 0.39 Validation
clearance and 2.29% any-waste. Fixed-penalty PPO was stopped after the latest checkpoint rather
than continued further.

## Current experiment: BC after waste-aware PPO

Hypothesis: prior BC attempts failed because they cloned a weak plain-PPO policy. The latest
waste-aware PPO policy has 18,750 greedy clearing, zero-waste demonstrations, so perhaps BC could
be a legitimate refinement pass.

Current command (still running when this report was written):

```powershell
corpus_train --bundle out/checkpoints/blank-waste-mine-p5-continued/checkpoint-17636 \
  --replay-bundle out/checkpoints/blank-waste-mine-p5-continued/checkpoint-17636 \
  --corpus out/corpus/positive-bc-latest-greedy \
  --waste-ceiling 0.5 --epochs 24 --per-epoch 1200 --batch 120 \
  --learning-rate 1e-5 --eval-seeds 600 --out out/checkpoints/clone-bc-latest
```

The acceptance gate is strict: no checkpoint may be selected unless greedy validation any-waste is
at most 0.5%.

### BC result through epoch 7

| checkpoint | Greedy validation clearance | Greedy validation any-waste |
|---|---:|---:|
| PPO baseline | 90.76% | 5.08% |
| BC epoch 1 | 90.64% | 5.47% |
| BC epoch 2 | 90.50% | 5.81% |
| BC epoch 3 | 90.42% | 6.20% |
| BC epoch 4 | 90.31% | 6.68% |
| BC epoch 5 | 90.06% | 7.16% |
| BC epoch 6 | 89.77% | 7.66% |
| BC epoch 7 | 89.73% | 8.15% |

The supervised loss improves monotonically (0.8044 → 0.7719), while both actual game metrics
worsen monotonically. No BC checkpoint has passed the 0.5% gate.

Independent greedy Validation clearance at epoch 7, measured by `clearance_eval --temperature
0.001 --seeds 600`:

| Faction | Clearance |
|---|---:|
| Hacan | 90.78% ± 0.95 |
| Jol-Nar | 88.81% ± 1.03 |
| L1Z1X | 94.19% ± 0.76 |
| Letnev | 82.69% ± 1.24 |
| Sol | 91.64% ± 0.90 |
| Xxcha | 90.25% ± 0.97 |
| Overall | 89.73% ± 0.40 |

Letnev is the clearest early collapse.

## Interpretation, but not a resolution

The BC data is clean; the objective is incomplete. BC rewards taking the recorded action in the
recorded state, with five opponents replayed under the frozen generating policy. As weights change,
the trained policy visits states not represented in the zero-waste corpus. Pure imitation has no
reward signal that says how to recover there, preserve opening clearance, or avoid waste. It can
therefore reduce cross-entropy while worsening greedy play. This is a plausible distribution-shift
explanation, but it is not a sufficient solution.

## Known failed approaches

1. **Plain PPO from blank, then BC**: BC lowered waste but plateaued/regressed clearance around
   89–90%.
2. **Large waste penalty retrofitted onto a trained policy**: waste became excellent, but clearance
   collapsed (including a severe Letnev failure).
3. **Fixed waste-penalty PPO continued past the 1,600-update sweet spot**: clearance and waste
   both regressed.
4. **BC after the latest waste-aware PPO**, reported above: immediately regresses both metrics.

## Call for help

We need a stronger training-design review, not another blind sweep. Please answer these concrete
questions:

1. How should PPO and imitation be combined so that clean trajectories improve policy quality
   without off-distribution collapse? Candidates include PPO+BC auxiliary loss, KL anchoring to the
   PPO actor, conservative/advantage-weighted BC, DAgger-style recollection, or another method.
2. What objective or curriculum can reach the much stricter **0.5% any-waste** target without
   recreating the clearance collapse of a large waste penalty?
3. How should acceptance be enforced by faction? Letnev collapses first, so an overall score alone
   is inadequate.
4. Is the Stage-1 reward missing a necessary dense signal for productive tactical actions, making
   waste minimization structurally adversarial to clearance?
5. What is the smallest controlled experiment worth running next, including exact starting
   checkpoint, loss terms/weights, update budget, and greedy evaluation gates?

Do not treat PPO rollout reports or BC cross-entropy as success criteria. The only acceptance
criterion is greedy, held-out faction-level clearance plus greedy waste incidence.

## Relevant artifacts

- `plans/HANDOVER_2026-09-05_STAGE1_RETRAIN.md`
- `plans/STAGE1_TRAINING_TECHNIQUES.md`
- `out/checkpoints/blank-waste-mine-p5/checkpoint-59540` — best pre-continuation asset
- `out/checkpoints/blank-waste-mine-p5-continued/checkpoint-17636` — latest PPO asset
- `out/corpus/positive-bc-latest-greedy` — latest clean greedy BC corpus
- `out/clone-bc-latest.log` — live BC epoch log
- `out/clearance-clone-bc-latest-epoch-7.log` — independent faction-level greedy result
