[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string[]]$Corpus,

    [Parameter(Mandatory = $true)]
    [string]$Checkpoint,

    [Parameter(Mandatory = $true)]
    [string]$Output,

    [int]$Workers = [Environment]::ProcessorCount,
    [int]$Batch = 4096,
    [int]$MicroBatch = 2048,
    [int]$Epochs = 5,
    [double]$LearningRate = 0.00003,
    [int]$ControlPerMillion = 30220
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$cudaRoot = Join-Path $repo 'out\libtorch-2.9.1-cu128'
$executable = Join-Path $repo 'target-cuda\release\examples\offline_bc.exe'
$checkpointPath = (Resolve-Path -LiteralPath $Checkpoint).Path
$corpusPaths = @($Corpus | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })
$outputPath = [System.IO.Path]::GetFullPath($Output)

if ($Workers -lt 1 -or $Batch -lt 1 -or $MicroBatch -lt 1 -or $Epochs -lt 1) {
    throw 'Workers, batch, micro-batch, and epochs must all be positive.'
}
if ($MicroBatch -gt $Batch) {
    throw 'Micro-batch cannot exceed batch.'
}
if (Test-Path -LiteralPath $outputPath) {
    throw "Output already exists: $outputPath"
}
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "CUDA training executable is missing: $executable"
}
if (-not (Test-Path -LiteralPath $cudaRoot -PathType Container)) {
    throw "Pinned CUDA libtorch is missing: $cudaRoot"
}
if (-not (Test-Path -LiteralPath (Join-Path $checkpointPath 'manifest.json') -PathType Leaf)) {
    throw "Checkpoint manifest is missing: $checkpointPath"
}

$inputEvidence = foreach ($path in $corpusPaths) {
    $manifest = Join-Path $path 'manifest.json'
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        throw "Published corpus manifest is missing (staging corpora are refused): $path"
    }
    [ordered]@{
        path = $path
        manifest_sha256 = (Get-FileHash -LiteralPath $manifest -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

$gitCommit = (& git -C $repo rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) {
    throw 'Could not resolve the repository commit.'
}
$runPlanPath = "$outputPath.run.json"
$logPath = "$outputPath.log"
if ((Test-Path -LiteralPath $runPlanPath) -or (Test-Path -LiteralPath $logPath)) {
    throw "Run evidence already exists beside output: $outputPath"
}

$runPlan = [ordered]@{
    schema = 'ti4-offline-bc-run-v1'
    created_utc = [DateTime]::UtcNow.ToString('o')
    git_commit = $gitCommit
    executable = $executable
    executable_sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    checkpoint = $checkpointPath
    checkpoint_manifest_sha256 = (Get-FileHash -LiteralPath (Join-Path $checkpointPath 'manifest.json') -Algorithm SHA256).Hash.ToLowerInvariant()
    corpora = @($inputEvidence)
    output = $outputPath
    settings = [ordered]@{
        workers = $Workers
        batch = $Batch
        micro_batch = $MicroBatch
        epochs = $Epochs
        learning_rate = $LearningRate
        control_per_million = $ControlPerMillion
        validation_percent = 10
    }
}
$runPlan | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $runPlanPath -Encoding utf8

$env:LIBTORCH = $cudaRoot
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$cudaRoot\lib;$env:PATH"
$env:OMP_NUM_THREADS = '1'

$trainArgs = @('train-raw-parallel')
foreach ($path in $corpusPaths) {
    $trainArgs += @('--corpus', $path)
}
$trainArgs += @(
    '--checkpoint', $checkpointPath,
    '--out', $outputPath,
    '--workers', $Workers,
    '--batch', $Batch,
    '--micro-batch', $MicroBatch,
    '--epochs', $Epochs,
    '--learning-rate', $LearningRate.ToString([Globalization.CultureInfo]::InvariantCulture),
    '--control-per-million', $ControlPerMillion,
    '--git-commit', $gitCommit
)

Write-Host "Launching authenticated multi-corpus CUDA training with $Workers parse workers."
Write-Host "Run plan: $runPlanPath"
Write-Host "Log:      $logPath"
& $executable @trainArgs 2>&1 | Tee-Object -LiteralPath $logPath
$exitCode = $LASTEXITCODE
if ($exitCode -ne 0) {
    throw "Offline BC exited with code $exitCode; run plan and log were retained."
}
Write-Host "Training completed: $outputPath"
