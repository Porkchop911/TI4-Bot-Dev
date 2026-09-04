# OBS-008a3 — load/cargo surface

## Package

- Milestone: Stage 2 complete decision contract, third slice of `OBS-008a`.
- Dependencies: `OBS-008a1`/`OBS-008a2` (the `tactical` family and `tactical_decision_features`),
  `OBS-003d` (the `load_cargo` typed context), `OBS-007a` (the preview contract).
- Objective: state, on each pickup option of a loading hold, the exact effect taking that unit
  aboard has on the hold's own remaining capacity, and read it into the policy — without changing
  any legal set, option ID, label, `DecisionContext` field, or replay behaviour.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008a` row, representation
  rule 5), `plans/OBS-007A_PREVIEW_CONTRACT.md`, LRR 95 (transport) and 16 (capacity).
- Acceptance references: focused `obs008a3` tests in `transit.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/transit.rs`, `crates/ti4-policy/src/features.rs`, this
  specification, package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `CargoWindow::pending_choice` attaches `Preview::certain` to every pickup option (`load|…`),
   carrying one `CapacityFree` delta: the hold's own remaining slots before this pickup, to one
   less. The "carry nothing further" decline option keeps no preview.
2. The delta states exactly what `resolve` charges — one slot per accepted load, the same
   bookkeeping `is_complete` already uses (`self.loaded.len()` against `self.capacity`) — rather
   than a corpus `capacityUsed` lookup that could disagree with what accepting the option actually
   does. (Every mobile unit's `capacityUsed` is 1 in the shipped corpus, so the two coincide today;
   the preview is written to match the engine's own charge, not the corpus field, so it cannot
   drift if that ever changes.)
3. `CapacityFree` is the same quantity `OBS-008a2` previews for a system's transport capacity —
   the same LRR 16 concept, here scoped to one ship's hold rather than a system's fleet. The two
   never appear on the same choice (`movement_step` vs `load_cargo`), so reusing the name is the
   shared vocabulary the family exists for, not a collision.
4. `tactical_decision_features`'s subtype guard (`OBS-008a1`) now also accepts `load_cargo`, so
   `tactical:subtype:load_cargo` and `tactical:option-count` reach the policy; its existing
   `CapacityFree` delta mapping (`OBS-008a2`) already emits `tactical:capacity-free-*` with no
   further change. No new feature family, so no vocabulary registry change.

## Invariants and boundaries

- Previews are `#[serde(skip)]`: no option ID, label, payload, legal set, replay script, or
  V1/V2 decision hash moves.
- No `DecisionContext` field is added or changed.
- No change to loading legality, `loadable`, `capacity_of`, or which units may be picked up.
- The bare test-only `CargoWindow::new` constructor is untouched; its holds preview exactly as
  every real `for_ship` hold does, since the preview reads only `capacity`/`loaded.len()`.

## Tests and commands

- `transit.rs`: an `obs008a3` test proving a fresh capacity-4 hold's pickup options all preview
  `CapacityFree` `4 -> 3`, the decline option has no preview, and after resolving one pickup the
  next choice's pickup options preview `3 -> 2` — the previewed `after` is what the next choice's
  `before` actually is.
- `features.rs`: an `obs008a3` test proving `tactical:subtype:load_cargo`,
  `tactical:option-count`, and exact `tactical:capacity-free-{before,after,change}` reach the
  policy on a pickup option, and that the decline option carries the subtype/count but no
  consequence fact.
- `cargo test -p ti4-engine --lib obs008a3`, `cargo test -p ti4-policy --lib obs008a3`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not preview from `capacityUsed`; preview from the same count `resolve`/`is_complete` charge.
- Do not attach a preview to the decline option; ending pickup has no bounded quantity here.

## Definition of done

Every pickup option states the hold's own exact capacity consequence; the policy reads it under
the existing `tactical` family; the decline option is untouched; old choices and V1/V2 replay
hashes are unchanged; no vocabulary change; focused and full checks pass; only scoped files are
committed.
