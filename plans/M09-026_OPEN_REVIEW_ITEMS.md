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
