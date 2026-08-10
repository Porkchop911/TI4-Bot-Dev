# M07 — Factions and Thunder's Edge

## Goal

Reach parity with every faction-specific and Thunder's Edge behavior implemented on the source branch.

## Work packages

| ID | Package | Depends | Python oracle | Deliverable and acceptance test |
|---|---|---|---|---|
| M07-001 | Faction plugin contract | M06 | `factions.py`, ability modules | Registration, setup, modifiers, timing hooks, validation, coverage reporting. |
| M07-002 | Sol | 001 | `faction_abilities/sol.py` | Setup, abilities, leaders, tech/mech behaviors and existing tests. |
| M07-003 | Letnev | 001 | `faction_abilities/letnev.py` | Abilities/leaders including sequence-scoped Munitions/Harrugh behavior. |
| M07-004 | Xxcha | 001 | `faction_abilities/xxcha.py` | Abilities, leaders, faction technology and reaction choices. |
| M07-005 | Hacan | 001 | `faction_abilities/hacan.py` | Trade/transaction abilities, leaders, faction behavior. |
| M07-006 | Jol-Nar | 001 | `faction_abilities/jolnar.py` | Research/combat modifiers, leaders, tech behavior. |
| M07-007 | L1Z1X | 001 | `faction_abilities/l1z1x.py` | Invasion/production/unit abilities and leaders. |
| M07-008 | Firmament | 001 | `faction_abilities/firmament.py` | Plot lifecycle and current implemented behavior. |
| M07-009 | Other implemented factions | 001 | `factions.py`, other-faction tests | Port only behavior actually implemented outside named modules; ledger-backed. |
| M07-010 | Expedition | M06 | `thunders_edge.py` | Current expedition state, choices, results, and tests. |
| M07-011 | Breakthroughs | 010 | `thunders_edge.py` | Earn/use lifecycle and current three implemented/partial boundaries. |
| M07-012 | Synergy | 010 | `thunders_edge.py` | Current calculations and effects. |
| M07-013 | Ingress/Fracture | 010 | `thunders_edge.py` | Current map/state/action behavior and explicit omissions. |
| M07-014 | TE coverage registry | 010–013 | `thunders_edge.RULES` | Implemented/partial/unmodelled labels match source exactly. |
| M07-015 | Cross-faction integration | 002–014 | faction integration/Save52 tests | Six-faction slice completes rounds and full games without effect leakage. |
| M07-016 | Scoped-effect regression suite | 002–015 | combat/production/activation sequence tests | Effects expire by sequence even on early exit/cancellation. |
| M07-017 | Faction differential suite | 002–016 | all faction/TE tests | Choices, events, state, and coverage match selected oracle cases. |
| M07-018 | Frontier coverage review | 001–017 | — | Verify no corpus-only record is falsely called implemented and no branch behavior is omitted. |

## Exit gate

The supported faction slice and current TE subsystems match Python; unimplemented content remains
visible and accurately counted.

