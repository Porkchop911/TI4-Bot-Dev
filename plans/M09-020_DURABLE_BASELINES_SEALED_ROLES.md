# M09-020 — Durable baselines and sealed data roles

**Status: complete 2026-08-23, pending independent Tier-C frontier review.** Branch
`wp/m09-020-durable-baselines-sealed-roles` from base commit `1a06ca9`. Dependencies M08-019
(accepted) and M09-018 (accepted) are met. This package is dependency-ready independently of the
pending M09-019a Tier-D recheck; its edit scope is disjoint from the files under review except
`crates/ti4-sim/src/baseline.rs` (one added role-check call), which extends — not alters — the
accepted fail-closed behavior. One declared scope extension: `.gitignore` negation block for
`fixtures/mlp-baselines/` (S1 in `plans/evidence/M09-020.md`). Evidence:
`plans/evidence/M09-020.md`, durable manifest `plans/evidence/MLP-ARTIFACTS.md`, ledger
`plans/M09-020_OPEN_REVIEW_ITEMS.md`.

Normative source: MLP plan revision 5 §10 ("Artifact manifest (prerequisite, not a
nice-to-have)"). Milestone row acceptance: "P2 ≤50 MiB compressed fixture policy, checksum
manifests, validation role for seed 777, sealed zero-overlap seed-20260822 final pool."

## Deliverables

1. **Sealed final pool (reproducible — recipe committed, file not).** Generate
   `out/pools/full_np8_12_final.json` with the §10 command (`--seed 20260822 --boards 1000
   --min 8 --max 12`, default template), verify **zero canonical board-hash overlap** against both
   train and validation pools, and commit only its generation recipe/checksum/role. No policy is
   run on it by this package or any test; only M10-038 may load final-role data, once, later. A
   collision or regeneration mismatch blocks the package.

2. **Durable fixtures (not reproducible — archived).** Deterministically compress exactly the two
   baseline checkpoints (`out/stage2_r6/final10000.json`, `out/stage1_hacanclone/frozen5000.json`)
   with a pinned single-threaded zstd tool into `fixtures/mlp-baselines/`. Record raw and
   compressed sha256, sizes, tool version/settings, license/provenance, and the extraction command
   in `fixtures/mlp-baselines/manifest.json`. Commit only if combined compressed size ≤ 50 MiB.
   No other `out/` content is added to Git. If the cap or provenance review fails, implementation
   stops for explicit P3 authority naming an external durable archive (a machine-local path is not
   accepted as durability).

3. **The manifest** (`plans/evidence/MLP-ARTIFACTS.md`, checksums verified at use): all five
   artifacts with full sha256, role (train / validation / final), recoverability, and generation
   recipe or archive reference. The seed-777 holdout's logical role is recorded as **validation**
   despite its filename; the final pool's checksum is assigned by this package.

4. **Fail-closed role enforcement.** New `ti4_sim::artifacts` module: a static durable manifest
   (full sha256 → role), `verify_pool_role(path, allowed)` that hashes the exact bytes and fails
   closed on unknown artifacts or disallowed roles, and `is_known_checkpoint(sha256)` for future
   teacher-checksum rejection. Wired into the two live corpus/panel entry points:
   - `ti4_sim::baseline::run_panel` (measurement command — final data must never be measured);
   - `stage2_training.rs --map-pool` (training command).
   Hermetic tests prove: a final-role checksum is refused by train/validation checks, known
   train/validation checksums pass, and unknown bytes fail closed. These are fail-closed tests,
   not operator conventions (§10).

## Permission class

**P2.** Bounded committed output: `fixtures/mlp-baselines/` (≤ 50 MiB compressed, two files +
manifest) plus plans/evidence records. Development scratch stays in gitignored `out/`. No network
beyond the already-locked crates.io registry entry for zstd 0.13.3 (present in Cargo.lock; no new
external crate). No external-state effects.

## Writable paths

- `crates/ti4-sim/src/artifacts.rs` (new module)
- `crates/ti4-sim/src/lib.rs` (+1 registration line)
- `crates/ti4-sim/src/baseline.rs` (one role-check call in `run_panel`)
- `crates/ti4-training/examples/stage2_training.rs` (role check at the `--map-pool` load site)
- `crates/ti4-sim/examples/seal_baselines.rs` (new deterministic sealing tool)
- `Cargo.toml` (+zstd workspace dependency), `crates/ti4-sim/Cargo.toml` (+zstd line),
  `Cargo.lock` (dependency wiring only; zstd already locked at 0.13.3)
- `fixtures/mlp-baselines/` (new: two `.zst` files + `manifest.json`)
- `plans/M09-020_DURABLE_BASELINES_SEALED_ROLES.md`, `plans/evidence/M09-020.md`,
  `plans/evidence/MLP-ARTIFACTS.md`, `plans/M09-020_OPEN_REVIEW_ITEMS.md`,
  `plans/EXECUTION_STATE.md`

Read-only: all other crates, `out/stage2_r6/final10000.json`, `out/stage1_hacanclone/frozen5000.json`,
the three pools in `out/pools/`, MLP plan §10.

## Review tier

**C — frontier model.** Sealed-data role separation is an integrity boundary the entire M10
evaluation depends on (final-role data must never inform training or baselines), and AGENTS.md
assigns security boundaries to frontier review.

## Non-goals

- No policy run, measurement, or training on final-role data (forbidden by §10; only M10-038 may
  load it later).
- No role wiring in the ~30 diagnostic example binaries that load pools — recorded as a ledger
  observation with rationale and follow-up note; they produce no baselines or checkpoints. The two
  live corpus/panel entry points (baseline panel, stage-2 trainer) are wired now.
- No M10-038 distillation work; `is_known_checkpoint` is provided and tested but its call site
  arrives with the teacher-comparison command.
- No retraining, no checkpoint modification, no pool regeneration beyond the three §10 commands.

## Acceptance

- Final pool generated by the exact §10 recipe; zero overlap against train and validation
  (recorded); checksum assigned in the manifest.
- Both pools regenerate bit-for-bit on the current tree matching their manifest checksums
  (corpus-has-not-moved proof).
- `fixtures/mlp-baselines/` committed with manifest; combined compressed size ≤ 50 MiB; sealing is
  reproducible byte-for-byte from the raw inputs by the committed command.
- Role enforcement wired at both entry points; hermetic fail-closed tests pass (final refused,
  train/validation accepted, unknown bytes refused).
- Full workspace suite green; clippy/fmt clean for scoped files; evidence with exact outputs.
