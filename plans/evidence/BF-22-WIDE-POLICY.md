# BF-22 evidence: wide roster in the policy and trainer (part 1)

Status: **open**. Part 1 (roster plumbing, battle and decomposition gates, census) is implemented
and tested; the vocabulary migration is blocked on an operator capacity decision (below). Tier-C
frontier review is outstanding: the author (Claude Opus 5.5, BF unification session 2026-10-08)
is not an independent reviewer.

## Decision

Operator, 2026-10-08, old-checkpoint compatibility for the wider roster: **option 3, migrate**:
keep the old weights, and the new factions start from untrained weights, recorded in the
checkpoint's provenance. No silent reinterpretation.

## What each model surface does with a new faction

| Surface | Faction identity | Wide-roster behaviour | Change |
|---|---|---|---|
| MLP actor rows (`ti4_mlp::FACTION_ROSTER`, 33 pinned rows: `delta`, `b_delta`, `embedding`) | per-faction rows | already sized for every seat; no width change | none. checkpoint-41476: the 27 non-six rows are exactly zero in all three tensors, so a new faction plays the shared model until PPO trains its row |
| MLP vocabulary (named features, OOV per family) | faction-derived names (abilities, techs, units, homes, `state-kind` crosses) | unseen names fall into their family's OOV column | census tool widened; migration blocked on capacity (below) |
| Battle predictor (`ti4_policy::battle::FACTIONS`, versions 1..=7) | dice-shift table, not a one-hot | any faction outside the six is the public `Unsupported::Faction` for every supported version | doc rule + test; widening needs a new trained feature version with a version-gated lookup |
| Arena `Profile::CATALOGUE` (ti4-training battle_arena) | six transient profiles | unchanged | deferred: only meaningful with a new predictor version (BF-22b) |

## Code (this package)

- `ti4_training::rollout::{parse_roster, game_factions}`: `six` returns the caller's fixed list
  unchanged (historical order kept for rotations and per-faction settings); `wide` draws one
  distinct faction per player with `seating::seat_wide` on its own seating stream for the seed, so
  every rotation of a seed seats the same six and no map/dice/deck stream moves.
- `ppo_update --roster six|wide`: learner = drawn[rotation]; `--waste-penalties` (per faction of the
  six) is refused with `wide`, and a faction outside the six takes the scalar `--waste-penalty`;
  the roster is printed and written into the checkpoint provenance `source`.
- `clearance_eval --roster six|wide`, `vocab_census --roster six|wide --rotations N`. The census now
  runs games on bounded rayon workers (one actor copy each); per-game censuses are merged in job
  order, so counts and first subtypes equal a serial pass.
- `battle::FACTIONS` doc: the per-version support rule.
- Tests:
  - `rollout::tests::the_six_roster_returns_the_callers_list_in_its_order`
  - `rollout::tests::the_wide_roster_draws_six_distinct_factions_by_seed_and_reaches_new_ones` (120 seeds, determinism, distinctness, home systems, >= 20 new factions reached)
  - `rollout::tests::wide_tables_set_up_and_play_a_bounded_game` (12 seeds, one round, no error)
  - `battle::tests::every_faction_outside_the_six_is_refused_by_every_supported_version` (every wide alias and Keleres variant, attacker and defender, versions 1..=7)
  - `features::tests::every_selectable_seat_emits_each_decomposition_family_it_has_content_for` (33 seats x 6 families; `faction-start-tech:` empty exactly where the record has no fixed starting technology: argent, bastion, crimson, deepwrought, firmament, keleres a/m/x, obsidian, ralnel, sardakk, winnu)

## Runs (release build with the cu128 libtorch the run configs use; logs in out/)

| Run | Result | Log |
|---|---|---|
| `vocab_census --roster wide --rotations 1 --seeds 160` on checkpoint-41476 (long500 r2), 16 workers | 160 games, 0 errors, 119.6 s; 20,412 unseen admitted names, 18,504 seen >= 3 times | out/bf22-wide-census-41476.{log,names} |
| same, `--roster six` | 160 games, 0 errors, 105.0 s; 2,541 unseen, 2,121 seen >= 3 times | out/bf22-six-census-41476.{log,names} |
| wide-only names (>= 3) | 17,216, of which 16,403 `state-kind` crosses and 813 other families | diff of the two files |
| `ppo_update --roster wide --updates 1 --seeds-per-update 4 --device cpu --no-checkpoint`, 8 workers | 4 games, 18 factions, 0 failed games, parameters moved | out/bf22-ppo-wide-smoke2.log |
| `clearance_eval --roster wide --seeds 4` (holdout pool), 8 workers | 24 games, 19 factions, 0 failures | out/bf22-eval-wide-smoke.log |

Smaller worker counts (8 instead of 16) and the 4-seed PPO smoke: the machine's commit headroom was
about 6 GB because another process (`strata`, not ours) held 43.5 GB. A 16-seed smoke died on
allocation (`DefaultCPUAllocator: not enough memory`) for both rosters, so it was an
infrastructure failure, not a wide-roster defect. `cargo test -j16` failed on the same pressure
(os error 1455); tests were rerun as `cargo test --no-fail-fast -j1 -p ti4-policy -p ti4-training --lib --tests -- --test-threads=4`: **430 passed, 0 failed, 1 ignored** (out/bf22-tests.log). `cargo check -p ti4-mlp --examples`: exit 0, no warnings in the edited examples.

## Blocked: vocabulary capacity

checkpoint-41476 holds 20,054 slots in capacity 20,480 (426 free rows). Appending the wide names
into zero rows (`migrate_bundle_append_names`) does not fit, because `Vocabulary::append` refuses
to overflow. Growing capacity means re-allocating (`allocated_for` = new slot count, capacity =
`capacity_for`, about 49,152 for about 39k slots, under the 65,536 `CAPACITY_LIMIT`) and padding
trunk `W1` with zero rows: about +7.3M zero parameters. Old decisions would score identically. This
is a material size choice and needs operator approval.

## Scope ledger

| Consumer | Decision | Reason |
|---|---|---|
| ppo_update, clearance_eval, vocab_census | widened (opt-in) | active MLP line |
| `ti4-training` stage1.rs `seat_in_scope` | excluded | legacy linear-profile harness; profiles exist only for the six, so wide seats would play uniformly at random |
| bench_generation, heads examples | excluded | M00 benchmark workloads must stay fixed |
| teacher_corpus `FIXED_FACTIONS`, capture_offline_pilot | excluded (pinned) | historical teacher corpus; readiness review requires it to stay pinned |
| `ti4-policy` bot.rs `scored_game` | excluded | authored-bot test campaign; authored bots retired, BF-23 dropped |
| `ti4-review` `FACTIONS` | BF-24 | the replayer already accepts an explicit `lineup` |
| arena `Profile::CATALOGUE` | deferred (BF-22b) | needs a newly trained predictor version |

## Open

1. Operator: approve or decline the capacity growth; then build the growth migration (tool +
   tests) and write a migrated copy of checkpoint-41476 (never in place).
2. Tier-C frontier review of this package by a reviewer other than the author.
3. A full 16-worker smoke when memory allows.
