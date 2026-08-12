# M04-016 — The status phase's command-token gain (LRR 81.5)

## Package

| Field | Value |
|---|---|
| IDs | M04-016 — status token allocation, the first half of the status choice boundary |
| Depends | M03-001…005 (choice model), M04-010 (status bookkeeping), M04-012 (step driver) |
| Objective | Give the status phase its real 81.5 decision window, so a driven game stops only at the one status step that is genuinely unimplemented. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Ported from `engine/game.py`: `gain_tokens` (LRR 52.4) and the 81.5 call site inside
`_status_phase`.

## What this closes

Every one of the 100 seeded runs in M04-014 stopped at `StatusChoicesUnimplemented`, an error
covering *two* unrelated gaps: objective scoring (81.1) and token allocation (81.5). The second
is fully portable today; only the first needs machinery that does not exist yet.

Splitting them means a driven game now performs 81.5 for real, and the remaining boundary names
exactly one missing rule:

```rust
#[error("status objective scoring (LRR 81.1) is not implemented")]
StatusScoringUnimplemented,
```

## One choice per token, not one per player

The oracle asks once per token, and the reason is in its own docstring: this is one rule shared
by Leadership and by the status phase, and "having the status phase quietly dump them all into
the tactic pool was an inconsistency, not a simplification". LRR 52.4 places tokens
individually, so a player gaining two may split them between pools. Asking once for a count
would make that legal play unrepresentable.

`TokenGain` therefore holds one pending entry per token, and `a_player_may_split_their_tokens_across_pools`
pins it.

The window lives in `tokens.rs` rather than in `status.rs` precisely because Leadership will
need the same window, not a second copy of it.

## The status phase had to be split

81.5 sits between 81.4 (tokens come off the board) and 81.6 (cards ready). The existing
`resolve_status_phase` was atomic and choice-free, so a choice could not be inserted where the
rules put it.

It is now two halves — `resolve_before_token_gain` (81.2–81.4) and `resolve_after_token_gain`
(81.6–81.8) — with the driver running the window between them. `resolve_status_phase` remains
for callers with no decider and simply runs both.

`the_two_halves_compose_into_the_whole` is the test that justifies the refactor: driving the
halves by hand must produce a byte-identical state and an equal report to the single call, or
the phase would silently depend on how it was run.

`StatusPhaseReport` gained `initiative_order`, because steps 81.3 and 81.5 both need it and
81.8 destroys it. Reading it afterwards yields seating order — pinned by
`initiative_order_is_captured_before_step_818_destroys_it`.

## A pre-existing test was self-fulfilling

While pinning initiative order, the ids in the existing status tests turned out not to name real
cards: the corpus calls them `pok1leadership` and `pok8imperial`, not `leadership` and
`imperial`. An unknown card sorts at initiative 99, so both players tied and the order fell back
to seating.

The old test computed its expected order *from* `initiative_order()`, so it passed either way and
proved nothing about ordering. Both tests now use real ids, and the new one asserts the
substantive fact — Leadership (1) precedes Imperial (8), against seating order.

No production code was wrong. The test was.

## Differences from the oracle

| Difference | Reason |
|---|---|
| The window is a value the driver steps, not a loop that calls `ask`. | The Rust driver resolves one decision per `step()` so a caller can inspect between them; a loop would resolve all of a player's tokens inside one step. Same questions, same order. |
| Token count is the constant `STATUS_TOKENS` (2). | Sol's Versatile, Cybernetic Enhancements and the L1Z1X note all modify it in the oracle and none are implemented. They will modify the count passed to `TokenGain::for_status`, not the constant. Stated so the gap is visible rather than looking like a fixed rule. |
| Events are strings on `Game`. | Matches the existing driver; the oracle's structured `_emit` has no counterpart here yet. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
142 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
332 total, 0 failed        (320 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in tokens.rs, status.rs or game.rs

$ cargo fmt --all      # clean
```

12 new tests: 9 in `tokens.rs`, 2 in `status.rs`, and the rewritten driver boundary test plus a
pool-destination test in `game.rs`.

The 100-seed campaign from M04-014 still passes unchanged, now reaching the narrower
`StatusScoringUnimplemented` boundary.

## Open findings

1. **81.1 objective scoring is still unimplemented**, and is now the only thing stopping a
   generic game from completing a round. It needs `objectives.scoreable` — a predicate registry
   of roughly forty requirements — plus `award`, the secret-objective window, and the 98.7
   victory check. That is the next package and it is substantial.
2. **Agenda voting remains unimplemented** (`AgendaChoicesUnimplemented`), untouched here.
3. **No faction, promissory or technology modifiers** to the token count, as above.
4. **No independent review.** Waived by the project owner.
