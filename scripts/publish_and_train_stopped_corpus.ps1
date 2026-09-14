[CmdletBinding()]
param(
    [string]$Staging = 'E:\ti4-corpus\vponly-single-236464-20260914.staging-59548',
    [string]$CorpusOutput = 'E:\ti4-corpus\vponly-single-236464-20260914-partial',
    [string]$Checkpoint = 'D:\Projects\ti4-engine-rs\out\offline-bc-v2-20260913-from-318956',
    [string]$TrainingOutput = 'D:\Projects\ti4-engine-rs\out\offline-bc-v3-20260914-from-bcv2',
    [int]$Workers = 32
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$capture = Join-Path $repo 'target-publisher\release\examples\capture_offline_pilot.exe'
$trainer = Join-Path $repo 'target-cuda\release\examples\offline_bc.exe'
$pool = Join-Path $repo 'out\pools\full_np8_12_train.json'
$cpuRoot = Join-Path $repo 'out\libtorch-2.9.1-cpu'
$cudaRoot = Join-Path $repo 'out\libtorch-2.9.1-cu128'
$publishLog = "$CorpusOutput.publish.log"

if ($Workers -lt 1) {
    throw 'Workers must be positive.'
}
if (Test-Path -LiteralPath $CorpusOutput) {
    throw "Corpus output already exists: $CorpusOutput"
}
if (Test-Path -LiteralPath $TrainingOutput) {
    throw "Training output already exists: $TrainingOutput"
}
if (-not (Test-Path -LiteralPath $Staging -PathType Container)) {
    throw "Stopped staging directory is missing: $Staging"
}
if (-not (Test-Path -LiteralPath $capture -PathType Leaf)) {
    throw "Verified publisher executable is missing: $capture"
}
if (-not (Test-Path -LiteralPath $trainer -PathType Leaf)) {
    throw "Verified CUDA trainer executable is missing: $trainer"
}

Push-Location $repo
try {
    # The publisher does not perform tensor work in recovery mode, but the Rust example is linked
    # against libtorch and Windows must resolve those DLLs before `main` can select the mode.
    $env:LIBTORCH = $cpuRoot
    $env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
    $env:PATH = "$cpuRoot\lib;$env:PATH"

    & $capture `
        --publish-staging $Staging `
        --out $CorpusOutput `
        --checkpoint 'D:\Projects\ti4-engine-rs\out\vponly-main-20260911\checkpoints\checkpoint-236464' `
        --games-played 158755 `
        --games-planned 332768 `
        --workers $Workers `
        --map-pool $pool `
        --map-pool-sha256 '106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df' `
        --policy-mode single `
        --generation-git-commit '6938019d690bf728f381bab4e203331982ebbf4d' `
        --generation-worktree-dirty true `
        --generator-sha256 'be6895cc64c77e38a23dc9b3a941bcfe191db669453348e4df2201961ce16b70' `
        2>&1 | Tee-Object -LiteralPath $publishLog
    if ($LASTEXITCODE -ne 0) { throw "Partial publication failed: $LASTEXITCODE" }

    $env:LIBTORCH = $cudaRoot
    $env:PATH = "$cudaRoot\lib;$env:PATH"
    & (Join-Path $PSScriptRoot 'train_offline_corpus.ps1') `
        -Corpus @((Join-Path $CorpusOutput 'good'), (Join-Path $CorpusOutput 'random')) `
        -Checkpoint $Checkpoint `
        -Output $TrainingOutput `
        -Workers $Workers `
        -Batch 4096 `
        -MicroBatch 2048 `
        -Epochs 5 `
        -LearningRate 0.00003 `
        -ControlPerMillion 30220
} finally {
    Pop-Location
}
