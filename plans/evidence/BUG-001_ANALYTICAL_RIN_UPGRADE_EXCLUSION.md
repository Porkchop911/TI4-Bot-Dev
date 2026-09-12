# BUG-001 — Analytical and Rin exclude every unit upgrade, generic included

- Date: 2026-09-12
- Branch: `wp/bug-001-analytical-rin-upgrade-exclusion` (from `main` @ `22266e1e`)
- Implementer: Qwen 3.8 27B via pi v0.84.2
- Operator-requested package (outside the M00–M13 table), following the rule-verification
  pass requested while reviewing the `gamesolfaultyproduction929` capture (checkpoint-132144).
  The defect below was found by code + corpus audit and a scratch repro, not from that
  capture.

## Defect

Jol-Nar's **Analytical** (`abilities.json`: "When you research a technology that is **not a unit
upgrade technology**, you may ignore 1 prerequisite") and Rin, the Master's Legacy
(`leaders.json` `jolnarhero`: "For each **non-unit upgrade** technology you own, you may
replace that technology with any technology of the same colour") both derived "is a unit
upgrade" from the `baseUpgrade` content key.

The corpus has **25 UNITUPGRADE technologies** (`crates/ti4-content/content/technologies.json`):
16 faction-specific ones that carry `baseUpgrade` (e.g. `ac2`→`cv2`, `so2`→`inf2`,
`helios2`→`sd2`, `linkship2`) and **10 generic ones that carry no `baseUpgrade` at all**:
`ws, sd2, cr2, dn2, dd2, pds2, cv2, ff2, inf2, m2` (Carrier II, Ship Design II, …, War Sun).
Their only type is `UNITUPGRADE` (LRR 90.7b: upgrades have no colour and inherit their
subject's prerequisites).

Consequences of the broken derivation:

- `faction_abilities::waived_prerequisites` returned **1** for all ten generic upgrades, so
  `technology::can_research`/`research` — which re-validate through the same gate — let a
  Jol-Nar with one blue research **Carrier II** (needs two blues, `cv2`), and e.g. with three
  of four prerequisites research the **War Sun**. The options were generated into the policy's
  offered set, not rejected late.
- `leaders.rs` `jolnarhero` treated a held generic upgrade as swappable; its "colour" was the
  phantom string `"UNITUPGRADE"` (`types.first()`), so the replacement pool matched the other
  generic upgrades and Rin swapped one upgrade for another.

The two pre-existing regression tests were vacuous for the generic shape: they selected their
"upgrade" fixture by `baseUpgrade` non-empty, which is exactly the shape that worked. This is
the same failure mode the `waived_prerequisites` doc comment already records for a prior
`unitUpgrade` key guess.

## Scratch repro (pre-fix, deleted)

`crates/ti4-engine/tests/scratch_analytical_upgrades.rs`, run against `main` @ `22266e1e`
before the fix:

- `waived_prerequisites` for a seated Jol-Nar: generic upgrades `ws, sd2, cr2, dn2, dd2,
  pds2, cv2, ff2, inf2, m2` → **1** each; faction-specific upgrades (`ac2, so2, sdn2, lw2,
  pws2, …`) → 0.
- Jol-Nar holding exactly one blue (`gd`, Gravity Drive): `can_research(cv2)` → **true**;
  `cv2` present in the `researchable` list (its requirements are `BB`).
- Rin (`use_leader` with `jolnarhero` Unlocked) holding only `cv2`: the upgrade was replaced
  by another generic upgrade and the hero purged.

## Fix (2 code sites, canonical predicate)

Both sites now use the engine's canonical class test, `technology::is_unit_upgrade`
(`technology.rs:798`, `types ∋ "UNITUPGRADE"`), already used by the AI Development Algorithm
and war-sun gating:

1. `crates/ti4-engine/src/faction_abilities.rs` `waived_prerequisites`: `is_upgrade` is now
   `crate::technology::is_unit_upgrade(content, &TechnologyId::new(technology))`.
2. `crates/ti4-engine/src/leaders.rs` `use_leader` → `jolnarhero`: held-tech eligibility uses
   `crate::technology::is_unit_upgrade(context.content, &alias)`; the replacement pool keeps
   the same-type filter (now safe, since an upgrade's "colour" can no longer be the phantom
   `UNITUPGRADE`) plus an explicit record-level `!types.contains("UNITUPGRADE")` guard.

Not touched: `action_cards.rs` `baseUpgrade` usages (upgrade *targeting* — different semantic),
the Mech/infantry line checks, and `brilliant` (separate ability, already correct).

## Tests

New:

- `faction_abilities::tests::analytical_waives_nothing_for_any_unit_upgrade_in_the_corpus`
  — every one of the 25 UNITUPGRADE aliases (FULL scope) gets waiver **0**; asserts the
  corpus has ≥20 upgrades incl. ≥5 generic; ordinary `gd` still gets 1.
- `faction_abilities::tests::a_jolnar_with_one_blue_cannot_research_carrier_ii`
  — end to end: `can_research(cv2)` false and `cv2` absent from `researchable`.
- `leaders::tests::rin_leaves_held_unit_upgrades_untouched` — held `cv2` survives; nothing
  swappable, so the hero is not spent and not purged.

Strengthened (fixture selectors now use the canonical class test, so "ordinary" can never be
a unit upgrade and "upgrade" covers both shapes):

- `faction_abilities::tests::analytical_waives_a_prerequisite_but_not_for_a_unit_upgrade`
- `leaders::tests::rin_swaps_a_technology_for_another_of_the_same_colour`
  (verified both still select `amd`, Antimass Deflectors — unchanged pick, non-empty pool)

## Commands and exact results (post-fix, post-fmt)

```
cargo test -p ti4-engine --lib   → ok. 1276 passed; 0 failed
cargo test -p ti4-engine         → ok. 1276 + 1 + 4 + 5 passed; 0 failed (lib + 3 integration binaries)
cargo test -p ti4-policy         → ok. 244 passed; 0 failed
cargo clippy -p ti4-engine --all-targets → no new warnings from this package
                                      (remaining: 1 pre-existing `ti4-model` bool-struct warning)
rustfmt --edition 2024 --check (the two changed files) → clean
```

Red phase (pre-fix, same commands): `analytical_waives_nothing_for_any_unit_upgrade_in_the_corpus`
failed on `ws` (waiver 1 vs 0), `analytical_waives_a_prerequisite_but_not_for_a_unit_upgrade`
failed on the strengthened generic-upgrade fixture, `a_jolnar_with_one_blue_cannot_research_carrier_ii`
failed at the `can_research` assertion, `rin_leaves_held_unit_upgrades_untouched` failed (upgrade
swapped, hero purged). All four green after the fix.

## Compatibility consequences (recorded, no action)

- The offered research set for Jol-Nar **shrinks**: generic-upgrade research that was
  spuriously legal is no longer generated. This affects any policy trained on pre-fix
  checkpoints (including checkpoint-132144 reviewed on 2026-09-12), whose weights saw the
  spurious option ids. No schema, choice-ID, or feature-vector change; no replay migration.
- Determinism: the predicate is a pure content lookup; no new iteration-order or scheduling
  dependence.

## Sources

- `crates/ti4-content/content/abilities.json` (`analytical`), `leaders.json` (`jolnarhero`),
  `technologies.json` (UNITUPGRADE class, 25 records) — embedded corpus at engine commit
  `22266e1e`.
- Printed 4th-edition ability/leader text and LRR 90.7b (verified against reference pages
  during the 2026-09-12 audit; ti4rules.github.io FAQ did not carry the entry, ti4lookup /
  fandom references were used).
- Historical Python reference: **not inspected** for this package.

## Review

Independent (frontier-tier, legality) review: **OUTSTANDING** — no review peer was available
in this session. The implementing agent self-verified only; the two code sites are small,
localized predicate swaps, but the package is not merge-complete until the review lands.