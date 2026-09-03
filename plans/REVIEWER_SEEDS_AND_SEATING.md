# How the reviewer must handle seeds and seating

Written for whoever maintains `ti4-review`. The goal is one property, and everything below exists
to serve it:

> **A review is reproducible from its manifest alone, and a batch of reviews covers the game rather
> than one corner of it.**

Today it fails both halves.

## What is wrong now

Two runs, seeds 501 and 502, otherwise identical:

```text
501 seating: seat0 sol, seat1 letnev, seat2 xxcha, seat3 hacan, seat4 jolnar, seat5 l1z1x
502 seating: seat0 sol, seat1 letnev, seat2 xxcha, seat3 hacan, seat4 jolnar, seat5 l1z1x
501 speaker: seat0        502 speaker: seat0
```

Identical. And it is not that the shuffle is missing: `seeded_faction_order` exists in
`ti4-review/src/lib.rs`, is a correct Fisher-Yates over a SplitMix64 stream, and is **uniform** —
measured over 60,000 seeds every faction lands in every seat 16.4–17.0% of the time, against 16.7%
expected. For seed 501 it returns `xxcha, letnev, jolnar, sol, hacan, l1z1x`, so seat 0 should be
Xxcha. The recorded game has Sol.

So the permutation is computed and then lost somewhere between `seeded_faction_order` and the
recorded state. `setup_game_with_decider_factory` passes the map to `seated`, and `seated` does
apply it (`seat.faction = faction.clone()`). The break is between those two facts and is the first
thing to find.

**Why it matters beyond tidiness.** Every review anyone has read was played with one seating. Sol
has always been seat 0 and always been first speaker. Any impression formed from these reviews
about how a faction plays, or about turn order, is an impression of one arrangement — and the
per-faction differences we chase in training are exactly the thing a fixed seating confounds.

## The rules

### 1. One public seed, and everything derives from it

A review names a single `seed`. Every random quantity is derived from it, and nothing else is a
source of randomness. No wall-clock, no thread id, no iteration counter, no global RNG.

### 2. Derive independent streams by domain separation

Never feed the raw seed to two different consumers. Each domain mixes the seed with its own
constant before use:

| stream | what it decides |
|---|---|
| seating | which faction sits in which physical seat |
| map | which arrangement is drawn from the pool |
| deck | strategy cards, objectives, action cards, promissory |
| dice | every roll |
| policy | the sampling stream per seat |

This is already the intent in the code — `seeded_faction_order` mixes with `0x4641_4354_494f_4e53`
and `seated_faction` with the golden ratio constant, precisely so "seating does not move in lockstep"
with the deck. Keep that, and keep the constants distinct and written down.

The property to preserve: **changing the seed changes all of them, and changing one consumer's code
does not disturb another's stream.**

### 3. Seating is a permutation, and it must actually vary

- Seating assigns the six factions to the six physical seats as a **uniform permutation** drawn from
  the seating stream.
- Two different seeds must, with high probability, produce different seatings.
- The same seed must always produce the same seating, on any machine, in any build.
- Seating is fixed for the whole game once drawn. It never changes mid-game.

### 4. Rotation is a separate, explicit knob — and it is not a substitute for seating

`--rotation 0..5` cyclically shifts the permutation across physical seats. It exists to
counterbalance *position* (adjacency, distance to Mecatol) while holding the faction permutation
fixed, which is what makes paired comparisons possible.

Two consequences:

- Rotation defaulting to 0 is fine for a single named review.
- **A batch must sweep rotation, not inherit the default.** A batch that leaves rotation at 0 and
  varies only the seed is testing one sixth of the positional space.

### 5. Speaker

- The **initial** speaker is drawn from the seed, like seating. It must not be hard-wired to seat 0.
- After that the speaker moves **only by the rules** — Politics, and agenda effects. The token does
  not rotate each round.
- The reviewer must record who the initial speaker was, and show every subsequent change with the
  effect that caused it.

### 6. Turn order is initiative order, and the reviewer must show it

The engine is right about this: `phase::advance_turn` walks `state.initiative_order()`, and there is
a test named `the_turn_follows_initiative_order_not_seating_order`. The reviewer must not present
players in seating order and leave the reader to infer the rest — that is what makes it look as
though nothing but the speaker token ever moves.

Each round the review should show:

- the strategy card each seat holds and its initiative number,
- the resulting turn order,
- the speaker, and any change to it with its cause.

### 7. The manifest must be sufficient to re-run the review exactly

It records, at minimum:

- `seed`, `rotation`,
- the **resolved** seat-to-faction map (not the canonical lineup — the one actually played),
- the initial speaker,
- map pool path and the identifier of the arrangement drawn,
- checkpoint identity (path plus content hash) and sampling temperature,
- engine commit,
- source set / expansion scope.

Recording the canonical faction list is not enough and is part of how this went unnoticed: the
manifest reads `["sol","letnev","xxcha","hacan","jolnar","l1z1x"]` for every review, which is the
lineup, and says nothing about seating.

**Acceptance:** re-running from a manifest reproduces the frames exactly. If it cannot, the manifest
is missing a field.

## Tests that would have caught this

1. **Seating varies.** Two seeds that differ produce different seat-to-faction maps, asserted on the
   *recorded state*, not on the helper's return value. This is the one that matters: the helper is
   already correct and already unit-testable, and the bug is that its result never reaches the game.
2. **Seating is stable.** The same seed reproduces the same seating.
3. **Seating is uniform.** Over a few thousand seeds each faction occupies each seat within
   tolerance of one sixth.
4. **The seed reaches the game.** A round-trip: derive the expected seating for a seed, run the
   review, assert the recorded seat-to-faction map equals it. Nothing weaker will do — every layer
   here was individually correct and the composition was not.
5. **The manifest is sufficient.** Re-run from a written manifest and compare frames.

Test 4 is the general lesson from this project, in its fifth instance today: `enforce_everywhere`,
`return_support`, `status_tokens`, `baseUpgrade` and now `seeded_faction_order` were all implemented
correctly and reached by nothing. A unit test proves a function works; only an end-to-end assertion
proves anything calls it. See `ti4-engine/src/wiring.rs`, which was written for exactly this failure
mode and whose checklist should grow to cover these.

## Separately: the narrative is unreliable

Not a seeding issue, but it will mislead anyone reading reviews and should be fixed alongside.

The action summary is a state diff, and it mispairs units when several land on different planets in
one action. In seed 501 it reported `Moved 1 infantry from Siig to Kraag` — a planet-to-planet move
that never happened and that no rule allows — and placed a casualty `at Kraag/Siig space` when the
unit actually disappeared from a planet. The decision log beneath it was correct and legal
throughout.

The summary should be built from the engine's own events and the decision log, not reconstructed by
differencing board states.
