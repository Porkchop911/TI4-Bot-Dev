# OBS-008a4 — ground-commitment surface

## Package

- Milestone: Stage 2 complete decision contract, fourth and closing slice of `OBS-008a`.
- Dependencies: `OBS-008a1`/`a2`/`a3` (the `tactical` family and `tactical_decision_features`),
  `OBS-003d` (the `commit_ground_forces` typed context), `OBS-007a` (the preview contract).
- Objective: state, on each landing option of the ground-commitment ask, the exact immediate
  effect landing that unit has on the invader's own ground-force presence on the target planet,
  and read it into the policy — without changing any legal set, option ID, label,
  `DecisionContext` field, or replay behaviour.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008a` row — "tactical
  commitments" — and representation rule 5), `plans/OBS-007A_PREVIEW_CONTRACT.md`, LRR 49.2.
- Acceptance references: focused `obs008a4` tests in `invasion.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/invasion.rs`, `crates/ti4-policy/src/features.rs`, this
  specification, package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. `invasion::commit_options` (shared by `commit_ground_forces` and
   `InvasionWindow::committing_choice`) attaches `Preview::certain` to every landing option
   (`commit|…`), carrying one `GroundForcesOnPlanet` delta: the invader's own ground-force count
   already on that planet, to one more. The "commit no more" decline option keeps no preview.
2. The preview states only what LRR 49.2 itself does — placing the unit on the planet — and claims
   nothing about the ground combat, casualties, or control transfer that may follow: those are a
   later resolution and a player choice, the same boundary `OBS-008c2b` already drew for
   production placement's fleet/capacity facts.
3. The count is the invader's own only; an opposing seat's ground forces already on the planet do
   not enter the before value, matching representation rule 5's "actor-owned" framing.
4. `tactical_decision_features`'s subtype guard (`OBS-008a1`–`a3`) now also accepts
   `commit_ground_forces`, so `tactical:subtype:commit_ground_forces` and `tactical:option-count`
   reach the policy; a new `GroundForcesOnPlanet -> tactical:ground-forces-*` mapping is added to
   the existing delta match. No new feature family, so no vocabulary registry change.

## Invariants and boundaries

- Previews are `#[serde(skip)]`: no option ID, label, payload, legal set, replay script, or
  V1/V2 decision hash moves.
- No `DecisionContext` field is added or changed.
- No change to landing legality, `landable`, `landable_planets`, or which units may land.
- `commit_options` gained three parameters (`state`, `invader`, `system`) to compute the count;
  both call sites are updated and the board is read once per choice via `state.system_state`
  (a clone), not once per option.

## Tests and commands

- `invasion.rs`: an `obs008a4` test proving every landing option on a two-planet arena, with an
  opposing seat's ground force already on one planet, previews `GroundForcesOnPlanet` `0 -> 1` for
  the invader (the opponent's presence does not enter the invader's own count); the decline option
  has no preview; and after one unit actually lands, the next choice's option for that same planet
  previews `1 -> 2` — the first preview's `after` is what the next choice's `before` actually is.
- `features.rs`: an `obs008a4` test proving `tactical:subtype:commit_ground_forces`,
  `tactical:option-count`, and exact `tactical:ground-forces-{after,change}` reach the policy on a
  landing option (with a genuine-zero `before` a dropped sparse entry), that the decline option
  carries the subtype/count but no consequence fact, and that the facts survive the MLP projection.
- `cargo test -p ti4-engine --lib obs008a4`, `cargo test -p ti4-policy --lib obs008a4`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not count the defending seat's ground forces in the invader's own before value.
- Do not preview an eventual control transfer or ground-combat outcome; only the immediate landing.
- Do not attach a preview to the decline option.

## Definition of done

Every landing option states its exact immediate ground-presence consequence for the invader; the
policy reads it under the existing `tactical` family; the decline option is untouched; old choices
and V1/V2 replay hashes are unchanged; no vocabulary change; focused and full checks pass; only
scoped files are committed. Closes the `OBS-008a` row (`a1`-`a4`).
