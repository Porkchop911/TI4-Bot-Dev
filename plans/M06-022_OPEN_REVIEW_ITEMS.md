# M06-022 — open review items

Independent Tier-B review requested for `wp/m06-022-counting-objective-progress` against base
`5d027e8`.

Review the uncommitted diff in `objectives.rs`, `secrets.rs`, the exact package specification, and
`plans/evidence/M06-022.md`. In particular verify:

- all 34 named aliases map to the correct stable family, parameter, and non-zero threshold;
- affected registered legality is derived only from the typed progress result;
- maximum, distinct, trait, colour, attachment, ship, structure, and unit semantics match the
  existing accepted predicates without double counting;
- map-dependent counts return unavailable rather than factual zero, while map-independent counts
  remain available;
- progress queries are deterministic and observationally pure;
- unknown aliases remain unavailable and unscoreable; and
- the focused, affected-crate, workspace, lint, and diff evidence is reproducible.

Record reviewer identity/model, independence, commands, findings, and final disposition here. The
implementer will resolve every actionable finding and rerun affected gates before commit.

---

# Review — 2026-08-21

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Did not write M06-022, its specification, or any code under review. Author of M06-021, which this package refactors — not a conflict for this diff. |
| Reviewed | uncommitted working tree vs `5d027e8` |
| Diff | `objectives.rs` +506/-, `secrets.rs` +277/- |
| **Disposition** | **Accept.** Three findings, none blocking; G1 should be recorded before M09-020. |

## Verification

Each checklist item was checked against the code, and the threshold mapping mechanically
diffed against the base commit rather than read by eye.

| Item | Result |
|---|---|
| 34 aliases → family, parameter, threshold | **Verified.** 24 in `objectives.rs` + 10 in `secrets.rs` = 34. Every family, parameter and threshold matches the pre-refactor helper exactly: **zero mismatches**, every threshold non-zero. |
| legality from typed progress only | **Verified.** Every arm of `requirement_for` reduces to `counting_satisfied(alias, p)` → `counting_progress`, and `RequirementProgress::satisfied` is the only comparison. |
| max / distinct / trait / colour / attachment / ship / structure / unit semantics | **Verified.** `same_trait_count` buckets by trait and takes the max, with dual-trait planets counting toward both; `colours_count` counts *distinct* colours meeting `per_colour`, with `UNITUPGRADE` and `NONE` excluded per 90.7b; `tech_specialties_count` counts a planet once regardless of how many specialties it carries; `Units { base_type }` and `PlanetsOfTrait { trait_name }` carry the parameter in the family rather than in the threshold. No double counting found. |
| map-dependent → unavailable, not zero | **Verified for this package's scope.** Exactly one counting family reads the optional map: `on_the_rim_count` (`objectives.rs:320`), which propagates `position.galaxy?` as `None`. The other helpers reference `ti4_content::galaxy::all_systems` — the *content corpus*, always available — not the board layout, so returning `usize` is correct. See G2 for what this does **not** cover. |
| deterministic and observationally pure | **Verified.** `counting_progress(&ObjectiveId, &Position) -> Option<RequirementProgress>`: immutable input, value output, no interior mutability or RNG. Two full 150-game runs produced byte-identical output. |
| unknown aliases unavailable | **Verified.** `_ => return None`, and `counting_satisfied` uses `is_some_and`, so an unknown alias is unsatisfied rather than defaulted. |
| evidence reproducible | **Verified.** `cargo test --workspace` green; `ti4-engine` 822 (from 816). Clippy: no new warnings. |

**Behaviour preservation.** The package claims to be a pure refactor, so the strongest check
is that nothing moved. 150 holdout games on the r6 champions reproduce the post-a2b engine
*exactly* — Betray a Friend 11/58, Drive the Debate 9/54, Darken the Skies 1/50, Prove
Endurance 17/45, Spark a Rebellion 5/39, Dictate Policy 10/37, Turn Their Fleets to Dust
3/32. Every count identical. Combined with zero threshold drift, the refactor is
behaviour-preserving on the evidence available.

## Findings

### G1 — MEDIUM · scope gap across the package sequence

**Four registered secrets fall between M06-022 and M06-023, and neither specification
claims them:** `eh`, `hrm`, `ose`, `sai`.

| alias | pre-refactor shape | why it is not in M06-022 |
|---|---|---|
| `eh` Establish Hegemony | `combined_value(12, Influence)` | value sum, not an item count |
| `hrm` Hoard Raw Materials | `combined_value(12, Resources)` | value sum |
| `ose` Occupy the Seat of the Empire | `hold_mecatol(3)` | compound: control Mecatol **and** 3+ ships |
| `sai` Seize an Icon | `a_legendary_planet()` | nullary; effectively a count ≥ 1 |

M06-023 covers "all 16 aliases" — six bespoke plus ten bought — so 34 + 16 = 50 and these
four are in neither.

`eh` and `hrm` are the least defensible omissions: they are already written as
`helper(threshold)` and would need only a `CombinedValue { kind }` family to fit the exact
shape this package introduces. They also carry real gradient — combined influence 0→12 is
precisely the "80% of the way there" signal the progress work exists to expose, and
Establish Hegemony scores 45% of draws while Hoard Raw Materials scores 53%, so these are
two of the most-scored secrets in the deck.

**This is a scoping decision, not a defect** — but it is currently undocumented, and
M09-020 will otherwise assume full progress coverage.

**Required action.** Name the residue explicitly in one specification, with either the
family it would need or a recorded decision that it stays boolean-only. Include the
position-only secrets in the same ledger, so the set with no progress representation is
written down once rather than inferred.

**Resolution (2026-08-21).** M06-023 now owns all seventeen remaining position-based secrets,
including exact combined-value, Mecatol, legendary, and map-dependent families. Its acceptance
ledger reconciles ten M06-022 counting + thirteen M06-021a occurrence + seventeen M06-023 position
paths to the complete 40-card deck; no position secret remains implicit or boolean-only.

### G2 — LOW · the unavailability fix stops at the counting families

M06-022 correctly distinguishes "no map" from "requirement not met" for `OnTheRim`. Every
remaining map-dependent predicate still collapses the two:

```rust
let Some(galaxy) = position.galaxy else { return false };
// objectives.rs:334, 354, 370   (intimidate_council, push_boundaries, rule_distant_lands)
// secrets.rs:470, 489, 520      (bespoke secret predicates)
```

Those are M06-023's scope, so this is not a defect here. But it is the same conflation this
package just fixed, and M06-023's specification does not currently mention it.

**Required action.** Carry the unavailable-vs-unmet distinction into M06-023's acceptance
criteria.

**Resolution (2026-08-21).** M06-023 now requires unavailable progress for all three
map-dependent public and all three map-dependent secret paths, plus the one-player `fc` boundary.

### G3 — INFORMATIONAL · stale doc comment

`requirement_for`'s doc comment still reads *"The oracle registers 32; 22 are covered here,
plus the eight bought ones."* The counts are now 30 plus ten bought, and Python parity is no
longer an acceptance criterion under the revised policy. It is the comment a reader consults
to learn what the registered set is, so it is worth correcting while the file is open.

**Resolution (2026-08-21).** The comment now states the accepted Rust registry accurately: thirty
position objectives plus ten bought objectives, with unknown aliases failing closed.

## Carried forward

**F11 from M06-021a is still open** and has no resolution note: Fight with Precision scores
0 of 63 draws end-to-end, and there is no in-situ evidence that the anti-fighter-barrage
pause ever fires in a real game. Unit-tested, never observed. A counter on
`Feat::BarrageTookTheLastFighters` over one 150-game run closes it either way.
