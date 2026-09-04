# OBS-008c2b — placement fleet and transport consequence surface

## Package

- Milestone: Stage 2 complete decision contract; second production-unit slice of `OBS-008c`.
- Dependencies: `OBS-003a–c`, `OBS-004`, `OBS-007a–b`, `OBS-008c1`, `OBS-008c2a`.
- Objective: let an actor see the fleet-supply and transport aftermath a produced unit reaches once
  its destination is known, and see explicitly when that destination is not yet decided.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008c`, representation rules
  5 and 6), LRR 16.2/16.3/16.3a, 37.1/37.1a/37.3, 68.2/68.4, 31.4, and the Fighter II text already
  implemented in `crates/ti4-engine/src/fleet.rs`.
- Acceptance references: focused `obs008c2b` tests in `fleet.rs`, `production.rs` and `features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/fleet.rs`, `crates/ti4-engine/src/production.rs`,
  `crates/ti4-policy/src/features.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

1. `fleet.rs` gains one arithmetic that answers fleet supply and transport together for a seat in a
   system, optionally with a hypothetical arrival. `over_supply`, `over_capacity` and
   `fighters_over_capacity` are expressed through it, so a preview and the enforcement that later
   removes units cannot drift apart by construction.
2. A production placement choice (`Stage::Placing`) carries `DecisionContext { source: Rule("68"),
   subtype: "place_unit", target: system }` with a `FleetSupply` constraint (limit, already charged)
   and a `TransportCapacity` constraint (transport present, already consumed).
3. Every placement option reports where it places, how many units actually arrive after LRR 31.4
   component limitation, the capacity it consumes, and an analytic `Preview` of
   `FleetSupplyHeadroom` and `CapacityFree` after that destination is taken.
4. A build option whose unit has exactly one legal destination extends its `OBS-008c2a` preview with
   the same two quantities, because that destination is already determined. A build option with a
   still-open destination states `placement_pending` instead and emits no fleet or transport
   quantity — an explicitly undetermined fact, never a factual zero.
5. Component limitation is part of the previewed arithmetic, not an assumption: a batch the box
   cannot supply previews the count that will actually be placed.
6. Policy features expose the new constraints, payload facts and preview quantities under the
   existing transferable `production` family.

## Boundaries

- No change to fleet-supply, capacity, component-limitation or placement legality, to option IDs,
  labels or the legal set, or to when enforcement runs.
- Negative headroom is reported, not prevented. Producing beyond a limit stays legal at placement and
  is resolved by the existing end-of-turn enforcement; this package only makes the outcome visible.
- Separating one planet destination from another needs control, defence and adjacency facts owned by
  `OBS-006`/`OBS-008a`. This package deliberately does not add them; see the scope ledger entry.
- Fighter support granted by a structure is fighter-only and is never folded into a ground-force
  capacity answer.

## Tests and commands

- `fleet.rs`: the shared arithmetic reproduces existing enforcement answers, including a Fighter II
  position where overflow is charged to the fleet pool rather than removed.
- `production.rs`: forced ship destination previews fleet and capacity aftermath and agrees with the
  applied placement; a ground-force placement choice separates space from planet and agrees; a
  component-limited batch previews the truncated count; an open destination is marked pending.
- `features.rs`: paired choices differing only in headroom, free capacity or pending state produce
  different features, survive MLP projection, and never turn an absent quantity into zero.
- Focused engine/policy tests, affected crate suites, training suite, strict all-target Clippy,
  and `git diff --check`.

## Definition of done

The model can see, for every production placement whose destination is known, exactly what fleet
supply and transport look like afterwards and how many units the box will actually supply; the
quantities agree with real placement and with the enforcement that follows; open destinations are
explicitly open; all checks and independent Tier-C review pass; only package files are committed.
