# M09-021 — Objective policy features

## Status

**In progress.** Implementation on branch `wp/m09-021-objective-policy-features` from integration
point `432f20a`.

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

5. **Crossed emission** — objective facts are choice-invariant, so per the accepted `StateCross`
   architecture they reach a head only crossed: `state-kind:<kind>:<fact>` under `ByKind`,
   `state-option:<option_id>:<fact>` under `ByOption`, inert under `None` (exactly as seat facts
   already behave). MLP §5.1's bare names are preserved verbatim as the fact-name portion of the
   crossed name; this is recorded as an architectural reconciliation, not a deviation — uncrossed
   emission would be mathematically inert in every linear head and the package would deliver
   nothing.

6. **Legacy subvector unchanged** — all new names contain `:objective-`; no existing feature name
   or value is touched. A pinning test records the full non-objective vector of a fixed fixture
   choice before the change and asserts it byte-for-byte after.

### Information boundary

Held-secret facts use only the acting seat's own cards (accessor enforces this). Opponent secret
aliases never enter any feature name; opponent redaction counts are M09-023's scope. A focused
test asserts that for two seats in one position, no seat's features contain an alias held by the
other.

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
