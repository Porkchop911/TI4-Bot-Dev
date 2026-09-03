# Watch a training run and stop the moment it stops making progress.
#
# A stage-2 policy found an engine loop that runs INSIDE one Game::step, where the run-level step
# limit is never checked, so a stalled run does not error, does not time out, and does not stop --
# it simply never writes another line. Waiting for the process to exit therefore detects nothing.
# The log is the liveness signal.
#
# Exits non-zero on a stall so the caller is notified, after dumping what is needed to localise it.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [Parameter(Mandatory = $true)][string]$Log,
    [int]$StallSeconds = 300,
    [int]$PollSeconds = 30
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$lastSize = -1
$lastGrew = Get-Date
Write-Host "watching pid $ProcessId, log $Log, stall threshold ${StallSeconds}s"

while ($true) {
    Start-Sleep -Seconds $PollSeconds
    $proc = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if (-not $proc) {
        Write-Host "run finished at $(Get-Date -Format 'HH:mm')"
        exit 0
    }
    $size = if (Test-Path -LiteralPath $Log) { (Get-Item -LiteralPath $Log).Length } else { 0 }
    if ($size -ne $lastSize) {
        $lastSize = $size
        $lastGrew = Get-Date
        continue
    }
    $quiet = [int]((Get-Date) - $lastGrew).TotalSeconds
    if ($quiet -ge $StallSeconds) {
        Write-Host ''
        Write-Host "STALLED: no log growth for ${quiet}s while pid $ProcessId is still alive"
        Write-Host "  time      $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
        Write-Host "  cpu(min)  $([int]($proc.CPU / 60))"
        Write-Host "  threads   $($proc.Threads.Count)"
        Write-Host "  log tail:"
        Get-Content -LiteralPath $Log -Tail 12 | ForEach-Object { Write-Host "    $_" }
        # Burning CPU means a loop, not a deadlock or a wait on IO. That distinction decides
        # whether to go looking for a decision loop or for a lock.
        $before = $proc.CPU
        Start-Sleep -Seconds 20
        $after = (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue).CPU
        if ($null -ne $after) {
            $burn = [math]::Round(($after - $before), 1)
            Write-Host "  cpu seconds burned in the last 20s of the stall: $burn"
            if ($burn -gt 5) {
                Write-Host "  => spinning, so this is a loop rather than a deadlock"
            } else {
                Write-Host "  => idle, so this is a wait rather than a loop"
            }
        }
        exit 1
    }
}
