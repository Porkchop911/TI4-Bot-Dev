# ARENA-002: lean simulator, combat predictor, battle facts in live play — 2026-09-17

Follows `ARENA_001_PROBE_2026-09-16.md`. The engine-driven probe was ~550 fights/s per core because
every fight built a whole game; a two-fleet simulator removes that, and a predictor trained straight
from it now feeds movement decisions.

## Deviation from the design, by user decision

`BATTLE_ARENA_DESIGN_2026-09-16.md` §6 says "never substitute an independent implementation of dice or
sustain rules". The user overrode it: the arena should model the two fleets and nothing else, like
ti4calc. The substitute is justified only by the two checks below and must keep passing them.

## Lean simulator — `ti4-training::battle_arena`

- Two fleets only. Anti-fighter barrage, **both sides'** space cannon before combat (user rules
  ruling; the engine lets only non-active players fire — an engine bug, queued), simultaneous rolls,
  sustain-first, casualties by base type (fighter, destroyer, cruiser, carrier, dreadnought,
  flagship, war sun — ti4calc's order), Jol-Nar Fragile, starting damage.
- Flagship effects the engine does not yet implement: Jol-Nar (9/10 = +2 hits), Letnev (repair each
  round), L1Z1X (flagship and dreadnought hits forced onto non-fighters). Xxcha flagship space
  cannon. Hacan flagship at **0 trade goods** (user: the ability is garbage; it fights on its plain
  2 dice at 7).
- Six factions in scope, with and without upgrades; war suns and flagships; supply limits
  (8 destroyers, 8 cruisers, 4 carriers, 6 dreadnoughts, 2 war suns, 1 flagship).
- ~7M fights/s on 32 cores for small fleets, ~21M fights/s in large batches.

Checks:

| check | result |
|---|---|
| vs engine `combat::resolve`, effects off, 6 factions ± upgrades with flagships, 400 × 2000 | mean gap 0.36pp, 0–1 of 400 at \|z\| > 3 (1.1 expected) |
| vs ti4calc (`tools/ti4calc`, git-ignored clone), 300 sampled positions, 10k rolls, sustain-first, attacker cannon on | mean gap 0.2pp, 0 at \|z\| > 3, every flagship group at noise |

Two mismatches found and resolved on the way: ti4calc's default `riskDirectHit: false` sustains
only when the cheapest ship can (policy difference, aligned for the check), and attacker space
cannon (rules question, decided for ti4calc).

Casualty-order bug fixed: ranking by exact unit id sent upgraded ships (`fighter2`, …) last.

## Fleet space

Up to 8 non-fighter ships and 16 fighters within capacity and supply: 1,617 hull combinations and
18,882 fleets per generic profile; 192,976 distinct fleets across the six factions ± upgrades once
identical resolved fleets are merged. Exhaustive enumeration is out of reach (≈4·10⁹ ordered pairs
per flagship variant), so training samples positions instead of storing a corpus.

## Predictor — `crates/ti4-mlp/examples/battle_predictor.rs`

Trains straight from the simulator; nothing is stored. Each step samples 4,096 positions, labels each
from 32 fights (win / loss / mutual), and takes one Adam step on the GPU. Held-out: 20,000 positions,
one in ten by a role-independent family hash, labelled from 8,192 fights.

86 → 512 → 512 → 3, cosine LR 1e-3 → 2e-5, 20,000 steps, ~3.5 minutes:

| | value |
|---|---|
| held-out KL | 0.0005 |
| attacker-win MAE, all | 0.43pp |
| contested (0.1–0.9, 29%) | 1.15pp (label noise ≈ 0.45pp) |
| foregone | 0.14pp |
| constant-rate baseline MAE | 41.0pp |

`--export` writes `ti4_policy::battle::BattlePredictor` JSON; the plain-Rust forward pass matches the
trained network to 2.4e-7.

## Live play — `ti4_policy::battle`, bundle schema 11/12

- One encoding (`FEATURE_VERSION` 1, 21 unit ids, input width 86) for trainer and game.
- `movement_query`: for each movement option, the fight at the destination if movement ended now —
  own ships there plus the moved ship (or none extra for "finish movement"), the single opponent's
  ships. Public information only. Not applicable when no enemy ships are there. Unsupported (flag
  only) for another faction, several opponents, anomalies, planet guns, galvanized ships.
- Facts, under the existing `action-plan` family: `battle-fight`, `battle-unsupported`,
  `battle-win`, `battle-loss`, `battle-mutual`, `battle-win-change` (against finishing now).
  No registry bump: they are appended into preallocated, zero vocabulary rows.
- The predictor rides on `Actor`, so every load, copy and save keeps it. Bundle schema 11/12
  (projection ABI 4) carries `battle_predictor.json`; its presence must match the schema. The GPU
  batched path refuses arena bundles.
- `migrate_bundle_to_arena` wrote `checkpoint-212544-arena-v1` (schema 12, slots 14,878 → 14,884).
  `arena_migration_check`: 4 seeds × 4 rounds, 11,709 decisions, **identical** play; 524 battle facts
  over 464 movement decisions; wall time ×1.014.

## Not done

Engine fixes (attacker space cannon; Jol-Nar, Letnev, L1Z1X flagship abilities). Ground combat,
retreat, action cards. Expected-survivor outputs. Fights with several opponents or planet guns.
The PPO pilot (`scripts/arena_pilot.psd1`) is the next step.
