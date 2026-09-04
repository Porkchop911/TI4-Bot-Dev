# OBS-008b3 — retreat fleet-arrival surface

## Package

- Milestone: Stage 2 complete decision contract, third slice of `OBS-008b`.
- Dependencies: `OBS-008b2` (`4b9c383`, the `combat` family), `OBS-003d` (the
  `announce_retreat`/`retreat_to` typed contexts).
- Objective: give the retreat-announcement and retreat-destination options the exact fleet
  before/after fact 78.7's "retreat the whole remaining fleet together" rule produces, and the
  board-fact payload (`system`) they previously carried none of at all.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008b` row), LRR 78.7
  (retreat) and 78.9 (announcing).
- Acceptance references: focused `obs008b3` tests in `combat.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/combat.rs`, `crates/ti4-policy/src/features.rs`, this
  specification, package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `CombatWindow::pending_choice`'s `Announcing` stage: "stay" and "retreat" (or the forced
   "retreat" alone) each gain a `system` payload (the combat system) and a `ShipsInSystem`
   preview. 78.7 retreats the *whole* remaining fleet together, never selectively, so "retreat"
   previews the asking seat's own ship count in the combat system falling to zero; "stay" previews
   no change.
2. The `Retreating` stage: each destination option gains a `system` payload (that destination) and
   a `ShipsInSystem` preview — the seat's own ship count already there, to that count plus the
   whole retreating fleet's size (read once per choice via `ships_of`, not per option).
3. Attaching `system` also reaches the pre-existing, already-approved generic board-fact pipeline
   (`structured_features`'s `option-system:*` family for any non-produce/placement/load kind
   carrying a `system` payload) — a fact these three option kinds previously exposed nothing of at
   all, since they carried no payload whatsoever before this package.
4. `combat_decision_features`'s subtype guard (`OBS-008b2`) now also accepts `announce_retreat`
   and `retreat_to`; its `Certain`-outcome delta match gains `ShipsInSystem -> "ships"`, reusing
   the same `combat:ships-{before,after,change}` names for both the departure and the arrival
   fact. No new feature family, no vocabulary change.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes, so no V2
  decision-hash movement.
- No change to retreat legality, `eligible_retreats`/`det_retreat_destinations`, or application
  (`retreat_to` the apply function, untouched).
- The preview claims only where ships end up, never a downstream combat or control consequence at
  the new system — the same boundary every prior OBS-008 slice has drawn.
- Both fleet-size reads (`ships_of` for the departing count, and per-destination for what is
  already there) are the same helper `Stage::Sustaining`/casualty code already depends on.

## Tests and commands

- `combat.rs`: an `obs008b3` test proving, on a hub fixture with two eligible retreat
  destinations already holding different fleet counts, that "stay" previews no change, "retreat"
  previews the fleet's own count falling to zero, and each destination previews its own existing
  count rising by the whole retreating fleet's size — all four options now carrying `system`.
- `features.rs`: an `obs008b3` test proving `combat:subtype:announce_retreat`/`retreat_to`,
  `combat:option-count`, and exact `combat:ships-{before,after,change}` reach the policy for both
  the announcement and the destination choice, with the genuine-zero cases (arrival at zero,
  staying unchanged) dropped as sparse entries; the arrival fact survives the MLP projection.
- `cargo test -p ti4-engine --lib obs008b3`, `cargo test -p ti4-policy --lib obs008b3`,
  `cargo test -p ti4-engine --lib combat::`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not preview a per-unit or partial retreat; 78.7 moves the whole remaining fleet as one.
- Do not recompute the departing fleet's size once per destination option; read it once per choice.
- Do not claim anything about the destination's later safety or combat; only the arrival count.

## Definition of done

Every retreat-announcement and retreat-destination option states its exact fleet
before/after and carries board context it previously had none of; the policy reads both under the
existing `combat` family; option identity and V1/V2 replay hashes are unchanged; focused and full
checks pass; only scoped files are committed.
