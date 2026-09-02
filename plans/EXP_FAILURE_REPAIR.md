# Experiment: can causal failure repair take stage-1 clearance to 99%?

Started 2026-09-02. Mandate: reach 99% greedy clearance on held-out maps, **or** establish that this
approach cannot. Both are acceptable outcomes; only an undocumented one is not.

Results are appended to `## Log` as they happen, including the ones that go the wrong way. The
design above the log is not edited to match them.

## Where this starts

| | |
|---|---|
| policy | `out/checkpoints/sweep-A-250/checkpoint-14476` |
| greedy clearance, held out | **93.58% ±0.40** |
| failure rate | 6.42% |
| target | 99.00% |
| gap | 5.42 points, i.e. 84% of all current failures must be removed |

That last framing is the honest one. 99% is not "a bit better than 93.6"; it is removing five out of
every six failures the best policy makes.

## What is already known

**The failures are concentrated by mode, not by state.** From `failure_census` on 10,800 seat-games
(682 failures, 6.31%):

| planets short | systems short | composition | count | share | cumulative |
|---|---|---|---|---|---|
| −1 | 0 | ok | 468 | 68.6% | 68.6% |
| −1 | −1 | ok | 88 | 12.9% | 81.5% |
| −2 | −1 | ok | 62 | 9.1% | 90.6% |
| 0 | 0 | SHORT | 25 | 3.7% | 94.3% |
| −1 | −1 | SHORT | 17 | 2.5% | 96.8% |

Three shapes cover 90.6%, and the dominant one is a seat needing exactly one more planet with
systems and fleet composition already satisfied. Every faction shows the same signature across a
5.8-point clearance spread, so this is a narrow decision class occurring everywhere rather than a
rare-state tail. That is the reason a replay-from-failed-start curriculum was rejected: the
difficulty is not in the initial condition.

**A constructive lower bound already exists.** `opening_reachability` finds a clearing continuation
for ~65% of failures by sampling the failing seat hot. At a 6.42% failure rate that is 4.17 points,
so **~97.75% is demonstrably reachable** by *some* play. 99% is above that bound, which does not
make it impossible — the search is weak and its failures prove nothing — but it does mean 99%
requires repairing failures that no search has yet cracked.

**The previous attempt at this failed, and why.** `rescue_imitation` (`339f42d`) cloned the first
decision where a hot rescue diverged from the champion. At temperature 2.5 a rescue diverges
immediately, so "first divergence" collapsed onto decision zero and two thirds of its targets were
the strategy card pick, credited for an outcome twenty decisions later. Held out: 91.5% → 89.3%,
every faction regressed, on 45 samples.

## The method

**Impose the divergence, do not find it.** `counterfactual_repair` replays a failed line, substitutes
one legal alternate at one index with everything else held identical, and lets the seat play on
under its own policy. That measures `P(clear | do(a_i = a'))`.

Soundness conditions, all enforced rather than assumed:

- greedy policy, so the prefix reproduces — **verified**, 250 failures × 4 recording passes, 0
  disagreements;
- the other five seats are never perturbed;
- only the substituted decision is imposed; afterwards the seat decides for itself;
- the option count at the substituted index is checked against the recording pass, and a mismatch
  discards that substitution and is counted rather than silently accepted.

**The training target** (from review, and stronger than the version this started with):

```
L = L_PPO + λ · (1/N) Σ_i (1/|C_i|) Σ_{c ∈ C_i} softplus(s_{f_i} − s_c)
```

- `f_i` is the action that actually produced the failure; `C_i` is the exhaustively enumerated set
  of alternates at that index that cleared.
- Non-clearing alternates are **not** negatives. They failed *under the current downstream policy*,
  which training is about to change, so labelling them bad asserts more than was demonstrated.
- Equal total weight per counterfactual state, averaged over its clearing alternates. One clearing
  action gets all the positive gradient; twenty share it. That is the right confidence behaviour and
  it costs no new hyperparameter.
- PPO data stays in the update. This is auxiliary, not a fine-tune.
- **The labels are policy-relative.** Whether an intervention clears depends on the downstream
  policy, so once the policy moves the rescue set must be regenerated rather than treated as
  permanent expert truth.

## Statistics

Failures are **clustered by map seed** — a single map contributes up to 14 of them, sharing
topology, slice and opponents. Confidence intervals are bootstrapped over map-seed clusters, never
over failures. Treating 250 failures as 250 independent observations understates the interval badly.

Reported for every repairability figure: the naive interval, the cluster-bootstrapped interval, the
number of unique maps, and the largest number of failures from one map.

## Decision rule

Set before the numbers arrive, so it cannot be fitted to them.

- **Single-decision repairability > 50%** → build the auxiliary loss and train.
- **20–50%** → build it, but expect a ceiling well below 99% and say so.
- **< 20%** → the failures are not local decision errors and this approach cannot reach 99%.
  Record that and stop rather than pursuing it.

Success is greedy clearance on the **Validation** pool, seeds 900000000+, measured by
`clearance_eval` at the greedy limit. Failures are collected on the **Train** pool. Nothing in this
experiment touches the sealed Final pool.

A regression at any point is reported as a result, not tuned away.

## Log

### 2026-09-02 — repairability, 250 failures (exploratory)

`counterfactual_repair` on `sweep-A-250/checkpoint-14476`, 63,484 replays in 573s.

**Sampling caveat, stated because it matters:** these 250 are a contiguous seed prefix
(800000000–800000106) drawn from **70 unique maps, up to 14 failures from one map**. Faction mix
tracks the true failure rates so there is no selection bias, but the effective sample is nearer 70
than 250 and no interval is quoted here. The full 682-failure run with per-failure output follows.

```
repairable by exactly one decision   137 of 250   54.8%
substitutions that cleared          1909 of 63484  3.01%
discarded (index not the recorded decision)  145   0.228%
```

The discard rate is negligible, so the soundness guard is not silently eating the sample.

**By failure shape — and this reverses my prediction.**

| planets | systems | composition | failures | repairable |
|---|---|---|---|---|
| −1 | 0 | ok | 177 | **40.1%** |
| −1 | −1 | ok | 31 | 100.0% |
| −2 | −1 | ok | 22 | 77.3% |
| −1 | −1 | SHORT | 7 | 100.0% |
| 0 | 0 | SHORT | 5 | 100.0% |

I predicted, in writing and before measuring, that the dominant class — one planet short with
everything else met — would be *the most* repairable, on the reasoning that such a seat needs only
one more planet. It is **the least**, at 40.1%, while every smaller and messier class is at or near
100%. A seat that is one planet short with systems and composition satisfied is mostly not a seat
that made one local mistake; it is the terminal symptom of something earlier.

The composition-only class is 5 of 5, which is the positive control behaving as hoped — though at
n=5 that is a hint, not a finding.

**By faction**, the split I predicted also failed to appear: hacan 64.4%, xxcha 62.2%, jolnar 61.5%
against letnev 37.5%, sol 41.7%, l1z1x 45.7%. Jol-Nar is among the *more* repairable, not the
plan-level outlier I expected from its concentration signature.

**Where the repairing decision sits:** 58.3% in the first fifth of the line, 24.9% in the second,
0.7% in the last. And **70.1% of repairable failures have their earliest repair at decision zero**,
which is the strategy card pick.

That last number needs care. It is *not* the pathology that sank `rescue_imitation`: there, decision
zero was merely where a hot sampler first diverged; here it is causally implicated, because changing
it with everything else held identical clears the opening. But it is still the coarsest possible
repair. What protects the training is that repairs are not one per failure — 732 repairing indices
across 137 failures, about 5.3 each — so strategy is **13.1%** of targets rather than
`rescue_imitation`'s 66%:

```
turn 18.3   activation 16.0   cargo 14.2   strategy 13.1   movement 12.3
production 11.9   secondary 5.1   landing 4.0   payment 1.5   trade 1.5
```

### The ceiling this implies, before any training

Single-decision repair can, at absolute best, remove the failures that are single-decision
repairable:

```
93.58 + 0.548 x 6.42 = 97.10%
```

The reachability search gives an independent constructive bound of 97.75%. **Both are below the 99%
target**, and 97.10% assumes training converts every demonstrated repair into policy behaviour,
which it will not.

So on the evidence available at this point, **99% is not reachable by one round of single-decision
repair.** The reason to continue rather than stop here is specific and testable: the labels are
policy-relative. Whether an intervention clears depends on the downstream policy, so a policy
improved by one round makes *more* failures single-decision repairable in the next. Whether that
compounds far enough is exactly what the iteration will show, and it is the only honest way to find
out. If it plateaus around 97, that is the answer and it gets recorded as the answer.


### 2026-09-02 — full enumeration, and the unanchored objective collapses

**Repairability on the full set is higher than the prefix suggested.** 682 failures, 24.6 min of
enumeration, against the current champion:

```
repairable by exactly one decision   428 of 682   62.8%
training samples                     2433
discarded                            228   0.13%
```

2,433 samples against `rescue_imitation`'s 45, with causal attribution instead of first-divergence.
The 250-failure prefix said 54.8%; the full set says 62.8%, so the prefix was pessimistic and the
exploratory caveat was worth stating.

Revised ceiling for one round: `93.58 + 0.628 × 6.42 = 97.61%`. Still short of 99%.

**Then the training collapsed.** Preference loss alone, `lr 1e-4`, full batch:

| epoch | loss | held out |
|---|---|---|
| baseline | — | **93.96%** |
| 1 | 7.708 | 93.93% |
| 2 | 7.379 | 93.93% |
| 4 | 6.762 | 92.74% |
| 6 | 6.213 | 88.68% |
| 8 | 5.730 | 79.46% |
| 10 | 5.313 | 62.12% |
| 12 | 4.958 | 40.36% |
| 14 | 4.648 | 22.03% |
| 16 | 4.364 | **12.94%** |

The loss descends smoothly and monotonically the whole way down. The objective is doing exactly what
it was told; what it was told is insufficient.

**Diagnosis.** 2,433 repair states sit inside a decision distribution of roughly 570,000 (10,800
seat-games × ~52 decisions) — **0.4%**. They share one trunk with the other 99.6%, and nothing in
the objective says the other 99.6% should stay as it is. Optimising a rank ordering on 0.4% of the
distribution, unconstrained, destroys the policy that produced the 93.96%.

**This is not a step-size problem.** Epoch 1 already reads 93.93% against a 93.96% baseline: the very
first step is flat-to-negative. There is no smaller step that finds an improvement this curve is
hiding — the direction itself does not help while unanchored.

**And it is not the attribution problem either.** That is the useful part. `rescue_imitation` failed
with 45 samples and first-divergence attribution, and the natural reading was that better
attribution and more data would fix it. This run has 54× the data and causally correct targets, and
it fails *worse*. So the missing ingredient was never label quality. It is that a targeted auxiliary
objective on a sliver of the state distribution needs something holding the rest of the policy in
place.

**Next:** an anchor. The review recommended keeping `L_PPO` in the update and I skipped it to get a
faster signal, which was the wrong trade — the fast signal is only interpretable now because the
anchor was missing. The cheapest sound version is a trust region in function space rather than
rollouts:

```
L = L_repair + β · E_s[ KL( π_ref(·|s) ‖ π(·|s) ) ]
```

over a broad sample of states drawn from ordinary play, with `π_ref` the frozen starting policy.
That directly states the thing the collapse shows to be missing: do not change what you already do
well. `β` is swept rather than guessed.


### 2026-09-02 - the anchored sweep, and why the approach fails

`L = L_repair + b * KL(pi_ref || pi)`, anchor of 19,222 states from 10 maps captured once against
the starting policy. One collection, three weights trained from identical weights against identical
labels. Baseline **93.96%**.

| epoch | unanchored | b=1 | b=10 | b=100 |
|---|---|---|---|---|
| 1 | 93.93% | 93.93% | 93.93% | 93.93% |
| 2 | 93.93% | 93.90% | 93.86% | **93.97%** |
| 4 | 92.74% | 92.75% | 92.90% | 93.90% |
| 8 | 79.46% | 80.81% | 90.69% | 93.90% |
| 12 | 40.36% | 57.38% | 90.90% | 93.76% |
| 16 | 12.94% | 52.08% | 89.42% | 93.62% |
| 20 | - | 55.69% | 86.50% | 93.57% |
| KL at end | - | 0.313 | 0.052 | 0.0045 |

Best across the whole sweep: **93.97%, or +0.01 points** on a +-0.55 interval. Nothing.

The asymptote was predictable and behaved as predicted: as the weight grows the policy freezes and
clearance returns to baseline. Every finite weight trades clearance away and none buys anything.
There is no sweet spot hiding between grid points either - **epoch 1 reads 93.93% in all four
configurations**, so the very first step is flat-to-negative regardless of the trust region.

### The diagnostic that settles it

b=100 is the interesting arm, because there the repair loss fell substantially (7.71 to 5.55) while
clearance stayed flat. The policy *was* learning the demonstrated repairs and getting nothing for
it. Two explanations fit that - the repairs are learned but do not generalise to new failures, or
they do not even hold on the failures they came from - and one measurement separates them.

Re-censusing the trained policy on **the same Train seeds its labels were collected from**:

| | failures | rate |
|---|---|---|
| champion | 682 | 6.31% |
| after repair training | **717** | **6.64%** |

It does not fix the failures it was trained on. The difference is about one standard error, so the
honest statement is *no better, possibly slightly worse* - but there is no improvement where
improvement should have been easiest, on the exact positions the labels describe.

### Why: the labels are invalidated by the act of learning them

A counterfactual proves `P(clear | do(a_i = c), pi) = 1` **for the specific pi that played the rest
of the round**. Training raises the score of `c`, which changes pi, which changes the continuation
after `a_i` - and the clearance that justified the label no longer follows from it. The label
describes a policy that no longer exists by the time it has been learned.

This is not an implementation detail and not a tuning failure. It is a property of the method: the
signal is self-invalidating. Iterating rounds does not escape it, because each round's labels are
undone by that round's own update.

It also settles the `rescue_imitation` post-mortem retroactively. That attempt was diagnosed as a
credit-assignment failure - first divergence at temperature 2.5 collapsing onto decision zero - and
the recorded fix was to impose the divergence rather than find it. That fix was correct and it
worked: strategy fell from 66% of targets to 13%, and repairs came out spread across turn,
activation, cargo, movement and production. **The attribution was fixed and the method still
failed**, with 54x the data. Attribution was never the binding constraint.

## Outcome

**The target was not reached. 99% is not achievable by this approach, and the approach does not
improve clearance at all.**

Final held-out greedy clearance: **93.96%**, unchanged from the starting champion. The best
checkpoint the entire experiment produced is the one it started with.

Three independent reasons, in increasing order of how fundamental they are:

1. **Arithmetic.** One round of single-decision repair caps at `93.58 + 0.628 x 6.42 = 97.61%`, and
   the reachability search gives an independent constructive bound of 97.75%. Both are below 99%
   before any training question is asked.
2. **Optimisation.** The repair objective touches 0.4% of the decision distribution through a shared
   trunk. Unconstrained it destroys the other 99.6%; constrained enough to be safe it changes
   nothing. There is no weight between those where it helps.
3. **The signal is self-invalidating.** Learning a counterfactual label changes the policy the label
   was conditional on. This is why the trained policy does not fix even its own training failures,
   and it is why more data, better attribution and a longer schedule cannot rescue it.

Reason 3 is the one that generalises. Any method that labels a decision by *the outcome of a fixed
downstream policy* and then trains that policy on the label has the same defect. A method that
avoids it would have to either re-derive the label continuously as the policy moves - which is what
on-policy RL already does, and is what PPO is - or produce labels that do not depend on the
downstream policy at all, e.g. from a search that proves a line clears under *any* reasonable
continuation.

### What was not tested, stated plainly

`L_PPO + lambda * L_repair` with fresh on-policy rollouts in the same update, which was the
reviewer's actual recommendation. The KL anchor was used as a cheaper stand-in for it and the
substitution is not free: PPO would re-derive the value of each state continuously, which is
precisely the defect in reason 3. So this experiment does **not** establish that a PPO-integrated
repair term is worthless. What it establishes is that the repair signal cannot carry a policy on its
own, that its labels do not survive being learned, and that 99% is out of reach of single-decision
repair regardless, on the arithmetic alone.

### A side finding worth keeping

**Mean KL is a weak proxy for greedy-policy change.** At b=1, a KL of 0.086 accompanied an 8-point
clearance drop. Small probability shifts flip argmaxes cheaply, so a distributional trust region
barely constrains a metric read off the argmax. Any future trust region protecting a greedy metric
should constrain argmax stability directly.
