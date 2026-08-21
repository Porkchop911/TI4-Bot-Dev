# M06-021a — open review items

Independent tier-C review of the event-scoped secret-timing correction.

| Field | Value |
|---|---|
| Reviewed | `wp/m06-021a-event-scoped-secret-timing`, uncommitted working tree, parent `92edea4` |
| Diff reviewed | `state.rs` +84, `secrets.rs` +160, `objectives.rs` +170 |
| Reviewer | Claude Opus 5 — implementer of M06-021, **not** of M06-021a1 |
| Full review | [`evidence/M06-021a1_REVIEW.md`](evidence/M06-021a1_REVIEW.md) |
| Checks rerun | Reviewer: workspace green, engine 816 + 5 doctests. Post-fix: workspace green, engine 819 + 5 doctests; strict model Clippy green |

## Disposition

| Package | Verdict |
|---|---|
| **M06-021a1** | **Accepted.** |
| **M06-021a2a** | **F1 resolution verified by the reviewer** — see verification pass 2. |
| **M06-021a2b** | **Accepted. Independent Tier-C review complete; F7-F10 resolved and full gates rerun.** |

**Verification pass 2 (reviewer, 2026-08-21).** All six original findings independently
re-checked against the code, not against the claims. All six hold. Five new items raised,
one of which (F7) should be closed before the parent Tier-C review.

The parent Tier-C integration review is complete. Its F7-F10 fixes are recorded below and passed
focused, affected-crate, workspace, lint, and diff gates. The package standard requires independent
review, resolution of actionable findings, and rerun evidence; all three are satisfied.

## Implementer integration audit (not an independent review)

The post-implementation audit found and resolved four integration defects before handoff:

- space-cannon scoring now closes before combat opening/barrage can roll;
- the next agenda remains queued until the prior agenda's scoring occurrence closes;
- synchronous combat/invasion wrappers consume internal scoring pauses and still finish;
- internal event helpers require `FeatOccurrence` rather than accepting a silent `Option` no-op.

Focused regressions cover each boundary, deterministic last-pass replay, and atomic rejection of
an invented occurrence-scoring choice. This audit does not satisfy the independent Tier-C gate.

The M06-021 finding is **upheld**. Rule 61.7 permits any number of objectives during an
action turn or agenda phase and caps scoring at one per combat; `Game::advance_turn`
opened one window per turn with a one-score cap, which is wrong in both directions. The
original author had recorded this as an open question and judged it low-priority. That
was a misjudgement, not a deferral.

**Independence note.** This review is independent of a1 but not of the defect a1
corrects. F5 concerns the reviewer's own prior code.

---

## F1 — BLOCKING · a2a · design

**The one-per-combat cap is enforced by window membership, not by the occurrence.**

`GameState` records no `(player, occurrence)` scoring history, and
`ScoringWindow::scored` is per-window. Two windows opened for the same
`FeatOccurrence` therefore each grant a score, whatever `EventScoreLimit` they carry.

**Why it bites a2a.** The plan is: *"Apply the same mechanism to space-cannon and
completed space combat, with `AnyPerPlayer`, `AnyPerPlayer`, and `OnePerPlayer`
respectively"* — separate pauses for anti-fighter barrage and for the completed space
combat. But **AFB is inside the space combat**:

| step | where the engine runs it | inside `CombatWindow`? |
|---|---|---|
| space cannon offense | `game.rs:162`, in `AftermathWindow::new` | no |
| anti-fighter barrage | `combat.rs:985`, in `CombatWindow::roll_round`, round 1 | **yes** |
| space combat resolution | `CombatWindow` | yes |

So the planned split permits Fight with Precision during AFB *and* Unveil Flagship after
the same combat — two objectives during or after one combat.

**Evidence.** Demonstrated, not inferred. A probe was added, run, and removed (tree
restored to a byte-identical diffstat). Two `OnePerPlayer` windows on one occurrence:

```text
first  window -> offered, scored "btv"
second window -> offered "dtgs", scored it
state.scored_by(a).len() == 2      // one combat occurrence
```

Reproduction is in the appendix.

**Required action — choose one, and record the choice as a plan revision:**

- **(a) One occurrence and one window per space combat.** Allocated when `CombatWindow`
  opens, settled after resolution. The cap then follows from there being one window.
  Cost: Fight with Precision's printed "during the anti-fighter barrage step" becomes an
  approximation.
- **(b) Move the cap onto the occurrence.** Record `(player, FeatOccurrence)` scores in
  `GameState`; `scoreable_event` excludes a player who has already scored in that
  occurrence. The cap then holds however many windows a combat opens.

**Recommended: (b).** It preserves printed timing and makes the cap structural rather
than a consequence of how many windows happen to be opened — the property that failed
the probe.

**Definition of done.** A test that opens two windows on one combat occurrence and
asserts the second offers nothing. Whichever option is chosen, that test must pass.

**Resolution (2026-08-21).** Chose option (b). `GameState` now records scored
`(PlayerId, FeatOccurrence)` pairs, and occurrence-scoped `OnePerPlayer` windows
exclude an already-scored player. The focused regression opens two windows for one
combat occurrence and verifies the second offers nothing.

**Caveat.** Rule 61.7 could not be verified from this repository —
`crates/ti4-content/content/rules.json` holds eight Fracture entries, not the LRR. This
finding rests on the rule as quoted in the package specification plus the engine's own
step structure above.

---

## F2 — HIGH · a2b · specification

**Ground combat is per planet.** An invasion of three defended planets contains three
combats, each carrying its own one-objective cap. Bombardment is not a combat at all —
it is invasion step 1, before any ground combat.

a2b covers "bombardment, control-loss, pass, and agenda emitters" without stating how
many occurrences an invasion allocates. One occurrence per invasion would apply the cap
far too broadly and would misclassify the bombardment window as a combat window.

**Required action.** Pin the granularity in the specification before implementing:
one occurrence per ground combat, plus a separate non-combat occurrence for bombardment.

**Resolution (2026-08-21).** M06-021a2a/a2b now pin one occurrence for each defended
planet's ground combat and a separate unlimited non-combat occurrence per bombardment
step.

---

## F3 — MEDIUM · a1 · consistency

`feat_occurrence_seq` (`state.rs:761`) is excluded from `GameState`'s `PartialEq`, while
all four sibling counters — `combat_round_seq`, `production_seq`, `activation_seq`,
`turn_seq` — are compared (`state.rs:878`).

The field comment justifies this as "transient timing state, like the feat ledgers". But
the ledgers are per-`Player` evidence, whereas this is a sequence counter that now
determines which scoring windows open, and replay determinism is an accepted contract.

**Required action.** Either compare it alongside its four siblings, or record in the
field comment why it differs from the counters it sits beside.

**Resolution (2026-08-21).** `feat_occurrence_seq` is now compared by `GameState`.

---

## F4 — LOW · a1 · latent

In `ScoringWindow::resolve`, `pending` is truncated before the award
(`objectives.rs:1375`), and the award falls through to the public path when
`secrets::award` returns `None` (`objectives.rs:1390`). Under `AnyPerPlayer` the scorer
stays pending, so a failed secret award calls the public `award` with a secret alias —
which errors — while the player remains eligible.

**Unreachable today.** Only `dhw` and `fsn` have costs (`secrets.rs::pay_for`) and both
are status-timed, so neither can reach an event window. It becomes live the moment a
costed action- or agenda-timed secret exists.

**Required action.** If the alias is a known secret, never fall through to the public
award: return the scoring error and drop the player from `pending`.

**Resolution (2026-08-21).** Known event-secret aliases now return
`ScoringError::SecretAwardFailed` on a failed secret award, never enter the public award path,
and are removed from the pending sequence. No currently registered costed action/agenda secret can
exercise this latent branch; the existing scoring suite verifies all currently reachable awards.

---

## F5 — INFORMATIONAL · parent

a1 changes no game-loop path, so the shipped engine still opens `for_event(...)` with
`LegacyTurn` and `OnePerPlayer` from `Game::advance_turn`. **The 61.7 defect remains
live until a2a and a2b land.**

Already recorded accurately in `evidence/M06-021a1.md` and `EXECUTION_STATE.md`
("no actual game timing is claimed fixed"). Restated here only so the parent's
completion boundary is unambiguous.

---

## F6 — LOW · a2b · cleanup

The turn-scoped ledger and the occurrence-scoped one now coexist, and every production
emitter still writes only the former:

```text
feats, record_feat, did_at_turn, did_this_turn, anyone_did_at_turn   (state.rs)
EventScope::LegacyTurn, for_event                                    (secrets.rs, objectives.rs)
```

**Required action.** Add removal of the legacy path to a2b's definition of done, once
the last emitter migrates. Otherwise the engine keeps two mechanisms for one job and the
unused one drifts.

**Resolution (2026-08-21).** `EventScope`, `LegacyTurn`, `ScoringWindow::for_event`, the
per-turn feat ledger and helpers, `Game::open_event_scoring`, and the `advance_turn` event
opening have been removed. Production event scoring now accepts only a concrete
`FeatOccurrence`; repository search finds the retired names only in historical review text.

---

## Accepted — not findings

Recorded so they are not re-litigated.

- **Occurrence model.** Monotonic, checked allocation; idempotent recording; exact
  matching on `(player, feat, occurrence)`. Tests cover owner attribution and
  cross-occurrence isolation, which are the two leak paths.
- ~~**Migration seam.**~~ Superseded: F6 removed `EventScope` entirely once the emitters
  migrated, which is the correct end state.
- **Backward compatibility.** `#[serde(default)]` on both new fields; old saved states
  remain readable.
- **Sequencing arithmetic.** The `keep` calculation was traced against the reversed
  `pending` vector and the `rev().enumerate()` offset in `next_askable`. Correct in both
  branches: `AnyPerPlayer` retains the scorer as next to be asked, skipped players are
  dropped in both. Terminates, because each accepted score both records the objective and
  removes the secret from hand, so the eligible set strictly shrinks.
- **Evidence discipline.** `evidence/M06-021a2a.md` states "implementation not started"
  and claims no test or review evidence for unimplemented behaviour. Accurate.

---

## Appendix — F1 reproduction

Add to `objectives.rs` tests, run, then remove.

```rust
#[test]
fn probe_two_windows_on_one_occurrence_each_allow_a_score() {
    let a = PlayerId::new("a");
    let mut state = game(std::slice::from_ref(&a));
    state.player_mut(&a).unwrap().secret_objectives = vec![
        ti4_model::id::SecretObjectiveId::new("btv"),
        ti4_model::id::SecretObjectiveId::new("dtgs"),
    ];
    let occurrence = state.begin_feat_occurrence();
    state.record_event_feat(&a, ti4_model::state::Feat::WonInAnAnomaly, occurrence);
    state.record_event_feat(&a, ti4_model::state::Feat::DestroyedACapitalShip, occurrence);

    let mut first = ScoringWindow::for_occurrence(
        std::slice::from_ref(&a),
        crate::secrets::Timing::Action,
        occurrence,
        EventScoreLimit::OnePerPlayer,
    );
    let choice = first
        .pending_choice(&state, ContentStore::embedded(), POK)
        .expect("first window offers");
    first
        .resolve(&mut state, ContentStore::embedded(), POK,
                 choice.option("btv").unwrap().clone())
        .unwrap();

    // Second window, same combat occurrence: the AFB pause then the end-of-combat pause.
    let mut second = ScoringWindow::for_occurrence(
        std::slice::from_ref(&a),
        crate::secrets::Timing::Action,
        occurrence,
        EventScoreLimit::OnePerPlayer,
    );
    let again = second.pending_choice(&state, ContentStore::embedded(), POK);
    assert!(again.is_some(), "second window offered nothing - cap holds");
    second
        .resolve(&mut state, ContentStore::embedded(), POK,
                 again.unwrap().option("dtgs").unwrap().clone())
        .unwrap();
    assert_eq!(state.scored_by(&a).len(), 2,
               "two objectives scored for one combat occurrence");
}
```

Observed: **passes** — both scores land. After F1 is fixed, this probe must fail at the
`again.is_some()` assertion.


---

# Verification pass 2 — reviewer, 2026-08-21

Re-checked against the working tree, not against the resolution notes.

| Finding | Claim | Verified how | Result |
|---|---|---|---|
| F1 | occurrence-scoped cap | the appendix probe re-run | **holds** — now fails at `again.is_some()` with "second window offered nothing"; `scored_at_occurrence` (`state.rs:1250`) consulted at `objectives.rs:1331`, recorded at `:1407` |
| F1 | AFB shares the combat occurrence | `combat.rs:1046` | **holds** — `roll_round` calls `ensure_combat_occurrence` and passes it to `anti_fighter_barrage_at`, so barrage and resolution are one occurrence |
| F2 | one occurrence per ground combat | `invasion.rs:790` | **holds** — allocated inside the per-planet loop, only for a contested planet |
| F2 | bombardment is non-combat | `invasion.rs:122` | **holds** — its own occurrence, separate from any ground combat |
| F3 | counter compared | `state.rs:867` | **holds** |
| F4 | no public-award fallthrough | `objectives.rs:1393-1399` | **holds** — `SecretAwardFailed`, player dropped from `pending` |
| F6 | legacy path removed | repo-wide grep | **holds** — `EventScope`, `LegacyTurn`, `open_event_scoring`, `record_feat`, `did_at_turn`, `did_this_turn`, `anyone_did_at_turn` all absent. The eight surviving `for_event` hits are `TimingRegistry::for_event` in `timing.rs`, unrelated |

**Checks rerun.** `cargo test --workspace` green; `ti4-engine` 816 + 5 doctests (from 806).
Clippy: no new warning classes.

**End-to-end, 150 holdout games, r6 champions.** No game errors. Ground-combat feats and
multi-score windows produce real gains over the pre-a2a engine:

| secret | before | after |
|---|---:|---:|
| Betray a Friend | 7% | **19%** |
| Spark a Rebellion | 5% | **13%** |
| Dictate Policy | 16% | **27%** |
| Turn Their Fleets to Dust | 6% | **9%** |
| Darken the Skies | 0% | **2%** |

Mean VP 2.918 → 2.933. Recording the four combat-generic feats for ground combat is a
correctness improvement over M06-021, which only ever recorded them for space combat —
Brave the Void, Darken the Skies, Spark a Rebellion and Betray a Friend all say "win a
combat", not "win a space combat". The space-only cards (`uf`, `dyp`) are correctly
absent from the ground path.

---

## F7 — MEDIUM · a2b · unmet criterion from its own specification

**`Become a Martyr` records an event that nothing consumes, so the loose position reading
still governs.**

`invasion.rs:841` records `Feat::LostAHomePlanet` and opens an occurrence for it. But
`feat_for` (`secrets.rs`) has no `bam` entry, so `scoreable_event` never consults that
feat. `bam` qualifies solely through `requirement_for` → `lost_a_home_planet`, a position
predicate: *a home planet currently in another player's hands*.

Consequences:

- `Feat::LostAHomePlanet` is dead in production. Its only readers are the two assertions
  in `invasion.rs:1535-1536`.
- Because the position stays true once a home planet is lost, `bam` remains offerable at
  **every later action occurrence for the rest of the game**. That is precisely the
  failure mode the parent package exists to eliminate — its own stated invariant is
  *"a triggering fact is tied to its concrete occurrence; stale facts never create a
  later offer."*
- M06-021a1's specification says: *"M06-021a2b will make Become a Martyr trigger only on
  losing control of a home-system planet; this child does not wire that event."*
  `evidence/M06-021a2b.md` records status *"implementation and verification complete"*,
  but this criterion is unmet.

**Required action.** Map `feat_for("bam") => Feat::LostAHomePlanet` and remove the
`lost_a_home_planet` position predicate, so the card triggers on the loss rather than on
the standing consequence.

**Definition of done.** A test asserting `bam` is offered in the occurrence that took the
home planet and **not** in a later unrelated action occurrence, with the position
unchanged throughout.

**Resolution (2026-08-21).** `bam` now maps to `Feat::LostAHomePlanet`; the stale position
predicate was removed. `become_a_martyr_is_offered_only_for_the_home_loss_occurrence` keeps the
lost-home board position unchanged, proves the loss occurrence offers the card, and proves a later
occurrence does not.

*Note:* this will not move the measured rate — `bam` scores 0/35 either way in 150
four-round games, because home planets are rarely taken that early. It is a correctness
item, not a scoring one.

---

## F8 — LOW · consistency

**The same card is implemented twice, with different answers.** `WonInARivalHome`:

```rust
// space  — combat.rs:1659   any rival's home, which is what the card says
.any(|seat| seat.id != winner && seat.home_system.as_ref() == Some(system))

// ground — invasion.rs      only the loser's own home
state.player(loser).and_then(|seat| seat.home_system.as_ref()) == Some(system)
```

Darken the Skies says "win a combat in another player's home system" — a fact about the
system, not about whose forces you beat. A ground combat won in B's home system against
C's forces (C having taken the planet earlier) records nothing under the ground path.

**Required action.** Use the space form in both, or extract one shared helper.

**Resolution (2026-08-21).** Space and ground combat now share
`combat::is_rival_home_system`. A three-player regression proves a winner records the feat in B's
home system even when the defeated force belongs to C.

---

## F9 — LOW · consistency

`WonAgainstANoteHolder` reads note holdings from a pre-combat snapshot in the space path
(`BeforeCombat::notes`, `combat.rs:1514`) and from live state in the ground path. The card
says "at the start of your tactical action", which is neither — but the two paths should
at least agree with each other. Cheap fix: snapshot once per tactical action and share it.

**Resolution (2026-08-21).** `TacticalWindow` snapshots note issuers when the action opens and
passes the same immutable snapshot through space combat and every ground combat. Standalone combat
and invasion APIs snapshot at their own entry boundary. A ground-combat regression removes the live
note after the snapshot and proves the start-of-action fact still governs.

---

## F10 — LOW · package size

`InvasionWindow::resolve` grew from 126 to 166 lines against a 100-line lint threshold
(pre-existing warning, worsened). The package standard asks that a package be reviewable
from a single diff; this function is now the largest obstacle to that in `invasion.rs`.

**Resolution (2026-08-21).** Commit completion and ground-round resolution were extracted into
focused helpers. Engine Clippy no longer reports `InvasionWindow::resolve`; only the documented
unrelated duplicate attribute, 103-line `Game::apply_tactical`, and strategy-test cast remain.

---

## F11 — INFORMATIONAL · evidence gap

**Fight with Precision scores 0 of 63 draws end-to-end**, unchanged from before a2a. The
anti-fighter-barrage pause is a2a's headline capability, it is unit-tested
(`barrage_scoring_pauses_combat_and_caps_the_whole_combat_occurrence`), and there is no
in-situ evidence that it ever fires in a real game.

Most likely genuine rarity — the barrage must destroy the *last* fighter, which needs a
unit with anti-fighter barrage facing a fighter-only remnant. But "we cannot distinguish
a rare path from a dead one" is exactly what an end-to-end counter settles cheaply.

**Suggested.** Instrument one 150-game run to count `Feat::BarrageTookTheLastFighters`
records. A non-zero count with zero scores would point at eligibility or window placement;
a zero count confirms rarity and closes the question.
