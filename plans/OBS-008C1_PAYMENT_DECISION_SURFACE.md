# OBS-008c1 — payment decision surface

## Package

- Milestone: Stage 2 complete decision contract, first slice of `OBS-008c`.
- Dependencies: `OBS-003a–c`, `OBS-004`, `OBS-007a–b`.
- Objective: deliver typed debt, exact per-face payment consequences, overpay and remaining
  payment flexibility to the acting policy without changing payment legality or application.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008c` and static/
  counterfactual gates), `plans/OBS-003A_TYPED_CHOICE_CONTEXT_SCHEMA.md`,
  `plans/OBS-007A_PREVIEW_CONTRACT.md`, `plans/OBS-007B_DETERMINISTIC_PREVIEW.md`.
- Acceptance references: focused `obs008c1` tests in `choice.rs`, `production.rs`, and
  `ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class required: P1.
- Writable paths: `crates/ti4-engine/src/choice.rs`, `crates/ti4-engine/src/production.rs`,
  `crates/ti4-policy/src/features.rs`, this specification, package evidence, and the package row
  in `plans/EXECUTION_STATE.md`.
- Read-only external paths: none.
- Network access, processes/ports, destructive actions, external-state changes: none.
- Expected generated artifacts: ordinary Cargo target output only; no committed bulk artifacts.

## Inputs, outputs, and compatibility

- Input: a legal multi-option resource or influence payment question and its actor-bound state.
- Output: the same options and prompts, plus an in-memory typed `DecisionContext` on the choice and
  an exact `Preview` on every offered payment face; explicit policy facts under the already-approved
  transferable `pay` family.
- `Choice.context` is optional and serde-defaulted. Old choices deserialize and serialize as before.
- `ChoiceOption.preview` is runtime analysis and is skipped by serde. It does not change option
  identity, payloads, legal sets, labels, or replay scripts.
- A recorded decision copies the supplied context. V1 hashes remain stable because the existing V1
  path strips context; V2 binds it as specified by `OBS-003b`.

## Normative behavior

1. A payment question states the full transaction bill and amount already paid, so its remaining
   debt is not reconstructed from prompt text. Transaction-local credit counts as already paid.
2. Each offered face previews the exact spendable-pool and trade-good state after taking that one
   face. The pool falls by the face's full worth, including overpayment.
3. The policy receives explicit amount/paid/remaining debt, option count, preview-known marker,
   before/after/change quantities, and the existing per-face cover/overpay/shortfall facts.
4. Unknown or unavailable previews never emit numeric consequence facts. A computed zero is
   distinguished by the preview-known marker.
5. All new policy names stay in the existing reviewed `pay` family; no vocabulary family or bundle
   generation changes in this slice.

## Non-goals

- Production-unit marginal cost/capacity/fleet consequences; that is `OBS-008c2`.
- Producer-specific semantic sources for every caller of the shared payment primitive.
- Prompt-free projection (`OBS-003i`), vocabulary publication (`OBS-011`), or legal/mechanical
  payment changes.
- The remaining `timing.rs::pick` seat-delivery migration.

## Tests and commands

- Add a replay test proving a supplied choice context is recorded and V1 compatibility holds.
- Add payment tests proving debt/credit context, exact planet and trade-good previews, full-worth
  overpay, and unchanged option identity/sets.
- Add policy counterfactual tests proving paid debt, payment flexibility, and exact consequences
  change features while an unknown preview emits no fabricated numeric delta.
- Run `cargo test -p ti4-engine --lib obs008c1`.
- Run `cargo test -p ti4-policy --lib obs008c1`.
- Run affected-crate suites and strict all-target Clippy for both crates.
- Run targeted rustfmt and inspect the exact diff.

## Known traps

- Do not compute a whole-payment plan for one face; preview only the offered face.
- Do not subtract the remaining debt where a face overpays; subtract the face's full `worth`.
- Do not serialize previews carrying static diagnostic reasons into replay artifacts.
- Do not encode missing preview values as zero.

## Definition of done

The actor can distinguish otherwise-identical payment decisions by debt and already-paid amount;
each face exposes exact immediate consequences and overpay; flexibility is visible; old choices and
V1 replay hashes remain compatible; focused and affected checks pass; Tier-C review findings are
resolved; evidence is committed without unrelated files.
