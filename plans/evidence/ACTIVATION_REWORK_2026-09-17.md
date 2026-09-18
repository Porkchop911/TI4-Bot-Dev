# Activation rework — evidence

Plan: `plans/ACTIVATION_REWORK_PLAN_2026-09-17.md`. Restore point:
`restore/pre-activation-rework-2026-09-17`.

## Phase 0 — defects (b1330a5, ti4-sim v42 at 1ab4185)

| id | finding | fix | test |
|---|---|---|---|
| D1 | a carried mech sustained a space hit | one `sustains_in_space` predicate (ship only) for both sustain paths; the combat window also gains the war-sun law and Metali shielding | `a_carried_mech_cannot_sustain_a_space_hit`, `a_combat_window_offers_no_sustain_to_a_carried_mech` |
| D2 | Skilled Retreat played by a seat not in the combat | the round event names attacker and defender; the four combat-round card windows admit only them (every such card acts on the player's own units in the combat) | `only_the_combatants_may_play_a_combat_round_card` |
| D3 | reviewer frames 1742–1746 repeated at 1747–1751 | **engine bug:** Ceasefire was spent on the first move attempt, then movement went ahead. It now denies movement for the whole activation; a holder of the same faction no longer counts as a loan (fixture seats share a placeholder faction) | `a_ceasefire_denies_movement_for_the_whole_activation` |
| D4 | arena round cap reported like mutual destruction | `Outcome::unresolved`, `ROUND_CAP`; the predictor trainer reports the count | `a_fight_nobody_can_win_is_unresolved_not_mutual_destruction`; 0 of 40,000,000 probe fights hit the cap |

ti4-sim: `faction_differentiation` left v41 (0.642 < 0.793). Bisection: the Ceasefire fix alone
stays inside v41; the two combat fixes move it. Re-baselined to v42 (`plans/evidence/M08-021.md`).

## Phase 1 — corrected activation facts (fact version 5)

Renames were dropped: the reach features already have distinct names and meanings, which are now
documented at their definitions (`activate-can-reach`: some hull, boosts one hull at a time;
`target:reachable`: printed-distance heuristic). Instead of changing `activate-enemy-ground`
(which old checkpoints read), arena bundles at fact version 5 also see
`action-plan:activate-enemy-ground-forces` and `action-plan:activate-enemy-structures`. Fact
versions from 5 reuse version 4's encoding; `migrate_arena_facts` relabels the predictor.

`checkpoint-212544-arena-v5` (from arena-v4, 2 zero rows appended):
`arena_migration_check` against plain 212544, 3 seeds x 4 rounds: **IDENTICAL**, 8,875 decisions,
wall time x1.025.

## Phase 2 — candidate generator (`ti4-policy::tactical_plan`, `ti4-policy::fleet_strength`)

`package_legality` with `checkpoint-212544-arena-v5` (near-greedy play at T=0.25), 4 seeds x 4
rounds, every 2nd tactical-capable turn:

| measure | value |
|---|---|
| positions / offered systems / packages | 800 / 29,556 / 16,937 |
| menu size per system | mean 0.57 (most offered systems are out of reach), max 5 |
| by strategy | light 5,665 · strong 3,767 · hold 2,896 · efficient 2,367 · capture 2,200 · origin-preserving 42 |
| executed as planned | 15,492 |
| interrupted, explained | Ceasefire 1,445 (the note is held in hand, so the generator cannot see it) |
| generation errors / dropped loads / wrong arrivals | **0 / 0 / 0** |
| generation cost | 4.8 ms per activation decision (every offered system, predictor on) |
| shortlist recall | 2,580 contested systems with at most 8 movable ships: mean regret 0.010 (win − own cost lost in tens) against every subset; regret > 0.1 in 3.0% |

Found and fixed on the way: a fighter moving on its own was also planned as cargo
(`a_fighter_moving_on_its_own_is_not_also_loaded`).

## Phase 3 — arm A: candidate-fleet summaries on activations (fact version 6)

Activation options of a version-6 bundle carry, from the generator's menu for that destination:
best space win, cost of the cheapest favoured fleet (win >= 0.5, in tens), best conditional take,
the least home exposure among favoured fleets, the number of candidates, and a flag when a fight
cannot be priced. The 22 package facts (read from version 7) are appended to the vocabulary too.

`checkpoint-212544-arena-v6` (from arena-v5, 28 zero rows appended): `arena_migration_check`
against plain 212544, 3 seeds x 4 rounds: **IDENTICAL**, 8,875 decisions.

Cost: wall time x1.26 at first. Three changes brought it to **x1.12–1.13** (two runs):
predictions cached per destination; whether a fight follows decided once per destination (with
next-door guns, which the first version missed); unreachable destinations skipped early; and the
engine's reachability search memoised on the observation, which the ordinary activation features
and the generator both ask. Generation alone: 2.7 ms per activation decision. Above the 10%
engineering budget; left there for the pilot, where inference and the critic take a larger share.

## Phase 4 — arm B: the fleet decision and its plan (fact version 7)

After the model answers an activation, a version-7 bot builds the destination's menu, scores it as
an ordinary movement-head decision (the candidates, plus "build the fleet step by step") and
samples one. The choice is recorded as its own PPO step, so the two levels factor as
P(system) x P(fleet | system) and a destination with more candidates gets no extra weight. The
chosen fleet then answers the engine's movement and cargo prompts; those answers are not recorded
as policy steps.

**Notes and records.** `ppo_update` used to rebuild its per-decision notes from the engine's
prompts and required one note per recorded step. A fleet decision has no engine prompt and a
planned step has no record, so the notes now come from the bot (`MlpBot::ppo_notes`), pushed with
each record. A fleet decision's note is a movement note, declined when the fleet moves nothing, so
the wasted-activation charge keeps working.

**Likelihood check** (`ppo_update --check-likelihood`, one update, 96 games, temperature 2.5,
rescored on a CPU inference copy like the rollouts):

| bundle | steps | max abs(log p - recorded) | steps by head |
|---|---|---|---|
| arena-v6 (arm A) | 37,516 | **0.0** | movement 1,350 · cargo 2,366 · landing 1,331 |
| arena-v7 (arm B) | 31,343 | **0.0** | movement 947 (fleet decisions included) · cargo 369 · landing 594 |

Rollout time was 22.1s (arm A) and 21.2s (arm B) for the same 96 games.

**Plan execution in play** (`arena_migration_check`, 6 seeds x 4 rounds, near-greedy): 274 fleet
decisions, 1,195 prompts answered by plans, **0 plans stopped**. The first run stopped 2 plans, both
a boosted move that was no longer offered: an earlier move can lend the rest of the fleet its
movement (Gravleash), so the executor now accepts the same ship needing fewer boosts than planned.

**Reviewer.** A fleet decision appears as its own entry ("fleet decision") with the menu, each
candidate's facts and its probability; prompts the plan answered are marked "planned". One
near-greedy game: 56 fleet decisions, 218 planned prompts.

**Starting behaviour.** The new rows are zero, so a freshly migrated arm B picks uniformly among
the candidates (and almost never "step by step"). Arm B therefore starts from a real change in
play, unlike arms 0 and A, whose migrations are identical to the source.

## Phase 5 — the three pilots (50 updates each)

Same seeds, pool, frozen opponents (plain 212544), VP-only reward, 4 rounds, 96 games per update.
Evaluated against the champion (`checkpoint-19280-diplomacy-v11`), 20 seed blocks, 4 rounds, on one
evaluator build.

| arm | learner | VP | margin | lead | cleared | final checkpoint |
|---|---|---:|---:|---:|---:|---|
| start | checkpoint-212544 | 3.12 | −1.52 | 12.2% | 94.0% | — |
| 0 (corrected facts) | arena-v5 | 3.15 | −1.66 | 11.7% | 86.9% | checkpoint-1812 |
| **A (information)** | arena-v6 | **3.43** | **−1.28** | **16.4%** | 88.6% | checkpoint-1804 |
| B (packages) | arena-v7 | 2.74 | −1.98 | 9.3% | 91.7% | checkpoint-1768 |

95% seed-block intervals on the margin: arm 0 −1.85 to −1.48, arm A −1.45 to −1.12, arm B −2.08 to
−1.88. Arm A's interval clears arm 0's and the start's; arm B's is below both.

**Cost.** Wall time per update: arm 0 25.4s, arm A 25.5s, arm B 25.8s — within 2%, so the
generator disappears into the rollout even though a near-greedy check measured x1.12.

**Reading.** Telling the model what could be sent, while it still moves ship by ship, is what pays
at this budget. Choosing the whole fleet does not, and the pilot cannot separate the action space
from its start: arm B's fleet rows begin at zero, so it opens by picking uniformly among a
destination's candidates, while arms 0 and A open exactly as 212544 played. Screening only, one
seed base, 50 updates.

## Arm B trained on, 2026-09-18

Two continuations of the arm B pilot, both on the fleet-decision architecture (fact version 7),
evaluated greedily (candidate temperature 0.001) against the champion, 20 seed blocks, 4 rounds.

| run | reward | updates | VP | margin | lead | cleared | waste |
|---|---|---:|---:|---:|---:|---:|---:|
| start (212544) | — | — | 3.12 | −1.52 | 12.2% | 94.0% | 12.6% |
| arm A (information) | VP only | 50 | 3.43 | −1.28 | 16.4% | 88.6% | 22.6% |
| arm B pilot | VP only | 50 | 2.74 | −1.98 | 9.3% | 91.7% | 32.1% |
| arm B, pure VP | VP only | +201 | 3.15 | −1.49 | 13.9% | **71.7%** | 59.4% |
| arm B, opening priced | VP 1, clearance 0.5, r1 3 / 0.1 | +1200 | 3.21 | −1.56 | 12.1% | 85.7% | 71.7% |

Victory points recovered to the starting level and beyond the pilot, but neither run reaches arm A,
and two things get worse the longer arm B trains:

- **Clearance.** VP-only took it to 71.7%, far below the start's 94%. `clearance-weight 0.5` with
  the opening priced (`r1-bonus 3`, `r1-shaping 0.1`, the reward's own defaults) brought it back to
  85.7%, still short.
- **Wasted activations.** 12.6% at the start, 22.6% for arm A, then 59.4% and 71.7% here. Nothing in
  either reward prices waste (`waste-penalty` is 0 and reported only as a diagnostic).

**What the fleet decision learned** (one greedy holdout game per checkpoint, fleets picked):

| run | step by step | capture | strong | light | efficient | hold |
|---|---:|---:|---:|---:|---:|---:|
| pure VP, +201 | 31 | 8 | 6 | 0 | 0 | 3 |
| opening priced, +1200 | 29 | 17 | 10 | 3 | 2 | 0 |

The manual option started at probability 0.003 (its row is zero at migration) and is now chosen about
half the time: given the choice, the trained policy often goes back to building the fleet ship by
ship. That is a result about the menu, not only about training length.
