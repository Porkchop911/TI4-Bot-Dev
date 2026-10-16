# Reward-terms pilot 2026-10-16: train control and treatment concurrently (50 updates each from FG
# checkpoint-16448), then greedy 600-seed holdout evals of both finals and the paired bootstrap.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$main = 'D:\Projects\ti4-engine-rs'
$libtorch = "$main\out\libtorch-2.9.1-cu128"
$evaluator = "$root\target\release\examples\clearance_eval.exe"
$holdout = "$main\out\pools\full_np8_12_holdout.json"
$bootstrap = "$main\scripts\paired_vp_bootstrap_20261011.py"
$arms = @{ C = "$main\out\ppo-rewardterms-C-pilot50b-20261016"; T = "$main\out\ppo-rewardterms-T-pilot50b-20261016" }

foreach ($arm in 'C', 'T') {
    & "$PSScriptRoot\ppo_train.ps1" -Config "$PSScriptRoot\pilot_rewardterms_$arm.psd1"
}
foreach ($arm in 'C', 'T') {
    $trainerPid = [int](Get-Content "$($arms[$arm])\pid.txt")
    Wait-Process -Id $trainerPid -ErrorAction SilentlyContinue
}

$env:LIBTORCH = $libtorch
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$libtorch\lib;$env:PATH"
$env:RAYON_NUM_THREADS = '30'
$env:OMP_NUM_THREADS = '1'
$env:MKL_NUM_THREADS = '1'
$finals = @{}
foreach ($arm in 'C', 'T') {
    $final = Get-ChildItem "$($arms[$arm])\checkpoints" -Directory |
        Where-Object { Test-Path "$($_.FullName)\manifest.json" } |
        Sort-Object { [long]((Get-Content "$($_.FullName)\manifest.json" -Raw | ConvertFrom-Json).update) } |
        Select-Object -Last 1
    $source = (Get-Content "$($final.FullName)\manifest.json" -Raw | ConvertFrom-Json).source
    if ($source -notmatch '50 update\(s\)') { throw "Arm $arm did not finish 50 updates: $source" }
    $finals[$arm] = $final.FullName
    & $evaluator --bundle $final.FullName --map-pool $holdout --roster wide --seeds 600 --seed-base 900000000 --rounds 4 --temperature 0.001 --diplomacy --per-seat "$($arms[$arm])\eval.per-seat.tsv" *> "$($arms[$arm])\eval.log"
    if ($LASTEXITCODE -ne 0) { throw "eval $arm exited $LASTEXITCODE" }
}
& C:\Python314\python.exe -B $bootstrap "$($arms.C)\eval.per-seat.tsv" "$($arms.T)\eval.per-seat.tsv" --expected-eval *> "$($arms.T)\paired-vs-C.txt"
Get-Content "$($arms.T)\paired-vs-C.txt"
"finals: C=$($finals.C) T=$($finals.T)"
