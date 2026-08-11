# M02-009 … M02-012 — Content corpus, indexes, provenance, referential validation

## Package

| Field | Value |
|---|---|
| ID | M02-009, M02-010, M02-011, M02-012 (implemented as one package) |
| Milestone | M02 — Content and model |
| Depends | M02-001 (identifier newtypes), M01 (workspace) |
| Objective | Load the full language-neutral content corpus into Rust with the oracle's ordering, scoping, and resolution semantics, then prove the corpus is intact and internally consistent. |
| Review tier | B (ordinary model code). **Not yet independently reviewed — see Open findings.** |
| Permission class | P1 (edits inside this repository), plus read-only inspection of the Python oracle. |

Grouped into one package because the four rows share a single edit scope
(`crates/ti4-content/`) and cannot be tested apart: an index has nothing to index and a
digest has nothing to digest until the corpus loads.

## Oracle

| Field | Value |
|---|---|
| Repository | `D:\Projects\ti4-engine` (read-only) |
| Branch | `codex/fully-learned-policy` |
| Commit | `37061c511a4780d4c0719e0342533a498cd4b457` |
| Tree state before and after | clean (verified with `git status --short --branch`) |

Python sources read: `engine/content.py`, `engine/units.py`, `engine/technology.py`
(lines 50–65, for how `decks` is consumed), `engine/content/manifest.json`.

Python tests mirrored: `tests/test_game.py:25-38` (content wiring, corpus provenance),
`tests/test_galaxy.py` (unit stats from data, capacity, fleet movement),
`tests/test_factions.py` (faction counts, complexity, corpus completeness).

## Changed paths

| Path | Change |
|---|---|
| `crates/ti4-content/content/*.json` | **New.** 29 files (28 categories + manifest), copied byte-for-byte from the oracle. |
| `crates/ti4-content/content/CHECKSUMS.sha256` | **New.** SHA-256 of every corpus file plus the provenance header. |
| `crates/ti4-content/src/lib.rs` | Rewritten from a stub module list to the crate API. |
| `crates/ti4-content/src/loader.rs` | Rewritten. `ContentStore` replaces two `todo!()`s. |
| `crates/ti4-content/src/manifest.rs` | Rewritten. Typed `Manifest` replaces a `todo!()`. |
| `crates/ti4-content/src/provenance.rs` | Rewritten. Canonical digests replace a `todo!()`. |
| `crates/ti4-content/src/validator.rs` | Rewritten. Referential validation replaces a `todo!()`. |
| `crates/ti4-content/src/record.rs` | **New.** Typed record access. |
| `crates/ti4-content/src/units.rs` | **New.** `UnitType` view over unit records. |
| `crates/ti4-content/src/error.rs` | **New.** `ContentError`, `ReferenceError`. |
| `crates/ti4-content/Cargo.toml` | Added `hex` (digest formatting). |
| `crates/ti4-model/src/content_types.rs` | Rewritten — see "Corrections to existing code". |
| `.gitattributes` | **New.** Pins corpus files to no end-of-line translation. |

Five `todo!()` stubs removed. No other crate was touched.

## Corrections to existing code

`ti4-model/src/content_types.rs` declared 28 content categories, **14 of which do not
exist in the corpus** (`Objectives`, `Secrets`, `ExplorationCards`, `Fragments`, `Maps`,
`Laws`, `ExpeditionTiles`, `FactionAbilities`, `UnitAbilities`, `TechAbilities`,
`CardEffects`, `GameRules`, `BotProfiles`, `TrainingConfigs`), and omitted 14 that do
(`abilities`, `agendas`, `attachments`, `colors`, `combat_modifiers`, `explores`,
`franken_errata`, `galactic_events`, `genericcards`, `map_templates`, `public_objectives`,
`rules`, `sources`, `strategy_card_sets`). The list was invented rather than read from the
oracle. It has been replaced with the real taxonomy, and two tests now pin it in both
directions: `every_corpus_file_is_declared_in_the_model` (a new file must gain a variant)
and `every_category_declared_in_the_model_is_present_and_non_empty` (a variant must have a
file). Nothing consumed the old enum except a `todo!()`, so no behaviour regressed.

## Semantics preserved from the oracle

| Behaviour | Oracle | Here | Test |
|---|---|---|---|
| File order is iteration order | `content.load` returns a tuple in file order | `records()` returns a `Vec` in file order; `Record::index` records the position | `file_order_is_preserved`, `a_source_filter_keeps_file_order` |
| Source filter compares the record's own tag | `r.get("source") in sources` | `Record::in_sources` | `an_untagged_category_is_empty_under_every_source_filter` |
| Untagged categories yield nothing when filtered | inherited | `ContentType::is_source_tagged` documents it | same |
| Strategy cards re-sort by initiative, stably | `sorted(cards, key=initiative)` | `sort_by_key` (stable) | `strategy_cards_are_sorted_by_initiative_not_file_order` |
| TE suffix fallback (`-te`, `_te`) | `content.resolve_id` | `ContentStore::resolve_id` | `a_te_suffixed_id_falls_back_to_its_base_record_when_te_is_out_of_scope` |
| Source sets `BASE` / `POK` / `FULL` | `frozenset` constants | `EnumSet<Source>` constants | `source_sets_nest` |
| Per-category identity index | per-module `catalogue(sources)` | `ContentStore::catalogue` | `identities_are_unique_within_every_keyed_category` |
| Unit stats are data | `units.UnitType` property views | `units::UnitType` borrowed view | 22 tests in `units.rs` |
| Capacity is computed, not read | `consumes_capacity` = fighter or (ground force and not structure) | same | `capital_ships_do_not_consume_capacity_despite_declaring_a_cost` |
| `productionValue: "+2"` means planet resources plus two | `int(str(raw).lstrip("+"))` with a relative flag | `UnitType::production(planet_resources)` | `a_generic_space_dock_produces_planet_resources_plus_two` |
| Zero means "does not fight" | `int(value) if value else None` | `positive()` helper | `a_unit_that_does_not_fight_has_no_combat_value` |

### Intentional differences

| Difference | Reason |
|---|---|
| The corpus is compiled in with `include_str!`, not read relative to a module path. | A harness that resolves content against the working directory loads nothing after a `cd`. `ContentStore::from_dir` remains for a regenerated or reduced corpus, and `a_corpus_read_from_disk_matches_the_embedded_one` proves the two agree. |
| Record counts are checked against `manifest.json` at load. | The oracle never cross-checks. A corpus and manifest from different extractions would otherwise load cleanly with every downstream count quietly wrong. |
| An unrecognised `source` tag is a load error, not a silent filter miss. | Extraction drops 56 homebrew tags; an eighth official tag appearing is a corpus change that should stop the build, not disappear. |
| `int()` rejects a leading `+`. | Rust's `i64::from_str` accepts `"+2"` and would read it as 2, silently halving a space dock's output. Python's `int("+2")` has the same hazard, which is why the oracle strips the sign explicitly. |
| No per-record caching of `from_sources`. | The oracle caches because the filter ran 1.27 M times per game in Python. Here the source tag is parsed once at load into an `Option<Source>`, so the filter is an integer comparison. |

## Corpus integrity

```
$ cd D:/Projects/ti4-engine/engine/content && sha256sum *.json > $TEMP/oracle_sums.txt
$ cd D:/Projects/ti4-engine-rs/crates/ti4-content/content && sha256sum -c $TEMP/oracle_sums.txt
29 files: OK, 0 mismatches
```

Recorded in `crates/ti4-content/content/CHECKSUMS.sha256` together with the oracle commit,
the upstream AsyncTI4 commit (`8e90459d789fb767b9d5aff3a55bd7dd0b3e781b`), and its licence
(Unlicense, software and tooling only — no art assets are included).

`.gitattributes` marks these files `-text` so that end-of-line translation cannot
invalidate the checksums on a non-Windows checkout.

Corpus contents as loaded: **1,800 records across 28 categories, 237 untagged**, matching
`manifest.json` exactly.

## Referential validation

15 reference rules plus deck contents, followed under `BASE`, `POK`, and `FULL`.
**2,393 references checked under `FULL`; 0 unexpected breaks.**

Three findings, each resolved by correcting the model rather than by loosening a check:

1. `technologies.baseUpgrade` is **not** a unit id. It names the generic unit-upgrade
   *technology* a faction technology replaces (Sol's `so2` → `inf2`). All 9 distinct values
   are technology aliases; only `ws` is coincidentally also a unit `asyncId`.
2. `explores.attachmentId` targets **either** an attachment **or** a token. `gamma`,
   `ionalpha`, and `mirage` are tokens, not attachments.
3. Deck `cardIDs` are checked for existence, not for source scope. A deck record declares
   its whole contents regardless of which expansions are enabled, and the game selects a
   deck rather than filtering one, so scope-checking decks reports false gaps.

19 references dangle upstream and are allowlisted individually in `KNOWN_GAPS` with a
reason. `every_allowlisted_gap_is_still_a_real_gap` fails if the allowlist and the corpus
ever disagree, so a stale entry cannot survive:

| Reference | Cause |
|---|---|
| `units.ghemina_carrier2 → dsghemcv` | Discordant Stars technology dropped at extraction; upstream tags the unit `base`. No faction lists this unit. |
| `decks.explores_cpti → 18 card ids` | 18 of 80 cards in the Council-Preview variant explore deck are homebrew and were dropped. Not used by base or PoK setup. |

**`BASE` is not a scope in which a faction can be seated**, and that is the corpus
describing the real game rather than a defect: `leaders.json` is entirely `pok` and
`thunders_edge`, mechs are `codex3` and later, and Arborec's `md` is the codex 4 printing
of Magen Defense Grid. 69 references are therefore unresolvable under `BASE` — 51 leaders,
17 mechs, 1 technology. Pinned as a characterisation test
(`the_base_scope_lacks_leaders_and_mechs_by_design`) with exact counts, so a *different*
breakage under `BASE` still fails rather than being lost in expected noise. This matches
the oracle, which defaults `factions()` to `FULL` and only `strategy_cards()` to `BASE`.

## Commands and results

```
$ cargo test -p ti4-content -p ti4-model
test result: ok. 73 passed; 0 failed  (ti4-content lib)
test result: ok.  9 passed; 0 failed  (ti4-model lib)
test result: ok.  1 passed; 0 failed  (ti4-content doc-tests)

$ cargo test --workspace
118 passed; 0 failed        (37 before this package)

$ cargo clippy -p ti4-content
0 warnings in crates/ti4-content/**

$ rustfmt --edition 2024 <the nine files listed above>
clean
```

Test counts by module: `loader.rs` 24, `units.rs` 22, `validator.rs` 10, `record.rs` 8,
`provenance.rs` 6, `content_types.rs` (ti4-model) 6, doc-test 1.

No benchmark was run: this package adds no hot path and the M00-012 microbenchmark
protocol has no harness in this repository yet. Parsing the full corpus takes well under
the 0.11 s the whole `ti4-content` suite occupies, which is the only measurement claimed.

## Open findings

1. **No independent review.** The standard requires that the implementer not be the sole
   reviewer, and this package has not had one. It is Tier B. Marked open rather than
   claimed as satisfied.
2. **No differential fixture evidence.** Every semantic claim above is checked against
   values read from the oracle's data and source, not against an exported oracle trace,
   because no oracle exporter exists in this repository (M00-009 was documented but never
   built). Parity here is "same data, same documented rule", not decision-boundary
   differential evidence, and must not be reported as the latter.
3. **`ti4-model::units::UnitType` is now redundant.** It is an invented, unpopulated struct
   with fields that do not match the corpus, still referenced by `view.rs`. Removing it
   touches `view.rs` and belongs in its own package.
4. **20 of 28 categories have no consumer yet.** The loader reads all 28; only `units`,
   `factions`, `strategy_cards`, `decks`, `technologies`, `leaders`, `planets`, and
   `systems` are interpreted. This is expected at M02 but should not be read as M02 being
   complete.

## Definition of done

- [x] Behaviour implemented; no `todo!()` remains in `ti4-content`.
- [x] Focused and affected-crate tests pass.
- [x] Corpus integrity established and recorded.
- [x] Compatibility semantics documented against named oracle sources, with intentional
      differences listed.
- [ ] Independent review performed. **Outstanding.**
- [x] Committed without unrelated edits.
