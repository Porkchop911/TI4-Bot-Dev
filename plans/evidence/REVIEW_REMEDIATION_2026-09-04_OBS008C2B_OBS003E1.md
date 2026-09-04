# Evidence — Tier-C remediation for OBS-008c2b and OBS-003e1

## Scope and provenance

- Branch: `wp/tier-c-review-remediation-obs008c2b-003e1`.
- Base: `6ccdfb0` (`OBS-003g`).
- Normative sources: the remediation specification, LRR 16/37/81.5, and the Tier-C review findings
  against `caac1e4`/`15158b0` and `a5cfb29`.
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, dependency, or committed
  generated artifact.

## Changed paths

- `crates/ti4-engine/src/production.rs`
- `crates/ti4-engine/src/strategy_cards.rs`
- `crates/ti4-engine/src/game.rs`
- `crates/ti4-policy/src/features.rs`
- `plans/REVIEW_REMEDIATION_2026-09-04_OBS008C2B_OBS003E1.md`
- `plans/evidence/REVIEW_REMEDIATION_2026-09-04_OBS008C2B_OBS003E1.md`
- `plans/EXECUTION_STATE.md`

The pre-existing `crates/ti4-engine/src/action_cards.rs` edit and untracked `sample.html`/
`sample.ti4review.json` are outside this package and will not be staged.

## Result

- `units_removed_after` is removed. Placement options instead expose exact
  `fleet_excess_after` and `capacity_excess_after` facts for the pre-enforcement position. They do
  not pretend that independent excesses determine future removals, whose order and chosen units are
  not fixed by the placement option.
- The policy reads and projects those facts in the existing `production` family.
- `redistribute_tokens` receives a caller-provided source/subtype. Warfare primary passes its card
  source; `Game::finish_status_phase` passes Rule 81.5 and `status_redistribute_tokens`.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib placement_facts_name_each_pre_enforcement_violation_without_predicting_removals --quiet` | 1 passed |
| `cargo test -p ti4-engine --lib status_redistribution_carries_the_status_rule_not_warfare --quiet` | 1 passed |
| `cargo test -p ti4-policy --lib obs008c2b --quiet` (isolated target) | 2 passed |
| `cargo test -p ti4-engine --quiet` | 1,175 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows` (isolated target) | 201 passed |
| `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture` (isolated target) | 1 passed; 102-game campaign, 307.26 s |
| `cargo test -p ti4-training --lib --quiet` | 133 passed |
| `cargo clippy -p ti4-engine -p ti4-policy --all-targets -- -D warnings` | passed |
| `git diff --check` | passed |

Policy used `C:\Users\Niko\AppData\Local\Temp\ti4-review-remediation-target` because a separate
live policy test executable held the shared target binary open. The temporary target is uncommitted.

`cargo fmt -p ti4-engine -p ti4-policy -- --check` reports formatting only in concurrently modified
`crates/ti4-engine/src/exploration.rs`, outside this package. No unrelated formatting was written.

## Review-finding resolution

1. The old `fleet_excess + capacity_excess` sum was neither an enforcement simulation nor a stable
   removal count: fleet removals run first and can alter capacity, while the owner chooses removals.
   The replacement facts name exactly what `fleet::standing` knows before enforcement. Focused engine
   and policy tests prove the old forecast is absent, each violation is present, and the latter
   reaches MLP projection.
2. `strategy_cards::redistribute_tokens` is shared by Warfare and status phase. It now receives its
   context identity from the caller; the new regression proves the status invocation shape is
   Rule 81.5 / `status_redistribute_tokens`, not Warfare.

## Independent Tier-C re-review

Approved. The independent re-review confirmed that the placement surface no longer represents
independent excesses as a forced-removal result. It also confirmed that status redistribution now
carries Rule 81.5, its separate subtype, and a neutral prompt, while Warfare retains its card-scoped
prompt. No remaining findings.

## Compatibility and non-goals

- No fleet/capacity or token-redistribution mechanics, timing, IDs, or labels changed.
- The observation change removes an inaccurate feature and replaces it with two exact facts; it does
  not claim an enforcement outcome that depends on later player decisions.
- No historical Python, vocabulary generation, bundle republish, or external state was used.
