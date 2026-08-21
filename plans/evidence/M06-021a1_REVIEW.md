# M06-021a1 — independent review

| Field | Value |
|---|---|
| Package | M06-021a1 — occurrence model and event-scoring semantics |
| Review tier | C — timing, legality, hidden information |
| Reviewer | Claude Opus 5 (not the implementer of a1) |
| Reviewed | working tree on `wp/m06-021a-event-scoped-secret-timing`, uncommitted, parent `92edea4` |
| Diff reviewed | `state.rs` +84, `secrets.rs` +160, `objectives.rs` +170 |
| Verdict | **a1 accepted with findings.** F1 blocks M06-021a2a as currently specified. |

**Independence note.** I authored M06-021, the package a1 corrects, so this review is
independent of a1 but not of the defect a1 responds to. F5 concerns my own code.

## The finding against M06-021 is upheld

Rule 61.7 permits any number of objectives during an action turn or agenda phase and
caps scoring at one per combat. `Game::advance_turn` opened a single window per turn
with an implicit one-score cap, which is wrong in both directions: too restrictive on
count, and scoped to the turn rather than to the combat. I had recorded this as an open
question ("one objective per turn, or per combat?") and judged it "rare enough not to
matter". That was the wrong call — it is a rules defect, not a tuning question, and the
review correctly escalated it.

## What is right

- **Occurrence model.** `FeatOccurrence` is monotonic, allocation is checked, recording
  is idempotent, and matching is exact on `(player, feat, occurrence)`. The tests cover
  owner attribution and cross-occurrence isolation, which are the two ways this could
  leak.
- **Migration seam.** `EventScope::{LegacyTurn, Occurrence}` keeps every existing caller
  on its old path with no behaviour change, so a1 is genuinely inert at runtime. That is
  the right shape for a split this size.
- **Backward compatibility.** `#[serde(default)]` on both new fields; old saved states
  remain readable.
- **Sequencing arithmetic.** I traced the `keep` calculation against the reversed
  `pending` vector and the `rev().enumerate()` offset in `next_askable`. It is correct
  in both branches: `AnyPerPlayer` retains the scorer as the next to be asked, and
  skipped players are dropped in both. Termination holds because every accepted score
  both records the objective and removes the secret from hand, so the eligible set
  strictly shrinks.
- **Evidence discipline.** `M06-021a2a.md` states "implementation not started" and
  claims no test or review evidence for unimplemented behaviour. `EXECUTION_STATE.md`
  says plainly that "no actual game timing is claimed fixed". Both are accurate.
- Workspace tests pass: `ti4-engine` 806 (from 802), no new Clippy warnings.

## Findings

### F1 — BLOCKING (design, before a2a is implemented)

**The one-per-combat cap is enforced by `ScoringWindow::pending`, not by the
occurrence.** Nothing in `GameState` records that a player has already scored against a
given `FeatOccurrence`, and `ScoringWindow::scored` is per-window. Two windows opened
for the same occurrence therefore each grant a score.

Demonstrated, not inferred. A temporary probe (added, run, removed; tree restored to
byte-identical diffstat) opened two `OnePerPlayer` windows on one occurrence:

```
first  window -> scored "btv"
second window -> offered "dtgs", scored it
state.scored_by(a).len() == 2      for a single combat occurrence
```

This matters because of the shape a2a plans: *"Apply the same mechanism to space-cannon
and completed space combat, with `AnyPerPlayer`, `AnyPerPlayer`, and `OnePerPlayer`
respectively"* — i.e. separate pauses for anti-fighter barrage and for the completed
space combat. **AFB is inside the space combat**: the engine rolls it in
`CombatWindow::roll_round` at round 1, whereas space cannon offense runs earlier in
`AftermathWindow::new`, outside `CombatWindow` entirely. So the planned split lets a
player score Fight with Precision during AFB and Unveil Flagship after the same combat —
two objectives during or after one combat.

Two ways out:

- **(a) One occurrence, one window per space combat**, allocated when `CombatWindow`
  opens and settled after resolution. Simple, and the cap follows from there being one
  window. Cost: Fight with Precision's printed "during the anti-fighter barrage step"
  becomes an approximation, and a fighter-clearing AFB that ends the combat still works
  because the window is after.
- **(b) Keep both pause points and move the cap onto the occurrence.** Record
  `(player, FeatOccurrence)` scores in `GameState` and have `scoreable_event` exclude a
  player who already scored in that occurrence. The cap then holds however many windows
  a combat opens.

**Recommend (b).** It preserves the printed timing, and it makes the cap structural
rather than a side effect of how many windows someone happened to open — which is the
property that just failed the probe.

*Caveat:* I could not verify 61.7's text from this repository. `rules.json` contains
eight Fracture entries, not the LRR. This rests on the rule as quoted in the package
specification plus the engine's own step structure.

### F2 — HIGH (design, before a2b)

**Ground combat is per planet.** An invasion of three defended planets contains three
combats, each carrying its own one-objective cap, and bombardment is not a combat at
all. a2b covers "bombardment, control-loss, pass, and agenda emitters" without saying
how many occurrences an invasion allocates. One occurrence per invasion would apply the
cap far too broadly and would misclassify the bombardment window. Pin the granularity in
the specification before implementing.

### F3 — MEDIUM

`feat_occurrence_seq` is excluded from `GameState`'s `PartialEq`, while its four
siblings — `combat_round_seq`, `production_seq`, `activation_seq`, `turn_seq` — are all
compared (`state.rs:878-879`). The field comment justifies this as "transient timing
state, like the feat ledgers", but the ledgers are per-`Player` evidence whereas this is
a sequence counter, and it now determines which scoring windows open. Given replay
determinism is an accepted contract, either compare it or record why it differs from the
four counters it sits beside.

### F4 — LOW (latent)

In `ScoringWindow::resolve`, `pending` is truncated *before* the award. Under
`AnyPerPlayer` the scorer stays pending, so a failed `secrets::award` falls through to
the public `award`, which errors on a secret alias, while the player remains eligible.
Unreachable today — only `dhw` and `fsn` have costs and both are status-timed, and
status never reaches an event window — but it becomes live the moment a costed action or
agenda secret exists. Cheap guard: if the alias is a known secret, never fall through to
the public path; return the scoring error and drop the player from `pending`.

### F5 — INFORMATIONAL

a1 changes no game-loop path, so the shipped engine still opens `for_event(...)` with
`LegacyTurn` and `OnePerPlayer` from `advance_turn`. **The 61.7 defect remains live until
a2a and a2b land.** Correctly recorded in the evidence and execution state; noted here so
the parent's completion boundary is unambiguous.

### F6 — LOW

The turn-scoped ledger (`feats`, `record_feat`, `did_at_turn`, `did_this_turn`,
`anyone_did_at_turn`, `for_event`, `EventScope::LegacyTurn`) and the occurrence-scoped
one now coexist, and every production emitter still writes only the former. a2b's
definition of done should include deleting the legacy path once the last emitter
migrates, or the engine keeps two mechanisms for one job and the dead one will drift.

## Disposition

- **a1: accepted.** F3, F4 and F6 are non-blocking and can be resolved in a2b.
- **a2a: do not implement as specified.** Resolve F1 first — the choice between (a) and
  (b) changes the state model, so it should be settled before the state-machine work
  rather than during it.
- **a2b: resolve F2 in the specification before implementation.**
