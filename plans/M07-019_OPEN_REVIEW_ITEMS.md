# M07-019 independent Tier-B review — post-M06 faction/TE revalidation

## Status

**Accept with one required correction (M1).** No source behavior changed, so nothing here blocks
on engine work. M1 is a claim in the evidence that the code does not support and must be corrected
before the package commits.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of the code under review. Reviewed M06-021a…025; the independence limitation recorded in the M06-024 adjudication still applies. |
| Reviewed | uncommitted working tree over `b721a9a` |
| Diff | `crates/ti4-engine/src/game.rs` +577/−0 (single hunk at 3203, inside `mod tests`), `plans/EXECUTION_STATE.md` +41 |
| Checks | engine **843** / workspace **1,316**, both reproduced; four new tests confirmed to be the only additions |

## Verification

### Diff shape — as declared

One hunk, `@@ -3202,0 +3203,577 @@ mod tests {`, zero deletions. The block ends at 3779;
`the_last_pass_opens_its_own_action_occurrence_before_status` at 3781 is pre-existing at
`b721a9a`. Exactly four tests were added. No public surface, no source file outside `game.rs`.

### Counts — reproduced

`cargo test -p ti4-engine` → 843 passed + 5 doctests. `cargo test --workspace` → 1,316 passed
(843 + 126 + 112 + 104 + 73 + 27 + 25 + 5 + 1), 0 failed. The four suites the evidence table
omits but the spec names by name were run here: faction 50/50, thunders 3/3, timing 32/32,
policy 112/112 — all green. `git diff --check` clean; `cargo fmt -p ti4-engine --check` leaves
game.rs clean, the five remaining diffs are the documented pre-existing drift in untouched files.

Note for the spec, not this package: there is **no suite named "redaction"** — `--lib redact`,
`observation`, `view`, `hidden` all match zero tests. The coverage exists (`choice.rs::redacted_for`
tests, `ti4-model/src/view.rs`) and rides in the workspace run, but the spec's checklist names a
suite that cannot be run by that name.

### Tests 1 and 3 are load-bearing — mutation-confirmed

Mutating the round-identity gate at `combat.rs:147` (`== Some(state.combat_round_seq)` →
`.is_some()`) turns `munitions_reserves_survive_the_barrage_scoring_pause` red on the `rerolls == 1`
assertion, with eleven `munitions:a` rerolls in the history instead of one. Mutating
`action_cards::move_bonus` to drop the activation comparison turns
`flank_speed_expires_at_the_activation_boundary_across_a_scoring_pause` red on
`move_bonus(state, &a, 2) == 0`. Both mutations reverted; all three touched files restored
byte-identically (game.rs md5 `2e641e51…` before and after).

Both mutations also kill a pre-existing test, so the new tests are not the only guard on the
identity rule itself — what they add is the guard on that rule *across the pause*, which is the
package's actual subject and which no other test covers.

### Test 4 — non-vacuous but shallow

`te_breakthrough_survives_the_combat_scoring_pause` asserts positive values
(`breakthrough == Some(letnevbt)`, `expedition_slices["trade_goods"] == a`,
`gravleash_move_values[ids[1]] == Some(2)`), so it is not vacuous. It is a persistence test over
fields the pause path never writes, so it would only catch a fairly gross regression. Fine as a
regression pin; it should not be read as coverage of TE/pause interaction.

### F-M07-019-1 — confirmed, correctly escalated

`invasion.rs:883` selects `.find(|unit| unit.owner != self.invader)`, so a PDS or Space Dock alone
makes a planet contested. Official rule: ground combat requires the defender's *ground forces*;
a planet with none falls without resistance. Structures are not ground forces. The escalation to
frontier adjudication is the right call — it changes invasion legality, and the fix is in a file
this package cannot write.

### F-M07-019-2 — confirmed, and the impact analysis holds

`combat.rs:1060` sets `Stage::RollingAfterBarrage` and returns **before** the `over()` check at
1063; `combat.rs:1243` resumes with `roll_round(..., run_barrage=false)`, and `Stage::Rolling` is
the only arm carrying an `over()` check. So after a barrage that both fires a feat and wipes a
fleet, the resumed round rolls both fleets. The "no observable state or choice difference" claim
checks out for a reason the evidence does not give: `combat_round_seq`, the `COMBAT_ROUND_STARTED`
emission, and the faction round-opening offers (Munitions Reserves among them) are **all inside the
`if run_barrage` block** at 1020–1043. The phantom round therefore cannot advance the round
identity, emit an event, or open an ability offer. Only the dice stream moves. Classification as
minor is correct, and the known-difference ledger entry the evidence asks for is warranted.

## Findings

### M1 — MEDIUM (required before commit) · the Assimilate test does not test Assimilate

`assimilate_runs_once_after_the_home_loss_scoring_pause` builds b's planet with `pds`, `pds`,
`spacedock` and invades with three infantry. Instrumented at the pause and after the resume:

```
standing: [("infantry","a"), ("infantry","a"), ("infantry","a")]
after:    [("infantry","a"), ("infantry","a"), ("infantry","a")]
```

b's three structures are **destroyed in the ground combat** — exactly as F-M07-019-1 predicts. What
`standing` holds is the invader's own infantry, so all three conversion assertions are vacuous:

| assertion | why it cannot fail |
|---|---|
| `!has_l1z1x(&standing)` | the units are plain `infantry` owned by `a`; true whether or not Assimilate ran |
| `after.len() == standing.len()` | compares a's own infantry against itself |
| `after.iter().all(\|u\| u.owner == a)` | they were always a's |

The last one also never checks the `l1z1x_` variant, even though `has_l1z1x` is defined right
above it — so even with surviving structures it would not verify the conversion.

The evidence's summary states the opposite: *"every surviving structure converted one-for-one to
a's l1z1x variants — count preserved, nothing duplicated, nothing left rival-owned."* Nothing
survived and no conversion was observed.

The test's **pause half is real and worth keeping**: two occurrences in the right order
(`WonInARivalHome` combat=true, then `LostAHomePlanet` combat=false), control transferred before
the pause, `captured == [(planet, Some(b))]`, window done after the second `settle`. That sequence
faithfully mirrors the real driver at `game.rs:349–372` (take → pause → settle → take → pause →
settle). Keep it.

**This cannot be fixed inside this package.** While `defender_on` counts structures, combat runs
until one side has no units and the defender rolls nothing, so structures can *never* survive a
contested invasion — Assimilate's conversion is unreachable through `InvasionWindow` by
construction, not by fixture choice. `invasion.rs` is not writable here.

**Required action.** (a) Correct the "What was done" paragraph to say the conversion assertions are
vacuous under current behavior and the test pins pause ordering and exactly-once capture only.
(b) Rename the test to what it actually asserts (e.g.
`the_home_loss_pause_holds_the_invasion_at_finalizing_control`). (c) Add
"Assimilate-after-pause coverage" to the F-M07-019-1 fix package as a required deliverable, so the
gap closes when the rule does.

### M2 — MEDIUM · `event_feats` is invisible to state equality, so the equivalence invariant cannot fail on it

The spec's trap #4 says: *"any new state field written by a faction/TE path must be part of the
projection or the direct-vs-stepped equivalence test fails."* `Player.event_feats`
(`state.rs:399`) — the M06 field that gates `did_at_occurrence` and therefore secret scoring
eligibility — is **not compared** in `Player::PartialEq`. Probe-confirmed: two states differing only
in `event_feats` compare equal.

This is inconsistent with the rest of the occurrence model, where `GameState::PartialEq` does
compare `scored_feat_occurrences` (`state.rs:868`) and `feat_occurrence_seq` (`state.rs:867`). It
also looks accidental rather than intended: the `impl PartialEq for Player` doc comment enumerates
the deliberate exclusions — *"Mirrors the oracle's `compare=False` on `relic_fragments`, `leaders`,
and `assimilated_technologies`"* — and `event_feats` is not among them, nor does its own doc
comment carry the `// Not compared.` marker that `assimilated_technologies` does.

Practical risk is bounded: a direct-vs-stepped divergence in feat evidence would usually also
diverge the scored objectives, which *are* compared. So this is detection latency, not permanent
blindness. But it means this package's headline invariant is satisfied by omission on precisely the
field M06 introduced.

**Required action.** Record it. The fix (add the comparison, or add the `// Not compared.` marker
with a reason) is a one-line change in `ti4-model`, outside this package's writable paths — so it
becomes a scoped child, per the spec's own rule.

### M3 — LOW (evidence) · the Clippy claim is false

Evidence: *"zero warnings in added code; only pre-existing `apply_tactical` too-many-lines
(game.rs:1260, untouched function) and pre-existing choice.rs unused-attribute remain crate-wide."*

`cargo clippy -p ti4-engine --lib --tests` reports five, and **two are in this package's new code**:

```
game.rs:1260  too many lines (103/100)   pre-existing (apply_tactical)
game.rs:3415  too many lines (109/100)   NEW — assimilate_runs_once_after_the_home_loss_scoring_pause
game.rs:3652  too many lines (107/100)   NEW — te_breakthrough_survives_the_combat_scoring_pause
choice.rs:568 unused attribute            pre-existing
strategy.rs:589 i64 → i32 cast            pre-existing
```

Both new sites are inside the 3203–3779 block. Cosmetic as warnings go, but the evidence asserts a
clean result that the tool does not give, and that assertion is the kind a later package will cite.

**Required action.** Correct the evidence line. Splitting the two tests or allowing the lint locally
is at the author's discretion.

### M4 — INFORMATIONAL · the redaction invariant is answered by argument, not by coverage

The evidence answers the spec's redaction invariant with *"no observation-boundary code was
touched."* True, and for a zero-source-change package that is a defensible answer. Recording what it
leaves open: `choice.rs::redacted_for` redacts `action_cards` and `secret_objectives` and nothing
else, so `Player.event_feats` and `GameState.scored_feat_occurrences` are visible in full to every
viewer. There is **no leak today** — a feat performed and the fact of a score are both public at a
real table, and the eligibility the spec worries about is protected because it derives from
`secret_objectives`, which *is* redacted. But nothing pins that judgement, so a later `Feat` variant
covering private information would leak silently.

## Disposition

**Accept.** M1 must be corrected in the evidence and the test renamed before the package commits;
M2 and M3 should be recorded, with M2 scoped as a child package; M4 is informational.

Both findings the package raises are real, correctly classified, and correctly escalated. The
package's honest result is three load-bearing nested-window regressions plus one ordering test —
not four — and two confirmed engine defects, one of which (F-M07-019-1) is the reason the fourth
test cannot yet assert what it was named for.

## Resolution (implementer, 2026-08-22)

All required corrections applied; re-verified before commit.

- **M1(a)** — `plans/evidence/M07-019.md` "What was done" item 2 rewritten: the three unit-
  conversion assertions are documented as vacuous under current behavior (b's structures die in the
  ground combat per F-M07-019-1; `standing` is a's own infantry), and the test is described as
  pinning pause ordering, pre-pause control transfer, exactly-once capture, and resume-to-done.
- **M1(b)** — test renamed to `the_home_loss_pause_holds_the_invasion_at_finalizing_control`; its doc
  comment now states what is load-bearing today versus what becomes load-bearing when F-M07-019-1
  is fixed; the two misleading assertion messages ("every surviving structure was assimilated
  exactly once", "Assimilate must wait…") reworded to what they actually check.
- **M1(c)** — `plans/evidence/M07-019.md` §F-M07-019-1 now carries the required deliverable of the
  fix package: Assimilate-after-pause coverage, including strengthening the ownership assertion to
  check for l1z1x_ variants and one-for-one count preservation against b's surviving structures.
- **M2** — recorded as finding F-M07-019-3 in `plans/evidence/M07-019.md` (with the reviewer's
  bounded-risk analysis) and scoped as child package **M07-021**: prep spec at
  `plans/M07-021_EVENT_FEATS_PROJECTION.md`, row added to `plans/M07_FACTIONS_AND_TE.md` with a
  hard ordering constraint (must complete before M07-020's exit review).
- **M3** — the two new `too_many_lines` sites resolved with targeted
  `#[allow(clippy::too_many_lines, reason = ...)]` matching the existing combat.rs precedent; the
  evidence Clippy line now states exactly what the tool reports (three pre-existing crate-wide
  warnings, zero in added code) and records that this correction followed the reviewer's first pass.
- **M4** — recorded as an informational note in `plans/evidence/M07-019.md` §"M4 note": no leak
  today, nothing pins the judgement; a later private-information `Feat` variant would leak silently.
  The spec's checklist no longer names a nonexistent "redaction" suite — it now names the actual
  coverage (`choice::redacted_for` tests and model view tests).
- **F-M07-019-2 refinement** — the reviewer's reason for why nothing besides the dice stream moves
  (round identity, `COMBAT_ROUND_STARTED`, and faction round-opening offers all live inside the
  `if run_barrage` block) is recorded in §F-M07-019-2 of the evidence.
- **Re-verification after corrections:** four tests pass individually under the new name; engine
  **843 + 5 doctests**; workspace **1,316 / 0** (single run post-correction — the pre-correction
  determinism pair stands for the unchanged test bodies); Clippy zero warnings in added code;
  game.rs rustfmt-clean under edition 2024; `git diff --check` clean. Diff is still a single purely-
  additive hunk inside `mod tests` (+588/−0).
