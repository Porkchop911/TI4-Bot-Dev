# M06-023 independent Tier-C review

## Status

Reviewed 2026-08-21. **Accepted after H1 resolution.** H1 was pre-existing behaviour that this
package formalised and tested, so it was fixed here rather than deferred downstream.

## Exact review frontier

- Base: accepted M06-022 commit `d58622c`.
- Branch: `wp/m06-023-remaining-objective-progress`.
- Scoped implementation: `crates/ti4-engine/src/objectives.rs`,
  `crates/ti4-engine/src/secrets.rs`, the M06-023 specification, evidence, review ledger, and
  execution-state checkpoint.
- Excluded dependency-safe preparation: M06-024, M07-019/020, and M08-018/019 specifications are
  not part of the M06-023 behavior review or eventual package commit.
- Normative behavior: accepted Rust predicates and exact payment planner; Python parity is out of
  scope.

## Reviewer

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Did not write M06-023, its specification, or any code under review. Author of M06-021 (superseded) and of the `before_combat` note snapshot referenced in H1. |
| Reviewed | uncommitted working tree vs `d58622c` |
| Diff | `objectives.rs` +594, `secrets.rs` +851 |

## Required independent checks

### 1. Alias-table reconciliation — **PASS**

Extracted by brace-matched parse of each function body and compared against the registries.

| table | count | expected |
|---|---:|---:|
| public counting (`counting_progress`, objectives) | 24 | 24 |
| public bespoke (`remaining_position_progress`, objectives) | 6 | 6 |
| public bought (`bought_progress`) | — | see below |
| `cost_of` arms | 10 | 10 |
| secret counting (`counting_progress`, secrets) | 10 | 10 |
| secret position (`remaining_position_progress`, secrets) | 17 | 17 |
| secret feat (`feat_for`) | 13 | 12 + `bam` |

No duplicate rows in any table. Public: 24 + 6 = 30 = `registered_aliases()`, plus 10 bought
= the full 40. Secret: 10 + 17 + 13 = **40 distinct, no alias in two paths, none missing,
none unregistered** — the complete deck. G1 from the M06-022 review is fully closed.

**`bought_progress` has no alias table at all** — it derives family and target from
`cost_of(alias)?`. A wrong or absent bought threshold is therefore structurally impossible
rather than merely absent from this diff. That is a better answer than the check asked for.

`feat_for` carrying 13 rather than 12 is correct: `bam` moved from a position predicate to
`Feat::LostAHomePlanet` when F7 was resolved.

### 2. Refactored legality vs previous accepted behaviour — **PASS**

Six map boundaries return unavailable rather than a factual zero:

```
objectives.rs  ships_adjacent_to_mecatol_count      -> Option<usize>   (intimidate)
               weaker_neighbours_count              -> Option<usize>   (push_boundaries)
               distinct_rival_home_reaches_count    -> Option<usize>   (distant_lands)
secrets.rs     ship_systems_beside_anomaly_count    -> Option<usize>   (lsc)
               ship_systems_beside_rival_home_count -> Option<usize>   (te)
               neighbour_progress                   -> Option<(usize, usize)>  (fc)
```

`neighbour_progress` returning a pair is the right shape for `fc`'s one-player boundary —
the denominator has to travel with the numerator, or "neighbours with everyone" is
unrepresentable at a table of one. G2 from the M06-022 review is closed.

Maximum and distinct reductions were verified under M06-022 and are unchanged here.

### 3. Bought progress is the greatest exactly affordable scaled cost — **PASS**

```rust
let have = (0..=target).rev()
    .find(|&amount| can_afford(state, content, sources, player, scaled_cost(family, amount)))
    .unwrap_or(0);
```

Correct, and correct for a reason worth recording: because the search runs **downward from
the target**, it returns the greatest affordable amount even if affordability were not
monotonic in `amount`. It does not rely on a monotonicity assumption that nothing proves.

Crucially it calls **the same `can_afford` and the same payment planner the purchase uses**,
so disjoint `AllThree`, trade-good substitution, exhausted planets and split token pools are
*inherited* rather than re-implemented. That is the single most important property in a
tier-C payments package: there is no second affordability model that can drift from the
first.

Offer legality is untouched — `can_afford` at the original `cost_of` value, unchanged from
base.

**Performance note, not a defect.** Each query costs up to `target + 1` `can_afford` calls,
and the `AllThree` arm runs `all_three_plan` on every one of them — up to 11 planner runs per
objective per query. Harmless at scoring time; worth measuring in **M09-021**, where progress
is emitted as a feature on every decision against a ~450 µs/decision budget.

### 4. Deterministic, immutable, no hidden hands, no payment mutation — **PASS**

Every progress API takes `&GameState`. `pay_for` remains the only `&mut` path and is not
reachable from any progress query. `fsn` reads `position.player`'s own `action_cards`; the
`.players` reads resolve faction and home-system identity, both public. Two full 150-game
runs produced byte-identical output.

### 5. Rival-note issuer identity — **FINDING H1**

See below. The identity logic is correct; it is unreachable.

### 6. Reproduce tests, lints, diff — **PASS**

`cargo test --workspace` green; `ti4-engine` **832** (from 822). Clippy: 5 warnings, all
pre-existing and unchanged. `git diff --check` clean. End-to-end on 150 holdout games: every
secret and public rate identical to `d58622c`, so the package is behaviour-preserving.

---

## Findings

### H1 — MEDIUM · the issuer lookup is correct but unreachable

`rival_note_issuers_count` does the domain mapping **exactly right**: it resolves a note's
faction to a seated player, keys entries `player:{id}` or `faction:{name}` so the two
identifier domains cannot collide, and collects into a `BTreeSet` so Support for the Throne
plus a faction note from the same seated issuer count once. That is precisely what check 5
asks for.

It is gated behind a lookup that always fails:

```rust
position.content.get(ContentType::PromissoryNotes, note)   // `note` is the full stored key
```

The engine stores notes as `note_id(alias, owner_faction)` (`promissory.rs:44`), i.e.
`"ambuscade:argent"`. `ContentStore::get` is an exact `by_id` lookup (`loader.rs:368`) and the
corpus id is the bare alias.

**Proven, not inferred.** A probe was added, run, and removed:

```
PROBE alias="ambuscade" faction="argent" real_key="ambuscade:argent"
PROBE lookup by bare alias -> Some("argent")
PROBE lookup by real key   -> None
```

So in production the faction-note branch never fires and `rival_note_issuers_count` counts
**Support for the Throne only**.

**Not a regression.** The pre-refactor `holds_a_rivals_note` performed the identical lookup,
and `sb` Strengthen Bonds is unchanged at 49/56 = 88% — Support for the Throne is traded
often enough to carry it.

**Why it still belongs to this package.** The accompanying test
`rival_note_progress_deduplicates_note_kinds_from_one_issuer` inserts the **bare alias** as
the note key. That is a key format the engine never produces, so the test is green over a
synthetic input and the dedup property check 5 asks about is not exercised on the production
path.

**Required action.** Use the existing helper — `promissory::alias_of(note)`
(`promissory.rs:49`) exists for exactly this — and rebuild the dedup test on a real
`note_id(alias, faction)` key.

**Related, outside this package.** The same faction-versus-`PlayerId` confusion appears in the
`WonAgainstANoteHolder` emitters, which compare a note's owner-faction string against a
`PlayerId` (`combat.rs` `BeforeCombat::notes`, and the ground path in `invasion.rs`). That is
my code from M06-021, not codex's, and it means `baf` Betray a Friend also resolves only via
Support for the Throne. Worth a separate item against M06-024 rather than widening this
package.

### H2 — INFORMATIONAL · misleading helper name

`rival_docks_adjacent_count` implements "ships in the **same system** as another player's
space dock", which is what `csl` Cut Supply Lines says and what the previous predicate did.
The name says "adjacent". Behaviour is right; the name will mislead the next reader.

**Resolution.** Renamed it to `rival_dock_systems_with_ships_count`, matching the same-system rule.

## Finding resolution

H1 is fixed by resolving production-format note keys through `promissory::alias_of(note)` before
the corpus lookup. `rival_note_progress_deduplicates_note_kinds_from_one_issuer` now inserts the
real `promissory::note_id(alias, faction)` key and proves it deduplicates with Support from the same
seated issuer. The focused test, Strengthen Bonds regression, full 832-test engine suite, five
doctests, and `git diff --check` pass after the fix.

## Disposition

**Accept.** H1 is resolved and H2's optional rename was applied. The related pre-existing
`WonAgainstANoteHolder` emitter defect is explicitly carried into M06-024 and does not weaken this
package's exact position-progress result.

Reconciliation, payment-planner reuse, map-unavailability, purity and evidence all pass, and
the 40-card secret deck now reconciles exactly across the three progress paths with no
overlap and no residue.
