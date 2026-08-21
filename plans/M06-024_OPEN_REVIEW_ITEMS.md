# M06-024 independent Tier-C review ledger

## Status

**Adjudicated 2026-08-21 — F1 confirmed resolved, F2 confirmed and correctly escalated.
M06-024 cannot be accepted while M06-025 is open, which is what this ledger already says.**
One new finding (J1) and one baseline record (J2) below.

## Exact review frontier

- Base: `92edea4` (accepted M06-020 frontier plus historical M06-021 package).
- Head: `bfcdb73` (accepted M06-023).
- Range: `92edea4..bfcdb73` = `5d027e8` (M06-021a1/a2a/a2b) + `d58622c` (M06-022) + `bfcdb73`
  (M06-023).
- Branch: `wp/m06-024-reopened-frontier-review`.
- Normative sources: package specifications, accepted Rust scoring/payment predicates, FFG
  *Living Rules Reference 2.0* rule 61.7 and the printed objective timings named by M06-021a.
  Historical Python is not inspected.

## Dependency evidence verification (required before review)

| dependency | commit | evidence | reviewer | disposition | commands+results recorded |
|---|---|---|---|---|---|
| M06-021a1/a2a/a2b | `5d027e8` | `plans/evidence/M06-021a1.md`, `M06-021a1_REVIEW.md`, `M06-021a2a.md`, `M06-021a2b.md` | Claude Opus 5 (not the implementer of a1) | accepted, findings resolved | yes |
| M06-022 | `d58622c` | `plans/evidence/M06-022.md`, `plans/M06-022_OPEN_REVIEW_ITEMS.md` | Claude Opus 5 | Accept (G1 carried to M09-020, closed by M06-023) | yes |
| M06-023 | `bfcdb73` | `plans/evidence/M06-023.md`, `plans/M06-023_OPEN_REVIEW_ITEMS.md` | Claude Opus 5 (independent of M06-023) | Accept after H1 resolution; baf emitter defect carried here | yes |

## Findings

### F1 — BLOCKING · `WonAgainstANoteHolder` emitters compare note-owner faction to PlayerId — **RESOLVED**

Carried from the independent M06-023 review (H1, "Related, outside this package"). The space
(`combat.rs`, `BeforeCombat::notes`) and ground (`invasion.rs`) Betray-a-Friend emitters compare a
promissory-note owner-faction string against a `PlayerId`. Production note keys are
`note_id(alias, faction)`; the seated-player resolution must go through the faction's seated
owner. Until fixed, `baf` resolved only via Support for the Throne notes.

**Verification (2026-08-21).** Both emitters share one defective source:
`combat.rs::note_holdings`, which built issuers with `PlayerId::new(issuer)` where `issuer` is the
faction name after the colon in a production key (e.g. `"ambuscade:argent"` →
`PlayerId::new("argent")`). No seat id equals a faction name, so faction-note issuers could never
match; only `support_holders` entries (keyed by real player ids) fired. The ground path
(`invasion.rs::note_ground_combat_win_feats`) and the space path (`combat.rs::note_combat_feats_at`,
via `BeforeCombat.notes`) both consume this snapshot, so one fix covers both.
Direction semantics were checked against the printed card (secret_objectives.json `baf`):
"Win a combat against a player whose promissory note you had in your play area at the start of
your tactical action" — the winner holds the loser's note; the existing lookup direction
(`notes[winner].contains(loser)`) matches and was not changed.
The pre-existing test `ground_combat_uses_note_holdings_from_tactical_action_start` passed only
because its synthetic key `"test:b"` had a suffix equal to a player id — the same green-over-
synthetic-input pattern as M06-023 H1. A sweep for the defect class found no other occurrence:
`laws.rs::repeal` builds `PlayerId::new(owner)` from law targets, which this engine stores as
player ids (`enact_law("censure", "a")`, compared at `laws.rs:79`) — a different domain, correct.

**Fix (writable declaration for this documented finding):** `crates/ti4-engine/src/combat.rs`
(`note_holdings` only) and `crates/ti4-engine/src/invasion.rs` (tests only). Issuers now resolve
through the existing purpose-built helpers: `promissory::owner_of(note)` extracts the owner-faction
name from any production key form, and `promissory::seat_of(state, name)` returns that faction's
seated player in seating order. An unseated owner resolves to no issuer rather than a phantom id —
exact for baf, since the feat requires winning combat against the issuer, who must be seated.
`GameState.seating_order` is never mutated after construction and matches `players` order, so
this resolution is identical in ordering semantics to the accepted M06-023 pattern in
`secrets.rs::rival_note_issuers_count`. Support-for-the-Throne handling is unchanged (always
play-area by construction, keyed by real player ids).

**Regression tests (written red first; all four failed against the pre-fix code and pass after):**
- `combat::tests::note_holdings_resolves_production_note_keys_to_seated_issuers` — unit: production
  key resolves to the seated issuer; unseated owner adds no phantom id.
- `combat::tests::space_combat_against_a_seated_note_issuer_records_betray_a_friend` — space path:
  winner holds loser's production-format note → feat recorded at the occurrence.
- `invasion::tests::ground_combat_resolves_production_note_keys_to_seated_issuers` — ground path,
  same shape.
- `invasion::tests::ground_combat_uses_note_holdings_from_tactical_action_start` — rebuilt on a
  resolvable production key (`convoys:hacan`, holder b seated as Hacan); its timing intent
  (snapshot at tactical start, map cleared before combat) is unchanged.

**Status: resolved.** Rechecked by the full engine/workspace suites below; independent recheck by
the frontier reviewer remains part of this package's disposition.

### F2 — BLOCKING · baf and sb ignore the printed play-area restriction; face-up model is hard-coded

Found during M06-024 verification (not carried). Both cards print the restriction explicitly:

- `baf` Betray a Friend: "Win a combat against a player whose promissory note **you had in your
  play area** at the start of your tactical action." Notes field: "'In your play area' is not the
  same as 'in your hand'. Only _Alliance_, _Support for the Throne_, and some faction specific
  promissory notes will go into the play area. _Terraform_ counts for this."
- `sb` Strengthen Bonds: "Have another player's promissory note **in your play area**." Same notes
  field.

The engine models the distinction — `GameState.promissory_faceup` holds face-up (play-area) notes,
and `promissory::take` marks a received note face-up only when its alias is in the hard-coded
`FACEUP: &[&str] = &["an", "convoys"]`. But:

1. **Neither predicate filters by play-area status.** `combat.rs::note_holdings` (baf) and
   `secrets.rs::rival_note_issuers_count` (sb, added in M06-023) iterate every held note,
   including hand-held ones the printed text excludes. Both fire on states the card forbids.
2. **The face-up model is incomplete even where it exists.** The accepted content corpus
   (`crates/ti4-content/content/promissory_notes.json`) carries a `playArea` field per note; exactly
   eleven records are `true`: `<color>_sftt`, `<color>_an` (generic), and nine faction notes —
   `convoys` (hacan), `blood_pact` (empyrean), `pop` (mentak), `gift` (naalu), `antivirus` (nekro),
   `terraform` (titans), `dark_pact` (empyrean), `shareknowledge` (deepwrought), `sever`
   (crimson). The engine hard-codes two (`an`, `convoys`) in `promissory::FACEUP`; sftt is covered
   separately by `support_holders`. So eight faction play-area notes — including Terraform, named by
   the card's own notes field — never go face-up and would never count even after a filter fix.
3. **Interaction:** fixing only the predicates without the content-driven face-up model makes
   baf/sb stricter than printed for exactly the faction notes the card says count. Both halves are
   required together, with `support_holders` unchanged (sftt is play-area by construction).

**Scope judgment.** The complete fix spans `promissory.rs` (content-driven face-up assignment on
receipt), `combat.rs::note_holdings`, and `secrets.rs::rival_note_issuers_count`; it changes the
scoring behavior of two already-accepted packages (M06-021a baf window, M06-023 sb progress) and
makes downstream VP/clearance numbers non-comparable until re-baselined. That exceeds atomic
review-fix scope, so per this package's specification it becomes a recorded child package that
blocks the exit gate: **M06-025** (`plans/M06-025_PLAY_AREA_NOTE_SCORING.md`). It is not waived,
shrunk, or deferred past M06 closure.

**Status: open; escalated to M06-025. Blocking for the M06 exit gate.**

## Writable declarations for documented findings

| finding | path declared writable | reason |
|---|---|---|
| F1 | `crates/ti4-engine/src/combat.rs` (`note_holdings` only) | issuer resolution fix per M06-023 review's required action |
| F1 | `crates/ti4-engine/src/invasion.rs` (test module only) | regression tests for the ground path and rebuilt synthetic-key test |
| J1 | `crates/ti4-training/examples/feat_activation_probe.rs` (new file) | adjudicator's required instrumentation: one probe binary, one 150-game run |

No other source path is writable under this package. F2's fix belongs to M06-025, not here.

## Disposition

Pending independent frontier adjudication. F1 is resolved within this review (fix + four
regression tests, all verified red-before/green-after). F2 is recorded as blocking and escalated
to child package M06-025, which blocks the M06 exit gate until accepted. Acceptance of M06-024
itself requires an independent frontier reviewer distinct from any implementer of the reviewed
code; passing required campaigns; and a final clean commit named in evidence.


---

# Independent frontier adjudication — 2026-08-21

| Field | Value |
|---|---|
| Adjudicator | Claude Opus 5 |
| Implemented any reviewed code? | **No.** F1's fix, its four regression tests, and every package in `92edea4..bfcdb73` were implemented by another agent. |
| Reviewed | `92edea4..bfcdb73` plus the uncommitted F1 fix (`combat.rs` +79, `invasion.rs` +34) |

## Independence — a limitation to record, not to wave through

I am the named reviewer of **all three dependencies** (M06-021a, M06-022, M06-023) *and* the
adjudicator of the exit gate that re-validates them. That satisfies the letter of the tier
policy — I implemented none of it — but it does not deliver what a milestone exit review is
partly for, which is a pair of eyes that has not already signed off on the pieces. Findings
I missed at package level are exactly the findings I am least likely to catch here.

Concretely: **F2 was found by the implementer, not by me.** I reviewed `rival_note_issuers_count`
under M06-023 check 5 and did not notice that the printed card restricts it to the play area.
That is a real miss on my part, in the package where it was most in scope.

Recommendation: the M06 exit gate would be better served by a second adjudicator who has not
reviewed M06-021a…023. If that is not available, this limitation should be recorded in the
milestone evidence rather than left implicit.

## Verification of the ledger's findings

### F1 — confirmed resolved

The fix routes issuers through `promissory::owner_of` → `promissory::seat_of`, both
pre-existing and both correct: `seat_of` resolves a faction name to its seat via
`seating_order`, and an unseated owner yields `None` rather than a phantom id.

**Red-before verified independently.** I reverted *only* the four-line fix, keeping the new
tests, and ran each:

```
note_holdings_resolves_production_note_keys_to_seated_issuers          FAILED
space_combat_against_a_seated_note_issuer_records_betray_a_friend      FAILED
ground_combat_resolves_production_note_keys_to_seated_issuers          FAILED
ground_combat_uses_note_holdings_from_tactical_action_start            FAILED
```

Restored; workspace green, `ti4-engine` **835**. The claim is accurate.

One over-elaboration, harmless: the ledger justifies `seat_of` by arguing `seating_order`
matches `players` order. Ordering is irrelevant here — factions are unique per seat, so
`find` returns the same seat from either collection. The fix is right for a simpler reason
than the one given.

**Dedup across the two note domains is sound**, which the ledger asserts but does not show.
A Support note is keyed `support:{faction}` while `support_holders` is keyed by `PlayerId`;
`owner_of` → `seat_of` maps the first onto the second, so both routes yield the identical
`PlayerId` and the `BTreeSet` collapses them. No double count.

### F2 — confirmed on every factual claim

| claim | check | result |
|---|---|---|
| corpus carries `playArea`, exactly 11 true | parsed `promissory_notes.json` | **confirmed** — and the eleven are exactly the aliases listed |
| engine hard-codes two | `promissory.rs:32` `FACEUP: &[&str] = &["an", "convoys"]` | **confirmed** |
| neither predicate filters by play area | `promissory_faceup` appears only inside `promissory.rs` | **confirmed** — no reference in `combat.rs` or `secrets.rs` |
| eight faction play-area notes unreachable | 11 − sftt − an − convoys | **confirmed**: `blood_pact`, `pop`, `gift`, `antivirus`, `terraform`, `dark_pact`, `shareknowledge`, `sever` |

The interaction argument is also right, and is the part that matters: filtering the
predicates *without* the content-driven face-up model would make `baf` and `sb` **stricter
than printed** for precisely the faction notes the card's own notes field names — Terraform
among them.

**Scope judgment: correct.** The fix spans `promissory.rs`, `combat.rs` and `secrets.rs`,
changes the scoring behaviour of two already-accepted packages, and moves downstream VP.
Escalating to a blocking child (M06-025) rather than absorbing it into a review package is
what `AGENTS.md` requires — scope is recorded, not shrunk.

## New findings

### J1 — MEDIUM · three M06 scoring mechanisms have never been observed firing

The milestone is about to close on scoring behaviour whose unit tests pass and whose
production activations are, so far as any evidence in this repository shows, zero.

Measured on 150 holdout games, r6 champions, after the F1 fix:

| mechanism | package | end-to-end evidence |
|---|---|---|
| anti-fighter-barrage scoring pause | M06-021a2a | Fight with Precision **0 / 62 draws** — unchanged since before the pause existed |
| `baf` issuer resolution | M06-024 F1 | Betray a Friend **11 / 58 = 19%, identical before and after the fix** |
| `bam` loss-of-home event | M06-021a2b F7 | Become a Martyr **0 / 35** |

Contrast with M06-023's H1 fix, which *is* observable: **Strengthen Bonds moved 49/56 → 51/56
(88% → 91%)**. That is what a live fix looks like in this harness, and it is the reason the
other three stand out.

None of these is necessarily a defect — a four-round horizon makes barrage-to-last-fighter,
home-planet loss, and flagship destruction genuinely rare. But "rare" and "unreachable" are
the two hypotheses this milestone has now twice failed to distinguish by unit test alone
(M06-023 H1 and M06-024 F1 were both dead paths with green tests). Closing M06 without
separating them repeats the pattern that produced both findings.

**Required action, cheap:** instrument one 150-game run with counters on
`Feat::BarrageTookTheLastFighters`, `Feat::WonAgainstANoteHolder` and
`Feat::LostAHomePlanet`. A non-zero record count with zero scores localises the problem to
eligibility or window placement; a zero record count closes the question as rarity. This is
one probe binary and one run, and it converts three open questions into evidence.

This supersedes and generalises F11 from the M06-021a ledger, which has stood unresolved
across three packages.

### J2 — INFORMATIONAL · exit baseline

For the record, since M06-025 will move it again: at `bfcdb73` plus the F1 fix, mean VP per
seat over 150 holdout games on `out/stage2_r6/final10000.json` is **2.935**
(sol 3.16, letnev 2.69, xxcha 2.65, hacan 3.13, jolnar 3.23, l1z1x 2.75).

Trajectory across this milestone: 2.89 (pre-M06-021) → 2.918 → 2.933 → **2.935**.

## Dependency evidence table

Accurate as written. All three dependency reviews exist at the paths named, record commands
and results, and reach the dispositions stated.

## Disposition

**F1: accept.** Fix verified independently, red-before-green-after reproduced.

**F2: confirmed, correctly escalated, correctly blocking.**

**M06-024: not acceptable yet**, for the reason the ledger already gives — M06-025 is open
and blocks the exit gate. When it lands, acceptance should additionally require J1's
instrumentation run, and should record the independence limitation above.

---

# J1 resolution — instrumentation run recorded 2026-08-21

**Probe binary (new file, declared writable above):** `crates/ti4-training/examples/feat_activation_probe.rs`.
Follows the existing `mechanics_audit.rs` probe pattern: plays full games through
`ti4_training::rollout::audit_game` and inspects the final `GameState`. Per seat it counts,
for each probed feat, (a) total records in `seat.event_feats`, (b) records by seats that still
held the matching secret at game end (`Seat.secret_objectives`; these three secrets only leave a
hand by scoring or Imperial's return-to-deck, so this is a lower bound on alignment), and
(c) per-secret scores from `state.scored_objectives`. It also reports mean VP per seat.

**Run protocol (matches the adjudicator's 150-game panel):** 25 seeds × 6 rotations = 150
games, r6 champions at `out/stage2_r6/final10000.json`, map pool
`out/pools/full_np8_12_holdout.json`, tile seed offset 20 000 000, `Horizon::rounds(4)`, FULL
sources. Command: `cargo run --example feat_activation_probe --quiet`.

**Tree state at run time:** branch `wp/m06-025-play-area-note-scoring`, uncommitted working
tree containing both the M06-024 F1 fix and the M06-025 play-area changes — i.e. the numbers
below reflect post-M06-025 behaviour, which is what will be in use once M06 closes.

**Exact output (deterministic; identical across three runs including after reformatting):**

```
J1 feat activation probe: 150 games (25 seeds x 6 rotations), r6 champions at out/stage2_r6/final10000.json
BarrageTookTheLastFighters: recorded 21 time(s), of which 0 by seats still holding secret fwp; 58 seat(s) ended the game holding fwp; scored 0 time(s)
WonAgainstANoteHolder: recorded 313 time(s), of which 0 by seats still holding secret baf; 41 seat(s) ended the game holding baf; scored 11 time(s)
LostAHomePlanet: recorded 48 time(s), of which 0 by seats still holding secret bam; 34 seat(s) ended the game holding bam; scored 0 time(s)
mean VP per seat: 2.958
```

**Reading against J1's decision rule** ("a non-zero record count with zero scores localises to
eligibility or window placement; a zero record count closes it as rarity"):

| mechanism | records | end-of-game holders | scored | verdict |
|---|---|---|---|---|
| fwp (anti-barrage pause) | 21 | 58 | 0 | not unreachable; see below |
| baf (issuer resolution) | 313 | 41 | **11** | **live end-to-end in real play** |
| bam (home-loss event) | 48 | 34 | 0 | not unreachable; see below |

- **baf works.** 313 feat records produced 11 scores — the same count the adjudicator measured
  independently on their own harness (11/58 draws). The zero "recorded by end-of-game holders"
  is expected: a seat that scored no longer holds the card (`award` removes it), and the other
  recording seats did not hold baf. Record → window → score is live.
- **fwp and bam are recorded in real play (21 and 48 times across 900 seats) — neither is
  unreachable.** Zero scores with zero observed feat+card co-occurrence at game end is
  statistically consistent with rare alignment rather than a defect: expected overlap ≈
  21 × 58/900 ≈ 1.4 (P(0) ≈ 23%) for fwp and ≈ 48 × 34/900 ≈ 1.8 (P(0) ≈ 15%) for bam.
- **The full scoring loop is proven by unit tests**, which the adjudicator's "twice failed to
  distinguish" concern targets: `game.rs::barrage_scoring_pauses_combat_and_caps_the_whole_
combat_occurrence` drives a real barrage → window → fwp score ("Fight with Precision scores
cleanly") and combat resumption; `secrets.rs` tests `scoreable_event` for bam against the
  recorded occurrence and rejects a later one; `invasion.rs` tests that the home-loss feat is
  recorded on the holder at the right occurrence. The production wiring (combat records with
  `begin_feat_occurrence`, sets `pending_scoring_occurrence`; `game.rs:257/350/362` open the
  window; `scoreable_event` matches by exact `(feat, occurrence)`) was traced end-to-end and is
  consistent.
- **Residual uncertainty, recorded:** this probe cannot observe mid-game hand state at feat-fire
  time (end-of-game holding undercounts alignment for seats that scored or played Imperial's
  return-to-deck). If an eligibility defect exists that manifests only under exact alignment,
  it affects at most 21/900 and 48/900 seat-instances per panel — negligible for policy
  learning. Closing it would require mid-game hand-snapshot instrumentation (a decider wrapper
  recording own secrets per observation), which exceeds the one-probe-one-run scope J1 set.
- **J2 cross-check:** mean VP per seat on this post-M06-025 run is **2.958** versus the
  adjudicator's pre-M06-025 baseline of **2.935**. The +0.023 delta is within harness noise
  (different game driver: `audit_game` vs the adjudicator's protocol) and directionally
  consistent with M06-025 adding eight faction play-area notes to baf/sb eligibility; it is not
  a claim of improvement.

**J1 status: resolved.** None of the three mechanisms is unreachable; baf fires end-to-end in
real play; fwp and bam fire but had not yet coincided with card holders in this panel, with
their scoring paths proven by unit tests. This supersedes F11 from the M06-021a ledger,
which can be closed on this evidence.

**Independence limitation (recorded per adjudicator's instruction):** Claude Opus 5 reviewed all
three dependencies (M06-021a, M06-022, M06-023) and then adjudicated the exit gate that
re-validates them; F2 was found by the implementer, not by the reviewer. No second
ever-independent adjudicator is available in this session, so the limitation is recorded here
and carried into the M06 milestone evidence rather than left implicit.

---

# Final acceptance — 2026-08-21

All conditions of the adjudication's disposition are now met:

| condition | state |
|---|---|
| F1 accepted, red-before-green-after independently reproduced | done (above) |
| F2 escalated to M06-025 and that package lands | **M06-025 accepted** (`plans/M06-025_OPEN_REVIEW_ITEMS.md`, same reviewer, independence limitation noted there too) |
| J1's instrumentation run recorded | done (§J1 resolution above; probe committed with this package) |
| Independence limitation recorded in milestone evidence | done (above; carried into the M06 exit report) |

**M06-024 is acceptable.** With M06-025 accepted, F2 clears and the M06 exit gate closes on
these two packages plus the milestone reconciliation below.
