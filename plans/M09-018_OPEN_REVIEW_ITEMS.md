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
| 5 | Compatibility / artifact import | **PARTIAL (gap)** — all real branch checkpoints (stage-1 + stage-2 envelopes, six factions each; every one schema 4) deserialize into `Profile`, pass `validate(Some(faction))`, and score non-zero through the current explicit path. But the persisted oracle schema-2 shape cannot deserialize into `Profile` at all and no importer exists (F-M09-018-7), so this is not a schema-2–5 PASS; schemas 3/5 untested (no local artifacts) |

Gates: `cargo test -p ti4-policy` **119/0** · `cargo test -p ti4-training` **104/0** ·
`git diff --check` clean · no source files touched by this package.

## Findings

### F-M09-018-1 — schema 3→4 economy migration absent (row 008) — MEDIUM *(corrected per R2)*

No code in the workspace migrates a schema-3 profile's single `economy` head into its successor
heads. Schema 3 carries one `economy` head; current `decision_head` requests **all four** successor
families — `trade`, `tokens`, `production`, and `payment` — none of which is in `SCHEMA3_HEADS`, so
`resolved_head` sends every one of them to the `other` fallback. A schema-3 checkpoint would
therefore validate (all 11 required heads present) but score trade, token, production, *and*
payment decisions from `other`, silently ignoring its trained economy weights for all four families.
**No local artifact is affected** — every surviving profile is schema 4 — but the milestone goal
("load supported learned-policy schemas 2–5") and exit gate ("existing supported profiles execute in
Rust") name it.

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

### F-M09-018-7 — schema-2 flat-vector import path absent (added per R1) — MEDIUM

The persisted oracle schema-2 shape at pinned `37061c5` stores one **flat** `learned.weights`
mapping plus a single `learned.temperature` (`blank_profile`: `{"weights": {"h0000": 0.0, …},
"temperature": 1.0}`). Rust's `Learned` deserializes `heads: BTreeMap<String, Head>` instead, so a
legacy schema-2 profile **cannot deserialize into the current `Profile` at all** — serde fails on
the `learned` object before `validate()` can refuse it gracefully — and no importer or migration
exists in the workspace (`learned.rs` assigns that job to M09-015, which was never built). Part 5
exercised only schema-4 artifacts; its verdict is therefore partial for the stated schema-2–5
compatibility objective, not a PASS.

- **Disposition:** scoped child package before M09-030 (exit review); dependency-safe — rows
  019–023 are schema-4/5-only work and do not need it. Blocks the M09 exit gate if left unresolved.

## Observations (not findings of this package)

- **O1 — pre-existing rustfmt drift.** `cargo fmt -p ti4-policy --check` fails on committed code at
  `features.rs:690/752` (let-else chain and call formatting under the current toolchain). Not
  introduced by this package; no source edits are in scope here. Disposition for reviewer/operator.
- **O2 — pre-existing compiler warning.** `unused_attributes` at `ti4-engine/src/choice.rs:563`
  (duplicate `#[must_use]`), recorded previously in the M08-019 review; unchanged.

## Status

**Corrections complete for this package's scope (review + records only).** Findings
F-M09-018-1/2/3/7 require child packages or operator scope decisions before M09-030; none blocks
the schema-4-only start of M09-019 after this gate's acceptance.

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

---

## Fresh independent Tier-C recheck on `b81ede2` (Codex frontier, 2026-08-23)

**Changes required; do not accept M09-018 yet.** R1 and R2 are technically correct in the newly
edited Part 5, finding, and resolution sections, but the active package records still contradict
those corrections.

### R3 — LOW, required records correction: stale PASS/status claims remain

- `plans/M09-018_FRONTIER_SCHEMA_MATH_REVIEW.md` still says **“Result: Parts 1/3/5 PASS”** in its
  active status summary, even though R1 correctly changed Part 5 to **PARTIAL (gap)**.
- This ledger's `## Status` still says only F-M09-018-1/2/3 require child packages or decisions
  before M09-030, omitting newly added F-M09-018-7, which its own disposition says blocks the M09
  exit gate if unresolved.

Correct both current-status statements and search the active M09-018 records for any other
non-historical schema-2–5/Part-5 PASS claim. Chronological text inside the earlier campaign and
review records may remain as history when a later section clearly supersedes it. No Rust source
change or gate rerun is required; `git diff 0886108..b81ede2 --check` is clean.

---

## R3 resolution and final Tier-C verdict (Codex frontier, 2026-08-23)

R3 is resolved mechanically with the review trail preserved: the active package result now says
Parts 1/3 PASS and Part 5 PARTIAL (gap), and the active ledger status lists findings 1/2/3/7 as
required pre-exit work. A search found no equivalent stale claim outside chronological records that
are explicitly superseded by later sections.

**Final verdict: accepted.** M09-018 accurately records the current implementation and its seven
findings. None blocks the schema-4-only M09-019 package; F-M09-018-1/2/3/7 remain mandatory before
M09-030 can close the milestone. No Rust source changed, so the independently reproduced policy
119/0, training 104/0, Clippy, and known rustfmt results remain applicable.

---

## Pre-M09-030 resolution of deferred findings (2026-08-26)

- **F-M09-018-1, F-M09-018-2, F-M09-018-7 — closed by explicit scope decision.** Python parity is
  no longer an acceptance criterion (`docs/MLP_PLAN.md` §11.3), and no retained accepted schema-2,
  schema-3, or schema-5 artifact exists. `M09_LEARNED_POLICY.md` now states the operative goal and
  exit gate in terms of retained schema-4 champions plus schema-6. These migrations/importers are
  not silently claimed as implemented; they are explicitly unsupported and may only return through
  a future reviewed compatibility package.
- **F-M09-018-3 — resolved in `fda6516`.** Test-only thread-local probes sit on the authored score
  dispatcher and shortlist boundary. The regression first calls `ScoredBot` and requires both probes
  to fire (non-vacuity), resets them, executes learned inference over a real observed choice, and
  requires exactly `(0, 0)` authored-path hits while every legal option is scored.

Focused gate: `cargo test -p ti4-policy
learned_inference_never_enters_the_authored_score_or_filter_paths` — **1 passed, 0 failed**;
`cargo clippy -p ti4-policy --all-targets --no-deps -- -D warnings` — **exit 0**.

**Deferred pre-exit findings are resolved.** This does not accept M09-029 or substitute for either
independent M09-030 Tier-D pass.
