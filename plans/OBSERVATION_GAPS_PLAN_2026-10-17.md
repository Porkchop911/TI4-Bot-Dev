# Observation gaps: breaches and similar unseen state — plan (2026-10-17)

Branch: `claude/crimson-rebellion-breaches-fc39ff` (worktree). Status: planned, nothing implemented.

## Finding

Crimson breaches are implemented and tested in the engine (crimson 44/44, breach 16/16), but the
policy never observes them. The audit found the same pattern for other state, plus adjacency code
that ignores non-geometric links.

**Crimson is not in the training roster** (`ti4_host::preset::ROSTER`, preset.rs:111, 24 factions),
so breach features stay latent until Crimson is seated. Roster-relevant gaps come first.

## Constraints (training is live)

- The reward-terms pilot is on the GPU (`ppo_update`); CPU greedy evals follow at 30 threads.
  No GPU. Builds only in this worktree's own target dir, `cargo -j4`, focused tests only
  (`-p ti4-policy`, `-p ti4-mlp`, `-p ti4-engine <filter>`), until COMPUTE_QUEUE.md says CPU is free.
- Never touch `out/`, the main checkout, or an existing bundle. Migrated bundles are written as new files.
- Step-0 identity: no existing feature name changes value. New names only, emitted only when non-zero,
  appended to the vocabulary with zero rows (no `--init-from-oov`). Reuse admitted families
  (`target`/`route`/`origin`/`destination`, projection.rs `FAMILY_ROLES`) so the OOV registry (v11) is not bumped.

## Phase 0 — shared groundwork

1. `Observed` accessors next to `active_breach_in` (choice.rs:1010): `breach_in`, `wormhole_tokens_in`,
   `faction_mark(player, key)`, and `adjacency_for(player)`, which builds the engine `Adjacency`
   (movement.rs:~800-867, includes `hooks::linked_systems`).
2. A policy BFS `reachable_via_links(adj, from, to, move)` beside `system_reachable` (features.rs:3195).
3. ti4-mlp test: copy `capacity_growth_keeps_every_logit_and_adds_zero_rows` (lib.rs:2207) for
   appended names. Old vectors keep identical logits; new names with zero rows also keep identical logits.

## Phase 1 — breach features (requested)

In `add_system_features` (features.rs:2785, after the anomaly block at :2927), emit:

| Name | Meaning |
|---|---|
| `{prefix}:breach` | the system holds a breach token |
| `{prefix}:breach-active` | that breach is active |
| `{prefix}:link-reachable` | not hex-reachable but reachable through links (breach, wormhole token, faction links); shared with Phase 2 |

Keep the battle.rs:604 `Unsupported` marker: it is a combat-model limit, not an observation gap.
Tests use features.rs fixtures (~:6760). (a) A Crimson state with one active and one inactive breach
emits the values above. (b) With no breaches the names are absent. (c) A non-Crimson state emits a vector
byte-identical to the old one.

## Phase 2 — roster-relevant gaps (high priority)

| # | Gap | Fix |
|---|---|---|
| 2a | Wormhole tokens (gamma explore, Creuss/Ghost gates); features.rs:2920 reads only the static record | `{prefix}:wormhole-token:<kind>` |
| 2b | Per-player links (Ghost, Winnu, Empyrean, Mahact); `route:adjacent` (:2662) and `system_reachable` are hex-only | `{prefix}:link-reachable` (Phase 0 BFS) + `route:link-adjacent`; old names unchanged |
| 2c | `faction_marks` (nekro, mahact, naalu, titans, argent, nomad, yssaril, saar, mentak) | Per-faction seat facts with an allowlist of decision-relevant keys, one name per key; audit each key's meaning first |

## Phase 3 — medium priority

Exploration planet attachments; frontier tokens; ion storm and purged/supernova changes (`anomaly` is static);
laws in play and custodians removed (already in projection/critic, not in features: check whether the network
sees them through projection before adding); Cabal ingress tokens and captured units; Nekro assimilated techs.

Low priority, deferred: Thunder's Edge state, plots, expeditions, legendary exhaustion, per-turn flags.

## Phase 4 — engine adjacency audit (rules correctness, separate commits)

These call `Galaxy::adjacent`/`are_adjacent` directly and miss breach and per-player links.
Decide each against the rules text; some "adjacent" cards may be geometric on purpose.
combat.rs:2002 (SPACE CANNON), 3699/3751/3785 (retreat, Skilled Retreat, Dark Energy Tap);
action_cards.rs:3871/4558/7049; agenda_effects.rs:729; faction_abilities.rs:603; relics.rs:565;
secrets.rs:566/590; objectives.rs:683; cabal.rs:833/1475; sardakk.rs:295; mahact_units.rs:414;
empyrean_units.rs:117; movement.rs:1182. Policy: battle.rs:431, tactical_plan.rs:551.
First check whether `Galaxy` already folds in wormhole tokens (galaxy.rs:747).
Each fix gets a regression test. Engine changes alter game outcomes: never during a run that depends on these binaries.

## Phase 5 — migration and validation (after training frees the CPU)

1. Collect new names with the vocab census tool; run `ti4-mlp/examples/migrate_bundle_append_names.rs`
   on the current checkpoint (examples/Current_Bots checkpoint-3888) into a **new** bundle, `--grow` only
   if the 429 free rows run out, no `--init-from-oov`.
2. Paired greedy eval, migrated vs original bundle, on the standard seeds: it must be identical
   (step-0 identity). Then it is ready for the next PPO run; queue it in COMPUTE_QUEUE.md.
3. Breach payoff needs Crimson seated: a separate operator decision.

## Order

0 → 1 → 2a/2b → 2c → 3, with 4 in parallel (separate commits). Phase 5 after the live pilot finishes.

## Phase 0–2b implemented names

Emitted only when non-zero; families `target`/`origin`/`destination`/`route` (no registry bump).

| Pattern | Meaning |
|---|---|
| `{prefix}:breach` | system holds a breach token (prefix in target, origin, destination, ...) |
| `{prefix}:breach-active` | that breach is active |
| `{prefix}:link-reachable` | not hex-reachable (`{prefix}:reachable` = 0) but a ship reaches it within its move over the seat's adjacency (wormholes incl. tokens, breaches, Ghost/Winnu/Empyrean/Mahact links) |
| `{prefix}:wormhole-token:<kind>` | `alpha`/`beta`/`gamma`/`delta`: kind put on the system by a token (Galaxy::token_wormholes: Creuss/gamma tokens, ion storm face, opened Nexus, delta flagship); not the printed tile record |
| `route:link-adjacent` | origin and destination adjacent for the seat but not for the map's `Galaxy::are_adjacent` (breach pair, per-player link) |

Notes: `Galaxy` already folds wormhole tokens and printed wormholes into `adjacent`, so `route:adjacent` already
covers those; `route:link-adjacent` adds only what `PlayerAdjacency` has beyond that. Accessors: `Observed::breach_in`,
`wormhole_tokens_in`, `adjacency_for` (cached per seat per position, `PlayerAdjacency::into_owned`).

### Phase 2c (faction_marks) names

Read through `Observed::faction_mark` / `faction_marks_with_prefix` (refuse `private:`, `cards:staged:`, `cards:reveal:`).

| Pattern | Meaning |
|---|---|
| `actor-inventory:mahact-pool-tokens` | acting Mahact: foreign command tokens in its fleet pool / 8 |
| `actor-inventory:artuno-goods` | acting Nomad: trade goods on Artuno |
| `actor-inventory:sleepers-placed` | acting Titans: own sleeper tokens on the board (0..5) |
| `actor-inventory:nekro-z-tokens` | acting Nekro: Z tokens placed / 7 |
| `{prefix}:sleeper`, `{prefix}:sleeper-own` | system has a planet holding a sleeper; `-own` when it is the actor's |

## Phase 4 decisions (adjacency audit, 2026-10-18)

`Galaxy::adjacent` already folds printed and token wormholes. Breach links are global (Crimson text:
systems with active breaches are adjacent, for everyone). Quantum Entanglement (Creuss), Lazax Gate
Folding (Winnu, own tactical action) and Void Tether (blocks other players' border) are per player, so
each site asks `PlayerAdjacency::new(.., acting player)`. All tests are in `factions/crimson.rs`
`adjacency_audit_tests` (active-breach pair, ring fixture).

| Site | Effect | Decision | Reason | Test |
|---|---|---|---|---|
| combat.rs `reaching_guns_by` | PDS II / Xxcha mech SPACE CANNON "ships in adjacent systems" | link-aware, per gun owner (dice order kept: neighbours still sorted) | adjacency is the firing player's; a Creuss link is not an Xxcha's | `space_cannon_reaches_across_a_breach_link` |
| combat.rs `eligible_retreats` | 78.7c retreat to adjacent system | link-aware, retreating player | a retreat is a move to an adjacent system | `retreats_may_cross_a_breach_link` |
| combat.rs `skilled_retreat_destinations` | Skilled Retreat "adjacent system" | link-aware, player | same | same |
| combat.rs `det_retreat_destinations` | Dark Energy Tap | link-aware, player | same | same |
| action_cards.rs Experimental Battlestation | dock "in or adjacent" | link-aware, player | card's player | (no dedicated test; same lookup as jamming) |
| action_cards.rs Exploration Probe | frontier token "adjacent to your ships" | link-aware, player | same | (no dedicated test) |
| action_cards.rs `jamming_systems` | Signal Jamming | link-aware, player | same | `signal_jamming_reaches_a_breach_linked_system` |
| agenda_effects.rs travel_ban | PDS "adjacent to a wormhole system" destroyed | breach links only | no acting player: breach links are global, per-player links have no owner | (no dedicated test; `breach_pairs` shared with the hook) |
| faction_abilities.rs `annexable` | Xxcha Civil Accords annex | link-aware, player | "adjacent to a planet you control" | `annexing_and_the_stellar_converter_...` |
| relics.rs `stellar_converter_targets` | Stellar Converter | link-aware, player | "adjacent to your bombardment unit" | same |
| secrets.rs (2) | secret objectives: ship beside anomaly / rival home | link-aware, scorer | objective is the scorer's board state | `a_ship_beside_an_anomaly_...` (anomaly case) |
| objectives.rs `ships_adjacent_to_mecatol_count` | public objective | link-aware, scorer | same | (no dedicated test) |
| cabal.rs `carrion_targets` | Cabal hero | link-aware, Cabal | hero text, Cabal's view | (no dedicated test) |
| cabal.rs `vortex_targets` | Vortex | link-aware, Cabal | same | `a_space_dock_vortex_and_the_mahact_hero_...` |
| sardakk.rs `commit_candidates` | Sardakk commander commits from adjacent systems | link-aware, invader | invader's adjacency | (no dedicated test) |
| mahact_units.rs `hero_pairs` | Mahact hero | link-aware, Mahact | same | same Vortex/hero test |
| empyrean_units.rs `near` (Dynamo, Watcher) | "in or adjacent to" | link-aware, Empyrean | owner's adjacency | (no dedicated test) |
| movement.rs:1182 | hex-geometry test fixture | geometric | test only | n/a |
| game.rs `wormhole_hops` | counts wormhole hops for move bonus | geometric (comment added) | a breach link is not a wormhole | n/a |
