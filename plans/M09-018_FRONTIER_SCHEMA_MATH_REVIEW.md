# M09-018 — Frontier schema/math review (re-execution on current-tree evidence)

## Status

**Independent Tier-C verdict: changes required (2026-08-23, reviewed tip `bd89568`).** The
schema-2 flat-vector import/migration gap is absent from the finding ledger and Part 5 cannot be a
schema-2–5 compatibility PASS when only schema 4 was exercised. F-M09-018-1 also omits two of the
four schema-3 economy successors affected by fallback (`trade` and `tokens`). See the independent
verdict in `plans/M09-018_OPEN_REVIEW_ITEMS.md`. Correct the records and request a fresh recheck;
no Rust source change is required for this review round.

**R1/R2 corrected (records only, 2026-08-23); requesting a fresh independent Tier-C recheck.**
F-M09-018-7 added for the schema-2 flat-vector import gap and Part 5 relabeled PARTIAL (gap);
F-M09-018-1 corrected to name all four successor decision families (`trade`, `tokens`,
`production`, `payment`). Both claims re-verified against pinned `37061c5` and the current tree
before editing. See the R1/R2 resolution sections in the ledger and evidence.

**Campaign complete 2026-08-23; pending independent Tier C frontier review.** Base commit `aa15a39`
(M08 closure). Branch `wp/m09-018-frontier-schema-math-review`.

Result: Parts 1/3/5 PASS on current-tree evidence (hashing bit-compatible with the oracle —
48/48 golden rows re-derived independently, 0 mismatches; softmax max-shifted and pinned by tests;
all real checkpoints load/validate/score through the current API). Part 2 finds **no migration code
exists** for schemas 3→4 or 4→5 (F-M09-018-1 MEDIUM, F-M09-018-2 LOW — no local artifact affected,
all 90 surviving profiles are schema 4). Part 4: feature purity holds structurally but row 014's
instrumented test is absent (F-M09-018-3 MEDIUM). Reconciliation tally: **8 full / 6 partial /
3 absent**. Six findings recorded in `plans/M09-018_OPEN_REVIEW_ITEMS.md`; none blocks the start of
M09-019, whose dependency is this gate's acceptance.

This package **re-executes** the gate. The historical record at `plans/evidence/M09-018.md` is a
hollow checklist — permission class "P1: Write evidence file", no commands, no test results, no
reviewer identity, findings of the form "Hash function partial / Accepted / Full hash in M10+" —
and it predates the entire M06–M08 rework (event-scoped secret scoring, invasion legality,
canonical choice-option ordering) that this code now runs against. The sibling records for rows
001–017 are hollow in the same way (boilerplate acceptance text, no evidence). The crate's own
documentation (`ti4-policy/src/lib.rs`) states what actually exists: `learned`, `features`,
`inference` (rows 001–004, 006, 013) plus the M09-track `progress`. This gate verifies that — and
the rows it does not cover — against the current tree on real evidence.

| Field | Value |
|---|---|
| Milestone | M09 — Fully learned policy |
| Depends | accepted M08-019 (M08 closed at `aa15a39`); historical rows 001–017 as reviewed here |
| Permission class | P1 (plans + evidence; **no source edits** unless a finding requires one, in which case the finding-specific writable path is declared in the ledger before the edit) |
| Review tier | **C — frontier model** (hashing compatibility, schema migration, inference numerics per AGENTS.md and PI_WORK_PACKAGE_STANDARD) |

## Objective

Review the delivered learned-policy code against its five gate areas on current-tree evidence:

1. **Hashing** — `bucket()` bit-compatible with the oracle's `_bucket`; golden corpus real,
   provenanced, and adequate; deterministic across runs.
2. **Migrations** — schema 3→4 economy split and 4→5 other-split behavior present or recorded as a
   gap; runtime head-resolution semantics (`resolved_head`/`head`) consistent with the schemas
   actually carried.
3. **Softmax stability / inference numerics** — temperature handling, non-finite guards at load vs
   inference, sampling determinism under the pinned RNG.
4. **Feature purity** — no authored score/filter/playbook value on any feature-construction or
   scoring path; row 014's "instrumented tests" claim verified against what actually exists.
5. **Compatibility / artifact import** — real branch checkpoints (schema 2–5) validate, load, and
   score through the current `Profile` API; fingerprints preserved.

And produce the reconciliation that makes those reviews meaningful: for each row 001–017, what was
claimed, what exists in the current tree (files, tests), and a verdict of full / partial / absent.

## Campaign parts

### Part 0 — Row-by-row reconciliation (rows 001–017)

For every row: the milestone-plan deliverable, the historical record's claim, the files/tests that
actually exist in `crates/ti4-policy` (and elsewhere where named), and a verdict of **full /
partial / absent**. Verdicts are evidence-based (file paths + test names + results), not inferred
from the hollow records.

### Part 1 — Hashing

- Re-run the golden-bucket test; inspect `crates/ti4-policy/tests/golden_buckets.json` for size,
  coverage of dimension counts, and internal consistency.
- Verify the implementation against its documented spec (blake2b-8, first four bytes LE mod
  dimensions, sign from low bit of byte five) by re-deriving a sample independently.
- Where the package names it as historical context: compare a bounded sample against the pinned
  oracle's `_bucket` (read-only inspection at `37061c5`).

### Part 2 — Migrations

- Determine whether schema 3→4 and 4→5 migration code exists anywhere in the workspace; if absent,
  record a gap finding with severity and disposition (child package), not an implicit pass.
- Verify `resolved_head`/`head()` fallback semantics against each carried schema's head set
  (`DECISION_HEADS`, `SCHEMA3_HEADS`, `STAGE1_DECISION_HEADS`) — including the documented
  divergence that schemas 3/4 route later splits to `other`.

### Part 3 — Softmax stability / inference numerics

- Inspect the inference path in `inference.rs`: temperature division, softmax normalization,
  max-subtraction (or its absence and why it is safe), sampling under the pinned RNG.
- Verify load-time guards (`Profile::validate`) cover every non-finite/temperature failure mode
  that would otherwise surface at inference as NaN or a panic.

### Part 4 — Feature purity

- Grep the feature-construction and scoring paths for authored constants (bot scores, playbook
  values); any constant found must be justified as factual (rules-derived) or recorded as a
  finding.
- Verify what instrumented isolation test row 014 claims actually exists; if absent, record a gap
  finding with disposition.

### Part 5 — Compatibility / artifact import

- Load the real branch checkpoints under `out/` (gitignored local data; read-only) through
  `Profile::validate` + scoring: schema field, mode, faction, head-set conformance per schema.
- Record which schemas are actually present in the surviving artifacts and whether each validates
  and scores without error. Fingerprints (name/schema/faction/dimensions) must round-trip.

## Normative sources

- `docs/MLP_PLAN.md` revision 5, §§2 (current model), 4 (target architecture boundaries), 11.2
  (package map: M09 ends at 018; rows 019+ depend on this gate).
- `plans/M09_LEARNED_POLICY.md` row table and exit gate.
- Accepted Rust code as reviewed here: `crates/ti4-policy/src/{learned,features,inference,intern,progress}.rs`,
  `crates/ti4-policy/tests/golden_*.json`.
- Historical Python reference (named context for Parts 1–2 only): `D:\Projects\ti4-engine` at
  `37061c5`, `engine/learned_policy.py` (`_bucket`, head tables, profile layout). Read-only; no
  command may write into that repository.

## Permission declaration (SCOPED_PERMISSIONS)

```text
Permission class required: P1
Writable paths:
  plans/M09-018_FRONTIER_SCHEMA_MATH_REVIEW.md   (this spec, status updates)
  plans/evidence/M09-018.md                      (re-execution record appended; historical text preserved)
  plans/M09-018_OPEN_REVIEW_ITEMS.md             (new ledger — repo convention)
  plans/EXECUTION_STATE.md                       (checkpoint + corrected handover)
Read-only external paths:
  D:\Projects\ti4-engine @ 37061c5 (engine/learned_policy.py; Parts 1–2 only, non-mutating git show)
Network access: none.
Processes/ports: cargo test/clippy/fmt only; bounded temporary probe under crates/ti4-policy/examples/
  if Part 5 requires it (deleted after the run).
Expected generated artifacts and maximum size: none committed; out/ scratch < 10 MiB, cleaned up.
Destructive actions: none (probe deletion verified by path before removal).
External-state changes: none.
```

## Invariants and compatibility class

- `bucket()` output is an **exact** compatibility surface: existing checkpoints are vectors indexed
  by it; one bit of drift scores every trained profile as noise with no error.
- Profile JSON layout (schema/mode/name/faction/learned.heads) is a persisted artifact contract —
  `semantic` compatibility, validated at load, refused rather than repaired.
- Head routing tables are **exact** against the oracle where the oracle has an opinion; local
  divergences must stay listed in the divergence ledger.

## Explicit non-goals

- No source-code changes (any required fix is a finding with its own declared writable path).
- No reimplementation of absent rows — gaps become scoped child packages, not silent scope growth.
- No P2 artifact generation beyond bounded test runs; no committed fixtures from `out/`.
- No rewriting or relabeling of the historical evidence records (append-only re-execution).
- No M09-019+ work: this gate only establishes whether they may start.

## Tests to add

None expected for a review package. Existing suites are the campaign: `cargo test -p ti4-policy`
(119 tests, including golden-bucket/golden-head routing corpora). A temporary Part 5 import probe
may be added under `crates/ti4-policy/examples/` and deleted after the run (M08-019 precedent).

## Commands to run

```text
cargo test -p ti4-policy                 # full crate suite incl. golden corpora
cargo clippy -p ti4-policy --all-targets # touched-file cleanliness (no source edits expected)
cargo fmt -p ti4-policy --check          # no drift introduced by any probe
git diff --check                         # whitespace hygiene on the commit
```

## Expected evidence

`plans/evidence/M09-018.md` gains a "Re-execution on current-tree evidence (2026-08-23)" section:
per-part results with pasted tool output, the row-by-row reconciliation table for 001–017,
findings cross-referenced to `plans/M09-018_OPEN_REVIEW_ITEMS.md`, and the exact commands run.

## Known traps

- The historical evidence files are hollow — do not treat their "COMPLETE" status as evidence;
  verify against code and tests only.
- `out/` artifacts are gitignored local data: read them, never commit or modify them.
- `cargo test` accepts one positional filter per invocation; run named tests individually.
- Cross-category alias collisions (ML-3): any identifier resolution in Part 5 must be scoped by
  content type / choice kind.
- The M06–M08 rework changed engine behavior underneath this code (event-scoped scoring, canonical
  option ordering) — the review is of the current tree, not of what existed when the hollow records
  were written.

## Definition of done

All five parts executed on current-tree evidence with pasted tool output; row-by-row reconciliation
for rows 001–017 complete with file/test citations; every finding recorded in the ledger with
severity and disposition (fix-in-package / child package / operator decision); evidence appended to
`plans/evidence/M09-018.md`; EXECUTION_STATE checkpoint written; scoped commit made; independent
Tier C frontier review requested.
