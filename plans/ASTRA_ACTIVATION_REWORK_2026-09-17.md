# Proposal for review — tactical actions decided as a whole, and a fleet strength index

Astra, 2026-09-17. We would like your evaluation of a rework of how the policy takes a tactical
action. The short version: the model picks the system before it can know what the fight looks like,
and then assembles the fleet one ship and one cargo load at a time, by which point the battle odds
we now compute are too late to matter. We propose that the activation choose a system **and** a
fleet together, with the battle odds for that exact fleet on the option, and a fleet strength index
as the shorthand for generating and comparing fleets.

## Where the code is

- Worktree `C:/Users/Niko/Documents/ChatGPT/ti4-engine-rs/diplomacy-worktree`, branch
  `codex/diplomacy-v1` (committed; the D: checkout is fast-forwarded to it).
- Battle arena and predictor: `crates/ti4-training/src/battle_arena.rs`,
  `crates/ti4-policy/src/battle.rs`, evidence in `plans/evidence/ARENA_002_PREDICTOR_2026-09-17.md`.
- Activation features: `crates/ti4-policy/src/projection.rs` (`activate-*`, around line 440) and
  `crates/ti4-policy/src/features.rs` (`target:*`, around line 2900).
- Strength index probe: `crates/ti4-training/examples/strength_index_probe.rs`.
- Build env: `LIBTORCH=D:/Projects/ti4-engine-rs/out/libtorch-2.9.1-cu128`,
  `LIBTORCH_BYPASS_VERSION_CHECK=1`, that `lib/` on `PATH`.

## Evidence: one tactical action, end to end

Source: `D:/Projects/ti4-engine-rs/out/action88game.ti4review.json.json`, a reviewer game with
`checkpoint-212544-arena-v4` (the arena-v4 migration of 212544, battle facts on), seed 42,
rotation 0, held-out pool, temperature 0.01. Reviewer action 87, frames 1740–1767: seat5 (Letnev),
round 4. A browsable version of the same trace: https://claude.ai/artifact/VwoZ7BVMw6yYhwJyh1fXqX

**Position.** Letnev had a dreadnought, a carrier, 3 infantry and a fighter in system 46; a carrier
and 2 destroyers in system 10, with 6 infantry and 2 mechs on Arc Prime; 2 destroyers in 109.
System 69 held one Hacan carrier II; its two planets (Accoen, Jeol Ir) were Jol-Nar's, defended only
by a space dock.

**The decisions.** 32 Letnev decisions in the action:

| step | decisions | battle or invasion odds on the options |
|---|---|---|
| choose tactical action | 1 | no |
| **activate a system** | **1 (44 options)** | **no** |
| move a ship / finish | 5 | yes (fleet committed so far) |
| load cargo | 13 | no |
| combat (card, payment, retreat, sustain) | 7 | retreat only |
| commit ground forces | 4 | yes |

**The activation.** Only 46 of the 394 features differ between the 44 options; the rest is table
state shared by all of them. The top of the ranking:

| rank | system | occupants | score | reach (hulls) | reach (map) | enemy ships | enemy ground (activate) | enemy ground (target) | res | inf |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 ✓ | 69 | Hacan carrier II; Jol-Nar space dock | 1234.6 | 1 | 1 | 1 | 1 | 0 | 4 | 6 |
| 2 | 64 | no units; 1 uncontrolled planet | 1223.7 | 1 | 1 | 0 | 0 | 0 | 3 | 1 |
| 3 | 72 | Hacan cruiser; Hacan planets Lisis, Velnor, undefended | 1217.8 | 1 | **0** | 1 | 0 | 0 | 4 | 3 |
| 4 | fracture2 | empty | 1208.7 | 1 | **0** | 0 | 0 | 0 | 0 | 0 |

What the option says about the fight is "1 enemy ship, 8 of my hulls can reach". Nothing says
whether those 8 hulls win, what they lose, or whether the planets can be taken.

**The fleet, assembled after the fact.** The first battle odds appear at frame 1742, after the
command token is down:

| frame | option (score) | win | loss | note |
|---|---|---|---|---|
| 1742 | ✓ move carrier from 46 (−468.4) | 0.57 | 0.41 | picked |
| 1742 | move dreadnought from 46 (−469.6) | 0.93 | 0.05 | 1.2 points behind |
| 1752 | finish moving (−519.8) | 0.84 | 0.15 | with carrier + cargo |
| 1752 | ✓ move dreadnought from 46 (−502.8) | 1.00 | 0.00 | picked |
| 1754 | ✓ move carrier from 10 (−510.8) | 1.00 | 0.00 | brought for capacity (2 mechs) |

The loads in between (frames 1743–1758) carry no odds at all. The landing commits do (take chance
0.95–1.00), but by then the only question left is which planet gets which unit.

Here the choice was good: the Hacan carrier died, both planets were taken, and Letnev lost nothing.
The point is that the model had no way to know that when it chose 69, and would have had no way to
know the opposite either.

**Defects found in the same action** (listed so they are not mistaken for policy behaviour):

1. Frames 1747–1751 repeat 1742–1746 exactly: same options, same scores, same picks. System 46 had
   one carrier. Not yet diagnosed: an engine re-ask or a duplicated trace.
2. The two reach features disagree: `action-plan:activate-can-reach` (a reachable hull exists)
   and `target:reachable` (map reachability) differ on systems 72, 06 and the Fracture tiles.
   System 06 is L1Z1X's home (6 ships, 15 ground forces): reachable on the map, no hull can get there.
3. `activate-enemy-ground` counts structures (69: the space dock → 1);
   `target:enemy-ground-total` counts ground forces only (69 → 0).
4. The Fracture tiles rank 4th with zero value: "can reach" is 1 while "within token budget" is 0.
5. Engine: a Letnev mech being carried cancelled a space-combat hit with sustain damage (frame
   1761; `combat.rs` `sustainers` does not require a ship).
6. Engine: L1Z1X played Skilled Retreat at the start of that combat with no ships in the system;
   the combat-round card windows have no participant check (`reactions.rs:255`).

## Proposal

### A. Decide the tactical action as a whole

The activation's options become **system + fleet package** pairs. The bot generates the packages,
scores them with the model, and then answers the engine's own movement, cargo and commit prompts
from the chosen plan. The engine does not change.

**Packages per reachable system** (at most 3, deduplicated):

| package | contents |
|---|---|
| everything | every ship that can reach, with full cargo |
| smallest decisive | the cheapest reachable fleet whose predicted win is ≥ 0.9, found with the strength index and confirmed by the predictor |
| invasion | ships as in "smallest decisive", plus as many ground forces as they can carry, sized for the planets' defenders |

A system without enemy ships gets one expansion package: the cheapest fleet that carries enough
ground forces for its free planets.

**Facts on each package option:**

- arena predictor for exactly that fleet against the occupants, space cannon included: win, loss,
  own cost lost, enemy cost lost;
- invasion take chance and cost for the carried ground forces;
- strength index of the package and of the defender, and their log ratio;
- what the move leaves behind: the strongest enemy fleet index that can reach each system the
  package empties;
- the existing gains (resources, influence, objective progress) and one reach definition (below).

**Who decides what afterwards:**

| step | today | proposed |
|---|---|---|
| system | model | model, as part of the package |
| which ships, which cargo | model, one prompt each | plan |
| retreat, sustain, casualties | model | model (unchanged) |
| which planet gets which ground force | model | plan by default |
| anything the plan cannot answer | — | model, as today |

A tactical action becomes 1 learned decision plus the combat ones, instead of roughly 20. Credit for
the outcome lands on the decision that caused it.

### B. Fleet strength index

One number per fleet, used to shortlist and compare fleets cheaply; the predictor still supplies
the odds that go on options.

**Definition** (Lanchester square law, after opening fire):

- firepower `F` = expected hits per round = Σ dice × P(hit), with faction modifiers (Jol-Nar −1)
  and the Jol-Nar flagship's extra hits;
- durability `D` = Σ (1 + sustain), with +1 for the Letnev flagship's repair;
- opening fire: the enemy's expected space-cannon hits, and its barrage hits up to our fighters,
  come off `D` first, and `F` falls in the same proportion;
- index `S = F × D`; win odds ≈ `sigmoid(k · ln(S_a / S_d) + c)`.

**Calibration** (`cargo run --release -p ti4-training --example strength_index_probe -- --pairs 20000 --fights 2000`):
20,000 random fleet pairs from the arena catalogue (six factions ± upgrades, up to 8 ships and 16
fighters, half of them drawn near each other in size), 2,000 simulated fights each; fitted on half,
scored on the other half.

| index | MAE vs simulated win rate | MAE on contested pairs (win 0.2–0.8, n 1,827) | favourite called right (n 9,424) |
|---|---|---|---|
| fleet cost | 20.9pp | 22.4pp | 83.4% |
| firepower × durability | 7.4pp | 16.4pp | 95.6% |
| … after opening fire | 5.7pp | 14.9pp | 97.1% |
| … also scaled by fleet size | 5.6pp | 14.8pp | 97.1% |
| arena predictor v4, for reference (its own held-out set, not these pairs) | 0.43pp | 1.26pp | — |

Reading: the index is a good **shorthand**: it names the favourite 97% of the time and is far
better than cost. It is **not** good enough to price a close fight: on contested pairs it is barely
better than saying 50%. Hence the split: the index shortlists, the predictor prices.

**Uses:**

1. Package generation: search fleets by index ratio, then confirm with the predictor.
2. Threat features: for each own system, the strongest enemy fleet index that can reach it next
   turn, and the ratio to what is there.
3. Enemy summaries anywhere a fleet appears (activation targets, diplomacy, agenda), as one number
   instead of unit counts.
4. Composition and production: the unit that raises the index most per resource spent, as a
   production feature.

**Possible refinement:** fit per-unit weights for `F` and `D` against the arena instead of reading
them off the cards. That could close part of the contested gap; it is not required for the uses above.

### C. Clean-up that should happen either way

- One reach definition: a hull can get there with movement, tokens and Fracture access considered.
  Drop the disagreeing duplicate.
- Count enemy ground forces and enemy structures separately.
- Fix the two engine bugs above (ti4-sim re-baseline v42 needed) and diagnose the repeated frames.

## What changes for training

- **Action space.** PPO must record the package options as the activation's option rows, so the
  log-probability is of the package. The trainer currently records the engine's system options; this
  is the first thing to verify.
- **Checkpoints.** New fact rows start at zero, but package choice changes play, so a migrated
  checkpoint will not play identically. This needs a fresh pilot, not an identity check.
- **Opponents.** Frozen opponents on the old bundle keep playing step by step; both kinds can share
  a table.
- **Cost.** One predictor call per package (≤ 3 per reachable system, 44 systems here) plus the
  index search. The v4 predictor adds 1–3% wall time today; packages should stay within ~10%.

## Proposed evaluation

1. **Legality:** every generated package is executable by the engine, and the plan completes
   without falling back to the model, on 10,000 recorded activation positions.
2. **Index:** the calibration table above reproduces with the probe command.
3. **Offline check:** on recorded games, the share of activations whose chosen system's best
   package has predicted win < 0.5 (today we cannot measure this at decision time; after the change
   it should be low).
4. **Play:** paired greedy evaluation against the champion
   (`out/blank-shaped-4layers/fracture-1x-styx16-20260915/checkpoint-19280-diplomacy-v11`),
   20 seed blocks, 4 rounds, VP-only reward, against the arena-v4 pilot
   (`out/ppo-arena-v4-pilot-20260917/checkpoints/checkpoint-10696`: VP 3.42, margin −1.34,
   cleared 80.6%) and its control (VP 2.77, margin −1.94).
5. **Decision count:** learned decisions per tactical action should drop from about 20 to about 3.
6. **Wall time:** per-update time within 10% of the arena-v4 pilot.

## Questions for you

1. Is plan-level activation the right fix, or would you keep step-wise movement and add
   "odds so far" facts to each step instead? (Our view: the latter still commits before it sees the
   fight.)
2. Are three packages per system enough, or should the model be able to fall back to building a
   fleet step by step when no package fits?
3. Should landings stay with the model?
4. Is the index worth refining (fitted weights), given the predictor already prices fights?
5. Which of the evaluation gates above would you require before a pilot?
