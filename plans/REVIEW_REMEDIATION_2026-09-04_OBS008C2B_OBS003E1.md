# Review remediation — OBS-008c2b and OBS-003e1 (2026-09-04)

## Package

- Milestone: Stage 2 complete-decision contract, Tier-C review remediation.
- Dependencies: `OBS-008c2b` (`caac1e4`, `15158b0`) and `OBS-003e` slice 1 (`a5cfb29`).
- Objective: remove an untruthful production-placement removal forecast and give status-phase token
  redistribution its actual typed source rather than mislabeling it as Warfare.
- Normative sources: `plans/OBS-008C2B_PLACEMENT_CONSEQUENCE_SURFACE.md`,
  `plans/OBS-003E1_STRATEGY_CARD_CONTEXT.md`, LRR 16/37/81.5, and the two Tier-C review findings.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/production.rs`, `crates/ti4-engine/src/strategy_cards.rs`,
  `crates/ti4-engine/src/game.rs`, `crates/ti4-policy/src/features.rs`, this specification, package
  evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, and external-state changes: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

1. Placement options state `fleet_excess_after` and `capacity_excess_after`: exact violations in
   the reached pre-enforcement position. They do not state an exact eventual removal count, because
   fleet enforcement runs first, changes capacity, and includes player choices.
2. Policy reads those two truthful facts under the existing transferable `production` family; the
   false `units_removed_after` feature is removed.
3. The shared token-redistribution helper accepts the caller's `DecisionSource` and subtype.
   Warfare primary remains `StrategyCard(Warfare)/warfare_redistribute_tokens`; status phase is
   `Rule(81.5)/status_redistribute_tokens`. The caller also supplies the visible prompt, so status
   phase does not present itself as Warfare.

## Invariants and boundaries

- No change to fleet/capacity enforcement, placement legality, token redistribution legality,
  option IDs, labels, or timing.
- Do not claim the number of later removals without modelling its order and player choices.
- No vocabulary generation, bundle republish, or unrelated `action_cards.rs` change.

## Tests and commands

- Assert placement facts expose each independent excess and no removal prediction.
- Assert status redistribution carries Rule 81.5 rather than Warfare.
- Update the production policy counterfactual/projection test to use the two excess facts.
- Run focused tests, engine and policy affected suites, training, strict Clippy, formatter scope and
  diff check. Obtain an independent Tier-C re-review before commit.

## Definition of done

Both review findings are resolved with focused regressions; the decision surface names only facts
it can know, status and Warfare redistribution are distinguishable, checks pass, independent review
approves, and only scoped files are committed.
