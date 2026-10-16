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

## Vocabulary migration (part 2)

Operator, 2026-10-08: grow capacity ("there is no other option").

checkpoint-41476 holds 20,054 slots in capacity 20,480 (426 free rows); `Vocabulary::append` refuses
to overflow.

- `Vocabulary::append_reallocating` (ti4-policy): the same append-only, key-ordered assignment. On
  overflow it re-allocates (`allocated_for` = new column count, capacity = `capacity_for`, the rule
  a fresh build uses); it is atomic (works on a copy) and refuses past `CAPACITY_LIMIT`. Tests:
  `reallocating_append_grows_capacity_and_moves_nothing`,
  `reallocating_past_the_limit_is_refused_and_changes_nothing`.
- `Actor::grow_input_capacity` (ti4-mlp): pads `W1`, and a separate critic's input, with zero rows,
  atomically. `Actor::copy_input_rows` copies rows in both tables. Tests:
  `capacity_growth_keeps_every_logit_and_adds_zero_rows`,
  `copying_a_fallback_row_keeps_the_logit_of_a_moved_name`.
- `migrate_bundle_append_names --grow [--init-from-oov]`.

**Finding: zero rows are not neutral.** A name the vocabulary lacks resolves to its family's
out-of-vocabulary column, and in checkpoint-41476 those columns are trained (L2: `critic-state`
0.64, `state-kind` 0.25, `option` 0.20, and smaller ones in 12 other families). Starting appended
rows at zero, the tool's previous behaviour, which its doc wrongly called score-preserving,
changes the score of every option carrying such a name, including 833 names that already occur in
six-faction games. `--init-from-oov` starts each new row as a copy of the OOV column it used to
resolve to, so scores are unchanged at migration time. The new names are still untrained: each
begins as the generic fallback and diverges only under training. That is operator option 3
without a silent behaviour change.

| Artifact | Contents |
|---|---|
| out/bf22-migrate-41476.names | 19,337 names = union of the wide and six censuses (>= 3 sightings) |
| out/bf22-checkpoint-41476-wide-oov | **the migrated bundle**: 39,391 slots, capacity 49,152, rows from OOV; provenance records the source, the growth and the init |
| out/bf22-checkpoint-41476-wide | superseded first attempt (zero rows), kept because nothing is deleted; do not train from it |

### Equivalence: source vs migrated (clearance_eval, 20 seeds x 6 rotations, 4 rounds, diplomacy, T=0.25, holdout pool)

| Roster | Source vs `-wide-oov` per-seat TSV | Table (source) |
|---|---|---|
| six | **byte-identical** (720 seats) | 93.47% clearance, 3.561 VP |
| wide | 712 of 720 seat rows identical; 8 differ | 46.53% clearance, 3.165 VP |
| six, zero-row copy (superseded) | differs (92.50%, 3.589 VP) | shows zero rows are not neutral |

The 8 wide differences are float rounding, not semantics. libtorch's fused embedding bag is not
bit-stable across mathematically equal inputs: with one row copied to two columns, the
duplicate-column form and the split form differ by up to 4.6e-5 in a logit (diagnostic test,
since removed). Near-greedy sampling flips the occasional near-tie. Wide games carry many migrated
names; six-faction games carry few, and those stayed byte-identical. The six-roster result is also
byte-identical before and after the engine fix below. Logs: out/bf22-equiv2-*.{log,tsv}.

## Engine defect found by wide-roster evaluation: endless research-waiver cycle

A near-greedy (T=0.25) wide `clearance_eval` over 20 seeds never finished: 6 of 20 seeds ran more
than 5 minutes alone, and one ran 25 minutes without reaching the step cap. `wide_stall_probe` (new example:
one game, every decision logged with a timestamp) showed seed 900000003 rotation 2 cycling
**Cabal: "research a technology" -> pick cv2 -> "research cv2: choose a prerequisite waiver" ->
decline -> "research a technology"**, about 265k decisions in 240 s. Cause: `offer_research_inner`
(the Technology primary) and the paid secondary research loop re-offered a technology whose
optional faction waiver had just been declined, and those sub-decisions sit inside one
`Game::run` step, so the runaway cap never fired. Higher temperatures (training T=1, census 2.5)
leave the cycle by chance, so it only showed at evaluation temperature.

Fix (`ti4-engine` strategy_cards.rs): within one research, a technology whose waiver was declined
is not offered again; every pass researches or shrinks the offer, so both loops terminate. Test:
`strategy_cards::tests::a_technology_whose_waiver_was_declined_is_not_offered_again` (Yin
commander waiver: re-selecting the declined technology is now `ScriptDiverged`; another technology
is researched). After the fix the 6 stalled seeds finish in 17-18 s each.

This is a Tier-C legality change and needs frontier review together with this package.

### Checks (part 2)

`cargo test --no-fail-fast -j16 -p ti4-engine -p ti4-policy -p ti4-training -p ti4-mlp --
--test-threads=16`: **3363 passed, 1 failed, 2 ignored** (out/bf22-tests2.log). The failure is the
known environmental `ti4-mlp smoke_refusals::a_pool_without_an_allowed_manifest_role_is_refused`
(out/vocabulary provenance placeholders from the 2026-09-08 reconstruction; unrelated). One
earlier attempt hit a transient LNK1104 file lock on an unrelated example binary; the rerun linked
it fine.

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

1. Tier-C frontier review of this package by a reviewer other than the author.
2. Done 2026-10-08: `ppo_update --bundle out/bf22-checkpoint-41476-wide-oov --roster wide --updates 1
   --seeds-per-update 16 --device cpu --no-checkpoint`, 16 workers: 16 games, 45,120 decisions,
   0 failed games, parameters moved (out/bf22-ppo-wide-smoke3.log).
