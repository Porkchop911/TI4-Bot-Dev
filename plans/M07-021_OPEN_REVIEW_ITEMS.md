# M07-021 independent Tier-B review — `event_feats` state-equality projection

## Status

**Accept. Scope extension into `combat.rs` approved.** Two findings, neither blocking, both about
what the completed harness still does not cover rather than about anything this package did wrong.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of the code under review. Reviewed M06-021a…025 and M07-019; the independence limitation recorded in the M06-024 adjudication still applies. |
| Reviewed | uncommitted working tree over `c034549` |
| Diff under `crates/` | `ti4-model/src/state.rs` +19/−0 (one compared field + one focused test), `ti4-engine/src/combat.rs` +18/−1 (test module only) |
| Checks | model **74**, engine **843**, workspace **1,317**, all reproduced; clippy model 0, engine 3 pre-existing |

## Verification

### Option A is the right call

`Player.event_feats` is a Rust-only M06 field with no oracle ancestor, so it was never covered by
the `compare=False` list the `impl PartialEq for Player` doc comment enumerates. Its omission was
accidental, and Option B would have required inventing a justification for an exclusion nobody
chose. The comparison is added with a reason recorded at the site. Correct.

### Red-first — reproduced, not taken on trust

Removing the single `&& self.event_feats == other.event_feats` line turns
`state::tests::event_feats_participate_in_state_equality` red at `state.rs:1477`. Restored; model
suite back to 74/74. The new test is load-bearing.

### The exposed dependence — reproduced, and the diagnosis is exactly right

Reverting **only** `combat.rs` while keeping the projection change reproduces the reported failure:

```
combat::tests::a_stepped_combat_matches_the_driven_one ... FAILED
panicked at combat.rs:2735: assertion failed: stepped_state.identical(&driven_state)
```

The named cause checks out. `note_combat_event_feats` records
`Feat::HeldThreeShipsAfterASpaceCombat` at `combat.rs:1681` for every side in `before.sides` holding
≥3 non-fighter ships — the attacker's three cruisers qualify. It reads `before.sides`, which is why
the snapshot is not optional. The driven state carried the feat; the stepped one did not.

### The scope extension is justified — the gap really is test-only

The decisive question is whether the harness was papering over a production divergence. It was not.
`CombatWindow::new` has exactly four call sites, two of them tests:

| site | bookkeeping |
|---|---|
| `combat.rs:1468` — synchronous `resolve()` | `before_combat` at 1467, `note_combat_event_feats` at 1492–1495 |
| `game.rs:216` — the Game driver | `before_combat_with_notes` at 209, `note_combat_event_feats` at 279–286 behind a `feats_noted` guard |
| `combat.rs:2659`, `combat.rs:2704` | tests |

Both production consumers already did the work. The stepped branch of
`a_stepped_combat_matches_the_driven_one` was an incomplete replica of `resolve()`, and the
projection change is what made that incompleteness observable. Completing the replica is the right
fix, no assertion was weakened, and `identical()` stands as written. The extension is test-module
only and changes no engine behavior. **Approved**, on the M06-025 precedent as declared.

### Counts and checks — all reproduced

model 74/74 (was 73, +1), engine 843/843 (unchanged), workspace 1,317/0 (was 1,316, +1).
`cargo clippy -p ti4-model --lib --tests` → zero. `cargo clippy -p ti4-engine --lib --tests` → three,
all pre-existing (`choice.rs:568` unused attribute, `game.rs:1260` too-many-lines, `strategy.rs:589`
cast) — the two M07-019 test warnings are gone, so the M3 correction landed as claimed.

## Findings

### N1 — MEDIUM · the completed harness still stalls on any fight that pauses

The stepped branch gained `resolve()`'s **feat bookkeeping** but not its **pause consumption**.
`resolve()` loops on `while window.outcome().is_none()` with `drive()`,
`take_scoring_occurrence()`, and `settle_open()` (combat.rs:1478–1487). The harness loops on
`while let Some(choice) = window.pending_choice(…)`, and `pending_choice` returns `None` for
`Stage::RollingAfterBarrage` (combat.rs:1259). A fight whose round-1 barrage fires a feat therefore
leaves the loop with the fight unresolved, and the new `.expect("the fight resolved")` panics.

Probe-confirmed. Running the harness's own stepped shape against the pausing fixture from
`a_driven_combat_continues_after_its_barrage_scoring_pause` (destroyer×1 vs fighter×1 + cruiser×1,
faces `[10,10,10,1]`):

```
probe_stepped_harness_on_a_pausing_fight ... FAILED
PROBE: the pending_choice loop exited with the fight unresolved (stage stalled at a scoring pause)
```

Probe removed; `combat.rs` restored.

The test passes today only because its fixture — cruiser×3 vs carrier×2 — has no fighters and so
never pauses. **The direct-vs-stepped equivalence invariant is therefore verified only on
non-pausing fights, which excludes exactly the M06 paths M07-019 and this package exist to
protect.** This is not a regression introduced here and it is not this package's job to fix; before
M07-021 the test could not have caught a pause-path divergence either, because feat evidence was
invisible to equality. But the package's own evidence says the invariant now "holds on the field M06
introduced", and that is true only outside the pause.

**Recommended action.** Record the limit in the evidence, and scope the extension —
a pausing fixture plus the `take_scoring_occurrence()` / `settle_open()` loop — as a follow-up.
Given the spec's hard ordering constraint that M07-021 precede M07-020's exit review, that
follow-up should be named before the exit review rather than after it.

### N2 — LOW · the harness replicates `resolve()` rather than exercising the stepped production path

`a_stepped_combat_matches_the_driven_one` compares `resolve()` against a hand-written copy of
`resolve()`. It can only catch drift between the copy and the original — which is precisely what
happened here, and what the original omission was. It never touches `game.rs:216`, the actual
stepped production consumer.

The two production paths also deliberately differ at the snapshot: the Game driver takes
`before_combat_with_notes` (carrying the promissory notes the `baf` feat needs), `resolve()` takes
plain `before_combat`. The harness correctly mirrors `resolve()` for what it compares — but it means
M07's stated invariant, *"direct and stepped tactical APIs produce equivalent faction/TE state"*, is
not the proposition this test establishes.

**Recommended action.** At the author's discretion, and cheap either way: factor the completion
bookkeeping (`before_combat` → `note_combat_event_feats` on `combat_occurrence()`) into one helper
that `resolve()` and the harness both call, so a third drift is structurally impossible; or point
the equivalence test at `Game` so it measures the invariant it is named for.

## Disposition

**Accept.** The scope extension into `combat.rs` is approved as a genuine test-only completion, on
the evidence that both production consumers already perform the bookkeeping. N1 should be recorded
in the evidence and scoped before M07-020's exit review; N2 is at the author's discretion.

The package does exactly what M07-019's M2 asked for, diagnosed the exposed dependence instead of
regenerating around it, and reported it accurately — including the fact that it had to widen scope
to do so. The evidence's one overstatement is that the equivalence invariant now holds on
`event_feats`; it holds on `event_feats` for fights that do not pause.

## Resolution (implementer, 2026-08-22)

- **N1** — recorded in `plans/evidence/M07-021.md` §"Coverage limit recorded per review N1": the
  corrected claim is that the invariant holds on `event_feats` **for fights that do not pause**, and
  the earlier "holds on the field M06 introduced" wording is explicitly retracted as an
  overstatement. Scoped follow-up named before the exit review, as instructed: **M07-022** (prep
  spec `plans/M07-022_STEPPED_EQUIVALENCE_ACROSS_PAUSES.md`, milestone-plan row added) — pause
  consumption in the stepped harness plus a pausing-fixture comparison. M07-020's dependency list
  now includes 022.
- **N2** — disposition recorded in `plans/evidence/M07-021.md` §"N2 disposition": the helper
  factoring (one completion-bookkeeping function shared by `resolve()` and the harness) is adopted
  into M07-022, where the replicated surface grows anyway; re-pointing an equivalence test at
  `Game` itself is deferred to M07-020's scope decision as a larger design change (including the
  deliberate `before_combat_with_notes` vs `before_combat` snapshot difference between the two
  production consumers).
- **Scope extension** — accepted as approved; no further action.
- **Re-verification:** none required by this resolution — it is documentation-only (evidence,
  spec, milestone plan, review file); the committed code state and its verification numbers stand
  unchanged (model 74/0, engine 843/0, workspace 1,317/0 ×2).
