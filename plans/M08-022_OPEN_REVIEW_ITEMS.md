# M08-022 independent Tier B review — ground-force predicate vs corpus flag (T1 / KD-5)

## Status

**PENDING INDEPENDENT REVIEW.** Implementation complete on branch
`wp/m08-022-titans-pds-ground-force-predicate`, commit `448540c` from base `476e0c4`. The
implementer (Qwen 3.8 27B via Pi) must not be the sole reviewer; no acceptance is claimed here.

| Field | Value |
|---|---|
| Review tier | B — independent Qwen + milestone integration test; **escalates to frontier** if the predicate change reclassifies any unit of a currently-rostered faction, or if the Naaz decision is contested (spec escalation clause) |
| Base | `476e0c4` (M08-021 close-out — accepted line tip) |
| Diff under `crates/` | `units.rs` +67/−6: one predicate body, one doc comment, three tests (one extended, two new). Nothing else. |
| Ledger note | This ledger was added in a focused follow-up commit after `448540c`, per the repo convention that every package ships its open-review-items file; it is review scaffolding only — no code or other plans changes. |
| Gates (implementer-run, pasted in evidence) | ti4-content **129/0**; workspace **1,335/0 identical ×2** (per-test lists byte-identical); clippy -p ti4-content --all-targets zero warnings; rustfmt clean; `git diff --check` clean |

## What to verify

### 1. The decision is right (the load-bearing judgment)

Union semantics: `flag("isGroundForce") || base_type ∈ {infantry, mech}`. The reviewer must
independently check the Naaz question against LRR 42/49 + printed ability text: are the two
unflagged space mechs (`naaz_mech_space`, `absol_naaz_mech_space`) ground forces? Implementer's
answer: **yes** — they are mechs that fight on planets; their ability adds ship status *in
space* and does not remove ground status. The corpus flag marks "ground force beyond the standard
infantry/mech", so it deliberately omits them; a bare `flag()` would have dropped both. If the
reviewer disagrees, the escalation clause applies (frontier) — but note even a different answer
stays dormant under D11's six-faction roster since Naaz is not on it.

### 2. The fix is exactly one classification change

`only_the_hel_titan_i_changes_classification` sweeps all **125** embedded unit records (pok 47,
base 54, codex3 3, thunders_edge 21 — a strict superset of the M08-020 reviewer's 46-record
comparison) and asserts that any record whose classification differs from the pre-M08-022
predicate must be `titans_pds`. Consequence: **no unit of any currently-rostered faction is
reclassified** → escalation clause not triggered. Verify the test actually bites (it fails if a
second record ever changes).

### 3. Red-first evidence is genuine

Pre-fix run: 126 passed / 2 failed — `the_titans_pds_is_a_ground_force_despite_being_a_structure`
(extended to both Titans PDS records vs corpus) and the decision-table sweep, both panicking on
`titans_pds`. Post-fix: 129/0. Exact output pasted in `plans/evidence/M08-022.md`.

### 4. The decision table is now an executable invariant

`the_ground_force_predicate_agrees_with_the_recorded_decision_table` re-derives the expected
classification for every record from (flag ∪ base-type match) and compares to the predicate — so
any future corpus edit that breaks the agreement fails loudly, rather than silently drifting like
the hardcoded id did.

### 5. Gates reproduce

Re-run: `cargo test -p ti4-content`, `cargo test --workspace` ×2 (compare per-test result lists),
`cargo clippy -p ti4-content --all-targets`, rustfmt check on units.rs, `git diff --check`.

## Disposition on acceptance

- Remove **KD-5** from `plans/KNOWN_DIFFERENCES.md` (spec: "on acceptance and commit").
- Update `plans/EXECUTION_STATE.md` milestone state; M08-022 ✅.
- D11 roster widening remains hard-blocked until this is accepted.

## Cross-reference

Sister pending package on a disjoint branch: **M08-019** resolution commit `9a8f5fd` (branch
`wp/m08-019-reopened-frontier-review`) — Tier C recheck of the F-M08-019-1 Option A fix + M08-021
v2 re-baseline. Disjoint file sets; reviewable and mergeable independently, in either order.
