# Overnight arena: the plateau is a decline, and the reward shaping is causing it

Date 2026-08-18. 15 runs, 5 arms x 3 training seeds, 2,500 updates each, 00:11–05:38.
Evaluated on the sealed 98M seed block, 200 seeds = 1,200 games per faction, each arm scored with
its own binary so the feature sets match.

---

## Result

All arms resumed from the same champion, which scores **12.344** table VP on this panel.

| arm | | mean table VP | across-seed range | vs start | vs A1 | |
|---|---|---|---|---|---|---|
| **A4** | `--clearance-weight 0` | **12.748** | [12.591, 12.842] | **+0.404** | **+1.045** | **disjoint** |
| **A5** | features + `clearance-weight 0` | **12.684** | [12.654, 12.722] | **+0.340** | **+0.980** | **disjoint** |
| A6 | `--learning-rate 0.06` | 11.773 | [11.724, 11.834] | −0.571 | +0.069 | overlap |
| A2 | target-system features | 11.759 | [11.685, 11.836] | −0.585 | +0.056 | overlap |
| A1 | baseline | 11.704 | [11.610, 11.806] | −0.640 | — | — |

## The headline was not the question I asked

**The baseline does not plateau. It declines.** Under the configuration the plateau was measured
in, 2,500 further updates cost **0.64 table VP** — on all three seeds, none overlapping zero. What
has been read for weeks as "training has converged" is training slowly making the policy worse,
and a flat champion line hides it because the gate keeps rejecting the candidates.

**`--clearance-weight 5.0` is the cause.** Removing it is the difference between −0.640 and
+0.404: a **+1.045 VP swing**, with the three seeds of A4 disjoint from the three of A1. That is
the largest effect anything in this investigation has produced.

It is also the effect predicted from first principles in
`ANALYSIS_2026-08-17_LEARNING_AND_COMPUTE` §2.4, before any of this was run: the term lands in the
**final** reward slot, so via the suffix sum every decision in the episode carries the full −5,
contributing ~34% of return variance while discriminating between games and never between
decisions inside one. The prediction was that it injects noise rather than signal. It does.

## The representation fix did not move victory points

This is the pre-registered informative negative, and it should be recorded as one rather than
explained away.

`FINDING_2026-08-17_ACTIVATION_IS_BLIND` proved 94% of activation decisions held options the
policy could not tell apart. The fix worked on its own terms — blind 94.0% → 14.1%, ceiling
0.681 → 0.995 — and produced **+0.056 VP, ranges overlapping**. A5 against A4 is the same story:
**−0.064**, no gain on top of the reward fix.

By the rule written before the run: *blind% fell to near zero and VP did not move by ≥0.15*, so
the representation hypothesis is **refuted at this budget**. The blindness was real, provable and
cheap to fix, and it was not the binding constraint.

One nuance that may matter for a second attempt. The new facts added **5 learnable weights out of
47,402** — one scalar each, because `uniform_kind` correctly drops their kind-crosses on
activation. So the policy can now *distinguish* two systems but has almost no capacity to express
*how* to value them: one weight for distance, one for resources, and so on, all linear. Bucketing
those scalars one-hot (`target:own-distance:3`) would give a weight per value at no structural
cost, and is the cheapest way to find out whether separability plus expressiveness beats
separability alone.

## Step size is not the problem

A6 at double the learning rate is indistinguishable from A1 (+0.069, overlapping). The reference
is not under-stepped, so nothing above is an artefact of a badly tuned baseline.

## The cost of the fix

Clearance falls when the clearance penalty is removed: **0.77 → 0.70**. That is the term doing the
one thing it was designed for. The trade is +1.0 table VP for −0.07 clearance, and the existing
gate's `max_faction_clearance_regression` of 0.03 would **reject** it — so the gate as configured
would have refused the best result of the night. Worth deciding deliberately rather than by
default.

## What to do next

1. **Stop training with `--clearance-weight 5.0`.** It is not a plateau to be broken, it is a
   penalty making things worse.
2. **Re-baseline.** Every "plateau" conclusion in `EXECUTION_STATE` from the clearance-weight runs
   describes a configuration that was actively degrading.
3. **Then re-ask the representation question**, with bucketed features and against a baseline that
   is not declining. The current answer is "no effect", but it was measured on top of a reward
   that was dominating everything.
