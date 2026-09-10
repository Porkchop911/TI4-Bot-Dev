# Play a TTS table against the trained MLP.
#
#   ./scripts/play_mlp.ps1                     # one turn, DRY RUN -- prints, sends nothing
#   ./scripts/play_mlp.ps1 -Go                 # one turn, actually played onto the table
#   ./scripts/play_mlp.ps1 -Go -Turns 3        # three turns
#   ./scripts/play_mlp.ps1 -Go -Hotseat Purple # you play Purple physically, bots take the rest
#   ./scripts/play_mlp.ps1 -Seats hacan,sol    # only these seats use the MLP; the rest stay
#                                              # on the Python bots
#
# Before any of this: the bridge must be running and TTS must have been pointed at it.
#
#   python bridge/run_bridge.py --capture out/bridge-captures
#   ...then in TTS chat:  !gamedata localhost
#
# A queued command is a physical change to a live table and undoing one by hand is slower than
# checking it, so the default is a dry run and -Go is deliberately a separate keystroke.

[CmdletBinding()]
param(
    [switch]$Go,
    [int]$Turns = 1,
    [string]$Seats = 'all',
    [string]$Hotseat,
    [string]$Play,
    [switch]$ReadHands,
    [string]$Bundle = 'out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428',
    [string]$Temperature = '0.001'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$serve = Join-Path $root 'target\release\examples\mlp_serve.exe'
if (-not (Test-Path $serve)) {
    Write-Host "the policy service is not built. Run:" -ForegroundColor Yellow
    Write-Host "  cargo build --release -p ti4-mlp --example mlp_serve"
    exit 1
}

# The service links libtorch, and the staged DLLs live beside the release binaries.
$env:PATH = "$root\target\release;$root\out\libtorch-2.9.1-cpu\lib;$env:PATH"
# Never write caches into the pinned Python repository.
$env:PYTHONDONTWRITEBYTECODE = '1'

try {
    $health = Invoke-WebRequest -Uri 'http://127.0.0.1:8080/' -UseBasicParsing -TimeoutSec 3
    $uploads = (ConvertFrom-Json $health.Content).uploads
    Write-Host "bridge is up, $uploads upload(s) received" -ForegroundColor Green
} catch {
    Write-Host "no bridge on 127.0.0.1:8080. Start one first:" -ForegroundColor Yellow
    Write-Host "  python bridge/run_bridge.py --capture out/bridge-captures"
    Write-Host "then in TTS chat:  !gamedata localhost"
    exit 1
}

# Is anything actually collecting commands? The mod's telemetry upload and the executor's command
# poll are separate channels: uploads can be arriving while nothing drains the queue, and then a
# turn is queued into a void and fires all at once whenever the executor wakes up. A ping proves
# the channel end to end, and costs one no-op command.
if ($Go) {
    $before = (ConvertFrom-Json (Invoke-WebRequest -Uri 'http://127.0.0.1:8080/' -UseBasicParsing -TimeoutSec 4).Content).pending
    $body = '{"action":"ping","message":"bridge liveness check"}'
    Invoke-WebRequest -Uri 'http://127.0.0.1:8080/queue' -Method POST -Body $body -ContentType 'application/json' -UseBasicParsing -TimeoutSec 4 | Out-Null
    # A failed request yields $null, and $null -le anything is TRUE in PowerShell -- so an
    # unreachable bridge read as "drained" and the guard reported LIVE while nothing was there.
    # Only a successful, numeric reading counts.
    $drained = $false
    foreach ($attempt in 1..6) {
        Start-Sleep -Seconds 1
        try {
            $now = (ConvertFrom-Json (Invoke-WebRequest -Uri 'http://127.0.0.1:8080/' -UseBasicParsing -TimeoutSec 4).Content).pending
        } catch {
            Write-Host "  bridge became unreachable during the check" -ForegroundColor Yellow
            $now = $null
        }
        if ($null -ne $now -and $now -is [int] -and $now -le $before) { $drained = $true; break }
    }
    if (-not $drained) {
        $stuck = (ConvertFrom-Json (Invoke-WebRequest -Uri 'http://127.0.0.1:8080/' -UseBasicParsing -TimeoutSec 4).Content).pending
        Write-Host "the executor is not collecting commands ($stuck queued and undrained)." -ForegroundColor Red
        Write-Host "Telemetry is arriving, so the mod is connected -- but the command channel is not."
        Write-Host "The executor bootstraps its poll from a response, and the mod suppresses uploads"
        Write-Host "whose CRC matches the last one, so a still table never triggers one."
        Write-Host ""
        Write-Host "Move any piece in TTS (or re-run !gamedata localhost), then try again."
        Write-Host "Nothing was queued for this turn."
        exit 1
    }
    Write-Host "command channel is live" -ForegroundColor Green
}

$arguments = @(
    'bridge/play_with_mlp.py',
    '--seats', $Seats,
    '--turns', [string]$Turns,
    '--bundle', $Bundle,
    '--temperature', $Temperature
)
if (-not $Go)      { $arguments += '--dry-run' }
if ($Hotseat)      { $arguments += @('--hotseat', $Hotseat) }
if ($Play)         { $arguments += @('--play', $Play) }
if ($ReadHands)    { $arguments += '--read-hands' }

if ($Go) {
    Write-Host "PLAYING FOR REAL onto the table ($Turns turn(s), seats: $Seats)" -ForegroundColor Red
} else {
    Write-Host "dry run -- nothing will be sent. Add -Go to play it." -ForegroundColor Cyan
}
Write-Host ''

# The policy service writes its readiness line and any refusals to stderr on purpose, so they are
# seen where they happen. Under `Stop`, PowerShell turns a native command's stderr into a
# terminating NativeCommandError and kills the run before a single decision is made.
$ErrorActionPreference = 'Continue'
python @arguments
exit $LASTEXITCODE
