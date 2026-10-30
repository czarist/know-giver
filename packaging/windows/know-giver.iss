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
AppPublisher=Trentin Soluções em Software
AppPublisherURL=https://trentin.software
AppSupportURL=https://github.com/czarist/know-giver/issues
AppUpdatesURL=https://github.com/czarist/know-giver/releases
AppCopyright=Trentin Soluções em Software. MIT OR Apache-2.0
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Know Giver setup
UninstallDisplayName=Know Giver
; A running Know Giver is closed before its files are replaced.
CloseApplications=yes
RestartApplications=no
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
; Read-me files in the five app languages.
Source: "{#SourceDir}\README.*.txt"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; The single Portuguese read-me of earlier versions.
Type: files; Name: "{app}\LEIA-ME.txt"

[Icons]
Name: "{group}\Know Giver"; Filename: "{app}\know-giver.exe"
Name: "{group}\{cm:UninstallProgram,Know Giver}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Know Giver"; Filename: "{app}\know-giver.exe"; Tasks: desktopicon

[UninstallDelete]
; The program folder goes entirely, including a .env with an API key. Study
; data lives elsewhere ({localappdata}\KnowGiver) and is asked about below.
Type: filesandordirs; Name: "{app}"

[CustomMessages]
ptbr.DeleteStudyData=Apagar também os seus dados de estudo (planos, progresso, modelos de voz e de fala)?%n%n%1%n%nSe escolher Não, eles ficam e voltam se você instalar o Know Giver de novo.
en.DeleteStudyData=Also delete your study data (plans, progress, voice and speech models)?%n%n%1%n%nIf you choose No, it stays and comes back if you install Know Giver again.

[Run]
Filename: "{app}\know-giver.exe"; Description: "{cm:LaunchProgram,Know Giver}"; Flags: nowait postinstall skipifsilent
; Silent installs (the one-line install and in-app updates) open the new version.
Filename: "{app}\know-giver.exe"; Flags: nowait; Check: WizardSilent

[Code]
// After uninstalling, offer to delete the study data too. No is the default,
// and silent uninstalls keep it.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Data: String;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    Data := ExpandConstant('{localappdata}\KnowGiver');
    if DirExists(Data) and (not UninstallSilent) and
      (MsgBox(FmtMessage(CustomMessage('DeleteStudyData'), [Data]),
        mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES) then
      DelTree(Data, True, True, True);
  end;
end;
