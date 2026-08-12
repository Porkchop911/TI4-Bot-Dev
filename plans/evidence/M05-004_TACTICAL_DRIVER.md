# M05-004 — Driving the tactical action

## Package

| Field | Value |
|---|---|
| IDs | M05-004 — sequencing activation, movement and cargo in the step driver |
| Depends | M05-001/002 (activation and move options), M05-003 (legality), M05-006 (application) |
| Objective | Make a driven game able to actually take a tactical action. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Sequencing follows `Game._apply`'s `pending` dispatch and `_move_step`.

## What this closes

Every piece of a tactical action existed and nothing joined them. A driven game can now:

**activate a system → move ships one at a time → load each ship's hold → roll its rifts → finish**

`a_driven_tactical_action_activates_moves_and_completes` runs the whole sequence through the
step driver and asserts the tactic token was spent, the ship left its origin, it arrived in the
active system, and the active system was closed at the end.

## Offered only when a map exists

`Game` gained an optional `galaxy`. Without one there is no board to activate anything on, so
the action is **never offered** rather than offered and then failing to resolve.

That has a second, deliberate effect: every existing test and the 100-seed campaign build no
galaxy, so none of them see a tactical action and none of their behaviour changed. The option is
also **appended** rather than inserted, so a table that always takes the first option keeps
taking the action it took before. Adding a new action did not silently rewrite every seeded
game.

## The route is computed once

`begin_one_move` computes the path when the ship is *selected*, and carries it through loading.
Cargo cannot change which systems a ship passes, and recomputing the route after the hold was
filled would risk rolling gravity rifts for a different path than the one that was legal when
the move was offered.

## Rift rolls share the game's seed

`Game::with_seeded_random(seed)` now seeds the `GameRng` as well as the decider, so a replayed
game rolls the same rifts. A separate generator would have made the dice reproducible only by
accident.

## The action finishes rather than blocking

Space cannon, space combat, invasion and production are unimplemented — the oracle's
`_finish_tactical_action` calls five modules with no counterpart here. The action **completes**
and emits `TACTICAL_STEPS_UNRESOLVED`.

This is the same judgement made for unimplemented agenda effects, and for the same reason: the
oracle itself announces an unresolved effect rather than silently doing nothing. Blocking would
have made the action unusable; proceeding silently would have hidden that **moving into an
enemy system currently has no consequence**. The test asserts the announcement is present, not
merely that the action finished.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No Space Cannon Offense, combat, invasion or production. | Unimplemented; announced at the boundary. |
| Fleet supply and capacity are not enforced after movement. | `fleet.py` is unported, so a system can end over its limits. |
| No frontier exploration on arrival (35.5). | Exploration and Dark Energy Tap are unimplemented. |
| The route is computed at selection, not re-derived per step. | See above — it is the same route either way today, but only because no ability changes movement mid-action. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
230 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
420 total, 0 failed        (415 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in game.rs

$ cargo fmt --all      # clean
```

5 new tests in `game.rs`.

## Open findings

1. **No consequence to arriving.** Combat is the largest remaining gap in the action: ships of
   two players can now share a system indefinitely.
2. **Fleet supply and capacity are unenforced**, so movement can leave a system over its limits
   and nothing corrects it.
3. **The seeded campaign does not exercise this**, because it builds no galaxy. A campaign with
   a map would be a worthwhile follow-up and would likely find more than these tests do.
4. **No independent review.** Waived by the project owner.
