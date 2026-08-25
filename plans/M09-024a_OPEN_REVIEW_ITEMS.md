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
