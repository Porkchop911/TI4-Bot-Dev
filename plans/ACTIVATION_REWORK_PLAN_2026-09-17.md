# Activation rework — execution plan

2026-09-17. Implements `ASTRA_ACTIVATION_REWORK_2026-09-17.md` as revised by
`ASTRA_ACTIVATION_REWORK_RESPONSE_2026-09-17.md`.

- **Restore point:** tag `restore/pre-activation-rework-2026-09-17` (commit `e392835`,
  branch `codex/diplomacy-v1`). Checkpoints on D: are untouched by this plan; every new run writes
  to a new `out/` folder.
- **User constraints:**
  - experiment arms run **at most 50 PPO updates** each;
  - all games end at round 4;
  - judge by paired greedy evals;
  - ti4-sim re-baseline v42 is part of this plan (engine fixes change authored play).

## Shape of the experiment

Three arms, one shared candidate generator, same seeds, pool, opponents and reward (VP only):

| arm | activation options see | fleet chosen by | movement/cargo answered by | landings |
|---|---|---|---|---|
| **0 baseline** | today's facts (corrected, Phase 1) | step-wise model | model | model |
| **A information** | + candidate-fleet summaries per system | step-wise model | model | model |
| **B package** | same as A | model, a second sampled decision over that system's candidates (+ manual) | plan, unless manual or invalidated | model |

Arm B's choice is hierarchical by construction: the existing activation decision samples the
system (P(system)), then a new package decision samples among that system's candidates
(P(package | system)). Each is an ordinary recorded PPO step with its own options, probability
and critic value. Systems with more candidates therefore get no extra probability mass.

## Phase 0 — defects (independent of the policy)

| id | defect | fix | test |
|---|---|---|---|
| D1 | a transported mech sustains a space-combat hit | `CombatWindow::sustainers` requires a ship; check casualty candidates for the same gap | engine test: carrier + mech in space, hit assigned, the mech is not offered |
| D2 | Skilled Retreat playable by a seat not in the combat | trace the card path first (reaction table → `playable_now` → effect); gate on the card's own semantics ("your ships" in the combat), not a blanket block on third parties | engine test: bystander with the card is not offered it; participant still is |
| D3 | reviewer frames 1742–1746 repeated at 1747–1751 | replay seed 42 rotation 0 in the reviewer headless path to frame 1751; find whether the engine re-asks or the trace duplicates | regression test for whichever it is |
| D4 | arena: a fight hitting the 50-round cap reports `winner: None`, the same as mutual destruction | explicit `Unresolved` outcome; count it in the probe and in the predictor label builder; decide the label (draw) and document it | unit test with two sides that cannot hurt each other; incidence count on the v4 label panel |

Then run ti4-sim. Re-baseline **v42** in a clean tree and record it in `plans/evidence/M08-021.md`.

## Phase 1 — corrected activation facts (baseline for all arms)

Keep separate concepts, one rules implementation each:

| fact | meaning | source |
|---|---|---|
| `activate-legal` | the engine offers this activation | the choice itself |
| `activate-hulls-reachable` (renamed from `activate-can-reach`) | some hull can move there, boosts considered individually | `tactical::movable_into` |
| `target:map-distance-reachable` (renamed from `target:reachable`) | printed-distance heuristic | `features.rs` |
| `target:token-budget-heuristic` (renamed from `target:within-token-budget`) | distance vs tactic tokens, a heuristic | `features.rs` |
| `activate-enemy-ground` | enemy **ground forces** on the planets | `projection.rs` |
| `activate-enemy-structures` (new) | enemy structures on the planets | `projection.rs` |
| `activate-enemy-space-cannon` (new) | enemy space-cannon dice that can fire into the system | combat helpers |

Renames change feature keys, so every bundle needs a migration that copies the old rows to the new
names (play is then identical except where meanings changed: enemy ground). The baseline bundle
for all arms is `checkpoint-212544-arena-v4` migrated this way (**arena-v5a**).

Gate: migration identity check. Expected: identical except decisions whose activation options
contain enemy structures. Report how many decisions differ.

## Phase 2 — candidate generator (`ti4-policy::tactical_plan`, new module in an existing crate)

**Inputs:** observation, player, active system, galaxy.

**Output:** `Vec<Package>`, each with:

- **Moves:** each move is a semantic identity (origin, unit type, damaged/galvanized, boost used),
  never an option index.
- **Cargo per carrier:** unit type, damaged, source (space or planet).
- **Reservations:** Gravity Drive (once per activation), Ionian, capacity, fleet supply (Armada
  included), and origins with own command tokens (cannot move out).
- **Strategy label and generator version.**

**Joint feasibility.** A shared ledger is built from `movable_into` per hull, then boosts are
reserved per package. A package is emitted only if every move and load fits the ledger.

**Strategies** (deduplicated by the full commitment, origins and cargo included; up to 6 per
system plus the manual option):

| strategy | description |
|---|---|
| hold | move nothing (valid for own systems: produce or reinforce) |
| light | cheapest found set whose index ratio vs the defender is ≥ 1 (or the single strongest hull if none) |
| efficient | best predicted (win − own cost lost) per resource among a bounded search |
| strong | the largest jointly feasible fleet |
| capture | efficient or strong ships plus the most ground forces they can carry, targeted at enemy/free planets |
| origin-preserving | efficient, excluding ships whose origin would be left weaker than the strongest enemy fleet that can reach it |

No hard win threshold excludes a package. Thresholds only steer search.

**Search budget.** Per system:

- subsets are enumerated by unit type count per origin;
- the index prunes them;
- the predictor confirms at most 8.

Measured budget target: ≤ 25 ms per activation decision (44 systems) on one core; the actual
number is reported.

**Strength index** (`ti4-policy::fleet_strength`):

- **Intrinsic descriptors:** firepower, durability, fighters, barrage, cannon, capacity.
- **Matchup comparison:** `ln(S_own_after_opening / S_enemy_after_opening)`, kept as its own
  number.
- **Promotion:** the formula moves from `strength_index_probe.rs` into the library, and the probe
  then uses the library.

**Package facts** (option features of the package decision; summarised per system for activation):

- `pkg-space-win`, `pkg-space-loss`, `pkg-own-cost-lost`, `pkg-enemy-cost-lost`: arena predictor
  for exactly that fleet vs the occupants and reaching guns;
- `pkg-ground-take-if-all-land`: invasion predictor with every carried ground force landing,
  labelled conditional (not a capture probability);
- `pkg-cargo-at-risk`: carried ground forces × space loss probability;
- `pkg-strength-ratio`, `pkg-hulls`, `pkg-cost`, `pkg-capacity-used`, `pkg-ground-carried`,
  `pkg-boost-used`;
- `pkg-origin-exposed`: the largest (enemy index − remaining index) over emptied origins;
- `pkg-strategy:<label>` one-hot, `pkg-manual` for the manual option;
- `pkg-unsupported`: the predictor cannot price this context.

**Activation summary facts (arm A and B):**

- best `pkg-space-win`;
- the cost of the cheapest package with space win ≥ 0.5 (0 if none);
- best `pkg-ground-take-if-all-land`;
- the minimum `pkg-origin-exposed` among packages with win ≥ 0.5;
- the number of distinct packages.

**Gates:**

- Unit tests:
  - Gravity Drive contention (two hulls needing it → no package uses it twice);
  - capacity loss;
  - an own-production activation;
  - a defended but shipless system;
  - a Fracture tile;
  - Armada supply.
- Legality panel: 2,000 recorded activation positions, stratified by those mechanics. Every
  package is executed through the engine by a scripted decider; zero unexplained failures.
- Shortlist recall on positions small enough to enumerate every subset: report the regret of the
  best shortlisted package vs the full menu (predicted win − cost), predictor calls and
  milliseconds.

## Phase 3 — arm A (information only)

`ti4_policy::battle::decision_facts` gains an activation branch: for each activation option, run
the generator and append the summary facts. The facts get new zero-initialized rows (**arena-v5a**
→ **arena-v5-info**), so play is identical at the start.

**Gates:**

- the migration identity check (identical);
- generator failures = 0 on a 20-game smoke;
- wall-time overhead reported.

## Phase 4 — arm B (package decision + plan execution)

**Package decision.**

- **What it is:** after the bot answers an activation, and only when the next engine prompt
  belongs to that tactical action, the bot builds a synthetic choice.
  - Options: that system's packages plus `manual`.
  - Head: `movement` (reused, so no schema change).
  - Features: package facts plus the same seat context the movement options carry.
- **Recording:** sampled and recorded as an ordinary step. The menu and generator version are
  stored with the step and never regenerated.

**Plan state** (per seat): activation id (round, player, system, event sequence), remaining moves
and loads, and a state that is one of:

| state | meaning |
|---|---|
| executing | answering prompts from the plan |
| complete | movement finished as planned |
| invalidated(reason) | an event made the rest impossible, e.g. a lost carrier or a card-changed capacity |
| manual | `manual` was picked: the model answers every step as today |
| generation error | the plan asked for something the engine does not offer without any intervening event; this is a bug, counted and logged |

**Executor.**

- **Scope:** answers only movement (`move|…`, `move_gd|…`, `done_moving`) and cargo (`load|…`,
  `done_loading`) prompts of the same activation, matching options by their payload
  (unit, source, damaged, galvanized, origin).
- **Recording:** those answers are **not recorded** as policy steps. Rewards and progress still
  accrue through the episode.
- **Out of scope:** any other prompt (reactions, combat, landing, payment) goes to the model as
  today.
- **Invalidation:** the executor re-checks remaining feasibility after each engine event. On
  invalidation it logs the reason and hands the rest of movement back to the model.

**PPO alignment.** `ppo_update`'s `Watching` log assumes one recorded step per engine choice with
two or more options. Change it so that each record carries its origin, and alignment is checked
against the bot's records rather than recomputed from engine choices:

- engine choice, learned;
- synthetic package choice;
- engine choice answered by the plan (not recorded).

Add a test that a package game's charges land on the right steps.

**Likelihood check.**

- Before the first optimizer step, recompute every recorded package step's probability with
  unchanged weights; the maximum difference must be ≤ 1e-6.
- Report the step count by head against the baseline arm (fewer movement/cargo rows, new package
  rows).

**Reviewer.** Show the package menu (strategy, moves, cargo, facts, probability) as its own
decision entry, and mark plan-answered steps as "planned".

## Phase 5 — pilots (≤ 50 updates each)

| arm | learner bundle | run folder |
|---|---|---|
| 0 | arena-v5a | `out/ppo-activation-arm0-<date>` |
| A | arena-v5-info | `out/ppo-activation-armA-<date>` |
| B | arena-v5-info + package decision on | `out/ppo-activation-armB-<date>` |

- **Settings:** `scripts/arena_v4_pilot.psd1` with `updates = 50`; same seed base, opponents
  (plain 212544), pool and VP-only reward.
- **Eval:** the champion, 20 seed blocks, 4 rounds, same evaluator build for all rows. Report:
  - VP and margin, by faction, with seed-block intervals;
  - cleared rate;
  - learned decisions per tactical action;
  - package invalidations and generation errors;
  - attacks below 0.5 predicted space win (descriptive only);
  - wall time per update, with equal-update and equal-time views.
- **Reading:** screening only. A promising arm is confirmed on a second seed base (also ≤ 50
  updates), not by running longer.

## Order and stop points

1. Phase 0 (D1, D2, D4; D3 diagnosis), ti4-sim v42, commit.
2. Phase 1 migration and identity check, commit.
3. Phase 2 generator, index library and tests; legality panel and recall report, commit.
4. Phase 3 and its gates, commit.
5. Phase 4 and its gates, commit.
6. Phase 5 pilots and evals; evidence in `plans/evidence/ACTIVATION_REWORK_<date>.md`.

Stop and report if: a gate fails without an obvious fix; ti4-sim shows play changes beyond the
fixed rules; the generator exceeds its budget by more than 4x; or a pilot's rollouts report
fallbacks.
