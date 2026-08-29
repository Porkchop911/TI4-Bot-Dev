# Handoff: the rest of the engine completion plan

Written 2026-08-29 by the engine implementer, for whoever continues. Companion to
`plans/ENGINE_COMPLETION_PLAN.md`, `engine-rules-audit.md`, and `plans/CARD_CONTENT_STATUS.md`.

Branch `wp/engine-completion` (worktree `D:/Projects/ti4-engine-work`) holds this work merged with
the card content from `wp/r01-review-viewer-contract`. **Merge it back before continuing.**

## Where things stand

| area | now |
|---|---|
| action cards | 82 of 142 |
| agendas | 63 of 63 |
| public objectives | 40 of 40 |
| secret objectives | 40 of 40 |
| relics | 14 of 24 |
| exploration | 73 of 80 |
| laws | 17 of 40 unenforced |
| reaction windows | **0 unsupported** — every printed window has its event and its binding |
| leaders (the six) | 3 unimplemented |
| breakthroughs (the six) | 3 unimplemented |

All eight defects in the original audit are fixed. Thunder's Edge mechanics are in: space stations,
coexistence, neutral units, synergy, entropic scars, the Fracture.

## Three hooks are ready to use now

Built against the printed card text. Each is scoped by a sequence number, in the style
`combat_bonus_round` already set — an unscoped bonus follows a seat for the rest of the game.

```rust
vote::add_votes(state, player, n)          // Distinguished Councilor, Bribery
combat::grant_hit_cancellation(state, p, n) // Shields Holding x4
combat::bar_retreat(state, player)          // Intercept
```

That unblocks **9 cards**: `distinguished`, `bribery`, `sh1`–`sh4`, `intercept`, and the two halves
of vote weighting. `hack` (vote last) still needs the vote *order* to be re-orderable.

## What each remaining group needs

Ordered by cards unblocked per unit of work.

### Scoped roll modifiers — 6 cards
`f_prototype` (+2 to fighters this round), `bunker` (-4 to BOMBARDMENT against your planets this
invasion), `war_machine1`–`4` (+4 PRODUCTION, -1 combined cost).

The pattern exists: `Player::combat_bonus_round` is Morale Boost. These need the same thing with a
*filter* — a unit-type predicate for `f_prototype`, a target predicate for `bunker`. Suggest one
field holding `(scope_seq, filter, delta)` rather than a field per card.

### Reroll — 3 cards
`fire_team` (reroll your ground dice), `scramble` (opponent rerolls all of theirs), and Jol-Nar's
commander. `Dice::reroll` already exists and records the replaced positions. What is missing is a
decider at the roll site: `bombardment_at` and `roll_ground` have `dice` and `rng` but no `Table`.
**This is the same blocker as the three faction leaders and coexistence 7/7.1** — four separate
items, one refactor. Thread `&mut Table` (or the whole `Resolving`) into the unit-ability roll path
and all four open at once. **Highest-leverage remaining work.**

### Invasion flow — 5 cards
`blitz` (grant BOMBARDMENT 6 to non-fighter ships this invasion), `disable` (opponents' PDS lose
PLANETARY SHIELD and SPACE CANNON), `parley` (return committed units to space), `ghost_squad` (move
ground forces between planets in the active system), `bunker`.

`UNITS_COMMITTED` is emitted per landing and carries the controller, so the windows fire. What is
missing is the ability to *undo* or *modify* a commit. Note `entropic_scars::abilities_usable` is
exactly the shape `disable` needs — a predicate asked where the ability fires.

### Cancel API — 4 cards
`sabo1`–`4`. `Resolver::emit_with_context` already returns `cancelled`, and
`reactions::announce` already honours it: "1.15 lets a WHEN ability cancel the event, not un-spend
the card." So the mechanism exists — what is missing is a card effect that *sets* it. Check whether
a WHEN reaction on `ACTION_CARD_PLAYED` can cancel before writing new machinery.

### Movement — 2 cards
`lost_star` (alpha/beta adjacent this tactical action), `solar_flare` (no SPACE CANNON against your
ships during movement). `Galaxy::wormholes_all_linked` is the Wormhole Reconstruction switch and is
exactly what `lost_star` wants, scoped to `activation_seq` instead of to a law. See
`laws::apply_to_galaxy`.

### Agenda and turn flow — 9 cards
`veto`×3, `confusing`, `confounding`, `deadly_plot`, `coup`, `crisis`, `master_plan`. These need
agenda redirection and a queue in `game.rs`. Largest and least shaped; do it last.

### Remaining relics — 10
`codex`, `mawofworlds`, `enigmaticdevice`, `dominusorb`, `stellarconverter`, `emphidia`, `thalnos`,
`heartofixth`, `neuraloop`, `titanprototype`. Several are ACTIONs and fit `use_relic` directly.
`heartofixth` ("after any die is rolled, add or subtract 1") needs the same roll-site decider as the
reroll group.

## Two things that must not be lost

**The behavioural baseline is red and must be moved once.** `ti4-sim`'s
`the_suite_reproduces_and_stays_within_the_recorded_bounds` fails because binding the eleven reaction
windows changes what `arm` registers per seat, which changes what deciders are asked. The owner has
confirmed the baseline is diagnostic and may be moved when the work is finished. Use
`cargo run --release -p ti4-sim --example rebaseline_behavior`, which prints old against new and
changes nothing, then record v5 -> v6 in `plans/evidence/M08-021.md` with the cause. v4 and v5 are
already recorded there as worked examples.

**`BUG_2026-08-29_PRODUCTION_COMBINED_PAYMENT` is still open** and overlaps `payment_faces`, where
`planet_value_now` and `price_of_under` now live. Read that bug before restructuring payment.
`BUG_2026-08-29_PROMISSORY_NOTE_TRANSACTION_OFFERS` is untouched and independent.

## The failure mode this codebase keeps producing

Every real defect found in the last stretch was the same shape: **a registry that had drifted from
the code that dispatches from it, guarded by a test asserting something weaker than the invariant.**

- `action_cards::unimplemented` returned every card unconditionally and never consulted `effect_for`.
  Its test asserted `len() > 50`, which held either way. 34 implemented cards were reported missing.
- Public objectives were counted from `registered_aliases` while the scorer reads `cost_of` too. Ten
  working cards were reported missing.
- Four laws were listed in `enforced_aliases` with a predicate written and no caller.
- Five passive relics added to `registered_aliases` would have been offered as component actions,
  because `available_actions` read that list rather than the arms of `use_relic`.

When you finish a batch, check the registry against the dispatch **programmatically**, and grep each
new helper for a caller. Both checks take a minute and each has caught a real defect here.

Two tests also *expired* rather than broke — one required an unregistered agenda to exist, one
required all 142 action cards to be unimplemented. A fixture that depends on the engine being
incomplete dies the moment it is completed; that is success, not regression.
