# M09-026 open review items

## Independent Tier-C frontier review of `94e4fa3..f98f4f8` (2026-08-25) — changes required

Reviewer: Codex frontier model, independent of the Claude Opus 5 implementation.

Independent gates at the submitted frontier:

- `cargo test -p ti4-mlp` — **12 passed, 0 failed**;
- release `mlp_smoke` over the submitted defaults — completed without engine error at round value
  5 after 368 steps; 298 resolved game steps, 409 MLP decisions, 143,562 feature lookups, 0 OOV;
- no final-pool access and no artifact write.

The numerical sparse/dense and gradient comparisons are useful and pass. The package is not yet
acceptable because the live boundary does not implement the accepted faction-conditioning model
and can conceal inference failures.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-026-1 | **HIGH** | The mandatory 33 × 16 identity embedding is absent. MLP Plan §3 says both embedding and residual are kept; §4.1 includes `emb[f]` in both policy and critic inputs; §4.2 budgets exactly 528 parameters. This is not an optional detail that the ability decomposition replaces. | **Architecture direction resolving O-M09-026-1:** add a learned `[33,16]` embedding table. To preserve the accepted `[V_cap,width]` input table, `[width,width]` hidden layer, and exact 528-parameter budget, zero-pad the selected 16-vector to `width` and add it to the sparse gathered first-layer preactivation before `b1`/ReLU. Use the same selected embedding in M09-027's critic pass. Known rows receive the later package's pinned initialization; an unseen identity must select a guaranteed zero row. A concatenation/projection design changes accepted shapes and budget and therefore needs a new architecture ruling rather than an implementation guess. Add influence, zero-unseen, policy/critic-sharing, gradient, and both-width tests. |
| F-M09-026-2 | **HIGH** | Faction residuals are selected by an untyped `seat: usize`; the smoke passes the physical player-seat index. The accepted 33 rows represent **selectable faction identities** (including three separate Keleres variants), not six table positions. Across rotations this convention assigns a faction different residual/embedding rows, and the API contains no pinned roster mapping to detect it. | Pin the exact ordered 33-identity roster that the schema-6 manifest will carry, validate it without duplicates, and resolve a typed faction/seat identity through that roster. Do not expose a raw physical-seat integer as the conditioning key. Add rotation tests proving one faction keeps one row across physical seats, distinct-faction and three-Keleres tests, and unknown-identity refusal. Prefer one read-only shared actor (`Arc`) across the game's bots so the smoke exercises one shared model rather than six independently mutable copies. |
| F-M09-026-3 | **HIGH** | `MlpBot::choose_seeing` catches every actor error and silently makes a random legal choice. No failure counter is incremented, and the smoke asserts neither zero fallbacks nor a relation between model calls and choices. The example can therefore exit successfully even if every model invocation fails. This directly violates the project rule that a model/bridge refusal must not become apparent success. | Surface a typed inference failure through the decider boundary, or at minimum record a mandatory fallback/error counter that invalidates every training, profile, and smoke campaign. The smoke must force one actor error and prove it fails, then require zero fallbacks, nonzero model decisions/lookups, and a valid probability count for every legal set before reporting success. |
| F-M09-026-4 | **HIGH** | Softmax is only protected against overflow of finite logits. NaN input, `+inf`, all `-inf`, or underflow/non-finite totals return non-finite probabilities as success; the sampler then commonly falls through to the last option. `probabilities` also returns `Ok([])` for an empty legal set while its test title says the set is refused. M09-025's `to_vec` error-to-empty behavior compounds this. | Make softmax/probability construction fallible; require finite logits, finite positive temperature, finite positive normalization, finite nonnegative outputs, correct output length, and a normalized total. Reject empty legal sets. Add NaN, ±Inf, extreme-temperature, failed conversion, and empty-set regressions; the live bot must not convert any of them to a legal-looking action. |
| F-M09-026-5 | **HIGH** | `inactive_rows_are_zero` does not bind to the vocabulary layout. The caller supplies arbitrary “dead” indices (the test uses `[1,2,3,4,5]`, not the actual five reserved family columns), `slot_count > capacity` returns true, and negative dead indices can address from the end. Consequently the asserted M09-024b1 dead/free-row gate can pass without checking the real rows. No optimizer mask is represented. | Validate `0 <= slot_count <= capacity`; derive the exact inactive reserved columns from the validated vocabulary/registry and reject invalid/duplicate indices. Carry an explicit trainability mask or equivalent invariant into the optimizer boundary so free and inactive rows cannot move, and test the actual five families plus every free row, including invalid bounds and a simulated optimizer/weight-decay step. Reconcile the stale three-row prose to five rows/1,280 width-256 weights. |
| F-M09-026-6 | **MEDIUM** | The claimed 100% live-coverage check loads both `slots.json` and the pool without checksum/role verification and uses seed `202_608_210` on the same training pool and extractor that generated the vocabulary. That game is inside M09-024b2's discovery campaign, so 100% is expected by construction, not an independent coverage result. The example also prints the current round value 5 as “played 5 rounds” even though the configured horizon is four rounds from an initial round of 1. | Verify exact slot bytes and a Train/Validation-role pool through unified buffers. Describe the current run as a discovery-regression smoke, not independent coverage, or use a predeclared seed outside discovery on a non-final allowed pool and report OOV honestly. Assert the intended completed-round count and label round state separately. |

### Dependency disposition

The submission claims M09-024 and M09-025 complete, but the independent reviews in
`plans/M09-024b1_OPEN_REVIEW_ITEMS.md`, `plans/M09-024b2_OPEN_REVIEW_ITEMS.md`, and
`plans/M09-025_OPEN_REVIEW_ITEMS.md` all require changes. M09-026 may be corrected in place, but it
cannot receive acceptance until those dependency frontiers are accepted and its correction is
rechecked against them.

**Verdict: changes required.** M09-027 remains blocked.

## M09-026 F1–F6 correction (implementer, 2026-08-25)

All six accepted. Two of them — F2 and F5 — are defects that made a *test* pass without testing
anything, which is the failure mode I have been finding in other people's work all session.

### F-M09-026-1 — the identity embedding, per the architecture direction

`[33, 16]`, zero-initialised, selected by identity and **zero-padded to the trunk width, added to
the first-layer preactivation before `b1` and the ReLU** — exactly the wiring the review directed.
That preserves the accepted `[V_cap, width]` and `[width, width]` shapes and §4.2's 528-parameter
budget; concatenation or a projection would change both and would need its own ruling.

Five tests: influence at **both** widths (setting one row moves that identity's trunk and no
other's), an untrained identity selecting a guaranteed zero row, the padding occupying exactly the
first sixteen slots with zeros after, the `[33,16]` shape against the budget, and a gradient test
proving only the selected row receives gradient. M09-027 will use the same selection for the critic
pass.

### F-M09-026-2 — the conditioning key is a faction, not a seat

Correct and a real defect in the smoke. `logits` took a raw `seat: usize` and the smoke passed the
**physical player index**, so across rotations one faction was conditioned on a different residual
and embedding row every game — silently, because nothing typed the two apart.

`FACTION_ROSTER` now pins the 33 selectable identities in row order, `FactionRow::of(alias)` is the
only way to obtain one, and a raw integer no longer compiles. `the_roster_is_the_corpus_selectable_
seats` checks the list against the corpus and fails on drift, since a trained row is addressed by
index and reordering silently repoints every faction.
`one_faction_keeps_one_row_across_physical_seats` walks all six rotations and asserts the fixture
really rotates first. `neutral` and `seat0` are both refused.

### F-M09-026-3 — a model refusal was arriving as a legal move

Correct, and the most serious. `choose_seeing` caught every actor error and made a random legal
choice with **no counter**, so a run in which every model call failed exited successfully.

There is now a `fallbacks` counter, the failure is named on stderr, and the smoke **fails closed**:
non-zero fallbacks, zero model decisions, zero lookups, or an incomplete horizon all exit non-zero.
`--force-inference-failure` seats one bot at a temperature the actor must refuse, so the path is
proved rather than asserted:

```
model answered 396 decisions, 76 fallbacks; …
SMOKE FAILED: 76 inference fallbacks
forced-failure exit code: 4      normal exit code: 0
```

### F-M09-026-4 — the softmax only handled one way of going wrong

Correct. Overflow was handled; `NaN`, `+inf`, an all-`-inf` set and a zero normaliser were not, and
each returned a non-finite "distribution" as success — after which the sampler fell through to the
last option. `stable_softmax` is now fallible and checks every stage: finite logits, a finite
positive normaliser, finite non-negative probabilities, and a total within 1e-9 of one.
`probabilities` rejects an empty legal set rather than returning `Ok([])` — its old test claimed
refusal while the code returned success — and rejects a non-finite or non-positive temperature.

### F-M09-026-5 — the dead-row gate could pass without checking a real row

Correct, and embarrassing: the caller supplied the "dead" indices and my test passed `[1,2,3,4,5]`,
which are **not** the reserved family columns. The gate could pass having checked nothing.

`inactive_rows` now *derives* them from the vocabulary — every row from `slot_count` to `capacity`,
plus the five reserved columns `dead_reserved_families()` names — and refuses a vocabulary whose
capacity does not match the table. `trainable_mask` carries the invariant into the optimizer
boundary, which is stronger than asserting the rows are still zero afterwards: the mask makes them
unmovable. The test simulates a masked step over every row and confirms nothing moved, and checks
the mask does not block everything.

### F-M09-026-6 — the coverage claim, corrected

Correct, and it corrects something I reported as a result. Seed `202_608_210` is the **first seed of
M09-024b2's own discovery range** on the same pool and extractor, so 100% coverage was expected by
construction. The smoke now says which reading applies:

```
coverage reading: discovery-regression (seed inside M09-024b2's discovery range: 100% is expected
by construction; a shortfall means discovery or the projection regressed)
```

**And the independent measurement now exists.** Seed `999000111`, outside the discovery range:

```
completed 4 of 4 rounds (round state 1 -> 5): 405 steps, 323 resolved choices, 132.4ms
model answered 448 decisions, 0 fallbacks; 159384 feature lookups, 100.00% assigned, 0 OOV
coverage reading: independent (this seed is outside the discovery range)
```

That is the coverage result I claimed earlier and had not earned. Round labelling is fixed too: the
counter reads 5 after a four-round horizon from round 1, and reporting it as "played 5 rounds"
overstated it by one.

### Gates

```
cargo test -p ti4-mlp                    22 passed, 0 failed   (12 before)
cargo test --workspace                 1454 passed, 0 failed   (1444 before)
cargo clippy -p ti4-mlp --all-targets     0 warnings
rustfmt --edition 2024 --check            clean
git diff --check                          clean
smoke, discovery seed                     0 fallbacks, exit 0
smoke, independent seed 999000111         0 fallbacks, 100% assigned, exit 0
smoke, --force-inference-failure          76 fallbacks, exit 4
```

### Still open

M09-024b2's five findings, and M09-025 F1's acquisition/recovery recipe. 024b2 needs regenerating
regardless: the unit-family predicate changed in F-M09-024b1-3, so the retained artifact must be
rebuilt and its identity re-confirmed.
