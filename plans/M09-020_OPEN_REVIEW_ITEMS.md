# M09-020 open review items

Ledger for the durable-baselines / sealed-data-roles package. Findings are recorded here as they
arise; dispositions must be resolved before M09-020 closes (review tier C — frontier model).

## Implementer observations (2026-08-23) — pending Tier-C review

### O-M09-020-1 — ~30 diagnostic example binaries load pools without role wiring — INFO (spec non-goal)

The spec wires the two live corpus/panel entry points (`baseline::run_panel`,
`stage2_training.rs --map-pool`). The remaining pool-loading examples are diagnostics that produce
no baselines or checkpoints and no training; wiring them would touch ~30 files for no integrity
gain. If a diagnostic is ever promoted to a measurement command, it must gain the role check at
that time (recorded here so the gap is visible).

### O-M09-020-2 — `is_known_checkpoint` has no call site yet — INFO (by design)

Provided and tested in this package; its consumer (teacher-checksum rejection) arrives with M10-038
per §10. No dead-code warning because it is a public API of the crate.

### O-M09-020-3 — pre-existing rustfmt drift in ti4-training examples — INFO (out of scope)

`cargo fmt -p ti4-training --check` fails on ~30 files that this package never touched (the crate
was never fmt-clean). Only the lines added by this package to `stage2_training.rs` were made
fmt-conformant. A formatting-only cleanup package could close this if ever desired; it is not a
gate for M09-020 because the acceptance criterion is "clippy/fmt clean for scoped files".

### O-M09-020-4 — `.gitignore` negation block (scope extension S1) — needs reviewer confirmation

Declared in `plans/evidence/M09-020.md`: a three-line negation following the existing
`legacy_entropy/bounded-v1` convention was required because `fixtures/*` ignored everything under
`fixtures/`, blocking the spec's own "fixtures committed with manifest" acceptance criterion. The
reviewer should confirm this is the right mechanism (vs. force-adding tracked files) and that no
other ignore behavior changed.
