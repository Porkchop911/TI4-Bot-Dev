# Bug: using a leader is offered nowhere in the driven game loop

**Status:** OPEN — found 2026-09-04 while wiring the production discount; deliberately not fixed
in that package.

**Severity:** HIGH — fifteen registered leader abilities, across every faction with an implemented
leader, are unreachable from real play.

## Summary

`crates/ti4-engine/src/leaders.rs::use_leader` is a single, correct, tested entry point for using
any agent, hero or commander ability. `leaders::registered_abilities()` names fifteen implemented
leaders. Every one of `use_leader`'s match arms is exercised by `leaders.rs`'s own unit tests, which
call `use_leader` directly against a hand-built state.

`use_leader` is called from nowhere else in the crate. No producer offers "use a leader" as a
`ChoiceOption`. No step in `game.rs`'s driven loop asks. A policy — learned or scripted — has no
path to ever use an agent, a hero, or benefit from a commander's unlock. The whole subsystem is the
same shape as the five modules `wiring.rs` was written to catch (combat, invasion, fleet
enforcement, production, leaders) — but `wiring.rs` proves each *module* is reached, not that every
ability inside a reached module is; `leaders.rs` is not otherwise called from the driven loop at
all, so this one would already fail a reachability check if `wiring.rs` covered it, and does not
currently.

## Impact

- Every agent's tap ability (Carth of Golden Sands' two commodities, Elder Qanoj's off-board planet
  ready, and eleven others) is dead code.
- Every hero's one-shot ability (Jace X clearing the board's command tokens, The Helmsman's fleet
  gather, Harrugh Gefhara's free production, and others) is dead code.
- Commander unlock passives (`pays_on_sustain`, `combines_planet_values`, `vote_bonus`, and
  `ignores_planetary_shield`) are separately gated by `LeaderStatus::Unlocked` and are read directly
  by name from several rule sites — those are unaffected, since a commander needs no "use" action to
  grant its passive once unlocked. Only the *active-use* abilities (agents and heroes) are affected.
- Any faction differentiation or evaluation metric that assumes leader abilities contribute to
  observed play has been measuring a game where they never fire.

## What this package intentionally does not do

`crates/ti4-engine/src/production.rs::ProductionWindow::free_this_use` now correctly reads
`Player::free_production_use` once it exists (see
`plans/BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md`), proven by writing the marker directly into
state and observing the discount apply. That is the *consumption* side. This defect is the
*invocation* side: nothing ever asks a policy whether it wants to use Harrugh Gefhara — or any other
leader — in the first place. Fixing invocation needs no further change to `production.rs`: the
moment a policy can answer "use Harrugh Gefhara" and reach `use_leader`, the discount already
applies correctly.

## What a fix needs to decide

- Where in the action-phase turn structure "use a leader" belongs: alongside strategic/tactical
  action selection (a genuine alternative to activating a system), as a step within an already-open
  window (an agent used mid-tactical-action, as several ability texts imply by their own timing),
  or both, depending on the leader's printed timing.
- Whether every leader's printed timing is even "any time during your turn" (agents generally are)
  versus a specific window (Harrugh's and War Machine's shared "when 1+ of your units use
  PRODUCTION" is exactly the mechanism `technology::production_used` now uses for Sarween
  Tools/AID, and is the most natural home for wiring Harrugh's ask alongside them once this is
  addressed).
- Whether to route through the existing `reactions.rs` generic `Ability`/window-matching machinery
  (built for action cards) or an ad hoc call site per timing, matching how `technology.rs::end_turn`/
  `start_turn`/`production_used` already resolve their own reactive prompts without the generic
  system.
- Atomic scope: fifteen abilities across at least five factions is too large for one package by this
  project's own size discipline (`plans/PI_WORK_PACKAGE_STANDARD.md`). A real fix should split by
  timing window (agents usable "any time" vs. heroes/abilities tied to a specific reactive window)
  or by faction, not attempt all fifteen at once.

## Evidence this is real, not a guess

`grep -rln "use_leader" crates/ti4-engine/src/*.rs` returns only `leaders.rs` itself. Every call is
inside `#[cfg(test)]`.
