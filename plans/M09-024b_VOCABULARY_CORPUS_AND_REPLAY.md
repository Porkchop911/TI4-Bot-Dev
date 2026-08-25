# M09-024b — Vocabulary corpus and discovery replay

**ID and title.** M09-024b — Vocabulary corpus and discovery replay.

**Milestone and dependencies.** M09; depends on **M09-024a (accepted)**, and through it on rows
019–023. Second and final child of M09-024.

**Normative references.** `docs/MLP_PLAN.md` revision 5 §4.5 (construction: the union of three name
sources) and §6.1 (the fixed teacher seed schedule).

**Acceptance test reference.** M09_LEARNED_POLICY row M09-024, jointly with M09-024a.

**Review tier.** C — schema. The output is the column layout every trained weight is addressed by.

## One-sentence objective

Assemble the three name sources §4.5 names, build the vocabulary from their union, and record the
resulting `slots.json`, `slot_count`, `V_cap` and manifest fields.

## Status — BLOCKED pending operator authorization

This package is **P2** and has not been authorized. It does not start until the declaration below
is granted. Nothing in it has been run.

## Permission class and scoped access declaration

**Class: P2** — bounded replay with a generated artifact. No network access, no downloads, no new
dependencies. Everything is local and reproducible from files already in the repository or already
on disk.

| what | declared bound |
|---|---|
| **Reads** | `out/stage2_r6/final10000.json` (the r6 champions, 33 MB, already present, read-only); `out/pools/full_np8_12_train.json` (the training map pool, already present, read-only); the embedded content corpus |
| **Compute** | game seeds `202_608_210..202_608_338` × six rotations = **768 games**, `Horizon` as §6.1 fixes it. Feature extraction runs on every non-forced decision. Single bounded pass; no training, no optimization, no repetition |
| **Expected wall time** | minutes, not hours — measured and recorded rather than promised |
| **Writes** | exactly one artifact: `out/vocabulary/slots.json` |
| **Artifact size cap** | **≤ 16 MiB.** ~50,000 slots at name + key. If it exceeds the cap the package stops and reports rather than writing |
| **Does not write** | nothing under `crates/`, no checkpoints, no shards, no corpus capture. M10-031 is what captures decisions; this pass keeps **names only** and discards everything else |
| **Determinism** | the pass is re-runnable and must produce byte-identical output; that is asserted, not assumed |

**What this pass is not.** §4.5 is explicit that this is "a bounded discovery pass, not the M10
training corpus". No option records, no probability vectors, no returns, no state — those are
M10-031's, under its own permission. The only thing that leaves this pass is a set of strings.

## The three sources

| # | source | how |
|---|---|---|
| a | the **41,113** names in the r6 profile | union across the six champions' head weight maps. Independently reproduced during M09-024a and matching §4.5 exactly |
| b | every name emitted by replaying the §6.1 teacher seed schedule with the completed extractors | the 768-game pass |
| c | every statically enumerable content name | derived from the corpus without playing anything |

Source (b) is the only one that needs the replay, and it is the only reason this package is P2.

## Invariants

1. **Order-independent.** The union is a set; M09-024a's assignment is by `FeatureKey`, so the
   order names are discovered in cannot reach `slots.json`. Asserted by building twice with the
   sources in reversed order and comparing bytes — the §4.5 double-build requirement, now over the
   real corpus rather than a fixture.
2. **The capacity limit is a stop, not a suggestion.** If the union pushes `V_cap` above 65,536 the
   package **stops for an explicit architecture review** and reports the exact figures. It does not
   raise the limit and does not proceed. This is the one outcome that would change the branch's
   architecture, and it is the reason the number is worth measuring rather than estimating.
3. **The reserved registry is not edited.** If replay surfaces a family the frozen v1 registry
   lacks, `the_frozen_registry_matches_the_live_grammar` fails and the correct response is a
   version bump recorded as a migration — not an in-place edit that moves existing columns.
4. **Names only.** No decision, option, probability or state value is retained.
5. **Hidden information.** The replay plays real games; extraction runs through the same bound
   `SeatObservation` path live play uses. No omniscient shortcut.

## Explicit non-goals

- No tensor, no model, no `tch` (M09-025 onward).
- No teacher-corpus capture (M10-031).
- No re-baseline of anything.

## Tests and evidence

- Determinism: two builds over reversed source order, byte-identical `slots.json`.
- Growth is visible: `slot_count` and `V_cap` recorded **before** (r6 only: 41,152 / 53,248, from
  M09-024a) **and after** each source is folded in, so the contribution of the replay and of the
  content names is separately readable rather than a single final number.
- Every source is non-empty, and each contributes at least one name the others do not — otherwise a
  source that silently produced nothing would be indistinguishable from one that was redundant.
- Manifest fields recorded: `slots_sha256`, `slot_count`, `V_cap`, `oov_registry_version`,
  `allocated_for`.
- Wall time and artifact size recorded against the declared bounds.

## Known traps

- **The silent empty source.** A replay that emits no new names looks exactly like a replay that
  did not run. Each source's individual contribution is measured.
- **The estimated `V_cap`.** 53,248 is the r6-only figure. Reporting it as the final one without
  running (b) and (c) would be a claim stronger than the construction supports — the failure mode
  this chain has hit repeatedly.
- **Vocabulary drift during the pass.** §4.5 forbids the vocabulary mutating during a rollout.
  Discovery here collects names and builds once at the end; it does not append mid-pass.

## Definition of done

Union assembled from all three sources with each contribution measured; `slots.json` written within
the declared cap; double-build byte-identical; `V_cap` recorded and under the limit, or the package
stopped and reported; evidence complete; independent Tier-C review resolved.

**Authorship note.** Claude Opus 5 authors and cannot review it.

## Architecture-gate result and continuation split (2026-08-25)

The Tier-C ruling in `plans/M09-024b_ARCHITECTURE_EVALUATION_REQUEST.md` selects a schema-4
explicit, feature-compressed MLP projection. It excludes unbounded memorisation crosses before
lookup (`prompt-bigram`, `prompt-option`, and `state-option` under the current grammar), requires a
new bounded bare family for the eight acting-seat facts, keeps the 65,536 hard ceiling, and approves
`V_cap = 24,576` subject to corrected single-path confirmation.

This package is split before more implementation:

- **M09-024b1 (P1, Tier C):** projection contract, bare-seat facts, versioned registry migration,
  pre-lookup filtering, and focused invariants. No replay or artifact.
- **M09-024b2 (P2, Tier C):** corrected 768-game single-path discovery, final deterministic
  `slots.json`, measured manifest fields, and final layout review.

The original parent acceptance criterion remains unchanged. M09-024b1 is next ready; M09-024b2 is
blocked on its acceptance.
