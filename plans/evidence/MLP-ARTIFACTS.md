# MLP artifact manifest (durable)

Owner: M09-020. Normative source: MLP plan revision 5 §10 ("Artifact manifest
(prerequisite, not a nice-to-have)"). This file is the single durable record of the five
artifacts outside Git that the M09/M10 work depends on. Checksums are verified at use by
`ti4_sim::artifacts` (pools) and by the committed fixture-integrity test (checkpoints); a
mismatch means the corpus moved and every number measured against it needs re-reading.

## The five artifacts

| # | Artifact | Path | Role | sha256 (full file bytes) | Recoverability | Recipe / archive reference |
|---|----------|------|------|--------------------------|----------------|----------------------------|
| 1 | Train map pool | `out/pools/full_np8_12_train.json` | **train** | `106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df` | Reproducible — regenerate, do not archive | `cargo run --release -p ti4-training --example generate_pool -- --seed 1 --boards 4000 --min 8 --max 12 --out out/pools/full_np8_12_train.json` (xorshift64*, no clock, no thread order) |
| 2 | Validation map pool | `out/pools/full_np8_12_holdout.json` | **validation** (seed-777 holdout; logical role is validation despite its filename — it has already informed architecture and thresholds) | `aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5` | Reproducible — regenerate, do not archive | `cargo run --release -p ti4-training --example generate_pool -- --seed 777 --boards 1000 --min 8 --max 12 --out out/pools/full_np8_12_holdout.json` |
| 3 | Final map pool (sealed) | `out/pools/full_np8_12_final.json` | **final** — sealed; no policy is run on it by any package or test; only M10-038 may load it, once, after models and analysis are frozen | `693253ecbcb33ac61c416110836286242be39271ecf49381a99c90acca653245` (assigned by M09-020) | Reproducible — regenerate, do not archive; zero canonical board-hash overlap against artifacts 1 and 2 verified at generation (recorded in `plans/evidence/M09-020.md`) | `cargo run --release -p ti4-training --example generate_pool -- --seed 20260822 --boards 1000 --min 8 --max 12 --out out/pools/full_np8_12_final.json` |
| 4 | Stage-2 r6 baseline checkpoint (update 10000, completed run) | `out/stage2_r6/final10000.json`; archived as `fixtures/mlp-baselines/final10000.zst` | **baseline** — measurement/baseline comparison only; never loaded by M10-038 final evaluation | raw `be792a2a207ced25d589162d875bae4fb1f320c8e5637045486db6a24ce5b55b` (33,886,908 bytes); compressed `c6bc823a23d6f8c7636e89a817fc3536c7a63628cf652741521d92e5f1d4e543` (4,524,217 bytes) | Not reproducible — training is stochastic across threads; archived as a bounded committed fixture | zstd crate 0.13.3, level 19, single-threaded `encode_all`; re-seal command `cargo run --release -p ti4-sim --example seal_baselines` (refuses on any mismatch); extraction `zstd -d fixtures/mlp-baselines/final10000.zst -o out/stage2_r6/final10000.json` |
| 5 | Stage-1 hacan-clone baseline checkpoint (update 5000, frozen) | `out/stage1_hacanclone/frozen5000.json`; archived as `fixtures/mlp-baselines/frozen5000.zst` | **baseline** — measurement/baseline comparison only; never loaded by M10-038 final evaluation | raw `0d0fa9e5d7a3f9ce848ef2c52a4a4144183af7ca5c15082850874a18c039ca4a` (6,261,762 bytes); compressed `6f9b90152e619b608b594b62fcfd3e59831b721570822efb75cd5bc6c5cca491` (580,437 bytes) | Not reproducible — archived as a bounded committed fixture | same tool/settings/commands as artifact 4 (`frozen5000.zst`) |

## Role rules enforced in code

- `ti4_sim::artifacts::verify_pool_role(path, allowed)` hashes the exact file bytes and fails
  closed on unknown artifacts or disallowed roles. Wired at both live corpus entry points:
  `ti4_sim::baseline::run_panel` (allows Train/Validation only) and
  `stage2_training.rs --map-pool` (allows Train/Validation only). The final pool's checksum is in
  the static manifest with role Final, so any attempt to train or measure on it fails closed.
- `ti4_sim::artifacts::is_known_checkpoint(sha256)` recognizes artifacts 4 and 5 by raw sha256;
  its call site (teacher-checksum rejection) arrives with M10-038.
- The committed fixture-integrity test (`artifacts::tests`) verifies the sealed `.zst` files
  against `fixtures/mlp-baselines/manifest.json` and re-decompresses them to confirm the raw
  bytes match artifacts 4 and 5 above.

## Provenance and license

- All five artifacts are this repository's own generated data (no third-party content).
- Compression toolchain licenses (verified against `cargo metadata --locked` on the current tree,
  recorded in `fixtures/mlp-baselines/manifest.json`):
  - Rust wrapper chain: `zstd 0.13.3` MIT; `zstd-safe 7.2.4` MIT OR Apache-2.0;
    `zstd-sys 2.0.16+zstd.1.5.7` MIT/Apache-2.0.
  - Bundled upstream native library: zstd 1.5.7, BSD-3-Clause (bundled by `zstd-sys`).
- Compression does not alter content; the fixtures contain only repository-generated training
  outputs.
- Combined compressed fixture size: 5,104,654 bytes — under the 50 MiB cap of MLP plan §10 /
  milestone row P2.
