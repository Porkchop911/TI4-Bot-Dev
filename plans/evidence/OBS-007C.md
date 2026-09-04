# Evidence — OBS-007c, stochastic preview foundation

## Scope and provenance

- Branch: `wp/tier-c-review-remediation-obs008c2b-003e1`, after `4b69835` (OBS-005).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (OBS-007c row, §3 stochastic
  paragraph); `plans/OBS-007C_STOCHASTIC_PREVIEW_FOUNDATION.md`; LRR 78.13;
  `crates/ti4-engine/src/dice.rs`'s existing `hits_on`/`hits()` threshold convention.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, or committed generated
  artifact.

## Changed paths

- `crates/ti4-engine/src/preview.rs`
- `plans/OBS-007C_STOCHASTIC_PREVIEW_FOUNDATION.md`
- `plans/evidence/OBS-007C.md`
- `plans/EXECUTION_STATE.md`

## Result

`preview::stochastic::hit_count_preview(dice, hit_on, deltas_for)` computes the exact binomial
hit-count distribution for a bounded d10 pool, matching `dice.rs`'s own `hits_on`/`hits()` threshold
convention. `hit_on <= 1`, `hit_on > 10`, and `dice == 0` all collapse to `Preview::certain` (no
chance is actually involved); otherwise every case's weight is an exact integer count out of `10^
dice`, never a rounded float. Pools larger than `MAX_DICE = 9` (the largest power of ten that fits
`Chance::weight`'s `u32`) return `Preview::unknown` rather than a rescaled or truncated distribution
— the module never approximates a rules-exact quantity.

No new API is added for "irreducible unknown draws": `Preview::unknown`, already shipped in
OBS-007a, is correct and sufficient, and the spec records why inventing a parallel helper would be
redundant rather than adding one for its own sake.

## Focused evidence

- `hit_count_distribution_matches_hand_computed_binomial_odds`: 2 dice at 6+ against the
  hand-computed 25/50/25 of 100 split, exactly.
- `hit_on_at_or_below_one_is_certain_not_a_one_case_distribution`,
  `hit_on_above_ten_is_certain_at_zero_hits`, `zero_dice_is_certain_at_zero_hits_for_any_threshold`:
  the three degenerate cases collapse to `Certain`, not a trivial `Chanced`.
- `nine_dice_is_the_largest_exact_pool_and_ten_is_refused`: the `MAX_DICE` boundary is exact — nine
  dice is informative, ten is `Unknown`, not silently rescaled.
- `the_expected_hit_count_is_the_textbook_binomial_mean`: 3 dice at 7+ gives the exact rational
  1200/1000 (= 1.2) through `Preview::expected`, chosen because it does not reduce to a whole
  number — proving the rational survives rather than being rounded.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs007c` | 6 passed |
| `cargo test -p ti4-engine --quiet` | 1,185 lib + 4 integration + 5 docs passed |
| `cargo clippy -p ti4-engine --all-targets -- -D warnings` | passed |
| `cargo fmt --check` (preview.rs) | passed |
| `git diff --check` | passed |

This package touches only `ti4-engine`; the policy/training suites and the deterministic campaign
are unaffected and were not rerun (no policy-facing code changed).

## Compatibility and non-goals

- No option ID, label, legal set, state transition, prompt, or replay contract changed.
- `Chance::weight`'s `u32` type (an OBS-007a decision) is not reopened; pools beyond it fail closed.
- No RNG is consumed; every computation is analytic, matching `preview.rs`'s own stated principle.
- No producer (OBS-008b/g/h) yet calls this helper — attaching it to combat/invasion/reroll steps
  is later work this foundation unblocks, not this package's scope.
- No card-draw distribution helper added; hidden deck order and hand contents remain unavailable.

## Independent Tier-C review

**OUTSTANDING**, per current instruction to continue without waiting on it.
