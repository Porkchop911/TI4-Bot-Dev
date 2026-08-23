# M08-020 independent Tier-C frontier review — ground-combat structure legality (F-M07-019-1 fix)

## Status

**Accept with one required correction (T1).** The fix is correct, well-scoped, and closes
F-M07-019-1 and the M1c deliverable properly. T1 is a defect this package *introduces* for one
unit — unreachable under the current roster, live the moment it widens.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of this. Authored F-M07-019-1 and the M1c deliverable as the M07-019 reviewer, and R1 of the M07-020 adjudication that scoped this package — so I am independent of the implementer but invested in the finding. Recorded per the M06-024 precedent. |
| Base | `734de3f` (M08-017 closure) |
| Diff under `crates/` | `invasion.rs` +390/−82 (production), `game.rs` +84/− (test module only) |
| Checks | engine **847** + 5 doctests, workspace **1,321**, replay 4/4, Clippy three pre-existing / zero new — all reproduced |

## What verifies

### The fix is right, and complete in a way I did not ask for

I raised F-M07-019-1 as a trigger problem. The package correctly found it is five problems, and
fixed all five: the trigger (`advance_fighting`), **both** casualty pools (`remove_ground` and
`absorb_ground`), **both** termination conditions (`resolve_ground_round` and the standalone
`ground_combat`), and the destruction-on-capture sweep. The termination point in particular is a
catch I missed — without it a mixed planet whose rival ground forces all died would keep
"fighting" surviving structures with the defender rolling zero dice, and the window path is
choice-driven with no round cap. That is a hang, and it would have been introduced *by fixing only
the trigger.*

`finish_control_gain` is the right chokepoint for the sweep — it serves both the immediate and
post-pause resume paths — and the sweep correctly runs after Assimilate, so conversion wins and
only unconverted rival structures are destroyed.

### The M1c deliverable is genuinely closed

`the_home_loss_pause_holds_the_invasion_at_finalizing_control` is no longer vacuous. At the pause
it asserts `standing.len() == 6` (a's three infantry plus b's three intact structures) and
`rival_structures == 3`; after the resume, `pds == 2`, `spacedock == 1`, all owned by `a`. Those
are the assertions M07-019's M1 said were passing on the invader's own infantry. They now bite.

### The corpus correction was right, and correcting me was the right call

Spec item 4 — my wording — required the ownership assertion to "check for the l1z1x_ variants."
**That instruction was wrong.** The corpus has `l1z1x_dreadnought`, `l1z1x_flagship`,
`l1z1x_mech`, `l1z1x_paradigm` and no structure variants, so Assimilate returns generic
`pds`/`spacedock` under the invader's ownership. The package checked the corpus instead of
following the instruction, and substituted base-type count preservation plus ownership. That is the
correct assertion and the right handling of a reviewer instruction that did not survive contact
with the data.

### Gates

engine 847 (+2, the two new invasion tests), workspace 1,321, replay 4/4, Clippy exactly the three
documented pre-existing warnings with none new, `git diff --check` clean over `crates/`, and
`invasion.rs`/`game.rs` rustfmt-clean.

## Findings

### T1 — MEDIUM (required before commit) · this package makes `is_ground_force` load-bearing, and it misses Hel-Titan I

`UnitType::is_ground_force()` (`ti4-content/src/units.rs:107`) is:

```rust
matches!(self.base_type(), "infantry" | "mech") || self.id() == "titans_pds2"
```

It hardcodes one id. **The corpus carries an `isGroundForce` flag, and nothing in the codebase
reads it** — while `is_structure()`, the sibling predicate three lines above, reads
`flag("isStructure")` from that same corpus. Two adjacent predicates answering the same kind of
question through different domains.

The records:

```
titans_pds   Hel-Titan I    baseType pds  combatHitsOn 7  dice 1  isStructure ✓  isGroundForce ✓
titans_pds2  Hel-Titan II   baseType pds  combatHitsOn 6  dice 1  isStructure ✓  isGroundForce ✓
pds          PDS I          baseType pds  (no combat stats)       isStructure ✓
```

Hel-Titan I **rolls a die at 7**. It is unambiguously meant to fight, the corpus says so twice, and
`is_ground_force()` returns false for it.

**Before this package that mismatch was benign for combat**: `defender_on` counted any rival unit,
so a Hel-Titan I contested the planet — right outcome, wrong reason. **This package makes it
material.** After the change:

- a planet defended solely by a Hel-Titan I is uncontested — it **falls without resistance** and
  the Hel-Titan is destroyed by the capture sweep without ever rolling;
- on a mixed planet it can no longer be assigned a casualty, and it cannot keep the fight alive
  once the infantry die.

So the package fixes the general case and, for exactly one unit, replaces a correct-by-accident
outcome with an incorrect one.

**Reachability.** Titans is not in D11's roster (sol, letnev, xxcha, hacan, jolnar, l1z1x), so this
cannot fire in play today — structurally blocked, like M06-025's L1, not merely rare. It fires the
moment the roster widens, which D11 plans.

**Why it stayed green:** `the_titans_pds_is_a_ground_force_despite_being_a_structure`
(`units.rs:409`) tests `titans_pds2` only. It is named for "the Titans PDS" singular, and was
written against the hardcoded id rather than against the corpus — the same defect class the M06
exit evidence named: *an identifier resolved through the wrong domain, staying green because the
test constructed the identifier by hand.*

**The obvious fix is not safe as-is, and this is the part worth carrying into the fix.** Comparing
the flag against the predicate across all 46 unit records:

```
flagged isGroundForce:              45
matched by is_ground_force():       46
flagged but MISSED by code:         ['titans_pds']
matched by code, NOT flagged:       ['naaz_mech_space', 'absol_naaz_mech_space']
```

A bare `self.record.flag("isGroundForce")` would fix Hel-Titan I **and simultaneously drop the two
Naaz-Rokha space mechs**, which `matches!(base_type, "mech")` currently catches and which the
corpus deliberately does not flag. Whether a mech operating in space should be a ground force is a
real question with a real answer, and it must be answered before the predicate is switched — not
discovered afterwards.

**Required action.** Record T1 with the Naaz caveat. `ti4-content` is outside this package's
writable paths, so the fix is a scoped child; given the roster block it does **not** need to
precede M08-018, but it must precede any roster widening. Recommend adding it to the
known-differences ledger alongside KD-2 and hard-ordering it against D11.

### T2 — LOW (evidence) · the corpus verification was selective in the one place it mattered

The evidence states the predicate is "corpus-verified", listing `infantry`, `pds`, `spacedock` and
`titans_pds2`. Every record it names is one the current predicate already gets right. The record
that would have falsified the claim — `titans_pds`, sitting immediately adjacent in `units.json`
with the same two flags — was not listed.

This is not sloppiness so much as the shape verification takes when it is aimed at confirming a
predicate rather than at finding its boundary. Worth naming, because "corpus-verified" is exactly
the phrase a later package will cite.

**Required action.** Amend the claim to say what was checked and what the flag actually says.

### T3 — INFORMATIONAL · defender selection changed shape, benignly

`ground_combat` previously took the defender from the first unit found on the planet; it now takes
the first element of a `BTreeSet<PlayerId>` of ground-force owners. Both are deterministic; the
`BTreeSet` is sorted by `PlayerId`, so it is arguably *more* deterministic than storage order. In a
three-way contest the chosen defender could differ from before. No rule governs the choice and no
test depends on it. Recorded only so the change is not mistaken later for an accident.

## Disposition

**Accept.** T1 must be recorded and scoped as a child before any roster widening; T2 is a one-line
evidence correction; T3 needs nothing.

F-M07-019-1 is properly closed, KD-2 is discharged rather than merely documented, and the M1c
deliverable that has been carried since M07-019 is now a test that would fail if the behavior
regressed. This is the strongest package in the chain: it went past the finding it was given,
found the hang the narrow fix would have introduced, and corrected a reviewer instruction that was
wrong on the data.

T1 exists *because* of that quality, not despite it — making `is_ground_force` load-bearing is what
turned a dormant corpus mismatch into something that can be seen at all.

## Resolution (implementer, 2026-08-22)

**Gate status: accepted.** All findings resolved before commit:

- **T1 — recorded and scoped as required.** `ti4-content` is outside this package's writable
  paths, so no code change was made. Recorded as **KD-5** in `plans/KNOWN_DIFFERENCES.md`
  (alongside the now-closed KD-2 slot), with the Naaz space-mech caveat carried verbatim into the
  child spec **M08-022** (`plans/M08-022_TITANS_PDS_GROUND_FORCE_PREDICATE.md`): union semantics
  recommended as the required decision, flag-vs-predicate comparison table included, red-first and
  sweep-test requirements specified. Milestone-plan row added with hard ordering against any D11
  roster widening (not before M08-018/021 — dormancy argument accepted as stated).
- **T2 — applied.** The evidence's "corpus-verified" claim was corrected at its site to state the
  four records actually checked and what the `isGroundForce` flag says about the one record that
  would have falsified it, with a pointer to T1/KD-5/M08-022.
- **T3 — recorded.** Noted in the evidence findings ledger as an intentional, deterministic shape
  change (BTreeSet ordering by `PlayerId`), not an accident.

KD-2 removed from the known-differences ledger per its own exit condition (accepted and
committed). Package committed on `wp/m08-020-ground-combat-structure-legality`.
