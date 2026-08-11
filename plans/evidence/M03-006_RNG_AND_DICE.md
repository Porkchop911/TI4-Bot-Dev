# M03-006 — The pinned RNG, and dice

## Package

| Field | Value |
|---|---|
| IDs | M03-006 (native pinned RNG with domain separation), plus `engine/dice.py` |
| Depends | M03-001…005 (choice model) |
| Objective | Give the engine one seeded random source, split by purpose, so that a game is reproducible from its seed and its decision log together. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, tree clean before and after.
`engine/dice.py` (97 lines) ported in full to `crates/ti4-engine/src/dice.rs`.
The generator itself is **not** a port — see below.

## Why this is a native generator, not a port

The oracle uses Python's Mersenne Twister through `random.Random(seed)`, both for dice and
for every `rng.shuffle` that builds a deck. That stream is not reproducible outside CPython,
so M03-006 specifies a *native pinned* generator rather than a translation. This uses
`ChaCha8`, seeded per domain.

**Consequence, stated plainly:** the same seed produces a different — equally legal — game
from the oracle's. Reproducing a *specific* oracle game needs its decision log replayed
through `Scripted`, or the legacy entropy translator planned in M03-007. This is documented
on `GameRng` itself, not only here.

## Domain separation

One stream for the whole game couples every random decision to every other. Adding a die roll
early in a round would shift the agenda deck, the exploration deck, and every later roll; a
seed-pinned regression test would then fail for reasons unrelated to what changed, and a fix
that altered the *number* of rolls would silently renumber every later draw.

So each purpose draws its own stream, seeded `SHA-256(seed_le_bytes || domain_name)`. Hashing
rather than adding or XOR-ing the domain in, so that two domains whose names differ by one
character do not produce related streams — pinned by
`domains_that_differ_by_one_character_are_unrelated`, which asserts more than 20 of 32 seed
bytes differ.

The property that matters is `drawing_from_one_domain_does_not_move_another`: 1,000 dice and
a relic shuffle happen, and the agenda deck still comes out in exactly the order it would
have without them.

Eight domains are named: dice, five card decks, exploration, and map selection.

## Dice

Ported faithfully, including the two details the oracle documents:

* **A reroll returns a new `Roll` and both stay in the history.** The sequence of draws from
  the generator is part of what a seed reproduces, so a reroll that overwrote its predecessor
  would make the log disagree with the game. Pinned by
  `a_reroll_is_recorded_alongside_its_predecessor`.
* **`rerolled` records which positions were replaced.** Some abilities care which dice were
  rerolled rather than only what they now show — the Crown of Thalnos destroys "each of their
  units that did not produce a hit with its reroll", which is unanswerable from the faces
  alone.

Positions outside a roll are ignored rather than refused, because abilities name dice by unit
and a unit may already have been destroyed by the time they resolve.

`a_shuffle_reaches_every_position` guards the classic Fisher-Yates off-by-one: written with
the wrong bound it leaves the first element fixed, and a deck whose top card never moves is a
deck that always reveals the same thing. It samples 50 seeds and requires more than 10
distinct top cards.

## Difference from the oracle

| Difference | Reason |
|---|---|
| `Dice` does not own its generator; the caller passes `&mut GameRng`. | So dice share one game's seed and draw from the `dice` domain. A `Dice` with its own `random.Random(seed)`, as the oracle has, is a second uncoordinated stream. |
| `hits_on` is `Option<u32>` and a roll with none hits nothing. | Same as the oracle, but explicit: an exploration or ability roll is not a hit roll, and `hits()` returning 0 for it is a decision, not a fallthrough. |

## Commands and results

```
$ cargo test --workspace
121 passed  (ti4-content)
 96 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
286 total, 0 failed        (262 before this package)

$ cargo clippy --workspace
0 warnings in rng.rs or dice.rs
```

24 new tests. `sha2` added to `ti4-engine` for the domain derivation.

## Open findings

1. **Nothing builds decks yet.** The RNG exists; the six deck builders
   (`objectives.starting_deck`, `agenda.build_deck`, and four more) are the next step, and
   they are what will make `start_game` complete. Until then a game starts with empty decks.
2. **Nothing rolls dice yet.** There is no combat.
3. **No independent review.** Waived by the project owner.
