# M08-020 — Ground-combat structure legality (F-M07-019-1 fix)

## Preparation status

**Accepted 2026-08-22.** Independent Tier-C frontier review (`plans/M08-020_OPEN_REVIEW_ITEMS.md`):
accept with T1 required before commit — resolved by recording KD-5 and scoping child package
M08-022 (ti4-content is outside this package's writable paths); T2 evidence correction applied;
T3 recorded. Note: item 4's "l1z1x_ variants" wording was corpus-corrected during implementation —
L1Z1X has no structure variants, so the load-bearing assertion is one-for-one base-type count
preservation plus invader ownership (see evidence, Design decisions; the reviewer confirmed this
was the right call). Evidence: `plans/evidence/M08-020.md`.

**Started 2026-08-22.** Base commit `734de3f` (M08-017 closure). Branch
`wp/m08-020-ground-combat-structure-legality`. Scoped by the M07-020 frontier adjudication of
finding F-M07-019-1 (see `plans/M07-020_OPEN_REVIEW_ITEMS.md`, R1, and `plans/evidence/
M07-020.md`). Dependencies met: M07-020 accepted, M08-017 closed. **Must complete before
M08-018** so that the post-M07 bot revalidation — and every downstream baseline built on it,
including M08-021's distribution baseline — runs against corrected behavior.

| Field | Value |
|---|---|
| Milestone | M08 — Authored bots |
| Depends | accepted M07-020 (adjudication), M08-017 ✅ |
| Blocks | M08-018, M08-021 (hard ordering) |
| Permission class | P1 |
| Review tier | C — legality/timing semantics of invasions (frontier model per AGENTS.md) |

## Objective and normative source

Make ground combat occur only when the planet contains enemy **ground forces**, per FFG *Living
Rules Reference 2.0* rule 49: "If a player lands units on a planet controlled by an opponent but
that does not contain any of the opponent's ground forces, that planet falls without resistance."

The engine today (`invasion.rs::defender_on`, `unit.owner != self.invader`) counts **any** rival
unit — including PDS and Space Dock, which roll no dice and are not ground forces — as a ground
defender. Consequences to be corrected:

- Structure-only planets trigger a full spurious ground combat that the invader always wins
  (structures absorb hits but never roll).
- Rival structures are destroyed in that fight rather than on control transfer, so L1Z1X
  Assimilate's structure conversion is unreachable through `InvasionWindow`.

## Adjudicated impact scope (from M07-020 review)

The deviation is real; its blast radius is bounded: no invader casualties from structures, final
control outcome unchanged, the observable difference being which step destroys structures plus the
dice and choices the spurious fight consumes. M08's accepted baseline was built on this same
M05-era behavior — hence the hard ordering before M08-018 rather than a mid-milestone re-baseline.

## Scoped access (declared at start, before any finding exists)

```text
Writable paths:
  crates/ti4-engine/src/invasion.rs
  crates/ti4-engine/src/game.rs        (test module only, unless a demonstrated regression requires more)
  plans/M08-020_GROUND_COMBAT_STRUCTURE_LEGALITY.md
  plans/evidence/M08-020.md
  plans/EXECUTION_STATE.md
Read-only supporting paths:
  crates/ti4-engine/src/{combat,faction_abilities}.rs
  plans/evidence/M07-019.md, plans/evidence/M07-020.md, plans/KNOWN_DIFFERENCES.md
Network/process needs: bounded Cargo test/lint/replay commands only
Generated artifacts: Cargo target output and bounded ignored replay logs only
External-state effects/destructive actions: none
```

## Required behavior and tests

1. `defender_on` (or its replacement) counts only ground forces; a planet whose rival units are
   all structures falls without resistance — no ground-combat occurrence, no dice consumed,
   structures destroyed on control transfer per the rules' landing sequence.
2. Mixed planets (structures **and** ground forces) still fight exactly as today for the ground
   forces; structure handling within that fight must match rule 49's treatment of non-ground units.
3. The home-loss occurrence and its `AnyPerPlayer` scoring window are unchanged in identity and
   timing relative to control transfer (M07-019 test 2's pause-ordering assertions must keep
   passing, re-pointed at the corrected flow).
4. **Required deliverable carried from M07-019 review M1c:** Assimilate-after-pause coverage —
   `the_home_loss_pause_holds_the_invasion_at_finalizing_control` strengthened so its conversion
   assertions become load-bearing: ownership assertion checks for the l1z1x_ variants, and
   one-for-one count preservation holds against b's surviving structures.
5. Red-first: a focused test demonstrating the current spurious-combat behavior fails after the
   fix (structures no longer trigger combat), plus regression tests that mixed-planet fights and
   the scoring-window sequence are unchanged where they must be.
6. Full affected-crate, workspace ×2, replay-determinism, Clippy (pasted output), and
   `git diff --check` gates; any VP/clearance movement recorded against the known-differences
   ledger with pre/post numbers.

## Non-goals

No changes to space combat, bombardment, or the occurrence-scoped scoring contract; no roster
widening; no bot work (that is M08-018's job, run after this package).
