# Stage-2 training FROM BLANK with the three added reward shapings.
#
# "From blank" = no --checkpoint: every faction starts from zero weights (the runner's own
# `blank()` profiles). Everything else stays at the runner's reference defaults (r1 bonus 3.0 /
# r1 shaping 0.1, learning rate 0.03, entropy 0.01, gamma 1, 4-round horizon, six factions).
#
# The three shapings (all off by default in the engine; this script turns them on):
#   --fleet-weight 0.03
#       Moderate fleet-strength term: a potential difference over the seat's fleet value in
#       resources (fighters count as 0.75 each, an upgraded ship at 1.3x its base unit's cost,
#       e.g. dreadnought 4 -> dreadnought II 5.2). A ~15-resource fleet pays about +0.45 over a
#       whole game -- deliberately well below any clearance-penalty scale used so far (arena arms
#       ran clearance_weight = 5), and paid back when the fleet is lost.
#   --tech-weight 0.1
#       Small term per technology owned beyond setup (+0.1 each; kept below vp_weight = 1.0 so
#       researching stays a path to points rather than an end in itself).
#   --strategy-diversity-weight 1.0
#       Terminal monoculture penalty: when a seat's most-played strategy card exceeds 80% of at
#       least three plays, the final slot pays up to this weight (zero at exactly 80%, full at
#       100%; four-of-four same card costs exactly 1.0). Carried by every decision's return like
#       the clearance floor.
#
# The run records its own arguments in the checkpoint document, so it is reproducible from its
# artifact. Resume later with -Checkpoint <file> (the bootstrap stays immutable; --out must then
# be a distinct file).
#
#   ./scripts/stage2_shaped_training.ps1                      # 10k updates to out\stage2_shaped
#   ./scripts/stage2_shaped_training.ps1 -Updates 500         # short probe first

[CmdletBinding()]
param(
    [int]$Updates = 10000,
    [double]$FleetWeight = 0.03,
    [double]$TechWeight = 0.1,
    [double]$DiversityWeight = 1.0,
    [string]$OutDir = 'out\stage2_shaped',
    # Set to resume from an existing checkpoint instead of training from blank.
    [string]$Checkpoint
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$out = Join-Path $OutDir "final$Updates.json"
New-Item -ItemType Directory -Force -Path (Split-Path $out -Parent) | Out-Null

# Invariant-culture formatting so the decimal point survives any machine locale.
function Format-Decimal([double]$Value) {
    $Value.ToString([System.Globalization.CultureInfo]::InvariantCulture)
}

$cargoArgs = @(
    'run', '--release', '-p', 'ti4-training', '--example', 'stage2_training', '--'
    "--updates", [string]$Updates,
    "--fleet-weight", (Format-Decimal $FleetWeight),
    "--tech-weight", (Format-Decimal $TechWeight),
    "--strategy-diversity-weight", (Format-Decimal $DiversityWeight),
    '--out', $out
)
if ($Checkpoint) {
    if ((Resolve-Path -LiteralPath $out).Path -eq (Resolve-Path -LiteralPath $Checkpoint).Path) {
        throw "--checkpoint and --out must be distinct files"
    }
    $cargoArgs += @('--checkpoint', $Checkpoint)
}

$mode = if ($Checkpoint) { "resuming from $Checkpoint" } else { 'from blank (zero weights)' }
Write-Host "stage-2 training $mode"
Write-Host "  updates   $Updates"
Write-Host "  fleet     +$FleetWeight per resource of fleet value"
Write-Host "  tech      +$TechWeight per technology beyond setup"
Write-Host "  diversity up to -$DiversityWeight for strategy-card monoculture (>80% of >=3 plays)"
Write-Host "  out       $out"
Write-Host ''

cargo @cargoArgs
