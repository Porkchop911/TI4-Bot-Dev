# Testing different learning algorithms

Date 2026-08-18. Supersedes `PLAN_2026-08-18_ALGORITHMS.md`, which mis-scoped the question.

---

## 0. What counts as a different algorithm

Two rounds of experiments have now been filed under "testing algorithms" without testing one, so
the definition goes first and everything below is checked against it.

**Not a different algorithm** — all of these keep the same update, estimated once per batch from
on-policy data:

- changing the reward (`--clearance-weight`, `--high-vp-bonus`) — that changes the *objective*
- changing the features — that changes the *policy class*
- changing step size or the entropy bonus — hyperparameters
- **discounting the return** (B1) — a different advantage, still REINFORCE
- **bucketing the baseline** (B2) — a variance-reduction component of REINFORCE

Yesterday's arena tested the first three. Today's B-arms test the last two. All seven arms are
REINFORCE.

**A different algorithm** changes at least one of:

1. **what the update optimises** — a different objective, not merely a different advantage
2. **how many times data is used** — off-policy or multi-epoch rather than a single pass
3. **where the learning signal comes from** — bootstrapping, search, or no gradient at all

## 1. The candidates

| | algorithm | what makes it different | effort |
|---|---|---|---|
| **P1** | **PPO** | clipped surrogate objective; the importance ratio makes a batch reusable for K epochs (criteria 1 + 2) | medium-high |
| **P2** | **A2C with a learned V(s)** | a second function approximator; advantage from bootstrapping rather than the Monte-Carlo return (criteria 1 + 3) | high |
| **P3** | **Evolution strategies over the weights** | zeroth-order: no gradient, no credit assignment at all (criterion 3, most strongly) | medium |
| **P4** | **Expert iteration with search** | targets come from lookahead and the policy is fitted by supervised cross-entropy, not by a return (criterion 3) | high |
| — | V-trace | off-policy correction for the staleness `--rollout-depth` already creates | needs P2 first |

### P1 — PPO

The one with a mechanically large expected effect. Simulation is ~100% of compute and each batch
of 96 games currently produces **one** gradient step. PPO reuses it K times, so at equal games
simulated it takes K times as many steps.

The trajectory already stores both things it needs: every option's feature vector, and the
behaviour policy's probability for each. Per epoch after the first, the ratio of new to old
probability for the chosen option weights a clipped surrogate, and the gradient is the usual
`phi_chosen - sum_o p_new(o) phi_o` scaled by that ratio, or zero where the ratio is clipped.

**The blocker is structural, not mathematical.** `train_factions` reduces each rollout to
statistics inside the parallel map and drops the trajectory. PPO needs it kept across epochs, so
the loop inverts to `roll out once -> {statistics, apply} x K`. **Measure a batch's retained size
before writing that loop** — 96 games x 6 seats x ~250 steps x ~6 options x ~18 entries. If it
does not fit comfortably, PPO becomes minibatched, which is a different and larger job.

### P2 — actor-critic with a learned value function

The current baseline is a scalar mean; a learned `V(s)` is a state-dependent one, and with it come
TD errors and GAE.

**There is an infrastructure gap to name up front:** every feature in this codebase is per option.
There is no state-only extractor, so `V(s)` has nothing to read. Building one is part of this
arm's cost and is a modelling decision in its own right (`seat_facts` plus a board summary is the
obvious start). This arm is not "add a value head".

### P3 — evolution strategies over the weights

Perturb the weight vector, evaluate each perturbation on a fixed seed block, recombine by
performance. **No gradient, no baseline, no credit assignment** — which makes it the sharpest
available test of whether the diagnosed credit-assignment problems are what actually limits this
system. If ES matches or beats REINFORCE, the gradient machinery is not earning its keep.

It is pure learning: the weights are the same fitted weights and only the search over them
changes. The heuristic-evolution work in the Python archive is *not* this and remains out of
scope.

Its cost is evaluations rather than gradients, and evaluation is now cheap (720 games in about
5 s). 287k parameters is large for ES; the antithetic, rank-normalised variant is the one that
works at this scale, and even so this is the arm most likely to fail for reasons of scale rather
than principle.

### P4 — expert iteration

Search at a subset of decisions produces an improved action distribution, and the policy is fitted
to it by supervised cross-entropy. A different paradigm: the target is a better *policy*, not a
return.

Needs a state-clone-and-rollout API, which does not exist. Selectivity is what makes it
affordable — search only the ~14% of decisions with 10 or more options, and the high-stakes
heads.

## 2. Fair comparison

**Equal games simulated, never equal updates.** PPO takes K steps per batch; comparing at equal
updates hands it a K-fold data advantage. Games are the resource that costs.

**Each arm gets a small hyperparameter sweep.** Comparing a tuned REINFORCE against an untuned PPO
measures tuning. Minimum: PPO over K in {2, 4} and clip in {0.1, 0.2}; ES over population size and
noise scale. That sweep is why the arm list is four and not eight.

**Protocol, unchanged from what worked:** 3 training seeds per arm on shared streams so
same-index comparisons are paired; every arm resumes from the same champion; evaluation on the
sealed 98M block at 200 seeds (1,200 games per faction); report across-seed **ranges** and call a
difference a result only when the ranges are disjoint.

**Baseline is `--clearance-weight 0`**, plus whichever of B1/B2 survives today's run. Comparing
against a configuration known to be degrading would flatter everything.

## 3. Decision rules, written before the runs

- **P1 beats the baseline at equal games** — the rule was wasting data; re-run the feature and
  reward questions under PPO before tuning anything else.
- **P3 matches or beats the gradient arms** — credit assignment is not the constraint, and the
  effort spent on returns and baselines was misdirected.
- **P4 beats everything but costs far more per game** — the ceiling is real but the route is
  search, and the question becomes how cheaply the search can be distilled.
- **Nothing beats the baseline** — the optimiser family is not the constraint. The remaining
  untested axis is the **policy class**, bilinear or a small MLP over the same features, and that
  becomes the next plan rather than a fifth algorithm.

## 3a. What the first two steps measured (2026-08-18)

**Batch retention — settled, and it went the easy way.** One retained batch of 96 games is 105,449
steps, 567,559 option-vectors and 15,362,544 feature entries, costing **572 MB** of resident
memory against the machine's 93.6 GB. PPO holds a full batch with room for eight concurrent arms;
**no minibatching**, which was the larger job the plan was braced for.

**PPO implemented and verified by parity, not by inspection.** At epoch zero the current policy
*is* the behaviour policy, so every importance ratio is exactly one and PPO must collapse to
REINFORCE. Across 573,458 weight deltas the worst relative disagreement is **4.2e-12** — pure
summation-order noise, since REINFORCE defers the centring mean and keeps two sums apart while PPO
knows the mean up front and weights each decision as it goes.

One thing turned out simpler here than in the general case, and it is worth stating because
getting it wrong would be invisible: **returns do not depend on the policy.** They are a function
of the episode, fixed once the batch is played. So the baseline is computed once and shared by
every epoch. Recomputing it per epoch would let it drift with the weights and quietly stop being
an advantage.

**Cost, at equal games simulated** (8 threads, contended):

| | games/step | s/step | steps per 96 games |
|---|---|---|---|
| REINFORCE | 96 | 1.797 | 1 |
| PPO K=4 | 24 | 0.758 | 4 |

Four times the gradient steps for 1.69x the wall clock; an extra step costs **28%** of a fresh one.
Equal games is automatic rather than something the protocol has to enforce — one update is one
batch of 96 games for every arm, whatever it then does with them, so PPO's cost lands in wall clock
and never in extra games.

### The result that changed the pilot: the clip never binds

Measured clip fraction is **0.0007**, rising only to 0.0053 by epoch 8. The trust region is
essentially never active, so PPO at K=4 is currently **four unconstrained gradient steps on one
batch** — which is hard to distinguish from one step at four times the rate.

That makes an arm the plan did not specify the one that decides whether the others mean anything:

> **P3 — REINFORCE at `--learning-rate 0.12`.** If PPO K=4 and REINFORCE at 4x the step land in
> the same place, PPO is buying nothing an existing flag could not, and reporting it as an
> algorithmic win would be reporting a step-size change.

Comparing tuned PPO against untuned REINFORCE would measure tuning; this is the cheapest way not
to make that mistake. The pilot is therefore P0 REINFORCE lr 0.03, P1 PPO K=4, P2 PPO K=2,
P3 REINFORCE lr 0.12 — every arm at `--rollout-depth 1`, because PPO cannot use the wave scheduler
and the control gives up the same throughput to keep the scheduler constant.

## 4. Order

1. ~~Measure batch retention.~~ **Done** — 572 MB, fits, no minibatching.
2. ~~**P1 PPO**~~ — **implemented, parity-verified, pilot running.** See §3a.
3. **P3 ES** — cheapest of the rest, and the most informative negative if it works.
4. **P2 A2C** — only once the state-feature gap is scoped as its own piece of work.
5. **P4 ExIt** — only if P1 to P3 all fail, or if its ceiling is wanted for distillation.

## 5. Explicitly out of scope

- **Anything using a hand-written evaluator as teacher or target.** Ruled out by instruction. That
  removes the Python evolution champion, and with it the only policy known to be stronger than the
  current one — so no arm here can be validated by imitation and all must be judged on VP.
- **Deep CFR and other regret minimisation** — the right family for imperfect information, but
  TI4's state space against poker's makes it a research project rather than an arm.
- **GPU work.** Measured earlier: the workload is allocation-bound rather than compute-bound, and
  `target-cpu=native` bought 1%.
