; Inno Setup script for Know Giver. Built by scripts\package.ps1, which
; passes AppVersion, SourceDir (staged files) and RepoRoot.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\KnowGiver"
#endif
#ifndef RepoRoot
  #define RepoRoot "..\.."
#endif

[Setup]
AppId={{6C1F3F7E-8E0B-4C55-9B7A-3B1E8A4D2F10}
AppName=Know Giver
AppVersion={#AppVersion}
AppPublisher=Know Giver
DefaultDirName={localappdata}\Programs\KnowGiver
DefaultGroupName=Know Giver
DisableProgramGroupPage=yes
; Per-user install: no administrator rights needed.
PrivilegesRequired=lowest
OutputBaseFilename=KnowGiver-{#AppVersion}-setup
SetupIconFile={#RepoRoot}\crates\app\assets\know-giver.ico
UninstallDisplayIcon={app}\know-giver.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "ptbr"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\know-giver.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\.env.example"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LEIA-ME.txt"; DestDir: "{app}"; Flags: ignoreversion isreadme

[Icons]
Name: "{group}\Know Giver"; Filename: "{app}\know-giver.exe"
Name: "{group}\{cm:UninstallProgram,Know Giver}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Know Giver"; Filename: "{app}\know-giver.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\know-giver.exe"; Description: "{cm:LaunchProgram,Know Giver}"; Flags: nowait postinstall skipifsilent
; Silent installs (the one-line install and in-app updates) open the new version.
Filename: "{app}\know-giver.exe"; Flags: nowait; Check: WizardSilent
