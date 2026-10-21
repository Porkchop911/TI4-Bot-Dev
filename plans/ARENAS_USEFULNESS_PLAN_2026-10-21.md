# Making the battle and trade arenas useful — plan, 2026-10-21

Status: **plan, nothing started.** Every run below needs the operator's explicit go and a line in
`plans/COMPUTE_QUEUE.md`. Reward/feature code is Tier C, so it needs a frontier review before it
trains a long run.

## 1. Where things stand (r7/r8 line)

| | What reaches the policy | What is not used |
|---|---|---|
| **Battle arena** | Predictor **v8** (lean arena, max 8 ships / 16 fighters, 60k steps) as `action-plan:battle-*` option facts at activation, movement and retreat choices. It is frozen: the same file is carried from the blank bundle through r7 into r8 | The ground network is still **v7's**: wide-roster mechs and ground abilities are not covered. v8's labels **predate the 2026-10-11 arena repairs**, so the corrected source has no qualified predictor. Nothing measures whether the policy actually uses these facts |
| **Trade arena** | The hand-written deal value sheet as three option facts per diplomacy choice (`deal-value-own/partner/score`) | The trade teacher (held-out accuracy 29% → 70%) and the PPO trade arena were never applied to this line: r7 started from a blank diplomacy readout. Negotiation is learned only through full-game PPO plus the 0.0005 per-step cost. Earlier teacher evidence showed no game gain (3.169 vs 3.171 VP) |

Two symptoms in r7's greedy eval (`eval320_r7_waste.txt`) bear on this:

- **Waste:** 3.54 wasted activations per seat-game, 28% of tactical actions. A wasted activation
  moves, builds and lands nothing, which is exactly the decision the battle facts are meant to inform.
- **Diplomacy volume:** diplomacy already took 41–56% of all decisions in older checkpoints
  (TRADE_ARENA_VALUE_SHEET §1), so whether those decisions do anything matters for speed as well as
  for points.

## 2. Principle

**Measure first, then fix what the measurement points at.** Both arenas produce local metrics
(predictor error, imitation accuracy) that have never translated into measured VP. Each step below
ends in a **paired greedy eval** on the same holdout seeds (production_eval, 320 seeds, T 0.001, 5
rounds, diplomacy on). The deciding numbers are VP per seat-game, wasted activations and decisions
per game. A local metric alone never promotes anything.

## 3. Phase A — does the policy use the facts? (CPU only, ~1 day of work)

- **A1. Fact ablation switch.** Add a production_eval flag that zeroes one fact family at inference
  (`--zero-facts battle` / `deal`), leaving the model unchanged.
  - Run it on r7 checkpoint-10800, and later r8, paired against the unmodified model.
  - Report per faction: ΔVP, Δwasted activations, Δdiplomacy decisions.
  - *Done when:* both ablations have 320-seed paired results with confidence intervals.
- **A2. Calibration in real games.** Log predicted versus actual outcomes for every space and
  ground combat that happens in eval games: attacker win, mutual destruction, cost lost.
  - Report calibration (Brier score, reliability by bucket) on the real fleet distribution.
  - Report the share of real fights the predictor marks *unsupported* or out of range (>8 ships,
    >16 fighters, wide-roster ground units).
  - *Done when:* there is a calibration table per faction plus the unsupported share.
- **A3. What wasted activations look like.** Run the existing `empty_activations` split
  (*unreachable* vs *declined*) for the wide roster and 5 rounds, on r7/r8 checkpoints.
  - Unreachable means a targeting failure: the predictor and movement facts aren't preventing it.
  - Declined means a valuation failure: the seat had a move and chose not to make it.

**Decision gate A:**

| Result | Next |
|---|---|
| Zeroing battle facts changes nothing | The policy ignores them, so Phase B must change how they reach the policy (B3/B4), not just retrain the predictor |
| Calibration is poor where real fights happen | B1/B2 first |
| Zeroing deal facts changes nothing | The value sheet isn't steering diplomacy; go to C1/C2 |

## 4. Phase B — battle arena

- **B1. Qualify the corrected arena, then train predictor v9.** This closes the open BF-22b items.
  - Run a corrected-source label panel against the engine, at the 200-scenario scale of the original
    agreement table.
  - Train a **wide ground network** (mechs and ground abilities of all seatable factions).
  - Retrain on the corrected labels; bump the feature version and refuse older encodings.
  - Swap v9 into r8's checkpoints with `swap_battle_predictor`.
  - *Done when:* v9 has held-out error at least as good as v8 on the corrected labels, engine
    agreement on the panel (0 of N scenarios with |z| > 3), and a paired eval no worse than v8.
- **B2. Train on the fleets that actually fight.**
  - Sample predictor training positions from fights recorded in r7/r8 games (A2's log) instead of
    uniform random fleets.
  - Keep a uniform slice so rare fleets stay covered.
  - Raise the 8-ship / 16-fighter cap if A2 shows real fights beyond it.
- **B3. Turn odds into an activation-level fact.** Today the facts describe individual fights.
  - Add one fact per activation option: *the best achievable outcome of this activation* — the best
    fight, take or build any legal follow-up reaches, or "nothing reachable".
  - This targets the *unreachable* half of waste directly, alongside the waste penalty (11 in r8).
- **B4. Unify with the power map.**
  - The power map's threat and opportunity numbers use a Lanchester formula fitted to the arena
    (σ(1.78·ln r − 0.14)). Replace that formula with the predictor where it is supported.
  - Expose *threat to my systems* and *best opportunity* as critic-state facts, so the critic sees
    the same combat picture as the actor.

## 5. Phase C — trade arena

- **C1. Apply the trade teacher to the faction line.** This is a short pilot, not a long run.
  - Run `trade_teacher` on the r8 per-faction checkpoints, training **only** the diplomacy readout and
    deal-fact rows, as before.
  - Paired eval with diplomacy on.
  - *Done when:* the paired eval shows ΔVP with a confidence interval plus the change in diplomacy
    decisions per game. A pass is ΔVP ≥ 0 with fewer wasted negotiation steps.
- **C2. Value deals with the faction's own goals.** The sheet prices items generically.
  - Add the deal's effect on the receiving seat's **faction-term potential** (the per-faction
    milestones in `faction_terms`), e.g. a promissory note or trade goods that complete a goal.
  - Keep the sheet's own-gain-minus-partner-share structure.
  - Inputs stay seat-visible.
  - SftT pricing is out of scope (operator rule).
- **C3. Critic-valued deals, if C1/C2 aren't enough.**
  - Score a proposed deal by the learned critic's value of each seat's position after the deal minus
    before it, on a copied state, instead of the hand-written sheet.
  - This makes the trade arena's signal consistent with what full-game PPO optimises.
  - It costs one critic evaluation per candidate deal. Measure the slowdown first and cap the number
    of candidates.
- **C4. Track what deals do.** Log settled deals per game: items, value-sheet score, whether
  promised items were delivered, and the VP and faction-goal change over the next round.
  - This is the first time deal quality would be measured in full games, not just the deal count.

## 6. Order and cost

| Step | Compute | Rough effort | Depends on |
|---|---|---|---|
| A1 ablation switch + evals | CPU, ~10 min per eval | small code change | — |
| A2 combat calibration log | CPU | moderate (engine event hook in the eval) | — |
| A3 waste split | CPU | existing example, extend to wide/5 rounds | — |
| B1 v9 + wide ground net | CPU labels (16 workers) + GPU fit, hours | large; the review is outstanding | gate A |
| B2 real-fleet sampling | as B1 | moderate | A2 |
| B3 activation-outcome fact | needs a new vocabulary slot, so a migration | moderate–large | A3 |
| B4 power map ← predictor | small runtime cost | moderate | B1 |
| C1 trade teacher pilot | ~3 min teacher + eval | small (existing tool) | — |
| C2 faction-goal deal value | CPU | moderate | C1 |
| C3 critic-valued deals | slower games | large | C1/C2 results |
| C4 deal tracking | CPU | small–moderate | — |

**Suggested start:** A1, A3 and C1, in parallel where files don't overlap. All three are cheap,
reuse existing tools, and decide whether the bigger battle-arena work (B1–B4) is worth doing at
all. No change touches the running r8. Any feature change that needs a new vocabulary slot
(B3, B4) must go through the bundle migration path, not a silent schema change.
