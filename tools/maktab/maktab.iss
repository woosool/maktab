; Maktab installer (Inno Setup 6): every Woosool app in one setup. Built by
; tools/maktab/package-windows.ps1, which passes /DVersion=… /DSource=<folder with one
; subfolder per app> /DOutput=<dist folder>. The apps follow tools/maktab/apps.tsv and the file
; types follow each app's own tools/windows/<app>.iss: keep them the same.
;
; Each app is a component in its own folder ({app}\<slug>), so each app's updater can
; replace its own folder. The apps share fonts and dictionaries, which solid LZMA2 with a large
; dictionary stores about once in the installer.
; Installs for the current user (no administrator rights). File types are offered in "Open
; with" for every app; the optional "makedefault" task also makes each installed app the default
; for its types. Saboura's own .sab files always open in Saboura.

#ifndef Version
  #define Version "0.1.1"
#endif

[Setup]
AppId={{9E3A5C21-7B44-4D0F-8E2A-6F1B0C4D9A52}
AppName=Maktab
AppVersion={#Version}
AppVerName=Maktab {#Version}
AppPublisher=Woosool
AppPublisherURL=https://woosool.com
AppSupportURL=https://woosool.com
DefaultDirName={localappdata}\Programs\Maktab
DefaultGroupName=Maktab
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#Output}
OutputBaseFilename=Maktab-{#Version}-windows-x86_64-setup
SetupIconFile=assets\icon.ico
UninstallDisplayIcon={app}\qalam\qalam.exe
LicenseFile=..\..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
LZMADictionarySize=262144
LZMAUseSeparateProcess=yes
LZMANumFastBytes=273
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"
Name: "ar"; MessagesFile: "compiler:Languages\Arabic.isl"

[Types]
Name: "full"; Description: "All apps"
Name: "custom"; Description: "Choose apps"; Flags: iscustom

[Components]
Name: "qalam"; Description: "Qalam قلم — Documents"; Types: full custom
Name: "jadwal"; Description: "Jadwal جدول — Spreadsheets"; Types: full custom
Name: "sard"; Description: "Sard سرد — Presentations"; Types: full custom
Name: "saboura"; Description: "Saboura سبورة — Whiteboard"; Types: full custom
Name: "waraq"; Description: "Waraq ورق — PDF editor"; Types: full custom
Name: "barid"; Description: "Barid بريد — Mail"; Types: full custom

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "makedefault"; Description: "Make the installed apps the default for their file types"; GroupDescription: "File types:"; Flags: unchecked

[Files]
Source: "{#Source}\qalam\*"; DestDir: "{app}\qalam"; Components: qalam; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Source}\jadwal\*"; DestDir: "{app}\jadwal"; Components: jadwal; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Source}\sard\*"; DestDir: "{app}\sard"; Components: sard; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Source}\saboura\*"; DestDir: "{app}\saboura"; Components: saboura; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Source}\waraq\*"; DestDir: "{app}\waraq"; Components: waraq; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#Source}\barid\*"; DestDir: "{app}\barid"; Components: barid; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Qalam"; Filename: "{app}\qalam\qalam.exe"; Components: qalam
Name: "{userdesktop}\Qalam"; Filename: "{app}\qalam\qalam.exe"; Components: qalam; Tasks: desktopicon
Name: "{group}\Jadwal"; Filename: "{app}\jadwal\jadwal.exe"; Components: jadwal
Name: "{userdesktop}\Jadwal"; Filename: "{app}\jadwal\jadwal.exe"; Components: jadwal; Tasks: desktopicon
Name: "{group}\Sard"; Filename: "{app}\sard\sard.exe"; Components: sard
Name: "{userdesktop}\Sard"; Filename: "{app}\sard\sard.exe"; Components: sard; Tasks: desktopicon
Name: "{group}\Saboura"; Filename: "{app}\saboura\saboura.exe"; Components: saboura
Name: "{userdesktop}\Saboura"; Filename: "{app}\saboura\saboura.exe"; Components: saboura; Tasks: desktopicon
Name: "{group}\Waraq"; Filename: "{app}\waraq\waraq.exe"; Components: waraq
Name: "{userdesktop}\Waraq"; Filename: "{app}\waraq\waraq.exe"; Components: waraq; Tasks: desktopicon
Name: "{group}\Barid"; Filename: "{app}\barid\barid.exe"; Components: barid
Name: "{userdesktop}\Barid"; Filename: "{app}\barid\barid.exe"; Components: barid; Tasks: desktopicon
Name: "{group}\{cm:UninstallProgram,Maktab}"; Filename: "{uninstallexe}"

[Registry]
; Qalam
Root: HKA; Subkey: "Software\Woosool\Qalam"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Qalam"; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Qalam — Documents, part of Maktab"; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Qalam"; ValueData: "Software\Woosool\Qalam\Capabilities"; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Qalam"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docx"; ValueType: string; ValueData: "Word Document"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docx\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docx\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\.docx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.docx"; ValueData: ""; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".docx"; ValueData: ""; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docx"; ValueData: "Woosool.Qalam.docx"; Components: qalam
Root: HKA; Subkey: "Software\Classes\.docx"; ValueType: string; ValueData: "Woosool.Qalam.docx"; Flags: uninsdeletevalue; Components: qalam; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docm"; ValueType: string; ValueData: "Word Macro-Enabled Document"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docm\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.docm\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\.docm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.docm"; ValueData: ""; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".docm"; ValueData: ""; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".docm"; ValueData: "Woosool.Qalam.docm"; Components: qalam
Root: HKA; Subkey: "Software\Classes\.docm"; ValueType: string; ValueData: "Woosool.Qalam.docm"; Flags: uninsdeletevalue; Components: qalam; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotx"; ValueType: string; ValueData: "Word Template"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotx\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotx\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\.dotx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.dotx"; ValueData: ""; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".dotx"; ValueData: ""; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dotx"; ValueData: "Woosool.Qalam.dotx"; Components: qalam
Root: HKA; Subkey: "Software\Classes\.dotx"; ValueType: string; ValueData: "Woosool.Qalam.dotx"; Flags: uninsdeletevalue; Components: qalam; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotm"; ValueType: string; ValueData: "Word Macro-Enabled Template"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotm\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.dotm\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\.dotm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.dotm"; ValueData: ""; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".dotm"; ValueData: ""; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".dotm"; ValueData: "Woosool.Qalam.dotm"; Components: qalam
Root: HKA; Subkey: "Software\Classes\.dotm"; ValueType: string; ValueData: "Woosool.Qalam.dotm"; Flags: uninsdeletevalue; Components: qalam; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.md"; ValueType: string; ValueData: "Markdown Document"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.md\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.md\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.md"; ValueData: ""; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".md"; ValueData: "Woosool.Qalam.md"; Components: qalam
Root: HKA; Subkey: "Software\Classes\.md"; ValueType: string; ValueData: "Woosool.Qalam.md"; Flags: uninsdeletevalue; Components: qalam; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.txt"; ValueType: string; ValueData: "Text Document"; Flags: uninsdeletekey; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.txt\DefaultIcon"; ValueType: string; ValueData: "{app}\qalam\qalam.exe,0"; Components: qalam
Root: HKA; Subkey: "Software\Classes\Woosool.Qalam.txt\shell\open\command"; ValueType: string; ValueData: """{app}\qalam\qalam.exe"" ""%1"""; Components: qalam
Root: HKA; Subkey: "Software\Classes\.txt\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Qalam.txt"; ValueData: ""; Flags: uninsdeletevalue; Components: qalam
Root: HKA; Subkey: "Software\Classes\Applications\qalam.exe\SupportedTypes"; ValueType: string; ValueName: ".txt"; ValueData: ""; Components: qalam
Root: HKA; Subkey: "Software\Woosool\Qalam\Capabilities\FileAssociations"; ValueType: string; ValueName: ".txt"; ValueData: "Woosool.Qalam.txt"; Components: qalam
Root: HKA; Subkey: "Software\Classes\.txt"; ValueType: string; ValueData: "Woosool.Qalam.txt"; Flags: uninsdeletevalue; Components: qalam; Tasks: makedefault
; Jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Jadwal"; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Jadwal — Spreadsheets, part of Maktab"; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Jadwal"; ValueData: "Software\Woosool\Jadwal\Capabilities"; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Jadwal"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsx"; ValueType: string; ValueData: "Excel Workbook"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsx\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsx\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xlsx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xlsx"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsx"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsx"; ValueData: "Woosool.Jadwal.xlsx"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xlsx"; ValueType: string; ValueData: "Woosool.Jadwal.xlsx"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsm"; ValueType: string; ValueData: "Excel Macro-Enabled Workbook"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsm\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xlsm\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xlsm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xlsm"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xlsm"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xlsm"; ValueData: "Woosool.Jadwal.xlsm"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xlsm"; ValueType: string; ValueData: "Woosool.Jadwal.xlsm"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltx"; ValueType: string; ValueData: "Excel Template"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltx\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltx\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xltx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xltx"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xltx"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xltx"; ValueData: "Woosool.Jadwal.xltx"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xltx"; ValueType: string; ValueData: "Woosool.Jadwal.xltx"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltm"; ValueType: string; ValueData: "Excel Macro-Enabled Template"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltm\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.xltm\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xltm\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.xltm"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".xltm"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".xltm"; ValueData: "Woosool.Jadwal.xltm"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.xltm"; ValueType: string; ValueData: "Woosool.Jadwal.xltm"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.ods"; ValueType: string; ValueData: "OpenDocument Spreadsheet"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.ods\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.ods\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.ods\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.ods"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".ods"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ods"; ValueData: "Woosool.Jadwal.ods"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.ods"; ValueType: string; ValueData: "Woosool.Jadwal.ods"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.csv"; ValueType: string; ValueData: "CSV Document"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.csv\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.csv\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.csv\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.csv"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".csv"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".csv"; ValueData: "Woosool.Jadwal.csv"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.csv"; ValueType: string; ValueData: "Woosool.Jadwal.csv"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.tsv"; ValueType: string; ValueData: "Tab-Separated Values"; Flags: uninsdeletekey; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.tsv\DefaultIcon"; ValueType: string; ValueData: "{app}\jadwal\jadwal.exe,0"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Woosool.Jadwal.tsv\shell\open\command"; ValueType: string; ValueData: """{app}\jadwal\jadwal.exe"" ""%1"""; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.tsv\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Jadwal.tsv"; ValueData: ""; Flags: uninsdeletevalue; Components: jadwal
Root: HKA; Subkey: "Software\Classes\Applications\jadwal.exe\SupportedTypes"; ValueType: string; ValueName: ".tsv"; ValueData: ""; Components: jadwal
Root: HKA; Subkey: "Software\Woosool\Jadwal\Capabilities\FileAssociations"; ValueType: string; ValueName: ".tsv"; ValueData: "Woosool.Jadwal.tsv"; Components: jadwal
Root: HKA; Subkey: "Software\Classes\.tsv"; ValueType: string; ValueData: "Woosool.Jadwal.tsv"; Flags: uninsdeletevalue; Components: jadwal; Tasks: makedefault
; Sard
Root: HKA; Subkey: "Software\Woosool\Sard"; Flags: uninsdeletekey; Components: sard
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Sard"; Components: sard
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Sard — Presentations, part of Maktab"; Components: sard
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\sard\sard.exe,0"; Components: sard
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Sard"; ValueData: "Software\Woosool\Sard\Capabilities"; Flags: uninsdeletevalue; Components: sard
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Sard"; Flags: uninsdeletekey; Components: sard
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\shell\open\command"; ValueType: string; ValueData: """{app}\sard\sard.exe"" ""%1"""; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.pptx"; ValueType: string; ValueData: "PowerPoint Presentation"; Flags: uninsdeletekey; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.pptx\DefaultIcon"; ValueType: string; ValueData: "{app}\sard\sard.exe,0"; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.pptx\shell\open\command"; ValueType: string; ValueData: """{app}\sard\sard.exe"" ""%1"""; Components: sard
Root: HKA; Subkey: "Software\Classes\.pptx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Sard.pptx"; ValueData: ""; Flags: uninsdeletevalue; Components: sard
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\SupportedTypes"; ValueType: string; ValueName: ".pptx"; ValueData: ""; Components: sard
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pptx"; ValueData: "Woosool.Sard.pptx"; Components: sard
Root: HKA; Subkey: "Software\Classes\.pptx"; ValueType: string; ValueData: "Woosool.Sard.pptx"; Flags: uninsdeletevalue; Components: sard; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.potx"; ValueType: string; ValueData: "PowerPoint Template"; Flags: uninsdeletekey; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.potx\DefaultIcon"; ValueType: string; ValueData: "{app}\sard\sard.exe,0"; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.potx\shell\open\command"; ValueType: string; ValueData: """{app}\sard\sard.exe"" ""%1"""; Components: sard
Root: HKA; Subkey: "Software\Classes\.potx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Sard.potx"; ValueData: ""; Flags: uninsdeletevalue; Components: sard
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\SupportedTypes"; ValueType: string; ValueName: ".potx"; ValueData: ""; Components: sard
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities\FileAssociations"; ValueType: string; ValueName: ".potx"; ValueData: "Woosool.Sard.potx"; Components: sard
Root: HKA; Subkey: "Software\Classes\.potx"; ValueType: string; ValueData: "Woosool.Sard.potx"; Flags: uninsdeletevalue; Components: sard; Tasks: makedefault
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.ppsx"; ValueType: string; ValueData: "PowerPoint Slide Show"; Flags: uninsdeletekey; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.ppsx\DefaultIcon"; ValueType: string; ValueData: "{app}\sard\sard.exe,0"; Components: sard
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.ppsx\shell\open\command"; ValueType: string; ValueData: """{app}\sard\sard.exe"" ""%1"""; Components: sard
Root: HKA; Subkey: "Software\Classes\.ppsx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Sard.ppsx"; ValueData: ""; Flags: uninsdeletevalue; Components: sard
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\SupportedTypes"; ValueType: string; ValueName: ".ppsx"; ValueData: ""; Components: sard
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ppsx"; ValueData: "Woosool.Sard.ppsx"; Components: sard
Root: HKA; Subkey: "Software\Classes\.ppsx"; ValueType: string; ValueData: "Woosool.Sard.ppsx"; Flags: uninsdeletevalue; Components: sard; Tasks: makedefault
; Saboura
Root: HKA; Subkey: "Software\Woosool\Saboura"; Flags: uninsdeletekey; Components: saboura
Root: HKA; Subkey: "Software\Woosool\Saboura\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Saboura"; Components: saboura
Root: HKA; Subkey: "Software\Woosool\Saboura\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Saboura — Whiteboard, part of Maktab"; Components: saboura
Root: HKA; Subkey: "Software\Woosool\Saboura\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\saboura\saboura.exe,0"; Components: saboura
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Saboura"; ValueData: "Software\Woosool\Saboura\Capabilities"; Flags: uninsdeletevalue; Components: saboura
Root: HKA; Subkey: "Software\Classes\Applications\saboura.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Saboura"; Flags: uninsdeletekey; Components: saboura
Root: HKA; Subkey: "Software\Classes\Applications\saboura.exe\shell\open\command"; ValueType: string; ValueData: """{app}\saboura\saboura.exe"" ""%1"""; Components: saboura
Root: HKA; Subkey: "Software\Classes\Woosool.Saboura.sab"; ValueType: string; ValueData: "Saboura Board"; Flags: uninsdeletekey; Components: saboura
Root: HKA; Subkey: "Software\Classes\Woosool.Saboura.sab\DefaultIcon"; ValueType: string; ValueData: "{app}\saboura\saboura.exe,0"; Components: saboura
Root: HKA; Subkey: "Software\Classes\Woosool.Saboura.sab\shell\open\command"; ValueType: string; ValueData: """{app}\saboura\saboura.exe"" ""%1"""; Components: saboura
Root: HKA; Subkey: "Software\Classes\.sab\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Saboura.sab"; ValueData: ""; Flags: uninsdeletevalue; Components: saboura
Root: HKA; Subkey: "Software\Classes\Applications\saboura.exe\SupportedTypes"; ValueType: string; ValueName: ".sab"; ValueData: ""; Components: saboura
Root: HKA; Subkey: "Software\Woosool\Saboura\Capabilities\FileAssociations"; ValueType: string; ValueName: ".sab"; ValueData: "Woosool.Saboura.sab"; Components: saboura
Root: HKA; Subkey: "Software\Classes\.sab"; ValueType: string; ValueData: "Woosool.Saboura.sab"; Flags: uninsdeletevalue; Components: saboura
; Waraq
Root: HKA; Subkey: "Software\Woosool\Waraq"; Flags: uninsdeletekey; Components: waraq
Root: HKA; Subkey: "Software\Woosool\Waraq\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Waraq"; Components: waraq
Root: HKA; Subkey: "Software\Woosool\Waraq\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Waraq — PDF editor, part of Maktab"; Components: waraq
Root: HKA; Subkey: "Software\Woosool\Waraq\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\waraq\waraq.exe,0"; Components: waraq
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Waraq"; ValueData: "Software\Woosool\Waraq\Capabilities"; Flags: uninsdeletevalue; Components: waraq
Root: HKA; Subkey: "Software\Classes\Applications\waraq.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Waraq"; Flags: uninsdeletekey; Components: waraq
Root: HKA; Subkey: "Software\Classes\Applications\waraq.exe\shell\open\command"; ValueType: string; ValueData: """{app}\waraq\waraq.exe"" ""%1"""; Components: waraq
Root: HKA; Subkey: "Software\Classes\Woosool.Waraq.pdf"; ValueType: string; ValueData: "PDF Document"; Flags: uninsdeletekey; Components: waraq
Root: HKA; Subkey: "Software\Classes\Woosool.Waraq.pdf\DefaultIcon"; ValueType: string; ValueData: "{app}\waraq\waraq.exe,0"; Components: waraq
Root: HKA; Subkey: "Software\Classes\Woosool.Waraq.pdf\shell\open\command"; ValueType: string; ValueData: """{app}\waraq\waraq.exe"" ""%1"""; Components: waraq
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Waraq.pdf"; ValueData: ""; Flags: uninsdeletevalue; Components: waraq
Root: HKA; Subkey: "Software\Classes\Applications\waraq.exe\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""; Components: waraq
Root: HKA; Subkey: "Software\Woosool\Waraq\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "Woosool.Waraq.pdf"; Components: waraq
Root: HKA; Subkey: "Software\Classes\.pdf"; ValueType: string; ValueData: "Woosool.Waraq.pdf"; Flags: uninsdeletevalue; Components: waraq; Tasks: makedefault
; Barid
Root: HKA; Subkey: "Software\Woosool\Barid"; Flags: uninsdeletekey; Components: barid
Root: HKA; Subkey: "Software\Woosool\Barid\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Barid"; Components: barid
Root: HKA; Subkey: "Software\Woosool\Barid\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Barid — Mail, part of Maktab"; Components: barid
Root: HKA; Subkey: "Software\Woosool\Barid\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\barid\barid.exe,0"; Components: barid
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Barid"; ValueData: "Software\Woosool\Barid\Capabilities"; Flags: uninsdeletevalue; Components: barid
Root: HKA; Subkey: "Software\Classes\Applications\barid.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Barid"; Flags: uninsdeletekey; Components: barid
Root: HKA; Subkey: "Software\Classes\Applications\barid.exe\shell\open\command"; ValueType: string; ValueData: """{app}\barid\barid.exe"" ""%1"""; Components: barid
Root: HKA; Subkey: "Software\Woosool\Barid\Capabilities\URLAssociations"; ValueType: string; ValueName: "mailto"; ValueData: "Woosool.Barid.mailto"; Components: barid
Root: HKA; Subkey: "Software\Classes\Woosool.Barid.mailto"; ValueType: string; ValueData: "URL:MailTo Protocol"; Flags: uninsdeletekey; Components: barid
Root: HKA; Subkey: "Software\Classes\Woosool.Barid.mailto"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""; Components: barid
Root: HKA; Subkey: "Software\Classes\Woosool.Barid.mailto\shell\open\command"; ValueType: string; ValueData: """{app}\barid\barid.exe"" ""%1"""; Components: barid

[Run]
Filename: "{app}\qalam\qalam.exe"; Description: "{cm:LaunchProgram,Qalam}"; Components: qalam; Flags: nowait postinstall skipifsilent unchecked
