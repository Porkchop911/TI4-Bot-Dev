# M05-016…019 — Production

Four plan packages: producing capacity, pricing, payment, placement. Ported from
`engine/production.py`. Oracle `37061c51…`, guard clean before and after.

**464 → 477 tests** (121 content, 287 engine, 68 model, 1 doc). Zero warnings, engine clippy
clean apart from two pre-existing `seating`/`setup` items.

**The tactical action is now complete end to end**: activate, move, capacity, space cannon,
combat, invasion, production. `PRODUCTION_UNRESOLVED` is gone; nothing in the action is
announced as missing.

## Rules with a test

* **68.2** — a unit whose printed cost is below one is produced **two at a time** for that one
  resource. My first version charged `ceil` and yielded one, which makes fighters and infantry —
  the two commonest units in the game — cost double. That is not a rounding detail; it is most
  of an early fleet. Caught by writing `a_fighter_costs_one_and_arrives_in_pairs`.
* **68.1a** — a space dock's production value comes from *its planet's* resources, which is why
  the planet travels with the producing unit rather than the value being read from the unit.
* **68.2/68.3/68.4** — ships go to space, structures to a planet with a producer on it, and
  space is also a placement when a producer sits in the space area.
* **79.2** — one space dock per planet, two PDS.
* **34.3 / 75.2** — a planet card is exhausted for resources **or** influence, never both.
  Pinned by paying influence and asserting the same planet then offers no resources.
* **75.3 / 47.3** — trade goods stand in for either.
* **67.x** — a war sun cannot be produced without its technology.
* An unaffordable cost spends nothing: `identical()` pins that the state does not move.

Paying happens *before* placing, so a unit that could not be afforded never reaches the board
even briefly — otherwise an ability reacting to placement sees something that was never bought.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No faction-specific hulls. | The oracle's `_buildable_for` resolves Sol's Advanced Carrier, L1Z1X's dreadnought and four more. Factions are unimplemented, so every seat builds the generic unit — six factions' distinguishing hulls are unreachable, which the oracle explicitly calls out as flattening faction differentiation. **Worth an early package once factions land.** |
| No production value caches. | The oracle memoises per game state, having measured 30,714 calls to 78 distinct answers. Nothing here is hot enough yet to justify the identity-keyed cache, and it is the kind of thing to add against a benchmark rather than on principle. |
| No laws, technology modifiers, Sling Relay, Integrated Economy, or Bellum follow-ups. | All unimplemented subsystems. |
| No alternate planet payment faces (attachments). | Exploration attachments are unimplemented. |
| Choices asked inline through a `Table`. | Matches `combat.rs` and `invasion.rs`. |

## Open findings

1. **The one-decision-per-step contract remains broken** for combat, invasion and production —
   all three resolve inside a single `step()`. The generic `Window` trait is what fixes it and
   is now the largest architectural debt.
2. **Faction units are the biggest correctness gap in production**, per the note above.
3. Remaining in M05: retreat (011), combat modifiers/rerolls (010), and the
   differential/fuzz/benchmark packages (021–023).
