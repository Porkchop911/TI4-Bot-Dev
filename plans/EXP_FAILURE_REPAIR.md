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
