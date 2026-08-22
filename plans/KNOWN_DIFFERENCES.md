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

### KD-5 — `is_ground_force()` misses Hel-Titan I (recorded at M08-020 acceptance, review T1)

`UnitType::is_ground_force()` hardcodes `titans_pds2` instead of reading the corpus's own
`isGroundForce` flag; `titans_pds` (Hel-Titan I — rolls a die at 7, flagged in the corpus) is not
classified as a ground force. Before M08-020 this was benign (any rival unit contested a planet);
after it, a Hel-Titan-I-only planet falls without resistance and the unit can never be assigned a
combat casualty. Source: T1 (`plans/M08-020_OPEN_REVIEW_ITEMS.md`).

- **Dormancy:** Titans is not in D11's six-faction roster, so no affected unit can exist on a
  board today — structurally blocked, like M06-025's L1. It fires the moment the roster widens.
- **Fix constraint (carried into the fix):** a bare flag read would also drop the two unflagged
  Naaz space mechs (`naaz_mech_space`, `absol_naaz_mech_space`) that the base-type match catches;
  the predicate switch must be preceded by an explicit recorded decision on them (union semantics
  recommended — see spec).
- **Fix:** scoped as **M08-022** (`plans/M08-022_TITANS_PDS_GROUND_FORCE_PREDICATE.md`),
  hard-ordered before any D11 roster widening; does not block M08-018/021. This entry leaves the
  ledger when M08-022 is accepted and committed.

## Mechanism limitations (engine-internal)

### ML-1 — `leaks()` is a two-field mirror, not field-complete (recorded at M07 closure, R2)

`ti4-policy/src/view.rs::leaks()` iterates exactly the two fields `redact_player` redacts
(`action_cards`, `secret_objectives`) by hand. A third private field added to `Player` would be
missed by both — the "newly added private field fails a test" guarantee in its doc comment is not
what the implementation provides.

- **Proof case:** `Player.event_feats` was added during M06; neither redaction implementation
  covers it and `leaks()` stayed silent. No actual leak today because `event_feats` holds
  table-public occurrence ids (M07-019 review M4).
- **Bounding note (M08-017 frontier review, 2026-08-22):** the M08-017 gate probed the policy
  layer past the two mirrored fields: `event_feats` and `scored_feat_occurrences` appear nowhere
  in `ti4-policy`, and `promissory_notes` appears only as the *named* `UNREDACTED` gap (KD-4) and
  its test. Nothing on the bot side consumes an unredacted field, so ML-1 is a **latent leak with
  no reader** — a risk, not a live hazard. Any package that gives the policy layer access to such
  a field must close this entry first.
- **Deferral, in writing:** making the check field-complete is a separate, larger question — no
  package scoped at M07 closure. **Condition for any future package that adds a private field to
  `Player`:** it must extend both redaction implementations (`choice.rs::redacted_for`,
  `ti4-policy`'s `redact_player`) and the leak check in the same commit, with a red-first test.

### ML-2 — inert reserved Seat fields (recorded at M07 closure)

`Seat::ground_roll_suppressed_round` and `Seat::sustained_damage_round` are declared and compared
in `PartialEq` but have no read or write site anywhere in the engine (F-M07-020-1). No leak risk;
recorded so a future package implementing the abilities they names must add set site, identity-
checked read site, and tests together.

## Scope dispositions (answerability at M12)

### SD-1 — M08 rows 008/010/013 cancelled; 014 and 016 waived; 015 required as M08-021 (operator decision, 2026-08-22)

F-M08-017-1 disposition: the operator adopted the M08-017 frontier review's recommendation
(option c hybrid) **as-is**. Full rationale table in `plans/M08_AUTHORED_BOTS.md` (Scope
dispositions); reasoning and the withdrawn-justification record in
`plans/M08-017_OPEN_REVIEW_ITEMS.md` §S3. Summary for M12 answerability:

- **Cancelled:** 008 tactical plans, 010 faction profiles, 013 experimental capabilities — no
  consumer in MLP Phases 2–8; inherited oracle-port scope. The "heuristics constraint"
  justification was withdrawn as wrong (the authored bot is architecturally isolated from
  training) and must not be cited for these cancellations.
- **Waived with reason:** 014 differential choices (determinism pins + the 112 behavioral tests
  cover the practical regression risk); 016 benchmark (M00-012's protocol and the MLP plan's D19
  CPU/CUDA gates define the throughput measurements that matter; the dead `criterion` dependency
  was removed per S1).
- **Required:** 015 as **M08-021** before M08-019 closes — the authored bot is the comparison
  baseline every cross-time VP measurement depends on, including the MLP Phase 8 ablation.
- **Deferred:** 012 serialization (no consumer; added with its first consumer).
- **No action:** 009 (misattributed to M08; content is M09-track `progress.rs`).

This entry leaves the ledger only if a committed package reopens one of these rows.
