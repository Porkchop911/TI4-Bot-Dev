# Pick a stage-2 checkpoint: VP is the objective, clearance is a floor, waste is charged outright.
#
# The ordering is not a preference. Victory points are the point of the game; round-one clearance
# and the absence of empty activations are human priors about what SUPPORTS scoring over a longer
# game, so neither is traded against points. Clearance is therefore a floor to clear, not a term to
# maximise, and among everything that clears it the best margin wins.
#
# Margin, not VP: the candidate holds one seat against five frozen copies of a fixed benchmark, and
# its margin null is NEGATIVE (about -0.15) because it is one draw against the maximum of five. A
# margin is read against that null, never against zero.
#
#   ./scripts/stage2_gate.ps1 -Checkpoints out/checkpoints/stage2-r3 -Tag r3

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Checkpoints,
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$Benchmark = 'out/champions/best-94.97_r2-epoch22',
    [double]$ClearanceFloor = 85.0,
    [int]$ClearanceSeeds = 300,
    [int]$CrossplaySeeds = 20,
    [int]$Rounds = 4
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

$out = Join-Path $root "out\gate-$Tag"
if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null

$clearanceExe = Join-Path $root 'target\release\examples\clearance_eval.exe'
$crossplayExe = Join-Path $root 'target\release\examples\crossplay_eval.exe'

$dirs = Get-ChildItem (Join-Path $root ($Checkpoints -replace '/', '\')) -Directory |
    Sort-Object { [int]($_.Name -replace '\D', '') }

Write-Host "stage-2 gate over $($dirs.Count) checkpoints"
Write-Host "  benchmark        $Benchmark"
Write-Host "  clearance floor  $ClearanceFloor%"
Write-Host ''
Write-Host 'checkpoint          clearance      VP   margin     win    waste   gate'

$rows = @()
foreach ($d in $dirs) {
    $c = $d.Name
    $bundle = "$Checkpoints/$c"

    # Full per-faction output is kept for every checkpoint, not just the table line: the stage-1
    # work repeatedly found the table average hiding a large gain and several losses.
    # The redirection target must sit on the same line as `*>`; a line break after the operator is
    # a parse error, not a continuation.
    $clearanceFile = Join-Path $out "$c.clearance.txt"
    $crossplayFile = Join-Path $out "$c.crossplay.txt"
    & $clearanceExe --bundle $bundle --temperature 0.001 --seeds $ClearanceSeeds *> $clearanceFile
    & $crossplayExe --bundle $bundle --opponent $Benchmark --seeds $CrossplaySeeds --rounds $Rounds *> $crossplayFile

    $clLine = Select-String -Path $clearanceFile -Pattern '^  table\s' | Select-Object -First 1
    $cpLine = Select-String -Path $crossplayFile -Pattern '^  ALL\s' | Select-Object -First 1
    if (-not $clLine -or -not $cpLine) { Write-Host "  $c  MEASUREMENT FAILED"; continue }

    $cl = ($clLine.Line -split '\s+') | Where-Object { $_ }
    $cp = ($cpLine.Line -split '\s+') | Where-Object { $_ }
    $clearance = [double]($cl[2] -replace '%', '')
    $vp = [double]$cp[2]
    $margin = [double]$cp[3]
    $win = [double]($cp[4] -replace '%', '')
    $waste = [double]($cp[6] -replace '%', '')
    $passes = $clearance -ge $ClearanceFloor

    $rows += [pscustomobject]@{
        Checkpoint = $c; Clearance = $clearance; VP = $vp
        Margin = $margin; Win = $win; Waste = $waste; Passes = $passes
    }
    $verdict = if ($passes) { 'pass' } else { 'REJECT' }
    # `{n,+8:N3}` is not valid composite formatting -- alignment must be a plain integer. The sign
    # comes from a positive;negative custom format instead, which matters here because a margin is
    # read against a negative null and the sign has to be visible.
    Write-Host ("{0,-18} {1,8:N2}% {2,7:N3} {3,8:+0.000;-0.000} {4,6:N1}% {5,7:N2}%   {6}" -f `
            $c, $clearance, $vp, $margin, $win, $waste, $verdict)
}

Write-Host ''
$survivors = $rows | Where-Object { $_.Passes }
if (-not $survivors) {
    Write-Host "NOTHING CLEARED THE $ClearanceFloor% FLOOR. The run is not usable as it stands."
    exit 1
}
$best = $survivors | Sort-Object Margin -Descending | Select-Object -First 1
Write-Host ("winner: {0} -- clearance {1:N2}%, margin {2:+0.000;-0.000}, win {3:N1}%, waste {4:N2}%" -f `
        $best.Checkpoint, $best.Clearance, $best.Margin, $best.Win, $best.Waste)
Write-Host ''
Write-Host 'per faction, the winner:'
Get-Content (Join-Path $out "$($best.Checkpoint).clearance.txt") |
    Select-String '^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|table)\s'
Write-Host ''
Get-Content (Join-Path $out "$($best.Checkpoint).crossplay.txt") |
    Select-String 'faction     games|^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|ALL)\s'
Write-Host ''
Write-Host "full per-faction output for every checkpoint is in out/gate-$Tag/"
