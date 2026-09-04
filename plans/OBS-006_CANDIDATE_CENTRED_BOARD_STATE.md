# OBS-006 — candidate-centred board state

## Package

- Milestone: Stage 2 complete decision contract.
- Dependencies: `OBS-002b`, `OBS-005`.
- Objective: close the two gaps representation rule 4 and the OBS-006 row name that the existing
  per-target board features (`features.rs::add_system_features`) do not yet cover: relationship-
  relative presence (rule 4's "relevant relationships", now buildable from OBS-005's slots) and
  objective-location facts (the row's own phrase; the engine's counterfactual objective-progress
  helper is wired to nothing).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` representation rule 4 and the
  OBS-006 row; `plans/OBS-005_RELATIONAL_PUBLIC_TABLE_STATE.md` (the slot machinery this reuses).

## What already existed, and what did not

`add_system_features` (pre-existing, not new to this package) already computes, per board-targeting
option: planet/control counts, own/enemy ship and ground-force counts, production-unit count,
resources/influence, wormholes/anomaly, distance to the nearest own-unit system and to Mecatol,
rival-seat count, reach-versus-token-budget, and reachability — most of "topology, composition,
reach... capacity, production" from the row already has a home. Two things did not:

1. **Relevant relationships.** Enemy presence in the target is an aggregate (`rival-seats`,
   `enemy-ships`), never broken down by which *kind* of opponent (combat counterpart, Support
   partner, neighbor, or unrelated) is actually there. OBS-005 built exactly the machinery this
   needed (`Observed::opponent_slots`) and this package was the reason it stayed deliberately
   player-ID-free.
2. **Objective-location.** `Observed::revealed_objective_progress_gaining(player, gained)` — the
   engine's own "what if I also controlled these planets" counterfactual — exists and is called
   from nowhere in `features.rs`. A board-targeting option that would take a planet carries no
   signal at all about whether taking it moves a revealed objective.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-policy/src/features.rs`, this specification, package evidence,
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only. No new engine code: both additions are
  pure `features.rs` wiring over engine capability that already exists (`opponent_slots` from
  OBS-005, `revealed_objective_progress_gaining` from the pre-existing observation surface).

## Behavior

Both additions land in `add_system_features`, so every board-targeting option kind that already
calls it (activation targets, production/placement/load systems) gets them for free without a
per-kind change:

1. **`{prefix}:present-slot-{0..4}`** — 1.0 iff that opponent slot's seat (from
   `seen.opponent_slots(player)`) holds any unit (space or ground) or planet control in the target
   system. The slot index is OBS-005's sort position, not a player id, so "the combat counterpart is
   here" reads the same way in every game.
2. **`{prefix}:objective-progress-gain`** — sum, over every revealed objective, of
   `max(0, clip(gaining_ratio) - clip(current_ratio))`, where `gaining` is
   `revealed_objective_progress_gaining(player, uncontrolled_planets_in_this_system)` and `current`
   is `revealed_objective_progress(player)`, clipped to `[0, 1]` exactly as `objective_facts`
   already clips (same convention, not a new one).
3. **`{prefix}:objective-newly-satisfied`** — count of revealed objectives that are unsatisfied now
   and satisfied in the gaining counterfactual. A stronger, more directly actionable signal than the
   ratio delta alone.

Only planet control is imagined for (2)/(3), the same restriction
`revealed_objective_progress_gaining`'s own doc already states: requirements about units,
technologies, or map shape read the real state on both sides and cancel out of the difference, so a
target that cannot affect a requirement never gets credited for it.

## Boundaries

- No new player, planet, or system identity in a feature name: slot indices are OBS-005 sort
  positions, and the objective facts are bounded sums/counts, not per-alias detail (the alias-level
  detail already exists in the `objective-progress:*` family and is not duplicated here).
- No new engine surface. Both facts are computed from capability that already existed before this
  package; nothing in `choice.rs` changes.
- No option ID, label, legal set, state transition, prompt, or replay contract changed.

## Tests and commands

- `obs006_present_slot_facts_are_relationship_relative_not_identity_relative`: mirrors OBS-005's own
  features test shape — the same opponent presence in a target system, put on a different seat,
  emits the same `target:present-slot-*` facts.
- `obs006_objective_progress_gain_reflects_the_counterfactual_not_the_current_position`: a target
  system holding an uncontrolled planet that would satisfy a revealed objective emits a positive
  `target:objective-progress-gain` and `target:objective-newly-satisfied` even though the *current*
  (ungained) position does not satisfy it; a target that cannot affect any revealed objective emits
  neither.
- Full engine suite (unchanged; sanity only), ordinary policy suite (deterministic campaign
  included), training suite, strict Clippy on `ti4-policy`, `cargo fmt --check`, `git diff --check`.

## Definition of done

`add_system_features` reports relationship-relative opponent presence and objective-location
counterfactual facts for every board-targeting option kind that already reaches it; every value is
bounded and identity-free; every existing suite passes unmodified in behavior; independent Tier-C
review OUTSTANDING per current instruction to continue without waiting on it.
