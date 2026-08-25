# M09-024b1 open review items

## Independent Tier-C review of `4f63973` (2026-08-25)

**Verdict: changes required.** The projection correctly restores all eight bare seat facts, leaves
the schema-4 extractor unchanged, suppresses the three named high-cardinality crosses before
lookup, and implements the pre-artifact v2 prefix migration. Two schema forcing functions remain
open.

### F-M09-024b1-1 — HIGH: projection admission is open by default and retains legacy-only rows

`admits` returns true for every family not present in the three-entry `EXCLUDED_FAMILIES` deny-list.
That is the opposite of the architecture ruling's requirement that a new/unclassified family is
excluded until reviewed. It also affects the current corpus: `kind-faction` and `option-faction`
are legacy-only channels that the schema-4 explicit extractor never emits, but `project_names`
admits their r6 checkpoint names. M09-024b2 would therefore retain roughly 6,188 stale ordinary
columns and may reproduce the contaminated 24,576 capacity instead of deriving the corrected
single-path layout.

The dead-row inventory has the same incomplete boundary. The reserved rows for `kind-faction` and
`option-faction` are just as unreachable from the MLP runtime path as `prompt-bigram`, but only the
three deny-listed families are marked inactive. The current dead reserved count is five, not three.

**Required:** replace the open-default deny-list with one closed classification covering every
registered family, such as `Transferable`, `UnboundedCross`, and `LegacyOnly`. Unknown/unclassified
families must fail closed. Pin that the classification domain equals the frozen/live family domain;
use the same classification for vector projection, name-source filtering, and dead-row metadata.
Add regressions proving `kind-faction` and `option-faction` checkpoint names are rejected, all five
inactive reserved rows are reported, and an unclassified synthetic family is not admitted.

### F-M09-024b1-2 — HIGH: registry order is not independently pinned

`OOV_FAMILIES_V2` is constructed directly from `OOV_FAMILIES_V1`, and the order test compares v2
back to that same source. If two v1 entries are swapped, v2 changes with them, the set-coverage test
still passes, and the prefix test still passes. The migration therefore removed the former sorted
comparison without replacing its order-sensitive forcing function, despite the clarification
requiring exact v1 bytes and exact v2 order to be pinned separately.

A silent v1 reorder changes the meaning of reserved model rows while both registry tests stay
green. That is the exact schema failure M09-024a's F1 correction was meant to prevent.

**Required:** pin each version's complete ordered bytes or an independent stable fingerprint, not
one constant against another derived from it. Include a falsification check showing that swapping
two v1 entries fails while the version remains unchanged. Keep the separate set-coverage and
v1-prefix assertions.

### Open-item dispositions

- **O-M09-024b1-1 escalated into F-M09-024b1-1:** the deny-list is not merely an INFO limitation;
  it violates fail-closed admission and already retains two legacy-only families.
- **O-M09-024b1-2 accepted/deferred:** tensor zeroing and optimizer masking remain M09-026/M09-028
  obligations, but their input inventory must be corrected to all five inactive families here.

### Independent checks

- projection-focused suite **7/0**;
- vocabulary-focused suite **26/0**;
- full `ti4-policy --lib` suite **169/0**;
- scoped Clippy produced no package-owned warning and only the documented pre-existing engine
  `too_many_lines` warning at `game.rs:1260`;
- scoped rustfmt and `git diff --check` clean.

**Status:** M09-024b1 remains open and M09-024b2 remains blocked. Correct both findings, rerun the
gates, and request a fresh independent Tier-C recheck.

## F-M09-024b1-1 / F-M09-024b1-2 correction (implementer, 2026-08-25)

Both findings are correct. F1 in particular is an inversion of the ruling's own words: it says an
unclassified family "is excluded by default and requires another architecture review to enter the
dense input", and I implemented a deny-list, which admits by default. I quoted that sentence in the
module doc while writing the opposite of it.

### F-M09-024b1-1 — admission is now closed by default

`EXCLUDED_FAMILIES` is gone. In its place is `FAMILY_ROLES`, a **total** classification of all 39
registered families into `Transferable`, `UnboundedCross` and `LegacyOnly`. `role_of` returns
`Option`, and `None` — a family nobody classified — is not admitted. A family nobody classified is
a family nobody decided to put in the model.

The table is written out rather than derived from `EXPLICIT_FIXED_FAMILIES`, for the same reason
the OOV registry is: deriving it would admit a newly added family to the dense input as a side
effect of an ordinary grammar edit. `the_classification_covers_exactly_the_registry` fails when the
table and the registry drift, and its message says the decision is an architecture one rather than
a test to make green.

**The two legacy-only families the finding named.** `kind-faction` and `option-faction` are never
emitted by the schema-4 explicit path — the explicit test asserts exactly that — but they *are* in
the r6 checkpoint, which is discovery source (a). The old `admits` let them through, so M09-024b2
would have carried roughly 6,188 stale columns and could have reproduced the contaminated capacity
instead of deriving the corrected one. They are now `LegacyOnly` and rejected.

**Dead rows are five, not three.** `inactive_families()` returns both non-transferable roles, and
`is_dead_reserved` is defined against the classification rather than against the crosses alone.
M09-026/M09-028's zeroing and masking inventory is corrected accordingly.

### F-M09-024b1-2 — each version has an independent ordered fingerprint

The finding is exact: `OOV_FAMILIES_V2` is built from `OOV_FAMILIES_V1` and the order test compared
v2 back to its own source, so swapping two v1 entries moved both together and every assertion
stayed green. The migration removed the sorted comparison's order-sensitivity without replacing it
— the same failure M09-024a's F1 correction existed to prevent, reintroduced one package later by
the fix for it.

`registry_fingerprint` is SHA-256 over the ordered names, and each version pins its own:

```
v1  7bde13aa2972405de8944f3fdb9593453f3efb34f7f90817374658e8dbdc7a04
v2  8bb0d25c5c49d9c751a2385016b3c3dcd1a70b86fcd856f1508148de1a5006ac
```

Neither is derivable from the other. The set-coverage and v1-prefix assertions are kept alongside.

### Falsification checks — one per finding

**F2, swapping two v1 entries with the version unchanged:**

```
test vocabulary::tests::the_reserved_order_is_pinned_and_v2_preserves_every_v1_index ... FAILED
    the ordered v1 registry changed. Reserved model rows are addressed by this order;
    a reorder is a migration, not an edit.
test result: FAILED. 25 passed; 1 failed
```

Under the previous code this mutation passed every registry test. That is the gap closed.

**F1, reclassifying a legacy-only family as transferable:**

```
test projection::tests::legacy_only_checkpoint_names_are_rejected ... FAILED
test projection::tests::every_inactive_family_is_reported_and_every_other_is_live ... FAILED
test result: FAILED. 10 passed; 2 failed
```

Both reverted; 174/0 on the reverted tree.

### Tests — five added, twelve in `projection`

`the_classification_covers_exactly_the_registry`, `an_unclassified_family_is_not_admitted`,
`legacy_only_checkpoint_names_are_rejected` (with a non-vacuity check that the same call keeps a
transferable name), `every_inactive_family_is_reported_and_every_other_is_live`, and
`the_unit_suffix_rule_resolves_to_one_role` — the `<kind>-unit` families share one registry entry,
so they must share one role rather than falling through to unclassified.

### Gates after the correction

```
cargo test -p ti4-policy --lib projection   12 passed, 0 failed   (7 before)
cargo test -p ti4-policy --lib vocabulary   26 passed, 0 failed
cargo test -p ti4-policy --lib             174 passed, 0 failed   (169 before)
cargo test --workspace                    1420 passed, 0 failed   (1415 before)
cargo clippy -p ti4-policy --all-targets    0 warnings in either file
rustfmt --edition 2024 --check              clean
git diff --check                            clean
```

### Dispositions

F-M09-024b1-1 and F-M09-024b1-2 resolved. O-M09-024b1-1 is closed rather than carried: it was
escalated into F1 and the deny-list it described no longer exists. O-M09-024b1-2 stands as
deferred, with its inventory corrected from three families to five.

**One consequence worth flagging for M09-024b2.** With the legacy-only families now rejected, the
corrected single-path union loses roughly 6,188 further names beyond the three crosses. The derived
capacity is likely to land at **16,384** rather than the 24,576 ceiling — which the clarification
already anticipates ("may therefore derive 16,384"). 024b2 measures it; nothing here assumes it.

Requesting a fresh independent Tier-C recheck. M09-024b1 remains open and M09-024b2 blocked.

## Fresh independent Tier-C recheck of `0b8bd8e` (2026-08-25) — changes required

The correction closes F-M09-024b1-1's two named legacy-only leaks and adds independent ordered
registry fingerprints, closing F-M09-024b1-2. Focused gates independently rerun at current HEAD:
projection **12/0**, vocabulary **26/0**.

One fail-open edge remains:

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-024b1-3 | **HIGH** | `role_of` maps **every** family ending in `-unit` to the transferable `*-unit` role. Thus `admits("never-reviewed-unit:anything")` is true even though the correction claims unknown families are denied and the architecture ruling approves only `<canonical-kind>-unit`. This is especially material because checkpoint names are a discovery source: an arbitrary historical `*-unit` family can enter the dense vocabulary without the architecture review required for a new family. The test covers two valid examples but no invalid suffix family. | Bound the wildcard to the closed canonical decision-kind inventory (or another independently pinned finite inventory), deny unknown suffix prefixes, and add positive tests for every approved unit family plus a negative unknown-`-unit` regression. Rerun discovery after the corrected predicate; if the retained name set or checksum changes, regenerate and re-review M09-024b2. |

The stale “three dead rows / 768 weights” prose in `vocabulary.rs` and the architecture request must
also be reconciled with the corrected five-row inventory (**1,280** weights at width 256), so
M09-026/M09-028 do not inherit conflicting requirements.

**Disposition:** the operator override remains an operator override, not a reviewer acceptance.
M09-024b1 still needs correction and recheck; M09-024b2 remains review-blocked by this predicate.
