# Codex Stage-1 Save-54 map parity

## Package specification

- **Objective:** load the Python `ti4-map-pool-v1` JSON.GZ artifact exactly, construct Save-54
  galaxies from its arrangements, and run the Stage-1 solved-profile comparison on the same maps.
- **Oracle:** `D:\Projects\ti4-engine` at `37061c511a4780d4c0719e0342533a498cd4b457`, read-only.
- **Oracle sources:** `engine/sim.py` (`MapPool`, `save_54_galaxy`) and
  `data/map_pools/save54_e2000_n8192.json.gz`.
- **Compatibility class:** exact artifact schema and deterministic draw; semantic game RNG.
- **Allowed Rust edits:** `crates/ti4-sim`, `crates/ti4-training`, workspace dependency manifests,
  Stage-1 parity documentation and this evidence file.
- **Non-goals:** building new pools, reproducing Python's `random.Random`, six-player Save-52,
  optimizer changes, or claiming full engine parity from aggregate clearance.

## Scoped permission declaration

- Permission class required: P1 for source/tests; P2 for downloading the maintained `flate2`
  crate and for a bounded 96-game differential panel.
- Writable paths: `D:\Projects\ti4-engine-rs` package files and ignored build output only.
- Read-only external paths: the named Python oracle source and Save-54 pool.
- Network access: Cargo registry only if `flate2` is absent from the local cache.
- Processes/ports: bounded Cargo build/tests and local comparison processes; no ports.
- Generated artifacts: Cargo build products only; no committed pool copy and no output over 10 MB.
- Destructive actions: none.
- External-state changes: none.

## Acceptance checks

1. Reject malformed schema, coordinate/slot mismatch, arrangement-width mismatch, duplicates, and
   unknown systems.
2. Draw is deterministic and uses the Python `seed % len(pool)` rule.
3. Faction homes replace pool home slots without modifying the loaded pool.
4. Three rotations share one selected outer arrangement.
5. Python and Rust can be evaluated on the same 32 pool selections and report per-faction metrics.

## Results

Implemented:

- `ti4-sim::MapPool` reads JSON and JSON.GZ with a 64 MiB decompressed bound, validates schema,
  dimensions, coordinates, slots, arrangement widths and duplicate systems, and draws by modulo.
- `MapPool::galaxy` replaces captured home slots by physical-seat order and lets `Galaxy::placed`
  validate every referenced system.
- `OpeningMap::Save54Pool`, the rotated batch runner, `FactionPlan`, evaluation, and the parity CLI
  all accept the same validated pool. Training and held-out evaluation use the pool when configured.
- `flate2 1.1.9` was obtained from crates.io; its only new lockfile package was
  `crc32fast 1.5.0` (the remaining inflate dependencies were already locked transitively).

Same-pool 32-seed x 3-rotation solved panel (`seed + 20_000_000`):

| engine | Hacan | Jol-Nar | Letnev |
|---|---:|---:|---:|
| Python | 0.969 | 0.979 | 0.865 |
| Rust | 0.312 | 0.292 | 0.010 |

Rust planet/system/unit means were respectively Hacan `2.43/2.46/1.62`, Jol-Nar
`2.26/2.29/2.47`, and Letnev `1.35/2.19/1.73`. Python planet means were `3.80/3.00/2.81`.
The controlled panel closes map-family uncertainty and leaves a game/decision-boundary gap.

Checks:

- `cargo test -p ti4-sim --lib` — 27 passed.
- `cargo test -p ti4-training --lib` — 93 passed.
- `cargo clippy -p ti4-sim --lib -- -D warnings` — passed.
- `cargo clippy -p ti4-training --lib --example stage1_parity -- -D warnings` — passed.
- Actual 8,192-arrangement gzip pool load and 96-game Rust panel — passed with no rollout errors.
- Python oracle same-pool panel ran with `PYTHONDONTWRITEBYTECODE=1` and `python -B`; no oracle
  artifacts were generated.
- Oracle HEAD remained `37061c511a4780d4c0719e0342533a498cd4b457`. Its pre-existing untracked
  `docs/POLICY_GRADIENT_HANDOVER.md` was present before and after this package; it was not created
  or modified by the map work.

The pre-existing `stage1_curve.rs` `manual_is_multiple_of` lint remains outside this package and
prevents an all-examples `-D warnings` invocation; the package-owned example is clean.
