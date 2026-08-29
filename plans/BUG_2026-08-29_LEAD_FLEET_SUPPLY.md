# Bug: Lead From the Front spends fleet tokens without enforcing fleet supply

Date: 2026-08-29
Status: OPEN
Severity: HIGH — rules violation creates persistent illegal board states and corrupts simulation/training outcomes

## Summary

Two linked defects exist in command-token objective payment:

1. **Lead From the Front can be paid from the fleet pool.** Its printed requirement is to spend a
   total of three tokens from the tactic and/or strategy pools. Fleet-pool tokens are not eligible.
2. **Reducing the fleet pool does not immediately enforce the new fleet-supply limit.** Ships that
   exceed the reduced limit remain on the board.

At fleet supply zero, a non-Letnev player may keep zero non-fighter ships in every system and must
remove all such ships. Letnev's Armada adds two to the limit, so Letnev may keep up to two
non-fighter ships per system when its fleet pool is zero. Fighters do not themselves count against
fleet supply; capacity must be checked after non-fighter ships are removed, as the existing fleet
enforcement routine already does.

## Current behavior and code evidence

`crates/ti4-engine/src/objectives.rs` represents `lead` as the generic `Cost::Tokens(3)`.
Affordability then checks `Player::total_tokens()`, which includes tactic, fleet, and strategy.

Payment explicitly takes tokens in this order:

```text
strategy -> fleet -> tactic
```

Consequently, a player with insufficient tactic/strategy tokens can still score Lead From the
Front by surrendering fleet tokens.

The payment path mutates the player's pools directly through `gain_token(pool, -take)`. It does not
call `fleet::enforce` or `fleet::enforce_seeing`. The fleet module already computes the correct
limit, including Fleet Regulations and Letnev's Armada, and removes excess non-fighter ships before
checking capacity—but this objective-payment path bypasses it.

The existing objective regression test only asserts that the player's **total** token count falls
by three. It therefore admits the incorrect fleet-pool payment and does not inspect the board after
a fleet-supply reduction.

## Minimal reproductions

### A. Illegal objective payment

1. Reveal Lead From the Front (`lead`).
2. Give a non-Letnev player fewer than three combined tactic + strategy tokens.
3. Give that player enough fleet tokens for their total across all pools to reach three.
4. Ask whether the objective is affordable and attempt to score it.

**Observed:** the objective is affordable, scores, and consumes fleet tokens.
**Expected:** it is unaffordable; fleet tokens cannot pay this objective.

### B. Fleet supply is not enforced

1. Place three non-fighter ships belonging to a non-Letnev player in one system.
2. Set that player's fleet pool to three.
3. Reduce the fleet pool to zero through the affected payment path.

**Observed:** all three ships remain in the system.
**Expected:** fleet supply is now zero and all three non-fighter ships must be removed; capacity is
then enforced for any fighters or ground forces left in space.

Repeat with Letnev:

**Expected with Armada:** the limit is two at fleet pool zero, so only one of three non-fighter
ships is removed.

## Rules-correct behavior

- Lead From the Front affordability counts only `tactic_tokens + strategic_tokens`.
- Its payment removes exactly three tokens from only those two pools.
- If the engine retains automatic payment rather than asking the player for the split, that
  simplification must still never touch the fleet pool.
- Every operation that reduces fleet tokens must enforce the resulting fleet limit across every
  system containing that player's units.
- Fleet enforcement must use the existing adjusted limit so laws and Armada remain effective.
- Supply is enforced before capacity, matching the existing `fleet::enforce_seeing` ordering.
- A failed removal choice must not leave a half-resolved score or silently preserve an illegal
  fleet; the scoring/payment boundary must define atomic error behavior.

## Acceptance tests

1. `lead` is unaffordable with tactic 1, strategy 1, fleet 8.
2. `lead` is affordable with tactic 1, strategy 2, fleet 0.
3. Paying `lead` never changes `fleet_tokens`.
4. Paying `lead` deducts exactly three combined tactic/strategy tokens and awards exactly one
   objective score.
5. Reducing an ordinary player's fleet pool from three to zero removes every non-fighter ship from
   every system they occupy.
6. The same reduction for Letnev preserves at most two non-fighter ships per system because of
   Armada.
7. When carrier removal strands capacity-consuming units, the existing post-supply capacity pass
   removes the excess.
8. Enforcement covers all fleet-token reduction call sites, not only objective scoring.
9. The owning player/decider chooses removals through the established fleet-removal choice path.
10. Regression tests distinguish tactic, strategy, and fleet pools rather than asserting only a
    change in their combined total.

## Impact

This is not a presentation-only defect. It changes objective eligibility, awards victory points
that should be unavailable, preserves fleets that the rules require removing, and exposes learned
policies to impossible positions. Replays and training/evaluation results produced through the
affected path may therefore contain rules-invalid state transitions.

## Resolution, 2026-08-29

**Defect 1 (fleet tokens are not eligible): FIXED.**

`TOKEN_COST_POOLS` names the two pools the cards allow — tactic and strategy — and both
affordability and payment read it. Strategy is taken before tactic among the two that are eligible:
a tactic token is the scarcer resource in an action phase and nothing in either card prefers one.

`bought_progress` needed no change: it derives from `can_afford`, so fixing affordability fixed
progress with it. The existing test
`token_progress_is_exact_across_all_small_pool_splits` asserted the *sum of all three pools* and so
encoded the bug; it now varies the fleet pool deliberately and asserts the answer does not move.

**Defect 2 (fleet supply not re-enforced when the pool shrinks): PARTIALLY ADDRESSED.**

`fleet::enforce_everywhere` now exists and does the right thing — both enforcement loops fall through
when a seat is inside its limits, so it is cheap and silent on the common step.

It is deliberately **not** called from the game loop. Doing so enforces limits for every seat in
every system continuously, which is arguably what 58.4 requires, and it broke eight existing tests
whose fixtures set up positions that were legal only because nothing looked. That is a behavioural
change large enough to move the `ti4-sim` baseline on its own, and it deserves its own reviewed
change rather than riding along with the eligibility fix.

With defect 1 fixed, the specific route this bug reported — scoring Lead From the Front by
surrendering fleet tokens — is closed. The general gap remains: Fleet Regulations and Clandestine
Operations both shrink the pool from sites with no decider.

**Behavioural baseline:** the fix changes play, because bots were buying the objective this way.
`vp_pace` falls 0.416 -> 0.387 and `faction_differentiation` rises 0.432 -> 0.563. Recorded as v5 in
`plans/evidence/M08-021.md`, approved by the project owner.
