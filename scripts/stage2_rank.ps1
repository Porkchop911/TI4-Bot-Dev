# Rank stage-2 checkpoints by cross-play margin, cheaply, before spending clearance runs on them.
#
# Cross-play at 20 seeds is 720 candidate seat-games and takes seconds; clearance at 300 seeds is
# 10,800 and takes minutes. Under an 85% floor almost everything passes, so the floor is not what
# discriminates -- margin is. Rank on margin first, then pay for clearance only on the leaders.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Checkpoints,
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$Benchmark = 'out/champions/best-94.97_r2-epoch22',
    [int]$CrossplaySeeds = 20,
    [int]$Rounds = 4,
    [int]$ClearanceSeeds = 300,
    [double]$ClearanceFloor = 85.0,
    [int]$Finalists = 4
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"

$out = Join-Path $root "out\rank-$Tag"
# Cleared, not reused. A killed evaluator leaves a file holding only its header, and a stale
# one from an earlier attempt is indistinguishable from a fresh result by name alone.
if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null
$crossplayExe = Join-Path $root 'target\release\examples\crossplay_eval.exe'
$clearanceExe = Join-Path $root 'target\release\examples\clearance_eval.exe'

$dirs = Get-ChildItem (Join-Path $root ($Checkpoints -replace '/', '\')) -Directory |
    Sort-Object { [int]($_.Name -replace '\D', '') }

Write-Host "ranking $($dirs.Count) checkpoints by cross-play margin (null is about -0.150)"
Write-Host ''
Write-Host 'checkpoint               VP    margin     win    waste  declined'
$rows = @()
foreach ($d in $dirs) {
    $c = $d.Name
    $file = Join-Path $out "$c.crossplay.txt"
    & $crossplayExe --bundle "$Checkpoints/$c" --opponent $Benchmark --seeds $CrossplaySeeds --rounds $Rounds *> $file
    $line = Select-String -Path $file -Pattern '^  ALL\s' | Select-Object -First 1
    if (-not $line) { Write-Host ("{0,-18} NO RESULT (evaluator did not finish)" -f $c); continue }
    $f = ($line.Line -split '\s+') | Where-Object { $_ }
    $row = [pscustomobject]@{
        Checkpoint = $c; VP = [double]$f[2]; Margin = [double]$f[3]
        Win = [double]($f[4] -replace '%', ''); Waste = [double]($f[6] -replace '%', '')
        Declined = $f[8]
    }
    $rows += $row
    Write-Host ("{0,-18} {1,8:N3} {2,9:+0.000;-0.000} {3,6:N1}% {4,7:N2}%  {5,8}" -f `
            $row.Checkpoint, $row.VP, $row.Margin, $row.Win, $row.Waste, $row.Declined)
}

Write-Host ''
Write-Host "=== clearance for the top $Finalists by margin (floor $ClearanceFloor%) ==="
$top = $rows | Sort-Object Margin -Descending | Select-Object -First $Finalists
foreach ($row in $top) {
    $file = Join-Path $out "$($row.Checkpoint).clearance.txt"
    & $clearanceExe --bundle "$Checkpoints/$($row.Checkpoint)" --temperature 0.001 --seeds $ClearanceSeeds *> $file
    $line = Select-String -Path $file -Pattern '^  table\s' | Select-Object -First 1
    $clearance = [double]((($line.Line -split '\s+') | Where-Object { $_ })[2] -replace '%', '')
    $verdict = if ($clearance -ge $ClearanceFloor) { 'pass' } else { 'REJECT' }
    Write-Host ("{0,-18} clearance {1,7:N2}%  margin {2,8:+0.000;-0.000}  {3}" -f `
            $row.Checkpoint, $clearance, $row.Margin, $verdict)
    $row | Add-Member -NotePropertyName Clearance -NotePropertyValue $clearance -Force
    $row | Add-Member -NotePropertyName Passes -NotePropertyValue ($clearance -ge $ClearanceFloor) -Force
}

$winner = $top | Where-Object { $_.Passes } | Sort-Object Margin -Descending | Select-Object -First 1
if (-not $winner) { Write-Host ''; Write-Host 'nothing cleared the floor'; exit 1 }
Write-Host ''
Write-Host "WINNER $($winner.Checkpoint)"
# Written so an unattended chain can start its next run from the winner without a human reading
# this log and retyping a path.
"$Checkpoints/$($winner.Checkpoint)" | Out-File -FilePath (Join-Path $out 'winner.txt') -Encoding ascii
Write-Host ''
Write-Host 'per-faction clearance:'
Get-Content (Join-Path $out "$($winner.Checkpoint).clearance.txt") |
    Select-String '^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|table)\s'
Write-Host ''
Write-Host 'per-faction cross-play:'
Get-Content (Join-Path $out "$($winner.Checkpoint).crossplay.txt") |
    Select-String 'faction     games|^  (sol|letnev|xxcha|hacan|jolnar|l1z1x|ALL)\s'
