[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Config,
    [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$configPath = (Resolve-Path -LiteralPath $Config).Path
$settings = Import-PowerShellDataFile -LiteralPath $configPath

foreach ($required in @('Bundle', 'Pool', 'Run', 'LibTorch', 'Flags')) {
    if (-not $settings.ContainsKey($required)) {
        throw "Config is missing required key '$required'."
    }
}

$bundle = [string]$settings.Bundle
$pool = [string]$settings.Pool
$run = [string]$settings.Run
$libtorch = [string]$settings.LibTorch
$flags = $settings.Flags

foreach ($path in @($bundle, $pool, $libtorch)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Required path does not exist: $path"
    }
}

$manifestPath = Join-Path $bundle 'manifest.json'
if (-not (Test-Path -LiteralPath $manifestPath)) {
    throw "Bundle has no manifest.json: $bundle"
}
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json

$diplomacy = $settings.ContainsKey('Diplomacy') -and [bool]$settings.Diplomacy
if ($diplomacy) {
    if ($manifest.schema -notin @(9, 10)) {
        throw "Diplomacy requires bundle schema 9 or 10; this bundle is schema $($manifest.schema)."
    }
    if ($manifest.heads -notcontains 'diplomacy') {
        throw 'Diplomacy requires a bundle whose manifest contains the diplomacy head.'
    }
}

$allowed = @(
    'clear-bonus', 'clearance-weight', 'conjunctive-weight', 'curriculum-seeds',
    'demo-corpus', 'demo-per-update', 'device', 'entropy-final', 'expansion-weight',
    'fleet-hoard-penalty', 'fleet-weight', 'fracture-entry-bonus', 'fracture-planet-bonus',
    'high-vp-bonus', 'learning-rate', 'movement-entropy', 'objective-weight', 'r1-bonus',
    'r1-shaping', 'report-every', 'rounds', 'secret-weight', 'seed-base', 'stage',
    'strategy-diversity-weight', 'styx-bonus', 'tech-weight', 'temperature',
    'opponent', 'trade-goods-hoard-weight', 'unit-weight', 'updates', 'vp-weight',
    'waste-penalties',
    'waste-penalty', 'zero-fleet-penalty', 'diag', 'capture-batch'
)

$unknown = @($flags.Keys | Where-Object { $_ -notin $allowed })
if ($unknown.Count -gt 0) {
    throw "Unknown PPO flag key(s): $($unknown -join ', ')"
}
if ($flags.ContainsKey('waste-penalty') -and $flags.ContainsKey('waste-penalties')) {
    throw "Choose either 'waste-penalty' or 'waste-penalties', not both."
}

$arguments = @('--bundle', $bundle, '--map-pool', $pool, '--out', (Join-Path $run 'checkpoints'))
if ($diplomacy) {
    $arguments += '--diplomacy'
}
foreach ($name in ($flags.Keys | Sort-Object)) {
    $arguments += "--$name"
    $arguments += [string]$flags[$name]
}

$exe = Join-Path $root 'target\release\examples\ppo_update.exe'
$head = (& git -C $root rev-parse HEAD).Trim()
if ($head -notmatch '^[0-9a-fA-F]{7,64}$') {
    throw "git rev-parse returned an invalid hexadecimal commit: $head"
}

Write-Host 'PPO launch plan'
Write-Host "  config      $configPath"
Write-Host "  repository  $root"
Write-Host "  executable  $exe"
Write-Host "  bundle      $bundle (schema $($manifest.schema), $($manifest.heads.Count) heads)"
Write-Host "  pool        $pool"
Write-Host "  output      $run"
Write-Host "  diplomacy   $diplomacy"
Write-Host "  commit      $head"
Write-Host "  command     $exe $($arguments -join ' ')"

if ($DryRun) {
    Write-Host 'DRY RUN: nothing was built, created, or started.'
    exit 0
}

$allowConcurrent = $settings.ContainsKey('AllowConcurrent') -and [bool]$settings.AllowConcurrent
$existing = @(Get-Process ppo_update -ErrorAction SilentlyContinue)
if ($existing.Count -gt 0 -and -not $allowConcurrent) {
    throw "A PPO trainer is already running (PID(s): $($existing.Id -join ', ')). Stop it or set AllowConcurrent = `$true deliberately."
}
if (Test-Path -LiteralPath $run) {
    throw "Run directory already exists: $run"
}

$build = -not $settings.ContainsKey('Build') -or [bool]$settings.Build
if ($build) {
    $env:LIBTORCH = $libtorch
    $env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
    $env:PATH = "$libtorch\lib;$env:PATH"
    & cargo build --release -p ti4-mlp --example ppo_update --manifest-path (Join-Path $root 'Cargo.toml')
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
}
if (-not (Test-Path -LiteralPath $exe)) {
    throw "PPO executable does not exist after preflight: $exe"
}

New-Item -ItemType Directory -Path $run | Out-Null
$env:LIBTORCH = $libtorch
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$libtorch\lib;$env:PATH"
$env:GIT_COMMIT = $head

$record = [ordered]@{
    config = $configPath
    started_at = (Get-Date).ToString('o')
    git_commit = $head
    dirty = -not [string]::IsNullOrWhiteSpace((& git -C $root status --short | Out-String))
    executable = $exe
    arguments = $arguments
}
$record | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $run 'launch.json')

$background = $settings.ContainsKey('Background') -and [bool]$settings.Background
if ($background) {
    $process = Start-Process `
        -FilePath $exe `
        -ArgumentList $arguments `
        -WorkingDirectory $root `
        -RedirectStandardOutput (Join-Path $run 'stdout.log') `
        -RedirectStandardError (Join-Path $run 'stderr.log') `
        -WindowStyle Hidden `
        -PassThru
    $process.Id | Set-Content -LiteralPath (Join-Path $run 'pid.txt')
    Write-Host "Started background PPO process PID $($process.Id)."
    Write-Host "Follow it with: Get-Content '$run\stdout.log' -Wait"
} else {
    & $exe @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "PPO exited with code $LASTEXITCODE"
    }
}
