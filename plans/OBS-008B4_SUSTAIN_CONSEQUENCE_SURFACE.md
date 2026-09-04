# OBS-008b4 — sustain-damage consequence surface

## Package

- Milestone: Stage 2 complete decision contract, fourth slice of `OBS-008b`.
- Dependencies: `OBS-008b1` (`aab65bc`, the sustain option's `unit` identity), `OBS-008b3`
  (`93839d9`/`7315f97`, the `combat` family's `ShipsInSystem` reading).
- Objective: give sustain-damage its own consequence — OBS-008b1 fixed only the option's unit
  identity, not what accepting or declining it does — using the same `ShipsInSystem` reading the
  retreat surface already established.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008b` row), LRR 82
  (sustain damage).
- Acceptance references: focused `obs008b4` tests in `combat.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/combat.rs`, `crates/ti4-policy/src/features.rs`, this
  specification, package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `offer_sustain` and the windowed `Stage::Sustaining` (its long-standing duplicate) each attach
   `ShipsInSystem` previews: sustaining previews no change (the ship survives, damaged); declining
   previews the seat's own ship count in the system falling by exactly one (the ship is
   destroyed). Both read the same before count once per ask, since it is shared by every option
   offered in that ask.
2. `combat_decision_features`'s subtype guard (`OBS-008b2`/`b3`) now also accepts
   `sustain_damage`; the existing `ShipsInSystem -> "ships"` mapping needed no change. No new
   feature family, no vocabulary change.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes, so no V2
  decision-hash movement.
- No change to sustain legality, `sustainers`, or application.
- The preview claims only the ship's own fate, never a downstream combat-round consequence.

## Tests and commands

- `combat.rs`: an `obs008b4` test proving, via both the standalone `offer_sustain` and the
  windowed `Sustaining` stage, that sustaining previews `ShipsInSystem` unchanged and declining
  previews it falling by exactly one.
- `features.rs`: an `obs008b4` test proving `combat:subtype:sustain_damage`,
  `combat:option-count`, and exact `combat:ships-{before,after,change}` reach the policy for both
  options, with sustaining's genuine-zero change dropped as a sparse entry; the arrival fact
  survives the MLP projection.
- `cargo test -p ti4-engine --lib obs008b4`, `cargo test -p ti4-policy --lib obs008b4`,
  `cargo test -p ti4-engine --lib combat::`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not compute the before count once per option; it is the same for every option in one ask.
- Do not claim anything about the combat round that follows; only the ship's immediate fate.

## Definition of done

Both sustain-damage options state their exact fleet consequence; the policy reads it under the
existing `combat` family; option identity and V1/V2 replay hashes are unchanged; focused and full
checks pass; only scoped files are committed.
