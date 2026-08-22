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

### Closure record (2026-08-22)

**M07 is closed.** The reopened frontier exit review (M07-020) is accepted by independent Tier-C
review (Claude Opus 5), and every package in the milestone's committed range `b721a9a..8ba6edc`
carries its own accepted review:

- M07-019 (`c034549`): four nested-window regression tests at Game level; findings F-M07-019-1/-2
  raised, F-M07-019-3 scoped as child. Accepted with corrections M1–M3.
- M07-021 (`5241f2d`): `event_feats` added to state equality (Option A), red-first; exposed
  stepped-harness dependence completed test-only. Accepted; N1/N2 dispositions recorded.
- M07-022 (`7f357b6`): stepped harness consumes scoring pauses exactly as `resolve()` does;
  pausing-fixture equivalence verified; completion bookkeeping factored into one shared helper
  (the range's only production hunk, behavior-preserving). Accepted with P1–P3.
- M07-023 (`8ba6edc`): pause→choice resumption pinned at the synchronous API level with log-based
  non-vacuity; Q1/Q2 hardening applied (pause ordering asserted `Some(0)`; inner-table emptiness
  guard). Accepted. Per its reviewer's instruction the M07-019→023 chain ends here.
- M07-020: five-part campaign (six nested-window paths traced and pinned; marker expiry verified
  against monotonic counters with atomic set sites; redaction boundary rechecked at the typed seam;
  registries reconciled; gates reproduced — engine 845 + 5 doctests, workspace 1,319/0 ×2,
  replay 4/4, Clippy zero new in frontier). **R1 (blocking) resolved by decision:** F-M07-019-1
  (structures count as ground defenders against LRR 49) accepted as known difference KD-2 with the
  fix scoped as M08-020, hard-ordered before M08-018 so all downstream baselines run against
  corrected behavior exactly once. R2–R4 (documentation) resolved in-package.
- **Independence limitation (carried per the M06-024 precedent):** the same frontier reviewer is
  independent of the implementer but not a fresh perspective on this range — it reviewed every
  package in the frontier as Tier-B reviewer and formed the M07-019 findings that R1 concerns.
  Recorded in `plans/M07-020_OPEN_REVIEW_ITEMS.md` rather than left implicit.
- **Known-differences ledger:** `plans/KNOWN_DIFFERENCES.md` created at this gate (R3). Entries:
  KD-1 baf/sb play-area note semantics (M06 closure; VP/clearance comparability break
  2.935 → 2.958); KD-2 structures as ground defenders (this gate; fix M08-020); KD-3 phantom dice
  consumption after a total-wipe fwp pause (dice-stream position only, no state/event divergence);
  KD-4 promissory-note holdings visible in redacted views (named gap since M08-001). Mechanism
  limitations: ML-1 `leaks()` is a two-field mirror, not field-complete (M06's `event_feats` is
  the proof case; deferral written with its condition); ML-2 inert reserved Seat fields.
- **Scope reaffirmed:** implemented/partial/unimplemented registries reconciled — registered ⊆
  corpus pinned by test, gaps reported rather than ignored (`unimplemented`, `unmapped_windows`),
  six documented blocked abilities. No corpus-only behavior was silently promoted in the M07
  range; the committed diff under `crates/` is 3 files, +816/−24 (test modules plus the accepted
  `complete_window` refactor and one `event_feats` equality line).
- No command run by any M07 package wrote to the historical Python reference.

**Next ready package:** M08 — begin with M08-017 (frontier information/review gate over rows
001–016), then **M08-020 before M08-018** per the hard ordering recorded above.
