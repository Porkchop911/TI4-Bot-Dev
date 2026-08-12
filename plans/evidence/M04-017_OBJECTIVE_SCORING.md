# M04-017 — Objective scoring (LRR 61, 81.1, 98)

## Package

| Field | Value |
|---|---|
| IDs | M04-017 — the 81.1 scoring window, the requirement registry, and the victory check |
| Depends | M04-016 (status token gain), M03-001…005 (choice model) |
| Objective | Give the status phase its scoring step so a generic game can complete a round. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Ported from `engine/objectives.py` (`scoreable`, `award`, `leader`, `winner`,
`controls_home_system`, `points_for`, the requirement helpers) and `engine/game.py`
(`_score_objectives`, `_check_victory`).

## The headline

**A generic game now completes a whole round.** Every one of the 100 seeded two-to-six-player
runs in the M04-014 campaign runs to the end of the round without a single step refusing, where
before this package all 100 stopped at an unimplemented boundary.

That test changed from asserting a bounded *failure* to asserting a bounded *success*, and it
is renamed accordingly: `one_hundred_seeded_generic_games_complete_a_round_with_only_offered_choices`.
It still checks that every recorded decision was one the engine offered.

## An unregistered objective is unscoreable, by design

The oracle's module docstring is explicit, and this is a faithful port of the *design*, not just
the functions:

> An objective with no registered predicate cannot be scored. That is deliberate: an
> unimplemented requirement must make the objective unavailable, never silently scoreable, so
> coverage gaps show up as an objective nobody can take rather than as a bot quietly winning on
> a rule that was never written.

This package registered a **first tranche** — the planet-control family:

| Alias | Requirement |
|---|---|
| `expand_borders` | control 6 planets in non-home systems |
| `subdue` | control 11 planets in non-home systems |
| `corner` | control 4 planets sharing a trait |
| `unify_colonies` | control 6 planets sharing a trait |
| `research_outposts` | control 3 planets with technology specialties |
| `brain_trust` | control 5 planets with technology specialties |

A **second tranche** followed in the same branch, covering technology and structures:

| Alias | Requirement |
|---|---|
| `develop` / `revolutionize` | own 2 / 3 unit-upgrade technologies |
| `diversify` / `master_science` | own 2 technologies in each of 2 / 4 colours |
| `build_defenses` / `massive_cities` | have 4 / 7 structures |
| `infrastructure` / `protect_border` | have structures on 3 / 5 planets outside your home system |

90.7b says unit upgrades have no colour, so they are counted separately rather than as one more
colour — `unit_upgrades_are_not_counted_as_a_colour` gives a player six upgrades and asserts
Diversify stays unscoreable, because otherwise a stack sharing no research track would score it.
`a_ship_in_space_is_not_a_structure` guards the other direction: structures sit on planets, and
counting hulls would make Build Defenses scoreable off a fleet.

The oracle registers 32; **14 are now covered, and the other 18 are deliberately absent and
therefore unscoreable.**
`unregistered_objectives()` reports which revealed objectives no predicate covers, so the gap is
queryable rather than buried — and two tests pin the tranche itself: every listed alias resolves
to a predicate, and every listed alias is a real objective in the corpus (a predicate registered
against a misspelled alias would never fire and nothing else would ever say so).

## What is complete rather than tranched

Scoring's *machinery* is fully ported, not sampled:

* **61.8** — an objective scores once per game per player.
* **61.16** — no public scoring without control of your whole home system.
* **98.4a** — victory points cap at the target. Without the cap a final objective could push a
  player past ten, and any check written as `== VICTORY_TARGET` would miss the win entirely.
* **98.7/98.8** — ties break by initiative order, including a scoreless game, where everyone
  level on zero is tied and the first in initiative takes it rather than the result being null.
* Points are read from **both decks**, because Classified Document Leaks moves a secret
  objective into the public area where anyone may score it, and a public-only lookup would
  silently value it at nothing.

## Order matters: 81.1 runs first

Scoring can end the game, which is why LRR 81 puts it before the reveal and why the driver now
opens the scoring window *before* the 81.2–81.4 bookkeeping added in M04-016. Players with
nothing scoreable are skipped rather than asked — the oracle offers them their secrets instead,
and with secrets unimplemented a forced "decline" would be a question with one answer.

## A performance defect found and fixed

The first working version resolved a player's controlled planets inside every predicate, which
rebuilt an index over the whole planet corpus for every requirement of every player on every
step. It was correct and it was quadratic: the 100-seed campaign stopped terminating in any
reasonable time and had to be killed.

`Position::new` now resolves the controlled-planet records once. The campaign runs in 0.20 s.

This is recorded rather than quietly fixed because the failure mode is worth remembering — the
code was never wrong, only unusable, and a test suite is what turned that into a visible event.

## An infinite loop this package introduced

One M04-016 test looped `while game.step().error.is_none() {}`, which terminated only because
the status phase used to refuse. With the phase completing, that loop ran rounds forever. It is
now bounded by phase and a step guard. Worth stating plainly: completing a phase turned a
previously-safe test into a hang.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No secret-objective window (`_score_secret`). | Secrets are not modelled yet. The window is public-only, and players with no public option are skipped rather than being asked a one-answer question. |
| No purchase objectives (61.10 `COSTS`). | Those spend resources, influence, trade goods or tokens; spending is not implemented. None are in the registered tranche, so none can be offered. |
| `Game` carries the source scope. | `GameState` does not record the scope it was set up under, so a game built under FULL and driven with the default would resolve its planet catalogue against the wrong corpus. `Game::with_sources` makes it explicit. **This is a latent gap in the state model**, flagged below. |
| No leader unlock check after scoring. | Leaders are not implemented. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
157 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
347 total, 0 failed        (332 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in objectives.rs or tokens.rs

$ cargo fmt --all      # clean
```

## Open findings

1. **18 of 32 requirement predicates are unregistered** after the second tranche, so many
   revealed objectives still cannot be scored. This is the designed behaviour for a gap, not a
   silent failure, but it means a game still ends by exhausting the objective deck more often
   than by anyone reaching ten.
2. **`GameState` does not record its source scope.** `Game` holds it instead. A state loaded
   from disk and driven without `with_sources` will score against the PoK catalogue whatever it
   was built from. This should move into the state model.
3. **Secret objectives are not modelled at all** — no deck, no window, no scoring.
4. **Agenda voting remains unimplemented** (`AgendaChoicesUnimplemented`).
5. **No independent review.** Waived by the project owner.
