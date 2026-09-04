# Evidence — production discount wiring (Sarween Tools, AI Development Algorithm, Harrugh Gefhara)

## Scope and provenance

- Branch: continuation of `wp/obs-008c2b-placement-consequence-surface` (no new branch cut; this is
  an engine-correctness prerequisite discovered while scoping `OBS-008c3`, not itself an OBS
  package — see `plans/BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md`).
- Base: `15158b0` (`OBS-008c2b` plus its stage-2 re-measurement).
- Normative sources: Sarween Tools, AI Development Algorithm and Harrugh Gefhara card text (read
  from the embedded content store), LRR 68 generally, and the existing War Machine/Bio-Stims
  implementations as the established pattern for "when 1+ units use PRODUCTION" and "may exhaust a
  reactive technology" respectively.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, or committed generated
  artifact.

## Changed paths

- `crates/ti4-engine/src/technology.rs`
- `crates/ti4-engine/src/production.rs`
- `crates/ti4-engine/src/game.rs`
- `crates/ti4-engine/tests/decision_delivery_inventory.rs`
- `plans/BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md`
- `plans/BUG_2026-09-04_LEADER_USE_UNREACHABLE.md`
- `plans/evidence/BUG_2026-09-04_PRODUCTION_DISCOUNT.md`
- `plans/EXECUTION_STATE.md`

## Result

See `plans/BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md` for the full defect description and
resolution. Summary: Sarween Tools and AI Development Algorithm now genuinely reduce a use of
PRODUCTION's combined resource bill; `Player::free_production_use` (Harrugh Gefhara) is now read
and correctly zeroes a use's cost when its marker names the current `production_seq`, though nothing
yet writes that marker from real play (separate, recorded defect).

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib technology::` | 28 passed (6 new) |
| `cargo test -p ti4-engine --lib production::` | 41 passed (5 new) |
| `cargo test -p ti4-engine --lib game::tests::sarween_tools_lowers_a_real_production_bill_in_a_driven_game -- --exact` | 1 passed |
| `cargo test -p ti4-engine --quiet` | 1,156 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 199 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` | 1 passed; 102-game deterministic campaign, 307.50 s (305.41 s for `OBS-008c2b` — no regression) |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine --all-targets -- -D warnings` | passed |
| `cargo fmt -p ti4-engine -- --check` | passed |
| `git diff --check` | passed |

## Analytic and end-to-end evidence

- `technology::production_used`: Sarween Tools contributes automatically with nothing asked;
  AI Development Algorithm asks, and accepting exhausts it and grants exactly the owned
  unit-upgrade-technology count; declining leaves it ready; an already-exhausted card offers
  nothing; both combine; a second, later call overwrites rather than accumulates (a stale value from
  an unrelated earlier use is discarded, not carried forward).
- `production.rs`: a build option's `cost` payload reflects the discount while `printed_cost` does
  not; the shared discount is offered to every option in one choice but spent only once, by the
  option actually resolved — proven by resolving the first of two otherwise-identical selections and
  reading the second's now-zero discount; Harrugh's marker zeroes every build when it names the
  window's own `production_seq` and grants nothing when it names a different one.
- `game.rs::sarween_tools_lowers_a_real_production_bill_in_a_driven_game`: the decisive test. Drives
  a real tactical action to completion twice through `Game`/`Table`/`Scripted` — once bare, once
  with Sarween Tools granted — and asserts the live-offered carrier costs exactly one resource less
  in the second run. This is not a hand-built `ProductionWindow`; it is the actual driven path a
  learned policy plays through.
- The 102-game deterministic campaign includes Jol-Nar (which starts holding Sarween Tools) in its
  faction rotation and completed legally and deterministically at an unchanged wall-clock cost,
  confirming the fix holds under realistic, non-scripted play.

## Decisions made and rationale

- **One arithmetic path, not a duplicate.** `ProductionWindow::discounted`/`spend_discount` mirror
  exactly how `credit` is already handled: offered to every option shown, spent only by the one
  resolved. A preview that let two offered options both claim the same point of discount would be
  wrong the same way a preview that double-counted credit would be.
- **State-level field, not window-local, for the two technology discounts.** `production_used` runs
  before the window's own first choice and has no window to hand a value to; `production_discount_remaining`
  is exactly the field the codebase already declared for this, so it is used rather than duplicated.
- **Overwrite, not add, on each call.** A use of PRODUCTION whose combined cost never reached the
  discount must not leave a residue for the next, unrelated use to spend.
- **`production_seq` incremented where `activation_seq` already is** — at the point the relevant
  window opens (`tactical.rs` for activation, now `enter_production` for production) — matching an
  established, already-reviewed pattern rather than inventing a new one.
- **Galaxy threaded from `ctx.timing`'s existing `TimingHandle`, not a new parameter.** `Resolving::ask_seeing`
  deliberately drops galaxy for callers with no map handle; this caller has one whenever real play
  does, and using it avoids a signature change through `AftermathWindow::settle`'s five call sites
  while still giving `Observed`-derived features the real board rather than a silent `None`.
- **War Machine's existing cost/capacity folding is not touched.** Its own doc comment states the
  simplification explicitly and it is separately tested and evidenced; revisiting whether folding a
  resource-cost discount into a unit-count budget is itself correct is a distinct question from the
  one this package answers, and reopening already-accepted, working code without being asked to is
  out of scope.
- **Harrugh Gefhara's invocation gap is recorded, not fixed.** It surfaced only because reading
  `use_leader` to understand the marker revealed the whole leader-use subsystem is unreachable —
  fifteen abilities, not one. Folding a fix for all of them into a discount-wiring package would
  violate the project's own atomic-package-size discipline; it is `plans/BUG_2026-09-04_LEADER_USE_UNREACHABLE.md`
  instead.

## Independent review

**OUTSTANDING.** This package touches legality-adjacent arithmetic (Tier C) and has not been
independently reviewed. The implementer is the sole reader of this diff so far.

## Non-goals retained

`integrated_economy`, `sling_relay` and `produce_one` are untouched. War Machine's implementation is
untouched. Leader-use invocation is untouched and separately recorded. `OBS-008c3` (policy-visible
discount features) is separate work, tracked next in `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`.
