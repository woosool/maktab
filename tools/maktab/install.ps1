# Install (or update) Maktab on Windows from the latest GitHub release, in PowerShell:
#
#   irm https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.ps1 | iex
#
# Why this way: the installer isn't signed with a paid code-signing certificate yet, and a
# browser download shows "Windows protected your PC" (SmartScreen). A file fetched by
# PowerShell has no "downloaded from the internet" mark, so it installs without that warning.
# Installs for the current user (no administrator rights), with every app.
# To choose apps, run with: $env:MAKTAB_APPS = "qalam,sard"; irm … | iex
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest is far faster without its progress bar
$repo = "woosool/maktab"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$version = $env:MAKTAB_VERSION
if (-not $version) {
  $releases = Invoke-RestMethod "https://api.github.com/repos/$repo/releases?per_page=30" -Headers @{ "User-Agent" = "Maktab-installer" }
  $tag = ($releases | Where-Object { $_.tag_name -like "maktab-v*" } | Select-Object -First 1).tag_name
  if (-not $tag) { throw "Couldn't find a Maktab release on github.com/$repo." }
  $version = $tag.Substring("maktab-v".Length)
}
$file = "Maktab-$version-windows-x86_64-setup.exe"
$url = "https://github.com/$repo/releases/download/maktab-v$version/$file"
$out = Join-Path $env:TEMP $file
Write-Host "Downloading $file…" -ForegroundColor Cyan
Invoke-WebRequest $url -OutFile $out -UseBasicParsing
Unblock-File $out   # in case anything marked it as downloaded

$args = @("/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER")
if ($env:MAKTAB_APPS) { $args += "/COMPONENTS=$($env:MAKTAB_APPS)" }
Write-Host "Installing Maktab $version…" -ForegroundColor Cyan
$p = Start-Process $out -ArgumentList $args -Wait -PassThru
Remove-Item $out -ErrorAction SilentlyContinue
if ($p.ExitCode -ne 0) { throw "The installer stopped (code $($p.ExitCode))." }
Write-Host "Maktab $version is installed. Find the apps in the Start menu under Maktab." -ForegroundColor Green
