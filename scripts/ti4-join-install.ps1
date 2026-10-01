# Installs ti4-join for the current user: no administrator rights, nothing outside the user profile.
#
#   Program:     %LOCALAPPDATA%\Programs\TI4 Join\ti4-join.exe
#   Start menu:  TI4 Join
#   Desktop:     TI4 Join            (skip with -NoDesktopShortcut)
#
# Run it from the unzipped folder, or double-click Install.cmd. -DryRun prints what would happen.
#
# It also writes `.ti4-join-install` beside the program, listing exactly what it put there. The
# uninstaller refuses to touch a folder without that marker and removes only the files it lists, so an
# install pointed somewhere unusual (-Destination into Downloads, say) uninstalls down to exactly what
# it added and never takes anything else with it.
param(
    [string]$Destination = (Join-Path $env:LOCALAPPDATA 'Programs\TI4 Join'),
    [switch]$NoDesktopShortcut,
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

$source = Join-Path $PSScriptRoot 'ti4-join.exe'
if (-not (Test-Path -LiteralPath $source)) {
    throw "ti4-join.exe is not next to this script ($PSScriptRoot). Unzip the whole package first."
}
$uninstaller = Join-Path $PSScriptRoot 'uninstall.ps1'
$programs = [Environment]::GetFolderPath('Programs')
$desktop = [Environment]::GetFolderPath('Desktop')
$links = @(Join-Path $programs 'TI4 Join.lnk')
if (-not $NoDesktopShortcut) { $links += Join-Path $desktop 'TI4 Join.lnk' }
$marker = Join-Path $Destination '.ti4-join-install'

if ($DryRun) {
    "copy      $source -> $Destination\ti4-join.exe"
    "copy      $uninstaller -> $Destination\uninstall.ps1"
    "marker    $marker"
    $links | ForEach-Object { "shortcut  $_" }
    return
}

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
Copy-Item -LiteralPath $source -Destination (Join-Path $Destination 'ti4-join.exe') -Force
$installed = @('ti4-join.exe')
if (Test-Path -LiteralPath $uninstaller) {
    Copy-Item -LiteralPath $uninstaller -Destination (Join-Path $Destination 'uninstall.ps1') -Force
    $installed += 'uninstall.ps1'
}
$shell = New-Object -ComObject WScript.Shell
foreach ($path in $links) {
    $link = $shell.CreateShortcut($path)
    $link.TargetPath = Join-Path $Destination 'ti4-join.exe'
    $link.WorkingDirectory = $Destination
    $link.Description = 'Join an online TI4 table'
    $link.Save()
}
# Written last, so a half-finished install has no marker and cannot be "uninstalled" by a later run.
@(
    '# TI4 Join install marker. The uninstaller removes only the files listed here and only'
    '# from the folder holding this file. Do not edit.'
) + $installed | Set-Content -LiteralPath $marker -Encoding utf8
"Installed to $Destination. Start it from the Start menu: TI4 Join."
