# BF-22b evidence: battle arena and predictor for the wide roster (version 8)

Status: **open** (author: Claude Opus 5.5, BF unification session, 2026-10-08). Tier-C frontier
review outstanding. Operator, 2026-10-08: "train the battle arena and diplomacy modules with the new
things"; scope chosen: **full modelling** (every faction's space-combat effects).

## What changed

### Lean arena (`ti4-training::battle_arena`)

- Reads units under the full content scope (Thunder's Edge included). The original six's 26 units
  print identically there (`the_original_units_print_the_same_in_the_full_scope`).
- `Profile::wide`: every seatable faction, the three Keleres apart, with and without upgrades.
- Space-combat effects, each with its engine semantics and a fixture (`wide_tests`):

| Effect | Rule | Engine route | Probe-checkable |
|---|---|---|---|
| Ambush (Mentak) | at combat start, the two best cruisers/destroyers roll once, unmodified | combat hook | yes |
| Raid Formation (Argent) | barrage hits beyond the opponent's fighters damage its sustaining ships, the largest first | combat hook | yes |
| Fourth Moon (Mentak flagship) | the opponent cannot sustain while it stands; the defender's casualties are taken first, so one destroyed by the attacker's hits no longer blocks the attacker | combat hook | yes |
| C'morran N'orr (Sardakk flagship) | +1 to its owner's other ships | combat hook | yes |
| Salai Sai Corian (Winnu flagship) | one die (hit on 7) per opposing non-fighter ship | combat hook | yes |
| Quetzecoatl (Argent flagship) | no space cannon against its owner's ships | combat hook | yes |
| Arvicon Rex (Mahact flagship) | +2 while the opponent's token is not in the Mahact fleet pool | direct in combat | yes (default board) |
| Bastion flagship | +1 per non-home system with a controlled planet | direct in combat | yes (0 conquests) |
| Van Hauge (Yin flagship) | when destroyed, every ship in the system is destroyed | **timing window** | no: the probe's `combat::resolve` runs without a timing resolver; covered by the engine's own `van_hauge_takes_every_ship_in_the_system_with_it` and the arena's `the_van_hauge_takes_every_ship_with_it` |
| Heaven's Eye (Firmament flagship) | repaired every round while the opponent holds a token on one of its plots | timing window | no (fixture only) |

- Declared policy (like the casualty order): optional sacrifices and payments are declined
  (Yin Devotion and Impulse Core, Exotrireme II, Empyrean repair for influence). Hacan's flagship
  spends no trade goods (unchanged from version 4).
- Refused at query time instead of guessed (`Unsupported::Context`): the Nekro flagship with Nekro
  ground forces in the system; the Crimson flagship at an active breach.
- Not modelled, as before: generic and faction technologies (Plasma Scoring, Non-Euclidean
  Shielding...), action cards, nebula and other anomalies (already refused), Nekro copying a
  flagship's text.

### Encoding version 8 (`ti4-policy::battle`)

- `UNIT_IDS_V8` (64): version 7's 21 slots unchanged, then every faction ship appended.
- Dice shift: Jol-Nar -1, Sardakk +1 (Unrelenting), others 0; every seatable faction supported.
- Four side inputs: Ambush, Raid Formation, edict (the Arvicon Rex's +2, or the Heaven's Eye's
  repair), conquests / 6.
- Version 8 carries version 7's facts (fact names unchanged) and version 7's ground network.
- A version-1..7 predictor cannot be relabelled into 8 and keeps refusing the new factions.
- Engine: `Observed::{mahact_fleet_pool_owners, non_home_systems_with_planets, firmament_puppets,
  active_breach_in}` expose the public facts the inputs need, delegating to the engine's own rules.

### Tools

- `battle_arena_probe`: wide profiles, full content scope, declared answers for Ambush, Raid
  Formation and declines, the active system set (flagship abilities read it), and the board context.
- `battle_predictor`: wide fleet space (Mentak and Argent kept apart in fleet keys), sampled board
  context, version-8 slots.
- `swap_battle_predictor` (new): a copy of an arena checkpoint with another predictor.

## Engine agreement (probe: engine vs lean, both sides vs Hacan, `--upgrades`, fleets up to 3
## ships and 4 fighters, 200 scenarios x 2,000 repetitions per faction)

| Faction | mean attacker-rate gap | \|z\| > 3 |
|---|---|---|
| sol | 0.0042 | 0 / 200 |
| letnev | 0.0051 | 0 / 200 |
| xxcha | 0.0059 | 1 / 200 |
| jolnar | 0.0058 | 0 / 200 |
| l1z1x | 0.0042 | 0 / 200 |
| mentak | 0.0056 | 0 / 200 |
| sardakk | 0.0050 | 0 / 200 |
| winnu | 0.0058 | 1 / 200 |
| yin | 0.0921 | 32 / 200 |
| argent | 0.0050 | 0 / 200 |
| mahact | 0.0048 | 0 / 200 |
| bastion | 0.0055 | 0 / 200 |
| arborec | 0.0045 | 0 / 200 |
| ghost | 0.0052 | 0 / 200 |
| muaat | 0.0055 | 0 / 200 |
| naalu | 0.0044 | 0 / 200 |
| nekro | 0.0058 | 0 / 200 |
| saar | 0.0052 | 1 / 200 |
| yssaril | 0.0055 | 0 / 200 |
| cabal | 0.0055 | 0 / 200 |
| empyrean | 0.0055 | 0 / 200 |
| naaz | 0.0047 | 0 / 200 |
| nomad | 0.0055 | 1 / 200 |
| titans | 0.0050 | 1 / 200 |
| keleresa | 0.0045 | 0 / 200 |
| keleresm | 0.0045 | 0 / 200 |
| keleresx | 0.0045 | 0 / 200 |
| crimson | 0.0055 | 0 / 200 |
| deepwrought | 0.0045 | 0 / 200 |
| firmament | 0.0055 | 0 / 200 |
| ralnel | 0.0044 | 0 / 200 |

Chance alone gives about 0.5 per 200 at \|z\| > 3. 30 of 31 agree. **Yin** is the known gap: the Van Hauge fires through a timing window, which the probe's engine path does not run, so the engine there never blasts and the lean arena (correctly, per the engine's own unit test) does. Two fixes came out of the sweep: the probe now sets the active system (flagship hooks read it; Sardakk/Mahact/Winnu disagreed without it), and the arena re-checks the Fourth Moon between the two sides' casualty assignments (Mentak disagreed, 12/200, before).

## Predictor version 8

`battle_predictor --max-ships 8 --max-fighters 16 --ground <v7 predictor>`, 16 workers, CUDA
optimiser, held-out 20,000 positions labelled from 1,024 fights each; 774,258 distinct fleets
(v4: the six only). Encoding: input 275, output 131.

| Run | Steps | Held-out KL | Attacker-win MAE | Contested | Survival MAE | Time |
|---|---|---|---|---|---|---|
| v4 (six factions, ARENA-002) | 20,000 | 0.0005 | 0.43pp | ~1.2pp | 0.7pp | ~6 min |
| v8, out/battle-predictor-v8-20261008.json | 20,000 | 0.0013 | 0.62pp | 1.79pp | 0.87pp | 9 min |
| **v8, out/battle-predictor-v8-60k-20261008.json (used)** | 60,000 | 0.0009 | 0.49pp | 1.40pp | 0.61pp | 25 min |

The exported plain-Rust forward pass matches the trained network to 7.2e-7. The ground network is
version 7's, carried unchanged. `swap_battle_predictor` wrote
**out/bf22-checkpoint-41476-wide-v8** (the migrated checkpoint with predictor v8; read back through
the loader).

## Engine defect found on the way: the Luminous loop

The wide-roster diplomacy teacher hung for hours on three seeds (1265000018, 1265000149,
1265000215). A per-game progress line and `--trace` located it at a Deepwrought seat's diplomacy
response; `cdb` showed the learner's power map in `tactical::movable_into` -> path search. The
Deepwrought flagship's +1 per own-unit system was earned on every pass, so a loop through a gravity
rift gained steps without end. Fixed in `fcb85791` (once per system, exact pruning key); the three
seeds now take about 2 s each.

## Open

- Train predictor v8, export, swap into the migrated checkpoint, evaluate.
- Ground predictor for the wide roster (phase 2): mechs and ground abilities of the new factions.
- Frontier review.
