# M04-004 — Faction seating, starting fleets, and the board

## Package

| Field | Value |
|---|---|
| IDs | M04-004 (faction seating and setup), part of M04-002 (galaxy/content mapping) |
| Depends | M02-003/005/008, M04-001/002/003/006/007 |
| Objective | Seat players as factions: read the opening position out of the corpus, resolve its planet references, deploy it onto a board, and take home control. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, tree clean before and after.

| Ported from | To |
|---|---|
| `engine/factions.py` (252 lines) — `FLEET_CODES`, `FACTION_SPECIFIC`, `_initials`, `resolve_planet`, `parse_fleet`, `catalogue`, `get`, `deploy`, `home_systems` | `crates/ti4-content/src/factions.rs`, `crates/ti4-engine/src/seating.rs` |
| `engine/game.py` `seated_game`, `neutral_systems` | `crates/ti4-engine/src/seating.rs` |

Tests mirrored: `tests/test_factions.py` — 14 of its 23, the ones covering parsing,
resolution, and deployment.

## The starting fleet format

Faction records carry the opening position as `"2 cv, dd, 3 ff,5 inf j, sd j"`, which is
`[count] <code> [where]` repeated. Two details make it harder than it looks, both preserved:

* **The abbreviations are not the corpus's `asyncId` values.** Starting fleets say `cr`,
  `inf`, and `pds` where `asyncId` says `ca`, `gf`, and `pd`, and `ws` means a war sun rather
  than the `nowarsun` placeholder it maps to there. `FLEET_CODES` is deliberately a separate
  table. Pinned by `muaat_opens_with_a_war_sun`.
* **Planet references resolve by initials first, prefix second.** Xxcha writes `at` and `ar`
  for Archon Tau and Archon Ren, and `ar` also prefixes *both* planets — so a prefix-first
  resolver either fails or silently picks the wrong archon. An ambiguous prefix is an error,
  not a guess. Pinned by `initials_beat_an_ambiguous_prefix` and
  `xxcha_splits_correctly_between_its_two_archons`.

`mech` and `flagship` are faction-specific and resolve per player at deploy time through
`ti4_content::units::faction_unit`, which goes via `resolve_id` so the Naalu still get their
mech when Thunder's Edge is out of scope.

All 34 factions parse and resolve; all 17 base factions deploy onto a board
(`every_official_faction_parses_and_resolves`, `every_base_faction_deploys_onto_a_board`).

## Replaced: the previous fleet parser

`ti4-model/src/factions.rs` was deleted. Its `parse_fleet` was the file the earlier audit
called "the only real algorithm in the crate", and it was wrong in four ways:

| Defect | Consequence |
|---|---|
| No initials resolution | Xxcha's `at` and `ar` both fall through to prefix matching; `ar` matches both planets. |
| Unknown unit code returned `None` and the entry was **skipped** | A faction quietly starts a ship short, with no error. |
| The planet token was stored verbatim as a `String`, never resolved to a planet id | `5 inf j` deploys onto a planet called `"j"`, which does not exist. |
| A token equal to `"space"` was special-cased | Not part of the format. Absence of a token means space; the literal never appears. |

Its two tests passed because they only asserted counts and that a planet token was carried
through as-is.

## Two bugs found while wiring it up

Both surfaced as test failures with Mecatol landing at hex `(-1, 1)` instead of the origin.

1. **`neutral_systems` returned Mecatol as filler.** Mecatol has planets, is not an anomaly,
   has no wormhole, and is nobody's homeworld, so it passed every filter. The oracle
   excludes `"18"` by name; that exclusion is now ported and pinned.
2. **`Galaxy::build` silently accepted a duplicate system id.** It writes both a
   `placement` (hex → id) and a `coords` (id → hex) map; a repeated id left the two
   disagreeing, putting one tile in two places and shifting every later tile one step round
   the spiral. Now `GalaxyError::DuplicateSystem`, pinned by `placing_a_system_twice_fails_loudly`.

The second is the more serious: it would have produced a silently wrong board rather than a
failure, and only showed up because the first bug happened to trigger it.

## Differences from the oracle

| Difference | Reason |
|---|---|
| `neutral_systems` returns corpus order, not a seeded shuffle. | The oracle shuffles because its map is one of its variables. Here a deterministic filler ring keeps board-dependent tests stable; seeded selection belongs with the simulation harness, which is what needs a representative map. |
| `deploy` resolves everything against the corpus *before* touching the state. | A faction that fails to deploy leaves no half-seated player behind. `seating_an_unknown_faction_fails_rather_than_seating_nothing` asserts the board stays empty. |
| `Faction` is a borrowed view over a `Record`, not an owned struct. | Same reason as `UnitType` and `System`: the corpus is immutable and already in memory. |
| Commodities are not set at seating. | LRR 21: the record's `commodities` is the capacity a player refreshes to, not an opening balance. The oracle sets `trade_goods=0` for the same reason. |

## Not done here

* **Leaders, promissory notes, and reaction slots.** The oracle's `deploy` ends by calling
  `leaders.deploy`, and `seated_game` then calls `reactions.arm`, `leaders.arm`,
  `technology.arm`, and `promissory.deal`. None of those subsystems exist yet.
* **Deck construction.** `start_game` in the oracle also builds the objective, exploration,
  relic, agenda, action-card, and secret decks from a seed, reveals two stage I objectives,
  and deals one secret each. That needs a seeded RNG (M03-006), which does not exist.
* **`empty_systems` and `anomaly_systems`.** The oracle documents why both matter — without
  a planetless system the Thunder's Edge expedition can complete and have nowhere legal to
  place its tile, and without anomalies two public objectives are unsatisfiable by anyone.
  Neither is ported, because neither subsystem exists yet to be starved.
* **Real map setup.** `build_board` is a spiral, not a draft. `map_templates` (34 records,
  174 KB) is still unread.

## Commands and results

```
$ cargo test --workspace
121 passed  (ti4-content)
 68 passed  (ti4-model)
 42 passed  (ti4-engine)
  1 passed  (doc-test)
232 total, 0 failed        (197 before this package)

$ cargo clippy --workspace
0 warnings in factions.rs or seating.rs

$ rustfmt --edition 2024 <the six changed files>
clean
```

`ti4-model` drops from 70 to 68 tests: the two that covered the deleted fleet parser went
with it, replaced by 18 in `ti4-content::factions`.

## Open findings

1. **No independent review.** Waived by the project owner.
2. **No differential fixture evidence.** Still blocked on the missing oracle exporter.
3. **A seated game cannot yet take a turn.** There is a board, factions on it, and a phase
   machine — but no choice model, so nothing can decide anything. That is the next package.
