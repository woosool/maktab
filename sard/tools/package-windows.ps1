# Windows packages (x64), run in PowerShell on Windows:
#   dist\Sard-<version>-windows-x86_64-setup.exe   installer (per user, Start menu, uninstaller, file types)
#   dist\Sard-<version>-windows-x86_64.zip         portable folder (also the update archive)
# Needs Rust (MSVC) and Inno Setup 6 (choco install innosetup).
# Set SIGN_CERT (path to .pfx) and SIGN_PASSWORD to sign with signtool.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$version = (Select-String -Path app\Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
cargo build --release -p slides-app --bin sard
if ($LASTEXITCODE -ne 0) { exit 1 }

New-Item -ItemType Directory -Force dist | Out-Null
$folder = "dist\sard"
if (Test-Path $folder) { Remove-Item -Recurse -Force $folder }
New-Item -ItemType Directory -Force "$folder\resources" | Out-Null
Copy-Item target\release\sard.exe $folder
# Fonts, dictionary, translations and themes are shared with Qalam.
foreach ($d in "fonts", "dict", "i18n", "themes") { Copy-Item -Recurse "..\qalam\assets\$d" "$folder\resources\" }
Copy-Item LICENSE $folder

function Sign($file) {
  if ($env:SIGN_CERT) { signtool sign /f $env:SIGN_CERT /p $env:SIGN_PASSWORD /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $file }
}
Sign "$folder\sard.exe"

$base = "dist\Sard-$version-windows-x86_64"
if (Test-Path "$base.zip") { Remove-Item "$base.zip" }
Compress-Archive -Path $folder -DestinationPath "$base.zip"

$iscc = @("$env:ProgramFiles(x86)\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "ISCC.exe") | Where-Object { (Get-Command $_ -ErrorAction SilentlyContinue) -or (Test-Path $_) } | Select-Object -First 1
if ($iscc) {
  & $iscc "/DVersion=$version" "/DSource=$((Resolve-Path $folder).Path)" "/DOutput=$((Resolve-Path dist).Path)" tools\windows\sard.iss
  if ($LASTEXITCODE -ne 0) { exit 1 }
  Sign "$base-setup.exe"
} else {
  Write-Warning "Inno Setup not found: only the zip was built."
}
Get-ChildItem dist -Filter "Sard-$version-windows*" | ForEach-Object { Write-Output "Built $($_.FullName)" }
