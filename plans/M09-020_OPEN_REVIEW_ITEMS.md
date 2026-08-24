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

## Independent Tier-C frontier review of `52c17fb` (2026-08-24)

**Verdict: changes required; M09-020 is not accepted.** The bounded fixtures, hashes,
reproducible sealing command, pool separation, and scoped gates independently reproduce. The
role boundary is not yet fail-closed for the bytes actually consumed.

### F-M09-020-1 — role verification and parsing use different reads — HIGH

`verify_pool_role` hashes one `fs::read`, returns only `()`, and both live consumers subsequently
reopen the path through `MapPool::load`. `baseline::run_panel` additionally performs a separate
checksum read. A pool can change after approval and before parsing, allowing bytes with an unknown
or final role to be consumed. Make role verification and parsing operate on one immutable byte
buffer (or return a parsed pool derived from the verified bytes) at both live consumers, and add a
focused test for the unified boundary.

### F-M09-020-2 — zstd license note does not describe the locked Rust dependencies — MEDIUM

The fixture manifest and sealing tool state `zstd is BSD-3-Clause`. `cargo metadata` reports the
locked Rust packages as `zstd 0.13.3` MIT, `zstd-safe 7.2.4` MIT OR Apache-2.0, and
`zstd-sys 2.0.16+zstd.1.5.7` MIT/Apache-2.0. If the note intends the bundled upstream native zstd
library, say so separately and record the Rust wrapper-chain licenses. Update the deterministic
manifest generator, regenerate the manifest, and keep provenance accurate.

### O-M09-020-4 disposition — ACCEPTED

The `.gitignore` negation is the correct repository-visible mechanism. Independent
`git check-ignore -v --no-index` checks showed all three intended fixture paths end at
`!fixtures/mlp-baselines/**`, while a sibling `fixtures/other-probe.txt` remains ignored by
`fixtures/*`; `git ls-files fixtures/mlp-baselines` lists exactly the two archives and manifest.
This is preferable to an opaque force-add and does not broaden sibling fixture behavior.
