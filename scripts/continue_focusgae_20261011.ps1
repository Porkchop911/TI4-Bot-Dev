[CmdletBinding()]
param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')
$taskRoot = Split-Path -Parent $PSScriptRoot
$settings = Import-PowerShellDataFile (Join-Path $PSScriptRoot 'resume_focusgae_20261011.psd1')
$run = [string]$settings.Run
$trainer = Join-Path $taskRoot 'target\release\examples\ppo_update.exe'
$evaluator = Join-Path $taskRoot 'target\release\examples\clearance_eval.exe'
$baseline = Join-Path $taskRoot 'out\ppo-cardtag-T-wide-pilot50-20261009\checkpoints\checkpoint-3584'
$holdout = Join-Path $taskRoot 'out\pools\full_np8_12_holdout.json'
$expectedTrainer = 'B3DF6353A4760296CD2478211A8845A28BFC67144FA4815E615B0318C4B987E3'
$expectedEvaluator = '69BCDE77A0CA3BCA9488FB21329F9776BD2FA93FEB271F667BCB720FC7DDF839'
$compiledCommit = '8432ba712bd2635575bb1eafe22835bdd9a0642e'
$python = 'C:\Python314\python.exe'
$analysisScript = Join-Path $PSScriptRoot 'paired_vp_bootstrap_20261011.py'
foreach ($path in @($settings.Bundle, $settings.Pool, $settings.LibTorch, $baseline, $holdout, $trainer, $evaluator, $python, $analysisScript)) {
    if (-not (Test-Path -LiteralPath $path)) { throw "Required input missing: $path" }
}
if ((Get-FileHash -LiteralPath $trainer).Hash -ne $expectedTrainer) { throw 'Trainer binary changed; revalidate provenance.' }
if ((Get-FileHash -LiteralPath $evaluator).Hash -ne $expectedEvaluator) { throw 'Evaluator binary changed; revalidate provenance.' }
$analysisHash = (Get-FileHash -LiteralPath $analysisScript).Hash
$manifest = Get-Content (Join-Path $settings.Bundle 'manifest.json') -Raw | ConvertFrom-Json
if ($manifest.schema -ne 12 -or $manifest.source -notmatch '200 update\(s\)') { throw 'Unexpected warm-start checkpoint.' }
foreach ($entry in $manifest.checksums.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath (Join-Path $settings.Bundle $entry.Name)).Hash -ne $entry.Value) { throw "Checkpoint checksum mismatch: $($entry.Name)" }
}
if (Test-Path -LiteralPath $run) { throw "Output already exists: $run" }
if (@(Get-Process ppo_update,clearance_eval -ErrorAction SilentlyContinue).Count -gt 0) { throw 'A trainer/evaluator is already active.' }
if ($settings.Flags.updates -ne 800 -or $settings.Flags.'seed-base' -ne 1264300100 -or $settings.Flags.'faction-focus' -ne 1 -or $settings.Flags.'gae-lambda' -ne 0.95) { throw 'Continuation settings changed.' }
$trainArgs = @('--bundle', $settings.Bundle, '--map-pool', $settings.Pool, '--out', (Join-Path $run 'checkpoints'), '--diplomacy')
foreach ($name in ($settings.Flags.Keys | Sort-Object)) { $trainArgs += @("--$name", [string]$settings.Flags[$name]) }
if ($CheckOnly) {
    Write-Output 'PASS: paths, checkpoint checksums, binary fingerprints, settings and exclusive process preflight.'
    Write-Output "Warm start: $($settings.Bundle); 800 updates; 16 rollout workers; fresh output $run"
    exit 0
}
New-Item -ItemType Directory -Path $run | Out-Null
$env:LIBTORCH = $settings.LibTorch
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($settings.LibTorch)\lib;$env:PATH"
$env:RAYON_NUM_THREADS = '16'
$env:OMP_NUM_THREADS = '1'
$env:MKL_NUM_THREADS = '1'
# Preserve the reused binary's recorded source revision; HEAD is separately recorded, not claimed compiled.
$env:GIT_COMMIT = $compiledCommit
$state = [ordered]@{ phase='preflight'; controller_pid=$PID; child_pid=$null; started_at=(Get-Date).ToString('o'); updated_at=$null; error=$null }
function Save-State {
    $state.updated_at = (Get-Date).ToString('o')
    $state | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $run 'status.json') -Encoding UTF8
}
function Run-Bounded([string]$exe, [string[]]$nativeArgs, [string]$phase, [int]$minutes) {
    $state.phase = $phase
    $child = Start-Process -FilePath $exe -ArgumentList $nativeArgs -WorkingDirectory $taskRoot -WindowStyle Hidden -RedirectStandardOutput (Join-Path $run "$phase.stdout.log") -RedirectStandardError (Join-Path $run "$phase.stderr.log") -PassThru
    $state.child_pid = $child.Id
    Save-State
    $deadline = (Get-Date).AddMinutes($minutes)
    while (-not $child.WaitForExit(30000)) {
        if ((Get-Date) -gt $deadline) { Stop-Process -InputObject $child; $child.WaitForExit(); $state.child_pid = $null; Save-State; throw "$phase exceeded its $minutes minute bound; owned child stopped." }
        Save-State
    }
    $child.WaitForExit()
    $exitCode = $child.ExitCode
    $state.child_pid = $null
    Save-State
    if ($exitCode -ne 0) { throw "$phase exited $exitCode; see $phase.stderr.log." }
}
$launch = [ordered]@{ config=(Join-Path $PSScriptRoot 'resume_focusgae_20261011.psd1'); checkout_head=(& git -C $taskRoot rev-parse HEAD); compiled_source_commit=$compiledCommit; compiled_from_dirty_tree=$true; trainer_sha256=$expectedTrainer; evaluator_sha256=$expectedEvaluator; analysis_sha256=$analysisHash; warm_start=$settings.Bundle; resets=@('optimizer moments','faction-focus smoothing','local update index'); discarded_unsaved_updates=29; workers=16; arguments=$trainArgs; timeout_minutes=480; eval_seeds=600; eval_seed_base=900000000; controller=$PSCommandPath }
$launch | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $run 'launch.json') -Encoding UTF8
try {
    # One GPU update without publishing. Separate seeds; runtime loaders validate pools and bundle contracts.
    $smokeArgs = @('--bundle', $settings.Bundle, '--map-pool', $settings.Pool, '--diplomacy', '--no-checkpoint')
    foreach ($name in ($settings.Flags.Keys | Sort-Object)) {
        $value = [string]$settings.Flags[$name]
        if ($name -eq 'updates') { $value = '1' }
        if ($name -eq 'seeds-per-update') { $value = '2' }
        if ($name -eq 'seed-base') { $value = '1264300000' }
        $smokeArgs += @("--$name", $value)
    }
    Run-Bounded $trainer $smokeArgs 'smoke' 10
    Run-Bounded $trainer $trainArgs 'training' 480
    $complete = @(Get-ChildItem -LiteralPath (Join-Path $run 'checkpoints') -Directory | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'manifest.json') } | ForEach-Object {
        $m = Get-Content (Join-Path $_.FullName 'manifest.json') -Raw | ConvertFrom-Json
        [pscustomobject]@{ path=$_.FullName; update=[long]$m.update; source=[string]$m.source }
    } | Sort-Object update)
    if ($complete.Count -eq 0 -or $complete[-1].source -notmatch '800 update\(s\)') { throw 'No final complete 800-update checkpoint; evaluation refused.' }
    $final = $complete[-1].path
    $finalManifest = Get-Content (Join-Path $final 'manifest.json') -Raw | ConvertFrom-Json
    foreach ($entry in $finalManifest.checksums.PSObject.Properties) {
        if ((Get-FileHash -LiteralPath (Join-Path $final $entry.Name)).Hash -ne $entry.Value) { throw "Final checkpoint checksum mismatch: $($entry.Name)" }
    }
    $state['final_checkpoint'] = $final
    foreach ($pair in @(@('baseline', $baseline), @('final', $final))) {
        $evalArgs = @('--bundle', $pair[1], '--map-pool', $holdout, '--roster', 'wide', '--seeds', '600', '--seed-base', '900000000', '--rounds', '4', '--temperature', '0.001', '--diplomacy', '--per-seat', (Join-Path $run "$($pair[0]).per-seat.tsv"))
        Run-Bounded $evaluator $evalArgs "eval-$($pair[0])" 120
    }
    if ((Get-FileHash -LiteralPath $analysisScript).Hash -ne $analysisHash) { throw 'Paired-analysis script changed after launch; report refused.' }
    Run-Bounded $python @('-B', $analysisScript, (Join-Path $run 'baseline.per-seat.tsv'), (Join-Path $run 'final.per-seat.tsv'), '--expected-eval') 'paired-summary' 10
    $state.phase = 'complete'
    Save-State
} catch {
    $state.phase = 'failed'
    $state.error = $_.Exception.Message
    Save-State
    Write-Error $_
    exit 1
}



