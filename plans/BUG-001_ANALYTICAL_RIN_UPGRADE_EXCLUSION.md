# BUG-001 — Analytical and Rin exclude every unit upgrade, generic included

## ID and title
BUG-001, "Analytical and Rin exclude every unit upgrade, generic included"

## Milestone and dependencies
Operator-requested bug fix, outside the M00–M13 milestone table. Depends on nothing; found while
reviewing the `gamesolfaultyproduction929` capture (checkpoint-132144) on 2026-09-12.

## One-sentence objective
Make Jol-Nar's Analytical waive zero prerequisites for **any** unit upgrade technology, and make
Rin, the Master's Legacy leave held unit upgrades (generic and faction-specific) untouched, by
deriving "is a unit upgrade" from the canonical `types ∋ UNITUPGRADE` check instead of the
`baseUpgrade` field.

## Exact normative rule/specification/source references
- `crates/ti4-content/content/abilities.json` `analytical`: window "When you research a technology
  that is not a unit upgrade technology" → "You may ignore 1 prerequisite." (matches printed
  4th-edition text, confirmed against twilight-imperium.fandom.com and scottmk.github.io
  references during the 2026-09-12 review).
- `crates/ti4-content/content/leaders.json` `jolnarhero` (Rin, the Master's Legacy): "For each
  **non-unit upgrade** technology you own, you may replace that technology with any technology of
  the same color from the deck."
- LRR 90.7b: a unit upgrade has no colour and carries its subject's prerequisites; the canonical
  class in this engine is `technology::is_unit_upgrade` (`crates/ti4-engine/src/technology.rs`,
  `types ∋ "UNITUPGRADE"`), already used for AI Development Algorithm and war-sun gating.
- Corpus: `technologies.json` has 25 UNITUPGRADE records — 16 faction-specific (carry
  `baseUpgrade`) and 10 generic (`ws`, `sd2`, `cr2`, `dn2`, `dd2`, `pds2`, `cv2`, `ff2`,
  `inf2`, `m2`) with **no** `baseUpgrade`.

## The defect
`faction_abilities::waived_prerequisites` re-derives "is upgrade" as
`record.text("baseUpgrade") non-empty`, so the waiver fires for all 10 generic upgrades: a
Jol-Nar with one blue researches Carrier II (needs BB), with three of four prereqs researches the
War Sun, etc. `research()` re-validates through the same broken gate, so these are generated as
legal rather than rejected late. Same root cause in `leaders.rs` `jolnarhero`: a held generic
upgrade is treated as swappable, and its phantom colour string `"UNITUPGRADE"` is matched against
*other* generic upgrades' colour, so the swap pool contains other upgrades. Two regression tests
share the broken `baseUpgrade` selector and are vacuous for the generic case
(`analytical_waives_a_prerequisite_but_not_for_a_unit_upgrade`,
`rin_swaps_a_technology_for_another_of_the_same_colour`) — the same failure mode the
`waived_prerequisites` doc comment already records for a prior `unitUpgrade` key guess.

## Exact acceptance-test references
- New: `analytical_waives_nothing_for_any_unit_upgrade_in_the_corpus` (all 25 UNITUPGRADE
  aliases get 0; an ordinary technology still gets 1).
- New: `a_jolnar_with_one_blue_cannot_research_carrier_ii` (end-to-end `can_research(cv2)`).
- New: `rin_leaves_held_unit_upgrades_untouched` (held `cv2` survives the swap; hero not purged
  when nothing is swappable).
- Strengthened selectors: the two pre-existing tests must choose their fixture technologies with
  the canonical upgrade check, so "ordinary" can never be a unit upgrade.

## Allowed Rust edit paths
- `crates/ti4-engine/src/faction_abilities.rs`
- `crates/ti4-engine/src/leaders.rs`
- `plans/evidence/BUG-001_ANALYTICAL_RIN_UPGRADE_EXCLUSION.md` (new)
- `plans/EXECUTION_STATE.md` (status row)

## Permission class and scoped access declaration
Class: local, offline, no network, no process, no external-state effects. Read: the engine crate
and content corpus. Write: the four paths above. No Python reference inspected.

## Inputs and outputs
Inputs: the 2026-09-12 diagnosis (repro transcript in the evidence file). Outputs: the two
predicate fixes, the strengthened tests, evidence, state row.

## Invariants and compatibility class
- Legal actions stay generated, not late-rejected: the offered research set shrinks; nothing that
  was correctly legal is removed.
- **Compatibility consequence, recorded**: sessions learned before this fix (including the
  checkpoint-132144 review) saw spurious research options for Jol-Nar; the policy's learned
  priors over those option ids remain in weights it no longer sees offered. No schema, choice-ID,
  or feature change; no vocabulary migration.
- Determinism: `is_unit_upgrade` is a pure content lookup; no iteration-order dependence
  introduced.

## Explicit non-goals
- No retraining, no policy-side change, no replay migration of existing captures.
- No other `baseUpgrade` call sites are touched: `action_cards.rs` (upgrade targeting) and
  `leaders.rs`'s Mech/infantry line checks use the field for a different semantic (the unit a
  faction upgrade replaces), which stays correct.

## Tests to add
The four listed in the acceptance section.

## Commands to run
```
cargo test -p ti4-engine --lib faction_abilities
cargo test -p ti4-engine --lib leaders
cargo test -p ti4-engine --lib technology
cargo test -p ti4-engine          # full crate: lib + integration + docs
cargo test -p ti4-policy
cargo clippy -p ti4-engine --all-targets -- -D warnings
rustfmt --check crates/ti4-engine/src/faction_abilities.rs crates/ti4-engine/src/leaders.rs
```

## Expected evidence
`plans/evidence/BUG-001_ANALYTICAL_RIN_UPGRADE_EXCLUSION.md`: repro output (waiver table over all
25 upgrades, `can_research(cv2)` with one blue), the two code diffs, full test results, and the
compatibility note.

## Known traps
- Both pre-existing tests' fixture selectors pick "ordinary" as *any* record without
  `baseUpgrade` — in POK scope the first such record can be a generic upgrade, so the fix breaks
  them until the selectors use the canonical check. This is expected and is part of the fix.
- The `UNITUPGRADE` string in `types.first()` was Rin's only "colour" signal for upgrades; with
  upgrades filtered out the pool filter can stay as-is, but the replacement must not be an
  upgrade either — it is excluded by the existing `baseUpgrade`-empty filter **only for** the
  faction-specific ones, so the pool filter must also switch to the canonical check.
- `wiring.rs`'s decision-delivery inventory names `waived_prerequisites` as a string; no
  signature change, so no inventory edit.

## Definition of done
Both predicates use `technology::is_unit_upgrade`; the four tests pass; the two vacuous selectors
are strengthened; full engine/policy suites green; clippy and fmt clean on the two files; evidence
written; committed on its own branch; independent (frontier-tier, legality) review recorded or
flagged outstanding; EXECUTION_STATE row added.