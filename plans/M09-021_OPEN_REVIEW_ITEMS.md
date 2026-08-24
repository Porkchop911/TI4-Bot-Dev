# M09-021 open review items

| ID | Severity | Item | Status |
|----|----------|------|--------|
| O-M09-021-1 | LOW (documentation) | `plans/evidence/M09-019.md` W2 numbers predate this package; the post-change extraction-cost measurement in `plans/evidence/M09-021.md` is the current reference. No M08-021 behavioral re-baseline triggered (game-level distributions unchanged; authored bot uses the untouched legacy hashed path). | Open — reviewer to confirm no bound is affected. |
| O-M09-021-2 | INFO | Two small pre-existing rustfmt drifts inside files this package edits (`choice.rs` `strategy_card_goods`, `features.rs` `owed` chain) became fmt-conformant via the whole-file format pass; pure line-wrapping, no semantic change. Out-of-scope engine files with pre-existing drift (`action_cards.rs`, `exploration.rs`, `strategy.rs`) were restored to HEAD after formatting. | Open — reviewer to confirm the in-file fixes are acceptable or should be reverted. |
| O-M09-021-3 | INFO | Crossed emission is an architectural reconciliation with accepted StateCross (MLP §5.1 bare names preserved as fact-name portion). Recorded in spec + evidence; not a deviation, but flagged for the frontier reviewer to confirm the reconciliation stands. | Open — reviewer confirmation requested. |
