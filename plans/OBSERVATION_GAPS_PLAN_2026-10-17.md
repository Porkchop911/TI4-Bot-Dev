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
| action_cards.rs Experimental Battlestation | dock "in or adjacent" | link-aware, player | card's player | `experimental_battlestation_docks_across_a_breach_link` |
| action_cards.rs Exploration Probe | frontier token "adjacent to your ships" | link-aware, player | same | `exploration_probe_reaches_a_frontier_token_across_a_breach_link` |
| action_cards.rs `jamming_systems` | Signal Jamming | link-aware, player | same | `signal_jamming_reaches_a_breach_linked_system` |
| agenda_effects.rs travel_ban | PDS "adjacent to a wormhole system" destroyed | breach links only | no acting player: breach links are global, per-player links have no owner | `travel_ban_counts_a_system_linked_to_a_wormhole_system` |
| faction_abilities.rs `annexable` | Xxcha Civil Accords annex | link-aware, player | "adjacent to a planet you control" | `annexing_and_the_stellar_converter_...` |
| relics.rs `stellar_converter_targets` | Stellar Converter | link-aware, player | "adjacent to your bombardment unit" | same |
| secrets.rs (2) | secret objectives: ship beside anomaly / rival home | link-aware, scorer | objective is the scorer's board state | `a_ship_beside_an_anomaly_...` (anomaly case) |
| objectives.rs `ships_adjacent_to_mecatol_count` | public objective | link-aware, scorer | same | `the_mecatol_objective_counts_a_ship_linked_to_mecatol` |
| cabal.rs `carrion_targets` | Cabal hero | link-aware, Cabal | hero text, Cabal's view | `cabal_carrion_reaches_a_ship_across_a_breach_link` |
| cabal.rs `vortex_targets` | Vortex | link-aware, Cabal | same | `a_space_dock_vortex_and_the_mahact_hero_...` |
| sardakk.rs `commit_candidates` | Sardakk commander commits from adjacent systems | link-aware, invader | invader's adjacency | `the_sardakk_commander_commits_from_a_breach_linked_system` |
| mahact_units.rs `hero_pairs` | Mahact hero | link-aware, Mahact | same | same Vortex/hero test |
| empyrean_units.rs `near` (Dynamo, Watcher) | "in or adjacent to" | link-aware, Empyrean | owner's adjacency | `empyrean_dynamo_and_watcher_reach_across_a_breach_link` |
| movement.rs:1182 | hex-geometry test fixture | geometric | test only | n/a |
| game.rs `wormhole_hops` | counts wormhole hops for move bonus | geometric (comment added) | a breach link is not a wormhole | n/a |
| combat.rs `reaching_guns_by`, `eligible_retreats` (per player) | Ghost Quantum Entanglement, Winnu Lazax Gate Folding | already link-aware (above); tests added | the link belongs to its holder only | `a_ghost_link_extends_only_the_ghosts_space_cannon_and_retreat`, `a_winnu_link_extends_only_the_winnus_own_tactical_action` |
| transactions.rs `are_neighbours` | LRR 60 neighbours | link-aware, either neighbour's adjacency (checked only when the plain map says no) | "in or adjacent"; adjacency includes links. The link hooks ignore content/sources, so `adjacency_of` passes the embedded store | `transactions_reach_across_a_breach_link`, `a_ghost_link_makes_wormhole_holders_neighbours_for_no_one_else` |
| keleres.rs `near_mecatol` (IIHQ) | "in or adjacent to the Mecatol Rex system" | link-aware, IIHQ holder | card's player | `iihq_neighbours_count_a_system_linked_to_mecatol` |
| choice.rs `opponent_relationship` | policy "Neighbor" slot | geometric (reverted) | existing policy input; changing it breaks step-0 identity for Ghost/Winnu checkpoints. Links are seen via `route:link-adjacent` | — |
| argent.rs `wing_transfer_targets` + finish | Wing Transfer "adjacent to that system" | link-aware, Argent | card text | `argent_wing_transfer_reaches_a_breach_linked_system` |
| arborec.rs `near` (Bioplasmosis) | "same or adjacent systems" | link-aware, Arborec | card text | `arborec_bioplasmosis_moves_infantry_across_a_breach_link` |
| obsidian.rs `with_adjacent` (Planesplitter, Neural Parasite) | "contains or is adjacent to your units" | link-aware, Obsidian | card text | `obsidian_neural_parasite_reaches_a_breach_linked_system` |
| leaders.rs `naalucommander` unlock | "in or adjacent to Mecatol Rex" | link-aware, Naalu | unlock text | `the_naalu_commander_unlocks_on_ground_forces_linked_to_mecatol` |
| leaders.rs Xxcha agent ready-planet | "system adjacent to one you control" | link-aware, Xxcha | card text | none (needs the full agent flow) |
| objectives.rs `distinct_rival_home_reaches_count`, `implicated_systems("intimidate")` | Distant Lands, Intimidate Council | link-aware, scorer | same as the Mecatol objective | `rival_home_objectives_and_intimidate_see_a_breach_link` |
| diplomacy/candidates.rs (4 sites) | deal-relevance heuristics (touching presence, military relevance) | geometric on purpose | no card text; two actors, no single owner | n/a |
| objectives.rs `edge_systems`, map_feasibility.rs | rim / setup scoring from hex coordinates | geometric on purpose | board shape, not an adjacency effect | n/a |
| movement.rs route search internals | route expansion | geometric inside `MovementRules`, which adds its own link table | already link-aware | n/a |
