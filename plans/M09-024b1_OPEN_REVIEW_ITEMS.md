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
