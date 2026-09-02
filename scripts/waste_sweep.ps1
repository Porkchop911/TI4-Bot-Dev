# PPO with a wasted-activation penalty, swept.
#
# Every arm is identical but for --waste-penalty, and one arm charges nothing. The zero arm is not
# a formality: nothing in the stage-1 reward has ever objected to a wasted activation, so what PPO
# alone does to the rate is unknown, and without it a fall could not be attributed to the penalty.
#
# Recipe is the one that produced the champion: temperature 2.5, movement entropy 0.05, lr 3e-4.
#
#   ./scripts/waste_sweep.ps1
#   ./scripts/waste_sweep.ps1 -Penalties 0,5 -Updates 200

[CmdletBinding()]
param(
    [double[]]$Penalties = @(0, 5, 20),
    [int]$Updates = 500,
    [string]$From = "out/checkpoints/mixed/epoch-14"
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($commit)) {
    throw 'cannot read the current commit; refusing to run an untraceable sweep'
}
$env:GIT_COMMIT = $commit

$train = Join-Path $root 'target\release\examples\ppo_update.exe'
if (-not (Test-Path $train)) { throw "$train is missing" }

$out = Join-Path $root 'out\waste'
if (-not (Test-Path $out)) { New-Item -ItemType Directory -Path $out | Out-Null }

Write-Host "waste sweep at $commit"
Write-Host "  from       $From"
Write-Host "  penalties  $($Penalties -join ', ')"
Write-Host "  updates    $Updates each"
Write-Host ''

foreach ($penalty in $Penalties) {
    $tag = "p$($penalty -replace '\.', '_')"
    $checkpoints = "out/checkpoints/waste-$tag"
    $log = Join-Path $out "$tag.log"
    if (Test-Path (Join-Path $root ($checkpoints -replace '/', '\'))) {
        Remove-Item -Recurse -Force (Join-Path $root ($checkpoints -replace '/', '\'))
    }

    Write-Host "=== penalty $penalty ==="
    $started = Get-Date
    $argv = @(
        '--bundle', $From,
        '--stage', '1',
        '--rounds', '1',
        '--temperature', '2.5',
        '--movement-entropy', '0.05',
        '--entropy-final', '1',
        '--learning-rate', '3e-4',
        '--waste-penalty', "$penalty",
        '--updates', "$Updates",
        '--report-every', '50',
        '--device', 'cuda',
        '--out', $checkpoints
    )
    & $train @argv *> $log
    if ($LASTEXITCODE -ne 0) { throw "penalty $penalty failed; see $log" }
    Write-Host "  trained in $([int]((Get-Date) - $started).TotalMinutes) min -> $log"
}

Write-Host ''
Write-Host 'done. Measure each with clearance_eval and build_positive_corpus --temperatures 0.001.'
