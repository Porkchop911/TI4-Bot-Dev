# M09-024b — architecture evaluation request

**To:** the frontier reviewer (Tier C — schema/architecture)
**From:** Claude Opus 5, author of M09-021 through M09-024b
**Date:** 2026-08-25
**Trigger:** `plans/evidence/M09-024b.md` — the discovery pass stopped at `V_cap` 245,760 against
the 65,536 limit. MLP plan §4.5 requires an explicit architecture review before anything proceeds.
**Blocks:** M09-024 (parent), and through it M09-026 onward. M09-025 is independent.

---

## The question

**Which feature families receive dense columns in the MLP input, and what is `V_cap`?**

Everything else in this document exists to make that question answerable against numbers. It is
not a request to confirm a recommendation; I do not have one I trust, for reasons in
"Why I am not proposing an answer" below.

---

## What was measured

768 games, MLP plan §6.1's fixed teacher seed schedule, four-round horizon, the r6 champions
playing. Full method and gates in `plans/evidence/M09-024b.md`.

| source | names | unique to it |
|---|---|---|
| (a) r6 champion profile | 41,113 | 9,573 |
| (c) content records | 295 | 187 |
| (b) §6.1 replay | 194,083 | 162,435 |
| **union** | **203,843** | |

`slot_count` 203,882 including reserved columns → **`V_cap` 245,760**, against a limit of 65,536.

Three families are **91.3%** of the union:

| family | names | share | in r6 profile | keyed by |
|---|---:|---:|---:|---|
| `state-option` | 88,909 | 43.6% | 2,496 | full option id |
| `prompt-option` | 58,637 | 28.8% | 35,409 | prompt token × option token |
| `prompt-bigram` | 38,542 | 18.9% | **0** | adjacent prompt tokens |

Samples, from the r6 profile:

```
state-option:holy_planet_of_ixth:strategic_tokens
state-option:te6warfare:commodities
prompt-option:piratecontract1:counter
prompt-option:starpoint:xanhact
```

Everything M09-021/022/023 added — objectives, faction decomposition, opponent counts — is
**0.3% combined**.

### Exclusion arithmetic

| dense vocabulary | slots | `V_cap` | ≤ 65,536? |
|---|---:|---:|---|
| everything | 203,882 | 245,760 | no (3.75×) |
| − `state-option` | 114,973 | 139,264 | no |
| − `state-option`, `prompt-bigram` | 76,431 | 92,160 | no |
| − all three | 17,794 | 24,576 | **yes** |

---

## A contamination in my own measurement — read this before the numbers

`prompt-bigram` is emitted **only** by `option_feature_names`, the legacy **schema-2 hashed** path
(`features.rs:302`). It is **not** in `EXPLICIT_FIXED_FAMILIES`, and the schema-4 r6 champions hold
**zero** of them. My collector called both the explicit extractor and the legacy hashed one, so it
enumerated a schema-2 channel that no schema-4 model reads.

**If the MLP's input is the schema-4 explicit vector, then 38,542 of my 203,843 names should not be
there at all**, and part of `prompt-option` may be in the same position. The union would be
165,301 — still 2.5× over the limit, so the *stop* stands either way. But the number is not clean,
and the composition question is exactly what this review turns on.

I am reporting this rather than quietly re-running because which path the MLP consumes is itself
part of what I am asking. Recorded as **O-M09-024b-4** against my own package.

---

## Where the 65,536 limit comes from

Traced, because the review should know how much authority the number carries.

- It appears **twice**: §4.5 ("an expected upper bound of 65,536") and §4.4's load bounds, as a
  validation constant. **No memory, compute or throughput calculation stands behind it.**
- It is headroom over §4.2's *provisional* estimate of 49,152 — itself derived from the r6
  champions' 41,113 names and explicitly labelled "uses 49,152 only as the current estimate", to be
  replaced by "the exact value and parameter count" M09-024 records.
- §4.2's estimate misapplies §4.5's own rule: 41,113 × 1.2 = 49,335.6, whose next multiple of 4,096
  is **53,248**, not 49,152. Immaterial, but it is why the r6-only figure I report is 53,248.
- The stated *reason* behind the number is not a ceiling: §4.2 — *"an unused column costs 256
  weights rather than one. That is the fact that rules out the hashing trick in §4.5, and it is why
  V is enumerated rather than generous."*

**The deeper point, which I think matters more than the limit.** 41,113 is a *survivor* count: the
names a trained checkpoint kept a weight for. It was never a measurement of the extractor's
reachable name space. So the plan's estimate was structurally an underestimate rather than a stale
one, and any replacement figure derived the same way would be too.

### What 245,760 would cost, since the plan never states it

| | budgeted (§4.2) | at 245,760 |
|---|---:|---:|
| input layer params | ~12.6M | **62.9M** |
| input layer f32 | ~50 MB | **252 MB** |
| + Adam moments (×2) | ~150 MB | **~755 MB** |
| total model | ~12.8M | ~63M |

Hardware is 24 GB / 32 threads, CPU-only torch. So this is affordable, and it is **5× the model
that was designed, costed and reviewed**. §8's risk row — *"Overfitting the 96-game batch — the
model is provisionally ~12.8M parameters"* — would need rewriting rather than re-reading.

---

## Why I am not proposing an answer

My instinct is that option-id and prompt-text keyed families do not belong in a dense input: a
`state-option:<full option id>:<fact>` column is seen in a vanishing fraction of games by
construction, and the codebase already refuses board identities on this exact ground — the explicit
path filters planet ids out of `option:` tokens because "a name that is a planet under any printing
is a board identity and should not become a feature under another."

**Two things stop me from acting on that instinct, and both are load-bearing:**

1. **§6.1 says the opposite.** *"Distillation initially uses only the legacy `factual` policy
   vector."* The KL targets come from six schema-4 champions whose logits are computed **from those
   families** — `prompt-option` alone is 35,409 of the champions' 41,113 names, 86% of their
   vocabulary. A student that cannot see what the teacher scored with is not distilling the
   teacher; it is fitting a different function to the teacher's outputs. That may be fine, or even
   desirable, but it is a change to §6.1 and not an implementation detail.

2. **§4.5 built the union deliberately.** It names the r6 profile as source (a) — legacy names,
   knowingly. Excluding them is a plan revision, and the plan says a failed gate "does not permit
   the implementer to move a threshold or choose a new dataset without a reviewed plan revision."

So the honest position is: the instinct points one way, the plan points the other, and the decision
is above the implementer's line.

---

## Options, and what each costs

Not exhaustive; the review may prefer something not listed.

**A — raise `V_cap` to fit the corpus.** Honest to §6.1; 5× the reviewed model; ~755 MB optimizer
state; invalidates §4.2's budget and §8's overfitting row. Also does not stop growing: 203,843 is a
lower bound at a four-round horizon.

**B — exclude the option/prompt-keyed families from the dense input.** Lands at 24,576, inside the
limit and near the budgeted model. Requires a §6.1 revision, because the student then sees a
strictly smaller feature set than the teachers scored with. Needs a stated position on whether that
is distillation or approximation.

**C — frequency pruning.** Keep names above a support threshold; the tail routes to its family's
OOV column, which already exists and is the machinery's designed answer for exactly this. Preserves
the input semantics approximately, needs a threshold nobody has justified yet, and needs a second
measured pass — my collector records *whether* a name appeared, never *how often*
(**O-M09-024b-2**).

**D — bounded hashing within a family.** Give each unbounded family `k` buckets instead of one OOV
column. Keeps every family present at fixed cost. §4.5 rules out hashing *globally*, on the
argument that a wasted column costs `width`; whether that argument also rules it out *within a
family already destined for a single shared column* is a real question and not one I should answer.

---

## What a satisfying answer must contain

So the result is decidable rather than an opinion:

1. **The composition rule**, stated as a predicate over family names, not a list — the list will be
   stale the next time a family is added.
2. **Which extractor path the MLP consumes** (schema-4 explicit, schema-2 hashed, or both). This
   settles the contamination above and is needed regardless of the rest.
3. **A `V_cap` figure and its parameter count**, since §4.5 requires M09-024 to record both.
4. **A position on §6.1** if the rule excludes anything the teachers scored with: is the result
   distillation, and if not, what replaces the KL objective's justification.
5. **Whether 65,536 stands, moves, or is replaced by a derived budget.** If it moves, what it is
   derived from — its current provenance is a round number over a stale estimate, and repeating
   that would leave the next package to rediscover this.
6. **A re-measurement instruction** if the rule requires data I did not collect (frequency counts,
   a longer horizon, single-path collection).

---

## Traps

- **Deciding by raising the limit.** 65,536 is soft, but the reason behind it is not: columns must
  earn their `width`. Raising the ceiling makes the model fit; it does not make an option-id column
  generalize.
- **Treating 203,843 as the number.** It is a lower bound at four rounds, and contaminated by the
  dual-path collection. Any capacity chosen from it inherits both.
- **Assuming the r6 figure was ever a vocabulary size.** It is a survivor count. Re-estimating the
  same way reproduces the same error.
- **Assuming the bare namespace is enough because the trunk is nonlinear.** F-M09-021-2 established
  that the trunk needs the bare facts; it established nothing about whether the crossed and legacy
  families are dispensable. That is this review's question, not a settled result.
- **Silently narrowing to make the number fit.** If the rule excludes families, it is a plan
  revision and should be recorded as one.

---

## Not being asked

Whether the objective, decomposition and opponent families belong — they are 0.3% and are not what
overran the limit. Anything about M09-025, the tensor packages, or M10. And no re-baseline: nothing
here touches engine behavior.

---

## State

M09-024a accepted. M09-024b ran within its authorized P2 bounds and **wrote no artifact**;
`out/vocabulary/` does not exist. Nothing proceeds on this frontier until this evaluation returns.
