# Maktab for Windows (x64), run in PowerShell on Windows from anywhere:
#   powershell -File tools\maktab\package-windows.ps1
# builds every app in apps.tsv and makes
#   dist\maktab\Maktab-<version>-windows-x86_64-setup.exe   one installer for all the apps (each optional)
#   dist\maktab\Maktab-<version>-windows-x86_64.zip         portable folder with every app
#   dist\maktab\<App>-<version>-windows-x86_64.zip          each app's update archive
# Needs Rust (MSVC) and Inno Setup 6 (choco install innosetup).
# Set SIGN_CERT (path to .pfx) and SIGN_PASSWORD to sign with signtool.
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
Set-Location $root
$version = (Select-String -Path qalam\app\Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
$dist = Join-Path $root "dist\maktab"
$stage = Join-Path $dist "stage"
New-Item -ItemType Directory -Force $dist | Out-Null
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null

function Sign($file) {
  if ($env:SIGN_CERT) { signtool sign /f $env:SIGN_CERT /p $env:SIGN_PASSWORD /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $file }
}

$apps = Get-Content tools\maktab\apps.tsv -Encoding UTF8 | Where-Object { $_ -and -not $_.StartsWith("#") } | ForEach-Object {
  $f = $_.Split("|")
  [pscustomobject]@{ Slug = $f[0]; Dir = $f[1]; Package = $f[2]; Bin = $f[3]; Name = $f[4]; Shared = $f[6] -eq "1" }
}

foreach ($a in $apps) {
  Write-Output "== $($a.Name) ($($a.Dir))"
  Push-Location $a.Dir
  cargo build --release -p $a.Package --bin $a.Bin
  if ($LASTEXITCODE -ne 0) { Pop-Location; exit 1 }
  Pop-Location
  $folder = Join-Path $stage $a.Slug
  New-Item -ItemType Directory -Force $folder | Out-Null
  Copy-Item "$($a.Dir)\target\release\$($a.Bin).exe" $folder
  Copy-Item LICENSE $folder
  if ($a.Shared) {
    New-Item -ItemType Directory -Force "$folder\resources" | Out-Null
    foreach ($d in "fonts", "dict", "i18n", "themes") { Copy-Item -Recurse "qalam\assets\$d" "$folder\resources\" }
  }
  Sign "$folder\$($a.Bin).exe"
  $zip = Join-Path $dist "$($a.Name)-$version-windows-x86_64.zip"
  if (Test-Path $zip) { Remove-Item $zip }
  Compress-Archive -Path $folder -DestinationPath $zip
}

$portable = Join-Path $dist "Maktab-$version-windows-x86_64.zip"
if (Test-Path $portable) { Remove-Item $portable }
Compress-Archive -Path "$stage\*" -DestinationPath $portable -CompressionLevel Optimal

$iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "ISCC.exe") | Where-Object { (Get-Command $_ -ErrorAction SilentlyContinue) -or (Test-Path $_) } | Select-Object -First 1
if ($iscc) {
  & $iscc "/DVersion=$version" "/DSource=$stage" "/DOutput=$dist" tools\maktab\maktab.iss
  if ($LASTEXITCODE -ne 0) { exit 1 }
  Sign (Join-Path $dist "Maktab-$version-windows-x86_64-setup.exe")
} else {
  Write-Warning "Inno Setup not found: only the zips were built."
}
Remove-Item -Recurse -Force $stage
Get-ChildItem $dist | ForEach-Object { Write-Output "Built $($_.FullName) ($([math]::Round($_.Length / 1MB, 1)) MB)" }
