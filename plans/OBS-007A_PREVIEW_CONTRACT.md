# OBS-007a — preview contract

## Package

- Milestone: Stage 2 complete decision contract, after `OBS-002b` and `OBS-003a`.
- Objective: define bounded factual before/after summaries, stochastic outcome descriptors,
  unknown/unavailable states, and an API that cannot mutate the game.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-007a.

## The problem

A policy can see what each option *is* — id, kind, payload — and not what it would *do*. "Produce a
carrier" and "produce a destroyer" differ in cost, fleet supply, capacity and what the seat can
afford afterwards, and none of that is in the option. The consequence is re-derived by the network
from the position, for every option, every time.

## Three rules the type enforces

**Fail closed.** `Outcome::Unknown` carries a reason and `expected()` returns `None` for it. There
is no zero-valued default. A shaping term fed a confident zero learns the action is free, so the
distinction between "computed as no change" and "not computed" is load-bearing and is carried by
`is_informative()`.

**Cannot mutate.** Every entry point takes `&GameState`. Previewing is a question, and a question
that can change the answer is not one. The borrow checker enforces it rather than a convention.

**Bounded.** Capped at `MAX_DELTAS`, with `truncated` saying when the cap bit, so a consumer sizing
a feature block knows the width in advance.

## Decisions worth recording

**Unknown is not Unavailable.** `Unavailable` means the engine would refuse the option;
`Unknown` means it is legal and the consequence was not computed here. Folding them together would
teach a policy that anything unmodelled is illegal — false, and self-reinforcing, because it would
stop choosing exactly the options nobody had modelled yet.

**Both ends of a delta, not a signed change.** Spending three of four resources and three of thirty
are different decisions and a change alone cannot tell them apart. There is a test for that.

**Odds are counts, never floats.** A d10 hitting on 7+ is `weight 4` of `out_of 10`, exact and
comparable. `out_of` is carried rather than re-derived by summing. A distribution with no cases or
zero total weight is refused as `Unknown` rather than invented.

**Deltas are sorted before truncation,** so the same option summarises identically however its
producer happened to build it.

**`Serialize` but not `Deserialize`.** The reasons are `&'static str`, which cannot deserialise, and
that is the point: the set of things a preview can say it does not know stays enumerable by reading
the source rather than becoming free text invented at runtime.

## Invariants and non-goals

- Computes nothing. Deterministic helpers are OBS-007b, stochastic OBS-007c, per-class producers
  OBS-008. Additive and unused on landing.
- No feature family, vocabulary entry, option set or replay artifact changes.

## Tests and commands

- `cargo test -p ti4-engine --lib preview`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine --all-targets`

## Definition of done

An uncomputed consequence cannot become a confident zero; unavailable and unknown are distinct;
deltas are capped and say so; the summary is order-independent; odds carry their denominator; a
degenerate distribution is refused; checks green.
