# Phase 9 follow-up — faction capacity, production, and Leadership payment

Date: 2026-09-03

Objective: honor printed faction-unit production/transport capacity from setup and make Leadership
charge one combined influence bill. Normative references: LRR 16 (capacity), 52.3 (Leadership), 68
(one use of PRODUCTION), and the printed faction sheets/unit records in the embedded corpus.

Allowed edit paths: `crates/ti4-content/src/factions.rs`,
`crates/ti4-engine/src/{production,seating,strategy_cards}.rs`, simulator baseline/evidence, and this
file. Invariants: legal choices are generated, failed payments do not create tokens, payment credit
cannot escape its Leadership transaction, generic units remain the fallback, and deterministic
simulation is preserved. Non-goals: changing unit data, changing production prices, or redesigning
the generic payment UI.

Permission class: P1. Writable paths were limited to Rust source/tests and local evidence inside
`D:\Projects\ti4-engine-rs`. No network, external process, destructive action, or external-state
change was used. Existing untracked review samples and scripts were left untouched.

## Defects and resolution

- Starting fleets resolved only generic `mech` and `flagship` codes against the faction sheet.
  `dreadnought`, `spacedock`, carrier, infantry, and fighter replacements were silently generic.
  `resolve_unit` now checks every starting type and falls back to the generic record only when the
  faction has no replacement.
- L1Z1X's starting dreadnought is consequently `l1z1x_dreadnought`, whose corpus capacity is 2.
- Saar's opening `sd` is consequently `saar_spacedock`, whose flat PRODUCTION value is 5. Warfare's
  secondary continues to use one `ProductionWindow`; its focused test now explicitly asserts that
  the number produced cannot exceed the per-use production limit.
- Leadership (LRR 52.3) is one "spend any amount of influence" transaction. Payment now retains a
  planet face's overpayment within that transaction. A 4-influence planet plus Arinam's printed 2
  buys two tokens for six total; the second offer accurately says that two more influence is owed.

## Checks

- `cargo test -p ti4-content`: 129 passed; doc tests 1 passed.
- `cargo test -p ti4-engine`: 1,107 passed; doc tests 5 passed.
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-content -p ti4-engine --all-targets`: passed.
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-content -p ti4-engine -p ti4-sim --all-targets`:
  passed after the final refactor.
- `cargo test -p ti4-sim`: 52 passed.
- `cargo run --release -p ti4-sim --example rebaseline_behavior`: deterministic v33 table recorded
  in `plans/evidence/M08-021.md`; only `share_SHIP_MOVED` left v32.

## Review status

Payments require Tier-C independent review. **Performed; approved, with one note.**

### Verified by mutation, not by reading

Each claim was checked by breaking the fix and confirming the guard fires, which is the only way to
tell a regression test from a test that happens to pass.

**Leadership combined payment.** `influence_purchase_choice` was mutated to ignore the retained
credit (`let owed = INFLUENCE_PER_TOKEN;`). `leadership_carries_planet_overpayment_across_command_
tokens` failed. Reverted, green. The guard is real: without the credit the seat is asked for a third
influence and pays seven for two tokens, which is the reported defect.

**Warfare secondary production limit.** `ProductionWindow`'s `remaining` was mutated to
`capacity(..) + 1`. `warfare_secondary_produces_in_the_home_system` failed on its own assertion
("cannot reset its limit between purchases"). Reverted, green.

**Super dreadnought capacity.** Checked independently rather than through the fix's own tests: the
corpus gives `l1z1x_dreadnought` capacityValue 2 against 1 for the generic `dreadnought`, and the
engine's capacity arithmetic honours it — two fighters sit in a Super Dreadnought and a third is
excess. A focused test is added at the fleet-capacity level, where the reported symptom lives, since
the existing coverage approaches this through setup and unit resolution instead.

### Note

`warfare_secondary_produces_in_the_home_system` derives its expected bound from
`production::capacity`, the same function the engine consults. It therefore guards the ENFORCEMENT
path — a window that ignored its limit fails, as the mutation showed — but not the capacity VALUE.
If `capacity` itself returned the wrong number for a faction dock, the test would compare against
the wrong bound and still pass. Saar's `saar_spacedock` PRODUCTION of 5 is the case that motivated
this change, so an assertion on the printed value, independent of `capacity`, would close the gap.

Full engine suite green at review time: 1,109 tests.
