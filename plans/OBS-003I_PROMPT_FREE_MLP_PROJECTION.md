# OBS-003i — prompt-free MLP projection

## Package

- Milestone: Stage 2 complete decision contract.
- Dependencies: typed producer migration through OBS-003h (`cf807e8`).
- Objective: make the MLP input depend on stable option IDs and factual fields, never `Choice`
  prompt text or `ChoiceOption` display labels, while preserving the frozen schema-4 extractor.
- Normative source: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md`, especially §§2 and 4 and
  OBS-003i's row.

## Scope and permissions

- Permission: P1.
- Writable paths: `crates/ti4-policy/src/features.rs`, `crates/ti4-policy/src/projection.rs`,
  `crates/ti4-policy/src/vocabulary.rs` (the dead-row invariant only), this specification, its
  evidence, and `plans/EXECUTION_STATE.md`; plus `crates/ti4-mlp/src/bundle.rs` and
  `crates/ti4-mlp/src/lib.rs` for the required fail-closed projection ABI and dead-row invariant.
- No external reads, network, ports, generated artifacts, destructive action, or external-state
  change.

## Contract

- `explicit_option_features` and `explicit_choice_features` are schema-4 compatibility surfaces
  and remain byte-for-byte unchanged.
- The MLP projection obtains a distinct source vector that tokenizes only stable `option.id`, never
  `option.label`, and supplies no prompt tokens. Structured factual option features, seat facts,
  and already-approved typed payment/production facts remain unchanged.
- `prompt-kind` is not an MLP-admitted family. Existing raw schema-4 names remain intact, but no
  new MLP vector contains prompt- or display-label-derived identity.
- Schema-7 bundles record projection ABI 2 and require it at load. Schema-6 bundles therefore fail
  clearly instead of silently scoring with a changed feature meaning.
- Prove exact projected-vector invariance under prompt and label rewording, while a distinct stable
  option ID remains distinguishable. No vocabulary publication, bundle migration, training run, or
  policy-weight artifact changes belong here; OBS-011 owns publication/migration.

## Checks

- Focused projection regression, full policy suite, engine/training suites, strict Clippy, formatter
  check limited to owned files, diff check, and Tier-C independent review.
