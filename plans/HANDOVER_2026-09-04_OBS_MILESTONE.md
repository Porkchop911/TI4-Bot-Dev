# Handover — OBS decision-surface milestone (2026-09-04)

## Objective

Continue the Stage 2 complete-decision-contract work after payment and the first production-unit
surface. The next bounded package is `OBS-008c2b`: expose only final fleet-supply and transport
consequences that depend on the later production-placement choice.

## Normative source versions

- `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`
- `plans/OBS-008C1_PAYMENT_DECISION_SURFACE.md`
- `plans/OBS-008C2A_PRODUCTION_LIMIT_SURFACE.md`
- LRR 68.1, 68.1a, and 68.3b for the completed production-limit slice
- Existing Bellum Gloriosum helpers in `crates/ti4-engine/src/breakthroughs.rs`
- Historical Python was not inspected and is not an acceptance oracle.

## Active milestone/package

- `OBS-008c` decision surface, after completed `OBS-008c1` and `OBS-008c2a`.
- Next ready package: `OBS-008c2b` final fleet/transport consequences after the placement choice.

## Status and completed acceptance criteria

- `OBS-008c1` is committed as `4bdf247`: typed choice context, nonserialized previews, and exact
  shared-payment debt/consequence facts.
- `OBS-008c2a` is committed as `0daa8a3`: Rule 68 production context, original/consumed production
  capacity, exact marginal bill/credit/yield facts, and analytic remaining-production plus Bellum
  allowance previews under the existing transferable `production` policy family.
- The c2a preview mirrors actual placement arithmetic without cloning or applying game state. It
  proves both a standard build and Sol's capacity-ship allowance followed by a fighter purchase.
- The policy surface has counterfactual coverage for capacity, credit, allowance, and absent/
  unknown/unavailable previews. Unknown facts never become numeric zero.
- Option IDs, labels, legal set, payment application, replay behavior, choice identity, and serde
  behavior remain unchanged in c2a.

## Current branch and HEAD

- Branch: `wp/obs-008c2a-production-limit-surface`
- HEAD: `0daa8a3477f5f002af421bfe01a380974990b5b2`
- Head commit: `0daa8a3 OBS-008c2a: expose production limits and allowance`

## Working-tree state

- After this documentation checkpoint, the only untracked files are the pre-existing,
  never-read-or-staged `sample.html` and `sample.ti4review.json`.
- Do not reset, clean, or stage those samples.

## Tests last run and exact results

- `cargo test -p ti4-engine --lib obs008c2a --quiet`: 2 passed.
- `cargo test -p ti4-policy --lib obs008c2a --quiet`: 2 passed.
- `cargo test -p ti4-engine --quiet`: 1,141 library + 4 integration + 5 doc tests passed.
- `cargo test -p ti4-policy --lib -- --skip scored_games_stay_legal_and_deterministic_across_nested_windows`:
  197 passed.
- `cargo test -p ti4-policy --lib bot::tests::scored_games_stay_legal_and_deterministic_across_nested_windows -- --exact --nocapture`:
  1 passed; 102-game deterministic campaign in 328.21 seconds.
- `cargo test -p ti4-training --lib --quiet`: 133 passed.
- `cargo clippy -p ti4-engine -p ti4-policy --all-targets -- -D warnings`: passed.
- `git diff --check`: passed before this documentation-only handover.

`cargo fmt --check` identifies pre-existing formatting drift in unrelated `ti4-content`,
`ti4-model`, and `ti4-sim` files. Do not reformat unrelated code. Direct formatter output also
contains pre-existing drift elsewhere in `production.rs`; no c2a-added hunk required formatting.

## Compatibility evidence

- Evidence: `plans/evidence/OBS-008C1.md` and `plans/evidence/OBS-008C2A.md`.
- C2a's initial independent Tier-C review found two P2 test gaps: the post-build continuation
  capacity context and non-certain production preview state. Both were added and the recheck was
  **PASS / APPROVED**, with no remaining mechanics, arithmetic, hidden-information, replay, serde,
  identity, performance, or test-adequacy finding.

## Decisions made and rationale

- C2 was split before code: c2a owns the choice-time facts determined before placement; c2b owns
  consequences that require a placement destination. This prevents any preview from claiming
  final fleet/transport truth too early.
- `ProductionFreeCapacity` means only Sol's per-use Bellum Gloriosum allowance. It is explicitly
  not transport capacity.
- Production facts stay in the established transferable `production` family; no vocabulary build,
  migration, bundle republish, or payment-legality change belongs to this slice.

## Open review findings or blockers

- No blocker or open Tier-C finding.
- C2b needs its own exact package specification, evidence, tests, and independent review before
  implementation. It must not broaden into transport or fleet-supply mechanics changes.

## Next exact action/command

1. Read `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`, `plans/PI_WORK_PACKAGE_STANDARD.md`, and
   c2a evidence; then write `plans/OBS-008C2B_*.md` before making source changes.
2. Implement only truthful post-placement fleet/transport consequences, with analytic/application
   agreement and policy counterfactual tests.

## Files to read first after compaction

1. `AGENTS.md`
2. `plans/SCOPED_PERMISSIONS.md`
3. `plans/EXECUTION_STATE.md`
4. `plans/HANDOVER_2026-09-04_OBS_MILESTONE.md`
5. `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`
6. `plans/PI_WORK_PACKAGE_STANDARD.md`
7. `plans/evidence/OBS-008C1.md` and `plans/evidence/OBS-008C2A.md`
