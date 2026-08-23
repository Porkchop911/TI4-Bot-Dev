# M08-019 — Reopened M08 frontier exit review

## Preparation status

**Tier-C recheck verdict: changes required (2026-08-23).** Dependencies met: M08-018
accepted/committed (`45fe569`), M08-021
closed with independent review fully resolved (`f110907`, `e5afb02`, close-out `476e0c4`).
Base commit `476e0c4`; branch `wp/m08-019-reopened-frontier-review`.

**Correction round complete (2026-08-23), pending fresh independent Tier-C recheck.** C1
(invasion landing-option order from the system record's own `planets` array, red-first verified),
C2 (`annexable` threads active content/sources; ordering test moved to in-scope system 58), and
C3 (full 30-seed perturbation rerun: **0/30 seeds diverge**) are resolved; the M08-021 baseline was
rederived once as v3 per the verdict's disposition. See Part 4 of
`plans/M08-019_OPEN_REVIEW_ITEMS.md` and `plans/evidence/M08-019.md`.

**Exact committed frontier under review:** `3c7ddd2..476e0c4` (seven commits: M08-017 ×2,
M08-020, M08-018, M08-021 ×3). Under `crates/`: `invasion.rs` (+390 — the only production
behavior change: ground-combat structure legality), `game.rs` (+84 test module re-pointing),
`bot.rs` (+509 test module only), `ti4-sim/src/behavior.rs` (new measurement module) +
registration, and one dead dependency line dropped from `ti4-sim/Cargo.toml`.

Dependency-safe review specification. Begin only after M08-018 **and M08-021** are accepted
and committed (the behavioral-distribution suite is required before this gate closes per the
operator's disposition of F-M08-017-1).

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots |
| Depends | accepted M08-018 and M08-021 (hard ordering: the exit gate's "paired-seed behavior remains within approved statistical bounds" clause must be met on real evidence before this review) |
| Permission class | P1 |
| Review tier | C — hidden information, legality, and determinism |

## Campaign progress (implementer-driven, 2026-08-23)

| Part | Status |
|---|---|
| Choice traces (action/agenda/combat/decline/empty) | Covered by M08-018's ~45 focused tests in `bot.rs` (committed `45fe569`, inside frontier); re-run in gate reproduction. |
| Redaction probes (opponent secrets, private eligibility, note-holder, payment plan) | Re-executed on current tree by M08-017 (`d69fcb1`, inside frontier); evidence `plans/evidence/M08-017.md`. |
| Determinism / perturbed insertion order | **Executed this campaign.** In-process determinism verified; loader fidelity verified; corpus-layout independence **fails** — exactly two file-order dependencies found and characterized: **F-M08-019-1**. Operator disposition adopted Option A; fix implemented in-package (canonical `researchable()` order + system-record `annexable()` order), red-first tests, M08-021 re-baselined to v2. Pending independent Tier C recheck. |
| Scope reconciliation (bot/guide/capability; off-by-default unchanged) | Done — cancelled rows 008/010/013 have no code added; `progress.rs` zero diff; no Serialize/fixtures/opt-in flags anywhere in the frontier. |
| Gate reproduction (focused/policy/engine/workspace ×2/clippy/fmt/diff --check) | Done pre-fix and re-done post-fix: policy 119/0 · engine 854/0 · workspace 1,335/0 identical ×2 · clippy/rustfmt clean on all touched files · `diff --check` clean. |
| F-M08-019-1 resolution | Implemented (Option A) + M08-021 v2 re-baseline; **pending independent Tier C recheck** before M08 closes. |

## Objective and required campaign

Independently review the exact M08-018/M08 frontier for authored-bot legality, observation
authorization, deterministic choice ordering, explanation reconciliation, and nested scoring-window
behavior. The reviewer must:

- trace representative action, agenda, combat, decline, and empty occurrence choices from typed
  observation through ranking and selected legal ID;
- probe opponent-held secret, private eligibility, note-holder, and payment-plan redaction;
- repeat identical runs under perturbed insertion order and compare choices/explanations/replay;
- reconcile accepted bot/guide/capability scope and confirm no off-by-default behavior changed;
- reproduce focused, policy, engine, workspace, lint, and diff gates; and
- record exact frontier, identity/model, independence, findings, resolutions, and reruns in
  `plans/evidence/M08-019.md`.

## Scoped access

```text
Writable paths before a finding exists:
  plans/M08-019_REOPENED_FRONTIER_REVIEW.md
  plans/evidence/M08-019.md
  plans/EXECUTION_STATE.md
  plans/M08_AUTHORED_BOTS.md
Read-only review frontier:
  crates/ti4-policy/src/bot.rs
  crates/ti4-policy/src/features.rs
  crates/ti4-policy/src/lib.rs
  crates/ti4-engine/src/choice.rs
  crates/ti4-engine/src/game.rs
  crates/ti4-engine/src/objectives.rs
  crates/ti4-engine/src/secrets.rs
  plans/evidence/M08-018.md
Network/process needs: bounded Cargo test/lint/replay commands only
Generated artifacts: Cargo target output and bounded ignored replay logs only
External-state effects/destructive actions: none
```

Review fixes require a finding-specific P1 scope; larger work becomes a blocking child package.
Statistical output may detect regressions but cannot excuse an illegal choice or information leak.

## Definition of done

Every actionable finding is resolved and independently rechecked; legal-choice, redaction,
determinism, explanation, workspace, and scope gates pass; evidence is complete; and M08 has no
unresolved finding. Only then may M09 packages dependent on M08-019 begin.
