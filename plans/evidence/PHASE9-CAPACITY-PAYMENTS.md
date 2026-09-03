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

Payments require Tier-C independent review. The implementation and evidence are ready, but no
independent reviewer was invoked in this task, so that gate remains open.
