# PPO training: practical guide

Use the launcher. Edit one configuration file; do not construct a raw `ppo_update.exe` command.

## Start a run

```powershell
Set-Location 'C:\Users\Niko\Documents\ChatGPT\ti4-engine-rs\diplomacy-worktree'
Copy-Item .\scripts\ppo_run.example.psd1 .\scripts\my_ppo_run.psd1
notepad .\scripts\my_ppo_run.psd1
```

In `my_ppo_run.psd1`, set these paths:

```powershell
Bundle = 'D:\path\to\checkpoint'
Pool = 'D:\Projects\ti4-engine-rs\out\pools\full_np8_12_train.json'
Run = 'D:\Projects\ti4-engine-rs\out\my-new-run-name'
LibTorch = 'D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128'
```

All training knobs are ordinary `name = value` entries under `Flags`. There are no backticks,
escaped underscores, or multi-line command continuations to get wrong.

Preview the launch without building, creating directories, or starting training:

```powershell
.\scripts\ppo_train.ps1 -Config .\scripts\my_ppo_run.psd1 -DryRun
```

Start it:

```powershell
.\scripts\ppo_train.ps1 -Config .\scripts\my_ppo_run.psd1
```

The launcher builds the correct executable, validates the bundle and diplomacy head, refuses stale
or conflicting paths, records `launch.json`, and starts the trainer in the background.

## Configuration structure

Top-level settings control launching:

| Setting | Meaning |
|---|---|
| `Bundle` | Input checkpoint directory containing `manifest.json`. |
| `Pool` | Authenticated training map pool. |
| `Run` | New output directory. It must not exist yet. |
| `LibTorch` | CUDA libtorch directory. |
| `Diplomacy` | `$true` enables diplomacy. Requires schema 9/10 and 15 heads. |
| `Background` | `$true` writes logs and returns; `$false` stays in the terminal. |
| `Build` | Build this worktree's release trainer. Keep `$true` normally. |
| `AllowConcurrent` | Permit another PPO process. Keep `$false` unless intentional. |

`Flags` contains training settings:

```powershell
Flags = @{
    'stage' = 2
    'rounds' = 4
    'temperature' = 1.0
    'learning-rate' = '1e-4'
    'movement-entropy' = 0.05
    'entropy-final' = 0.25
    'waste-penalties' = '15,12,5,5,8,8'
    'updates' = 1000
    'report-every' = '1:1,25:250,100'
    'seed-base' = 1261000000
    'device' = 'cuda'
}
```

## Settings that matter most

| Setting | Practical choice | Effect |
|---|---:|---|
| `stage` | `2` | Train four-round VP play. Stage `1` trains the opening. |
| `rounds` | `4` | Game horizon. Stage 1 normally uses `1`. |
| `temperature` | `1.0` | Higher is more random and also weakens the logit gradient. |
| `learning-rate` | `1e-4` to `3e-4` | Adam update size. Start at `1e-4` for a new experiment. |
| `movement-entropy` | `0.05` | Extra exploration for movement choices. |
| `entropy-final` | `0.25` | Final entropy multiplier. `1` disables annealing. |
| `updates` | `1` smoke; `500–1000` run | One update is 96 games: 16 seeds × 6 rotations. |
| `seed-base` | unique integer | First seed. Each update consumes 16 seeds. |
| `report-every` | `1:1,25:250,100` | Checkpoint at 1, every 25 through 250, then every 100. |

### Waste penalty

Choose one form:

```powershell
'waste-penalty' = 10
```

or faction-specific values:

```powershell
'waste-penalties' = '15,12,5,5,8,8'
```

Plural order: `Sol, Letnev, Xxcha, Hacan, Jol-Nar, L1Z1X`. Do not set both forms.

## Reward, in one place

Default Stage-2 progress potential:

```text
1.00 × victory points
+ 0.35 × scoreable public objectives
+ 0.25 × scoreable secret objectives
```

Round one additionally receives:

```text
0.10 × change in opening potential
+ 3.0 × (opening cleared − 0.1 × opening shortfall)
```

Future returns are undiscounted (`gamma = 1`).

Main controls:

| Flag | Default | Prices |
|---|---:|---|
| `vp-weight` | `1.0` | Victory points. |
| `objective-weight` | `0.35` | Scoreable public objectives; must be below VP weight. |
| `secret-weight` | `0.25` | Scoreable secrets; must be below VP weight. |
| `r1-bonus` | `3.0` | Opening clearance and shortfall. |
| `r1-shaping` | `0.1` | Opening progress during round one. |
| `clearance-weight` | `0` | Failing the opening bar. |
| `waste-penalty(s)` | `0` | Wasted tactical activations. |

Optional terms are off by default:

| Flag | Prices |
|---|---|
| `fleet-weight` | Fleet resource value. |
| `tech-weight` | Technologies gained. |
| `high-vp-bonus` | Finishing with at least 3 VP. |
| `strategy-diversity-weight` | Strategy-card monoculture above 80%. |
| `fleet-hoard-penalty` | Ending with at least 7 fleet tokens. |
| `zero-fleet-penalty` | First transition to zero fleet tokens. |
| `trade-goods-hoard-weight` | Trade goods above 7 at round end. |
| `styx-bonus` | Controlling Styx at the horizon. |
| `fracture-entry-bonus` | First Fracture entry. |
| `fracture-planet-bonus` | Fracture planets controlled at the horizon. |

There is no direct diplomacy reward. Diplomacy learns only through its effect on the ordinary game
return.

### Avoid reward domination

Compare every bonus with `vp-weight`. `styx-bonus = 16` and `vp-weight = 1.1` makes Styx worth about
14.5 victory points. Start optional terms small and inspect behavior before increasing them.

## Smoke test

Make a separate config with a fresh `Run` path and change:

```powershell
Background = $false

# Under Flags:
'updates' = 1
'rounds' = 2
'report-every' = 1
```

A healthy test prints:

```text
parameters  moved
adam state  advanced
```

## Monitor and stop

The run directory contains:

```text
launch.json    exact source and arguments
pid.txt        background process ID
stdout.log     progress and checkpoints
stderr.log     errors
checkpoints/   model bundles
```

Follow progress:

```powershell
$run = 'D:\Projects\ti4-engine-rs\out\my-run'
Get-Content "$run\stdout.log" -Wait
```

Check errors:

```powershell
Get-Content "$run\stderr.log" -Tail 50
```

Stop:

```powershell
$pidValue = [int](Get-Content "$run\pid.txt")
Stop-Process -Id $pidValue
```

A forced stop loses work since the last checkpoint.

## Checkpoint meaning

A checkpoint contains weights, vocabulary, head layout, critic mode, checksums, and manifest.
Starting from it is a **warm start**, not an exact resume: Adam moments, optimizer history, entropy
progress, and the previous seed cursor are not restored.

Give every continuation a new `Run` directory and seed base. Checkpoint directory numbers count
optimizer minibatch steps, not PPO updates.

## Common failures

### `unknown argument "--diplomacy"`

You used an executable from another checkout. Use `scripts/ppo_train.ps1`; it builds and runs this
worktree's executable.

### `Bundle has no manifest.json`

The path is not a completed checkpoint. Never use a `.tmp` directory.

### PowerShell treats `--vp-weight` as an operator

A raw multi-line command ended. Use the config launcher; flags are data, not PowerShell commands.

### Run directory already exists

Choose a new name. Runs are never overwritten.

### Another trainer is running

```powershell
Get-Process ppo_update -ErrorAction SilentlyContinue
```

Avoid concurrent trainers on one GPU. `AllowConcurrent = $true` is an explicit override.

## Advanced flags

Most runs do not need these:

| Flag | Default | Purpose |
|---|---:|---|
| `expansion-weight` | `2` | Stage-1 planet and system progress. |
| `unit-weight` | `1` | Stage-1 capacity-ship and infantry progress. |
| `conjunctive-weight` | `0` | Balanced planet/system progress. |
| `clear-bonus` | `22` | Stage-1 terminal clear bonus. |
| `curriculum-seeds` | none | File with exactly `updates × 16` seeds. |
| `demo-corpus` | none | Clean trajectory corpus directory. |
| `demo-per-update` | `0` | Demonstrations per update; divisible by 24. |
| `diag` | none | Per-update JSON diagnostics. |
| `capture-batch` | none | Save the first frozen PPO batch. |

The launcher accepts every value-taking PPO flag through `Flags`. Rare boolean diagnostic flags
remain raw-driver tools.

## Compare experiments

For A/B testing: use the same source bundle, change one setting, use fresh training seeds, and
evaluate both outputs on the same held-out seeds and seat rotations. Run replicates before treating
a small difference as real. Do not select the final checkpoint automatically; PPO often peaks
earlier.
