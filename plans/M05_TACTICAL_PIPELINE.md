# M05 — Tactical pipeline

## Goal

Port the complete activation-to-production tactical action with exact legal generation and atomic failure.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
|---|---|---|---|---|
| M05-001 | Activation choices | M04 | `game.py`, `movement.py` | Eligible systems and tactic-token payment; activated/forbidden systems match fixtures. |
| M05-002 | Adjacency modifiers | 001 | `galaxy.py`, `movement.py` | Wormholes, anomalies, temporary adjacency, faction-neutral modifiers. |
| M05-003 | Ship movement legality | 002 | `movement.py` | Origin/path/range/blockade rules; generated moves match Python. |
| M05-004 | Fleet composition | 003 | `fleet.py`, `supply.py` | Fleet supply and legal ship subsets without combinatorial duplicates. |
| M05-005 | Capacity/cargo choices | 003,004 | movement/transport tests | Fighters/ground forces assigned to capacity; no independent troop movement. |
| M05-006 | Atomic movement application | 003–005 | movement state transitions | Full move commits or no state changes; conservation property. |
| M05-007 | Space cannon offense | 006 | `combat.py`, reactions | Eligibility, rolls, hits, assignment, events; PDS entry scenario matches. |
| M05-008 | Combat setup and rounds | 006 | `combat.py` | Participants, initiative, dice, hit thresholds, round sequence. |
| M05-009 | Sustain and casualties | 008 | combat/unit modules | Damage identity, assignment legality, destruction events, no double sustain. |
| M05-010 | Combat modifiers/rerolls | 008,009 | combat/reaction tests | Scoped modifiers, extra dice, rerolls, morale, munitions; no effect leakage. |
| M05-011 | Retreat | 008–010 | combat tests | Legal destinations, cargo, timing, tokens, combat termination. |
| M05-012 | Bombardment | 006,008 | invasion/combat modules | Eligibility, planetary shield interactions currently implemented, hits. |
| M05-013 | Landing choices | 005,012 | `invasion.py` | Cargo-to-planet assignment, multi-planet order, atomic landing. |
| M05-014 | Ground combat | 013 | `invasion.py` | Rounds, modifiers, casualties, control transition, termination. |
| M05-015 | Planet control | 013,014 | invasion/state | Claim, previous owner, structures, custodians hooks, event order. |
| M05-016 | Production capacity | 001,015 | `production.py` | Producing units/systems/capacity and blockade legality. |
| M05-017 | Production pricing | 016 | production/technology | Unit costs, discounts, free-production sequence, Integrated Economy constraints. |
| M05-018 | Production payment | 017 | production/payment tests | Enumerate atomic disjoint payment plans; failed payment consumes nothing. |
| M05-019 | Production placement | 016–018 | production/state | Space/planet placement, capacity/fleet checks, produced events. |
| M05-020 | Tactical orchestration | 001–019 | `game.py` tactical flow | Correct optional-step order and `TACTICAL_ACTION_COMPLETE`; no skipped live step. |
| M05-021 | Tactical differential corpus | 020 | tactical-related tests | 10,000 generated scenarios compare choices/events/post-state. |
| M05-022 | Tactical fuzz/properties | 020 | — | No panic, duplication, negative pools, partial commit, or unbounded combat. |
| M05-023 | Tactical benchmarks | 020 | benchmark tool | Activation enumeration and complete tactical latency recorded without parity loss. |
| M05-024 | Frontier critical review | 001–023 | — | Review legality, atomicity, capacity, combat termination, and scoped effects. |

## Exit gate

The entire tactical action matches Python across the differential corpus, properties hold, and the
pipeline cannot partially apply an invalid action.

