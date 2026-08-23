# M09-018 — Open review items (re-execution on current-tree evidence)

Package: frontier schema/math review over rows 001–017, re-executed because the historical record
(`plans/evidence/M09-018.md`, and its siblings for rows 001–017) is a hollow checklist — no
commands, no results, no reviewer identity — and predates the M06–M08 rework this code now runs
against. Spec: `plans/M09-018_FRONTIER_SCHEMA_MATH_REVIEW.md`.

## Campaign summary (2026-08-23)

| Part | Area | Result |
|---|---|---|
| 0 | Row-by-row reconciliation, rows 001–017 | **8 full / 6 partial / 3 absent** (table in evidence) |
| 1 | Hashing | **PASS** — `bucket()` bit-compatible with the oracle's `_bucket`; all 48 golden rows re-derived independently (Python `hashlib.blake2b` vs Rust `blake2`) with **0 mismatches**; corpus covers dimensions {1, 7, 512, 4096} × realistic feature names incl. unicode and a 100-char name |
| 2 | Migrations | **GAP** — no schema 3→4 or 4→5 migration code exists anywhere in the workspace; runtime `resolved_head`/`head()` fallback verified consistent with all three carried head sets (every required set contains `other`, so post-validation resolution always succeeds) |
| 3 | Softmax stability / inference numerics | **PASS** — max-shifted softmax, temperature guarded at load (`validate`) and inference (`max(1e-6)`), non-finite total falls back to uniform rather than NaN, pinned ChaCha8Rng sampling with determinism + empirical-odds tests |
| 4 | Feature purity | **HOLDS structurally; deliverable absent** — zero authored constants on the feature/scoring path (full literal scan); legality-only sampling pinned by test; but row 014's *instrumented* isolation test does not exist |
| 5 | Compatibility / artifact import | **PASS** — all real branch checkpoints (stage-1 + stage-2 envelopes, six factions each) deserialize into `Profile`, pass `validate(Some(faction))`, and score non-zero through the current explicit path; fingerprints (schema=4, mode, faction, 14 heads) round-trip. All 90 surviving nested profiles are schema 4 |

Gates: `cargo test -p ti4-policy` **119/0** · `cargo test -p ti4-training` **104/0** ·
`git diff --check` clean · no source files touched by this package.

## Findings

### F-M09-018-1 — schema 3→4 economy migration absent (row 008) — MEDIUM

No code in the workspace migrates a schema-3 profile's `economy` head into the schema-4
`production`/`payment` split. A schema-3 checkpoint would validate (all 11 required heads present)
but score production/payment decisions from the `other` fallback, silently ignoring its economy
weights. **No local artifact is affected** — every surviving profile is schema 4 — but the
milestone goal ("load supported learned-policy schemas 2–5") and exit gate ("existing supported
profiles execute in Rust") name it.

- **Disposition:** scoped child package before M09-030 (exit review). Not blocking for rows
  019–023, which profile and extend the schema-4/5 surface only.

### F-M09-018-2 — schema 4→5 other-split migration absent (row 009) — LOW

No code splits a schema-4 `other` head into the schema-5 `scoring`/`agenda`/`exploration`/
`ability`/`transit` heads. Mitigated by design: `resolved_head` documents that "schema 3/4 route
later splits to `other`", so schema-4 profiles play correctly — they simply never use the split
heads.

- **Disposition:** child package or explicit operator scope decision before M09-030. Not blocking
  for rows 019–023.

### F-M09-018-3 — row 014's instrumented isolation test absent — MEDIUM

The milestone exit gate requires inference to be "demonstrably free of authored utilities". This
review verifies purity on the current tree (full numeric-literal scan of `features.rs`/
`inference.rs`: only structural constants — unit weights, clamps, the 1e-6 temperature guard, the
0..1 sampling range; no bot scores or playbook values), and the legality-only property is pinned by
`every_legal_option_keeps_a_chance`. But **no committed test would catch a future regression** that
adds an authored constant to a feature path.

- **Disposition:** scoped child package before M09-030; recommended early, because rows 021–023 add
  new feature families that must also be pure and the instrumentation should cover them.

### F-M09-018-4 — no committed test loads a real artifact through the Profile API — LOW

Part 5's import proof used a temporary probe (deleted after the run, M08-019 precedent). The
mechanism is proven; nothing in the tree pins it.

- **Disposition:** natural home is M09-020 (durable baseline fixtures with checksum manifests) —
  once ≤50 MiB compressed artifacts are committed, a committed import test against them belongs
  there. If M09-020's scope does not absorb it, fold into the F-M09-018-3 child package.

### F-M09-018-5 — `validate` does not check head-size uniformity for schema 2 — LOW

A hand-crafted schema-2 profile with mismatched per-head bucket counts passes validation; scoring
then uses the first head's size for hashing while other heads silently miss lookups (zero scores).
No real artifact is affected (all surviving profiles are schema 4 explicit).

- **Disposition:** note; fix opportunistically in whichever child package next touches
  `Profile::validate`.

### F-M09-018-6 — golden feature fixture is thin — INFO

`golden_features.json` carries 5 rows against the largest feature surface in the crate. The bucket
(48) and head-routing (46) corpora are adequate by comparison.

- **Disposition:** expand when M09-021–023 add their feature families; no standalone action.

## Observations (not findings of this package)

- **O1 — pre-existing rustfmt drift.** `cargo fmt -p ti4-policy --check` fails on committed code at
  `features.rs:690/752` (let-else chain and call formatting under the current toolchain). Not
  introduced by this package; no source edits are in scope here. Disposition for reviewer/operator.
- **O2 — pre-existing compiler warning.** `unused_attributes` at `ti4-engine/src/choice.rs:563`
  (duplicate `#[must_use]`), recorded previously in the M08-019 review; unchanged.

## Status

**Corrections complete for this package's scope (review + records only). Requesting independent
Tier C frontier adjudication.** Findings F-M09-018-1/2/3 require child packages or operator scope
decisions before M09-030; none blocks the start of M09-019, whose dependency is this gate's
acceptance.

---

## Independent Tier-C verdict on `bd89568` (Codex frontier, 2026-08-23)

**Changes required; do not accept M09-018 yet.** The hashing and numerical-stability conclusions
are supported, and the recorded executable gates reproduce. Two material gaps remain in the
campaign record:

### R1 — MEDIUM: schema-2 import incompatibility is missing from the findings

The persisted schema-2 format at pinned `37061c5` stores one flat `learned.weights` mapping and
one `learned.temperature`. Rust deserializes `Learned { heads: ... }` instead, and
`learned.rs` explicitly says importing an oracle schema-2 checkpoint requires a migration owned by
M09-015. No such importer or migration exists in the workspace. A legacy schema-2 profile therefore
does not merely lack a committed test: it cannot deserialize into the current `Profile` shape.

Part 5 tests only schema-4 artifacts, so its area verdict cannot be **PASS** for the stated
schema-2–5 compatibility objective. Add a distinct finding for the absent schema-2 flat-vector
migration/import path, change Part 5 to a partial/gap verdict, and give the finding a dependency-safe
child-package disposition before M09-030. It need not block schema-4-only M09-019 work once the
record is corrected, but it does block the M09 exit gate if left unresolved.

### R2 — MEDIUM: F-M09-018-1 understates the schema-3 routing failure

Schema 3 carries one `economy` head. Current `decision_head` requests all four successor heads —
`trade`, `tokens`, `production`, and `payment` — and `resolved_head` sends every absent one to
`other`. F-M09-018-1 names only production/payment, but trade and token decisions also ignore the
trained economy weights. Correct the finding, evidence, and summary everywhere they characterize
the affected decision families. The existing disposition (a migration child package before the
exit review, non-blocking for schema-4-only rows 019–023) remains reasonable.

### Reproduced gates

- `cargo test -p ti4-policy` — **119 passed, 0 failed**; doc tests 0/0.
- `cargo test -p ti4-training` — **104 passed, 0 failed**; doc tests 0/0.
- `cargo clippy -p ti4-policy --all-targets` — no warning in `ti4-policy`; only the two recorded
  pre-existing `ti4-engine` warnings.
- `cargo fmt -p ti4-policy --check` — reproduces only O1 at `features.rs:690/752`.
- `git diff aa15a39..bd89568 --check` — clean.

F-M09-018-2 through F-M09-018-6 are otherwise fairly characterized. After R1/R2 are corrected,
request a fresh Tier-C recheck; no Rust source change is required by this verdict.
