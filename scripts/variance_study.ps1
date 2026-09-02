# How much does one PPO arm vary between runs?
#
# The waste sweep produced a per-faction result -- Xxcha 90.47% -> 99.22% under a penalty -- that a
# later arm contradicted with 80.67%. An 18-point spread against a +-1.0 measurement interval is
# run-to-run variance, not measurement error, and it is larger than any effect the sweep claimed.
#
# So: the SAME arm, several times, differing ONLY in the rollout seed base. Everything else --
# starting policy, temperature, learning rate, penalties, update count -- is held. Whatever spread
# comes out is the noise floor that every single-run comparison in this project has been read
# against, per faction and on the table.
#
#   ./scripts/variance_study.ps1
#   ./scripts/variance_study.ps1 -Replicates 3 -Penalty 8

[CmdletBinding()]
param(
    [int]$Replicates = 3,
    [double]$Penalty = 8,
    [int]$Updates = 500,
    [string]$From = "out/champions/table-best-93.88_mixed-epoch14",
    [int]$EvalSeeds = 600
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($commit)) {
    throw 'cannot read the current commit'
}
$env:GIT_COMMIT = $commit

$train = Join-Path $root 'target\release\examples\ppo_update.exe'
$eval = Join-Path $root 'target\release\examples\clearance_eval.exe'
foreach ($exe in @($train, $eval)) { if (-not (Test-Path $exe)) { throw "$exe is missing" } }

$out = Join-Path $root 'out\variance'
if (-not (Test-Path $out)) { New-Item -ItemType Directory -Path $out | Out-Null }

Write-Host "variance study at $commit"
Write-Host "  from        $From"
Write-Host "  penalty     $Penalty (uniform)"
Write-Host "  replicates  $Replicates x $Updates updates, differing only in seed base"
Write-Host ''

for ($r = 1; $r -le $Replicates; $r++) {
    $checkpoints = "out/checkpoints/var-r$r"
    $log = Join-Path $out "r$r.log"
    $native = Join-Path $root ($checkpoints -replace '/', '\')
    if (Test-Path $native) { Remove-Item -Recurse -Force $native }

    # The one thing that differs. Far enough apart that no two replicates share a seed.
    $seedBase = 650000000 + ($r - 1) * 100000000

    Write-Host "=== replicate $r (seeds $seedBase) ==="
    $started = Get-Date
    $argv = @(
        '--bundle', $From,
        '--stage', '1',
        '--rounds', '1',
        '--temperature', '2.5',
        '--movement-entropy', '0.05',
        '--entropy-final', '1',
        '--learning-rate', '3e-4',
        '--waste-penalty', "$Penalty",
        '--seed-base', "$seedBase",
        '--updates', "$Updates",
        '--report-every', '1000',
        '--device', 'cuda',
        '--out', $checkpoints
    )
    & $train @argv *> $log
    if ($LASTEXITCODE -ne 0) { throw "replicate $r failed; see $log" }

    $last = Get-ChildItem $native -Directory |
        Sort-Object { [int]($_.Name -replace '\D', '') } |
        Select-Object -Last 1
    Write-Host "  trained in $([int]((Get-Date) - $started).TotalMinutes) min"
    & $eval --bundle "$checkpoints/$($last.Name)" --temperature 0.001 --seeds $EvalSeeds |
        Select-String "^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|table)\s"
    Write-Host ''
}

Write-Host 'done. The spread across replicates is the noise floor for every single-run comparison.'
