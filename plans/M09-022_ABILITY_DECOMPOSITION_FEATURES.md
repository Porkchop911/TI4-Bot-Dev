# M09-022 — Ability decomposition policy features

**ID and title.** M09-022 — Ability decomposition policy features.

**Milestone and dependencies.** M09; depends on accepted M08-019, M09-018, and the M09-021
feature-emission structure it reuses.

**One-sentence objective.** Emit typed faction-decomposition facts — abilities, starting
technology, faction technology, starting fleet, home planets, commodities — so that the acting
seat's faction is described by what it *does* rather than by an identity embedding, with every one
of the 33 selectable seats distinguishable from every other.

**Exact normative references.** `docs/MLP_PLAN.md` revision 5 §5.3 (the decomposition table and
its separation requirement), §4.1 (per-option trunk input contract), and decision D8 (the identity
embedding stays, but must not be doing the separating). Content authority is
`crates/ti4-content/content/factions.json` through `ti4_content::factions`.

**Exact acceptance-test references.** M09_LEARNED_POLICY row M09-022: "Typed
ability/start/home/commodity/faction-tech facts separate all 33 selectable seats; unseen identity
remains zero."

**Historical Python references.** None. Rows 019 onward are governed by `docs/MLP_PLAN.md`
revision 5; Python parity is not an acceptance criterion here.

## Measured corpus facts this package is built on

Verified before implementation, against the embedded corpus at `DEFAULT` (= `FULL`):

| quantity | measured |
|---|---|
| faction records at `DEFAULT` | **34** |
| records with empty `homeSystem`, `startingFleet` **and** `homePlanets` | **1** (`neutral`) |
| **selectable seats** | **33** |
| distinct under abilities only | 32 / 34 records — collision `keleresa = keleresm = keleresx` |
| distinct + starting tech | 32 / 34 — same collision |
| distinct + faction tech | 32 / 34 — same collision |
| **distinct + starting fleet, home planets, commodities** | **34 / 34 — no collision** |

Two notes on this table, because both differ from what §5.3 records:

1. §5.3 says "33 playable seats" and this corpus holds **34 faction records**. The extra record is
   `neutral` — the Thunder's Edge neutral-units record. It has no home system, no starting fleet,
   no home planets, no abilities, no leaders, no promissory notes, zero commodities, and none of
   the playable-seat fields (`complexity`, `preferredColours`, `priorityNumber`, `wikiURL`). It is
   not a seat anyone selects. Excluding it gives exactly the plan's 33.
2. The package therefore defines a **selectable seat** by a corpus predicate — a non-empty
   `homeSystem` — rather than by naming `neutral` in code. Exactly one record fails it today, and
   a future non-seat record added the same way is excluded without a code change.

The separation structure §5.3 reports is otherwise reproduced exactly: one collision, the three
Keleres, resolved only by the last row's fields. So the decomposition separates every selectable
seat, and the identity embedding is not carrying that load.

## Allowed Rust edit paths

- `crates/ti4-policy/src/features.rs` — the fact builder, its emission, and the closed-grammar
  family list.
- `crates/ti4-policy/src/lib.rs` — only if a re-export is needed.

No engine edits. No changes to the legacy hashed extractor, to scoring, legality, replay, or to any
schema-2 bucket name.

**Permission class.** P1. No scoped external access; no downloads; no generated artifacts.

**Inputs and outputs.** Input: the acting seat's faction, read from the public observation, and its
corpus record through the game's **active** content store and source scope. Output: additional
named `f64` facts on the explicit feature vectors, in the two namespaces M09-021 established.

## Invariants and compatibility class

1. **The legacy factual policy subvector is unchanged.** Every existing feature name keeps its
   value; the M09-019b pinned inventory and the legacy-subvector pin must pass unmodified except
   for the reviewed addition of the new families.
2. **Active domain, not embedded.** Faction records resolve through `seen.content()` and
   `seen.sources()` — never `ContentStore::embedded()` and never a hardcoded scope. This is the
   defect class M08-019 Y2 named and M09-021 AA1 repeated; it does not get a third outing.
3. **Unseen identity remains zero.** A faction that is not the acting seat's contributes nothing.
   Absent facts are absent, not zero-valued — the existing zero-skip convention.
4. **Bare and crossed namespaces stay disjoint**, exactly as F-M09-021-2 settled: bare names on
   every option under every crossing mode including `StateCross::None`, crossed copies under
   `state-kind:` / `state-option:` for linear delivery.
5. **Deterministic and option-order independent.** Facts derive from the seat and the corpus, not
   from the option list; `BTreeMap`/sorted iteration everywhere.
6. **Hidden information.** Everything read here is public: a seat's faction, and the printed
   contents of its faction card. No hand contents, no deck, no opponent private state.

## Explicit non-goals

- No identity embedding change (D8 keeps it; this package only removes its separating job).
- No leader or promissory-note facts — §5.3 does not list them in the decomposition.
- No opponent-faction facts. This package describes the acting seat only.
- No retraining, no re-baseline. Bounds move only if measured to move.

## Tests to add

1. `ability_decomposition_separates_every_selectable_seat` — build the fact set for all 33
   selectable seats; assert 33 distinct fact sets and zero collisions.
2. `keleres_variants_separate_only_on_the_last_row` — assert the three Keleres share abilities,
   starting tech and faction tech, and are separated by fleet/home planets/commodities. Pins the
   §5.3 claim rather than restating it.
3. `the_selectable_seat_predicate_excludes_exactly_the_neutral_record` — 34 records, 33 selectable,
   and the excluded one is `neutral`. Fails loudly if the corpus grows a seat.
4. `ability_facts_survive_state_cross_none` — a uniform-kind composite-id choice asserted to
   resolve to `StateCross::None`; all fact classes present on every option under bare names, no
   crossed copy, order-deterministic.
5. `ability_facts_use_the_active_content_domain` — a store loaded from a directory, not the
   embedded one, proves the facts follow the active store. Guards invariant 2 by construction.
6. `unseen_factions_contribute_nothing` — no fact names another seat's faction.
7. `the_legacy_subvector_is_pinned_against_the_recorded_baseline` — existing test, must pass
   unmodified.
8. `m09_019b_feature_inventory_is_pinned` — existing test, updated only by the reviewed addition
   of the new families, with the rationale recorded.

## Commands to run

```
cargo test -p ti4-policy
cargo test --workspace
cargo clippy -p ti4-policy --all-targets
rustfmt --edition 2024 --check crates/ti4-policy/src/features.rs
git diff --check
```

## Expected evidence

`plans/evidence/M09-022.md`: commit, the corpus measurement table above regenerated on the
committed tree, focused test output, workspace totals, clippy and format results, and an explicit
statement of whether any M08-021 behavioral bound moved (expected: none — the authored bot's path
is untouched).

## Known traps

- **The embedded-store trap.** `ti4_content::factions::get` takes a store; passing
  `ContentStore::embedded()` compiles, works in every test, and is wrong. Test 5 exists because
  nothing else would catch it.
- **The count trap.** "33" is a corpus fact, not a constant. Hardcoding 33 without the predicate
  makes the test pass for the wrong reason the day a faction is added.
- **The separation trap.** A separation test that builds its key from the same code the facts come
  from proves nothing about the *emitted* features. Test 1 must build its keys from the emitted
  fact set, not from a private helper.
- **Vacuity.** Every separation and survival assertion needs a non-emptiness precondition; §5.3's
  own table is only meaningful because the collision row is non-empty.

## Definition of done

Facts emitted in both namespaces; all 33 selectable seats separated by emitted features; active
content domain proven by test; legacy subvector and inventory pins pass; workspace green; clippy
and format clean; evidence recorded; independent review resolved.

**Review tier.** B (ordinary policy code) — with the caveat that invariant 2 is the M08-019 Y2
defect class, so the reviewer should check the domain question specifically.

**Authorship note.** Written and implemented by Claude Opus 5, who reviewed M08-017 through
M09-021. The reviewer of this package must therefore be someone else; the independent-review seat
for M09-022 onward is open.
