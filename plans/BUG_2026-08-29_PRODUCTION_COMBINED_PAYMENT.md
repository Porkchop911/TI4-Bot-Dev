# Bug: incremental PRODUCTION payment discards value between unit batches

**Status:** OPEN

**Severity:** HIGH

**Reported:** 2026-08-29

## Summary

A single use of `PRODUCTION` is currently selected and paid as a series of independent unit purchases. This is incorrect when a player produces multiple unit batches, especially units whose cost covers more than one unit.

For example, four infantry are selected as two batches of two infantry. Each batch costs one resource and is paid immediately. If the first batch is paid for by exhausting a two-resource planet, the unused resource is discarded. The second batch then demands another payment source even though that planet should have paid the combined two-resource cost of all four infantry.

## Observed behavior

1. A player starts one use of `PRODUCTION`.
2. The player selects two infantry, creating a one-resource bill.
3. The player exhausts a two-resource planet to pay that bill.
4. The excess resource is lost.
5. The player selects two more infantry during the same use of `PRODUCTION`.
6. The engine creates a new one-resource bill and requires another planet or trade good.

The implementation in `crates/ti4-engine/src/production.rs` processes the production window incrementally. Each selection enters its own payment stage with a fresh amount paid, while only production capacity is retained between selections. The payment helper's overpayment exists only for the current bill and is not retained for the rest of the same production use.

## Expected behavior

One use of `PRODUCTION` has one combined production cost. The player chooses all units to produce and pays their total cost as a single bill, after applying all relevant modifiers.

Four infantry have a combined cost of two resources. Exhausting one two-resource planet must therefore pay for all four infantry. Overpayment may be lost only after the complete production bill has been settled, not after each incremental unit selection.

The user interface may continue to collect unit selections incrementally, but payment semantics must be equivalent to selecting the entire build before paying. An implementation may either defer payment until selection is finished or retain payment value within the current production use.

## Rules and state invariants

- Every distinct use of `PRODUCTION` has exactly one combined bill.
- Infantry and fighter pair pricing contributes its correct amount to that combined bill.
- Planet resources, trade goods, commodities where permitted, and other legal payment sources contribute to the same bill.
- Excess payment value is retained while the current production bill still has unpaid cost.
- Any excess remaining after the final combined bill is paid is lost normally.
- Payment value never carries into a separate use of `PRODUCTION`.
- Production discounts and cost modifiers are applied with their intended frequency to the combined bill; incremental selections must not reset or duplicate them.
- Production capacity remains based on the number of units produced, independently of how their combined resource cost is paid.
- Production must be atomic: an unaffordable final build cannot leave units placed, planets exhausted, trade goods spent, or other partial payment state behind.

## Scope

The immediate defect is the ordinary multi-selection `PRODUCTION` window. Other effects that produce units through separate resolutions should not accidentally share payment credit. Triggered and special production paths should be audited to determine whether they represent one combined production use or separate effects.

## Acceptance criteria

1. Producing four infantry in one use of `PRODUCTION` costs two resources in total, and one two-resource planet can pay the entire bill.
2. Selecting those infantry as two separate two-infantry entries does not require a second payment source after exhausting the two-resource planet.
3. A mixed build pays the sum of all selected unit costs as one bill.
4. Ending unit selection before using all production capacity charges only for the units actually selected.
5. Planets and trade goods can be combined across the complete bill, with overpayment evaluated only against the final total.
6. Production discounts and cost modifiers are consumed or applied according to one production use, not once per unit entry.
7. If the complete bill cannot be paid, no units or partial payment changes remain committed.
8. Payment credit from one use of `PRODUCTION` does not carry into another.
9. Infantry/fighter pair counts and production-capacity accounting remain unchanged and correct.
10. Automated tests cover both deferred-payment and incremental-selection behavior so future UI changes cannot reintroduce per-entry billing.

## Impact

The defect makes legal builds appear unaffordable, causes players to exhaust more planets or spend more trade goods than required, and changes game state and simulator training outcomes. It is therefore a rules-engine issue rather than only a reviewer presentation issue.
