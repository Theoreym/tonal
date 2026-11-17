#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PackageDir
  #error PackageDir is required
#endif
#ifndef OutputPath
  #error OutputPath is required
#endif
[Setup]
AppId={{A5F24B83-4642-4FE8-B534-2C9A3495720F}
AppName=Tonal
AppVersion={#AppVersion}
AppVerName=tonal v{#AppVersion}
DefaultDirName={localappdata}\Programs\Tonal
DefaultGroupName=Tonal
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
OutputDir={#OutputPath}
OutputBaseFilename=tonal-{#AppVersion}-windows-x64-setup
SetupIconFile=..\assets\tonal.ico
UninstallDisplayIcon={app}\tonal.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
DisableProgramGroupPage=yes
CloseApplications=yes
RestartApplications=no
[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
[Tasks]
Name: "desktopicon"; Description: "Créer un raccourci sur le Bureau"; Flags: unchecked
[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
[Icons]
Name: "{autoprograms}\Tonal"; Filename: "{app}\tonal.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Tonal"; Filename: "{app}\tonal.exe"; WorkingDir: "{app}"; Tasks: desktopicon
[Run]
Filename: "{app}\bin\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install"; StatusMsg: "Installation du composant de connexion Microsoft…"; Check: NeedsWebView2; Flags: waituntilterminated
Filename: "{app}\tonal.exe"; Description: "Ouvrir Tonal"; Flags: nowait postinstall skipifsilent
[Code]
function NeedsWebView2: Boolean;
var Version: String;
begin
  Result := not ((RegQueryStringValue(HKLM32, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')) or
                 (RegQueryStringValue(HKCU, 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0')));
end;
