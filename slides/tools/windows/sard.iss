; Sard installer (Inno Setup 6). Built by tools/package-windows.ps1, which passes
; /DVersion=… /DSource=<folder with sard.exe and resources> /DOutput=<dist folder>.
; Installs for the current user (no administrator rights), so updates can replace it.
;
; File types (.pptx, .potx, .ppsx): every install registers ProgIDs Woosool.Sard.<ext>, adds them to each
; extension's OpenWithProgids, and declares Capabilities + RegisteredApplications so Sard is listed
; in Settings > Apps > Default apps. The optional "makedefault" task also sets each extension's
; HKCU\Software\Classes\.<ext> default ProgID; Windows 10/11 keep a user's own UserChoice over that,
; and installers can't set UserChoice, so the user may still have to confirm in Settings.

#ifndef Version
  #define Version "0.1.0"
#endif

[Setup]
AppId={{1C581CE3-D640-4179-90FF-DE2E7EE0B9B3}
AppName=Sard
AppVersion={#Version}
AppVerName=Sard {#Version}
AppPublisher=Woosool
AppPublisherURL=https://woosool.com
AppSupportURL=https://woosool.com/en/products/sard
DefaultDirName={localappdata}\Programs\Sard
DefaultGroupName=Sard
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir={#Output}
OutputBaseFilename=Sard-{#Version}-windows-x86_64-setup
SetupIconFile=..\..\assets\icon.ico
UninstallDisplayIcon={app}\sard.exe
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
Name: "makedefault"; Description: "Make Sard the default for .pptx, .potx, .ppsx files"; GroupDescription: "File types:"; Flags: unchecked

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Sard"; Filename: "{app}\sard.exe"
Name: "{group}\{cm:UninstallProgram,Sard}"; Filename: "{uninstallexe}"
Name: "{userdesktop}\Sard"; Filename: "{app}\sard.exe"; Tasks: desktopicon

[Registry]
Root: HKA; Subkey: "Software\Woosool\Sard"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Sard"
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A presentation editor for .pptx files in English, French and Arabic, written in Rust."
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\sard.exe,0"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Sard"; ValueData: "Software\Woosool\Sard\Capabilities"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Sard"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\shell\open\command"; ValueType: string; ValueData: """{app}\sard.exe"" ""%1"""
; .pptx
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.pptx"; ValueType: string; ValueData: "PowerPoint Presentation"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.pptx\DefaultIcon"; ValueType: string; ValueData: "{app}\sard.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.pptx\shell\open\command"; ValueType: string; ValueData: """{app}\sard.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.pptx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Sard.pptx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\SupportedTypes"; ValueType: string; ValueName: ".pptx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pptx"; ValueData: "Woosool.Sard.pptx"
Root: HKA; Subkey: "Software\Classes\.pptx"; ValueType: string; ValueData: "Woosool.Sard.pptx"; Flags: uninsdeletevalue; Tasks: makedefault
; .potx
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.potx"; ValueType: string; ValueData: "PowerPoint Template"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.potx\DefaultIcon"; ValueType: string; ValueData: "{app}\sard.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.potx\shell\open\command"; ValueType: string; ValueData: """{app}\sard.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.potx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Sard.potx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\SupportedTypes"; ValueType: string; ValueName: ".potx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities\FileAssociations"; ValueType: string; ValueName: ".potx"; ValueData: "Woosool.Sard.potx"
Root: HKA; Subkey: "Software\Classes\.potx"; ValueType: string; ValueData: "Woosool.Sard.potx"; Flags: uninsdeletevalue; Tasks: makedefault
; .ppsx
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.ppsx"; ValueType: string; ValueData: "PowerPoint Slide Show"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.ppsx\DefaultIcon"; ValueType: string; ValueData: "{app}\sard.exe,0"
Root: HKA; Subkey: "Software\Classes\Woosool.Sard.ppsx\shell\open\command"; ValueType: string; ValueData: """{app}\sard.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\.ppsx\OpenWithProgids"; ValueType: string; ValueName: "Woosool.Sard.ppsx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\sard.exe\SupportedTypes"; ValueType: string; ValueName: ".ppsx"; ValueData: ""
Root: HKA; Subkey: "Software\Woosool\Sard\Capabilities\FileAssociations"; ValueType: string; ValueName: ".ppsx"; ValueData: "Woosool.Sard.ppsx"
Root: HKA; Subkey: "Software\Classes\.ppsx"; ValueType: string; ValueData: "Woosool.Sard.ppsx"; Flags: uninsdeletevalue; Tasks: makedefault

[Run]
Filename: "{app}\sard.exe"; Description: "{cm:LaunchProgram,Sard}"; Flags: nowait postinstall skipifsilent
