# OBS-003f — trade context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`.
- Objective: populate typed `DecisionContext` for offers, answers, counters, and transaction
  limits.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003f`), LRR 60
  (transactions), 94.1 (one transaction per pair per turn).
- Acceptance references: focused `obs003f` test in `transactions.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/transactions.rs`, this specification, package evidence,
  and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

`TradeWindow::pending_choice`'s two stages attach typed context:

- `Stage::Proposing` — Rule 60, subtype `propose_transaction`, target the counterpart.
- `Stage::Answering` — Rule 60, subtype `answer_transaction`, target the proposer. A "counter"
  answer re-enters `Proposing` through the same state machine and is not a third subtype: it is
  the same offer question asked again, from the other side.

Per the `OBS-002a` registry, `transactions.rs::pending_choice` (count 2) is the entire producer
surface `OBS-003f`'s row describes — "promises" and "replenishment" are carried inside `Terms`
(promissory notes as a term of a deal, not a separate ask) and inside `strategy_cards::gain_tokens`
family/`trade_primary`'s replenish ask, already typed in `OBS-003e` slice 1.

## Boundaries

- No option ID, label, legal set, or application-side behavior change. The existing suite in
  `transactions.rs` passed unmodified.
- No features.rs change, matching every other `OBS-003` package: `OBS-008e` reads this into policy
  features later.

## Tests and commands

- One `obs003f_propose_and_answer_carry_typed_context` test proving both subtypes and their
  counterpart-relative targets, driven through a real `TradeWindow` and a real `resolve` call
  (not a hand-built `Choice`), so the typed context matches what a policy actually receives.
- Full engine suite, policy suite plus the 102-game deterministic campaign, training suite, strict
  Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

Both transaction-window decisions state a stable, machine-readable subtype and source; legal sets,
option IDs, and replay/serde behavior are unchanged; all checks and independent Tier-C review pass;
only package files are committed.
