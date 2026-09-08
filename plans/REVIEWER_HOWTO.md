# How to run the reviewer

Watch a trained policy play, step by step, on a real map.

**The repository alone is not enough.** `/out/` is gitignored, so a fresh clone has no libtorch, no
map pools and no trained checkpoints. §1 covers what you must obtain separately and how to check you
got the right thing; §2–§4 are the actual run.

Paths below are **relative to the repository root**. Set this once per shell and everything else
follows:

```powershell
# Windows PowerShell — adjust to wherever you cloned it
$REPO = "C:\path\to\ti4-engine-rs"
cd $REPO
```

```bash
# Linux / macOS
export REPO=~/src/ti4-engine-rs
cd "$REPO"
```

---

## 1. The three things that are not in the repository

### 1a. libtorch — the tensor runtime

The policy is a PyTorch network, so the build links libtorch. The version is **pinned exactly**:
`Cargo.toml:57-59` fixes `tch = "=0.22.0"`, which is the release built against **libtorch 2.9.x**.
Newer `tch` wants libtorch 2.13 and will not work.

**For the reviewer, the CPU build is enough and is far smaller.** Inference is CPU-only by design —
`ti4_tensor::inference_device()` is hard-coded to `Device::Cpu` under §7.1 — so you only need the
CUDA build if you also intend to *train*. The CUDA tree is 4.46 GB.

The project's own copies came from pip (recorded in `plans/artifacts/libtorch-2.9.1-*.manifest.json`
as "copied once from a pip install of torch==2.9.1 into a staging directory"). Reproduce that:

```powershell
# CPU — enough to run the reviewer
python -m pip install --target .\_torchstage torch==2.9.1
Move-Item .\_torchstage\torch $REPO\out\libtorch-2.9.1-cpu
```

```powershell
# CUDA — only if you will also train
python -m pip install --target .\_torchstage torch==2.9.1+cu128 `
  --index-url https://download.pytorch.org/whl/cu128
Move-Item .\_torchstage\torch $REPO\out\libtorch-2.9.1-cu128
```

You should end up with a directory containing `lib/`, `include/`, `share/` and `licenses/`. Then,
**in every shell you build or run from**:

```powershell
$env:LIBTORCH = "$REPO\out\libtorch-2.9.1-cpu"      # or ...-cu128
$env:LIBTORCH_BYPASS_VERSION_CHECK = "1"
$env:PATH = "$env:LIBTORCH\lib;$env:PATH"
```

```bash
export LIBTORCH="$REPO/out/libtorch-2.9.1-cpu"
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH="$LIBTORCH/lib:$LD_LIBRARY_PATH"
```

Two failure modes worth recognising:

- **Build fails, cannot find libtorch** — `LIBTORCH` is unset or points at the wrong directory.
- **Build succeeds, binary dies with `c10.dll: cannot open shared object file`** (or `libc10.so`) —
  the library directory is not on `PATH`/`LD_LIBRARY_PATH`, *or* you have mixed a CPU and a CUDA
  tree. Pick one and use it consistently; mixing produces exactly this error.

To verify against the project's pinned copy, compare `lib/` file hashes with
`plans/artifacts/libtorch-2.9.1-cpu.manifest.json` (or the `cu128` one) — 55 files are pinned there
with sizes and sha256s.

### 1b. Map pools — ask for them, then verify

Pools are generated artifacts, not source. They are **verified against a durable manifest of pinned
sha256 hashes** (`crates/ti4-sim/src/artifacts.rs`), and the reviewer refuses anything not in it, so
you cannot substitute your own file.

**Get these from whoever runs the project** and place them at `out/pools/`:

| file | role | sha256 |
|---|---|---|
| `full_np8_12_train.json` | Train | `106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df` |
| `full_np8_12_holdout.json` | **Validation** | `aba33c81aa04cefb15857b8ed1d40173f6f3de5e9b6e9633a6855c1d5a4c27e5` |
| `full_np8_12_final.json` | Final — **sealed, see below** | `693253ecbcb33ac61c416110836286242be39271ecf49381a99c90acca653245` |

Check what you received:

```powershell
Get-FileHash out\pools\full_np8_12_train.json -Algorithm SHA256
```

```bash
sha256sum out/pools/full_np8_12_train.json
```

A pool *can* in principle be regenerated — `cargo run --release -p ti4-training --example
generate_pool` takes `--seed`, `--boards`, `--min`, `--max` and friends — but only a byte-identical
result passes verification, and the exact arguments used for the pools above are not recorded here.
Treat regeneration as a research task, not as setup.

> **Use `full_np8_12_train.json` for ordinary watching, or `full_np8_12_holdout.json` for maps the
> policy did not train on. Do not use `full_np8_12_final.json`.** It is the sealed final pool —
> `artifacts.rs:26-28`: *"Only M10-038 may load it, once, after models and analysis are frozen."*
> The reviewer verifies that a pool is a known artifact but does **not** enforce its role, so the
> sealed pool loads without complaint. The seal is a project rule, not a check. Watching games on
> those maps is how a sealed evaluation set stops being sealed.

Note the name/role mismatch that catches everyone once: `full_np8_12_holdout.json` carries the
**Validation** role, not Final. `artifacts.rs` explains why — that pool already informed architecture
and thresholds, so its logical role is validation despite the filename.

### 1c. A trained checkpoint — ask for one

Checkpoints are training output, roughly **23 MB per checkpoint directory**, and are not
reproducible without a multi-hour training run. Ask for one and put it under `out/checkpoints/`.

A checkpoint directory contains `manifest.json`, `slots.json`, `trunk.safetensors`,
`readout.safetensors`, `value.safetensors` and `embedding.safetensors`. `manifest.json` records the
schema, the training command and the update count — check `"schema": 7`:

```powershell
Get-Content out\checkpoints\<run>\<checkpoint-N>\manifest.json | ConvertFrom-Json |
  Select-Object schema, source
```

**Only schema 7 loads.** `ti4_mlp::bundle::read` refuses anything else outright
(`ti4-mlp/src/bundle.rs:50`) and there is no migration path, so every pre-rework champion is
permanently unloadable.

Checkpoints known-good as of 2026-09-08, in case you are told a name rather than handed a file:

| checkpoint | what it is |
|---|---|
| `stage2-mlp-shaped/checkpoint-473312` | update 3,600 — the current best, and the default recommendation |
| `stage2-mlp-shaped/checkpoint-444224` | update 3,400 — measured 92.31% clearance, 0.26% any-waste |
| `stage2-blank-p5/checkpoint-240504` | stage-2 from blank, no shaping terms — 91.20% / 0.54% |
| `blank-waste-mine-p5/checkpoint-59540` | the stage-1 champion — 93.40% / 2.31% |

**No checkpoint at all?** You can still prove the setup works. `cargo run --release -p ti4-mlp
--example blank_bundle` writes an untrained schema-7 bundle that loads and plays — badly, at random,
but it exercises the whole path. Do not mistake its play for a policy.

---

## 2. Build and launch

```powershell
cargo build --release -p ti4-review
.\target\release\ti4-review.exe
```

**Build in release.** `cargo run -p ti4-review` produces a *debug* binary; the engine and the network
then run unoptimised, and that dominates everything else the reviewer costs. Either use
`--release`, or run the built executable directly as above.

Running with no arguments opens the native GUI.

## 3. What to choose in the GUI

**The checkpoint.** The file picker wants a *file* while the loader wants a *bundle*. `load_policy`
(`ti4-review/src/lib.rs:1426-1438`) accepts the checkpoint **directory**, or `manifest.json`, or
`slots.json` inside it, and resolves all three to the same bundle. Selecting `slots.json` in the
dialog is correct, not a workaround.

**The map pool.** `out/pools/full_np8_12_train.json`, per §1b.

**Settings:**

| setting | suggested | why |
|---|---|---|
| temperature | `0.01` (or `0.001`) | Near-greedy, so you see the policy's actual preference. Training samples at 2.5, which looks far more random than the policy really is. |
| seed | any, e.g. `42` | With the rotation, selects the map arrangement and the whole game. |
| rotation | `0`–`5` | Which faction sits in which seat. One seed at six rotations is six different games on one map draw. |
| table | `learner` | `accepted` is for the older per-faction profile runs, not MLP bundles. |

Temperature must be finite and greater than zero, rotation must be 0–5; both are rejected at load
rather than clamped (`lib.rs:625-632`).

## 4. Without the GUI

```powershell
# Play a whole game into a session file
.\target\release\ti4-review.exe simulate `
  --checkpoint out\checkpoints\stage2-mlp-shaped\checkpoint-473312 `
  --map-pool out\pools\full_np8_12_train.json `
  --out out\reviews\game.ti4review.json `
  --seed 42 --rotation 0 --temperature 0.01 --until end

# Check a session loads and is internally consistent
.\target\release\ti4-review.exe validate out\reviews\game.ti4review.json

# Render a standalone HTML page
.\target\release\ti4-review.exe render out\reviews\game.ti4review.json out\reviews\game.html
```

`--unit step|decision|action` with `--count N` advances a bounded amount instead of
`--until round|end`.

## 5. Known limits

**Sessions get large.** Every frame stores a complete `GameState` (`ti4-review/src/lib.rs:252`), so a
long review grows without bound — measured at **333 MB for 1,826 frames**, with the largest saved
sessions at 483 MB and 524 MB against a 1 GB hard cap. Autosave backs off adaptively as the file
grows rather than firing on a fixed step interval, but the underlying size is unchanged. Incremental
persistence would fix it properly and would change the session format; not done.

**Live review runs the policy on CPU**, and inference is roughly 82% of a rollout. One core busy out
of many reads as low total CPU while feeling slow — that is expected, not a fault.

## 6. Keeping this current

Checkpoint names change. To re-derive rather than trust §1c:

```powershell
Get-ChildItem out\checkpoints -Recurse -Filter manifest.json |
  Sort-Object LastWriteTime -Descending | Select-Object -First 10 |
  ForEach-Object { "{0}`n  {1}" -f $_.Directory, (Get-Content $_.FullName -Raw | ConvertFrom-Json).source }

Get-ChildItem out\pools\*.json
```

Ignore any `out\checkpoints\timing-*` directories — those are throwaway benchmark output, not
trained policies.
