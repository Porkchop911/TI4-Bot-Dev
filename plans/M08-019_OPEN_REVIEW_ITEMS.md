# M08-019 — review ledger

# Part 1 — independent Tier-C adjudication (Claude Opus 5)

## Status

**Do not close the gate yet, and do not close it on Option B.** F-M08-019-1 is a real porting
deviation, well-measured and honestly framed — but it is **misattributed in one half and
under-scoped**. Mechanism (b) names a site that provably cannot be what the experiment measured,
and the site that *can* be is a bigger one the campaign did not look at.

The recommendation is still Option A. The scope attached to it is wrong.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of this. Reviewed M08-017, M08-018, M08-020, M08-021 — this gate covers a range I reviewed package-by-package, so I am independent of the implementer but not a fresh perspective on M08. Recorded per the M06-024 precedent. |
| Base | `476e0c4` (M08-021 closed) |
| Diff under `crates/` | none — characterization record only, as declared |
| Oracle | `D:/Projects/ti4-engine @ 37061c5`, read-only; every quotation below re-read at source |

## What verifies

I re-read every oracle quotation at the pin rather than taking it, and recomputed the corpus
numbers directly.

**The oracle quotations are exact.** `engine/technology.py:234-235` is
`tuple(sorted(a for a in catalogue() if can_research(game, player, a)))`.
`engine/faction_abilities/xxcha.py` iterates `game.galaxy.system(system_id).planets`. Both as
quoted.

**The corpus number is exact.** Recomputed independently from `systems.json` and `planets.json`:
**231 systems, 13** whose record planet order differs from the relative planets.json order —
`09`, `107`, `108`, `110`, `111`, `12`, and seven more. (The record's example reads `druua` for
`druaa`; the substance is right.)

**`researchable()` does iterate file order**, and the fix's sort key is the right one — the Rust
id *is* the alias (`record.text("alias")` then `TechnologyId::new`), so sorting by `TechnologyId`
reproduces the oracle's `sorted()` over aliases rather than merely producing *some* canonical
order. That is the part of Option A that could have been silently wrong, and it is not.

**`ContentStore::strategy_cards()` has no production consumers** — confirmed; the grep hits are
`Player::unused_strategy_cards()`, a validator doc line, and one test.

**A restraint worth crediting.** The oracle iterates `sorted(reachable)` for systems; Rust's
`reachable` is a `BTreeSet<String>`, which iterates sorted already. The campaign did not flag this
as a deviation, and was right not to.

**The methodology note is a genuine self-caught error** and the most reusable thing in the record:
comparing whole `GameResult` values catches `seconds: f64` and reports universal divergence. It
turned a false "all 28 categories diverge" into the correct two. Keep it where future campaigns
will find it.

## Findings

### Y1 — HIGH (blocking) · mechanism (b) cannot be what the bisect measured, and the real consumer is far more load-bearing

`annexable()` (`faction_abilities.rs:466`) takes **no `ContentStore` parameter**. At line 489 it
calls `ti4_content::galaxy::planets_in(ContentStore::embedded(), …)` — it reads the *compiled-in*
corpus, not the store the game is running on.

The perturbation loaded reversed corpora through `ContentStore::from_dir`. **`annexable()`'s output
is therefore byte-identical under every one of those perturbations**, by construction. It cannot be
the cause of the planets.json divergence, and the bisect could not have detected it either way.
This is a proof, not a competing hypothesis.

The live-store consumer that *does* explain the measurement is in the invasion path:

```rust
// crates/ti4-engine/src/invasion.rs:246
fn landable_planets(state, content: &ContentStore, sources, system) -> Vec<PlanetId> {
    ti4_content::galaxy::planets_in(content, system.as_str(), sources)   // live store, file order
        ...
}
```

feeding `commit_options` — *"One option per distinguishable landing — unit type, sustained damage
and planet"*. Planet order here is choice-option order for every ground commitment.

And the oracle deviates in exactly the way mechanism (b) describes, at this site:

```python
# engine/invasion.py:44-47
def planets_in(game, system_id):
    return game.galaxy.system(system_id).planets      # system-record order

# engine/invasion.py:260
planets = planets_in(game, system_id)                 # feeds `landable`, then the options
```

So the deviation the campaign found is real and it is *also* — and much more consequentially — in
`invasion.rs`. Xxcha Peace Accords is one ability of one faction; ground commitment happens in
essentially every game that invades anything. The same 13-of-231 systems are affected, and on those
systems Rust offers commit options in a different order than the oracle would have.

**Required before this gate closes.** Re-attribute mechanism (b): the measured planets.json
dependency is `invasion.rs::landable_planets`, not `annexable()`. Add invasion to the fix scope.
The Xxcha deviation stays in the record — it is a genuine oracle divergence — but it must be
labelled *unmeasured*, because the instrument used could not see it.

### Y2 — MEDIUM · `annexable()` resolving content through the wrong domain is a defect in its own right

Independently of Y1: a function that takes `state`, `galaxy` and `player`, then reaches for
`ContentStore::embedded()` and a hardcoded `POK` internally, ignores both the active store and the
active source scope. Today every production call site happens to run on the embedded store at POK,
so this is **latent, not live** — but it is precisely the defect class the M06 exit evidence named:
*an identifier resolved through the wrong domain*. It will stay green under any test that also
assumes embedded/POK, which is all of them.

**Recommended action.** Thread `content` and `sources` through `annexable()` as its siblings do.
Small, and it removes the trap rather than documenting it. If Y1's fix touches this function
anyway, do it in the same change.

### Y3 — MEDIUM · the bisect is a single-seed result, stated as a property of the engine

The bisect ran seed `813_001`, one six-player game. The conclusion — *"exactly two categories change
the game"* — is true of that game. The record's own phrasing ("the only ones measured") is careful,
but the summary line above it is not, and this gate's output is what M09 will cite.

A category whose records only matter in a game that reaches an agenda vote, an exploration draw, a
relic, or a faction ability that seed never triggered would show as "same" for reasons having
nothing to do with order-independence. Twenty-six clean categories from one game is weak evidence
for twenty-six clean categories.

**Recommended action.** Re-run the bisect over the M08-021 thirty-seed set. That suite was built for
exactly this and is now mechanically re-derivable (V2/R1). Twenty-eight categories × thirty seeds is
a long batch but a cheap one to run unattended, and it converts the claim into the thing it is
already being read as.

## Adjudication — Option A, with corrected scope

**Option B is not available.** The `researchable()` doc comment promises "a stable order" and does
not deliver one; a documented known difference would preserve, in the engine's most-cited exit gate,
a divergence from the oracle in the *invasion commit path*. That is not a corpus-layout footnote.

**Option A, extended:**

1. `researchable()` — sort by `TechnologyId`. Sort key verified to match the oracle's.
2. `invasion.rs::landable_planets` — take planets in **system-record order**, matching
   `engine/invasion.py:44`. *(new — Y1)*
3. `annexable()` — system-record order, and thread `content`/`sources` while there. *(Y2)*
4. Correct the loader module doc, whose deck rationale is stale, to name the real dependencies.

**Sequencing.** All four land together, then the M08-021 baseline is re-derived **once** through its
versioned process. This is the same argument the M07-020 R1 adjudication used to order M08-020 ahead
of M08-018: an ordering-affecting change made after downstream baselines are recorded invalidates
them silently, so it is cheaper to move every baseline exactly once. The re-baseline machinery now
exists and is mechanical, which is what makes Option A affordable.

**Operator disposition is still required** — this changes public observation ordering and
invalidates a recorded baseline — but the decision I am handing up is "A with invasion included",
not "A or B".

## Disposition

**Blocked on Y1.** Re-attribute mechanism (b), add invasion to the scope, and the gate can proceed.
Y2 rides with the same fix; Y3 is a re-run of an existing suite.

The campaign itself is good work — the perturbation protocol is sound, the control experiments are
the right ones, the semantic-equivalence check on the reversed corpora is more rigorous than it had
to be, and the `seconds` error was caught and reported by the campaign rather than by me. Y1 is not
a flaw in the method; it is the one thing the method could not see, because the site it should have
found reads its content from somewhere the experiment could not reach.

---

# Part 2 — independent Tier-C recheck of `9a8f5fd` (Codex frontier review, 2026-08-23)

## Verdict

**Changes required; do not close M08-019.** This review independently reproduced the code-path
analysis in Part 1. The technology-order fix is correct and its focused test passes. The submitted
planet-order resolution is incomplete and the evidence currently overstates what was fixed.

This verdict does not rely on Python parity. The accepted M08-019 Rust specification requires
identical choices/explanations/replay under perturbed content insertion order. The live invasion
path still derives choice-option order from `planets.json` record layout, so that accepted Rust
criterion remains unmet regardless of historical Python behavior.

### C1 — HIGH, blocking: live invasion choice ordering remains corpus-layout-dependent

`crates/ti4-engine/src/invasion.rs::landable_planets` still calls
`galaxy::planets_in(content, system, sources)`, which preserves the live store's `planets.json`
record order. Its result feeds `commit_options`, making that order observable in every affected
ground-commitment choice. Commit `9a8f5fd` does not touch `invasion.rs`; therefore it cannot resolve
the measured live-store perturbation or satisfy the package's insertion-order gate.

**Required:** derive landing planets from the active system record's `planets` array (preserving
the custodians filter), add a red-first invasion option-order test, and include the resulting
ordering change in the single versioned behavior re-baseline.

### C2 — MEDIUM, required: `annexable` still bypasses active content and sources

The new implementation still constructs `ContentStore::embedded()` inside `annexable` and its
signature still accepts neither `content` nor `sources`. The production caller already has both in
its context. Consequently the new test proves ordering only for embedded content and cannot cover
the package's perturbed live store; alternate source scopes can resolve a different domain.

**Required:** thread the active `ContentStore` and `SourceSet` through `annexable` and every call
site/test, then resolve system records through that domain.

### C3 — MEDIUM, required evidence correction: the 28-category conclusion is one-seed evidence

The category bisect is recorded from seed `813_001`, while the summary generalizes its result to
the engine. A category unused by that game is indistinguishable from an order-independent one.

**Required:** run the 28-category perturbation campaign across the existing 30-seed M08-021 set,
or explicitly narrow every conclusion to the single exercised seed. Because M08-019 is an exit
gate intended to support later milestones, the 30-seed rerun is the accepted resolution.

## Reproduced checks

- `researchable_offers_options_in_canonical_sorted_order`: **1/0**.
- `peace_accords_candidates_follow_the_system_record_planet_order`: **1/0**, but it exercises
  embedded content and therefore does not close C2.
- M08-021 v2 behavior gate: **1/0**.
- Engine Clippy: no new warning in the submitted touched files; reported warnings are the known
  pre-existing sites in `choice.rs`, `game.rs`, and `strategy.rs`.

## Disposition

**Blocked on C1. C2 and C3 are also required before re-review.** The existing v2 baseline must not
be treated as final because the missing invasion fix will change option ordering again. After all
three corrections land together, rederive the baseline once and request a fresh Tier-C recheck.

---

# Part 3 — implementer's campaign record, verbatim

Campaign-driven findings recorded by the implementing agent during the exit-review campaign.
Adjudication belongs to the independent Tier C reviewer; where a fix would change public
observation ordering or invalidate a recorded baseline, operator disposition is also required
per AGENTS.md's autonomous decision policy.

# F-M08-019-1 — choice option order follows content file layout in two places (porting deviation)

**Status: RESOLVED IN-PACKAGE (Option A) — pending independent Tier C recheck.** Operator
disposition 2026-08-23: "keep working on the actual project; reviews handled by the Claude
loop" — read as adopting the implementer's recommendation (Option A). Fix implemented under the
finding-specific scope declared below; M08-021 re-baselined through its versioned process.
Independent review of the fix + re-baseline is still required before M08-019 closes.

## What was measured

Perturbation protocol (temporary probe `crates/ti4-sim/examples/perturb_probe.rs`, deleted after
use; scratch corpora under gitignored `out/`, removed):

1. Reversed every one of the 28 category JSON arrays in a copy of the corpus, loaded via
   `ContentStore::from_dir`. Semantic equivalence verified independently: canonicalized
   (sorted-key) per-record JSON sets are identical between embedded and perturbed corpora —
   **zero content differences**; only record order differs.
2. Control experiments on seed 813_001, six-player POK `Seats::Scored`, comparing all
   `GameResult` fields **except `seconds`** (wall-clock field — see methodology note below):
   - embedded store played twice: **SAME** (no in-process nondeterminism);
   - embedded vs `from_dir` of an unmodified copy: **SAME** (loader faithful, pipeline sound);
   - embedded vs `from_dir` of the fully reversed corpus: **DIVERGED**.
3. Bisect — each category reversed individually (28 single-category corpora), same seed:
   exactly **two** categories change the game; the other 26 are bit-identical in every compared
   field:

```text
planets: DIVERGES        technologies: DIVERGES
all other 26 categories: same
```

## Mechanism (a) — `researchable()` iterates file order where the oracle sorted

`crates/ti4-engine/src/technology.rs:793`:

```rust
/// Everything this player could research now, in a stable order.
pub fn researchable(...) -> Vec<TechnologyId> {
    let active = active_aliases(content);
    content
        .records(ContentType::Technologies)   // file order of technologies.json
        .iter()
        ...
}
```

The doc comment promises "a stable order", but the order is the **file layout** of
`technologies.json`, not a canonical one. The oracle (pinned `37061c5`, read-only) sorted:

```python
def researchable(game, player):
    return tuple(sorted(a for a in catalogue() if can_research(game, player, a)))
```

Consumers build choice options directly from this list's order:
- `crates/ti4-engine/src/strategy_cards.rs:236` and `:278` — the "research a technology"
  choice (every research opportunity for every faction);
- `crates/ti4-engine/src/action_cards.rs:1096` — Focused Research options.

The authored bot samples over candidates positionally when scores tie, so option order changes
bot choices and cascades through the whole game. Measured: reversing only `technologies.json`
changes the seeded game (step 3 above).

## Mechanism (b) — `annexable()` iterates planets in file order where the oracle used system-record order

`crates/ti4-engine/src/faction_abilities.rs` (~line 486, Xxcha Peace Accords): for each
reachable system it iterates `ti4_content::galaxy::planets_in(...)` — which returns planets in
**planets.json file order** filtered by system membership. The oracle (pinned `37061c5`,
`engine/faction_abilities/xxcha.py:55`) iterated the **system record's own `planets` array**:

```python
for planet in game.galaxy.system(system_id).planets:   # systems.json "planets" order
```

Corpus measurement: for **13 of 231** systems the system-record planet order differs from the
relative planets.json file order (e.g. system `09`: record `[maaluuk, druua]` vs file order
`[druaa, maaluuk]`). So on those systems Rust offers Xxcha a different candidate order than the
oracle would have. Measured: reversing only `planets.json` changes the seeded game (step 3).

## What this is and is not

- **Not** an illegality issue: every legal option is still offered; no illegal choice becomes
  possible or impossible. Legality gates are unaffected.
- **Not** in-process nondeterminism: given a corpus, behavior is fully deterministic (control
  experiment). The dependency is on *corpus layout*, which is version-controlled data — but it
  means reordering records inside `technologies.json` or `planets.json` silently changes every
  seeded game that researches technology or where Xxcha annexes.
- **Is** a porting deviation from the oracle's canonical orderings in both places, and the
  "stable order" doc comment overstates what is actually guaranteed.
- Note: `ContentStore::strategy_cards()` (file-order tie-break on initiative) has **no
  production consumers** — verified by grep; initiative ties exist in the corpus (2→2 cards,
  4→3, 6→2) but never reach gameplay through that accessor. Deck construction sorts IDs before
  shuffling (`deck.rs` `ids()`), so decks are file-order-independent despite the loader module
  doc's stale claim that "reordering a category changes every seeded game" via deck building —
  the two real dependencies above are the only ones measured.

## Methodology note (recorded for future campaigns)

`GameResult` contains `seconds: f64` (wall time). Comparing whole `GameResult` values with
`==` therefore always reports divergence between runs. Any equivalence check must compare all
fields except `seconds`. The first perturbation run in this campaign compared whole structs and
falsely reported "all 28 categories diverge"; the corrected comparison found exactly two.

## Options for resolution (adjudication requested)

- **Option A — fix both now.** Sort `researchable()` output by `TechnologyId` (restores oracle
  semantics; makes the doc comment true); make `annexable()` iterate planets in system-record
  order (restore oracle semantics). Both are small, localized changes. Consequence: choice
  option ordering changes → authored-bot choices can change → the M08-021 recorded baseline
  (2026-08-23) must be re-baselined through its versioned process with review approval before
  M08-019 closes. Requires finding-specific P1 writable-path declarations for
  `crates/ti4-engine/src/technology.rs` and `crates/ti4-engine/src/faction_abilities.rs`.
- **Option B — accept as documented corpus-layout dependency.** Record in
  `plans/KNOWN_DIFFERENCES.md`; correct the loader module doc (its deck rationale is stale) to
  name the two real dependencies; defer any fix. No baseline invalidation; M08-019 can close on
  schedule.

**Implementer's recommendation: Option A.** Both orderings deviate from the oracle's canonical
form, the "stable order" promise already exists in a doc comment, and leaving them means every
future corpus regeneration risks silently changing all seeded games. The re-baselining cost is
bounded — the M08-021 suite exists precisely to absorb such changes under review. But this
changes public observation ordering and invalidates a recorded baseline, so it is presented for
independent reviewer + operator disposition rather than decided unilaterally.

## Finding-specific writable paths (declared before source edits, per spec)

```text
crates/ti4-engine/src/technology.rs        (researchable() sort + its test module)
crates/ti4-engine/src/faction_abilities.rs (annexable() system-record order + its test module)
crates/ti4-sim/src/behavior.rs             (baseline_bounds v2 — versioned re-baseline only)
plans/M08-019_OPEN_REVIEW_ITEMS.md, plans/evidence/M08-019.md,
plans/evidence/M08-021.md (old/new bounds side by side), plans/EXECUTION_STATE.md
```

No other path. The fix changes option *order* only — the candidate sets are provably unchanged:
system-record planet lists and `tileId` membership agree on all 231 systems (measured); sorting
reorders, never filters.

## Resolution record (Option A)

- **Fix (a):** `researchable()` now returns its result sorted by `TechnologyId` (derived `Ord`,
  lexicographic) — restoring the oracle's `sorted(...)` and making the "stable order" doc comment
  true in the canonical sense.
- **Fix (b):** `annexable()` iterates each reachable system's planets from the **system record's
  own `planets` array** (`System::planets()`, corpus order) instead of `galaxy::planets_in`
  (planets.json file order filtered by tileId) — restoring the oracle's iteration.
- **Red-first tests:** `researchable_offers_options_in_canonical_sorted_order` (technology.rs)
  and `peace_accords_candidates_follow_the_system_record_planet_order` (faction_abilities.rs,
  system 110: record `[horizon, elnath, luthieniv]` vs file order `[elnath, horizon,
  luthieniv]`; with luthieniv controlled the two empty candidates swap relative order between
  the old and new code). Both verified RED before the fix, GREEN after.
- **Re-baseline (M08-021 v2):** recorded in `plans/evidence/M08-021.md` with old/new bounds side
  by side; semantic cause: canonical choice-option ordering changes bot sampling on tied scores.

---

# Part 4 — correction round for C1/C2/C3 (implementer, 2026-08-23)

## Finding-specific writable paths (declared before source edits, per spec)

```text
crates/ti4-engine/src/invasion.rs          (C1: landable_planets from the active system record's
                                             `planets` array + red-first option-order test in its
                                             test module)
crates/ti4-engine/src/faction_abilities.rs (C2: annexable() threads content/sources through;
                                             production caller + test call sites updated)
crates/ti4-sim/src/behavior.rs             (baseline_bounds v3 — the single versioned
                                             rederivation required by the verdict's disposition:
                                             "rederive the baseline once" after C1+C2 land)
plans/M08-019_OPEN_REVIEW_ITEMS.md, plans/evidence/M08-019.md,
plans/evidence/M08-021.md (v3 re-baseline, old/new side by side),
plans/EXECUTION_STATE.md
```

No other path. C3 is evidence-only: the 28-category perturbation rerun over the full 30-seed
M08-021 set (`812_001..=812_030`), recorded in `plans/evidence/M08-019.md`.

## Scope notes carried into implementation

- **C1 membership invariant:** system-record `planets` arrays and `tileId` membership agree on all
  231 systems (measured); no system mixes planet sources, so the per-planet scope filter keeps the
  result set identical for every active source scope — only order changes. The custodians filter
  (`mr` while the token sits) is preserved verbatim.
- **C2 domain:** `strategy_resolved`'s `TimingContext` already carries both `content` and
  `sources`; tests pass `(ContentStore::embedded(), POK)` matching their fixtures. The scope check
  on system records is a no-op for reachable systems (they exist in the active galaxy by
  construction) but makes the function honest about its domain under perturbed live stores.
- **Re-baseline:** one v3 rederivation after C1+C2 land together, per the verdict's disposition;
  the v2 baseline is interim and is not treated as final.

## Resolution record (correction round)

- **C1 — resolved.** `landable_planets` derives landing planets from the active system record's
  own `planets` array (per-planet scope filter mirroring `planets_in`; custodians filter
  preserved). Red-first test `commit_options_follow_the_system_record_planet_order` verified RED
  against the old implementation, GREEN after. `two_planet_arena()` re-pointed to canonical order;
  single-planet `arena()` untouched (asserts no order).
- **C2 — resolved.** `annexable` threads `(content, sources)` through and resolves system records
  with an explicit scope check; the embedded-store bypass is gone. Production caller passes the
  TimingContext's domain; tests pass `(embedded, POK)`. The ordering test moved from out-of-scope
  system 110 to in-scope system 58 (same red-first property inside the active domain).
- **C3 — resolved.** Full 30-seed perturbation rerun: **0/30 seeds diverge** across all comparable
  categories (42–43 event labels + seven scalar fields; `seconds` excluded by design). Protocol and
  result in `plans/evidence/M08-019.md`.
- **Re-baseline — done once, as directed.** M08-021 v3 recorded with old/new side by side in
  `plans/evidence/M08-021.md`; gate integrity check bit-verifies the transcription.
- **Gates:** engine 850/0 + 5/0 · policy 119/0 · sim 32/0 (v3 gate) · workspace 1,336/0 twice,
  timing-free identical · clippy/rustfmt clean on touched files.

**Status: corrections complete; requesting a fresh independent Tier-C recheck.** The v3 baseline is
interim until that review accepts it.
