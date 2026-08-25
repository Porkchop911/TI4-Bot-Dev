# M09-024 — Dense vocabulary and OOV capacity

**ID and title.** M09-024 — Dense vocabulary, OOV registry and capacity.

**Milestone and dependencies.** M09; depends on rows 019–023, all now accepted.

**Normative references.** `docs/MLP_PLAN.md` revision 5 §4.5 (decision D21) and §6.1 (the fixed
teacher seed schedule).

**Acceptance test reference.** M09_LEARNED_POLICY row M09-024: "P2 replay of the fixed teacher seed
schedule; deterministic double-build, reserved OOVs, append-only logical slots, fixed physical
capacity and hard migration boundary."

## Declared split — read this before the rest

The row does not fit the standard's bounds. It is one behavior cluster only if "build the
vocabulary" and "discover the names to build it from" are the same job, and they are not: the first
is pure logic over a set of strings, the second is a P2 bounded replay of 768 games with a
generated artifact. Together they cross the file/line bound and cannot be reviewed from a single
diff.

Per the plan's own instruction — *"If a row cannot meet the standard's file/line/test bounds, its
task specification records suffixed children before implementation; dependencies and the parent
acceptance criterion remain unchanged"* — M09-024 is split:

| child | scope | permission |
|---|---|---|
| **M09-024a** | The vocabulary itself: reserved OOV registry, key-ordered assignment, collision refusal, logical/physical sizing, append-only growth, `slots.json` round trip and validation. No name discovery. | **P1** |
| **M09-024b** | The corpus: the r6 profile names, the §6.1 teacher-schedule replay, and the statically enumerable content names, folded into the vocabulary; final `slots.json`, `V_cap` and manifest fields recorded. | **P2** (bounded feature-discovery replay, generated artifact) |

The parent acceptance criterion is unchanged and is met only when both land. **This document
specifies M09-024a**; M09-024b gets its own once 024a is accepted.

The split is also what lets the sizing rule be tested at all: 024a can assert what capacity a
corpus of any size demands without generating one.

## M09-024a — objective

One sentence: map feature names to dense columns under a rule that no run can vary, and make
growth append-only so a trained weight never changes meaning.

## Allowed Rust edit paths

- `crates/ti4-policy/src/vocabulary.rs` — new.
- `crates/ti4-policy/src/lib.rs` — module registration.
- `crates/ti4-policy/src/intern.rs` — `FeatureKey::from_bits`, to rebuild a key from a stored one.
- `crates/ti4-policy/src/features.rs` — expose the closed family list so the registry can enumerate
  it. No change to any emitted feature.

## Invariants

1. **Reserved columns never move.** The global OOV is column 0, then one column per registered
   family, in a sorted order fixed by `OOV_REGISTRY_VERSION`. A trained model addresses these by
   index, so they are allocated before anything a corpus could vary.
2. **Assignment is by ascending `FeatureKey`.** The key is a pure function of the name, so input
   order cannot reach the output. This is what makes the double-build check meaningful and what
   lets discovery run in any order on any number of threads.
3. **A key collision is a hard error.** Never an arbitrary tie-break, never a silent alias.
4. **Logical ≠ physical.** `slot_count` is assigned columns; `capacity` is the next multiple of
   4,096 at or above `1.2 × slot_count`, refused above 65,536.
5. **Append-only.** Columns are never reordered or reused. A batch is assigned in ascending key
   *within the batch*. An append that would exceed capacity is refused; capacity rises only by an
   explicit migration.
6. **Unseen names are not dropped.** They reach their family's OOV column, or the global one.
   Dropping would make an unknown `option:` word indistinguishable from its absence — exactly the
   case where the policy should be uncertain rather than confident.

## Explicit non-goals

- No name discovery, no replay, no generated artifact (that is 024b).
- No tensor, no model, no `tch` (that is M09-025 onward).
- No change to any emitted feature name or value.

## Tests to add

Twelve, listed in the evidence. The ones that carry the design: reserved-column stability across
two different corpora; byte-identical output under reversed input; ascending-key assignment;
OOV fallback through family then global; the sizing rule at its boundaries; over-limit refusal;
append key-ordering and non-movement; append overflow refused **and not partially applied**;
stored-file collision refused; stored key/name mismatch refused; JSON round trip preserving
lookups; and every registered family having exactly one reserved column.

## Known traps

- **The vacuous determinism check.** Building twice from the same `Vec` proves nothing. The
  reversed-input test asserts the two inputs actually differ in order before comparing outputs.
- **The untestable collision.** A real 64-bit FNV-1a collision cannot be constructed in a test.
  The branch is reached the way it is genuinely reachable end to end — a stored `slots.json` that
  claims one — rather than by adding a seam that exists only for the test.
- **The registry that silently falls behind.** If a family is added to the extractors and not to
  the OOV registry, its unseen names pool into the global column and nothing complains. Asserted,
  not assumed.
- **Partial application on refusal.** An append that overflows must leave the vocabulary untouched,
  or the refusal is worse than the overflow.

## Definition of done

Invariants 1–6 implemented and tested; workspace green; clippy and format clean; evidence
recorded; independent review resolved.

**Review tier.** C — schema. The column layout is a migration boundary: every trained weight and
every Adam moment is addressed by it.

**Authorship note.** Written and implemented by Claude Opus 5, who reviewed M08-017 through
M09-021 and cannot review this package.
