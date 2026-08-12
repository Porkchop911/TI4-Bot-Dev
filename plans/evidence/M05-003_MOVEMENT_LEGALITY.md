# M05-003 — Movement legality

## Package

| Field | Value |
|---|---|
| IDs | M05-003 — reachability, anomalies, blockades, and the gravity-rift budget |
| Depends | M04-001/002 (galaxy and adjacency) |
| Objective | Make `ti4-content::galaxy` load-bearing: decide which ships may reach an active system. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

`engine/movement.py` (204 lines) ported in full to `crates/ti4-engine/src/movement.rs`.

## What this closes

`Galaxy` has had adjacency since M04-001 and **nothing used it**. It is now the basis of a real
rule, which was the standing finding "`Galaxy` is not wired into the engine".

## Rules, each with a test

Quoted from the Living Rules Reference rather than recalled, as the oracle does:

| Rule | Behaviour |
|---|---|
| 58.4a | A ship must end its movement in the active system |
| 58.4b | It cannot move *through* a system containing another player's ships — but arriving there is the whole point of the action |
| 58.4c/d | Its own command token pins it where it stands, yet may be moved *through* freely |
| 58.4e | It may leave the active system and return, given move value |
| 58.4f | The number of systems *entered* cannot exceed its move value |
| 11.1 / 86.1 | Asteroid fields and supernovae are impassable |
| 59.1 / 59.1a | A nebula may be entered only as the active system, and never crossed |
| 59.2 | Starting in a nebula caps the move value at 1 |
| 41.1 / 41.3 | A gravity rift adds 1, and may do so several times in one movement |

Two details carry the oracle's reasoning because getting them wrong fails silently:

* **The rift bonus lands before the budget is judged.** It is what pays for the departure, so a
  ship arriving at a rift with nothing left can still leave it. Ordering it the other way
  strands ships with no error.
  (`the_rift_bonus_lands_before_the_budget_is_judged`)
* **Ignoring anomalies gives up the rift bonus too.** A rift's +1 is as much an effect of an
  anomaly as a supernova's bar, so Nav Suite drops both. Keeping the bonus while dropping the
  bars would be a strictly better card than the one printed.
  (`ignoring_anomalies_gives_up_the_rift_bonus_too`)

Reachability is a breadth-first **search**, not a distance comparison, because rifts make the
budget path-dependent: a route through two rifts is worth two extra movement.

## `Board` reads ships, not units

58.4b speaks of ships. `Board::for_player` consults the unit catalogue's `is_ship`, so a lone
infantry sitting on a planet is not a blockade — counting it would close routes the rules leave
open. Pinned by `enemy_ships_are_read_from_ships_not_ground_forces`, which adds an infantry
(no blockade), then a destroyer (blockade).

## The fixture took three attempts, and that is the finding

The map is built by spiralling tiles outward, so a test cannot simply lay systems in a line. My
first two fixtures asserted things that were true for the wrong reasons:

1. **A ring is a route.** Two opposite outer systems are two apart *through the centre* but
   three apart *around the ring*. Blocking the centre therefore only bites at a move value of 2;
   the original tests used 5 and the detour satisfied them.
2. **"Two apart" does not mean "opposite".** Ring positions two seats round are also two apart,
   and their route avoids the centre entirely. `Hub::across` now finds the system whose *only*
   shared neighbour is the centre, which is what makes the centre a genuine bottleneck.

Both would have produced a test suite that passed while testing almost nothing about blockades.
The fixture is now derived from the galaxy's real adjacency and says so in a comment, rather
than naming tiles whose geometry a reader would have to take on trust.

A third expectation was wrong in a more interesting way: `origins_within_range(1)` does not
include the active system. 58.4e is "leave *and return*", which enters two systems — so a ship
already in the active system needs a move value of 2 to qualify, not 1. The test now asserts
both halves.

## Differences from the oracle

| Difference | Reason |
|---|---|
| Ability modifiers are public fields with defaults, ported in full. | Nothing sets most of them yet — Nav Suite, Antimass Deflectors, Light/Wave Deflector and the rest are unimplemented. They are what the rules *are*, and a caller gaining one later should find the rule already written rather than have to reopen this search. |
| The system index is resolved once in `MovementRules::new`. | Looking a system up per search step rebuilt an index over the whole corpus, which is exactly how the objective predicates went quadratic in M04-017. |
| `#[allow(clippy::struct_excessive_bools)]`. | One field per printed ability, one-to-one with the oracle. Collapsing them into a bitset would lose the documented reason each exists separately — notably that Antimass Deflectors must **not** imply Nav Suite. |
| A system absent from the corpus cannot be entered. | The oracle would raise on the lookup; refusing is the safer reading, since an unknown tile is not a licence to move anywhere. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
193 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
383 total, 0 failed        (364 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in movement.rs

$ cargo fmt --all      # clean
```

19 new tests, all in `movement.rs`.

## Open findings

1. **Nothing calls this yet.** The tactical action does not exist, so movement is legal-move
   *knowledge* the engine cannot act on. Applying a move — transport, capacity, and actually
   relocating units — is the next package.
2. **The 41.2 gravity-rift destruction roll is not here**, deliberately: it is a consequence of
   moving, not a legality question, and belongs to the tactical action.
3. **No ability sets any modifier.** Every flag defaults off, so no card yet changes movement.
4. **Wormhole edges come from the galaxy**; `token_wormhole_systems` (Creuss tokens) is
   supported but never populated.
5. **No independent review.** Waived by the project owner.
