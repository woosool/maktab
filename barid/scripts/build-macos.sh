#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
profile="${1:-debug}"
case "$profile" in
  debug) cargo build --locked ;;
  release) cargo build --locked --release ;;
  *) echo 'Usage: scripts/build-macos.sh [debug|release]' >&2; exit 2 ;;
esac
mkdir -p target/Barid.app/Contents/MacOS
cp packaging/macos/Info.plist target/Barid.app/Contents/Info.plist
cp "target/$profile/barid" target/Barid.app/Contents/MacOS/barid
printf 'Built %s/target/Barid.app\n' "$PWD"
