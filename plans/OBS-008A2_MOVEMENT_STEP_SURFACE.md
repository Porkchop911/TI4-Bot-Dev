# OBS-008a2 — movement-step surface

## Package

- Milestone: Stage 2 complete decision contract, second slice of `OBS-008a`.
- Dependencies: `OBS-008a1` (`a53ae23`, the `tactical` family and `tactical_decision_features`),
  `OBS-003d` (the `movement_step` typed context), `OBS-006`, `OBS-007b` (`fleet::standing_using`
  is the shared deterministic limit arithmetic).
- Objective: state, on each ship-move option, the exact fleet-supply and transport change the ship
  makes on arrival in the active system, and read it into the policy — without changing any legal
  set, option ID, label, `DecisionContext` field, or replay behaviour.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008a` row, representation
  rule 5), `plans/OBS-007B_DETERMINISTIC_PREVIEW.md`, LRR 37 (fleet supply) and 16 (capacity).
- Acceptance references: focused `obs008a2` tests in `tactical.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/tactical.rs`, `crates/ti4-engine/src/game.rs`,
  `crates/ti4-policy/src/features.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `tactical::preview_moves` attaches `Preview::certain` to every ship-move option (`move|…` and
   `move_gd|…`) of the movement-step choice, carrying two deltas for the active (destination)
   system: `FleetSupplyHeadroom` and `CapacityFree`, from `fleet::standing_using` with and without
   a single `Arrival { count: 1, in_space: true }` of the option's unit type. `game.rs`'s
   `tactical_choice` Moving arm calls it once the choice is built.
2. The "finish movement" decline option keeps no preview: ending the step has no bounded quantity.
3. `preview_moves` shares `fleet::standing_using` with production placement and end-of-turn
   enforcement, so the arrival preview and the removal that may later follow it cannot drift apart.
4. `tactical_decision_features` (from `OBS-008a1`) reads the two new preview quantities into the
   existing `tactical` family: `tactical:fleet-headroom-{before,after,change}` and
   `tactical:capacity-free-{before,after,change}`, under the `tactical:preview-known` marker.
   No new feature family, so no vocabulary registry change.

## Invariants and boundaries

- Previews are `#[serde(skip)]`: no option ID, label, payload, legal set, replay script, or V1/V2
  decision hash moves.
- No `DecisionContext` field is added or changed. `movement_step` is still not marked `optional`
  and carries no `target` — those are a later, separately reviewed context touch, deferred so this
  slice cannot move a V2 decision hash.
- Destination-only: the origin losing a hull is not previewed. A move spends no resource, influence,
  trade-good, or command-token pool, so none is previewed.
- No change to movement legality, `movable`, reachability, or Gravity Drive handling.

## Tests and commands

- `tactical.rs`: an `obs008a2` test proving a carrier-move option previews `CapacityFree` rising by
  the carrier's printed capacity and `FleetSupplyHeadroom` falling by one, a fighter-move option
  previews `FleetSupplyHeadroom` unchanged, the decline option has no preview, and the previewed
  `after` values equal `fleet::standing` recomputed once the ship is actually in the active
  system — two independent computations.
- `features.rs`: an `obs008a2` test proving `tactical:fleet-headroom-*` and
  `tactical:capacity-free-*` reach the policy with the expected values on a move option, that the
  `movement_step` subtype and option count still land on the preview-less decline option, and that
  the facts survive the MLP projection.
- `cargo test -p ti4-engine --lib obs008a2`, `cargo test -p ti4-policy --lib obs008a2`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not preview the origin system; the arrival is the constraint.
- Do not mark the `movement_step` context `optional` here — that is a V2-hash change for a later
  slice.
- `fleet::standing_using` takes an already-built catalogue; build it once per choice, not per
  option.

## Definition of done

Every ship-move option states its exact fleet and capacity consequence in the active system; the
policy reads both; the decline option is untouched; old choices and V1/V2 replay hashes are
unchanged; no vocabulary change; focused and full checks pass; only scoped files are committed.
