# M06-001 — Space combat

## Package

| Field | Value |
|---|---|
| IDs | M06-001 — the combat round loop, dice, Sustain Damage, and casualties |
| Depends | M03-006 (dice and RNG), M03-001…005 (choice model) |
| Objective | Give arriving in an enemy system a consequence. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Ported from `engine/combat.py`: `combatants`, `ships_of`, `_roll_combat`, `absorb_hits`,
`_offer_sustain`, `_choose_casualty`, and the round loop of `resolve`.

## Rules, each with a test

| Rule | Behaviour |
|---|---|
| 78.3 / 78.3a | Anti-fighter barrage: simultaneous, first round only, hits fall **only** on fighters, and it can end the fight before any combat die is rolled |
| 78.1 | Fewer than two players with **ships** is not a combat — ground forces do not fight in space |
| 78.5b/c | Dice are rolled grouped by combat value, ascending |
| 78.5f | The attacker rolls everything first |
| 78.6 | Hits are **simultaneous** — both sides absorb before either is checked |
| 87.1 | Each undamaged sustaining unit may cancel one hit, and it is always optional |
| 15.2a | Excess hits beyond the units available have no effect |

A printed combat value of zero means "does not fight", not "hits on 0" — which is why
`hits_on` returns an `Option` rather than a number with a sentinel.

**Simultaneity is the one worth stating.** Resolving sequentially would let the attacker's
casualties reduce the return fire they had already earned. `hits_are_simultaneous` puts a lone
fighter on each side, has both hit, and asserts both die.

## A divergence my own test caught

The first version rolled **one die batch per unit**. The oracle groups by combat value first, so
three destroyers are *one roll of three dice*, not three rolls of one.

This is not cosmetic: the number of draws from the seeded stream is part of what a seed
reproduces, so rolling them apart would silently renumber every later draw and make a
seed-pinned game diverge for reasons unrelated to what changed.
`every_fighting_ship_rolls` asserts one roll containing three faces, and it failed on the first
implementation. A `BTreeMap` supplies the ascending order 78.5b asks for as a side effect.

## Duplicate options, twice more

The same problem the choice model documents, in two new places:

* **Casualties.** Five fighters are one decision, not five. Offering per hull mattered — a
  sampling decider draws per option, so with five fighters and one dreadnought it destroyed a
  fighter five times in six whatever it thought of the trade. The count decided, not the
  scoring. Damage stays in the key and the label: losing an already-damaged dreadnought is a
  different proposition from losing a fresh one.
* **Sustain.** One option per unit *type*, since everything offered is undamaged by definition,
  so two of a type are the same decision written twice — and a bot would have sustained on
  whichever type it happened to own more of.

## Differences from the oracle

| Difference | Reason |
|---|---|
| Choices are asked inline through a `Table`, not exposed as a resumable window. | Matches the oracle's own shape. It means the step driver does not run combat yet — the position movement was in before its driver landed. |
| No retreats (78.4). | Its own body of rules; not stubbed or approximated. |
| Space cannon offense fires only from the system itself. | PDS II firing into an adjacent system, Linkship copying, and Thunder's Edge ability suppression are all unimplemented. |
| No Argent strike-wing infantry kills from a barrage. | Faction units are unimplemented. |
| No rerolls, faction abilities, laws, or Morale Boost in `hits_on`. | `effective_hits_on` in the oracle combines three modifier sources, none of which exist here. The printed value is used unmodified. |
| Sustain cancels exactly one hit. | Non-Euclidean Shielding cancels two; technology is unimplemented. |
| `MAX_ROUNDS` returns an error rather than breaking the loop. | A fight that cannot end is an engine bug, and should say so rather than quietly stopping. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
255 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
445 total, 0 failed        (425 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in combat.rs

$ cargo fmt --all      # clean
```

20 tests in `combat.rs`, including `the_same_seed_fights_the_same_battle`.

Anti-fighter barrage and space cannon offense landed alongside. Barrage is wired into round one
of `resolve`; **space cannon is not called by anything yet**, because it belongs to the tactical
action's post-movement sequence rather than to combat itself.

`a_barrage_kills_only_fighters` sweeps 40 seeds rather than pinning one: it asserts the cruiser
survives every time, and that fighters die on at least one seed — a barrage that never hits
anything would not be testing the hit path at all.

`the_active_players_own_guns_do_not_fire_at_them` is the other half of space cannon: guns belong
to everyone *except* the active player, and rolling the active player's own PDS would have them
shooting at themselves.

## Open findings

1. **Nothing calls this.** `finish_tactical` still emits `TACTICAL_STEPS_UNRESOLVED` without
   fighting. Wiring combat into the tactical action needs the choice windows made resumable, so
   the step driver can resolve one decision at a time — that is the next package and it is the
   same shape as the vote window.
2. **Space cannon offense is implemented but uncalled**, since the tactical action does not run
   its post-movement sequence. **Retreats are not implemented.**
3. **No invasion or production**, so a won combat still takes no planets.
4. **Fleet supply and capacity are still unenforced.**
5. **No independent review.** Waived by the project owner.
