# M03-001 … M03-005 — The choice model

## Package

| Field | Value |
|---|---|
| IDs | M03-001 (Option/Choice, stable ids), M03-002 (choice validation), M03-003 (Decider interface), M03-004 (simple deciders), M03-005 (DecisionLog) |
| Depends | M02-003/005 (state model) |
| Objective | Give the engine the one shape every decision takes, so that options are generated rather than accepted, and a game replays from a seed plus its log. |
| Permission class | P1, plus read-only oracle inspection. |

The five rows are one module in the oracle and one here; splitting them would mean shipping
a `Choice` nothing can answer, or a `Decider` with nothing to answer.

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, tree clean before and after.
Ported from `engine/choice.py` (240 lines) to `crates/ti4-engine/src/choice.rs`, in full.

## What it establishes

Every decision in the game is the same shape: the engine enumerates legal options and an
actor picks one. Four guarantees fall out of that, each with a test:

| Guarantee | Test |
|---|---|
| The engine is authoritative — options are generated, never accepted from outside | `an_answer_that_was_not_offered_is_rejected` |
| A bot or LLM has no channel to invent a move | `a_table_rejects_an_invented_answer_before_recording_it` |
| A game replays from a seed plus its decision log | `a_seeded_run_replays_exactly_from_its_own_log` |
| An actor sees only the `Choice` it is handed | structural — `Decider::choose` takes a `&Choice` and nothing else |

The replay test is the load-bearing one: it drives 25 choices from `SeededRandom`, feeds the
resulting log back through `Scripted`, and asserts the two logs are equal.

## The duplicate-option problem

`distinct_units` is ported with its rationale intact, because it is not a tidiness measure.
A unit is its type, its owner, and whether it has taken damage; units matching on those are
interchangeable, so offering one option each offers the same move several times. **Deciders
weigh options one by one**, and a sampling bot draws from the option list — so a move written
five times drew five times the probability of an equally good move written once. In the
oracle a player holding five fighters and one dreadnought assigned its hits to a fighter five
times in six regardless of what its scoring thought of the trade, because the count decided
rather than the score.

Damage stays in the key rather than being folded away: losing a ship that has already taken a
hit is a different proposition from losing a fresh one, and collapsing the two would hide a
real choice instead of removing a false one. Pinned by
`interchangeable_units_are_offered_once` and
`a_damaged_unit_is_a_different_option_from_a_fresh_one`.

`the_kept_index_points_at_a_real_element` guards the other half: the returned index must
still address the original slice, or the option id points at nothing.

## Differences from the oracle

| Difference | Reason |
|---|---|
| `Option` is `ChoiceOption`. | A type called `Option` in scope alongside `std::option::Option` is a trap for every later reader. |
| `DECLINE` is `ChoiceOption::decline()`, not a constant. | Its fields are `String`; a `const` cannot hold one. `DECLINE_ID` and `DECLINE_KIND` are constants, and `is_decline` tests the *kind*, so a differently-named declining option still counts. |
| `Decider::choose` takes `&mut self` and returns `Result`. | A decider carries state (a script position, an RNG stream). Returning `Result` lets an exhausted-and-diverged script report which option it wanted, instead of raising through a layer that cannot describe it. |
| An empty option list is `IllegalChoice::NoOptions`, not a panic. | The oracle's `FirstOption` indexes `choice.options[0]` and its `SeededRandom` calls `random.choice`, both of which raise `IndexError` on an empty list. Naming the condition makes an engine bug legible instead of arriving as a panic from inside a decider. |
| **`SeededRandom` does not reproduce the oracle's stream.** | The oracle uses Python's Mersenne Twister; this uses `ChaCha8`, which is reproducible across platforms and Rust versions in a way Python's is not. The same seed plays a *different legal game*. Reproducing a specific oracle game needs its decision log replayed through `Scripted`, or the legacy entropy translator planned in M03-007. Documented on the type. |

That last one is the only semantic divergence, and it is the divergence M03-006/007 already
anticipate — a native pinned RNG plus a translator for legacy entropy.

## Commands and results

```
$ cargo test --workspace
121 passed  (ti4-content)
 72 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
262 total, 0 failed        (232 before this package)

$ cargo clippy --workspace
0 warnings in choice.rs

$ rustfmt --edition 2024 crates/ti4-engine/src/choice.rs crates/ti4-engine/src/lib.rs
clean
```

30 new tests, all in `choice.rs`.

## Open findings

1. **Nothing generates options yet.** This package supplies the shape; the engine still has
   no `legal_options()`. That is M04-005/012 and needs the strategy-phase and action-phase
   option generators from the oracle's `Game._strategy_options` and `_action_options`.
2. **No seeded RNG for the engine itself.** `SeededRandom` here is a *decider*. Deck
   construction, dice, and map generation need the pinned RNG of M03-006, which does not
   exist — which is why setup still builds no decks.
3. **`payload` is `serde_json::Value`.** Typed payloads would be better, but the payload's
   shape depends on the option kind, and inventing that taxonomy before there are real
   option generators is how the previous engine went wrong.
4. **No independent review.** Waived by the project owner.
