# Known-differences ledger

Standing record of every accepted deviation from official rules or external references, and every
known limitation of the engine's own mechanisms. Created by the M07-020 frontier adjudication
(R3): findings had been "carried to" this destination without it existing. **M12 qualification is
where these entries must be answerable from** — each entry names its source package, its exact
scope, and what would make it moot or require re-checking.

Behavioral differences are ordered by the milestone that accepted them. An entry leaves this file
only when a committed package removes the difference, with a pointer to that package's evidence.

## Behavioral differences (rules / external references)

### KD-1 — baf/sb count play-area notes only (accepted at M06 closure, 2026-08-21)

`BarrageTookTheLastFighters` and `StrengtheningBonds` score from face-up (play-area) promissory
notes only; hand-held notes do not count. Source: M06-025 (`plans/evidence/M06-025.md`), fixing
the F2 finding of the M06-024 reopened review.

- **Effect on comparability:** VP/clearance numbers are non-comparable across this change until
  re-baselined — pre-M06-025 baseline mean VP per seat **2.935**; post-M06-025 probe run
  **2.958** on the same protocol (150 games, r6 champions, `out/stage2_r6/final10000.json`).
- **Standing re-check condition (L1):** eight of the nine faction play-area notes cannot fire
  under D11's six-faction roster. Any future package that widens the roster must re-verify baf/sb
  activation for those notes before citing their scoring behavior.

### KD-2 — structures count as ground defenders (accepted at M07 closure, 2026-08-22)

`invasion.rs::defender_on` counts any rival unit on a planet — including PDS and Space Dock,
which roll no dice — as a ground defender. Official LRR rule 49: a planet with no enemy *ground
forces* falls without resistance. Source: F-M07-019-1 (`plans/evidence/M07-019.md`), adjudicated
at M07-020 (R1, option 2 — accepted as a recorded known difference).

- **Blast radius (adjudicated):** bounded. Structures roll no dice, so the invader takes no
  casualties from them and always wins the spurious fight; final control outcome unchanged. The
  observable differences are which step destroys structures (combat vs control transfer) plus the
  dice and choices the spurious fight consumes. L1Z1X Assimilate's structure conversion is
  unreachable through `InvasionWindow` until fixed.
- **Fix:** scoped as **M08-020** (`plans/M08-020_GROUND_COMBAT_STRUCTURE_LEGALITY.md`) with hard
  ordering before M08-018, so bot revalidation and all downstream baselines run against corrected
  behavior. This entry leaves the ledger when M08-020 is accepted and committed.

### KD-3 — phantom dice consumption after a total-wipe fwp scoring pause (accepted at M07 closure)

When round-1 Anti-Fighter Barrage both records `BarrageTookTheLastFighters` and destroys one side's
entire fleet, the resume path (`RollingAfterBarrage`) rolls both fleets **before** any `over()`
check. Source: F-M07-019-2 (`plans/evidence/M07-019.md`).

- **Scope (reviewer-refined):** hits against the empty fleet are discarded by the 15.2a branch, so
  final unit state, winner, and events are identical to a rules-correct immediate conclusion;
  `combat_round_seq`, event emission, and faction round-opening offers all sit inside the
  `run_barrage` block and cannot diverge. **Only the dice stream position moves.** Determinism
  within this engine is unaffected.
- **Effect on comparability:** post-fwp-pause dice sequences are non-comparable with any external
  reference (e.g., a rules-correct simulator's RNG trace). No package has been scoped to fix it;
  if one is, it must re-run the M07-019/022/023 equivalence suite.

### KD-4 — promissory-note holdings visible in redacted views (accepted at M08-001, reaffirmed at M07 closure)

`GameState::promissory_notes` is a table-wide map and cannot be redacted per player; every view
therefore exposes who holds which note, which rule 69.6 makes hidden until played. Source:
M08-001 (`crates/ti4-policy/src/view.rs`, `UNREDACTED = ["promissory_notes"]`), reaffirmed by the
M07-020 campaign (F-M07-020-2).

- **Why it stays:** the oracle had the same exposure and its bots were tuned against it; closing
  it would change how bots choose before improving anything. It is pinned by a naming test
  (`the_gaps_this_view_still_has_are_named`), so redacting it later is a deliberate act, not an
  accident. Any package that closes this gap must re-baseline bot behavior and say so in evidence.

## Mechanism limitations (engine-internal)

### ML-1 — `leaks()` is a two-field mirror, not field-complete (recorded at M07 closure, R2)

`ti4-policy/src/view.rs::leaks()` iterates exactly the two fields `redact_player` redacts
(`action_cards`, `secret_objectives`) by hand. A third private field added to `Player` would be
missed by both — the "newly added private field fails a test" guarantee in its doc comment is not
what the implementation provides.

- **Proof case:** `Player.event_feats` was added during M06; neither redaction implementation
  covers it and `leaks()` stayed silent. No actual leak today because `event_feats` holds
  table-public occurrence ids (M07-019 review M4).
- **Deferral, in writing:** making the check field-complete is a separate, larger question — no
  package scoped at M07 closure. **Condition for any future package that adds a private field to
  `Player`:** it must extend both redaction implementations (`choice.rs::redacted_for`,
  `ti4-policy`'s `redact_player`) and the leak check in the same commit, with a red-first test.

### ML-2 — inert reserved Seat fields (recorded at M07 closure)

`Seat::ground_roll_suppressed_round` and `Seat::sustained_damage_round` are declared and compared
in `PartialEq` but have no read or write site anywhere in the engine (F-M07-020-1). No leak risk;
recorded so a future package implementing the abilities they names must add set site, identity-
checked read site, and tests together.
