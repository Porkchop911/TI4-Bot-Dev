# M09-021 — Objective policy features

## Status

**ACCEPTED (operator attestation of the independent Tier-C recheck of `4bf8e9f`, 2026-08-25; no
written verdict in repo — M09-019b precedent). F-M09-021-1 resolved and closed after four rounds +
AA1.** The round-4 recheck had resolved F-M09-021-1 but required **AA1** before close: `promissory_notes()`
returned the whole note-position map under a doc comment claiming everything was faceup — 25 of 34
corpus records are in-hand and private. Applied as specified: the accessor now returns only the
faceup subset (filtered by `state.promissory_faceup`), and `military_support.rs` moved onto the
explicit-records model (main drives each game step by step and reads the note position from full
state at visible cost; the policy side is plain learned bots). The round-3 recheck had accepted the
authority-gated seam but found two HIGH holes: Z1 — `SeatObservation::held_state()` returned an
owned `GameState`, so the bound view itself was a state handle and the "full-state possession" gate
was not a gate (a decider could mint any seat's capability one method call away);
Z2 — even that copy's redaction is defeated by set complement (`secret_deck` unredacted, deck +
dealt == 40), naming every opponent's secret. Round 4 removes the state source from the
capability entirely (reviewer option 1): no method on `SeatObservation` or `Observed` produces a
`GameState`; redaction moves to an authority-gated free function; the examples read face-up facts
through new `Observed` accessors plus a bound-seat `held_secrets()`; and the regression is
rewritten as an active attack attempt with a non-vacuous complement demonstration. The round-2
recheck had accepted the private type, live binding, and redaction closure but found that public
`ask_private(choice, seen, decider)` still minted capabilities from caller-controlled data (the
caller-supplied decider *is* caller code); round 3 gates `ask_private` by full-state possession —
it takes raw `&GameState` and constructs the observation internally. Initial implementation on
branch `wp/m09-021-objective-policy-features` from integration point `432f20a`; the independent
Tier-C review of `51ca544` returned *changes required* (F-M09-021-1/2/3, all HIGH). F2 and F3 were
corrected in round 1 (bare §5.1 namespace on every option under every crossing mode; dimensionally
invalid performance claim removed) and accepted as resolved by the recheck of `870a8f5`. F-M09-021-
1 remained HIGH: the choice-bound accessor was still forgeable, because `Choice` is public. Round 2
replaces caller convention with a type-level binding — `SeatObservation` (no public constructor,
built only inside engine ask paths), argumentless bound-seat secret accessors, removal of every
public `Observed` method that returns named private data for a caller-chosen seat (including
`redacted_for`, which had the identical hole and was closed in the same round), an explicit-
records API for offline contexts, and negative boundary tests. See the "Information boundary"
section below and `plans/M09-021_OPEN_REVIEW_ITEMS.md` (round 2 + addendum).

## Normative sources

- `docs/MLP_PLAN.md` revision 5, §5.1 "Objectives — requirement and progress" (feature list, D17
  normalisation, aggregation rule, bespoke-as-families table, bought-objective affordability via
  the exact payment planner).
- Milestone row M09-021 in `plans/M09_LEARNED_POLICY.md`: "Requirement/progress/met/stage families
  use scoring sources of truth; legacy factual policy subvector unchanged."
- Accepted Rust architecture: `StateCross` crossing in `crates/ti4-policy/src/features.rs` (a
  linear softmax cannot see option-invariant features; choice-level facts reach a head only
  crossed by kind or option id).

## Dependencies (all closed)

M06-023 (secret position progress complete), M08-019 (authored-bot frontier review accepted),
M09-018 (schema/math frontier review accepted).

## Permission class and paths

Class: P2 (crate-local production + tests, no network, no external state).

Writable:

- `crates/ti4-engine/src/objectives.rs` — family/cost-family token functions, `CardProgress`
  record type, their tests.
- `crates/ti4-engine/src/choice.rs` — two `Observed` accessors (`revealed_objective_progress`,
  `held_secret_progress`) and their tests.
- `crates/ti4-policy/src/features.rs` — `ChoiceContext` extension, objective-fact construction,
  crossed emission, pinning + behavior + redaction tests (test module plus the named production
  changes).
- `crates/ti4-policy/tests/objective_baseline.json` — new test-data fixture recording the full
  pre-change feature vectors of the pinning fixture (same convention as
  `tests/golden_features.json`).
- `crates/ti4-policy/examples/m09_021_baseline_dump.rs` — new example that regenerates the
  baseline fixture; versioned regeneration process for the pinning test.
- `plans/M09-021_OBJECTIVE_POLICY_FEATURES.md`, `plans/evidence/M09-021.md`,
  `plans/M09-021_OPEN_REVIEW_ITEMS.md`, `plans/EXECUTION_STATE.md`.

Read-only: everything else, including the legacy hashed extractor (`option_features`), scoring,
inference, and all engine legality code. The progress APIs in `objectives.rs`/`secrets.rs` are
consumed, not modified.

## Design

### Engine side (single source of truth consumed, never duplicated)

1. **Canonical family tokens** (`objectives.rs`): one function mapping every `CountFamily`
   variant to a stable lowercase token (e.g. `NonHome → "non_home"`,
   `Colours { per_colour: 2 } → "colours_2"`, `PlanetsOfTrait { trait_name } →
   "planets_of_trait_<trait>"`, `Units { base_type } → "units_<base>"`), and one for
   `CostFamily` (`Spend(kind) → "cost_spend_<kind>"`, `TradeGoods → "cost_trade_goods"`,
   `Tokens → "cost_tokens"`, `AllThree → "cost_all_three"`). Underscores within the token,
   matching the existing feature-name convention; MLP §5.1's `cost-<kind>` hyphen is
   pseudocode and is reconciled to the accepted name space here (recorded decision).

2. **`CardProgress` record** (`objectives.rs`, next to `RequirementProgress`): one revealed or
   held card's exact progress — alias, family token, raw `have`, `threshold` (always > 0),
   `satisfied`, and stage for publics (`Option<u8>`, from the existing `stage_of`; `None` for
   secrets). Unifies `RequirementProgress` (counting + bespoke) and `CostProgress` (bought) so
   the policy side sees one shape.

3. **Two `Observed` accessors** (`choice.rs`), following the established rules-predicate pattern
   of `scoreable_public`/`scoreable_secret` (state stays private; only computed facts cross the
   boundary):
   - `revealed_objective_progress(player) -> Vec<CardProgress>` — for each alias in
     `state.revealed_objectives`, dispatch through the existing engine APIs:
     `objectives::counting_progress` → `objectives::remaining_position_progress` (the six
     formerly-bespoke publics) → `objectives::bought_progress` (affordability via the exact
     payment planner, which already rejects target ≤ 0). Disjoint match arms make the chain safe.
   - `held_secret_progress(player) -> Vec<CardProgress>` — for each alias in that seat's own
     `secret_objectives`, through `secrets::counting_progress` → `secrets::remaining_position_
     progress`. Answered **only for the seat asking** (same boundary as `scoreable_secret`).
     Occurrence-based secrets return `None` from both and are skipped — they have no position
     progress representation, and missing context is not emitted as factual zero.

   Both build one `Position` per call with galaxy attached when the game has a map, so
   galaxy-dependent families (e.g. `on_the_rim`) resolve exactly as scoring does; without a
   galaxy they return `None` and are skipped rather than zero-filled.

### Policy side (`features.rs`, explicit path only)

4. **Fact construction** — one helper per choice, computed once in `explicit_choice_features`:
   - Input: the two accessor lists (acting seat).
   - Per card: `objective-met:<alias>` = 1.0 when satisfied (absent otherwise; the vector's
     zero-skip convention already encodes "not met").
   - Group by family token with a `BTreeMap` (canonical order, deterministic):
     - `objective-progress:<family>` = **max** over that family's cards of clipped
       `min(1, have / threshold)` — D17 normalisation; the gap feature is deliberately not
       emitted (`1 − progress` is linear in what is already there).
     - `objective-progress:<family>:<threshold>` per distinct threshold (max where two cards
       share family and threshold, e.g. the two `colours_2` publics).
     - `objective-need:<family>:<threshold>` = 1.0 marker per distinct (family, threshold) —
       the threshold as its own feature, because a ratio alone cannot distinguish "3 of 4" from
       "9 of 12".
     - `objective-count:<family>` = how many cards are revealed/held in that family.
   - Publics only: `objective-stage:1` / `objective-stage:2` = count of revealed publics at each
     stage (secrets have no printed stage).
   - **Max is applied before the vector is constructed**; no duplicate names rely on additive
     merge (§5.1 aggregation rule).

5. **Dual-namespace emission (corrected per F-M09-021-2)** — objective facts are emitted in two
   disjoint namespaces:
   - **Bare** — MLP §5.1's names verbatim (`objective-progress:<family>`, `objective-met:<alias>`,
     …), on **every option under every crossing mode**, including `StateCross::None`. This is the
     nonlinear MLP input contract (§4.1/§5.1): a per-option trunk can let an option-invariant state
     fact interact with option facts, so they must be present even where no linear cross exists.
   - **Crossed** — `state-kind:<kind>:<fact>` under `ByKind`, `state-option:<option_id>:<fact>`
     under `ByOption` (absent under `None`), the linear-schema delivery path: a linear softmax
     cannot see an option-invariant name, so crossed copies are how these facts reach existing and
     future linear heads.
   The namespaces are disjoint by construction (bare names start with `objective-`; crossed ones
   carry the `state-kind:`/`state-option:` prefix). A focused test proves bare need/progress/met/
   stage facts survive a `StateCross::None` choice and stay option-order deterministic. The five
   bare families are added to the closed explicit inventory (a reviewed extension; every legacy
   name is unchanged).

6. **Legacy subvector unchanged** — no existing feature name or value is touched: new names either
   start with `objective-` (bare) or contain `:objective-` (crossed), and a pinning test records
   the full non-objective vector of a fixed fixture choice before the change and asserts it
   byte-for-byte after.

### Information boundary (corrected per F-M09-021-1, round 2)

Held-secret facts use only the acting seat's own cards, enforced **by type surface**:

- `SeatObservation<'a>` (`ti4_engine::choice`) is a private observation bound to exactly one seat.
  It has **no public constructor**: values exist only where engine ask paths bind them (the decider
  was just looked up by `choice.player`). Its secret accessors — `held_secret_progress()` and
  `held_secrets()` — take no arguments, so even holding a valid capability there is no call that
  names another seat. It derefs to `Observed` for public facts.
- **No method on either type produces a `GameState` or any deck data** (round 4 removed the former
  `held_state()` — a capability that can hand out a copy of the state is a state handle, and it made
  every other seat mintable through `ask_private`). The only private-data surface is the two
  no-argument bound-seat accessors; everything else reachable is face-up table data on `Observed`
  (board, counts, revealed objectives, **faceup** note positions — in-hand notes are private and do
  not appear, AA1).
- No method on the public `Observed` returns named private data with any caller-controlled identity
  argument (the former `held_secret_progress(player)`, `held_secret_progress_for_choice(choice)`,
  and `redacted_for(viewer)` are all gone; `seat(player)` returns counts only).
- Live play: `Decider::choose_seeing` takes `&SeatObservation<'_>`; the learned path passes
  `seen.held_secret_progress()` explicitly into feature extraction.
- Offline/training/diagnostic contexts (which hold full state by design) compute records via the
documented free function `ti4_engine::choice::held_secret_progress(state, content, sources,
galaxy, viewer)` and pass them as an explicit `held_secrets` parameter — every call site names
the secret data its extraction uses.
- `ask_private(choice, state, content, sources, galaxy, decider)` is the public test/offline seam,
  **authority-gated by full-state possession** (round 3): it takes raw `&GameState`, not an
  `Observed`. A live policy-side caller holds neither a state handle nor any way to extract one,
  so no code holding only bound/public observations can mint a capability for another seat through
  this seam; an offline context that does hold complete state may bind any decider to any owner,
  because hidden information does not exist there (every seat's cards are readable fields of the
  state it possesses — the same model as `held_secret_progress(...)`).

Opponent secret aliases never enter any feature name; opponent redaction counts are M09-023's scope.
Focused tests: the engine boundary test binds two views from one public `Observed` and asserts each
answers for its own seat only (records **and** full-state form, with marker assertions); a second
test drives the offline seam through shared validation; the round-4 regression is an active attack
attempt — an attacker decider handed exactly what `choose_seeing` provides tries every reachable
read (bound records, raw held secrets, public facts naming the opponent) and records anything that
names the opponent's actual card, which the test asserts never happens; it also executes the
complement computation from the table side (catalogue − deck recovers exactly the dealt cards,
asserted non-vacuously) to prove the danger the gate prevents is real and unreachable through any
bound-asset call; the policy-level test asserts that for two seats in one position, no seat's
features contain an alias held by the other.

### Zero-threshold safety

`bought_progress` already returns `None` for target ≤ 0 (content validation at the API edge);
counting/bespoke thresholds are hardcoded ≥ 1. A test asserts every registered public and secret
alias that yields progress has threshold > 0, so normalisation never divides by zero or invents a
zero-cost meaning.

## Non-goals

- No opponent-secret redaction counts (M09-023).
- No ability decomposition features (M09-022).
- No changes to the legacy hashed extractor, scoring, inference, or any engine legality/scoring
  semantics — progress APIs are consumed read-only.
- No retraining and no learned-behavior claims; new names hash to zero-weight buckets in existing
  artifacts (hashing trick), so old profiles score unchanged until retrained.

## Acceptance criteria

1. Requirement/progress/met/stage families emitted from the engine's scoring sources of truth
   (`counting_progress`, `remaining_position_progress`, `bought_progress`, `stage_of`); no
   hand-written requirement table in policy code.
2. Legacy factual policy subvector byte-identical (pinning test).
3. §5.1 aggregation: max before vector construction, threshold-keyed slots, need markers, count
   slots, clipped ratios, no gap feature; family-keyed not alias-keyed.
4. Held secrets only for the asking seat; opponent aliases absent from all seats' features.
5. No zero-threshold division reachable (API guarantees + test over every registered alias).
6. Deterministic output: stable card order, `BTreeMap` grouping, identical vectors across runs.
7. Workspace suite green; clippy/fmt clean on touched crates; extraction-cost measurement
   recorded before/after in evidence.

## Review tier

**C — frontier model.** The package touches the hidden-information boundary (held-secret progress
accessor) and feature purity for learned policy, both of which AGENTS.md escalates to a frontier
reviewer.
