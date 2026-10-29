#!/bin/sh
# macOS packages for one architecture:
#   tools/package-macos.sh [aarch64|x86_64]
# makes dist/Sard.app, dist/Sard-<version>-macos-<arch>.dmg (drag to Applications) and
# dist/Sard-<version>-macos-<arch>.zip (the update archive, see app/src/update.rs).
# Set CODESIGN_IDENTITY to sign (and NOTARY_PROFILE to notarize) with your Apple Developer ID.
set -e
cd "$(dirname "$0")/.."
ARCH=${1:-$(uname -m | sed 's/arm64/aarch64/')}
TARGET="$ARCH-apple-darwin"
VERSION=$(sh tools/version.sh)
SHARED=../qalam/assets   # fonts, dictionary, translations and themes are shared with Qalam
# A native build goes to target/release (shared with local builds); a cross build to target/<triple>.
HOST=$(uname -m | sed 's/arm64/aarch64/')-apple-darwin
if [ "$TARGET" = "$HOST" ]; then OUT=target/release; TFLAG=""; else OUT="target/$TARGET/release"; TFLAG="--target $TARGET"; rustup target add "$TARGET" >/dev/null 2>&1 || true; fi
# shellcheck disable=SC2086
MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release -p slides-app --bin sard $TFLAG
mkdir -p dist
app=dist/Sard.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$OUT/sard" "$app/Contents/MacOS/Sard"
cp -R "$SHARED/fonts" "$SHARED/dict" "$SHARED/i18n" "$SHARED/themes" "$app/Contents/Resources/"
cp assets/icon.png "$app/Contents/Resources/"
# The Finder/Dock icon: an .icns built from the full-size artwork.
src="assets/icon-full.png"; [ -f "$src" ] || src=assets/icon.png
set_dir=$(mktemp -d)/AppIcon.iconset && mkdir -p "$set_dir"
for s in 16 32 128 256 512; do
  sips -z $s $s "$src" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) "$src" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$set_dir" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$set_dir")"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Sard</string>
  <key>CFBundleDisplayName</key><string>Sard سرد</string>
  <key>CFBundleExecutable</key><string>Sard</string>
  <key>CFBundleIdentifier</key><string>com.woosool.sard</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© Woosool, MIT License</string>
$(sed '/<!--/,/-->/d' tools/macos/document-types.plist)
</dict></plist>
PLIST
if [ -n "$CODESIGN_IDENTITY" ]; then
  codesign --force --deep --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$app"
else
  codesign --force --deep --sign - "$app"   # ad-hoc, so Apple silicon runs it
fi
base="dist/Sard-$VERSION-macos-$ARCH"
rm -f "$base.zip" "$base.dmg"
ditto -c -k --keepParent "$app" "$base.zip"
# The disk image: the app and a shortcut to Applications.
stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
hdiutil create -volname "Sard $VERSION" -srcfolder "$stage" -ov -format UDZO "$base.dmg" >/dev/null
rm -rf "$stage"
if [ -n "$CODESIGN_IDENTITY" ]; then
  codesign --sign "$CODESIGN_IDENTITY" --timestamp "$base.dmg"
  if [ -n "$NOTARY_PROFILE" ]; then
    xcrun notarytool submit "$base.dmg" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$base.dmg"
  fi
fi
echo "Built $base.dmg and $base.zip"
