# M05-006 — Applying a move: cargo and the gravity-rift roll

## Package

| Field | Value |
|---|---|
| IDs | M05-006 — transport (LRR 95), the 41.2 destruction roll, and relocation |
| Depends | M05-003 (movement legality), M03-006 (dice and RNG), M03-001…005 (choice model) |
| Objective | Make a legal move something the engine can actually take. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Ported from `Game._load_cargo`, `Game._survives_gravity_rifts`, and the body of
`Game._move_one`.

## What this closes

M05-003 left movement as legality *knowledge* nothing could act on. A ship can now be loaded,
moved, and — if a rift takes it — lost with its passengers.

## Cargo is tracked by index, never by value

The single most important detail in the port, and the oracle flags it too. Units are plain
data, so **two infantry compare equal**. Filtering an "already taken" list by equality would
silently make the second one unloadable: a carrier with capacity 3 sitting on three infantry
would load one and report success, and the invasion that followed would arrive short with
nothing having failed.

`CargoWindow` therefore holds `loaded: Vec<usize>` into a fixed candidate list.
`identical_units_are_tracked_by_index_not_by_value` loads three identical infantry and asserts
all three go aboard.

Interchangeable pickups are still *offered* once — same reason as `distinct_units` in the choice
model: a sampling decider draws per option, so a pickup written three times would carry three
times the weight of an equally good one written once. But where a unit *stands* is part of its
identity, so the same infantry in space and on a planet are two different pickups
(`a_unit_on_a_planet_is_a_different_pickup_from_one_in_space`).

## Passengers arrive in space, not on a planet

A ground force loaded from a planet arrives in the destination's **space area**. Landing is
invasion — a separate step with its own decisions. Dropping troops straight onto a planet here
would conquer it without anyone choosing to.
(`a_passenger_from_a_planet_arrives_in_space_not_on_a_planet`)

## The rift roll, and why it lives here

41.2 rolls one die per rift **exited**; 1–3 removes the ship. The destination is never exited,
so ending your move in a rift is safe — `only_rifts_that_are_exited_roll` pins both directions.

This is in `transit` rather than `movement` because it is a consequence of moving, not a
question about whether the move may be made. But Nav Suite's "ignore the effect of anomalies"
has to be honoured in **both** places or the card half works: it would open the route and then
still sink the ship. `ignoring_anomalies_survives_every_rift` asserts not just survival but that
no die is rolled at all — a die drawn and discarded would still advance the seeded stream and
desynchronise replay.

95.1b is pinned separately: a ship lost to a rift takes its cargo down with it, so troops do not
stay standing in a system whose fleet has drowned.

## Outcomes name their passengers

`MoveOutcome` carries the `Cargo` itself, not a count. The oracle learned this the hard way and
says so: a bridge that moved the hull and left the troops behind produced "refusing land: wanted
1 Infantry in 37 but found 0", with every step reporting success. A count cannot be acted on —
a table told only that a ship was lost cannot find the piece to take off the board.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No Gravity Drive, no Letnev breakthrough, no Ceasefire. | Technology, breakthroughs and promissory notes are unimplemented. Move values come from the unit record. |
| Picking up en route (95.1) is not modelled. | The oracle does not model it either; noted here so the gap is not mistaken for an omission in the port. |
| No Cabal space-dock rifts (Dimensional Tears). | `MovementRules::gravity_rift_systems` supports them; nothing populates it. |
| `apply_move` takes `survives` as a parameter. | Keeps the roll separate from the relocation, so a caller can decide, log, or replay the roll without the state mutation being entangled with the RNG. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
210 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
400 total, 0 failed        (383 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in transit.rs or movement.rs

$ cargo fmt --all      # clean
```

18 new tests, all in `transit.rs`. `MovementRules::is_rift` was made public so the roll and the
legality rules answer the same question from one place.

## Open findings

1. **There is still no tactical action.** Activation, the movement step's option generation, and
   the invasion/production steps that follow are not written, so nothing in the driver calls
   `CargoWindow` or `apply_move` yet. The pieces exist; the sequence does not.
2. **Fleet supply and capacity limits are not enforced** after a move (`fleet.py` is unported),
   so a system can end up over its limits.
3. **Space combat does not exist**, so moving into an enemy system has no consequence.
4. **No independent review.** Waived by the project owner.
