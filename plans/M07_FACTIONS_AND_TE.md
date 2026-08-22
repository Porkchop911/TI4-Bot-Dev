# M07 — Factions and Thunder's Edge

## Goal

Implement and preserve the accepted faction-specific and Thunder's Edge behavior under official
rules and versioned Rust specifications.

## Work packages

Rows 001–018 retain the historical comparison evidence under which they were
executed. Python parity is no longer an acceptance criterion. Rows 019–020
revalidate the accepted Rust scope after M06's event-window correction.
Rows 021–023 close gaps found during the M07-019/M07-021/M07-022 reviews and must
complete before row 020.

| ID | Package | Depends | Historical source / normative context | Deliverable and acceptance test |
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
| M07-019 | Post-M06 faction/TE integration revalidation | M06-024,M07-018 | Accepted Rust faction/TE scope; FFG objective timing | Run faction integration/scoped-effect/full-workspace gates with nested secret windows; fix only demonstrated regressions. |
| M07-020 | Reopened frontier exit review | 019, 021, 022, 023 | — | Resolve timing/effect-scope/coverage findings and reaffirm the accepted faction/TE scope. |
| M07-021 | `event_feats` state-equality projection (child of M07-019 review M2) | 019 | `ti4-model` `Player::PartialEq`; M06 occurrence model | Include `Player.event_feats` in state equality (or record an explicit exclusion with reason); the direct-vs-stepped equivalence invariant must be able to fail on feat evidence. Prep spec: `plans/M07-021_EVENT_FEATS_PROJECTION.md`. Must complete before 020. |
| M07-022 | Stepped-vs-driven equivalence across scoring pauses (child of M07-021 review N1) | 021 | `combat.rs` synchronous API loop; M06 pause path | The stepped harness consumes scoring pauses exactly as `resolve()` does; a pausing-fixture comparison verifies equivalence across the M06 pause path; completion bookkeeping factored into one shared helper. Prep spec: `plans/M07-022_STEPPED_EQUIVALENCE_ACROSS_PAUSES.md`. Must complete before 020. |
| M07-023 | Stepped equivalence across pause→choice resumption (child of M07-022 review P2) | 022 | `combat.rs` synchronous API loop; M06 pause path composed with a choice at the retained frame | A pausing fixture that continues past the barrage into a casualty assignment: the stepped driver resumes into a choice at the retained frame and both sides end identically; fixture proven non-vacuous for P2 (choice-after-pause asserted, old bug class reproduced). Prep spec: `plans/M07-023_POST_PAUSE_CHOICE_COMPOSITION.md`. Must complete before 020. |

## Exit gate

The supported faction slice and current TE subsystems meet the accepted Rust specifications and
named official rules; unimplemented content remains visible and accurately counted; M07-020 has no
unresolved finding.
