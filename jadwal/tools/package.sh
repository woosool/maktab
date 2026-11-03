#!/bin/sh
# Package Jadwal for this computer (see package-macos.sh, package-linux.sh, package-windows.ps1),
# then write dist/update.json.
set -e
cd "$(dirname "$0")/.."
case "$(uname)" in
  Darwin) sh tools/package-macos.sh "$@" ;;
  Linux) sh tools/package-linux.sh ;;
  *) echo "On Windows run: powershell -File tools/package-windows.ps1"; exit 1 ;;
esac
python3 tools/manifest.py
