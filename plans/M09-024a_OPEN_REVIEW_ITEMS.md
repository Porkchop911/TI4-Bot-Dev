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
