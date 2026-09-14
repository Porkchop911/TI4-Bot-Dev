# OFFLINE-PILOT-SINGLE-CHECKPOINT — single-checkpoint multi-temperature capture (2026-09-14)

## Purpose

The operator requested a generation run that uses **one** checkpoint at various temperatures
instead of the default mixed 11-kind policy cycle. Target:
`out/vponly-main-20260911/checkpoints/checkpoint-236464` (schema 7, slot_count 14877,
slots_sha256 `fa3d6f94…c41fc` — same vocabulary as the other evaluated models).

## Change

Single file: `crates/ti4-mlp/examples/capture_offline_pilot.rs`.

New CLI:

```text
--single <checkpoint-dir>          use only this checkpoint for every seat
--temperatures t1,t2,...           temperature cycle (default "0.25,1.0,2.5")
```

`--single` refuses to be combined with `--current/--older/--evolutionary`. In single mode:

- The bundle loads into the existing **current** slot; the unused older/evolutionary slots are
  filled with inert deep copies (`inference_copy`) so the per-chunk tensor-copy machinery and all
  downstream code paths stay uniform between modes. Actor is ~22 MB, so the extra copy per worker
  chunk is negligible (94 GB RAM machine).
- Seat assignment replaces `POLICY_CYCLE[(offset + seat) % 11]` with
  `temps[(offset + seat) % temps.len()]`; the per-game offset still advances by the player count,
  so temperature rotation across seats and games is a pure function of (game_index, seat_index).
- Policy ids follow the existing convention: `single_mlp_t025`, `single_mlp_t100`,
  `single_mlp_t250` for the default trio.
- Manifest gains additive field `policy_mode` (`"single"` | `"mixed"`); in single mode
  `checkpoint_manifests` contains exactly one entry (the requested checkpoint) instead of three.

## Design decisions

- **Reuse the current slot rather than adding a third actor to every struct**: smallest diff, no
  Option ripple through WorkerActors/LocalAssets/MasterAssets or the policy() arms. The invariant
  "older slots are inert in single mode" is documented at `load_single`.
- **Default temperatures 0.25 / 1.0 / 2.5**: identical to the existing greedy/standard/hot trio,
  so a single-checkpoint corpus is directly comparable with mixed-mode corpora on the temperature
  axis.
- **`parse_temperatures` validates finite > 0 and non-empty**: an empty list would make seat
  assignment divide by zero; the MLP decider refuses non-positive temperatures anyway.

## Verification

| command | result |
|---|---|
| `cargo test -p ti4-mlp --example capture_offline_pilot` | ok. **14 passed** (13 prior + `single_mode_temperatures_parse_and_refuse_bad_values`) |
| 12 games, seed base 9500001, `--workers 32` vs `--workers 1`, same `--single` checkpoint | every data shard **byte-identical** (good/decisions `803e026a…d05`, good/games `1203f61b…b32`, empty bad/random frames `6fb85438…cbd`, retention.jsonl `78a71a52…cb2`); manifests identical except `created_utc` and `workers` |
| per-seat policy audit of the 10 retained games (60 seats) | exactly three ids: `single_mlp_t025`/t100/t250, 20 seats each; all point at the requested checkpoint |
| manifest inspection | `policy_mode = "single"`, one `checkpoint_manifests` entry, buckets good=10/bad=0/random=0 |
| `cargo clippy -p ti4-mlp --example capture_offline_pilot` | clean for this example (remaining warnings pre-existing in ti4-model/ti4-bridge/ti4-policy/ti4-training) |

Determinism protocol satisfied: identical seeds → byte-identical shards at any worker count.

## Usage

```powershell
$env:PATH = "D:\Projects\ti4-engine-rs\target\release;" + $env:PATH
.\target\release\examples\capture_offline_pilot.exe `
  --out E:\ti4-corpus\<name> --games <N> --seed-base <fresh base> `
  --single out/vponly-main-20260911/checkpoints\checkpoint-236464
```

Launch script for the operator's run: `out/launch_single_ckpt_run.ps1` (game count and seed
base are marked at the top).
