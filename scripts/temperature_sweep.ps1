# Run the PPO temperature sweep described in plans/EXP_TEMPERATURE_SWEEP.md.
#
# Every arm differs from every other in exactly two values -- temperature and learning rate -- and
# in nothing else. That is the whole design, so the shared arguments are built once, below, and no
# arm may add to them.
#
# Each arm is measured by clearance_eval at a fixed 0.25 on the Validation pool, whatever it trained
# at. The in-run clearance table is training data sampled at the training temperature and is not
# comparable across arms; see the experiment document.
#
#   ./scripts/temperature_sweep.ps1                     # every arm, in order
#   ./scripts/temperature_sweep.ps1 -Arms A-100         # the control alone (run this first)
#   ./scripts/temperature_sweep.ps1 -Arms A-025,C-025   # a pair
#   ./scripts/temperature_sweep.ps1 -EvalOnly           # re-measure existing checkpoints

[CmdletBinding()]
param(
    # Which arms to run. Defaults to all of them, control first.
    [string[]]$Arms = @('A-100', 'A-025', 'A-050', 'A-150', 'A-250', 'C-025', 'C-250'),

    # Skip training and only re-measure whatever checkpoints already exist.
    [switch]$EvalOnly,

    # Seeds per evaluation point. 400 x 6 rotations x 6 seats = 14,400 seat-games, about +-0.5pp.
    [int]$EvalSeeds = 400,

    # Measure every Nth checkpoint. One checkpoint is written per 10 updates, so a stride of 5 is a
    # point per 50 updates -- fine enough to see where an arm plateaus, which is the only shape this
    # experiment reads off the curve. The final checkpoint is always measured whatever the stride.
    [int]$EvalStride = 5,

    # Updates per arm. The pilot reached its final level by update 200 and spent the next 700 inside
    # its own confidence interval (83.19 at 200, 84.17 at 300, 84.30 at 500, 83.91 at 900, all
    # +-0.6), so 500 is comfortably past the plateau with room for an arm that travels more slowly
    # than the pilot did -- A-250 most of all, at 0.4x the gradient per update.
    [int]$Updates = 500,

    # Independent runs per arm, differing **only** in the rollout seed stream.
    #
    # One run per arm gives a between-arm difference with no idea what a within-arm difference looks
    # like. The clearance_eval half-width measures sampling error in the *evaluation* and says
    # nothing about run-to-run variation in the training, which is the larger of the two and the one
    # that decides whether a two-point gap between arms means anything.
    #
    # Replicate r shifts the training seed base by r * 100000000, far enough that no two replicates
    # share a seed. Everything else -- checkpoint, temperature, learning rate, update count, and the
    # evaluation seeds -- is identical.
    [int]$Replicates = 1
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

# --- the arms -------------------------------------------------------------------------------
#
# Learning rate for a compensated arm is 3e-4 * T, cancelling the 1/T that dividing the logits by T
# introduces into the gradient. See the experiment document; the cancellation is approximate because
# Adam normalises by a running second moment.
$definitions = [ordered]@{
    'A-100' = @{ Temperature = 1.0;  LearningRate = 3e-4;   Note = 'control: the historical default' }
    'A-025' = @{ Temperature = 0.25; LearningRate = 3e-4;   Note = 'near-greedy, as an operator gets it' }
    'A-050' = @{ Temperature = 0.5;  LearningRate = 3e-4;   Note = 'is the effect monotone below 1' }
    'A-150' = @{ Temperature = 1.5;  LearningRate = 3e-4;   Note = 'mildly hot' }
    'A-250' = @{ Temperature = 2.5;  LearningRate = 3e-4;   Note = 'the search temperature, used to train' }
    'C-025' = @{ Temperature = 0.25; LearningRate = 7.5e-5; Note = 'A-025 with the gradient scaling cancelled' }
    'C-250' = @{ Temperature = 2.5;  LearningRate = 7.5e-4; Note = 'A-250 with the gradient scaling cancelled' }
}

foreach ($arm in $Arms) {
    if (-not $definitions.Contains($arm)) {
        throw "unknown arm '$arm'. Known arms: $($definitions.Keys -join ', ')"
    }
}

# --- environment ----------------------------------------------------------------------------

$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

# A checkpoint manifest without a commit cannot be traced back to the code that produced it, and a
# sweep whose arms cannot be attributed to a commit is not evidence. ppo_update already fails closed
# on this; failing here says so before three hours are spent.
$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($commit)) {
    throw 'cannot read the current commit; refusing to run an untraceable sweep'
}
$dirty = (& git status --porcelain -- crates plans scripts)
if ($dirty) {
    Write-Warning "the tree is dirty; checkpoints will claim $commit but were not built from it exactly:"
    $dirty | ForEach-Object { Write-Warning "  $_" }
}
$env:GIT_COMMIT = $commit

$train = Join-Path $root 'target\release\examples\ppo_update.exe'
$eval  = Join-Path $root 'target\release\examples\clearance_eval.exe'
foreach ($exe in @($train, $eval)) {
    if (-not (Test-Path $exe)) {
        throw "$exe is missing. Build with: cargo build --release -p ti4-mlp --example ppo_update --example clearance_eval"
    }
}

$sweep = Join-Path $root 'out\sweep'
if (-not (Test-Path $sweep)) { New-Item -ItemType Directory -Path $sweep | Out-Null }

# Fixed for every arm and every replicate. Nothing here may vary between them.
# --seed-base is deliberately absent: it is the one thing a replicate changes, added per run below.
$bundle = 'out/checkpoints/run-028/checkpoint-60672'
$shared = @(
    '--bundle', $bundle,
    '--stage', '1',
    '--rounds', '1',
    '--movement-entropy', '0.05',
    '--entropy-final', '1',
    '--updates', "$Updates",
    '--report-every', '10',
    '--device', 'cuda'
)

# --- measurement ----------------------------------------------------------------------------

function Measure-Arm {
    param([string]$Arm, [string]$CheckpointDir)

    $report = Join-Path $sweep "$Arm.eval"
    "arm $Arm -- measured at temperature 0.25 on the Validation pool, $EvalSeeds seeds" |
        Set-Content -Path $report -Encoding utf8

    # The starting bundle, so every arm's curve begins from a measured point rather than an assumed
    # one. It is the same policy in every arm, so the seven readings should agree; if they do not,
    # the instrument is not deterministic and nothing below it means anything.
    $points = @([pscustomobject]@{ Name = 'start'; Path = $bundle })
    if (Test-Path $CheckpointDir) {
        $all = @(Get-ChildItem -Path $CheckpointDir -Directory |
            Sort-Object { [int]($_.Name -replace '\D', '') })
        for ($i = 0; $i -lt $all.Count; $i++) {
            # Every stride-th, and always the last: the endpoint is the number every comparison in
            # the experiment document is stated against.
            if ((($i + 1) % $EvalStride) -ne 0 -and $i -ne ($all.Count - 1)) { continue }
            $points += [pscustomobject]@{ Name = $all[$i].Name; Path = $all[$i].FullName }
        }
    }

    foreach ($point in $points) {
        Write-Host "  measuring $Arm / $($point.Name)"
        $output = & $eval --bundle $point.Path --temperature 0.25 --seeds $EvalSeeds --seed-base 900000000
        if ($LASTEXITCODE -ne 0) {
            throw "clearance_eval failed on $($point.Path)"
        }
        Add-Content -Path $report -Value '' -Encoding utf8
        Add-Content -Path $report -Value "===== $($point.Name) =====" -Encoding utf8
        Add-Content -Path $report -Value $output -Encoding utf8

        $table = $output | Where-Object { $_ -match '^\s+table\s' }
        if ($table) { Write-Host "    $($table.Trim())" }
    }
    Write-Host "  wrote $report"
}

# --- run ------------------------------------------------------------------------------------

Write-Host "temperature sweep at $commit"
Write-Host "  arms       $($Arms -join ', ')"
Write-Host "  start      $bundle"
Write-Host "  updates    $Updates per arm x $Replicates replicate(s)"
Write-Host "  measured   temperature 0.25, $EvalSeeds seeds, Validation pool, every $EvalStride checkpoints"
Write-Host ''

foreach ($arm in $Arms) {
    $spec = $definitions[$arm]

    for ($rep = 1; $rep -le $Replicates; $rep++) {
        # A single replicate keeps the bare arm name, so a one-replicate sweep reads the way the
        # experiment document describes it rather than littering every path with "-r1".
        $label = if ($Replicates -eq 1) { $arm } else { "$arm-r$rep" }
        $out = "out/checkpoints/sweep-$label"
        $log = Join-Path $sweep "$label.log"
        $seedBase = 650000000 + ($rep - 1) * 100000000

        if (-not $EvalOnly) {
            Write-Host "=== $label : temperature $($spec.Temperature), lr $($spec.LearningRate), seeds $seedBase -- $($spec.Note) ==="
            $started = Get-Date
            $argv = $shared + @(
                '--temperature', "$($spec.Temperature)",
                '--learning-rate', "$($spec.LearningRate)",
                '--seed-base', "$seedBase",
                '--out', $out
            )
            & $train @argv *> $log
            if ($LASTEXITCODE -ne 0) {
                throw "$label failed; see $log"
            }
            Write-Host "  trained in $([int]((Get-Date) - $started).TotalMinutes) min -> $log"
        }

        Measure-Arm -Arm $label -CheckpointDir (Join-Path $root ($out -replace '/', ''))
        Write-Host ''
    }
}

Write-Host 'done. Record the findings under ## Results in plans/EXP_TEMPERATURE_SWEEP.md.'
