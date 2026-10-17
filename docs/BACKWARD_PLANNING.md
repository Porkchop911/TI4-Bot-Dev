# Backward planning for the rule bot (shipped in bot version 4)

Code: `crates/ti4-rule-bot/src/backward/` (module docs there). All knobs `bw_*` in `tuning.rs`;
the `bw_*` knobs are **on in bot version 4** (see Final decisions) and off in versions 1 to 3, which stay unchanged and hash-identical.

## Idea

From round `bw_round` (3) the seat looks at the objectives it can still score (revealed publics and held
secrets) and plans *backwards* from each: what must be true, what is the cheapest route this turn, how likely
is it to work, what does it cost. A weighing step compares the plans with each other and with Mecatol Rex /
Imperial and picks the best expected VP. Below a threshold nothing steers and the default logic plays unchanged.

## Design

- `ObjectivePlanner` (trait): `objective_ids()` (aliases or family tokens) and `plan(obs, ctx) -> Option<Plan>`.
  The dispatcher (`verdict`) calls it for every unmet goal that matches, with `ctx.goal` set. One source file per
  group: `territory.rs`, `fleet.rs` (Agent A), `mecatol.rs`; Agent B's groups (payment, technology, combat).
- `PlanCtx`: round, tokens, resources, influence, trade goods, Mecatol state, and `steps`: every activation that
  takes a planet or brings a new ship, with its success odds (fight odds from the fleet-strength matchup, 0.9 for a
  landing, 0.6 when the ship would have to leave a system it alone holds).
- `PlanCtx::route`: the fewest steps (<= 3, <= tactic tokens, best 7 candidates) whose imagined result makes the goal
  met, judged by the engine's own requirement functions (`Imagined` planets / systems). Probability is the product
  of the step odds, 0.9 per extra step.
- `weigh`: expected VP = probability x marginal points - `bw_token_cost` per token - `bw_resource_cost` per
  resource. A second public objective only counts a quarter unless Imperial is in play (as `objectives::gain`).
  With two likely public plans (>= `bw_likely`, a met public counts as one) the Imperial card is wanted and each
  likely plan gains `bw_imperial_bonus`. Spend-N plans lose `bw_payment_discount` of their value. The Mecatol plan
  (custodians point if the token stands + 0.6 Imperial point; or hold + play Imperial) competes as one more candidate.
  Best expected VP below `bw_threshold`: fallback, no steer.
- Integration (`objectives.rs`): the chosen plans add `bw_steer x expected VP` to the activation value of the systems
  on their routes (so both the tactical-vs-pass decision and the system choice see it); the Imperial draft gets +20
  when the verdict wants Imperial. Movement, landing and production are the existing logic.

## Knobs

`bw_enabled`, `bw_round`, `bw_threshold`, `bw_steer`, `bw_likely`, `bw_imperial_bonus`, `bw_payment_discount`,
`bw_token_cost`, `bw_resource_cost`, `bw_mecatol`, `bw_mecatol_weight`, and group switches `bw_territory`,
`bw_fleet`, `bw_payment`, `bw_technology`, `bw_combat`.

## Per-objective logic

- Territory (`non_home`, `on_the_rim`, `same_trait`, `tech_specialties`, `planets_of_trait_*`, `attached_planets`,
  `legendary_planets`, `rival_home_planets`, `shared_planet_systems`, `notable_systems`): route of planet-taking steps.
- Fleet (`ships_in_systems`, `planetless_systems`, `ship_systems_beside_*`, `ships_adjacent_to_mecatol`,
  `rival_dock_systems_with_ships`, `mecatol_ships_while_controlling`, `distinct_rival_home_reaches`,
  `weaker_neighbours`): route of ship-arriving steps. Fleet-size goals (`fleet_in_one_system`, capital ships) stay
  with `planner::Bias`.

## Prep layer (two-step plans for every planner)

`backward/prep.rs` appends *prepared steps* to `PlanCtx::steps`: an activation preceded by a prep, judged on the
position after the prep. `route()` / `route_plan()` see them without change (territory, fleet, Mecatol); a route holds
at most one prepared step and stays within the tactic tokens (preps included), strategy tokens, resources and 3
activations. Probability of a prepared step = prep odds x act success x 0.9. `plan_of` copies the prep into the plan:
cost, `steer.systems` (prep activations first), `builds`, `cards` (drafted), `play` (played now), `gather`, `techs`.

| kind | knob | prep | adds to the act step |
|---|---|---|---|
| Build | `bw_prep_build` | home dock activation, Construction primary production (Thunder's Edge), Warfare primary (free activation of the dock, Thunder's Edge), printed Warfare's freed token for a dock activation, Warfare secondary (home production, 1 strategy token, odds 0.6) | ground forces to land, a carrier, cruisers to win the fight, a ship to send |
| Warfare | `bw_prep_warfare` | a gather activation beside the target (paid by Warfare's extra activation when tokens are short) | the ships and cargo that reach the neighbour but not the target; the movement hook (`Steer::gather`) moves and loads everything there |
| TechRange | `bw_prep_tech_range` | Gravity Drive or a faster unit upgrade (own Technology card 0.95, rival secondary 0.7 for 1 strategy token + 4 resources, B's model) | ships one hex short (`Observed::can_reach` with the new move value) |
| TechStrength | `bw_prep_tech_strength` | a unit upgrade | the fleet's strength (`fleet_strength::matchup` on the upgraded counts) |

Engine facts that shaped it: the Construction secondary only places a structure (no production), the printed Warfare
primary only recalls a command token, Warfare's secondary produces at the home system; only the Thunder's Edge cards
(`te4construction`, `te6warfare`) give a production / free activation to the card's owner. Limits: fleet supply is not
checked for the build prep's cruisers/carriers; the Warfare gather has no ordering guarantee for secondaries; only unit
upgrades (not Magen/Plasma-type combat technologies) count as strength; the Mecatol plan is the only user besides
territory and fleet (B's planners may read `ctx.steps`).

Mecatol two-step plans (`mecatol_two_step.rs`) are the prepared steps on Mecatol Rex's system when the one-step plan is
below `bw_likely`.

## Fleet shape (`fleet_shape.rs`, `bw_fleet_shape`)

`raise_fleet`, `command_armada` (gather the ships in one system: one activation and the gather hook; or build the
cheapest missing ships), `engineer_marvel` (build the flagship), `supremacy` (move the flagship / war sun to Mecatol Rex
or a rival home, or build it then act), secret `otf` (ground pile: carriers' forces on one planet, optional Warfare
gather, commit hook piles them).

## A/B

Fixed table (shared with Agent B): 4 players, map 4pBgaBalancedD, factions sol/letnev/jolnar/mentak, cyclic rotation,
mixed mode, seeds 20000.., baseline = knobs off. Games in `/root/rulebot-results/backward-a/`. All numbers are
candidate minus baseline, 95% intervals, n = paired tables.

| candidate | n | ctrl VP / round | ctrl R3 | ctrl R4 | ctrl R5 | VP R5 | win pp | notes |
|---|---|---|---|---|---|---|---|---|
| `bw_enabled` (base: territory, fleet, Mecatol) | 600 | +0.008 [-0.006, +0.021] | +0.020 | +0.039 [+0.002, +0.076] | +0.046 | +0.116 [+0.049, +0.184] | +2.5 [+0.3, +4.7] | imperial_primary +0.048 |
| + `bw_prep_build` | 400 | +0.032 [+0.014, +0.051] | +0.044 [+0.011, +0.077] | +0.116 [+0.062, +0.170] | +0.168 [+0.085, +0.251] | +0.184 | +2.9 [+0.1, +5.6] | objective +0.092, secret +0.076 |
| + `bw_prep_warfare` | 400 | +0.019 [+0.001, +0.037] | +0.043 | +0.083 [+0.027, +0.139] | +0.124 | +0.153 | +2.5 [-0.5, +5.5] | |
| + `bw_prep_tech_range` | 400 | +0.028 [+0.009, +0.047] | +0.041 | +0.106 [+0.054, +0.159] | +0.162 | +0.199 | +2.6 [-0.3, +5.5] | secret +0.062 |
| + `bw_prep_tech_strength` | 400 | +0.019 [+0.001, +0.038] | +0.037 | +0.099 [+0.046, +0.151] | +0.127 | +0.158 | +1.5 [-1.4, +4.4] | secret +0.062 |

| + `bw_fleet_shape` + build | 400 | +0.021 [+0.002, +0.040] | +0.039 | +0.105 | +0.128 | +0.156 | +0.9 [-2.0, +3.7] | no gain over build alone |
| all four preps + fleet shape | 400 | +0.027 [+0.008, +0.047] | +0.050 | +0.125 | +0.161 | +0.188 | +2.1 [-1.0, +5.2] | preps do not stack |
| **direct**: build + fleet shape vs build only | 400 | **-0.013 [-0.023, -0.003]** | -0.002 | +0.002 | -0.049 [-0.086, -0.011] | -0.077 | -0.25 [-2.0, +1.5] | objective -0.061, secret -0.024: fleet shape hurts, keep off |

Reading: the base framework gives a small early gain (mostly Imperial's Mecatol point); the build prep is the strongest
single addition (ctrl +0.032 per round, +0.17 after R5, objective and secret VP both up); tech range is close, warfare and
tech strength smaller; the preps do not add up and fleet shape is a net loss. Gates: golden bench identical to
`baseline.json` (200/200) with the knobs off; `bot_version_check` fails on seed 2 identically on the unmodified base
(pre-existing).

## Open ideas

- Combine the preps with a per-turn budget (they compete for the same tokens and resources).
- Fleet-supply check for builds; Magen/Plasma-type combat technologies as strength preps.
- Draft-phase modelling of which card a rival will take (Warfare secondary odds are a constant 0.6).
- Reorder Imperial's play after the plan's steps are done.

## Final decisions (bot version 4, 2026-10-16)

Felix decided to ship everything except the fleet shape. Bot version 4 turns on `bw_enabled`, the four preps (`bw_prep_build`, `bw_prep_warfare`,
`bw_prep_tech_range`, `bw_prep_tech_strength`), the group switches (territory, fleet, payment, technology, combat) and Agent B's second-round
knobs (`bw_combat_abilities`, `bw_agenda`, `bw_holdings`, `bw_ttfd`, `bw_production`). **`bw_fleet_shape` stays off**: in the direct A/B
(build + fleet shape against build alone, 400 tables) it cost -0.013 ctrl VP per round [-0.023, -0.003] and -0.049 ctrl VP after R5; the
code and its tests stay behind the knob. The preps do not add up in the fixed-table A/Bs above, but the combination is what v4 measures
(`docs/RULE_BOT_OPTIMISATION.md`, Round 3: +0.046 ctrl VP per round, +5.4 pp win share against v2 on the fixed table; +0.028 / +2.6 pp on the
mixed 4p+6p tables, a gain at 4 players and none at 6; cost: bot time 3.4x).
