#!/bin/sh
# macOS packages for one architecture (as Qalam's tools/package-macos.sh):
#   tools/package-macos.sh [aarch64|x86_64]
# makes dist/Waraq.app, dist/Waraq-<version>-macos-<arch>.dmg (drag to Applications) and
# dist/Waraq-<version>-macos-<arch>.zip (the update archive).
# Set CODESIGN_IDENTITY to sign (and NOTARY_PROFILE to notarize) with your Apple Developer ID.
set -e
cd "$(dirname "$0")/.."
ARCH=${1:-$(uname -m | sed 's/arm64/aarch64/')}
TARGET="$ARCH-apple-darwin"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -1)
rustup target add "$TARGET" >/dev/null 2>&1 || true
MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release -p pdf-app --bin waraq --target "$TARGET"
mkdir -p dist
app=dist/Waraq.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$TARGET/release/waraq" "$app/Contents/MacOS/Waraq"
cp assets/icon.png "$app/Contents/Resources/"
# The Finder/Dock icon, rendered from assets/icon.svg by tools/render-icon.
cp assets/Waraq.icns "$app/Contents/Resources/AppIcon.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Waraq</string>
  <key>CFBundleDisplayName</key><string>Waraq ورق</string>
  <key>CFBundleExecutable</key><string>Waraq</string>
  <key>CFBundleIdentifier</key><string>com.woosool.waraq</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© Woosool, MIT License</string>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>PDF document</string>
    <key>CFBundleTypeRole</key><string>Editor</string>
    <key>LSHandlerRank</key><string>Alternate</string>
    <key>LSItemContentTypes</key><array><string>com.adobe.pdf</string></array>
  </dict></array>
</dict></plist>
PLIST
if [ -n "$CODESIGN_IDENTITY" ]; then
  codesign --force --deep --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$app"
else
  codesign --force --deep --sign - "$app"   # ad-hoc, so Apple silicon runs it
fi
base="dist/Waraq-$VERSION-macos-$ARCH"
rm -f "$base.zip" "$base.dmg"
ditto -c -k --keepParent "$app" "$base.zip"
stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "Waraq $VERSION" -srcfolder "$stage" -ov -format UDZO "$base.dmg" >/dev/null
rm -rf "$stage"
if [ -n "$CODESIGN_IDENTITY" ]; then
  codesign --sign "$CODESIGN_IDENTITY" --timestamp "$base.dmg"
  if [ -n "$NOTARY_PROFILE" ]; then
    xcrun notarytool submit "$base.dmg" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$base.dmg"
  fi
fi
echo "Built $base.dmg and $base.zip"
