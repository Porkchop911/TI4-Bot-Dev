# OBS-008b6 — ground casualty, coexisting-combat identity, and custodians surface

## Package

Sixth and closing slice of `OBS-008b`, batched: three producers, one commit, one verification
pass (per the owner's "stop slicing this fine" direction).

- Dependencies: `OBS-008b1` (`aab65bc`, `casualty-unit`), `OBS-008b5` (`39a7c05`, the
  opponent-identity leak fix and `combat:target-slot-*`), `OBS-008c1` (the `pay` family).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008b` row), LRR 42
  (ground combat), Coexistence 12, LRR 27.2/27.3 (custodians).
- Writable paths: `crates/ti4-engine/src/invasion.rs`, `crates/ti4-policy/src/features.rs`, this
  spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **Ground casualty** (`absorb_ground`, Rule 42): each `destroy|{index}` option gains
   `unit`/`damaged` payload, the same fix `OBS-008b1` made for space combat. `GROUND_CASUALTY_KIND`
   already canonicalizes to `"casualty"`, so this reaches the already-approved `casualty-unit`
   family with no further wiring.
2. **Coexisting-combat identity** (`start_next_ground_combat`, Coexistence 12): the "fight this
   player next" option's id (`fight|{seat}`) is the same raw-identity shape `OBS-008b5` fixed for
   bombardment. `explicit_option_features_with`'s `dropped`-token computation gains a subtype-keyed
   case: for this subtype, the whole argument half of the `verb|argument` id is dropped (not
   filtered to planet ids), and `opponent_identity_features` (renamed from
   `bombardment_target_features`, now handling both producers) emits `combat:target-slot-{index}`.
3. **Custodians removal** (`remove_custodians`, 27.2/27.3): the "yes" option previews the exact
   payment (`deterministic::spend`, reused from `OBS-007b`) plus a capped `VictoryPoints` delta;
   "no" previews no change. `payment_decision_features` is admitted by subtype (`remove_custodians`)
   alongside its existing kind check, since these options keep the `decline`/`custodians` kinds
   rather than `pay`; a `VictoryPoints -> "victory-points"` arm joins its delta match.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes. No vocabulary
  registry change — all three reuse existing families (`casualty-unit`, `combat`, `pay`).
- The custodians preview is fail-closed: if `deterministic::spend` cannot resolve `Certain`
  (unreachable given `custodians_removable`'s own affordability check, but not assumed), the
  victory-point claim is not appended and the non-informative outcome passes through unchanged.
- `fight_ground_combat_round`'s single, forced "fight" option is left as is: one option with
  nothing to discriminate is not worth a fact.

## Tests and commands

One batched test per crate, each covering all three producers in one fixture pass rather than
three separate tests:

- `invasion.rs::obs008b6_ground_casualty_and_custodians_previews`: a ground casualty option names
  its unit; `fight|b` still carries the raw seat id for engine routing; removing custodians
  previews `VictoryPoints 0 -> 1`, declining previews `0 -> 0`.
- `features.rs::obs008b6_ground_casualty_next_combat_and_custodians_features`: the ground
  casualty's payload reaches `casualty-unit:is-ground`; the coexisting-combat option emits
  `combat:target-slot-0` and never `option:b`; the custodians options read under `pay:subtype:
  remove_custodians` with exact `pay:victory-points-*` (a genuine-zero decline is a dropped sparse
  entry).
- Full engine + policy (incl. the 102-game campaign) + training suites; strict Clippy; `cargo fmt`;
  `git diff --check`.

## Definition of done

All three producers' options carry the facts described above; no vocabulary change; option
identity and V1/V2 replay hashes unchanged; full checks pass; only scoped files committed. Closes
the `OBS-008b` row.
