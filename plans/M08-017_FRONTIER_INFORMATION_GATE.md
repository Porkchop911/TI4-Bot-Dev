# M08-017 — Frontier information/review gate (re-execution)

## Status

**Accepted 2026-08-22; F-M08-017-1 open pending operator decision.** Base commit
`3c7ddd2` (M07 closure). Branch `wp/m08-017-frontier-information-gate`.

Result: Parts 1–3 PASS on current-tree evidence (hidden information, parameter leakage,
determinism — all clean; game-level determinism pins already existed, so no scope extension was
used). Part 4 FAILS: M08-015/016 were never built, so the exit-gate clause "paired-seed behavior
remains within approved statistical bounds" is unmet as written. Reconciliation tally: 7 rows
delivered, 2 partial, 7 absent.

**Independent Tier C frontier adjudication (Claude Opus 5 — genuinely independent here: no prior
involvement with M08): Accept.** Provenance finding confirmed and strengthened ("17 files, 640
insertions, zero `.rs` files"); all 16 row verdicts spot-checked; Parts 1–4 reproduced. S1
(MEDIUM — Part 4's search under-scoped; dead `criterion` dependency in the normal build graph)
and S2 (LOW — row 010 carries row 009's misattribution shape) applied in-package, with a declared
scope extension for the two-line manifest fix. ML-1 bounding note added to the ledger.
**F-M08-017-1 stays open pending an operator decision**: the reviewer declined to make the scope
call alone and escalated it with a complete recommendation (option c hybrid — cancel 008/010/013
with corrected rationale, no action on 009, defer-or-do 012, waive 014, **require 015 before
M08-019 closes**, waive 016). When the decision lands it is recorded in `plans/KNOWN_DIFFERENCES.md`
and the M08 scope ledger with its reasoning. See `plans/M08-017_OPEN_REVIEW_ITEMS.md`.

This package **re-executes** the gate. The historical record at
`plans/evidence/M08-017.md` (committed 2026-08-11 in `3180f0e`) is a hollow checklist: no
commands, no verification, no reviewer identity, and it was committed **before any of the code it
reviews existed** — that commit's diff is evidence files only. The crate's own documentation
(`ti4-policy/src/lib.rs`) states what M08 actually implemented: `view`, `valuation`, `scoring`,
`bot` (rows 001–004, 011) plus the M09-track learned modules. This gate verifies that against the
current tree and reviews the four areas on real evidence.

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots |
| Depends | accepted M07-020 (M07 closed); historical rows 001–016 as reviewed here |
| Permission class | P1 (plans + evidence; test-module scope extension only if Part 3 requires it) |
| Review tier | **C — frontier model** (hidden information, determinism, and a milestone-scope decision per AGENTS.md) |

## Objective

Review the delivered M08 bot against its four gate areas on current-tree evidence:

1. **Hidden information** — no leakage of private state to the policy layer; redaction at the
   typed seam; known limitations carried accurately (ML-1, KD-4).
2. **Parameter leakage** — no accidental parameter capture or dead knobs in the authored bot;
   deterministic named-component aggregation.
3. **Determinism** — stable seeded sampling end to end; decision path free of wall-clock and
   hash-order dependence.
4. **Statistical acceptance** — paired-seed behavior within approved bounds, as the milestone exit
   gate requires.

And produce the reconciliation that makes those reviews meaningful: for each row 001–016, what was
claimed, what exists in the current tree (files, tests), and a verdict of full / partial / absent.

## Campaign parts

### Part 0 — Row-by-row reconciliation (rows 001–016)

For every row: claimed deliverable (milestone plan), historical evidence claim, actual state
(current files + test counts, pasted command output), verdict. Provenance check of `3180f0e`
(evidence-only commit claiming "M08 COMPLETE") recorded with its own admission note.

### Part 1 — Hidden information (critical)

- `view.rs`: redaction semantics and `leaks()` re-run on the current tree; agreement between the
  engine (`choice.rs::redacted_for`) and policy (`view.rs::redact_player`) implementations.
- Raw path blindness: proof that `bot.rs` / `scoring.rs` / `valuation.rs` never read
  `action_cards` or `secret_objectives` (grep + structural argument — the raw dispatcher receives
  only `choice` and `option`).
- Seen path: all position-sensitive scoring goes through the typed `Observed` seam; spot-check the
  seen scorers for anything beyond what `Observed` exposes.
- Known limitations carried: ML-1 (`leaks()` two-field mirror), KD-4 (promissory-note holdings
  visible) — from `plans/KNOWN_DIFFERENCES.md`.

### Part 2 — Parameter leakage (high)

- `Components`: named parts, deterministic aggregation and ordering, `explain()` reproduces the
  total.
- `ScoredBot` field audit: every field read by a live code path; no dead knobs. The harvesting trap
  is documented in `progress.rs`'s module doc (identifier-shaped keys silently became tunable
  weights in the oracle) — verify the Rust equivalent holds: `Progress` lives outside the bot, and
  no struct field on the policy side is written but never read.

### Part 3 — Determinism (high)

- Sampling path audit: `ChaCha8Rng::seed_from_u64`, BTreeMap ordering in scores/shortlist/sample;
  no wall-clock or hash iteration anywhere a choice is made.
- Existing pins re-run: `the_same_seed_makes_the_same_choices`, temperature behavior, legality
  boundary (`a_bot_only_answers_with_an_option_it_was_offered`).
- Gap check: does any test pin **game-level** determinism (same seed + same game → identical
  decision log)? If not — finding; a single focused test may be added under `crates/ti4-policy`
  (test module only, declared here as the sole permitted code scope extension).

### Part 4 — Statistical acceptance (medium)

- Verify whether M08-015's behavioral-distribution suite and M08-016's benchmark exist in any form
  (grep + file search, pasted output). If absent: they cannot be accepted; the milestone exit-gate
  clause "paired-seed behavior remains within approved statistical bounds" is unmet as written.
  Record as a finding with explicit options for the adjudicator — this gate reviews and reports; it
  does not implement missing rows or rewrite the exit gate itself.

## Scoped access (declared before any finding exists)

```text
Writable paths:
  plans/M08-017_FRONTIER_INFORMATION_GATE.md
  plans/evidence/M08-017.md          (re-execution record; supersedes the 2026-08-11 checklist, which is quoted in it)
  plans/EXECUTION_STATE.md
Read-only review frontier:
  crates/ti4-policy/src/*.rs
  plans/evidence/M08-0*.md (historical rows), plans/M08_AUTHORED_BOTS.md, plans/KNOWN_DIFFERENCES.md
Permitted scope extension (only if Part 3's gap check fails):
  crates/ti4-policy — test module only, one focused game-level determinism pin
Scope extension declared at review resolution (S1 required action, before the edit):
  crates/ti4-sim/Cargo.toml   — drop the dead `criterion.workspace = true` line from [dependencies]
  Cargo.toml                  — drop the orphaned workspace.dependencies entry `criterion = "0.5"`
  (nothing in the workspace imports criterion; no [[bench]] target exists anywhere)
  plans/KNOWN_DIFFERENCES.md — ML-1 entry only: add the reviewer's bounding note that nothing on
  the bot side consumes `event_feats` / `scored_feat_occurrences` ("latent leak with no reader")
Network/process needs: bounded Cargo test/lint commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

## Non-goals

- No implementation of undelivered rows (008–016) — that is scope for the adjudicator's decision,
  not this gate.
- No review of the M09-track learned modules (`features`, `inference`, `learned`) or the training
  loop — separate track with its own evidence.
- No re-review of all 46 post-`3180f0e` commits; the four areas are verified against current code.

## Definition of done

Reconciliation table complete for all 16 rows; four areas reviewed with pasted command output;
findings ledger written (including the integrity finding on the hollow historical evidence and, if
confirmed, the statistical-acceptance gap); `plans/evidence/M08-017.md` rewritten as a real gate
record quoting what it supersedes; independent Tier C frontier adjudication obtained at
`plans/M08-017_OPEN_REVIEW_ITEMS.md`; all actionable findings resolved and rechecked.
