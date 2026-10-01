; Inno Setup script for the TI4 Join Windows installer.
;
; Built by scripts\package_ti4_join.ps1, which passes:
;   /DCommit=<full commit>   /DShort=<8-char commit>   /DExe=<path to ti4-join.exe>   /DReadme=<rendered README>
;
; Produces out\TI4-Join-Setup-<short>.exe: a per-user install with no administrator rights, into
; %LOCALAPPDATA%\Programs\TI4 Join (the same folder the PowerShell installer uses), with a Start-menu
; shortcut, an optional desktop shortcut, and an entry in Settings > Apps.
;
; Uninstall is Inno's own: it removes only the files this setup installed and the shortcuts it made,
; and removes the folder only when that leaves it empty. Nothing else in the folder is touched.
;
; AppId is permanent. A later setup with the same AppId upgrades in place rather than installing a
; second copy, which is what lets a player go from one build to the next by running the new setup.

#ifndef Commit
  #error Commit is not defined: build this through scripts\package_ti4_join.ps1
#endif

[Setup]
AppId={{540C692A-5A39-4F38-B2DE-E7BF19F37235}
AppName=TI4 Join
AppVersion={#Short}
AppVerName=TI4 Join (build {#Short})
AppPublisher=TI4 engine project
VersionInfoVersion=0.1.0.0
VersionInfoDescription=TI4 Join - a seat at an online TI4 table (build {#Commit})
PrivilegesRequired=lowest
DefaultDirName={autopf}\TI4 Join
DisableProgramGroupPage=yes
DisableDirPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
OutputBaseFilename=TI4-Join-Setup-{#Short}
UninstallDisplayName=TI4 Join (build {#Short})
UninstallDisplayIcon={app}\ti4-join.exe

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
Source: "{#Exe}"; DestDir: "{app}"; DestName: "ti4-join.exe"; Flags: ignoreversion
Source: "{#Readme}"; DestDir: "{app}"; DestName: "README.txt"; Flags: ignoreversion isreadme

[Icons]
Name: "{autoprograms}\TI4 Join"; Filename: "{app}\ti4-join.exe"; WorkingDir: "{app}"; Comment: "Join an online TI4 table"
Name: "{autodesktop}\TI4 Join"; Filename: "{app}\ti4-join.exe"; WorkingDir: "{app}"; Comment: "Join an online TI4 table"; Tasks: desktopicon

[Run]
Filename: "{app}\ti4-join.exe"; Description: "Start TI4 Join now"; Flags: nowait postinstall skipifsilent
