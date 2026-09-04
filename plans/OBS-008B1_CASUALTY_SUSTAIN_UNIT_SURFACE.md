# OBS-008b1 — casualty and sustain unit-identity surface

## Package

- Milestone: Stage 2 complete decision contract, first slice of `OBS-008b`
  (combat and invasion options).
- Dependencies: `OBS-003d` (typed context for `assign_casualty`/`sustain_damage`), `OBS-004`.
- Objective: give every space-combat casualty and sustain option a structured `unit` (and, for
  casualty, `damaged`) payload, and approve the resulting `casualty-unit` family for the dense MLP
  input — closing a gap where these options carry **no** structured identity at all under the
  completed prompt-free contract (`OBS-003i`): `destroy|0`/`destroy|1` and `sustain|0`/`sustain|1`
  are bare indices, and the unit type exists only in the display label the prompt-free path drops.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008b` row, static gate
  "100% of consequential `(head, kind, subtype)` rows have an approved option descriptor"),
  `plans/OBS-003I_PROMPT_FREE_MLP_PROJECTION.md`, LRR 78.4 (assign a hit) and 82 (sustain damage).
- Acceptance references: focused `obs008b1` tests in `combat.rs` and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/combat.rs`, `crates/ti4-policy/src/projection.rs`, this
  specification, package evidence, and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, ports, destructive actions, external-state changes: none.
- Generated artifacts: ordinary bounded Cargo output only.

## Behavior

1. Every casualty (`destroy|…`) option, at both sites that build them (the standalone
   `choose_casualty` and `CombatWindow::pending_choice`'s `Assigning` stage, which duplicate the
   option-building logic rather than share it), now carries `.with("unit", type_id)` and
   `.with("damaged", sustained_damage)`.
2. Every sustain (`sustain|…`) option, at both sites (`offer_sustain` and `CombatWindow::
   pending_choice`'s `Sustaining` stage), now carries `.with("unit", type_id)`. `damaged` is
   omitted: every sustain candidate is filtered to `!unit.sustained_damage` by construction, so the
   key would always be `false` — dead weight, not a fact.
3. `canonical_feature_kind` already maps both the raw `casualty` kind and the raw `sustain` kind to
   the canonical kind `"casualty"`. `structured_features`'s existing, generic
   `if let Some(unit) = payload_string(option, "unit") { add_unit_features(seen, unit,
   "casualty-unit", features) }` therefore already fires for both once the payload exists — no new
   engine or `features.rs` wiring, only the payload the pipeline was missing.
4. `casualty-unit` is added to `projection::APPROVED_UNIT_FAMILIES` (5 → 6), which resolves it to
   the shared, pre-existing `*-unit` reserved OOV column (`UNIT_SUFFIX_FAMILY`) — the same column
   every other approved unit family already shares. No `EXPLICIT_FIXED_FAMILIES` change (unit
   families are matched by suffix, not individually registered) and no vocabulary registry version
   change: the reserved column already exists at v1.

## Invariants and boundaries

- No option ID, label, or legal set changes anywhere. No `DecisionContext` field changes, so no
  V2 decision-hash movement.
- No change to casualty or sustain legality, dedup-by-(type, damage), or application.
- No preview attached in this slice: which unit absorbs a hit is a choice among interchangeable
  consequences (the hit is absorbed either way), not a quantity with a clean before/after; the
  option's own unit stats (now reachable) are the discriminating signal, not a delta.
- Approving `casualty-unit` is the architecture decision `APPROVED_UNIT_FAMILIES`'s own doc
  comment calls out as needing review; it is made here on the same terms as every family/registry
  decision this workstream has made under the standing "continue, independent review deferred"
  instruction, and is itself part of what independent review must check.

## Tests and commands

- `combat.rs`: an `obs008b1` test proving a fresh and a damaged dreadnought are distinguishable
  casualty options each naming `unit: "dreadnought"` and the correct `damaged` flag (via the
  standalone function and the windowed `Assigning` stage), and that a sustain option names
  `unit: "dreadnought"` with no `damaged` key (via the standalone function and the windowed
  `Sustaining` stage).
- `features.rs`: an `obs008b1` test proving a `casualty` option's `unit` payload reaches the policy
  as `casualty-unit:is-ship`/`casualty-unit:sustain`/etc., that a `sustain` option's `unit` payload
  reaches the *same* family (the canonical-kind unification), and that the facts survive
  `mlp_option_features`/`projection::admits` now that the family is approved.
- `cargo test -p ti4-engine --lib obs008b1`, `cargo test -p ti4-policy --lib obs008b1`,
  `cargo test -p ti4-engine --lib combat::`.
- Full engine suite (unit, integration, doc); full policy suite plus the 102-game deterministic
  campaign; training suite; strict all-target Clippy for engine and policy; targeted `cargo fmt`
  and `git diff --check`.

## Known traps

- Do not attach `damaged: false` to a sustain option; every candidate is undamaged and the key
  would carry no information.
- Do not forget the windowed `Assigning`/`Sustaining` duplicates of `choose_casualty`/
  `offer_sustain`; they build their own option lists rather than calling the standalone functions.
- Do not add `casualty-unit` to `EXPLICIT_FIXED_FAMILIES`: unit-suffix families are matched by the
  generic `*-unit` suffix rule, and adding an explicit entry would be redundant, not additive.

## Definition of done

Every casualty and sustain option names the unit type (and, for casualty, the damage state) it
concerns; the policy reads the unit's own stats under the approved `casualty-unit` family; option
identity, legal sets, and V1/V2 replay hashes are unchanged; focused and full checks pass; only
scoped files are committed.
