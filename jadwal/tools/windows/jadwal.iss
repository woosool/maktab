; Jadwal installer (Inno Setup 6). Built by tools/package-windows.ps1, which passes
; /DVersion=… /DSource=<folder with jadwal.exe and resources> /DOutput=<dist folder>.
; Installs for the current user (no administrator rights), so updates can replace it.
;
; File types (.xlsx, .xlsm, .xltx, .xltm, .ods, .csv, .tsv): every install registers ProgIDs Woosool.Jadwal.<ext>, adds them to each
; extension's OpenWithProgids, and declares Capabilities + RegisteredApplications so Jadwal is listed
; in Settings > Apps > Default apps. The optional "makedefault" task also sets each extension's
; HKCU\Software\Classes\.<ext> default ProgID; Windows 10/11 keep a user's own UserChoice over that,
; and installers can't set UserChoice, so the user may still have to confirm in Settings.

#ifndef Version
  #define Version "0.1.0"
#endif

[Setup]
AppId={{4BB3AC56-0EED-48A5-9A8F-1FD1B4CA754C}
AppName=Jadwal
AppVersion={#Version}
AppVerName=Jadwal {#Version}
AppPublisher=Woosool
AppPublisherURL=https://woosool.com
AppSupportURL=https://woosool.com/en/products/jadwal
DefaultDirName={localappdata}\Programs\Jadwal
DefaultGroupName=Jadwal
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#Output}
OutputBaseFilename=Jadwal-{#Version}-windows-x86_64-setup
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\jadwal.exe
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
Name: "makedefault"; Description: "Make Jadwal the default for .xlsx, .xlsm, .xltx, .xltm, .ods, .csv, .tsv files"; GroupDescription: "File types:"; Flags: unchecked

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Jadwal"; Filename: "{app}\jadwal.exe"
Name: "{group}\{cm:UninstallProgram,Jadwal}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Jadwal"; Filename: "{app}\jadwal.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Woosool\Jadwal"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Jadwal"
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A spreadsheet editor for .xlsx files in English, French and Arabic, written in Rust."
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Jadwal"; ValueData: "Software\Woosool\Jadwal\Capabilities"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Jadwal"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
; .xlsx
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsx"; ValueType: string; ValueData: "Excel Workbook"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsx\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsx\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xlsx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xlsx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsx"; ValueData: "Woosool.Jadwal.xlsx"
Root: HKA; Subkey: "Software\Classes\.xlsx"; ValueType: string; ValueData: "Woosool.Jadwal.xlsx"; Flags: uninsdeletevalue; Tasks: makedefault
; .xlsm
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsm"; ValueType: string; ValueData: "Excel Macro-Enabled Workbook"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsm\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsm\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xlsm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xlsm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsm"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsm"; ValueData: "Woosool.Jadwal.xlsm"
Root: HKA; Subkey: "Software\Classes\.xlsm"; ValueType: string; ValueData: "Woosool.Jadwal.xlsm"; Flags: uninsdeletevalue; Tasks: makedefault
; .xltx
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltx"; ValueType: string; ValueData: "Excel Template"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltx\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltx\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xltx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xltx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xltx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xltx"; ValueData: "Woosool.Jadwal.xltx"
Root: HKA; Subkey: "Software\Classes\.xltx"; ValueType: string; ValueData: "Woosool.Jadwal.xltx"; Flags: uninsdeletevalue; Tasks: makedefault
; .xltm
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltm"; ValueType: string; ValueData: "Excel Macro-Enabled Template"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltm\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltm\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.xltm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xltm"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xltm"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xltm"; ValueData: "Woosool.Jadwal.xltm"
Root: HKA; Subkey: "Software\Classes\.xltm"; ValueType: string; ValueData: "Woosool.Jadwal.xltm"; Flags: uninsdeletevalue; Tasks: makedefault
; .ods
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.ods"; ValueType: string; ValueData: "OpenDocument Spreadsheet"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.ods\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.ods\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.ods\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.ods"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".ods"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ods"; ValueData: "Woosool.Jadwal.ods"
Root: HKA; Subkey: "Software\Classes\.ods"; ValueType: string; ValueData: "Woosool.Jadwal.ods"; Flags: uninsdeletevalue; Tasks: makedefault
; .csv
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.csv"; ValueType: string; ValueData: "CSV Document"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.csv\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.csv\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.csv\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.csv"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".csv"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".csv"; ValueData: "Woosool.Jadwal.csv"
Root: HKA; Subkey: "Software\Classes\.csv"; ValueType: string; ValueData: "Woosool.Jadwal.csv"; Flags: uninsdeletevalue; Tasks: makedefault
; .tsv
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.tsv"; ValueType: string; ValueData: "Tab-Separated Values"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.tsv\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.tsv\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.tsv\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.tsv"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".tsv"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tsv"; ValueData: "Woosool.Jadwal.tsv"
Root: HKA; Subkey: "Software\Classes\.tsv"; ValueType: string; ValueData: "Woosool.Jadwal.tsv"; Flags: uninsdeletevalue; Tasks: makedefault

[Run]
Filename: "{app}\jadwal.exe"; Description: "{cm:LaunchProgram,Jadwal}"; Flags: nowait postinstall skipifsilent
