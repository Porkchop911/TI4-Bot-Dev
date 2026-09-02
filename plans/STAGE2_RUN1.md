# Stage 2, first run

300 PPO updates, four rounds, initialised from the stage-1 champion
`out/champions/best-94.97_r2-epoch22`. Waste penalty 5, uniform across factions.

## The headline

Against a **fixed** benchmark (five frozen copies of the stage-1 champion, greedy), the same
weights before and after, 720 candidate seat-games on the Validation pool:

| | champion | after 300 updates | null |
|---|---|---|---|
| VP (greedy) | 0.040 | **1.299** | — |
| margin | −0.178 | **+0.858** | −0.150 |
| win | 3.6% | **59.2%** | 3.3% |
| declined a scoring chance | 58.8% | **10.6%** | — |
| wasteful games | 1.11% | **0.00%** | — |

The margin null is *negative* (−0.150): the candidate is one draw and the best opponent is the
maximum of five, so an identical policy scores below zero by construction. +0.858 is measured
against that, not against zero.

## The cost, which is not acceptable as it stands

Stage-1 convention, greedy, 21,600 seat-games:

| | clearance |
|---|---|
| champion | 94.97% ±0.21 |
| after 300 updates | **92.44% ±0.35** |

−2.53 points. That is outside the measurement interval **and** outside the 1.54-point training
noise floor, so it is a real trade, not run-to-run variation. Round-one quality was sold for
mid-game points.

Per faction the loss is uneven: Xxcha *rose* to 98.94%, everything else fell to 90–92%.

## The mistake I made reading this run

While it trained I reported "clearance recovers, VP does not move" from the in-training table,
which showed VP 1.849 → 1.738. That reading was wrong, and wrong for a reason worth writing down.

**The in-training number is self-play.** Six copies of one policy play each other, and the points
available at a table are close to fixed, so mean VP per seat is pinned regardless of how much
better every seat gets. It cannot show improvement, ever. It is the exact tautology
`crossplay_eval` was built to escape — and it was still read as progress, by me, for 300 updates.

Judge a stage-2 run only against a frozen opponent. The in-training VP column is a diagnostic for
collapse, not a measure of skill.

## The structural finding

`STAGE1_DECISION_HEADS` has fourteen entries and `scoring` is not one of them. `decision_head`
routes a score choice to `"scoring"`; `Actor::resolve_head` does not find it and returns `"other"`,
the catch-all shared with abilities, agendas, exploration and transit. The network has no dedicated
capacity for the decision stage 2 exists to teach.

It learned anyway — the decline rate fell 58.8% → 10.6% through the shared head. What that head
cost is unmeasured. Expanding is cheap and behaviour-preserving: head weights are rows
(`w_shared[h] + delta[f, h]`), so a fifteenth row seeded from `other` starts numerically identical
and is then free to specialise. The bundle manifest records the head list and validates it on load,
so it needs a migration path rather than a constant edit.

## What run 2 changes

1. **Clearance becomes an acceptance gate, not a coefficient.** The stage-1 waste work established
   this: `--waste-ceiling` on checkpoint selection worked where a reward coefficient did not. Same
   shape here — reject any checkpoint below a clearance floor, then maximise margin among what
   survives. Raising `r1_bonus` instead just re-runs the argument the coefficient already lost.
2. **Keep the waste penalty.** It is doing its job: 0.00% wasteful games at greedy, from 1.11%.
3. **Give scoring its own head**, seeded from `other`.

## Reproduce

```text
ppo_update --bundle out/champions/best-94.97_r2-epoch22 --stage 2 --rounds 4 \
  --temperature 2.5 --movement-entropy 0.05 --entropy-final 1 \
  --learning-rate 3e-4 --waste-penalty 5 --updates 300 --device cuda \
  --out out/checkpoints/stage2-pilot
```

Evaluated with `crossplay_eval --opponent out/champions/best-94.97_r2-epoch22` and
`clearance_eval --temperature 0.001 --seeds 600`.
