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
Name: "startup"; Description: "{cm:StartOnStartup}"; GroupDescription: "{cm:AdditionalOptions}"; Flags: unchecked
Name: "studyreminders"; Description: "{cm:StudyReminders}"; GroupDescription: "{cm:AdditionalOptions}"; Flags: unchecked
; Offered only when Claude Code is missing and someone is watching (never in updates).
Name: "claudecode"; Description: "{cm:InstallClaudeCode}"; GroupDescription: "{cm:AccountServices}"; Check: ClaudeCodeMissing

[Files]
Source: "{#SourceDir}\know-giver.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\.env.example"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\vcruntime140_1.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\local-runtime\*"; DestDir: "{app}\local-runtime"; Flags: ignoreversion recursesubdirs createallsubdirs
; Read-me files in the five app languages.
Source: "{#SourceDir}\README.*.txt"; DestDir: "{app}"; Flags: ignoreversion

[InstallDelete]
; The single Portuguese read-me of earlier versions.
Type: files; Name: "{app}\LEIA-ME.txt"

[Icons]
Name: "{group}\Know Giver"; Filename: "{app}\know-giver.exe"
Name: "{group}\{cm:UninstallProgram,Know Giver}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Know Giver"; Filename: "{app}\know-giver.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Know Giver"; ValueData: """{app}\know-giver.exe"" --background"; Tasks: startup; Flags: uninsdeletevalue

[UninstallDelete]
; The program folder goes entirely, including a .env with an API key. Study
; data lives elsewhere ({localappdata}\KnowGiver) and is asked about below.
Type: filesandordirs; Name: "{app}"

[CustomMessages]
ptbr.DeleteStudyData=Apagar também os seus dados de estudo (planos, progresso, modelos de voz e de fala)?%n%n%1%n%nSe escolher Não, eles ficam e voltam se você instalar o Know Giver de novo.
en.DeleteStudyData=Also delete your study data (plans, progress, voice and speech models)?%n%n%1%n%nIf you choose No, it stays and comes back if you install Know Giver again.
ptbr.AdditionalOptions=Outras opções
en.AdditionalOptions=Additional options
ptbr.StartOnStartup=Iniciar o Know Giver quando você entrar no Windows
en.StartOnStartup=Start Know Giver when you sign in to Windows
ptbr.StudyReminders=Receber lembretes de estudo a cada 3 horas enquanto o Know Giver estiver aberto, mesmo na bandeja
en.StudyReminders=Receive study reminders every 3 hours while Know Giver is running, including in the system tray
ptbr.AccountServices=Entrar com a sua assinatura
en.AccountServices=Sign in with your subscription
ptbr.InstallClaudeCode=Instalar também o Claude Code, o programa oficial da Anthropic, para usar a sua assinatura Claude nas aulas
en.InstallClaudeCode=Also install Claude Code, Anthropic's official program, to use your Claude subscription in lessons
ptbr.InstallingClaudeCode=Instalando o Claude Code com o instalador oficial da Anthropic…
en.InstallingClaudeCode=Installing Claude Code with Anthropic's official installer…
ptbr.ReminderPreferenceFailed=Não foi possível salvar a escolha dos lembretes de estudo.
en.ReminderPreferenceFailed=Could not save the study reminder preference.

[Run]
; Anthropic's official installer, for this user only; a failure leaves Know Giver installed.
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -Command ""irm https://claude.ai/install.ps1 | iex"""; StatusMsg: "{cm:InstallingClaudeCode}"; Tasks: claudecode; Flags: runhidden waituntilterminated
Filename: "{app}\know-giver.exe"; Description: "{cm:LaunchProgram,Know Giver}"; Flags: nowait postinstall skipifsilent
; Silent installs (the one-line install and in-app updates) open the new version.
Filename: "{app}\know-giver.exe"; Flags: nowait; Check: WizardSilent

[Code]
// Claude Code on the PATH, from its native installer, or from npm.
function ClaudeCodeMissing: Boolean;
begin
  Result := (not WizardSilent)
    and (FileSearch('claude.exe', GetEnv('PATH')) = '')
    and (not FileExists(ExpandConstant('{%USERPROFILE}\.local\bin\claude.exe')))
    and (not FileExists(ExpandConstant('{userappdata}\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe')));
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  FlagFile: String;
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('studyreminders') then
  begin
    FlagFile := ExpandConstant('{app}\.study-reminders-enabled');
    if not SaveStringToFile(FlagFile, '1', False) then
      RaiseException(CustomMessage('ReminderPreferenceFailed'));
  end;
end;

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
