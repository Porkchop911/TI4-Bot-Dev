# F-M10-034-C1 — "cleared" means a different thing in Stage 2 than the reward says it does

**Found by:** Claude Opus 5 (implementer), 2026-08-27, while smoke-testing the Stage-1 MLP driver.
**Severity:** HIGH. **Status:** open — needs an independent reviewer and an operator decision.

## The observation

`ti4_engine::opening::measure` has no notion of a round. It takes a `GameState` and a setup
snapshot and reports the deltas between them. `rollout::finish_game` calls it on `game.state` —
the state at the end of `horizon.rounds`.

So `Episode::cleared` means "reached the opening bar by the end of the horizon", and the horizon
is a parameter:

| horizon | what `cleared` means | measured on `checkpoint-shared-1` |
|---|---|---|
| `rounds: 1` (Stage 1) | reached the bar by the end of round one — the opening gate | **67.19%** (576 seat-games) |
| `rounds: 4` (Stage 2) | reached the bar at any point in four rounds | **83.53%** (28,800 seat-games) |

The same policy, the same bar, a 16-point gap.

## Why this is load-bearing

**The reported clearance is not the opening gate.** Every `run-001`/`run-002` clearance figure is
the four-round measure. Read as "stage-1 clearance" — which is how the column is labelled and how
it has been reported to the operator — it overstates the opening by about sixteen points. The
operator's standard for this quantity is 100%; the number being tracked against that standard is
the wrong one, and it is wrong in the flattering direction.

**Stage 2's round-one bonus contradicts its own justification.** `reward::returns` credits
`r1_bonus * (cleared - 0.1 * shortfall)` at the last round-one decision, and the comment explains
the placement:

> Credited at the last decision taken in round one, so every round-one decision carries it and no
> later one does. A round-three decision cannot change whether round one cleared, and paying it
> there would only add variance.

Under a four-round horizon a round-three decision *can* change whether `cleared` is true. The
rationale is correct about round-one clearance and the code is not computing round-one clearance.
The bonus therefore pays round-one decisions for expansion that happened in rounds two to four —
credit assigned to decisions that did not cause it, which is the failure the placement exists to
avoid.

**It is not a Stage-1 problem.** The linear Stage-1 trainer plays `Horizon::opening()`, one round,
so its `cleared` is the genuine gate. The ambiguity appears only when the same field is read under
a longer horizon.

## What this does not establish

That the four-round measure is *useless* — "reached the bar eventually" is a real quantity, and
reporting it is defensible if it is labelled as that. The defect is that it is labelled and used as
the opening gate, in a reward term whose stated purpose is to price round one.

Nor does it explain `run-002`'s clearance decline on its own. That decline is in the four-round
measure; whether the one-round measure fell further, less, or not at all is unmeasured, because
nothing records it under a Stage-2 horizon.

## Recommended disposition

1. **Measure the opening where the opening happens.** Capture `opening::measure` against the state
   at the end of round one regardless of horizon, and carry it on `Episode` beside the terminal
   one. This makes the Stage-2 round-one bonus compute the thing its comment describes and gives
   the report a column that means what it says.
2. **Relabel until then.** The existing column is "reached the bar within the horizon", not
   "stage-1 clearance".
3. Treat any prior clearance claim in M10-032/033/034 evidence as the four-round measure.

Whether to change the Stage-2 reward is an operator decision: it alters a pre-registered reward
term, and the run that exposed it is still in flight.
