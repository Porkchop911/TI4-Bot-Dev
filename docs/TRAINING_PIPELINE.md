# Rust policy-gradient training pipeline

This is the production path for learned-policy Stage 1 and Stage 2. Both stages use one optimizer
and one deterministic parallel execution substrate; only the reward, horizon, factions, and map
pool differ.

## Execution architecture

Each update freezes the current faction profiles behind `Arc<Profile>`. Rayon schedules every
`(map seed, physical-seat rotation)` game on one persistent work-stealing pool. A worker plays its
game and immediately reduces the trajectory to sufficient policy-gradient statistics. The parent
never serially revisits the full legal-option/feature matrices.

Worker results are collected in deterministic seed/rotation order. Statistics are merged in
parallel by `(faction, decision head)`, while each pair preserves the original game order for
floating-point accumulation. The policy update begins only after the whole batch is reduced.

This replaces three expensive behaviours:

- deep-cloning a roughly 30,000-weight schema-4 profile for every seat in every game;
- creating and destroying a bespoke set of scoped threads every update; and
- returning all trajectories to one serial reducer.

`RAYON_NUM_THREADS` may cap the persistent pool. If it is unset, Rayon uses the logical processors
available to the process. More threads do not change seeds, rotation order, statistics, or weights.

## Stage definitions

| Property | Stage 1 | Stage 2 |
|---|---|---|
| Purpose | Learn the opening clearance task | Learn VP and objective progress |
| Horizon | One round | Four rounds by default; configurable with `--rounds` |
| Default factions | Letnev, Jol-Nar, Hacan | Sol, Letnev, Xxcha, Hacan, Jol-Nar, L1Z1X |
| Rotations | 3 | 6 |
| Training maps | Save-54 pool | Save-52 pool |
| Default seeds/update | 16 | 16 |
| Games/update | 48 | 96 |
| Teacher/action labels | Never | None; no validated faction-meta artifact exists yet |
| Bootstrap checkpoint | Continuation only | Optional Stage-1 or Stage-2 learner/champion profiles |

Stage 1 remains teacher-free. Stage 2's `--checkpoint` is a policy bootstrap, not imitation: the
configured-horizon Stage-2 reward still decides every gradient. If faction-specific teacher/meta scoring is
added later, it belongs in the Stage-2 reward path only; it must not inject authored legal-choice
scores into Stage 1 or inference.

## Stage 1

Build:

```powershell
cargo build --release -p ti4-training --example stage1_parity
```

Run from blank on varied Python-compatible Save-54 maps:

```powershell
.\target\release\examples\stage1_parity.exe `
  --updates 25000 --every 100 --eval-seeds 32 `
  --map-pool D:\Projects\ti4-engine\data\map_pools\save54_e2000_n8192.json.gz `
  --out out\stage1_rust.json
```

The output checkpoint is replaced at every reporting boundary. The checkpoint stores explicit
named profiles and the completed update, so a stopped run retains the last fully completed block.

## Stage 2

Build:

```powershell
cargo build --release -p ti4-training --example stage2_training
```

Blank six-faction run on the Python-compatible Save-52 pool:

```powershell
.\target\release\examples\stage2_training.exe `
  --updates 1000 --every 25 --train-seeds 16 --rounds 4 `
  --validation-seeds 32 --confirmation-seeds 32 --accept-sigmas 2 `
  --map-pool D:\Projects\ti4-engine\data\map_pools\save52_e400_n8192.json.gz `
  --out out\stage2_rust.json
```

Bootstrap Stage 2 from a prior Stage-1 or Stage-2 checkpoint:

```powershell
.\target\release\examples\stage2_training.exe `
  --checkpoint out\stage1_rust.json `
  --updates 1000 --every 25 --train-seeds 16 --rounds 4 `
  --validation-seeds 32 --confirmation-seeds 32 --accept-sigmas 2 `
  --map-pool D:\Projects\ti4-engine\data\map_pools\save52_e400_n8192.json.gz `
  --out out\stage2_from_stage1.json
```

`--checkpoint` and `--out` must be different files. The runner records the bootstrap's SHA-256
in `resumed_from` and refuses to overwrite that evidence. To continue a Stage-2 run, use its last
checkpoint as input and a new segment filename as output.

Factions absent from the bootstrap start with blank schema-4 profiles. Present profiles are
validated for explicit schema and faction identity. The checkpoint's completed update advances the
training seed schedule; resumed updates do not replay earlier games.

`--rounds` controls both training and evaluation. A Stage-2 checkpoint inherits its recorded
horizon when the flag is omitted; an explicit flag deliberately starts a new horizon segment.
Stage 2 refuses horizons below two rounds. The step guard scales with the selected horizon.

### Learner versus champion

Stage 2 keeps two policy tables:

- `profiles` is the active learner and receives every policy-gradient update;
- `accepted` is the deployable champion and changes only after promotion.

At every `--every` boundary the complete learner table is measured on the fixed validation panel.
It must add more than `--accept-vp-margin` mean VP per faction (default `0.05`), while no faction
may lose more than `--max-faction-vp-regression` VP (default `0.15`) or
`--max-faction-clearance-regression` opening clearance (default `0.03`). A passing table is tested
again on the disjoint confirmation panel. If the assembled table fails, each learner faction is
tested in isolation against the accepted opponents, preserving Python's useful single-faction
promotion path.

The aggregate gain must also exceed `--accept-sigmas` standard errors (default `2.0`). This is a
paired estimate: candidate-minus-champion table VP is computed for each source seed, with all six
physical-seat rotations kept inside that seed's sample. It removes shared map/deal luck without
pretending correlated rotations are independent. `0` restores the fixed-margin-only gate.

Validation and confirmation default to 32 source seeds each. Every source seed includes all six
physical-seat rotations, so each faction is measured in 192 games per panel. Reducing these values
is suitable for smoke tests, not promotion evidence.

### Audit records

`history` is append-only across Stage-2 resumes. Each new entry records the update, candidate and
accepted metrics, any assembled confirmation panel, promoted factions, promotion kind, and elapsed
time. It also records aggregate VP gain, paired standard error, and source-seed sample count for
validation and confirmation. `training_telemetry` retains one row per training block with:

- decisions and rollout errors;
- zero-movement update count;
- per-faction update-norm movement; and
- mean and maximum return standard deviation.

This separates “the program is busy” from “the reward varies and weights move.” Checkpoints retain
all earlier Stage-2 history rather than replacing it with the latest evaluation.

Stage-2 reports, by faction:

- mean victory points;
- VP margin against the best opponent and win-or-tie frequency;
- currently scoreable public plus secret objectives;
- planets gained since setup;
- absolute systems controlled; and
- units gained since setup;
- opening shortfall and clearance.

The Python design reserves an optional faction-specific meta teacher for Stage 2, but no scoring
contract or validated artifact was produced. The Rust runner therefore records `meta teacher: none`
and does not silently convert the old authored tactical preferences into training labels. Adding a
teacher requires a versioned artifact, a reward-only integration contract, and paired validation;
Stage 1 remains teacher-free.

`Archive::save` writes checkpoints atomically and emits a SHA-256 companion file. A process failure
cannot replace the last good checkpoint with a partially serialized document.

## Determinism and verification

The test suite proves:

- one-worker and 32-worker Save-54 panels produce identical ordered rollouts;
- worker-side Stage-1 statistics equal the historical parent-side reducer exactly;
- worker-side four-round Stage-2 statistics equal the parent-side reducer exactly;
- faction Stage-2 stop/resume equals uninterrupted training in memory; and
- Stage-2 promotion requires aggregate VP gain and enforces per-faction VP/clearance vetoes;
- checkpoints retain evaluation history, learning telemetry, bootstrap checksum, and separate
  learner/champion profiles; and
- the Save-52 Stage-2 runner loads the real pool, rotates all six factions, trains, evaluates, and
  writes a checksummed checkpoint.

Relevant commands:

```powershell
cargo test -p ti4-training
cargo test -p ti4-sim
cargo clippy -p ti4-policy --lib -- -D warnings
cargo clippy -p ti4-training --lib --examples -- -D warnings
```

## Measured performance

All Stage-1 figures use 16 seeds x 3 rotations and schema-4 named profiles on the same machine.

| Path | Seconds/update | Average equivalent cores |
|---|---:|---:|
| Original Rust rotated loop | ~1.02 | ~1 |
| First threaded Rust loop | ~0.41 | ~4.8 |
| Shared profiles + Rayon + worker reduction | **~0.091** | **~16.8** |
| Optimized Python historical control | 0.556 | ~60.7% machine CPU |

The optimized Rust path is approximately 6.1 times faster per update than the recorded optimized
Python control. The final 200-update sustained benchmark used 306.5 CPU-seconds over 18.2 wall-
seconds, peaked at 36 process threads, and produced no stderr.

CPU saturation is not itself the objective: update dependencies leave the final policy update and
checkpoint serialization serial. Increasing `--train-seeds` creates more parallel games but also
changes the optimizer batch. Preserve the reference batch when comparing learning curves.

The next major throughput ceiling is the named `BTreeMap<String, f64>` feature representation.
Interning stable numeric feature IDs could reduce allocation and comparison cost, but it requires a
versioned checkpoint mapping and solved-profile transfer proof; it is intentionally not hidden
inside this execution change.
