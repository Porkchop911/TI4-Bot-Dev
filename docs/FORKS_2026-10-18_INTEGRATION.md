# Integration of both forks (2026-10-18)

Branch `claude/funny-gauss-ifwjw4`, from `origin/main` f7a0786d (merge of PR #7). Two real merge
commits, so later pushes of either fork merge again:

1. `Feelx234/main` at ff584a8e (77 commits: bot v5, rules versions 12-16, faction/tech effect
   test suites, v5 golden re-recorded at version 16). Clean merge.
2. `ProbablyPlayable/dev` at 55543020 (146 commits; contains PP `main` and `mock-removal`: web2,
   the ti4-host invasion view, the Log, rules versions 12-17).

`CLAUDE.md` stays ours. `Cargo.lock` is the merged one; `cargo fetch` left it unchanged.

## Rules version renumbering

Felix keeps 12-16; PP's gates move up by five in the same order. `RULES_VERSION` = 22.

| Constant | Old PP | Now |
| --- | --- | --- |
| `RULES_COMMAND_TOKEN_POOL` / `SELF_ASSEMBLY` / `AGENDA_GRANT_REVEAL` / `TECH_EFFECTS` / `FACTION_EFFECTS` (Felix) | - | 12 / 13 / 14 / 15 / 16 |
| `RULES_STATUS_GAIN` | 12 | 17 |
| `RULES_TIMING_DOUBLE_PASS` | 13 | 18 |
| `RULES_EXPLORE_BEFORE_CONTROL` | 14 | 19 |
| `RULES_FLOATING_FACTORY_SPACE` | 15 | 20 |
| `RULES_TE_MECATOL_LANDING` | 16 | 21 |
| `RULES_TECH_PAYMENT_KEEPS_SPECIALTIES`, `RULES_STATUS_REDISTRIBUTION_REFUSED`, `RULES_REVERSE_ENGINEER_QUALIFIER` | 17 | 22 |

Every test of both sides gates on the constants (`CONST`, `CONST - 1`), so only the constants,
code comments, `docs/RULES_VERSION.md` and the regenerated protocol fixtures (`"rules_version": 22`)
changed. Saves written by a PP build at 12..17 need old `n` -> `n + 5`; nothing in a save names
the fork (the server envelope's `rules_identity` is the crate version 0.1.0 in both, the content
checksum is identical, `HostSave` has `bot_version` 5 in both). No load-time migration was built.

The ungated "second pass closes a window" fix that origin/main carried (87b1978d) is gated at 18
by PP's commit, so Felix-line games at 12..16 replay without it.

## Conflicts resolved

- `combat.rs`: PP's `CannonShot` list; Felix's Antimass Deflectors (+1 to the hit number when the
  shot's target owns `amd` and is not the gunner, v15+) per shot, after Plasma Scoring's pick.
  Test `amd_2_applies_to_each_cannon_shot_and_keeps_plasma_scorings_die`.
- `production.rs`: PP's typed hints (`FleetSupplyHints`, which already carries `unlimited` and the
  Armada bonus as `fleet_supply_bonus`, and `withheld`); Felix's capacity detail typed as
  `production::CapacityHints` under `capacity` (in the wire types). Bots and encoders read no
  production details (only option payloads).
- `technology.rs`: PP's pure `specialty_planets` (used by the v22 "payable without the kept
  specialties" filter). Psychoarchaeology (v15+) returns no planets (nothing exhausted, nothing kept
  out of the payment); AI Development Algorithm counts as a waiver there and is exhausted in
  `exhaust_specialties_for_research` only when nothing else covers the prerequisites. Tests
  `with_psychoarchaeology_a_research_specialty_is_not_kept_out_of_the_payment`,
  `ai_development_algorithm_unlocks_a_paid_research_at_the_current_version`.
- `agenda_vote.rs`: Felix's stateless bot v5 (bot version 5). PP's side of this file was rustfmt plus
  one `contains` rewrite of the shared ancestor's code, not a rewrite of its own.
- `storage.rs`: both `state_checksum` resets (`relic_log`, `fight_log`).
- Command tokens: Felix's exploration / Leadership Rider pool question and PP's status gain in the
  redistribution are separate paths. Test
  `an_exploration_token_and_the_status_gain_are_each_placed_once` (v22).
- Fixtures and `web-shared` wire types regenerated with `TI4_WRITE_WIRE=1` (the `wire-web` test set).
- Merge fallout: Felix's leader tests pass the `SourceSet` PP added to `leaders::unimplemented`;
  a `state_fact` test gets `discard_reason`; a tactical view test counts Antimass Deflectors; the
  board-delta test uses seed 3 (seed 2 no longer has a two-round space combat at version 22).

## Golden baseline

`tools/rulebot/golden.json` now pins `"rules_version": 16`, the version `baseline.json` (v5) was
recorded at. Without the pin the bench plays version 22, whose games differ by behaviour (PP's
gates), not only by the number, so it was not re-recorded. A deliberate re-record at 22 is open.

## Test results

- Game crate set (`t-web` packages, fast tier): all pass. ti4-engine 3426 passed / 2 ignored,
  ti4-host 333, ti4-server 59 + it 317 (13 ignored), ti4-wasm 9 + it 29, ti4-policy 295,
  ti4-rule-bot 177, ti4-model 93, ti4-content 144, ti4-calc 24, ti4-infer 17 + 16.
- Rule-bot golden: `bench` (v5, pinned 16) 200/200 identical to `baseline.json`; `bench-v4`
  (pinned 11) 200/200 identical to `baseline-v4.json`. No stuck game, engine error or panic.
- Training stack (libtorch 2.9.1 CPU, Linux): builds; ti4-mlp lib 114, api_boundary 3,
  critic_invariance 3 passed before the run was stopped.

## Not validated

- The rest of the training-stack tests (infer_parity onwards, ti4-tensor, ti4-training, ti4-sim,
  ti4-selfplay, ti4-review, ti4-replayer, ti4-cli): stopped on the owner's instruction.
- The slow tier (`t-web-slow`), clippy (`l-web`, `l-mlp`), wasm build.
- A version-22 rule-bot soak (stopped at 30/200 games with no problem), bench-v1..v3.
- Loading the MLP checkpoints of `models/index.json` with the merged encoder.
- web/ and web2/ typecheck, unit and e2e tests (the replayer UI parts included).
- The rider-slot port into agenda_vote (owner decision 1): not done, see the report.
