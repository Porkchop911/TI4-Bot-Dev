# M07-022 independent Tier-B review — stepped-vs-driven equivalence across scoring pauses

## Status

**Accept.** All three deliverables land and the red-first claim reproduces exactly. Three findings:
one evidence correction required before commit, one coverage gap worth scoping, one note.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of the code under review. Reviewed M06-021a…025, M07-019, M07-021; the independence limitation recorded in the M06-024 adjudication still applies. |
| Reviewed | uncommitted working tree over `5241f2d` |
| Diff under `crates/` | `ti4-engine/src/combat.rs` +119/−40 — one private helper in production code, the rest test module |
| Checks | engine **844** + 5 doctests, workspace **1,318**, both reproduced |

## Verification

### The `complete_window` refactor is behavior-preserving

Line-for-line equivalent to the tail it replaces. Old: `outcome().ok_or_else(Unresolved)?` → feats
if `combat_occurrence()` is `Some` → `Ok(outcome)`. New: `complete_window` returns `None` on no
outcome (recording nothing, exactly as the old early return did), otherwise records and returns
`Some`; `resolve()` maps that with the same `ok_or_else`. No ordering, no early-exit, and no
discarded-value semantics changed. The suite agrees: 844, +1 over base and that +1 is the new test.

### Red-first — reproduced, not taken on trust

Replacing `stepped_fight`'s pause branch with `break` (the pre-M07-022 shape):

```
a_stepped_combat_matches_the_driven_one_across_a_barrage_pause ... FAILED   (panic at combat.rs:2739)
a_stepped_combat_matches_the_driven_one                        ... ok
```

Both halves of the claim confirmed in one run: the new test is load-bearing, and the loop really
does degenerate to the old shape for fights that never pause, so the refactor did not quietly
change what the original test proves. Probe reverted; `combat.rs` restored.

### The chain actually closes — `identical()` reaches `event_feats`

Worth stating explicitly because the whole M07-019 → 021 → 022 chain depends on it and no evidence
file checks it: `GameState::identical` opens with `self == other` (`state.rs:960`) before its extra
field comparisons, so M07-021's `Player::PartialEq` addition does feed the `identical()` assertions
these tests make. Had `identical()` been a disjoint stricter comparison, M07-021 would have closed
nothing here. It is not, and it does.

### The both-sides feat assertion is a real guard

`Feat::BarrageTookTheLastFighters` is asserted on both states *before* `identical()`, so a future
fixture change that stopped firing the barrage feat would fail on the informative assertion rather
than passing a vacuous identity check. Correctly reasoned and correctly ordered.

### Scope and hygiene

Only `combat.rs` under `crates/`, as declared. `cargo fmt -p ti4-engine --check` leaves combat.rs
clean — the five remaining diffs are the documented pre-existing drift in untouched files.
`git diff --check` clean. Engine 844 + 5 doctests, workspace 1,318, both reproduced here.

## Findings

### P1 — LOW (evidence, required before commit) · the Clippy claim is wrong, on both halves

Evidence: *"no new warnings — only the two documented pre-existing engine warnings remain."*
`cargo clippy -p ti4-engine --all-targets` — the command the evidence names — reports:

```
choice.rs:568    unused attribute                      pre-existing
game.rs:1260     too many lines (103/100)              pre-existing
strategy.rs:589  casting i64 to i32 may truncate       pre-existing
combat.rs:2701   item in documentation is missing backticks   NEW — this package
```

Two errors. First, this package **does** introduce a warning: `pending_choice` unbackticked in
`stepped_fight`'s doc comment. Second, the pre-existing count is **three**, not two — M07-021's own
evidence and its review both recorded three.

Neither matters to the code. It matters that this is the second Clippy misreport in this chain
(M07-019's M3 was the first, and was corrected), and that this one is a regression against a number
the immediately preceding package got right.

**Required action.** Add the backticks (one character either side) and paste the tool's actual
output into the evidence rather than summarising it in prose. Given the repeat, pasting rather than
paraphrasing is the durable fix.

### P2 — MEDIUM · no test covers a choice *after* a pause

Instrumented both equivalence tests to log which branch of `stepped_fight` they take:

| test | pause branch | choices asked |
|---|---|---|
| `a_stepped_combat_matches_the_driven_one` | never | 1 — "assign a hit" |
| `…_across_a_barrage_pause` | once | **0** |

The two branches are each covered, and never composed. The pausing fixture — destroyer×1 vs
fighter×1 + cruiser×1, faces `[10,10,10,1]` — pauses, resumes, and ends without any decision
pending, so nothing verifies that a stepped driver resumes *into a choice* at the retained frame.

That composition is the failure mode M07-019's charter names in its first required invariant:
*"a faction reaction that was in flight when the window opened must resume at the exact retained
frame; the failure mode is a skipped or doubled effect, not a crash."* M07-019 pins it through
`Game`; nothing pins it for the synchronous API and its replica, which is what this package is
about. The equivalence invariant is now verified across a pause that decides nothing.

This is a narrower gap than N1 was and it does not undo what this package achieved. But the
evidence's framing — *"equivalence is verified across the M06 pause path"* — again claims slightly
more than the tests establish, which is the same shape of overstatement N1 corrected.

**Recommended action.** Extend the pausing fixture so the fight continues past the barrage into a
casualty assignment (more cruisers on the defender, dice that leave hits to absorb), or add a third
fixture that does. Small change, and it is the case most likely to actually diverge.

### P3 — INFORMATIONAL · sharing the helper removed the last independent check on it

Before this package the harness carried its own copy of the completion bookkeeping, so an error in
`resolve()`'s copy would surface as a divergence. Both sides now call `complete_window`, so a bug
*inside* the helper is invisible to both equivalence tests — they would agree on the wrong answer.

This is the correct trade and exactly what N2 asked for: drift between copies is the bug class that
actually bit, twice, whereas the helper's four lines are unlikely to be wrong. Recording it only so
that a green equivalence test is not later cited as validating the bookkeeping's *content*. What
validates that content is `a_driven_combat_continues_after_its_barrage_scoring_pause`, which asserts
the feat against `resolve()` directly.

## Disposition

**Accept.** P1 must be corrected before commit — add the backticks and paste the real Clippy output.
P2 should be scoped as the natural successor before M07-020's exit review, on the same reasoning
that put M07-022 there. P3 is informational.

The package does what N1 and N2 asked, diagnosed nothing away, and its red-first evidence reproduces
precisely. The one recurring weakness in this chain is not the code — it is that the evidence's
summary lines keep claiming a little more than the checks establish, in the same direction each
time.

## Resolution (implementer, 2026-08-22)

- **P1 (required) — corrected.** Backticks added at the site (`stepped_fight`'s doc comment now
  reads `` `settle` / `pending_choice` / `resolve` ``). The evidence's Clippy line is replaced by
  the tool's actual pasted output: exactly the three documented pre-existing warnings remain
  (choice.rs:568, game.rs:1260, strategy.rs:589), zero new. Post-fix re-verification recorded in
  the evidence: combat.rs rustfmt-clean; both equivalence tests ok; engine **844 + 5 doctests**;
  workspace **1,318 / 0**. Per P1's instruction the durable fix is pasted output rather than prose
  summaries — adopted for this package and noted in the M07-023 spec's required checks.
- **P2 (scoped) — successor named before the exit review.** **M07-023** prep spec written at
  `plans/M07-023_POST_PAUSE_CHOICE_COMPOSITION.md`, milestone-plan row added, M07-020's dependency
  list now includes it. Scope per P2's recommendation: extend the pausing fixture (or add a third)
  so the fight continues past the barrage into a casualty assignment; deliverables include proof of
  non-vacuity for exactly this gap (choice-after-pause asserted, old bug class reproduced against
  the pre-M07-022 harness shape). The evidence's framing is corrected in its new §"Coverage limit
  recorded per review P2": equivalence holds on `event_feats` across a scoring pause **with no
  choice after it**.
- **P3 (informational) — recorded.** Evidence gains the note that sharing `complete_window`
  removed the last independent check on its content: both equivalence tests now agree by
  construction, so a green equivalence test must not be cited as validating the bookkeeping's
  content; what validates that is `a_driven_combat_continues_after_its_barrage_scoring_pause`.
- **Re-verification:** code change since the review is doc-comment-only (the P1 backticks); engine,
  focused tests, fmt, and one workspace pass re-run post-fix with unchanged counts — recorded in
  the evidence.
