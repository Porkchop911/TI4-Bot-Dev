# M07-023 — Stepped equivalence across pause→choice resumption (child of M07-022 review P2)

## Status

Dependency-safe preparation only. Begin after M07-022 is accepted and committed; **must complete
before the M07-020 exit review** (it is now a dependency of it), because the equivalence invariant
is currently verified across a scoring pause that decides nothing — no test composes a pause with a
choice at the retained frame.

| Field | Value |
|---|---|
| Milestone | M07 — Factions and Thunder's Edge |
| Depends | accepted M07-022 (finding P2 / review of the pausing fixture) |
| Permission class | P1 |
| Review tier | B; escalates to frontier if a pause-path divergence is found in engine behavior rather than as missing coverage |

## Objective

Close the coverage limit recorded in `plans/evidence/M07-022.md` §"Coverage limit recorded per
review P2": the reviewer instrumented both equivalence tests and found the two branches of
`stepped_fight` each covered but never composed — the pausing fixture pauses, resumes, and ends
with **zero** choices asked. Nothing verifies that a stepped driver resumes *into a choice* at the
retained frame after consuming a scoring pause.

That composition is exactly the failure mode M07-019's charter names first: *"a faction reaction
that was in flight when the window opened must resume at the exact retained frame; the failure mode
is a skipped or doubled effect, not a crash."* M07-019 pins it through `Game`; this package pins it
for the synchronous API and its stepped replica — which is what the M07-021/022 chain exists to
protect.

## Deliverables

1. **A pausing fixture that continues into a choice.** Extend
   `a_stepped_combat_matches_the_driven_one_across_a_barrage_pause` (or add a third sibling test)
   so the fight continues past the barrage pause into a casualty assignment — per P2's suggestion:
   more cruisers on the defender, dice that leave hits to absorb. The stepped driver must resume at
   the retained frame and answer the choice; outcomes must match and final states identical, with
   the barrage feat asserted on both sides as in M07-022.

2. **Proof the fixture is not vacuous for P2.** Two checks: (a) the new/extended fixture actually
   asks a choice *after* the pause — assert or instrument the branch sequence so a future fixture
   change that stops doing so fails informatively; (b) sensitivity to the old bug class — run the
   fixture against a harness without pause consumption (the pre-M07-022 shape, e.g. the reviewer's
   `break` probe) and record the failure, proving the fixture really pauses.

3. **No production code changes expected.** If the composition exposes an actual divergence in
   engine behavior between the stepped and driven paths, that is a finding to escalate per the tier
   row above — not something to paper over with fixture adjustments.

## Writable declaration (exact paths)

- `crates/ti4-engine/src/combat.rs` (**test module only** — fixture extension or new sibling test;
  no production code unless deliverable 3 escalates, in which case re-declare before editing).
- `plans/M07-023_POST_PAUSE_CHOICE_COMPOSITION.md`, `plans/evidence/M07-023.md`.
- `plans/EXECUTION_STATE.md` (checkpoint only).

Read-only reference: `crates/ti4-engine/src/game.rs` (Game-level pause semantics),
`crates/ti4-model/src/state.rs` (equality chain), the M07-019 tests in game.rs.

## Required checks

- Red-first per deliverable 2(b): fixture fails against the pre-pause-consumption harness shape;
  green under the shared `stepped_fight`.
- Focused: both existing equivalence tests + the new/extended one, individually named.
- Full engine suite; workspace suite ×2 (determinism); Clippy with **pasted** output (per M07-022's
  P1 lesson — no prose summaries of tool output in evidence); rustfmt on touched files;
  `git diff --check`.

## Scope statement

This package adds coverage for the pause→choice composition at the synchronous-API level. It does
not re-point equivalence tests at `Game` (deferred to M07-020's scope decision per the M07-021/022
evidence), does not change any engine behavior, and does not widen any registry or fixture content
beyond the one combat fixture named above.
