# M07-023 independent Tier-B review — stepped equivalence across pause→choice resumption

## Status

**Accept.** Two findings, both one-line hardening **inside this package** — neither should become a
child package. This chain should end here and M07-020 should proceed.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of the code under review. Reviewed M06-021a…025, M07-019, M07-021, M07-022. |
| Reviewed | uncommitted working tree over `7f357b6` |
| Diff under `crates/` | `ti4-engine/src/combat.rs` +67/−0, test module only, no production code |
| Checks | engine **845** + 5 doctests, workspace **1,319**, clippy three pre-existing / zero new — all reproduced |

## Verification

### The composition is real — measured, not just argued

The evidence argues structurally that no choice can arise before the barrage pause in this fixture.
That argument is correct, and it is now measured rather than reasoned. Instrumenting
`stepped_fight` to log which branch it takes, in order:

```
PROBE order: PAUSE-BRANCH
PROBE order: CHOICE assign a hit
```

One pause, then one choice, in that order. This is genuinely the pause→choice composition P2 named,
and it is the first test in the chain to cover it.

### Non-vacuity probe — reproduced

Replacing `stepped_fight`'s pause branch with `break` (the pre-M07-022 shape):

```
…_across_a_pause_and_assignment ... FAILED
…_across_a_barrage_pause        ... FAILED
a_stepped_combat_matches_the_driven_one ... ok
```

Matches the evidence exactly. The new fixture really pauses, pause consumption is what lets it
reach the assignment, and the non-pausing test still degenerates correctly. Probe reverted.

### The log-equality assertion adds real coverage

`stepped_log == driven_log` is not decoration: `DecisionLog` derives `PartialEq`, both sides answer
through the same `table` under the same default decider, and the assertion establishes that the two
drivers were asked the same choices in the same order — a stronger proposition than final-state
identity alone. Using the existing `Table` log rather than adding a counting wrapper was the right
call.

### Counts and hygiene — all reproduced

engine 845 + 5 doctests (+1 over base, and that +1 is the new test); workspace 1,319; `cargo clippy
-p ti4-engine --all-targets` gives exactly the three documented pre-existing warnings and nothing
new; combat.rs rustfmt-clean; `git diff --check` clean; test module only.

**The Clippy claim is correct for the first time in this chain.** Pasting the tool's output instead
of paraphrasing it — P1's required action from M07-022 — worked. Keep it.

## Findings

### Q1 — MEDIUM · the ordering the test is named for is argued, never asserted

The test is `…_across_a_pause_and_assignment`, and its value is that the choice comes *after* the
pause. Assertion 3 checks only that an `"assign a hit"` record for the defender exists **somewhere**
in the log. Nothing in the test asserts it came after the pause.

The ordering is currently guaranteed by fixture structure — `Announcing` cascades without a choice
because `retreats()` is empty in the arena, and `Rolling`/`RollingAfterBarrage` return `None` from
`pending_choice`. That reasoning is sound; I verified it holds. But it is fixture-dependent
reasoning propping up the invariant the test is named for, in a file where fixtures have been
adjusted three times in as many packages. A fixture change that let a choice precede the barrage
would leave the test green while it stopped testing the composition.

**Recommended action, inside this package.** One assertion in `stepped_fight` or at the test site
that pins the order — e.g. record the index at which the pause branch first fires and assert the
first log record post-dates it. A comment is what exists now; a comment is what M07-019's M1 also
had.

### Q2 — LOW · the log-equality assertion silently depends on the harness's second table staying unasked

`stepped_fight` carries two tables: `ctx.table` (`inner`) and the ask table passed by the caller.
Only the caller's table feeds `stepped_log`. Probe-confirmed: `ctx.table.log.records.len() == 0` for
this fixture, so the comparison is whole today.

It is not structurally whole. In the driven path everything is asked through the one table
`resolve()` receives; in the stepped path anything asked internally through `ctx.table` lands in
`inner` and never reaches the assertion. A fixture using factions with combat-round offers — Letnev
Munitions Reserves is the obvious one, and M07-019 already built exactly that fight — would split
the two logs and fail `stepped_log == driven_log` for a reason unrelated to stepped-vs-driven
equivalence.

M07-022's evidence called the two-table structure "behavior-neutral", which was true when nothing
asserted on either log. Adding the log-equality assertion made it load-bearing without anyone
noticing.

**Recommended action, inside this package.** One line at the end of `stepped_fight`:
`assert!(ctx.table.log.records.is_empty(), …)`. That turns a future silent divergence into an
informative failure and documents the dependency at the site where it lives.

## Disposition

**Accept**, with Q1 and Q2 applied here rather than scoped as M07-024. Both are single assertions in
a function this package already owns; neither is new behavior, new coverage, or new scope, and
neither meets the bar for a package of its own.

Stating this explicitly because the chain invites the opposite: M07-019's review spawned 021, 021's
spawned 022, 022's spawned 023, and M07-020's exit review has receded four times. This package
closes P2 cleanly and introduces no gap of the same kind. **Q1 and Q2 are hardening, not a gap —
apply them and take M07-020.** Any remaining harness-fidelity questions belong in the exit review's
known-limits ledger, adjudicated once, rather than as a fifth child.

## Resolution (implementer, 2026-08-22)

Both findings applied inside this package, exactly as dispositioned — no M07-024 spawned.

- **Q1** — `stepped_fight` now returns `(CombatOutcome, Option<usize>)`: the outcome plus how many
  choices had been asked when the first scoring pause was consumed. Consumption stays unconditional
  (the harness still mirrors `resolve()` exactly); only the measurement is new. The test asserts
  `stepped_asks_before_pause == Some(0)` — the fixture pauses and no choice may precede the barrage
  pause, so every recorded ask (the assignment included) came after it. A fixture change that lets
  a choice precede the barrage now fails informatively.
- **Q2** — `stepped_fight` ends with `assert!(ctx.table.log.records.is_empty(), …)`; a future
  fixture routing an internal ask through the context's table (Letnev Munitions Reserves is the
  reviewer's example) fails informatively instead of comparing split logs. The two-table comment at
  the site was updated — M07-022's "neither is asserted on" wording was stale once the log-
  equality assertion landed.
- **Re-verification (post-resolution):** all three equivalence tests ok; engine **845 + 5
doctests** (count unchanged); workspace **1,319 / 0 identical ×2**; Clippy — same pasted output as
  the review's own run (three pre-existing warnings, zero new); combat.rs rustfmt-clean;
  `git diff --check` clean. Final diff under `crates/`: +104/−15, test module only.
- **Chain closed.** Per the reviewer's instruction, M07-020 proceeds next; any remaining
  harness-fidelity questions go to its known-limits ledger for one-time adjudication.
