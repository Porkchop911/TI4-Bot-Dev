# M07-020 — Reopened M07 frontier exit review

## Status

**Campaign complete 2026-08-22; independent Tier C frontier review pending.** All dependencies
met: M07-019 (`c034549`), M07-021 (`5241f2d`), M07-022 (`7f357b6`), M07-023 (`8ba6edc`) — all
accepted and committed. Branch: `wp/m07-020-reopened-frontier-review` from `8ba6edc`. The review
frontier is the exact committed range `b721a9a..8ba6edc` (the four M07 packages over the M06
closure commit: M07-019, M07-021, M07-022, M07-023), with M07-019's diff as the primary subject
and the 021/022/023 hardening chain as its accepted corrections. The implementing agent executed
the full five-part campaign (evidence: `plans/evidence/M07-020.md`): all six nested-window paths
traced and pinned, marker expiry verified against monotonic counters with atomic set sites,
redaction boundary rechecked at the typed seam, registries reconciled with reported gaps, and
every gate reproduced (engine 845+5; workspace 1,319/0 ×2; replay 4/4; Clippy zero new in
frontier). **No actionable findings** — two informational entries (inert reserved Seat fields;
promissory-note redaction gap reaffirmed) and no source edits.

**Accepted 2026-08-22.** Independent Tier C frontier adjudication (Claude Opus 5, independence
limitation recorded per the M06-024 precedent): one blocking finding — R1: F-M07-019-1 had been
escalated to this gate and not answered — plus three documentation findings (R2 false
guards-the-guard claim; R3 missing known-differences ledger; R4 unrecorded M07-019 carries). All
four resolved inside this package with no engine work: **R1 decided option 2** — F-M07-019-1
accepted as known difference KD-2, fix scoped as M08-020 hard-ordered before M08-018; R2 claim
corrected at its site with the deferral written down (ML-1); `plans/KNOWN_DIFFERENCES.md`
created (KD-1…KD-4, ML-1, ML-2); carries folded into the evidence. See
`plans/M07-020_OPEN_REVIEW_ITEMS.md` for the adjudication and resolution.

| Field | Value |
|---|---|
| Milestone | M07 — Factions and Thunder's Edge |
| Depends | accepted M07-019, M07-021, M07-022, M07-023 |
| Permission class | P1 |
| Review tier | C — timing, effect scope, and hidden information (independent frontier reviewer) |

## Objective and frontier

Independently review the M07-019 diff and the accepted M07 frontier against occurrence-scoped
scoring, nested timing, effect expiry, replay, redaction, and scope-registry contracts. Record exact
base/head commits and use accepted Rust specifications plus named official timing rules; Python is
not an acceptance source.

## Scoped access

```text
Writable paths before a finding exists:
  plans/M07-020_REOPENED_FRONTIER_REVIEW.md
  plans/evidence/M07-020.md
  plans/EXECUTION_STATE.md
  plans/M07_FACTIONS_AND_TE.md
Read-only review frontier:
  crates/ti4-engine/src/faction_abilities.rs
  crates/ti4-engine/src/game.rs
  crates/ti4-engine/src/timing.rs
  crates/ti4-engine/src/thunders_edge.rs
  crates/ti4-policy/src/bot.rs
  plans/evidence/M07-019.md
Network/process needs: bounded Cargo test/lint/replay commands only
Generated artifacts: Cargo target output and bounded ignored replay logs only
External-state effects/destructive actions: none
```

Any source fix requires the review ledger to declare a finding-specific writable path before the
edit. A larger fix becomes a blocking child package.

## Required review campaign

- Trace nested scoring-window entry/resume through representative combat, invasion, production,
  activation, agenda, and TE reaction paths.
- Verify effect markers cannot leak across sequence identities or survive decline/error/early exit.
- Recheck observer views and authored-bot inputs for secret identity/eligibility leakage.
- Reconcile implemented/partial/unimplemented registries and confirm no corpus-only behavior was
  silently promoted.
- Reproduce focused, affected-crate, workspace, replay-determinism, lint, and diff gates.
- Record every finding, fix/rejection rationale, rerun, reviewer identity/model, and independence in
  `plans/evidence/M07-020.md`.

No aggregate rollout result substitutes for a timing or redaction decision-boundary test.

## Definition of done

An independent frontier reviewer has reviewed the exact committed frontier; every actionable
finding is resolved and rechecked; all campaigns pass; the accepted faction/TE scope and registries
are reaffirmed; evidence is complete; and M07 has no unresolved finding. Only then may M08-018 begin.
