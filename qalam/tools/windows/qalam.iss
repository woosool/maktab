; Qalam installer (Inno Setup 6). Built by tools/package-windows.ps1, which passes
; /DVersion=… /DSource=<folder with qalam.exe and resources> /DOutput=<dist folder>.
; Installs for the current user (no administrator rights), so updates can replace it.
;
; File types (.docx, .docm, .dotx, .dotm, .md, .txt): every install registers ProgIDs Woosool.Qalam.<ext>, adds them to each
; extension's OpenWithProgids, and declares Capabilities + RegisteredApplications so Qalam is listed
; in Settings > Apps > Default apps. The optional "makedefault" task also sets each extension's
; HKCU\Software\Classes\.<ext> default ProgID; Windows 10/11 keep a user's own UserChoice over that,
; and installers can't set UserChoice, so the user may still have to confirm in Settings.

#ifndef Version
  #define Version "0.1.0"
#endif

[Setup]
AppId={{6C2E8F4A-3B1D-4E7A-9C55-0A51A7E0A1A7}
AppName=Qalam
AppVersion={#Version}
AppVerName=Qalam {#Version}
AppPublisher=Woosool
AppPublisherURL=https://woosool.com
AppSupportURL=https://woosool.com/en/products/qalam
DefaultDirName={localappdata}\Programs\Qalam
DefaultGroupName=Qalam
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#Output}
OutputBaseFilename=Qalam-{#Version}-windows-x86_64-setup
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\qalam.exe
LicenseFile=..\..\LICENSE
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "ar"; MessagesFile: "compiler:Languages\Arabic.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "makedefault"; Description: "Make Qalam the default for .docx, .docm, .dotx, .dotm, .md, .txt files"; GroupDescription: "File types:"; Flags: unchecked

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Qalam"; Filename: "{app}\qalam.exe"
Name: "{group}\{cm:UninstallProgram,Qalam}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Qalam"; Filename: "{app}\qalam.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Woosool\Qalam"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Qalam"
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A word processor for .docx files in English, French and Arabic, written in Rust."
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Qalam"; ValueData: "Software\Woosool\Qalam\Capabilities"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Qalam"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
; .docx
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docx"; ValueType: string; ValueData: "Word Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docx\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docx\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.docx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.docx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".docx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docx"; ValueData: "Woosool.Qalam.docx"
Root: HKA; Subkey: "Software\Classes\.docx"; ValueType: string; ValueData: "Woosool.Qalam.docx"; Flags: uninsdeletevalue; Tasks: makedefault
; .docm
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docm"; ValueType: string; ValueData: "Word Macro-Enabled Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docm\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docm\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.docm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.docm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".docm"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docm"; ValueData: "Woosool.Qalam.docm"
Root: HKA; Subkey: "Software\Classes\.docm"; ValueType: string; ValueData: "Woosool.Qalam.docm"; Flags: uninsdeletevalue; Tasks: makedefault
; .dotx
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotx"; ValueType: string; ValueData: "Word Template"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotx\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotx\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.dotx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.dotx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".dotx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dotx"; ValueData: "Woosool.Qalam.dotx"
Root: HKA; Subkey: "Software\Classes\.dotx"; ValueType: string; ValueData: "Woosool.Qalam.dotx"; Flags: uninsdeletevalue; Tasks: makedefault
; .dotm
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotm"; ValueType: string; ValueData: "Word Macro-Enabled Template"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotm\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotm\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.dotm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.dotm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".dotm"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dotm"; ValueData: "Woosool.Qalam.dotm"
Root: HKA; Subkey: "Software\Classes\.dotm"; ValueType: string; ValueData: "Woosool.Qalam.dotm"; Flags: uninsdeletevalue; Tasks: makedefault
; .md
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.md"; ValueType: string; ValueData: "Markdown Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.md\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.md\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.md"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "Woosool.Qalam.md"
Root: HKA; Subkey: "Software\Classes\.md"; ValueType: string; ValueData: "Woosool.Qalam.md"; Flags: uninsdeletevalue; Tasks: makedefault
; .txt
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.txt"; ValueType: string; ValueData: "Text Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.txt\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.txt\shell\open\command"; ValueType: string; ValueData: """{app}\qalam.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.txt\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.txt"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".txt"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".txt"; ValueData: "Woosool.Qalam.txt"
Root: HKA; Subkey: "Software\Classes\.txt"; ValueType: string; ValueData: "Woosool.Qalam.txt"; Flags: uninsdeletevalue; Tasks: makedefault

[Run]
Filename: "{app}\qalam.exe"; Description: "{cm:LaunchProgram,Qalam}"; Flags: nowait postinstall skipifsilent
