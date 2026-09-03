# OBS-004 — complete actor-owned inventory

## Package

- Milestone: Stage 2 complete decision contract, after `OBS-002b`.
- Objective: extend the seat-bound view with the actor's own private holdings, add the public rule
  modifiers the matrix found missing, and prove by mutation that neither leaks an opponent's hand.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-004;
  `plans/evidence/OBS-002B_RULE_DEPENDENCY_MATRIX.md`.

## What the matrix asked for, and what was already there

`Observed` already carried `custodians_removed()`. The matrix's finding that no feature mentions
custodians was therefore a **feature-layer** gap, not an observation one, and nothing is added for it
here.

Laws were a genuine observation gap: no accessor at all, against fourteen producers reading law,
agenda and elected-target state for legality or application.

The private side was also already careful. `PublicSeat` exposes `action_cards_held` and
`secret_objectives_held` as counts, and `Observed::promissory_notes` exposes only the faceup subset.
What was missing was the bound seat's own *contents*.

## Added

On `SeatObservation`, no-argument and bound at construction, so no caller can name another seat:

- `held_action_cards()` — the seat's own hand.
- `held_promissory_notes()` — notes held and not yet faceup.

On `Observed`, public to every seat:

- `laws()` — alias to elected outcome.
- `law_outcome(alias)`.

## Leak tests

Four, and the mutation is the point of the first. A view that merely *happens* to hold the right
cards proves nothing: it could be reading a snapshot taken before the opponent acted. So the
opponent's hand is grown mid-test and the bound seat re-read.

- An opponent drawing three cards changes nothing the bound seat sees, while the public count moves
  from 2 to 5, because a hand size is public and its contents are not.
- A promissory note in hand is visible to its holder and absent from `promissory_notes()`; playing
  it face up reverses both.
- An opponent's unplayed note is not listed for anyone else.
- A law reads identically from either seat, because a standing law binds the table.

## Invariants and non-goals

- Accessors only. No feature family, vocabulary entry or bundle changes; wiring these into features
  is OBS-008 and OBS-011.
- No `SeatObservation` accessor takes a player argument, and none is added here.
- No legal option set, option id, prompt or replay artifact changes.

## Tests and commands

- `cargo test -p ti4-engine --lib obs004`
- `cargo test -p ti4-engine`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine --all-targets`

## Definition of done

The bound seat can read its own hand and unplayed notes and no other seat's; laws are readable and
identical from every seat; the opponent-mutation test passes; checks green.
