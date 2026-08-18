# Testing actual learning algorithms

Date 2026-08-18. Yesterday's arena varied features, reward shaping and step size — every arm ran
the **same** rule: undiscounted REINFORCE with a scalar batch-mean baseline. This plans the rule
itself.

Baseline for everything below is **A4 config** (`--clearance-weight 0`), because yesterday showed
the old configuration was actively degrading and anything measured on top of it is measured on
sand.

---

## What the current rule actually is

From `reward.rs::returns` and `gradient.rs`:

```
G_a = Σ_{t ≥ a} r_t                      undiscounted suffix sum
A_a = G_a − mean(G over batch, per head) scalar baseline
∇   = Σ_a A_a · (φ_chosen − Σ_o p_o φ_o) / temperature
```

Three named weaknesses, each of which is a different algorithm to fix:

1. **No discounting.** Every decision is credited with the entire rest of the game — ~190 decisions
   per seat-episode. A decision in round 1 carries round 4's outcome at full weight.
2. **A scalar baseline.** It cannot correct the systematic difference between early decisions
   (large suffix sums) and late ones (small), so the estimator is biased across game-time.
3. **One gradient step per batch.** 96 games are simulated, used once, and thrown away. Simulation
   is ~100% of compute, so this is the most expensive possible way to use data.

## The arms

| | arm | what changes | effort |
|---|---|---|---|
| **B0** | reference | A4 config, unchanged rule | — |
| **B1** | **discounted returns** | `γ` on the suffix sum | ~10 lines |
| **B2** | **per-(head, round) baseline** | baseline bucketed by round instead of one scalar | ~30 lines |
| **B3** | **PPO** | clipped surrogate, K epochs per batch | real work |
| **B4** | **frozen opponents** | one faction learns, five held at the starting champion | moderate |

B1 and B2 are the two halves of the credit-assignment problem and are nearly free. B3 is the one
with a large expected effect: reusing each batch K times is a K-fold increase in learning per
simulated game, and simulation is the entire cost. B4 changes the *problem* rather than the rule —
it removes co-adaptation and makes training match the per-faction gate.

## B3 in detail, because it is the one that needs design

The trajectory already stores what PPO needs: every option's feature vector and the probability
the behaviour policy gave it. So for epoch k > 0:

```
p_new(o)  = softmax over w_k · φ_o
ratio     = p_new(chosen) / p_old(chosen)
surrogate = min(ratio · A, clip(ratio, 1±ε) · A)
∇         = [ratio · A if unclipped else 0] · (φ_chosen − Σ_o p_new(o) φ_o) / temperature
```

The structural change is to `train_factions`: today it reduces each rollout to statistics inside
the parallel map and drops the trajectories. PPO needs them kept across epochs, so the loop
becomes `roll out once → { compute statistics under current weights; apply } × K`.

Memory is the question to settle first, not last. A batch is 96 games × 6 seats × ~250 steps, and
after yesterday's compaction each step holds `Vec<(FeatureKey, f64)>` per option. **Measure the
retained size of one batch before writing the loop** — if it does not fit comfortably, PPO becomes
minibatched and that is a different piece of work.

## Protocol

Same as yesterday, which worked:

- 3 training seeds per arm, shared seed streams so same-index comparisons are paired.
- All arms resume from the same champion.
- Evaluated on the sealed 98M block, 200 seeds = 1,200 games per faction.
- Report across-seed **ranges**, and call a difference a result only when the ranges are disjoint.
- 8 concurrent runs at 4 threads.

**Equal-compute, not equal-updates.** PPO does K gradient steps per simulated batch, so comparing
at equal update counts would hand it a K-fold advantage in data. Arms are compared at **equal
games simulated**, which is the resource that actually costs.

## Order of work

1. B1 and B2 — both small, both in code paths already understood. Launch a 4-arm run
   (B0, B1, B2, B1+B2) while B3 is written.
2. Measure batch retention; write B3.
3. B4 if time allows.

## What would count as a result

- **B1/B2 beat B0** → the credit-assignment diagnosis was right and the fix is cheap.
- **B3 beats B0 at equal games** → the rule was wasting data, and everything else should be
  re-run under PPO before further tuning.
- **B4 beats B0** → the plateau is partly co-adaptation, and the training protocol should match
  the gate.
- **Nothing beats B0** → the optimiser family is not the constraint, and the next move is the
  policy *class* (bilinear or a small MLP over the same features), which is the one axis nothing
  has tested yet.
