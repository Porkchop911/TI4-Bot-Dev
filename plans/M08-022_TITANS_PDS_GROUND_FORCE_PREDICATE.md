# M08-022 — Ground-force predicate vs corpus flag (M08-020 review T1)

## Preparation status

**Accepted 2026-08-23.** Branch `wp/m08-022-titans-pds-ground-force-predicate` from `476e0c4`
(M08-021 close-out — the accepted line tip; deliberately independent of the pending M08-019
resolution commit so both packages review and merge independently). Dependencies met: M08-020
accepted (`00d6562`). Independent Codex frontier review found no actionable issue; D11 roster
widening is no longer blocked by this package.

**Dependency-safe specification only.** Scoped by the independent Tier-C frontier review of
M08-020, finding **T1** (`plans/M08-020_OPEN_REVIEW_ITEMS.md`). Begin any time after M08-020 is
accepted; **hard-ordered before any D11 roster widening** (the moment Titans can be seated, this
defect fires). It does **not** block M08-018 or M08-021: under the current six-faction roster
(sol, letnev, xxcha, hacan, jolnal, l1z1x) no affected unit can exist on a board, so the defect is
structurally dormant — the same class as M06-025's L1.

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots (child of M08-020 review) |
| Depends | accepted M08-020 |
| Blocks | any D11 roster widening (hard); nothing else on the current path |
| Permission class | P1 |
| Review tier | B; escalates to frontier if the predicate change reclassifies **any** unit of a currently-rostered faction, or if the Naaz decision is contested |

## Objective and normative source

Make `UnitType::is_ground_force()` (`crates/ti4-content/src/units.rs`) agree with the corpus's own
`isGroundForce` flag for every unit record, so that M08-020's ground-force-only invasion semantics
(LRR 49/42) are correct for **all** units, not all-but-one.

Normative sources: FFG LRR 2.0 rules 42/49 (ground forces fight in ground combat); the corpus
`units.json` `isGroundForce` flag as the content-level statement of which units are ground forces;
the unit's printed ability text where it bears on classification (Naaz space mech).

## The defect (as found by the M08-020 reviewer)

The predicate today:

```rust
pub fn is_ground_force(&self) -> bool {
    matches!(self.base_type(), "infantry" | "mech") || self.id() == "titans_pds2"
}
```

hardcodes one id, while the sibling `is_structure()` three lines above reads `flag("isStructure")`
from the same corpus. The records:

| id | name | baseType | combat | isStructure | isGroundForce (corpus) | predicate today |
|---|---|---|---|---|---|---|
| `titans_pds` | Hel-Titan I | pds | hits 7, dice 1 | ✓ | **✓** | ✗ **missed** |
| `titans_pds2` | Hel-Titan II | pds | hits 6, dice 1 | ✓ | ✓ | ✓ (hardcoded id) |
| `pds` | PDS I | pds | none | ✓ | — | ✗ (correctly) |

**Why M08-020 made it material:** before that package, any rival unit contested a planet, so a
Hel-Titan I defended its planet by accident of the over-broad trigger. After it, a planet
defended solely by a Hel-Titan I falls **without resistance**, and on a mixed planet the
Hel-Titan can no longer be assigned a casualty or keep the fight alive once infantry die — an
incorrect outcome for exactly one unit, replacing correct-by-accident with wrong.

**The flag is not a drop-in replacement.** Comparing flag vs predicate across all 46 records:

```text
flagged isGroundForce:              45
matched by is_ground_force():       46
flagged but MISSED by code:         ['titans_pds']
matched by code, NOT flagged:       ['naaz_mech_space', 'absol_naaz_mech_space']
```

A bare `flag("isGroundForce")` would fix Hel-Titan I **and drop the two Naaz space mechs**, which
the base-type match currently catches and the corpus deliberately does not flag.

## Required decision (answer before switching the predicate)

**Are the Naaz space mechs ground forces?** Recommended answer: **yes — keep them classified as
ground forces, via union semantics:**

```rust
pub fn is_ground_force(&self) -> bool {
    self.record.flag("isGroundForce") || matches!(self.base_type(), "infantry" | "mech")
}
```

Rationale to verify against the rules at implementation time: a Naaz space mech (e.g. Z-Grav
Eidolon, `absol_naaz_mech_space`: "If this unit is in the space area of the active system, it is
also a ship") **is** a mech — on a planet it stands and fights as a ground force (corpus gives it
combat stats: hits 8, dice 1); its ability adds ship status *in space*, it does not remove ground
status on planets. The corpus flag appears to mark "ground force beyond the standard infantry/mech"
rather than "exclusively a ground force"; union semantics preserve every current classification
while adding exactly `titans_pds`. Under this change:

- `titans_pds` → ground force (fixed);
- `titans_pds2`, all infantry, all mechs (incl. both Naaz space mechs) → unchanged;
- `pds`, `spacedock` and every other structure/ship → unchanged.

If the rules say otherwise for the Naaz units, record that decision with its source in evidence —
but note it would reclassify roster-reachable units only if Naaz enters the roster (it does not
today), so even a different answer stays dormant under D11's six factions; the escalation clause
above still applies.

## Required behavior and tests

1. Red-first: a focused test asserting `is_ground_force()` is true for `titans_pds` (fails before,
   passes after) — placed next to the existing
   `the_titans_pds_is_a_ground_force_despite_being_a_structure`, which must be extended or
   replaced so it tests **both** Titans PDS records against the corpus rather than a hand-built id.
2. A property-style sweep test: for every unit record in the embedded corpus,
   `is_ground_force()` equals the recorded decision table (flag ∪ base-type match) — i.e., the
   45/46 comparison above becomes an executable invariant, so any future corpus edit that breaks
   the agreement fails loudly.
3. No classification change for any unit of a currently-rostered faction (asserted or argued in
   evidence against the roster list).
4. Full affected-crate + workspace ×2 + replay-determinism + Clippy (pasted) + `git diff --check`
   gates; KD-5 removed from the ledger on acceptance and commit.

## Scoped access (declared at start, before any finding exists)

```text
Writable paths:
  crates/ti4-content/src/units.rs        (predicate + its tests)
  plans/M08-022_TITANS_PDS_GROUND_FORCE_PREDICATE.md
  plans/evidence/M08-022.md
  plans/EXECUTION_STATE.md
  plans/KNOWN_DIFFERENCES.md             (KD-5 removal on acceptance only)
Read-only supporting paths:
  crates/ti4-content/content/units.json, crates/ti4-engine/src/invasion.rs,
  plans/M08-020_OPEN_REVIEW_ITEMS.md, plans/evidence/M08-020.md
Network/process needs: bounded Cargo test/lint commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

## Non-goals

No invasion-flow changes (M08-020's semantics stand); no roster widening; no bot work. If the
sweep test surfaces further flag/predicate disagreements beyond the three records above, stop and
record them — do not silently widen the fix.
