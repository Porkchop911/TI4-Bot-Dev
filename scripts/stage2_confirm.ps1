# Confirm a set of candidates against the benchmark on a seed range none of them was SELECTED on.
#
# Each leg's winner is the maximum of about fifteen noisy checkpoints, so its reported margin is
# upward biased by the selection itself. Re-measuring on fresh seeds removes that bias: the ranking
# chose on one sample, this reports on another.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string[]]$Candidates,
    [string]$Benchmark = 'out/champions/best-94.97_r2-epoch22',
    [int]$Seeds = 30,
    [int]$SeedBase = 1300000000,
    [int]$Rounds = 4,
    [int]$MaxSteps = 8000,
    [int]$TimeoutSeconds = 1800
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

$out = Join-Path $root 'out\confirm'
if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null
$exe = Join-Path $root 'target\release\examples\crossplay_eval.exe'

Write-Host "confirmation on seeds $SeedBase.. ($Seeds seeds = $($Seeds * 36) seat-games each)"
Write-Host "  benchmark $Benchmark    null margin about -0.150"
Write-Host ''
foreach ($c in $Candidates) {
    $name = Split-Path $c -Leaf
    $file = Join-Path $out "$name.txt"
    $argv = @('--bundle', $c, '--opponent', $Benchmark, '--seeds', $Seeds,
        '--rounds', $Rounds, '--max-steps', $MaxSteps, '--seed-base', $SeedBase)
    $proc = Start-Process -FilePath $exe -ArgumentList $argv -NoNewWindow -PassThru `
        -RedirectStandardOutput $file -RedirectStandardError "$file.err"
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        $proc.Kill(); $proc.WaitForExit()
        Write-Host ("{0,-34} TIMED OUT" -f $name)
        continue
    }
    $line = Select-String -Path $file -Pattern '^  ALL\s' | Select-Object -First 1
    if (-not $line) { Write-Host ("{0,-34} NO RESULT" -f $name); continue }
    Write-Host ("{0,-34} {1}" -f $name, (($line.Line) -replace '\s+', ' ').Trim())
}
Write-Host ''
Write-Host 'per-faction, each candidate:'
foreach ($c in $Candidates) {
    $name = Split-Path $c -Leaf
    $file = Join-Path $out "$name.txt"
    if (-not (Test-Path -LiteralPath $file)) { continue }
    Write-Host ''
    Write-Host "--- $name ---"
    Get-Content $file | Select-String 'faction     games|^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|ALL)\s'
}
