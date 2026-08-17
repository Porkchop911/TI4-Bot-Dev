# The largest remaining efficiency lever: nearly half the features do nothing

Date 2026-08-17. Follows `plans/RESULTS_2026-08-17_ENGINE_EFFICIENCY.md` (2.96× already banked).

---

## The finding

**47.8% of every feature instance built, stored and accumulated during training is provably inert.**

Measured over 600 real choices / 3,578 options / 85,782 feature instances, drawn from an actual
game rather than a synthetic position:

| family | instances | inert | inert share |
|---|---|---|---|
| `prompt-kind` | 12,184 | 10,004 | **82.1%** |
| `kind` | 3,578 | 2,525 | **70.6%** |
| `state-kind` | 23,702 | 16,709 | **70.5%** |
| `prompt-option` | 30,246 | 9,286 | 30.7% |
| `option` | 9,703 | 2,412 | 24.9% |
| `target` | 5,799 | 0 | 0.0% |
| **total** | **85,782** | **41,003** | **47.8%** |

"Inert" here means one specific, checkable thing: **the feature has the same value on every option
of its choice.**

## Why an option-invariant feature is exactly inert

Not approximately, not usually — exactly, for both play and learning.

Let a slot take the same value `c` on every option of a choice.

**Play.** Its score contribution is `w·c` for every option alike. Softmax is invariant to adding a
constant to every logit, so no probability changes and no sample changes.

**Policy gradient.** The per-decision term is `φ_chosen(slot) − Σₒ pₒ·φₒ(slot)`. With
`φₒ(slot) = c` for all `o`, and `Σₒ pₒ = 1`:

```
c − c·Σₒ pₒ  =  c − c  =  0
```

**Entropy gradient.** The term is `Σₒ coeffₒ·φₒ(slot)` with `coeffₒ = −pₒ(ln pₒ + H)/T`. With
`φₒ(slot) = c` it factors to `c·Σₒ coeffₒ`, and

```
Σₒ coeffₒ = −(Σₒ pₒ ln pₒ + H·Σₒ pₒ)/T = −(−H + H)/T = 0
```

So both gradient terms vanish and the score ranking is untouched. **Such a feature can never move
a weight and can never change a decision.** Computing it, storing it in the trajectory, and
summing it into three accumulators is work whose result is discarded by arithmetic.

## What it is worth

Inert features cost in three of the four slices of a training update:

| slice | share of runtime | inert portion |
|---|---|---|
| feature construction | ~45% | 47.8% |
| trajectory recording | ~20% | 47.8% |
| gradient reduction | ~18% | 47.8% |
| engine | ~9% | — |

`0.478 × 83% ≈ 40% of the update`, i.e. up to **~1.66×** if all of it is avoided.

## How to avoid it without paying to detect it

Detecting inertness by comparing the finished vectors costs exactly what it saves. Both large
sources are **structurally predictable** from the choice, before any feature is built:

**1. The kind-dependent families** (`kind`, `prompt-kind`, `state-kind` — 45.9% of all instances,
70–82% of them inert). Every one of these depends only on `(choice, canonical_kind(option))`. So:

> If every option in the choice canonicalises to the same kind, skip these three families entirely.

An `O(options)` scan of the kinds, once per choice. Exactly the inert case, no false positives.

**2. The token-dependent families** (`option`, `prompt-option` — 46.8% of instances, 25–31% of
them inert). These are keyed on the option's own tokens. A token present in *every* option's token
set produces the same feature on every option. So:

> Compute the intersection of the options' token sets once per choice, and skip features derived
> solely from tokens in that intersection.

An `O(options × tokens)` pass once per choice, against `O(options × tokens²)` saved in the
prompt-option cross product.

Together these two rules cover essentially the whole 47.8%.

## Verification this would need

The neutrality argument above is a proof, but it rests on the implementation matching it, so it
should be checked and not asserted:

1. **Decision trace on seed 83000001** — choices, prompts and option counts must be unchanged.
   Raw *scores* will differ by a per-choice constant; that is the whole point, and the trace's
   score field is expected to move.
2. **Panel metrics over 720 games** — per-faction VP and clearance, against the current build.
   The earlier hash-keying change set the precedent for this gate and came back identical to four
   decimal places.
3. **A unit test of the invariant itself**: build a choice whose options all share a kind, assert
   the dropped slots are exactly those with equal values across options, and assert the gradient
   contribution of each dropped slot is zero.

## The other structural levers, measured

| Lever | Measured | Verdict |
|---|---|---|
| **Straggler tail / batch depth** | 96 games 7.64 ms/game → 384 games (`--rollout-depth 4`) **7.06** → 768 games 7.13 | ~8%, and depth 8 gives it back. This lever was worth ~20% before mimalloc; the allocator removed most of the contention that made it big. Keep `--rollout-depth 4`, do not raise it. |
| **`f32` instead of `f64`** for feature values and weights | not measured | Halves memory traffic in the three slices that matter, and this workload is memory-bound. Plausibly 10–15%, but it is a genuine numerical change needing its own validation, unlike the inert-feature work which is exact. |
| **Fewer decisions per game** (prune dominated trade offers) | hacan generates 59% more decisions than l1z1x | Real, but it changes the offered option set — a modelling decision with oracle-parity consequences, not a speedup. Separate package. |
| **Shortlisting options before featurising** | — | **Ruled out by design.** `inference.rs` documents that a learned policy must see every legal option, precisely so no authored judgement about "worth considering" leaks in. An option filtered out is one the policy can never be taught to want, and its absence is invisible in every metric. |

## Recommendation

Implement the kind-family rule first — it is the simpler of the two, covers ~34% of all feature
instances on its own (≈1.39×), and validates the neutrality argument end to end before the
token-intersection rule is attempted.
