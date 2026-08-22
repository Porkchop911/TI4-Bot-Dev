# M08-019 — Reopened M08 frontier exit review

## Preparation status

Dependency-safe review specification only. Begin only after M08-018 **and M08-021** are accepted
and committed (the behavioral-distribution suite is required before this gate closes per the
operator's disposition of F-M08-017-1).

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots |
| Depends | accepted M08-018 and M08-021 (hard ordering: the exit gate's "paired-seed behavior remains within approved statistical bounds" clause must be met on real evidence before this review) |
| Permission class | P1 |
| Review tier | C — hidden information, legality, and determinism |

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
