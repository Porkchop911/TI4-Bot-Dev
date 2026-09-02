# What stage 2 should take from stage 1

Written 2026-09-02, at the end of a long stage-1 session that moved held-out greedy clearance from
93.58% to 93.88% and produced far more method than result. The method is the part worth carrying.

This is not a summary of what happened. It is the set of things that, if they have to be learned
again in stage 2, will cost the same weeks they cost here.

## The measured noise floor, which is the single most useful number here

One PPO arm, three replicates, differing **only** in the rollout seed base. Everything else held:
starting policy, temperature, learning rate, penalty, update count. Greedy, 3,600 seat-games per
faction, 21,600 on the table.

| | replicate 1 | replicate 2 | spread |
|---|---|---|---|
| hacan | 90.53% | 90.94% | 0.41 |
| jolnar | 93.83% | 95.31% | 1.48 |
| l1z1x | 95.00% | 95.17% | 0.17 |
| **letnev** | 87.53% | 91.00% | **3.47** |
| sol | 94.11% | 94.14% | 0.03 |
| **xxcha** | 96.28% | 93.14% | **3.14** |
| **table** | **92.88%** | **93.28%** | **0.40** |

**A per-faction difference smaller than ~3.5 points between two single runs is nothing.** The
measurement interval is only ±0.6–1.1, so this is not sampling error in the evaluation — it is the
training run itself landing somewhere different.

**The table average is an order of magnitude steadier**, at 0.40 against a ±0.33 interval, because
six factions' noise partly cancels. Compare tables; treat a faction column as one sample.

Stage 2 has longer games, more rounds and self-play opposition, so assume this floor is *higher*
there until measured. Measuring it is the first experiment, not a later refinement.

## Rules, not preferences

1. **Measure the noise floor before any comparison.** The worst process failure of this session was
   comparing single runs to single runs for hours. One faction read 80.67%, 90.47% and 99.22% across
   three arms before anyone asked what two identical arms would read.
2. **Paired and clustered.** Evaluate both policies on identical seeds; bootstrap by resampling
   **map seeds**, carrying all 36 of a map's paired seat-games with it. Unpaired intervals on the
   same data were ±0.33; paired-clustered were ±0.14, because map difficulty cancels.
   `scripts/paired_cluster_bootstrap.py` needs only its outcome type changed for a graded objective.
3. **Greedy is the only scale-invariant reading.** Training at temperature `T` optimises
   `softmax(s/T)`, so an arm's logit *scale* depends on the temperature it trained at, and a fixed
   non-zero evaluation temperature does not put two policies on one footing. `argmax(s) =
   argmax(s/c)`. Read at 0.25, one policy looked eight points worse than it was.
4. **Never compare a count against an incidence.** `E[wastes per seat-game]` and
   `P(at least one waste)` can reorder freely. Reporting them separately produced a whole wrong
   conclusion.
5. **Every shaping term will be satisfied the cheapest way available.** A penalty on wasted
   activations was answered by taking 47% fewer tactical actions. Report a degenerate-solution
   indicator *beside* every shaped quantity — `tactical/seat` next to the waste rate — or the
   collapse is only visible after the run. Stage 2 has six shaping coefficients and correspondingly
   more room for this.
6. **Correlation on a behavioural metric proves nothing.** Waste was higher in failed games than
   cleared ones (68.8% vs 56.2%). Removing 98% of it made clearance *worse*. The causal test is
   cheap and the correlation was misleading.
7. **Prove the instrumentation inert.** A decider wrapper added to record decisions was verified
   identical over 2,880 seat-games and 146,513 decisions before its numbers were trusted.
   `wrapper_identity` is the pattern.

## The one concept that must not transfer

**Counterfactual labels conditioned on a fixed downstream policy.**

`P(clear | do(a_i = a'), π) = 1` is true only for the π that played the rest of the round. Training
raises `a'`, which changes π, which changes the continuation — and the outcome that justified the
label no longer follows. The label describes a policy that no longer exists by the time it has been
learned.

This was tested properly and failed comprehensively: 2,433 causally-attributed repairs (54× a
previous attempt, with correct attribution this time), held-out clearance 93.96% → 12.94%
unanchored; with a KL trust region swept over two orders of magnitude the best result was +0.01pp;
and the trained policy did not fix even the failures it was trained on (682 → 717).

Stage 2's longer horizon makes this strictly worse. Any proposal of the form "this decision caused
the win, so upweight it" is the same defect in different clothes. Whole-trajectory imitation does
**not** have this problem — a complete demonstration claims only that the whole line worked, which
is what was actually observed.

## What worked, and is worth trying first

**Behaviour cloning on a filtered corpus of successful trajectories.** The only intervention this
session that moved clearance with a proper interval behind it: **+0.45pp, paired map-cluster
bootstrap 95% CI [+0.31, +0.58], sign-flip p = 0.0001**.

It is also stable in a way nothing else was — it never destroyed the policy, at any setting.

Three details that made it work, all of which cost a debugging cycle to find:

- **The corpus stores specifications, not features.** Seed, rotation, faction, temperature, and the
  option id at every non-forced decision. The engine is deterministic, so replay regenerates features
  under whatever model is training. A feature dump would be ~300M numbers pinned to today's
  vocabulary.
- **The temperature is part of the specification.** A trajectory is a line played against five
  particular opponents sampling at that temperature; replaying at any other setting faces different
  opponents and the line does not exist. Omitting it failed 59 of 60 replays.
- **Replay uses the frozen generating policy, never the one being trained.** Otherwise the opponents
  drift as training proceeds, replay failures climb — 176 → 876 over twelve epochs — and the
  surviving trajectories are the ones least sensitive to the change, so the dataset biases itself in
  the flattering direction.

The stage-2 analogue needs a definition of "successful trajectory" for a graded objective. That
definition is a filter on training data and deserves the same causal scrutiny as any other: run
`decision_criticality` on it before trusting it.

## Tools that carry over

| tool | change needed for stage 2 |
|---|---|
| `clearance_eval --per-seat` | emit per-seat VP instead of a cleared bit |
| `scripts/paired_cluster_bootstrap.py` | continuous outcome instead of binary |
| `scripts/variance_study.ps1` | point at the stage-2 arm |
| `positive_corpus` (format) | new admission filter; format is horizon-agnostic |
| `corpus_train` | new "successful" definition |
| `decision_criticality` | none — arguably more valuable with more decision types |
| `wrapper_identity` | none |
| `failure_census` | new failure definition |

## Diagnostics worth running early, before any training

**Decision criticality.** Substitute one legal alternate at one index of a *successful* line and ask
whether it still succeeds. In stage 1 this showed that the heads with the worst imitation agreement —
tokens 39.6%, trade 61.0% — are heads whose decisions are ~100% **free**: every alternate still
cleared. Their low agreement was not weakness, and imitating them teaches noise. The load-bearing
heads turned out to be exactly the ones with the *highest* agreement.

Without this, an agreement metric will send you after the wrong heads. With more decision types in
stage 2, the trap is larger.

## What was tested and is not worth repeating

- **Relational / graph architecture.** Tested cheaply and unsupported. On positions the policy
  *fails*, it already ranks the winning activation first 88% of the time, mean rank 1.18 of 35
  options; movement 90.6%, landing 91.9%. The spatial and relational heads are its strongest. Note
  the test was partly circular — the demonstrations were sampled from the policy itself — so this is
  "no positive evidence", not a refutation. Do not build it first.
- **Prefix-branching rescue search.** Predicted to beat whole-line hot sampling because the latter
  diverges at decision zero; it did not (58.8% against ~65%). Late branch points preserve the play
  that produced the failure.

## Carrying the stage-1 policy forward

Initialize stage 2 from the stage-1 champion rather than from scratch. The stage-2 reward already
carries a `clearance_weight` term precisely so opening quality is not traded for mid-game points,
which only means anything if the opening is there to begin with.

`plans/CHAMPIONS.md` records which checkpoints are worth keeping, where the durable copies live, and
which measurement convention each number was taken under — three were used this session and two of
them differ by about half a point.

## The honest state at handover

Held-out greedy clearance **93.88%**. The target was lowered from 99% to 95% and was not reached.
Search establishes a constructive lower bound of 97.4% on what is reachable — a lower bound, not a
ceiling; nothing proves the remainder unreachable.
