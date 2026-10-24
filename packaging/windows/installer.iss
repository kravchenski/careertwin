; Setup program for the portable Windows package (build_windows.sh makes the folder it packs).
;
;   ISCC /DAppVersion=v0.2.0 /DSourceDir=C:\path\to\CareerTwin-Interview /OC:\out installer.iss
;
; It installs for the current user (no administrator rights) into %LOCALAPPDATA%\Programs, adds a
; Start menu entry and an optional desktop shortcut, and starts the program when it finishes.
; The program keeps its settings and documents in the `data` folder next to it. Setup never writes
; there, so an update keeps them, and the uninstaller leaves them for you to delete or keep.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #error SourceDir is not defined: pass /DSourceDir=<the unpacked CareerTwin-Interview folder>
#endif

#define AppName "CareerTwin Interview"

[Setup]
AppId={{10B902B4-D15B-43B5-8AB0-B2BCA40C9DCA}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=CareerTwin
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=CareerTwin-Interview-{#AppVersion}-windows-x64-setup
SetupIconFile={#SourceDir}\careertwin-interview.ico
UninstallDisplayIcon={app}\careertwin-interview.ico
UninstallDisplayName={#AppName}
Compression=lzma2/normal
SolidCompression=yes
LZMAUseSeparateProcess=yes
LZMANumBlockThreads=4
WizardStyle=modern
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion; Excludes: "data\*,__pycache__"

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\python\pythonw.exe"; Parameters: """{app}\start.pyw"""; WorkingDir: "{app}"; IconFilename: "{app}\careertwin-interview.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\python\pythonw.exe"; Parameters: """{app}\start.pyw"""; WorkingDir: "{app}"; IconFilename: "{app}\careertwin-interview.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\python\pythonw.exe"; Parameters: """{app}\start.pyw"""; WorkingDir: "{app}"; Description: "Start {#AppName}"; Flags: postinstall nowait skipifsilent

[UninstallDelete]
; Python writes compiled caches next to what Setup installed; clear the program, keep `data`.
Type: filesandordirs; Name: "{app}\python"
Type: filesandordirs; Name: "{app}\site-packages"
