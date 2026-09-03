# Confirm each candidate on a seed range it was not selected on, one invocation per candidate.
#
# Not a -Candidates array: `powershell -File` binds a comma-separated argument as ONE string, so a
# four-bundle list arrived as a single path and the evaluator refused it for having no manifest.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

$names = @(
    'stage2-r3-m2.494_clear93.32',
    'stage2-r4-m2.587_clear93.22',
    'stage2-r5-m2.526_clear93.75',
    'stage2-r4b-m2.364_clear93.40'
)
$seeds = 24
$seedBase = 900000100
$out = Join-Path $root 'out\confirm'
if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null
$exe = Join-Path $root 'target\release\examples\crossplay_eval.exe'

Write-Host "confirmation on seeds $seedBase.. ($seeds seeds = $($seeds * 36) seat-games each)"
Write-Host '  null margin is about -0.150; the stage-1 champion reads -0.178'
Write-Host ''
foreach ($n in $names) {
    $file = Join-Path $out "$n.txt"
    $argv = @('--bundle', "out/champions/$n", '--opponent', 'out/champions/best-94.97_r2-epoch22',
        '--seeds', $seeds, '--rounds', '4', '--max-steps', '8000', '--seed-base', $seedBase)
    $proc = Start-Process -FilePath $exe -ArgumentList $argv -NoNewWindow -PassThru `
        -RedirectStandardOutput $file -RedirectStandardError "$file.err"
    if (-not $proc.WaitForExit(1500000)) {
        $proc.Kill(); $proc.WaitForExit()
        Write-Host ("{0,-32} TIMED OUT" -f $n)
        continue
    }
    $line = Select-String -Path $file -Pattern '^  ALL\s' | Select-Object -First 1
    if ($line) {
        Write-Host ("{0,-32} {1}" -f $n, (($line.Line) -replace '\s+', ' ').Trim())
    } else {
        Write-Host ("{0,-32} NO RESULT" -f $n)
    }
}
Write-Host ''
Write-Host '=== per faction ==='
foreach ($n in $names) {
    $file = Join-Path $out "$n.txt"
    if (-not (Test-Path -LiteralPath $file)) { continue }
    Write-Host ''
    Write-Host "--- $n ---"
    Get-Content $file | Select-String 'faction     games|^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|ALL)\s'
}
