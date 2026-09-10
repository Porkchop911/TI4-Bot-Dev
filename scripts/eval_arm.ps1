# Evaluate one experiment arm's published checkpoints at the acceptance convention.
#
# Three measurements per checkpoint, each to its own named log kept as evidence:
#   clearance  greedy, Validation pool, 600 seeds x 6 rotations  (clearance_eval)
#   waste      greedy, Train pool, per-faction clear/waste/tactical/joint (build_positive_corpus)
#   drift      reference KL and greedy flips against checkpoint-59540 (demo_benchmark)
#
# Sequential on purpose: these are CPU-only (inference is CPU by design, §7.1) and run beside a
# CUDA training job, so parallelising them would starve its rollouts.
#
#   ./scripts/eval_arm.ps1 -RunDir out/checkpoints/hybrid-guided-control-s1 -Arm control -Suffix s1

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RunDir,
    [Parameter(Mandatory = $true)][string]$Arm,
    [Parameter(Mandatory = $true)][string]$Suffix,
    [int[]]$Updates = @(),
    [int]$Seeds = 600,
    [int]$PerCorpus = 400,
    [string]$Reference = 'out/checkpoints/blank-waste-mine-p5/checkpoint-59540',
    [string]$Corpus = 'out/corpus/positive-hybrid-p5-multitemp'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"
$env:GIT_COMMIT = (& git rev-parse HEAD).Trim()

# Update number comes from each manifest, never from the directory name: the directory number is
# an Adam-step count and the two do not correspond.
$checkpoints = Get-ChildItem $RunDir -Directory | ForEach-Object {
    $m = Join-Path $_.FullName 'manifest.json'
    if (Test-Path $m) {
        $j = Get-Content $m -Raw | ConvertFrom-Json
        if ($j.source -match '(\d+) update\(s\)') {
            [pscustomobject]@{ Path = $_.FullName; Update = [int]$Matches[1] }
        }
    }
} | Sort-Object Update

if ($Updates.Count -gt 0) {
    $checkpoints = $checkpoints | Where-Object { $Updates -contains $_.Update }
}
if (-not $checkpoints) { throw "no checkpoints matched under $RunDir" }

Write-Host "evaluating $Arm-$Suffix at $($env:GIT_COMMIT)"
Write-Host "  updates $(($checkpoints | ForEach-Object { $_.Update }) -join ', ')"
Write-Host ''

foreach ($cp in $checkpoints) {
    $u = $cp.Update
    $tag = "$Arm-$Suffix-u$u"
    Write-Host "=== $tag ($($cp.Path)) ==="

    $clearLog = "out/eval-$tag-clearance.log"
    if (Test-Path $clearLog) {
        Write-Host "  clearance  skipped, $clearLog exists"
    } else {
        & (Join-Path $root 'target\release\examples\clearance_eval.exe') `
            --bundle $cp.Path --temperature 0.001 --seeds $Seeds *> $clearLog
        if ($LASTEXITCODE -ne 0) { throw "clearance_eval failed for $tag; see $clearLog" }
        $line = Select-String -Path $clearLog -Pattern '^\s+table' | Select-Object -First 1
        Write-Host "  clearance  $($line.Line.Trim())"
    }

    $wasteLog = "out/eval-$tag-waste.log"
    $wasteOut = "out/corpus/eval-$tag-waste"
    if (Test-Path $wasteLog) {
        Write-Host "  waste      skipped, $wasteLog exists"
    } else {
        & (Join-Path $root 'target\release\examples\build_positive_corpus.exe') `
            --bundle $cp.Path --seeds $Seeds --temperatures "0.001" --out $wasteOut *> $wasteLog
        if ($LASTEXITCODE -ne 0) { throw "build_positive_corpus failed for $tag; see $wasteLog" }
        $line = Select-String -Path $wasteLog -Pattern 'TABLE' | Select-Object -First 1
        Write-Host "  waste      $($line.Line.Trim())"
    }

    $driftLog = "out/eval-$tag-drift.log"
    if (Test-Path $driftLog) {
        Write-Host "  drift      skipped, $driftLog exists"
    } else {
        & (Join-Path $root 'target\release\examples\demo_benchmark.exe') `
            --bundle $cp.Path `
            --replay-bundle $Reference `
            --reference-bundle $Reference `
            --corpus $Corpus `
            --rescued out/corpus/does-not-exist `
            --per-corpus $PerCorpus *> $driftLog
        if ($LASTEXITCODE -ne 0) { throw "demo_benchmark failed for $tag; see $driftLog" }
        $line = Select-String -Path $driftLog -Pattern '^\s*ALL' | Select-Object -First 1
        Write-Host "  drift      $($line.Line.Trim())"
    }
    Write-Host ''
}

Write-Host "done. logs: out/eval-$Arm-$Suffix-u*.{clearance,waste,drift}.log"
