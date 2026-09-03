# OBS-008c2a — production selection and allowance surface

## Package

- Milestone: Stage 2 complete decision contract; first production-unit slice of `OBS-008c`.
- Dependencies: `OBS-003a–c`, `OBS-004`, `OBS-007a–b`, `OBS-008c1`.
- Objective: let an actor distinguish production-unit choices by exact marginal bill, carried
  payment credit, produced yield, production-limit use, and Sol's per-use free-unit allowance.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008c`), LRR 68.1/68.1a/
  68.3b, and `crates/ti4-engine/src/breakthroughs.rs` Bellum Gloriosum implementation.
- Acceptance references: focused `obs008c2a` tests in `production.rs` and `features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/preview.rs`, `crates/ti4-engine/src/production.rs`,
  `crates/ti4-policy/src/features.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

1. A production selection has `DecisionContext { source: Rule("68"), subtype:
   "produce_unit", target: system }` and a `ProductionCapacity` constraint whose amount is the
   current use's limit and whose paid value is the amount of that limit already consumed.
2. Each build option reports the printed and actual resource bill, current carried credit, what is
   still owed, and the unit yield. Existing option IDs, labels and legal set remain untouched.
3. Each build option has an analytic preview of `ProductionRemaining` and
   `ProductionFreeCapacity` after that unit selection is completed. It must agree with actual
   placement and must never clone/apply production to compute its answer.
4. `ProductionFreeCapacity` is the per-use Bellum Gloriosum allowance, not ship transport capacity.
   It grows when a capacity ship is produced and is consumed by fighters/ground forces.
5. Policy features expose these facts under the already-approved transferable `production` family;
   missing/unknown preview facts are not numeric zero.

## Boundaries

- A preview only states the quantities it can determine before the later placement decision; it
  does not claim fleet-supply or transport aftermath here.
- Fleet/transport final-placement consequences are `OBS-008c2b`.
- No production/payment legality or application behavior changes, and no vocabulary generation,
  prompt-free migration, or bundle republish occurs.

## Tests and commands

- Add paired build choices proving different limit state, credit and Bellum allowance change the
  policy surface and survive MLP projection.
- Prove preview/application agreement for ordinary production, allowance consumption, and a
  capacity ship opening allowance.
- Run focused engine/policy tests, affected crate suites, training suite, strict all-target Clippy,
  rustfmt and diff check.

## Definition of done

The model can see exactly what a legal build costs, how much prior payment covers, what it yields,
and which one-use production constraints it leaves; quantities agree with real production; all
checks and independent Tier-C review pass; only package files are committed.
