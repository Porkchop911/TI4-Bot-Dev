# M05-001/002 — The tactical action: activation and the movement step

## Package

| Field | Value |
|---|---|
| IDs | M05-001 (activation, LRR 89.1), M05-002 (movement-step option generation) |
| Depends | M05-003 (movement legality), M05-006 (applying a move) |
| Objective | Join legality to application: make a move something a player can be *offered*. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Ported from `Game._activatable`, `_activation_options`, `_activate`, `_movable` and the option
half of `_move_step`.

## Activation

89.1b bars a system holding **your own** command token — and only your own. An opponent's token
is no obstacle; activating a system they hold is how you attack it. That asymmetry is easy to
get backwards and would quietly make the game peaceful, so
`every_system_without_your_own_token_may_be_activated` asserts both directions.

`activate` spends a tactic token, places it, sets the active system, and bumps `activation_seq`
so anything scoped to one activation can tell them apart. Both refusals — no token, and
already-held — are checked before any mutation, with `identical()` asserting the state did not
move on the failure path.

A player with no tactic token is not *offered* the action at all, rather than offered one they
cannot pay for.

## The movement step

`movable` asks [`MovementRules`] rather than re-deriving legality, which is the point of the
package: the two modules meet here and nowhere else.
`an_enemy_blockade_removes_a_ship_from_the_movable_list` is the join test — a destroyer parked
on the only route makes the move vanish from the options offered, with no code in this module
knowing why.

**One option per distinguishable move, not per hull.** Three cruisers in one system are three
ways to write the same move, and the copies are not free: a sampling decider draws per option,
so a move written three times drew three times the weight of an equally good one written once —
its tie-break was counting hulls rather than weighing moves. Pinned by
`interchangeable_ships_are_one_option_not_three`, which checks three hulls are all *movable*
while producing one option.

Damage stays in the dedup key **and** in the label. A damaged and an undamaged dreadnought in
the same system are genuinely different moves — you would rather advance the fresh one — but
both read "move dreadnought from 01", so nothing choosing between them could see which was
which.

89.2b's "move nothing" is always offered, even when nothing can move.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No Gravity Drive variant of a move option. | Technology is unimplemented, so there is no second move value to offer. The oracle emits `move_gd` alongside `move`. |
| No Ceasefire check before offering movement. | Promissory notes are unimplemented. |
| No Saar space docks or Nomad flagship in `movable`. | Both are faction units that move under their own rules; faction abilities are unimplemented, so only things the corpus calls ships are offered. |
| `read_move` parses the option id rather than the driver doing it. | Keeps the id format in one place, and validates against the generated choice first so a decider cannot name a ship it was not offered. |
| No `effective_galaxy`. | Laws that purge or alter systems are unimplemented; the galaxy is passed in as built. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
225 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
415 total, 0 failed        (400 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in tactical.rs

$ cargo fmt --all      # clean
```

15 new tests, all in `tactical.rs`.

## The ring geometry caught me a second time

`an_enemy_blockade_removes_a_ship_from_the_movable_list` first picked its destination as "a
system two away", which on a one-ring map can be a tile two seats round — reachable by a route
that never touches the centre. Blocking the centre then proved nothing.

This is the same trap recorded in `M05-003_MOVEMENT_LEGALITY.md`, hit again in a different
module. The fix is the same: pick the system whose *only* shared neighbour is the centre. Worth
recording twice, because the failing version looked entirely reasonable and passed the eye test
both times.

## Open findings

1. **The tactical action still does not run end to end.** Activation and the movement step
   exist as functions; nothing in `Game` sequences them, so a driven game still cannot take one.
   Wiring them into the step driver — activation window, then a movement window per ship with
   its cargo sub-window — is the next package.
2. **Everything after movement is missing**: space cannon, space combat, invasion, production.
   The oracle's `_finish_tactical_action` calls five modules that have no counterpart here.
3. **Fleet supply and capacity are still not enforced.**
4. **No independent review.** Waived by the project owner.
