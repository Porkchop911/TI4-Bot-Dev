# Bug: Sarween Tools, AI Development Algorithm and Harrugh Gefhara never reduced a production bill

**Status:** FIXED 2026-09-04

**Severity:** HIGH — every game a faction owning Sarween Tools has ever played through this engine
charged its units' full printed price. Jol-Nar starts every game holding it.

**Reported:** 2026-09-04, while scoping `OBS-008c3` (exposing production-discount facts to the
learned policy). The package could not be written honestly: there was no discount for a preview to
report, and previewing a constant zero is exactly the "confident zero" failure `OBS-007a`'s preview
contract exists to prevent.

## Summary

`GameState::production_discount_remaining` and `Player::free_production_use` are declared, typed,
compared for equality, and correctly initialized to their empty values. Their doc comments
correctly describe Sarween Tools, AI Development Algorithm and Harrugh Gefhara. Nothing wrote to
`production_discount_remaining` and nothing read either field. `ProductionWindow` charged every
build's full printed cost regardless of what technologies or leaders a player held.

`GameState::production_seq`, the field `free_production_use`'s value is compared against ("this use
of PRODUCTION, not a later one"), was declared and never incremented — it held `0` for the life of
every game.

## Current behavior and code evidence (before this fix)

- `crates/ti4-model/src/state.rs`: `production_discount_remaining: i32` and
  `free_production_use: Option<u32>` are fields with no writer anywhere in the crate.
- `crates/ti4-engine/src/leaders.rs::use_leader`, the `"hacanhero"` arm, wrote
  `seat.free_production_use = Some(seq)` — but `use_leader` itself is called from nowhere in the
  driven game loop (see the separate, larger, deliberately unfixed finding below).
- `crates/ti4-engine/src/production.rs::ProductionWindow` computed cost from
  `price_of_under(Some(state), kind)` alone, with no discount subtracted anywhere in `build_options`
  or `resolve`.
- Jol-Nar's `startingTech` is `["amd", "st", "nm", "ps"]` — `st` is Sarween Tools' alias. Every
  Jol-Nar game measured by this project's stage-2 evaluation (`stage2-r3`/`r4`/`r5`, confirmed
  2026-09-04) scored under this defect: Jol-Nar's real combined production bill was one resource
  higher than the rules allow, every use of PRODUCTION, for the entire campaign.

## Resolution, 2026-09-04

**Sarween Tools: FIXED.** `technology::production_used`, called once from
`game.rs::AftermathWindow::enter_production` at the same "when 1 or more of your units use
PRODUCTION" moment War Machine already reacts to, grants it automatically — the card names no
"may" and nothing to exhaust. `ProductionWindow::discount_remaining` is populated from
`state.production_discount_remaining` in `refresh()` and spent by
`ProductionWindow::discounted`/`spend_discount` exactly the way `credit` is: offered to every
option shown in one choice (a preview cannot let two offered options both spend the one point of
discount), and drawn down for real only for the option actually chosen.

**AI Development Algorithm: FIXED.** The same `technology::production_used` asks — a genuine
choice, since exhausting the card forecloses its other ability (ignoring one prerequisite) until it
readies. Declining leaves it ready; accepting exhausts it and grants a discount equal to the
player's owned unit-upgrade technology count (`technology::is_unit_upgrade`).

**Harrugh Gefhara: the consumption side is fixed; the leader is still unreachable in real play.**
`ProductionWindow::free_this_use` now reads `free_production_use == Some(production_seq)` in
`refresh()` and zeroes every build's cost when true — proven directly by driving the marker into
state and confirming the discounted bill (`production.rs::a_free_production_marker_for_this_seq_zeroes_every_build`).
`production_seq` is now incremented once per opened production step
(`enter_production`), so the marker's own sequence check is meaningful rather than comparing
against a value that never moves. What is **not** fixed: nothing in the driven game loop ever
offers "use a leader" as a choice at all, for any of the fifteen registered leader abilities, not
only Harrugh's. That is a separate, much larger defect, recorded rather than folded into this
package: `plans/BUG_2026-09-04_LEADER_USE_UNREACHABLE.md`.

**Affordability** is judged on the discounted bill (`build_options`'s affordability check moved to
after the discount is applied), so a unit the discount brings within reach is offered rather than
withheld for a price nobody would actually charge.

**`production_seq`: FIXED.** Incremented in `enter_production`, mirroring how `tactical.rs` already
increments `activation_seq` at the start of a tactical action.

## Rules and state invariants

- Sarween Tools and AI Development Algorithm's discounts are per-use: `production_used` overwrites
  `production_discount_remaining` rather than adding to it, so an unspent remainder from a use whose
  combined cost never reached it cannot leak into a later, unrelated use.
- The discount reduces the combined resource bill (`cost`), never the PRODUCTION-value budget
  (`remaining`/`capacity`) units are counted against — these are two separate quantities in this
  engine's own accounting, confirmed by reading `capacity()`'s and `place()`'s bodies, and are not
  conflated here the way War Machine's existing (separately reviewed, untouched) implementation
  folds its own "-1 combined cost" into "+5 capacity". That existing shortcut is flagged, not
  changed, in the evidence file.
- `printed_cost` stays the face value; `cost` is the actual bill after the discount. This is the use
  `printed_cost` was already split out from `cost` for in `OBS-008c1`/`c2a`, so no existing payload
  contract changes meaning — it starts being used as designed.
- A discount offered to several build options shown in the same choice cannot be spent by more than
  one of them: the preview states what taking *that* option would cost, and only the option actually
  resolved draws the shared discount down.

## Non-goals

- `production.rs::integrated_economy`, `sling_relay` and `produce_one` are separate producers with
  their own cost/budget arithmetic, outside `ProductionWindow`. Whether Sarween Tools/AID apply to
  them too is a real rules question this package does not answer; they are unchanged.
- War Machine's existing cost/capacity folding is not revisited.
- Offering "use a leader" as a choice: see the separate bug doc.
- `OBS-008c3` (exposing `cost`/`discount` as a policy feature) is separate work, tracked in
  `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`.

## Verification

- `crates/ti4-engine/src/technology.rs`: six focused unit tests (`production_used`'s automatic
  Sarween contribution, AID's ask accepted/declined/already-exhausted, both combined, and
  overwrite-not-accumulate across uses).
- `crates/ti4-engine/src/production.rs`: five focused tests (the discounted bill on a real build
  option, the discount spent once across two selections in the same use, Harrugh's marker zeroing a
  build for its own sequence and granting nothing for a different one).
- `crates/ti4-engine/src/game.rs`: one end-to-end test driving a real tactical action through to
  production twice — with and without Sarween Tools — and asserting the discounted run's offered
  carrier costs exactly one resource less, through the real driven `Game`/`Table`/`Scripted` path,
  not a hand-built window.
- Full suites: engine 1,156 lib + 4 integration + 5 docs; policy 199 + the 102-game deterministic
  campaign (307.50 s, against 305.41 s for `OBS-008c2b` — no regression); training 133; strict
  Clippy and `cargo fmt --check` clean for every changed file.

Evidence: `plans/evidence/BUG_2026-09-04_PRODUCTION_DISCOUNT.md`.
