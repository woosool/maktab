#!/bin/sh
# Windows x64 build from macOS or Linux (MinGW: brew install mingw-w64; rustup target add
# x86_64-pc-windows-gnu). Makes dist/Qalam-<version>-windows-x86_64.zip: qalam.exe with its
# resources, ready to unzip and run (Windows 10 or later). The installer needs Inno Setup on
# Windows (tools/package-windows.ps1).
set -e
cd "$(dirname "$0")/.."
VERSION=$(sh tools/version.sh)
TARGET=x86_64-pc-windows-gnu
rustup target add $TARGET >/dev/null 2>&1 || true
cargo build --release -p word-app --bin qalam --target $TARGET
mkdir -p dist
folder=dist/qalam-windows/qalam
rm -rf dist/qalam-windows && mkdir -p "$folder/resources"
cp target/$TARGET/release/qalam.exe "$folder/"
cp -R assets/fonts assets/dict assets/i18n "$folder/resources/"
cp LICENSE "$folder/"
zip_name="Qalam-$VERSION-windows-x86_64.zip"
(cd dist/qalam-windows && rm -f "../$zip_name" && zip -qr "../$zip_name" qalam)
rm -rf dist/qalam-windows
echo "Built dist/$zip_name"
