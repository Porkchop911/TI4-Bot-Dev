# OBS-007b — deterministic preview foundation

## Package

- Milestone: Stage 2 complete decision contract, after `OBS-007a`.
- Objective: share rules helpers for exact deterministic costs, limits and immediate deltas; prove
  the preview agrees with application and that a failed application is atomic.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-007b.

## Analytic, not simulated

`deterministic::spend` computes the pool afterwards from the chosen plan's `worth`. It does **not**
clone the state and apply the change. That choice is the whole value of the package: a preview built
by applying would agree with application by construction and the agreement test would check nothing.
Two independent computations that agree is evidence; one computation compared against itself is not.

## The pool falls by worth, not by cost

Exhausting a four-influence planet against a three-influence bill removes four from what remains
spendable. Reporting a fall of three would describe a position the seat is not in — and that gap is
the same one that billed seven influence for two command tokens.

## Tests

- **Agreement**, over four cost-and-kind combinations: preview, then apply the same plan for real
  and re-measure both the pool and the trade goods.
- **Agreement with a second face.** `production::available` counts a planet at its largest face and
  Archon's Gift adds the other kind's printed value; `Plan::worth` reads content alone and cannot
  see a breakthrough the seat holds. Those could diverge, which would make a preview report a pool
  the position does not have. Asserted directly with the breakthrough in play, because the ordinary
  fixture cannot reach it. **It holds** — the risk was real enough to test and did not materialise,
  and the case is now pinned rather than assumed.
- **Atomicity.** A plan naming an already-exhausted planet is refused and takes nothing on the way
  out. `payment::apply` validates before mutating, so this passes today; asserted because a
  half-applied payment would take the trade goods and leave the bill unpaid.
- **Unaffordable is `Unavailable`**, not a zero-change `Certain`, and `expected` returns `None`.
- **A zero cost is a computed no-change**, distinguishable from an absent one.

## Invariants and non-goals

- `spend` takes `&GameState`. Nothing here can mutate a game.
- No producer calls it yet; wiring is OBS-008. No feature family, vocabulary, option set or replay
  artifact changes.
- Only spending is covered. Production limits, capacity and fleet-supply headroom are named in
  `Quantity` and are not yet computed; they arrive with the classes that need them.

## Tests and commands

- `cargo test -p ti4-engine --lib obs007b`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine --all-targets`

## Definition of done

Preview and application agree on independent computations, including with an alternate face; a
refused payment changes nothing; unaffordable is distinguishable from unknown and from free; checks
green.
