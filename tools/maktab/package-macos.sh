#!/bin/sh
# Maktab for macOS, one architecture:
#   tools/maktab/package-macos.sh [aarch64|x86_64]
# builds every app in apps.tsv and makes
#   dist/maktab/Maktab-<version>-macos-<arch>.dmg      all the apps and a shortcut to Applications
#   dist/maktab/<App>-<version>-macos-<arch>.zip       each app's update archive (its updater's download)
# The disk image is LZMA-compressed (ULMO), the smallest format macOS mounts natively.
# Set CODESIGN_IDENTITY to sign (and NOTARY_PROFILE to notarize) with your Apple Developer ID.
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
ARCH=${1:-$(uname -m | sed 's/arm64/aarch64/')}
TARGET="$ARCH-apple-darwin"
VERSION=$(sh tools/maktab/version.sh)
SHARED="$ROOT/qalam/assets"   # fonts, dictionary, translations and themes, shared by the Qalam family
DIST="$ROOT/dist/maktab"
rustup target add "$TARGET" >/dev/null 2>&1 || true
mkdir -p "$DIST"
STAGE=$(mktemp -d)/Maktab
mkdir -p "$STAGE"

sign() {
  if [ -n "$CODESIGN_IDENTITY" ]; then codesign --force --deep --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$1"
  else codesign --force --deep --sign - "$1"; fi   # ad-hoc, so Apple silicon runs it
}

while IFS='|' read -r slug dir pkg bin name arabic shared bundle <&3; do
  case "$slug" in ''|\#*) continue ;; esac
  echo "== $name ($dir)"
  (cd "$dir" && MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release -p "$pkg" --bin "$bin" --target "$TARGET")
  app="$STAGE/$name.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$dir/target/$TARGET/release/$bin" "$app/Contents/MacOS/$name"
  if [ "$shared" = 1 ]; then cp -R "$SHARED/fonts" "$SHARED/dict" "$SHARED/i18n" "$SHARED/themes" "$app/Contents/Resources/"; fi
  cp "$dir/assets/icon.png" "$app/Contents/Resources/"
  # The Finder/Dock icon, from the full-size artwork.
  src="$dir/assets/icon-full.png"; [ -f "$src" ] || src="$dir/assets/icon.png"
  set_dir=$(mktemp -d)/AppIcon.iconset && mkdir -p "$set_dir"
  for s in 16 32 128 256 512; do
    sips -z $s $s "$src" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
    sips -z $((s*2)) $((s*2)) "$src" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$set_dir" -o "$app/Contents/Resources/AppIcon.icns"
  types=""
  if [ -f "$dir/tools/macos/document-types.plist" ]; then types=$(sed '/<!--/,/-->/d' "$dir/tools/macos/document-types.plist"); fi
  cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>$name</string>
  <key>CFBundleDisplayName</key><string>$name $arabic</string>
  <key>CFBundleExecutable</key><string>$name</string>
  <key>CFBundleIdentifier</key><string>$bundle</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© Woosool, MIT License</string>
$types
</dict></plist>
PLIST
  sign "$app"
  zip="$DIST/$name-$VERSION-macos-$ARCH.zip"
  rm -f "$zip"
  ditto -c -k --keepParent "$app" "$zip"
done 3< tools/maktab/apps.tsv

# The disk image: every app, a shortcut to Applications and the licence.
ln -s /Applications "$STAGE/Applications"
cp LICENSE "$STAGE/License.txt"
# Without a paid Developer ID, macOS blocks the apps on first open: say how to open them.
[ -n "$CODESIGN_IDENTITY" ] || cp tools/maktab/open-help.txt "$STAGE/If an app won't open.txt"
dmg="$DIST/Maktab-$VERSION-macos-$ARCH.dmg"
rm -f "$dmg"
# hdiutil sometimes fails with "Resource busy" on CI runners (a disk image still attached,
# Spotlight indexing the stage): wait and try again.
for try in 1 2 3 4 5; do
  hdiutil create -volname "Maktab $VERSION" -srcfolder "$STAGE" -ov -format ULMO "$dmg" >/dev/null && break
  [ "$try" = 5 ] && exit 1
  echo "hdiutil create failed; retrying ($try)" >&2
  rm -f "$dmg"
  sleep $((try * 10))
done
if [ -n "$CODESIGN_IDENTITY" ]; then
  codesign --sign "$CODESIGN_IDENTITY" --timestamp "$dmg"
  if [ -n "$NOTARY_PROFILE" ]; then
    xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$dmg"
  fi
fi
rm -rf "$(dirname "$STAGE")"
echo "Built $dmg ($(du -h "$dmg" | cut -f1))"
