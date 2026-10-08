; Cheesestrap installer. Build with: iscc installer\Cheesestrap.iss

#define MyAppName "cheesestrap"
; keep this in sync with Cargo.toml, they are bumped together
#define MyAppVersion "1.0.2"
#define MyAppExe "Cheesestrap.exe"

[Setup]
AppId={{4C4F7B38-F25B-4B4C-8D62-11543D6A8F5D}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Baran
DefaultDirName={autopf}\Cheesestrap
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
OutputDir=output
OutputBaseFilename=Cheesestrap-Setup-{#MyAppVersion}-x64
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; The self update runs this over the top of a live cheesestrap, so let the
; installer close us instead of failing on a locked exe.
CloseApplications=yes
RestartApplications=no
WizardImageFile=wizard.bmp
WizardSmallImageFile=wizard-small.bmp
UninstallDisplayName=cheesestrap
SetupIconFile=..\assets\icon.ico
DisableProgramGroupPage=yes

[Files]
Source: "..\target\release\{#MyAppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\cheesestrap"; Filename: "{app}\{#MyAppExe}"
Name: "{autodesktop}\Cheesestrap"; Filename: "{app}\{#MyAppExe}"; Tasks: desktopicon

[UninstallDelete]
Type: filesandordirs; Name: "{app}\clients"
Type: filesandordirs; Name: "{app}\logs"
Type: files; Name: "{app}\settings.json"

[Tasks]
Name: desktopicon; Description: "Create a &desktop icon"; Flags: unchecked

[Run]
Filename: "{app}\{#MyAppExe}"; Description: "Launch cheesestrap"; Flags: nowait postinstall skipifsilent shellexec; Verb: runas

[Code]
function ProtoPointsToUs(const Proto: String): Boolean;
var
  Cmd: String;
begin
  Result := False;
  if RegQueryStringValue(
    HKCU, 'SOFTWARE\Classes\' + Proto + '\shell\open\command', '', Cmd)
  then
    Result := Pos(Lowercase(ExpandConstant('{app}')), Lowercase(Cmd)) = 2;
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    if ProtoPointsToUs('roblox') then
      RegDeleteKeyIncludingSubkeys(HKCU, 'SOFTWARE\Classes\roblox');
    if ProtoPointsToUs('roblox-player') then
      RegDeleteKeyIncludingSubkeys(HKCU, 'SOFTWARE\Classes\roblox-player');
  end;
end;
