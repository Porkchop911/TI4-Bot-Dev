# M04 — Game skeleton

## Goal

Run deterministic generic-faction games from setup through a bounded finish without faction effects.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
|---|---|---|---|---|
| M04-001 | Hex coordinates | M02 | `engine/galaxy.py` | Coordinates, distance, neighbors, deterministic serialization; property tests. |
| M04-002 | Galaxy/content mapping | 001 | galaxy/content modules | System placement, planets, anomalies, home systems; setup fixtures match. |
| M04-003 | Deck construction | M02,M03 | game/deck modules | Versioned seeded shuffle or explicit entropy; deck conservation and golden orders. |
| M04-004 | Faction seating/setup | 002,003 | `factions.py`, `game.py` | Player order, homes, fleets, starting tech/tokens/cards; 3p/6p projections match. |
| M04-005 | Strategy-card draft | 004 | `strategy.py`, `game.py` | Legal picks, cards per player, speaker order, deterministic choices. |
| M04-006 | Phase state machine | 004 | `game.py`, `timing.Phase` | Strategy/action/status/agenda transitions with explicit legal commands. |
| M04-007 | Turn order and passing | 005,006 | game action loop | Initiative, active player, strategic-action-before-pass, multi-card handling. |
| M04-008 | Generic strategy primary | 005–007 | `strategy.py` | Structural primary flow and exhaustion without content-specific bonuses. |
| M04-009 | Generic strategy secondary | 008 | `strategy.py` | Eligibility, strategy-token payment, decline, completion tracking. |
| M04-010 | Status phase | 006,007 | `game.py`, objectives structural flow | Ready/reset/reveal/order bookkeeping, without full scoring semantics. |
| M04-011 | Agenda structural phase | 006 | `agenda.py` | Reveal, voting order, resolution placeholder only where Python is structural. |
| M04-012 | Step/run API | 006–011 | `Game.step`, `Game.run` | One-decision stepping, round horizon, finish/error metadata, event observation. |
| M04-013 | Random-legal bot | M03,012 | seeded random behavior | Seats all players and never fabricates an action. |
| M04-014 | Generic completion suite | 012,013 | game/sim smoke tests | 100 seeds terminate or return an explicit bounded failure; repeat hashes match. |
| M04-015 | Differential phase suite | 004–012 | setup/game/strategy tests | Canonical choices, events, and state match selected Python fixtures. |
| M04-016 | Frontier milestone review | 001–015 | — | Review phase completeness, no rule stubs disguised as success, and termination bounds. |

## Exit gate

Three- and six-player generic games traverse every phase and finish reproducibly; all structural
choices are generated rather than accepted by rejection.

