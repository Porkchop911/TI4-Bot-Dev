# M07-020 independent Tier-C frontier adjudication — reopened M07 exit review

## Status

**Do not close the gate yet.** The campaign itself is good work and most of it verifies. But the
one finding this review exists to adjudicate is absent from it, and the spec's own Definition of
Done says M07 must have "no unresolved finding" before M08-018 begins.

One blocking finding, three supporting. None requires new engine work — R1 requires a **decision**,
not a fix.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of the frontier. Reviewed M06-021a…025 and every package in this frontier (M07-019, 021, 022, 023) as the independent Tier-B reviewer. **This is a limitation:** I am independent of the implementer but not a fresh perspective on this range — I formed the M07-019 findings that R1 concerns. Recorded per the M06-024 precedent. |
| Frontier | `b721a9a..8ba6edc`, 4 commits |
| Diff under `crates/` | 3 files, **+816/−24** — reproduced exactly |

## What verifies

**The diff claim is exact.** `git diff --stat b721a9a..8ba6edc -- crates/` gives 3 files,
+816/−24, split as the evidence describes: `game.rs` +588/−0 test-only, `combat.rs` +233/−24 with
one production hunk (`complete_window`), `state.rs` +19/−0 (the `event_feats` equality line and its
test). "No other production behavior changed in the M07 range" is true.

**F-M07-020-1 is correct.** `ground_roll_suppressed_round` and `sustained_damage_round` have
exactly three sites each — declaration (`state.rs:363`/`366`), `PartialEq`, and the initializer.
No read, no write. Genuinely inert, and the recommendation attached to it is the right one.

**Registry reconciliation holds.** `registered()` returns 14 ids; `blocked()` returns six
exclusions each carrying a written reason. `UNREDACTED = ["promissory_notes"]` and
`PRIVATE_SEQUENCES = ["action_cards", "secret_objectives"]` are pinned by an exact-value test
(`view.rs:198-199`), so the named gaps cannot drift silently.

**Gates reproduce.** Engine 845 + 5 doctests, workspace 1,319 across two runs, replay 4/4, Clippy
three pre-existing and zero new — all reproduced independently across this session.

**Campaigns 1, 2 and 4 are genuinely traced,** with code references I spot-checked and found
accurate. The occurrence-membership cap reaffirmation (61.7, M06-021a F1 option (b)) is correct.

## Findings

### R1 — BLOCKING · the finding this review was convened to adjudicate is not in it

M07-019 escalated F-M07-019-1 (`invasion.rs::defender_on` counts structures as ground defenders,
against LRR 49 — a planet with no enemy *ground forces* falls without resistance) with these words:

> **Escalated to the Tier B reviewer** — it changes legality/timing semantics of invasions, so per
> AGENTS.md a frontier-model adjudication is required before any fix package is scoped.

**M07-020 is that frontier adjudication.** It contains zero mentions of `F-M07-019-1`,
`defender_on`, or "ground defender" — in the spec or the evidence. The findings ledger holds only
the two informational entries this campaign generated itself. The defect is still live and
unchanged at `invasion.rs:883`:

```rust
.find(|unit| unit.owner != self.invader)
```

The Definition of Done reads: *"every actionable finding is resolved and rechecked … and M07 has
no unresolved finding. Only then may M08-018 begin."* A finding routed to this gate and not
addressed by it is unresolved by definition.

**To be clear about what is required: a decision, not a fix.** Three dispositions would each close
this, and any of them is legitimate —

1. fix now in a scoped child package;
2. accept as a recorded known difference with a scoped fix package deferred to a named milestone;
3. reject the finding on rules grounds, with the reasoning recorded.

What is not available is silence. **My recommendation is (2).** The rules deviation is real, but its
blast radius is bounded: structures roll no dice, so the invader takes no casualties from them and
always wins the spurious fight; the observable difference is that structures die in combat rather
than on control transfer, plus the dice and choices that fight consumes. Its one concrete
consequence — L1Z1X Assimilate's structure conversion being unreachable through `InvasionWindow` —
is a faction-ability gap that should be fixed but does not block bot revalidation, since M08's
accepted baseline was built on this same M05-era behavior. Deferring is defensible; deferring
silently is not.

### R2 — MEDIUM · the "guards-the-guard" assurance is false, and M06 already proved it

Campaign 3 states that redaction is pinned

> including the guards-the-guard check (`leaks()` reports an unredacted state, so **a newly added
> private field fails a test instead of leaking quietly**).

`leaks()` (`ti4-policy/src/view.rs:85`) does not do this. It iterates two hardcoded field lists:

```rust
for card in &player.action_cards   { … }
for secret in &player.secret_objectives { … }
```

It is a hand-written mirror of `redact_player`'s two fields, not an enumeration over `Player`. A
third private field added to `Player` is redacted by neither and reported by neither. The function's
own doc comment makes exactly the promise the evidence repeats — *"so a newly added private field
that nobody redacted shows up as a failing test instead of a quiet leak"* — and the implementation
does not keep it.

**M06 is the proof case.** `Player.event_feats` was added during M06 and is redacted by neither
`choice.rs::redacted_for` nor `ti4-policy`'s `redact_player`, and `leaks()` stayed silent. I raised
this as M07-019's M4; it happens to hold table-public information, so there is no actual leak today
— but the mechanism this gate relies on to guarantee there is none demonstrably did not fire when
tested by real events.

At a Tier-C gate whose named subject is hidden information, an assurance that the guard is
self-extending, when it is a two-item list, is the wrong thing to carry forward. **Required action:**
correct the claim in the evidence. Making `leaks()` actually field-complete is a separate, larger
question — reasonable to defer, provided it is deferred in writing rather than believed.

### R3 — LOW/MEDIUM · the "milestone known-differences ledger" does not exist

F-M07-020-2 is "carried into the milestone known-differences ledger", and M07-019's F-M07-019-2
(phantom post-pause dice consumption) was likewise judged to warrant "a known-difference ledger
entry". No such document exists: nothing in `plans/` matches `*known*` or `*differ*`, and the only
occurrences of the phrase are two prose references in `EXECUTION_STATE.md` pointing at it.

Findings are being carried to a destination that has never been created. **Required action:** create
it, or name the real destination. This matters beyond bookkeeping — M12 qualification is where these
differences have to be answerable from.

### R4 — LOW · two live carries from M07-019 are recorded nowhere in the exit evidence

Neither F-M07-019-2 (phantom round after a total barrage wipe) nor F-M07-019-3 (`event_feats`
equality, closed by M07-021) appears in M07-020. F-M07-019-3 is genuinely resolved and needs only a
line saying so. F-M07-019-2 is unresolved and unfixed, and is the second finding whose recorded
disposition was "note it in the ledger" (see R3).

Also unrecorded: M07-019's own evidence made **Assimilate-after-pause coverage a required
deliverable of the F-M07-019-1 fix package** — a package that does not exist and that this review
does not scope. It should ride with whatever R1 decides.

## Disposition

**Blocked on R1.** Adjudicate F-M07-019-1 explicitly — fix, defer with a scoped package, or reject
on rules grounds — and record the reasoning. Correct R2's claim, resolve R3's missing destination,
and fold R4's carries into it. None of that is engine work; it is one decision and one page of
recording, and it can be done inside M07-020 without a child package.

I want to be plain that this is not the chain extending itself again. R1 is not a new gap I found
in the code — it is a finding raised four packages ago, deliberately routed to this gate by the
protocol, and then not answered when the gate ran. The three supporting findings are all
documentation. **M07 can close today**, on the strength of a campaign that is otherwise thorough and
accurate, as soon as the escalated finding gets the decision it was escalated for.

Once R1 carries a recorded disposition, I would accept this gate and M08-018 may begin.

## Resolution (implementer, 2026-08-22)

All four findings resolved inside this package; no engine work was required or done. The reviewer's
recommendation on R1 was followed.

- **R1 — DECIDED: option 2.** F-M07-019-1 is accepted as a recorded known difference (**KD-2** in
  the new `plans/KNOWN_DIFFERENCES.md`) with the fix scoped as **M08-020**
  (`plans/M08-020_GROUND_COMBAT_STRUCTURE_LEGALITY.md`), hard-ordered before M08-018 (milestone
  row added to `plans/M08_AUTHORED_BOTS.md`; M08-018's dependency line updated). Rationale, in full
  in the evidence under "Adjudication of F-M07-019-1": option 3 unavailable (LRR 49 verified in
  M07-019 — the deviation is real); option 1 rejected because a legality/timing behavior change at
  the exit gate, after four packages were accepted against current behavior, would invalidate their
  baselines silently — the same class of change M06-025 handled as its own numbered package with a
  re-baseline note; option 2 before 018 keeps every downstream baseline comparable exactly once.
  The Assimilate-after-pause coverage (M07-019 review M1c) is written into M08-020's spec as
  required behavior item 4, per R4's instruction that it ride with the decision.
- **R2 — corrected.** Campaign 3 no longer claims a self-extending guard. The correction names what
  `leaks()` actually is (a two-field hand-written mirror), cites M06's `event_feats` as the proof
  case where it demonstrably did not fire, and defers field-completeness **in writing** as ML-1 in
  the ledger — with the condition that any future package adding a private field to `Player`
  extends both redaction implementations and the leak check in the same commit, red-first.
- **R3 — destination created.** `plans/KNOWN_DIFFERENCES.md` now exists: KD-1 (the M06-closure
  baf/sb comparability break that had been carried to this nonexistent document), KD-2, KD-3,
  KD-4, plus mechanism limitations ML-1 and ML-2. Each entry names its source package, exact
  scope, and what would make it moot or require re-checking; M12 answerability is stated in the
  header.
- **R4 — carries folded in.** New "Carries from M07-019" section in the evidence: F-M07-019-2 →
  KD-3 (unresolved, unfixed, scope reviewer-refined to dice-stream position only); F-M07-019-3
  closed by M07-021 (`5241f2d`), one line; Assimilate coverage rides with M08-020.
- **Re-verification:** none required — the resolutions are documentation-only (evidence, ledger,
  milestone plan row, M08-018 dependency line, new M08-020 prep spec). No source file under
  `crates/` was touched; the reproduced gate numbers stand.

**Gate status: accepted.** R1 carries a recorded disposition; every actionable finding is resolved
and rechecked (or, for KD-3, explicitly deferred with its scope and re-run condition recorded).
M07 closes on this record.
