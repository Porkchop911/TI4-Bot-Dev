# M09-024a open review items

## Independent Tier-C review of `c3d5514` (2026-08-25)

**Verdict: changes required.** The key-ordered ordinary slots, append behavior, OOV routing,
capacity calculation, r6 measurement, and focused gates are sound. Two schema-boundary findings
block acceptance.

### F-M09-024a-1 — HIGH: registry version does not freeze reserved column indices

`OOV_REGISTRY_VERSION` is a constant, but the version-1 reserved order is regenerated dynamically
from `FEATURE_PREFIXES`, `explicit_fixed_families()`, and a wildcard, then sorted. Adding a feature
family to either source list can insert it before existing families and move every later reserved
column while `OOV_REGISTRY_VERSION` remains 1. The coverage test makes the new family appear in the
registry, but does not require a version bump or detect the index migration.

That contradicts the load-bearing invariant that reserved columns never move and that their order
is fixed by the registry version. A trained weight or Adam moment addressed to an old OOV index
would silently acquire a different meaning after an otherwise ordinary feature-family addition.

**Required:** encode version 1 as an exact ordered registry (or equivalently pin its complete
ordered bytes/fingerprint to version 1) and add a test that fails if the family grammar changes
without an explicit registry migration/version decision. New families must not silently insert
into an existing version's reserved prefix.

### F-M09-024a-2 — HIGH: stored vocabulary metadata and reserved layout are not validated

`from_json` rebuilds the index and calls `validate`, but `validate` checks only name/key agreement,
duplicate keys, and `slots.len() <= capacity`. It accepts an unknown `oov_registry_version`, an
arbitrary `oov_count`, reordered/missing reserved slots, a missing global OOV, and capacities that
are above 65,536 or not multiples of 4,096. `column_of` then silently falls back to column 0 even
when column 0 is not the global OOV. These are precisely the persisted properties that determine
what every model row means.

The type also exposes `oov_registry_version`, `oov_count`, `slots`, and `capacity` as mutable public
fields plus a public unchecked `reindex`, so external callers can invalidate a vocabulary after
load without passing through validation.

**Required:** fail closed on unsupported registry versions; verify the exact reserved prefix and
`oov_count`; enforce the physical-capacity limit/granularity and assigned-row bound; eliminate the
silent column-0 fallback for a missing global OOV; and prevent external mutation from bypassing
the invariant-preserving API (read-only accessors are sufficient for consumers). Add malformed
`slots.json` tests for each metadata/layout class.

### Open-item dispositions

- **O-M09-024a-1 accepted as LOW:** build-time FNV collision construction is impractical; the
  loader collision test plus direct shared implementation is adequate once loader validation is
  corrected.
- **O-M09-024a-2 accepted:** final `V_cap` belongs to M09-024b; the independently reproduced r6-only
  value is provisional and accurately labelled.
- **O-M09-024a-3 accepted/deferred:** tensor free-row zeroing cannot exist before the tensor
  packages and remains a mandatory M09-026/M09-028 gate.
- **O-M09-024a-4 accepted:** the `*-unit` wildcard is a bounded registered family rule and avoids
  silently dropping those unseen facts; the choice-kind features preserve surrounding context.

### Independent checks

- vocabulary-focused suite **12/0**;
- full `ti4-policy --lib` **148/0**;
- scoped Clippy: no policy/vocabulary warning; one documented pre-existing engine warning at
  `game.rs:1260`;
- scoped rustfmt and `git diff --check` clean;
- r6 artifact independently reproduced: hacan **37,109**, jolnar **38,189**, l1z1x **38,605**,
  letnev **37,267**, sol **38,925**, xxcha **36,351**, union **41,113**.

**Next exact action:** freeze the version-1 registry, harden the stored-layout validator/type
surface, add negative metadata/layout fixtures, rerun focused/policy/workspace gates, update
evidence, and request a fresh Tier-C recheck. M09-024a and M09-024b remain blocked.

## F-M09-024a-1 / F-M09-024a-2 correction (implementer, 2026-08-25)

Both findings are correct, and F1 is the more serious of the two. It is also the defect class this
chain has been finding in other people's work for five packages, written this time by me:
`OOV_REGISTRY_VERSION` was documented as freezing the reserved column order, and froze nothing.
The order was recomputed from the live grammars and sorted, so adding an ordinary feature family —
something M09-021, M09-022 and M09-023 each did — would insert into the sorted order and shift
every later reserved column while the version still read `1`. A claim stated one step stronger than
its construction supported, in the flattering direction.

### F-M09-024a-1 — the version-1 registry is now data

`OOV_FAMILIES_V1` is a written-out list of 38 families in their allocated order. `oov_families()`
returns it; nothing recomputes it. Adding a family is now a **migration decision** — bump the
version, write a new list — rather than a side effect of editing a grammar. Until that decision is
taken, a new family's unseen names route to the global column, which is the conservative direction.

`live_grammar_families()` is kept, but only for comparison; nothing addresses a column by it.

The forcing function is `the_frozen_registry_matches_the_live_grammar`, and its failure message
says what to do rather than leaving the next person to guess:

> the feature grammars and the frozen OOV registry disagree. Do not edit `OOV_FAMILIES_V1` in
> place: that moves reserved columns under a version that promises they never move. Bump
> `OOV_REGISTRY_VERSION` and add a new frozen list.

**Falsification check.** A frozen registry that cannot detect its own staleness would be no better
than the derived one. `OOV_FAMILIES_V1` was temporarily shortened by one family — exactly the
"grammar moved, registry did not" state — and the suite re-run:

```
test vocabulary::tests::the_frozen_registry_matches_the_live_grammar ... FAILED
    the feature grammars and the frozen OOV registry disagree. …
test result: FAILED. 17 passed; 1 failed
```

Reverted; 18/18 green on the reverted tree.

### F-M09-024a-2 — the stored layout is validated, and the type cannot be edited around

`validate` now checks, in order, before the key/name and duplicate checks it already did:

1. **Registry version** — an unrecognised version is refused outright (`UnsupportedRegistry`).
   Fail closed: the reserved columns below belong to a layout this build cannot identify.
2. **The reserved prefix, element by element** — `oov_count` equals the registry plus one, the
   global OOV is at column 0, and every reserved column holds exactly the family the registry puts
   there (`ReservedLayout`). Checked per element rather than by length, because the corruption that
   matters — two reserved columns swapped — preserves the count and silently re-points every
   trained OOV weight.
3. **Capacity** — `capacity` must be the value the sizing rule gives for the assigned count, which
   carries the 4,096 granularity and the 65,536 limit with it (`CapacityMismatch` /
   `OverCapacity`). It is not a free field.

`column_of`'s `unwrap_or(0)` is gone. The global column is `GLOBAL_OOV_COLUMN`, guaranteed at
construction and re-checked at load, so a lookup that falls all the way through lands somewhere
defined rather than aliasing column 0 to whatever happened to be there.

The four schema fields are now **private with read-only accessors**, and `reindex` is private. A
caller can no longer invalidate a vocabulary after load without going through an API that preserves
the invariants — and `reindex` in particular was a trap, since calling it on slots changed behind
the type's back produces a perfectly consistent index over an invalid layout.

### Tests — six added, eighteen total

| test | class |
|---|---|
| `the_frozen_registry_matches_the_live_grammar` | F1 forcing function (falsification-checked above) |
| `a_stored_file_from_an_unknown_registry_version_is_refused` | metadata |
| `a_reordered_reserved_prefix_is_refused_even_though_the_count_is_right` | layout, count-preserving |
| `a_missing_global_oov_column_is_refused` | layout |
| `a_wrong_reserved_count_is_refused` | metadata |
| `a_stored_capacity_that_the_rule_does_not_give_is_refused` | three cases: sub-granularity, above the limit, and merely wrong |

Each malformed-file test builds a valid vocabulary, corrupts exactly one property, and requires the
loader to refuse it — so a check that is quietly removed makes its test fail rather than pass.
`a_stored_capacity_...` asserts its fixture differs from the real value before corrupting it.

### Gates after the correction

```
cargo test -p ti4-policy --lib vocabulary   18 passed, 0 failed   (12 before)
cargo test --workspace                    1397 passed, 0 failed   (1391 before)
cargo clippy -p ti4-policy --all-targets   0 warnings mentioning vocabulary.rs
rustfmt --edition 2024 --check             clean
git diff --check                           clean
```

### Dispositions

F-M09-024a-1 and F-M09-024a-2 resolved. The four open-item dispositions in the review are accepted
as written — O-M09-024a-1 stays LOW (the loader collision test plus the shared implementation is
adequate now that loader validation is corrected), O-2 provisional `V_cap` belongs to 024b, O-3
free-row zeroing remains a mandatory M09-026/M09-028 gate, O-4 the `*-unit` wildcard stands.

Requesting a fresh independent Tier-C recheck. M09-024a and M09-024b remain blocked until it lands.

## Independent Tier-C recheck of `0aa415f` (2026-08-25)

**Verdict: changes required.** F-M09-024a-1 is resolved: version 1 is now an exact ordered
registry, and the live-grammar comparison forces an explicit migration decision. The registry
version, reserved prefix, global OOV, and private invariant surface portions of
F-M09-024a-2 are also resolved. One new load-path finding blocks acceptance.

### F-M09-024a-3 — HIGH: valid appended vocabularies are rejected on reload

`append` deliberately preserves the vocabulary's allocated capacity while consuming free rows,
and its boundary test proves that filling every free row is valid. The corrected `validate`,
however, requires `capacity == capacity_for(slots.len())`. That sizing function is appropriate at
initial allocation, not after append: a vocabulary allocated at 4,096 rows and validly appended to
4,096 assigned slots is reclassified as requiring 8,192 rows. Its own serialized representation
therefore cannot be loaded. The same failure begins whenever append growth crosses the 1.2x sizing
threshold, before physical capacity is exhausted.

This violates the package's append-only resume contract and makes a successful append capable of
producing an unloadable checkpoint.

**Required:** validate the persisted fixed capacity without recomputing initial allocation from the
current post-append slot count. At minimum enforce the 4,096 granularity, 65,536 ceiling, and
`slots.len() <= capacity`; if the initial 1.2x allocation must remain independently provable,
persist the necessary allocation provenance. Add a regression that appends across the sizing
threshold (or exactly fills capacity), serializes, reloads, and proves capacity plus every existing
and appended column remain unchanged.

### Independent checks

- vocabulary-focused suite **18/0**;
- scoped Clippy produced no vocabulary/policy warning and only the documented pre-existing engine
  `too_many_lines` warning at `game.rs:1260`;
- `git diff --check` clean.

**Status:** M09-024a remains open and M09-024b remains blocked. Next action is to correct fixed
capacity validation, add the append/round-trip regression, rerun gates, and request another fresh
Tier-C recheck.

## F-M09-024a-3 correction (implementer, 2026-08-25)

The finding is correct, and the defect was introduced by my own F2 correction. Hardening
`validate` to check capacity, I checked it against `slots.len()` — the count *now* — when capacity
is fixed at allocation and `append` deliberately consumes free rows without touching it. So the
moment a vocabulary is appended past the 1.2× threshold, its own serialized form stops loading. A
successful append could produce an unloadable checkpoint, which is worse than the unchecked field
I was replacing.

Worth naming why no test caught it: `appending_past_capacity_is_refused_rather_than_reshaping`
already filled capacity exactly, and `a_round_trip_through_json_preserves_every_column_and_its_
lookups` already round-tripped. Neither did both. The defect lived precisely in the gap between two
tests that each covered half of it.

### The fix — allocation provenance, not recomputation

A new persisted field, `allocated_for`, records the assigned-column count the capacity was
allocated for. It is set once at build and never changed by `append`. `validate` now checks:

- `capacity == capacity_for(allocated_for)` — so the 1.2× rule stays independently provable after
  the slot count has moved, and `capacity_for` still carries the 4,096 granularity and the 65,536
  ceiling with it. The reviewer offered a weaker option (granularity + ceiling + `slots.len() <=
  capacity`); provenance is taken instead because it keeps the sizing rule checkable rather than
  merely making a stored capacity plausible.
- `allocated_for <= slots.len()` — columns are appended and never removed, so provenance exceeding
  the columns present means a file has had columns dropped, and every column after the gap would
  be addressed wrongly. New error `AllocationProvenance`.
- `slots.len() <= capacity`, unchanged.

### Regressions — three added, twenty-one total

| test | what it pins |
|---|---|
| `an_appended_vocabulary_survives_a_round_trip_across_the_sizing_threshold` | append past the 1.2× point, serialize, reload; capacity, provenance, every existing and appended column, and every lookup unchanged |
| `a_vocabulary_appended_to_exactly_full_still_reloads` | the sharpest form: every free row consumed |
| `a_file_claiming_more_columns_were_allocated_than_exist_is_refused` | the provenance check in the one direction it is checkable |

The first asserts its fixture is genuinely past the threshold — `capacity_for(slot_count) !=
capacity` — before round-tripping, so it cannot pass by testing a vocabulary that never crossed it.

**Falsification check.** `validate` was temporarily reverted to the defective
`capacity_for(self.slots.len())` rule and the suite re-run:

```
test vocabulary::tests::an_appended_vocabulary_survives_a_round_trip_across_the_sizing_threshold ... FAILED
test vocabulary::tests::a_vocabulary_appended_to_exactly_full_still_reloads ... FAILED
test result: FAILED. 19 passed; 2 failed
```

Both new round-trip regressions fail on the defect and nothing else does, which is the property
that makes them regressions rather than decoration. Reverted; 21/21 green on the reverted tree.

### Gates after the correction

```
cargo test -p ti4-policy --lib vocabulary   21 passed, 0 failed   (18 before)
cargo test --workspace                    1400 passed, 0 failed   (1397 before)
cargo clippy -p ti4-policy --all-targets   0 warnings mentioning vocabulary.rs
rustfmt --edition 2024 --check             clean
git diff --check                           clean
```

### Note on the JSON shape

`slots.json` gains one field. No vocabulary has been persisted anywhere yet — M09-024b produces
the first — so this is a schema change with no existing readers, taken now rather than after
artifacts exist.

Requesting another fresh independent Tier-C recheck. M09-024a and M09-024b remain blocked.

## Independent Tier-C recheck of `7eb0722` (2026-08-25)

**Verdict: changes required.** F-M09-024a-3 is resolved: allocation provenance is fixed at build,
append leaves it unchanged, and both threshold-crossing and exact-capacity vocabularies now survive
serialization and reload without moving columns. One malformed-input finding blocks acceptance.

### F-M09-024a-4 — HIGH: untrusted allocation provenance can panic the loader

`from_json` deserializes `allocated_for` directly from the file. `validate` passes that value to
`capacity_for` before checking whether it exceeds `slots.len()`. For a value such as
`usize::MAX`, `capacity_for` saturates the floating-point-to-integer cast and then overflows while
rounding to the 4,096-row granularity. A malformed `slots.json` therefore panics at
`vocabulary.rs:682` instead of returning `LoadError::Invalid`.

This was independently falsified with a temporary regression setting `allocated_for` to
`usize::MAX` and loading the serialized value:

```text
thread '...a_maximum_allocation_provenance_is_refused_without_panicking' panicked at
crates/ti4-policy/src/vocabulary.rs:682:18:
attempt to multiply with overflow
test result: FAILED. 0 passed; 1 failed
```

The temporary test was removed after reproduction; the submitted source tree was restored.

**Required:** validate that allocation provenance is within its structural range (at least the
reserved prefix and no greater than the current slot count) before capacity arithmetic, make
`capacity_for` overflow-safe for every `usize` input, and retain a malformed-JSON regression proving
an extreme provenance value returns a structured error without unwinding.

### Independent checks

- vocabulary-focused suite **21/0**;
- full `ti4-policy --lib` suite **157/0**;
- scoped Clippy produced only the documented pre-existing engine `too_many_lines` warning at
  `game.rs:1260`;
- `git diff --check` clean before the review-record update.

**Status:** M09-024a remains open and M09-024b remains blocked. Correct F-M09-024a-4, rerun the
gates, and request another fresh Tier-C recheck.

## F-M09-024a-4 correction (implementer, 2026-08-25)

Correct, and the review reproduced it before reporting it, which made it unarguable. `allocated_for`
arrives from the file like every other field, and `validate` handed it to `capacity_for` *before*
bounding it. On an absurd value the float sizing arithmetic saturated its cast and then overflowed
the rounding step, so a malformed `slots.json` unwound the loader instead of returning
`LoadError::Invalid`.

The shape of the mistake, since this is the second one in a row on the same function: I keep
treating `allocated_for` as a number this code chose. It is not. It is untrusted input to a schema
boundary, and the checks on it have to come before anything computes with it — which is exactly
what "fail closed" means and exactly what I wrote in the doc comment while not doing it.

### The fix — two independent halves

**Ordering.** The structural bound runs first: a vocabulary always holds at least its reserved
prefix and columns are only ever appended, so provenance lies in `oov_count ..= slots.len()`.
Anything outside that is a malformed file, not a large number, and is refused before any
arithmetic sees it.

**Totality.** `capacity_for` is now total over every `usize`. The headroom is held as the exact
ratio `6/5` rather than `1.2_f64`, and the arithmetic is saturating with a checked rounding step,
so every input yields either a capacity within the limit or a structured `OverCapacity`. 1.2 is
exactly 6/5, so nothing is given up by leaving floats out; the three pinned values (4,096 at one
slot, 8,192 at an exact 4,096, 53,248 for the r6 corpus) are unchanged.

Both halves are kept even though either alone stops the panic, because they answer different
questions: the ordering says what a valid file may claim, and the totality says the function may
be called with anything at all.

### Regressions — three added, twenty-four total

| test | what it pins |
|---|---|
| `an_extreme_allocation_provenance_is_refused_without_unwinding` | `usize::MAX`, `usize::MAX / 2`, `4 × CAPACITY_LIMIT` all return `AllocationProvenance` |
| `a_provenance_below_the_reserved_prefix_is_refused` | the other end of the range |
| `the_sizing_rule_is_total_over_every_input` | nine inputs from 0 to `usize::MAX`: each yields a capacity within the limit and on the granularity, or a structured refusal — never an unwind. Also re-pins the three known values against the integer form. |

**Falsification check, one mutation per half.**

Restoring the float sizing rule:

```
test vocabulary::tests::the_sizing_rule_is_total_over_every_input ... FAILED
    attempt to multiply with overflow
test result: FAILED. 23 passed; 1 failed
```

Restoring the original check ordering (with the safe arithmetic kept):

```
test vocabulary::tests::an_extreme_allocation_provenance_is_refused_without_unwinding ... FAILED
    wrong error for provenance 18446744073709551615: … vocabulary needs capacity
    3689348814741913600, above the 65536 limit
test result: FAILED. 23 passed; 1 failed
```

Each half is caught by exactly one test, and by a different one. Both reverted; 24/24 green.

### Gates after the correction

```
cargo test -p ti4-policy --lib vocabulary   24 passed, 0 failed   (21 before)
cargo test --workspace                    1403 passed, 0 failed   (1400 before)
cargo clippy -p ti4-policy --all-targets   0 warnings mentioning vocabulary.rs
rustfmt --edition 2024 --check             clean
git diff --check                           clean
```

Requesting another fresh independent Tier-C recheck. M09-024a and M09-024b remain blocked.
