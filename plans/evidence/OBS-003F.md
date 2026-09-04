# Evidence — OBS-003f trade context producers

## Scope and provenance

- Branch: continuation, no new branch cut.
- Base: `3496eca` (`OBS-003e` slice 2).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003f`),
  `plans/OBS-003F_TRADE_CONTEXT.md`, LRR 60 and 94.1.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, or committed generated
  artifact.

## Changed paths

- `crates/ti4-engine/src/transactions.rs`
- `plans/OBS-003F_TRADE_CONTEXT.md`
- `plans/evidence/OBS-003F.md`
- `plans/EXECUTION_STATE.md`

## Result

`TradeWindow::pending_choice`'s two stages (`Proposing`, `Answering`) attach typed
`DecisionContext`. This is the entire producer surface `OBS-003f`'s row names, confirmed against
the `OBS-002a` registry (`transactions.rs::pending_choice`, count 2, is the only producer for this
module).

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs003f` | 1 passed |
| `cargo test -p ti4-engine --quiet` | 1,171 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` | 201 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` | 1 passed; 102-game deterministic campaign, 266.91 s (265.26 s for `OBS-003e` slice 2 — no regression) |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine --all-targets -- -D warnings` | passed |
| `cargo fmt -p ti4-engine -- --check` | passed |
| `git diff --check` | passed |

## Counterfactual and agreement evidence

- A real `TradeWindow` opened between two neighbouring seats, driven through a genuine trade-good
  swap offer and its own `resolve` call, proves both subtypes and that each names the *other*
  player as its target (`propose_transaction` targets the partner; `answer_transaction` targets the
  proposer) — the target flips with the perspective, which is the fact a counterpart-relative
  feature will eventually need.

## Decisions made and rationale

- **"Counter" is not a third subtype.** A "counter" answer re-enters `Stage::Proposing` through the
  same state machine with a new offer; the next `pending_choice` call already returns
  `propose_transaction` again, correctly, with no separate handling needed.
- **"Promises" and "replenishment" are not separate producers.** The `OBS-002a` registry names
  exactly one producer for this module (`pending_choice`, count 2); promissory notes are a term
  inside `Terms`, part of the same offer/answer questions rather than their own ask, and
  replenishment is `strategy_cards::gain_tokens`'s family, already typed in `OBS-003e` slice 1
  under `trade_choose_replenish`. Recorded here so the row is not mistakenly re-scoped as
  incomplete by a later reader expecting a producer that does not exist.

## Independent review

**OUTSTANDING.** Tier C. Not yet independently reviewed, alongside `OBS-008c2b`, the
production-discount bug fix, `OBS-008c3`, `OBS-003d`, and `OBS-003e` slices 1 and 2 — seven
packages now owed review.

## Non-goals retained

No features.rs change. No vocabulary generation or bundle republish. No transaction legality or
application change.
