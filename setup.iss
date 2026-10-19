; Download Manager — Inno Setup installer script
; Builds a single setup.exe: service + UI + browser integration + extension.

[Setup]
AppId={{7D3A9F21-4C8E-4E5A-9B6D-DOWNLOADMGR01}
AppName=Download Manager
; Version is injected by CI (/DAppVersion=x.y.z from the VERSION file); default for local builds.
#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
AppVersion={#AppVersion}
AppPublisher=Download Manager
DefaultDirName={autopf}\Download Manager
DefaultGroupName=Download Manager
OutputDir=dist
OutputBaseFilename=setup
Compression=lzma2/max
SolidCompression=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
DisableProgramGroupPage=yes
UninstallDisplayIcon={app}\dm-ui.exe

[Files]
Source: "target\release\dm-service.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "target\release\dm-ui.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "target\release\dm-host.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "target\release\dm-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "extension\*"; DestDir: "{app}\extension"; Flags: recursesubdirs ignoreversion
Source: "install-service.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "uninstall-cleanup.ps1"; DestDir: "{app}"; Flags: ignoreversion
Source: "register-host.ps1"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Download Manager"; Filename: "{app}\dm-ui.exe"
Name: "{autodesktop}\Download Manager"; Filename: "{app}\dm-ui.exe"

[Run]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\install-service.ps1"" -Dir ""{app}"""; Flags: runhidden waituntilterminated; Description: "Installing Windows service"; StatusMsg: "Installing Windows service..."
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\register-host.ps1"" -HostPath ""{app}\dm-host.exe"" -AllUsers"; Flags: runhidden waituntilterminated; Description: "Registering browser integration"; StatusMsg: "Registering browser integration..."
Filename: "{app}\dm-ui.exe"; Description: "Launch Download Manager"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\uninstall-cleanup.ps1"""; Flags: runhidden waituntilterminated runascurrentuser

[UninstallDelete]
Type: files; Name: "{app}\com.downloadmanager.host.chrome.json"
Type: files; Name: "{app}\com.downloadmanager.host.firefox.json"
Type: filesandordirs; Name: "{app}\extension"
