# Removes what ti4-join-install.ps1 put in place, and nothing else.
#
# There is no recursive delete. The folder must carry the `.ti4-join-install` marker the installer
# writes; only the files that marker lists (plus the marker itself) are removed, then the two
# shortcuts if they point at this install, then the folder only if that has left it empty.
#
# An earlier version removed any folder that merely contained ti4-join.exe, recursively. Pointed at a
# folder holding a copy of the exe - Downloads, or a build directory - it would have deleted everything
# in it. Checked 2026-10-01 against target\release (82 items) with -DryRun, which reported
# "remove target\release".
param(
    [string]$Destination = (Join-Path $env:LOCALAPPDATA 'Programs\TI4 Join'),
    [switch]$DryRun
)
$ErrorActionPreference = 'Stop'

$marker = Join-Path $Destination '.ti4-join-install'
if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) {
    throw "$Destination has no TI4 Join install marker (.ti4-join-install); nothing removed. Only a folder the installer created is ever touched."
}

# Only plain file names from the marker, never paths: a listed name cannot reach outside the folder.
$listed = Get-Content -LiteralPath $marker |
    Where-Object { $_ -and -not $_.StartsWith('#') } |
    Where-Object { $_ -notmatch '[\\/:]' -and $_ -ne '.' -and $_ -ne '..' }
$files = @($listed) + '.ti4-join-install' | ForEach-Object { Join-Path $Destination $_ } |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }

# A shortcut is removed only if it launches this install's exe.
$exe = Join-Path $Destination 'ti4-join.exe'
$shell = New-Object -ComObject WScript.Shell
$links = @(
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'TI4 Join.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'TI4 Join.lnk')
) | Where-Object { (Test-Path -LiteralPath $_) -and ($shell.CreateShortcut($_).TargetPath -eq $exe) }

if ($DryRun) {
    $files | ForEach-Object { "remove    $_" }
    $links | ForEach-Object { "remove    $_" }
    "remove    $Destination   (only if empty afterwards)"
    return
}

$links | ForEach-Object { Remove-Item -LiteralPath $_ -Force }
# The marker goes last, so an interrupted uninstall can simply be run again.
$files | Sort-Object { $_ -like '*.ti4-join-install' } | ForEach-Object { Remove-Item -LiteralPath $_ -Force }
if (-not (Get-ChildItem -LiteralPath $Destination -Force)) {
    Remove-Item -LiteralPath $Destination -Force
    "TI4 Join removed."
} else {
    "TI4 Join removed. $Destination still holds other files, so the folder itself was left in place."
}
