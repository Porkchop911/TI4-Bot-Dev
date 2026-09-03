# Stage 2: the overnight campaign, and what the replicate retired

Four legs, all four rounds, all initialised from the stage-1 champion's line. Every number below
is **margin** in `crossplay_eval`: the candidate holds one seat against five frozen copies of
`best-94.97_r2-epoch22`, greedy, on the Validation pool.

**The margin null is −0.150, not zero.** The candidate is one draw and the best opponent is the
maximum of five, so an identical policy scores below zero by construction. The stage-1 champion
itself reads −0.178.

## The result that holds

| | margin | win | VP | clearance |
|---|---|---|---|---|
| stage-1 champion | −0.178 | 3.6% | 0.040 | 94.97% |
| r3 `checkpoint-22900` | **+2.494** | 87.4% | 3.079 | 93.32% |

That is the gain. It is enormous and it is not in doubt: the champion scores essentially no points
and loses to the table, the stage-2 policy scores three and beats it in seven games out of eight,
while giving up 1.65 points of round-one clearance — far inside the 85% floor.

## The results that do not hold

| leg | winner | margin | claimed gain |
|---|---|---|---|
| r3 | checkpoint-22900 | +2.494 | — |
| r4 | checkpoint-1064 | +2.587 | +0.093 over r3 |
| r5 | checkpoint-1040 | +2.526 | −0.061 over r4 |
| **r4b** | checkpoint-7520 | **+2.364** | — |

**r4b is r4's recipe from r4's start with only the rollout seed base changed.** The gap between
them is **0.223**, and that is the noise floor for "best checkpoint of one leg".

Both claimed gains — +0.093 and −0.061 — are inside it. **Legs r4 and r5 did not improve on r3.**
Three legs of training after r3 produced nothing distinguishable from re-rolling the dice.

Without r4b this would have been written up as steady progress across four legs.

## The confirmation, which settled it independently

Re-measured on seeds 900000100 upward -- the same Validation pool, adjacent to the selection range,
**never selected on** -- at 864 seat-games each.

| policy | selection seeds | fresh seeds | win | waste | declines |
|---|---|---|---|---|---|
| r3 `checkpoint-22900` | +2.494 (3rd) | **+2.587** (2nd) | 88.9% | 2.43% | 1.28% |
| r4 `checkpoint-1064` | +2.587 (1st) | **+2.517** (3rd) | 86.3% | 3.59% | 8.38% |
| r5 `checkpoint-1040` | +2.526 (2nd) | **+2.611** (1st) | 90.7% | 1.85% | 2.87% |
| r4b `checkpoint-7520` | +2.364 | TIMED OUT | — | — | — |

**The order inverted.** On the seeds that chose them the ranking was r4 > r5 > r3; on fresh seeds it
is r5 > r3 > r4, and the whole spread is 0.094. Two independent lines of evidence -- the replicate
leg and the fresh-seed re-measurement -- agree that these are one policy quality sampled four times.

`r5/checkpoint-1040` is the sensible default: best on fresh seeds by margin (+2.611), win (90.7%),
waste (1.85%) and clearance (93.75%). Not because it is better -- it is inside noise of the others --
but because nothing argues for picking a different one.

Note r4's decline rate: **8.38%**, against 1.28% and 2.87% for the other two. It passes up scoring
chances far more often, and it was the leg that looked best on its own selection seeds.

r4b timing out on this range is the map-fragility problem again: different seeds draw different maps
and these policies produce unplayable games on some of them.

## Two biases worth carrying forward

**Selection over checkpoints.** Each leg's winner is the maximum of ~15 noisy checkpoints, so its
margin is upward biased by the selection itself. Within-leg spreads are larger than any between-leg
difference: r4 ranged +1.942 to +2.587, r5 +2.031 to +2.526, r4b +2.115 to +2.364. Leg *medians* —
r4 2.16, r4b 2.25, r5 2.13 — order differently from leg winners, and the two same-recipe legs
bracket the supposedly-better one.

**Self-play cannot measure this.** The in-training VP column rose from ~1.85 to ~2.8 across the
campaign, but the five opponents improve in lockstep with the candidate, so a gain against a fixed
standard is masked and a plateau is invisible. Judge stage 2 only against a frozen opponent.

## The schedule finding, which cost the most time

Later checkpoints cannot be evaluated at all. Of run 3's twenty checkpoints, **seven timed out** at
four minutes each, where the champion plays 144 games in three seconds.

The first guess was a decision loop, so `crossplay_eval` gained `--max-steps` and stopped treating a
step limit as fatal. That guess was wrong: lowering the cap to 1500 did not help and no game ever
reported a truncation. The cost is not the *number* of decisions but the cost of each one — these
policies accumulate board state until combat and production steps get slow. No step cap bounds that,
so the bound is wall-clock per checkpoint.

The consequence for scheduling: training longer does not merely erode margin, it produces policies
that cannot be measured. Legs are 150 updates checkpointing every 10, not 600 every 50. The useful
window is the first few tens of updates, which is also where every leg's winner was found.

## Waste, per faction

The per-faction penalties (`sol 15, letnev 12, jolnar 8, l1z1x 8, xxcha 5, hacan 5`) worked
unevenly. Against r3's winner, r4b's winner reads: hacan 0.83%, letnev 0.00%, xxcha 0.00%, jolnar
1.67%, sol 1.67%, l1z1x 2.50% — table **1.11%**, down from 3.33%. Jolnar's 9.17% in r3 is gone.

Scalar penalties would have punished hacan and xxcha, which were already at zero.

## What is worth doing next

1. **Stop chaining legs.** Three consecutive legs bought nothing. The gain came from the first
   application of a properly-priced reward, not from repetition.
2. **Give scoring its own head.** `STAGE1_DECISION_HEADS` has fourteen entries and `scoring` is not
   among them, so every score decision lands in the `other` catch-all with abilities, agendas,
   exploration and transit. The policy learned to score anyway — its decline rate went 58.8% to
   near zero — through a head it shares with everything unnamed. Head weights are rows
   (`w_shared[h] + delta[f, h]`), so a fifteenth row seeded from `other` starts numerically
   identical and is then free to specialise. The bundle manifest records and validates the head
   list, so it needs a migration rather than a constant edit.
3. **Find out why later checkpoints become unplayable.** A policy that cannot finish a game in
   bounded time is a defect, not merely an inconvenience, and it currently caps how long any leg
   can usefully train.
