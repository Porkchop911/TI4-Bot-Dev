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
