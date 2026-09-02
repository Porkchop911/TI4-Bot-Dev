# One round of the loop that is actually working: regenerate the corpora from the current policy,
# then clone it on them under a waste ceiling.
#
# Why regenerate rather than reuse: a corpus is a set of lines played against five particular
# opponents by a particular policy, and replay must use that policy or the lines do not exist. Once
# the policy has moved, the old corpus still replays (the generator is recorded) but it demonstrates
# an older, worse policy's idea of good play. Regenerating points the demonstrations at what the
# current policy can nearly do.
#
# The rescued corpus matters more each round, not less: it is built from the starts the CURRENT
# policy fails, and those change as it improves.
#
#   ./scripts/clone_round.ps1 -From out/champions/... -Tag r2

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$From,
    [Parameter(Mandatory = $true)][string]$Tag,
    [int]$CorpusSeeds = 3000,
    [int]$RescueSeeds = 300,
    [int]$Epochs = 24,
    [double]$WasteCeiling = 5.0,
    [int]$EvalSeeds = 600
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$env:LIBTORCH = Join-Path $root 'out\libtorch-2.9.1-cu128'
$env:LIBTORCH_BYPASS_VERSION_CHECK = '1'
$env:PATH = "$($env:LIBTORCH)\lib;$($env:PATH)"
$commit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'cannot read the commit' }
$env:GIT_COMMIT = $commit

$positive = "out/corpus/positive-$Tag"
$rescued = "out/corpus/rescued-$Tag"
$out = "out/checkpoints/clone-$Tag"
foreach ($p in @($positive, $rescued, $out)) {
    $native = Join-Path $root ($p -replace '/', '\')
    if (Test-Path -LiteralPath $native) { Remove-Item -LiteralPath $native -Recurse -Force }
}

Write-Host "clone round $Tag at $commit"
Write-Host "  from $From"
Write-Host ''

Write-Host '=== 1/3 positive corpus ==='
& (Join-Path $root 'target\release\examples\build_positive_corpus.exe') `
    --bundle $From --seeds $CorpusSeeds --temperatures "0.25,0.5,0.75" --out $positive
if ($LASTEXITCODE -ne 0) { throw 'positive corpus failed' }

Write-Host ''
Write-Host '=== 2/3 rescued corpus ==='
& (Join-Path $root 'target\release\examples\rescue_search.exe') `
    --bundle $From --seeds $RescueSeeds --branches 16 --attempts 12 --temperature 1.5 --out $rescued
if ($LASTEXITCODE -ne 0) { throw 'rescue search failed' }

Write-Host ''
Write-Host '=== 3/3 cloning ==='
& (Join-Path $root 'target\release\examples\corpus_train.exe') `
    --bundle $From --replay-bundle $From `
    --corpus $positive --rescued $rescued --rescued-share 0.5 `
    --waste-ceiling $WasteCeiling --epochs $Epochs `
    --per-epoch 1200 --batch 120 --learning-rate 1e-5 `
    --eval-seeds $EvalSeeds --out $out
if ($LASTEXITCODE -ne 0) { throw 'cloning failed' }

Write-Host ''
Write-Host "done. Confirm the best epoch at 1200 seeds before believing it -- a 300-seed"
Write-Host "measurement read 95.00% where 1200 seeds read 94.45% for the same checkpoint."
