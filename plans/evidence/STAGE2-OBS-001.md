# STAGE2-OBS-001 — actor observation surface evidence

## Identity and scope

- Branch: `wp/stage2-actor-observation-surface`
- Base: `b77e18b`
- Safe point: `safepoint/pre-actor-gamestate-surface-2026-09-03` -> `b77e18b`
- Specification: `plans/STAGE2_ACTOR_OBSERVATION_SURFACE.md`
- Review tier: C (observation/hidden-information boundary); independent review pending.
- External or historical repository access: none.

## Implementation

- Reconciled the current lineage before editing. Pi's earlier Stage-2 feature work is already
  ancestral to the package base: `50468b0` added opening progress/concentration, `36dacf3` and
  `16ebc7b` added and corrected movement/commit option facts, and `b0ad876` added activation reach,
  worth, occupation, and objective consequences. The clean `wp/engine-completion` worktree has no
  pending diff to merge.
- Extended the typed public `Observed` facade with phase-order relations, pending step, custodians,
  planet readiness, production capacity, fleet limit, and public strategy-card/technology readiness.
- Added the MLP-only public surface under the existing transferable `seat-state` family. It includes
  timing/order, own public standing/economy/board footprint, opponent public aggregates, public card
  identities/readiness, board occupancy and active-system threat, fleet headroom, production
  opportunities, Mecatol/custodians, and faceup promissory/Support relationships.
- Kept `explicit_choice_features` unchanged. Tests confirm the legacy schema-4 vector remains
  untouched and the richer state is present under every MLP crossing mode.
- Existing bundle/vocabulary indices are unchanged. A future vocabulary discovery/publish is needed
  before every new fact has its own trained column; old bundles route unseen names through the
  already reserved `seat-state` OOV row.

## Verification

- `cargo test -p ti4-policy projection::tests -- --nocapture`: 22 passed.
- `cargo test -p ti4-engine`: 1,108 unit tests and 5 doc tests passed.
- `cargo test -p ti4-policy`: 194 passed after the final review fixes.
- `cargo test -p ti4-mlp`: 90 unit, 3 API-boundary, 3 critic-invariance, 2 refusal, 1 doc, and 2
  compile-fail doc tests passed.
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine -p ti4-policy --all-targets`: passed after fixing
  the two package-owned lint findings.
- `rustfmt --edition 2024 --check crates/ti4-engine/src/choice.rs crates/ti4-policy/src/projection.rs`:
  passed. Workspace-wide `cargo fmt --check` remains blocked by pre-existing drift in
  `ti4-content/src/galaxy.rs`, `ti4-model/src/state.rs`, and
  `ti4-sim/examples/rebaseline_behavior.rs`; those unrelated files were not changed.
- `git diff --check`: passed.

## Review status

Independent Tier-C review: **PASS / APPROVE**. The reviewer initially found three blockers:

1. `board.len()` exposed storage/history rather than semantic board state;
2. War Machine made empty systems appear to be production opportunities and the scan rebuilt the
   unit catalogue per system;
3. opponent spendable economy and system footprint were absent.

All three were corrected and covered by focused regressions. The final recheck found no remaining
hidden-information, semantic, determinism, non-transferable-identity, or material performance
finding. Final reviewer checks: projection 22/22, engine choice 43/43, and diff-check clean.
