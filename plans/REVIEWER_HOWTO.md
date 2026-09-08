# How to run the reviewer

Watch a trained policy play, step by step, on a real map. Written 2026-09-08; the file and
checkpoint paths below were current at that date and §5 says how to re-derive them.

## 1. Build it — in release, always

```powershell
cd D:\Projects\ti4-engine-rs
$env:LIBTORCH = "D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128"
$env:LIBTORCH_BYPASS_VERSION_CHECK = "1"
$env:PATH = "$env:LIBTORCH\lib;$env:PATH"
$env:GIT_COMMIT = (& git rev-parse HEAD).Trim()

cargo build --release -p ti4-review
./target/release/ti4-review.exe
```

**`cargo run -p ti4-review` builds a debug binary.** Inference and the engine then run unoptimised,
which dominates everything else the reviewer does. Use `--release`, or run the built executable
directly as above.

The environment variables are needed because the reviewer links libtorch to run the policy. Without
`LIBTORCH` the build fails; without the `PATH` entry the binary starts and then cannot find
`c10.dll`. Use the **cu128** libtorch — mixing CPU and CUDA builds produces exactly that DLL error.

## 2. Point it at a policy and a map pool

Running with no arguments opens the native GUI. Two things must be chosen there.

### The policy (checkpoint)

Current best, and the default recommendation:

```
out\checkpoints\stage2-mlp-shaped\checkpoint-473312
```

Stage-2 shaped run, 3,600 updates, schema 7. Measured greedy at update 3,400 (`checkpoint-444224`):
92.31% ± 0.36 stage-1 clearance, 0.26% any-waste, `clear+zero` 92.35% — the best joint result the
project has recorded.

Alternatives worth loading:

| checkpoint | what it is |
|---|---|
| `out\checkpoints\stage2-mlp-shaped\checkpoint-444224` | update 3,400 — the measured one above |
| `out\checkpoints\stage2-blank-p5\checkpoint-240504` | stage-2 from blank, no shaping terms, 91.20% / 0.54% |
| `out\checkpoints\blank-waste-mine-p5\checkpoint-59540` | the stage-1 champion, 93.40% / 2.31% |

**The file picker wants a file, but the loader wants the bundle.** `load_policy`
(`ti4-review/src/lib.rs:1426-1438`) accepts either the checkpoint **directory** or the
`manifest.json` **or** `slots.json` inside it, and resolves all three to the same bundle. Picking
`slots.json` in the dialog is correct and is the normal way to do it.

Only **schema 7** bundles load. `ti4_mlp::bundle::read` refuses anything else outright
(`ti4-mlp/src/bundle.rs:50`) and there is no migration path, so every pre-rework champion is
permanently unloadable. Ignore the `out\checkpoints\timing-*` directories — they are throwaway
benchmark output, not trained policies.

### The map pool

Use the **train** pool for ordinary watching:

```
out\pools\full_np8_12_train.json
```

or the **validation** pool if you want held-out maps:

```
out\pools\full_np8_12_holdout.json
```

> **Do not use `full_np8_12_final.json` for casual review.** It is the sealed *final* pool
> (`ti4-sim/src/artifacts.rs:26-28`: "Only M10-038 may load it, once, after models and analysis are
> frozen"). The reviewer verifies that a pool is a known artifact but does **not** enforce its role,
> so it will load happily — the seal is a project rule, not a check. Watching games on it is how a
> sealed evaluation set stops being sealed, because what you have seen informs later judgement.
> `holdout` is the right choice when you want maps the policy did not train on.

Note the filename/role mismatch, which trips everyone once: `full_np8_12_holdout.json` carries the
**Validation** role, not Final. The comment in `artifacts.rs` explains why — that pool already
informed architecture and thresholds, so its logical role is validation despite its name.

## 3. Settings that matter

| setting | suggested | why |
|---|---|---|
| temperature | `0.01` (or `0.001`) | Near-greedy, so you watch the policy's actual preference. Training samples at 2.5, which looks far more random than the policy really is. |
| seed | any, e.g. `42` | With the rotation, picks the map arrangement and the whole game. |
| rotation | `0`–`5` | Which faction sits in which seat. The same seed at six rotations is six different games on one map draw. |
| table | `learner` | `accepted` is for the older per-faction profile runs, not MLP bundles. |

Temperature must be finite and greater than zero; rotation must be 0–5. Both are rejected at load
rather than clamped (`lib.rs:625-632`).

## 4. Without the GUI

Produce a session file headlessly, then render or inspect it:

```powershell
# Play a whole game to a session file
./target/release/ti4-review.exe simulate `
  --checkpoint out\checkpoints\stage2-mlp-shaped\checkpoint-473312 `
  --map-pool out\pools\full_np8_12_train.json `
  --out out\reviews\game.ti4review.json `
  --seed 42 --rotation 0 --temperature 0.01 --until end

# Check a session loads and is internally consistent
./target/release/ti4-review.exe validate out\reviews\game.ti4review.json

# Render a standalone HTML page
./target/release/ti4-review.exe render out\reviews\game.ti4review.json out\reviews\game.html
```

`--unit step|decision|action` with `--count N` advances a bounded amount instead of `--until
end|round`.

## 5. Keeping this current

Checkpoints and pools change. To re-derive rather than trust the paths above:

```powershell
# Newest schema-7 checkpoints, with their update counts
Get-ChildItem out\checkpoints -Recurse -Filter manifest.json |
  Sort-Object LastWriteTime -Descending | Select-Object -First 10 |
  ForEach-Object { "{0}`n  {1}" -f $_.Directory, (Get-Content $_.FullName -Raw | ConvertFrom-Json).source }

# Available pools
Get-ChildItem out\pools\*.json
```

A checkpoint directory holds `manifest.json`, `trunk.safetensors`, `readout.safetensors`,
`value.safetensors`, `embedding.safetensors` and `slots.json` — about 23 MB. `manifest.json` records
the schema, the training command, and the update count.

## 6. Known limits

**Sessions get large.** Every frame stores a complete `GameState` (`ti4-review/src/lib.rs:252`), so a
long review grows without bound — measured at **333 MB for 1,826 frames**, and the largest saved
sessions are 483 MB and 524 MB against a 1 GB hard cap. Autosave now backs off adaptively as the
file grows rather than firing on a fixed step interval, but the underlying size is unchanged.
Incremental persistence would fix it properly and would change the session format; not done.

**Live review runs the policy on CPU.** `ti4_tensor::inference_device()` is hard-coded to
`Device::Cpu` under §7.1, and inference is roughly 82% of a rollout. One core busy out of 32 reads
as ~3% CPU while feeling slow — that is expected, not a fault.
