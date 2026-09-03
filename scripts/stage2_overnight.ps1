# Unattended stage-2 campaign.
#
# Each leg trains from the previous leg's GATED winner, not from its last checkpoint: stage-2 runs
# peak early and then erode both margin and clearance, so the last checkpoint is reliably not the
# best one. The gate is margin, subject to a clearance floor, and clearance is a floor rather than
# a term to maximise because round-one quality is a human prior that SUPPORTS scoring rather than
# a rival objective to trade against it.
#
# Leg 3 is not a training run. It re-runs leg 1's recipe with a different rollout seed base and
# nothing else changed, to size the training noise floor -- without it, every comparison between
# legs is unfounded. Stage 1 measured a 1.54-point table floor this way, after hours of comparing
# single runs to single runs, and stage 2 has longer games and more shaping terms so the floor is
# expected to be larger, not smaller.

[CmdletBinding()]
param(
    # Rank an already-trained checkpoint directory first and start the campaign from its winner.
    # Given instead of -From, so the night does not begin by waiting on a human to read a ranking.
    [string]$RankFirst,
    [string]$RankFirstTag = 'r3',
    [string]$From,
    # Short legs, sampled densely. Stage-2 runs peak within tens of updates and then degrade, and
    # the degradation is not only in margin: later checkpoints accumulate board state until a single
    # game cannot be played in bounded time and cannot be evaluated at all. Three of run 3's first
    # four checkpoints timed out where the earliest took three seconds. Training longer produces
    # unevaluable policies, so the budget goes into resolution over the window that works.
    [int]$Updates = 150,
    [string]$WastePenalties = '15,12,5,5,8,8',   # sol, letnev, xxcha, hacan, jolnar, l1z1x
    [double]$ClearanceFloor = 85.0,
    [int]$MaxSteps = 8000,
    [int]$TimeoutSeconds = 240,
    [switch]$SkipR4,
    [string]$ReplicateFrom
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"
$env:GIT_COMMIT = (& git rev-parse HEAD).Trim()

$train = Join-Path $root 'target\release\examples\ppo_update.exe'
$rank = Join-Path $root 'scripts\stage2_rank.ps1'

function Invoke-Leg {
    param([string]$Tag, [string]$Start, [int]$SeedBase, [string]$Penalties)

    Write-Host ''
    Write-Host "################ leg $Tag ################"
    Write-Host "  from   $Start"
    Write-Host "  seeds  $SeedBase"
    Write-Host "  waste  $Penalties"
    Write-Host "  began  $(Get-Date -Format 'HH:mm')"

    $checkpoints = "out/checkpoints/stage2-$Tag"
    $native = Join-Path $root ($checkpoints -replace '/', '\')
    if (Test-Path -LiteralPath $native) { Remove-Item -LiteralPath $native -Recurse -Force }

    & $train --bundle $Start --stage 2 --rounds 4 `
        --temperature 2.5 --movement-entropy 0.05 --entropy-final 1 `
        --learning-rate 3e-4 --waste-penalties $Penalties `
        --vp-weight 4.0 --objective-weight 1.4 --secret-weight 1.0 --r1-bonus 2.0 `
        --updates $Updates --report-every 10 --seed-base $SeedBase `
        --device cuda --out $checkpoints *> (Join-Path $root "out/stage2_$Tag.log")
    if ($LASTEXITCODE -ne 0) { Write-Host "leg $Tag FAILED to train"; return $null }

    & powershell -NoProfile -ExecutionPolicy Bypass -File $rank `
        -Checkpoints $checkpoints -Tag $Tag -ClearanceFloor $ClearanceFloor -MaxSteps $MaxSteps -TimeoutSeconds $TimeoutSeconds 2>&1 |
        Tee-Object -FilePath (Join-Path $root "out\rank_$Tag.log") | Out-Host

    $winnerFile = Join-Path $root "out\rank-$Tag\winner.txt"
    if (-not (Test-Path -LiteralPath $winnerFile)) { Write-Host "leg $Tag produced no winner"; return $null }
    $winner = (Get-Content $winnerFile -Raw).Trim()
    Write-Host "leg $Tag winner: $winner  ($(Get-Date -Format 'HH:mm'))"
    # `return` hands back everything left in the success stream, not just this value. The
    # ranking pipeline above therefore has to go to the host explicitly or its whole
    # transcript is concatenated onto the winner path -- which is what happened: leg r5 was
    # handed a bundle beginning "ranking 15 checkpoints by cross-play margin" and refused.
    return $winner
}

Write-Host "stage-2 overnight campaign at $($env:GIT_COMMIT)"
Write-Host "  clearance floor $ClearanceFloor%"
Write-Host "  began           $(Get-Date -Format 'yyyy-MM-dd HH:mm')"

if ($RankFirst) {
    Write-Host ''
    Write-Host "################ ranking $RankFirst first ################"
    & powershell -NoProfile -ExecutionPolicy Bypass -File $rank `
        -Checkpoints $RankFirst -Tag $RankFirstTag -ClearanceFloor $ClearanceFloor -MaxSteps $MaxSteps -TimeoutSeconds $TimeoutSeconds 2>&1 |
        Tee-Object -FilePath (Join-Path $root "out/rank_$RankFirstTag.log")
    $winnerFile = Join-Path $root "out/rank-$RankFirstTag/winner.txt"
    if (Test-Path -LiteralPath $winnerFile) {
        $From = (Get-Content $winnerFile -Raw).Trim()
        Write-Host "starting the campaign from $From"
    } else {
        Write-Host 'ranking produced no winner; falling back to -From'
    }
}
if (-not $From) { Write-Host 'no starting bundle'; exit 1 }
Write-Host "  starting from   $From"

# -SkipR4 resumes a campaign whose first leg already finished, so a failure later in the night does
# not cost the legs that succeeded. $From is then r4's winner rather than r3's.
if ($SkipR4) {
    Write-Host "skipping leg r4; resuming from $From"
    $leg1 = $From
} else {
    $leg1 = Invoke-Leg -Tag 'r4' -Start $From -SeedBase 950000000 -Penalties $WastePenalties
    if (-not $leg1) { Write-Host 'campaign stopped: leg r4 produced nothing'; exit 1 }
}

$leg2 = Invoke-Leg -Tag 'r5' -Start $leg1 -SeedBase 1050000000 -Penalties $WastePenalties
if (-not $leg2) { Write-Host 'campaign stopped after r4'; $leg2 = $leg1 }

# The replicate. Same recipe and same start as leg 1, different rollout seeds only.
# The replicate must start where leg r4 started, which is not $From once the campaign resumed.
$replicateStart = if ($ReplicateFrom) { $ReplicateFrom } else { $From }
$replicate = Invoke-Leg -Tag 'r4b' -Start $replicateStart -SeedBase 1150000000 -Penalties $WastePenalties

Write-Host ''
Write-Host '################ campaign summary ################'
foreach ($tag in @($RankFirstTag, 'r4', 'r5', 'r4b')) {
    $log = Join-Path $root "out\rank_$tag.log"
    if (-not (Test-Path -LiteralPath $log)) { continue }
    Write-Host ''
    Write-Host "--- $tag ---"
    Get-Content $log | Select-String 'WINNER|clearance .*margin|^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|table|ALL)\s|faction     games'
}
Write-Host ''
Write-Host 'r4 against r4b is the noise floor. Any r4-to-r5 difference smaller than that gap is'
Write-Host 'not a result, it is the same recipe landing somewhere else.'
