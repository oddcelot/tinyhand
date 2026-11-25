; Inno Setup script for the Rust TinyHand on Windows (x64). Not run yet on a real machine.
;
; From the repo root, after `cargo build --release -p tinyhand` on Windows:
;   iscc /DVersion=1.1.0 packaging\windows\tinyhand.iss
; Output: dist\windows\TinyHand-<version>-setup.exe
;
; Layout (what crates/app/src/assets.rs and the sound player look for next to the exe):
;   {app}\tinyhand.exe
;   {app}\tinyhand.png                 welcome card icon
;   {app}\assets\hand_*.png            pointer images
;   {app}\assets\menubar_*.png         tray icons
;   {app}\assets\sounds\tap_*.wav      tap sounds
; Per-user install (no admin prompt), like the app's settings in %APPDATA%\TinyHand.

#ifndef Version
  #define Version "1.1.0"
#endif
#define Root "..\.."

[Setup]
AppId={{6F1D6C53-2D0B-4C47-9C0E-7A7D3B1E5A11}
AppName=TinyHand
AppVersion={#Version}
AppPublisher=Kamran Ahmed and TinyHand contributors
AppPublisherURL=https://tinyhand.app
DefaultDirName={localappdata}\Programs\TinyHand
DefaultGroupName=TinyHand
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#Root}\dist\windows
OutputBaseFilename=TinyHand-{#Version}-setup
Compression=lzma2
SolidCompression=yes
LicenseFile={#Root}\LICENSE
UninstallDisplayIcon={app}\tinyhand.exe
; TODO: SetupIconFile needs an .ico (none in assets/ yet); the exe has no icon either.

[Files]
Source: "{#Root}\target\release\tinyhand.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\packaging\linux\tinyhand.png"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\assets\hand_*.png"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\menubar_*.png"; DestDir: "{app}\assets"; Flags: ignoreversion
Source: "{#Root}\assets\sounds\tap_*.wav"; DestDir: "{app}\assets\sounds"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\TinyHand"; Filename: "{app}\tinyhand.exe"

[Registry]
; tinyhand:// URLs start (or are forwarded to the running) tinyhand.exe with the URL as its argument.
Root: HKCU; Subkey: "Software\Classes\tinyhand"; ValueType: string; ValueName: ""; ValueData: "URL:TinyHand"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\tinyhand"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\tinyhand\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\tinyhand.exe"" ""%1"""

[Run]
Filename: "{app}\tinyhand.exe"; Description: "Start TinyHand"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{cmd}"; Parameters: "/C taskkill /IM tinyhand.exe /F"; Flags: runhidden; RunOnceId: "StopTinyHand"
