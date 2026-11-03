#!/bin/sh
# Linux packages (x86_64 or aarch64, built on that machine):
#   dist/Qalam-<version>-linux-<arch>.tar.gz   portable folder (also the update archive, as .zip)
#   dist/qalam_<version>_<debarch>.deb         installs to /opt/qalam with a menu entry
#   dist/Qalam-<version>-linux-<arch>.AppImage when appimagetool is on PATH
# Making Qalam the default app for its formats is opt-in: run `qalam-set-default` as the user.
# Build needs: libgtk-3-dev libxkbcommon-dev libwayland-dev libxcb1-dev libpipewire-0.3-dev
#              libgbm-dev libegl-dev dpkg-dev
set -e
cd "$(dirname "$0")/.."
VERSION=$(sh tools/version.sh)
ARCH=$(uname -m)
DEBARCH=$( [ "$ARCH" = aarch64 ] && echo arm64 || echo amd64 )
cargo build --release -p word-app --bin qalam
mkdir -p dist
base="dist/Qalam-$VERSION-linux-$ARCH"

# Portable folder: qalam + resources/.
folder=dist/qalam
rm -rf "$folder" && mkdir -p "$folder/resources"
cp target/release/qalam "$folder/"
cp -R assets/fonts assets/dict assets/i18n "$folder/resources/"
cp assets/icon.png "$folder/qalam.png"
cp tools/linux/qalam.desktop tools/linux/qalam-set-default "$folder/"
chmod +x "$folder/qalam-set-default"
tar -C dist -czf "$base.tar.gz" qalam
(cd dist && rm -f "$(basename "$base").zip" && zip -qr "$(basename "$base").zip" qalam)

# Debian package.
deb=$(mktemp -d)/qalam
mkdir -p "$deb/DEBIAN" "$deb/opt" "$deb/usr/bin" "$deb/usr/share/applications" "$deb/usr/share/icons/hicolor/512x512/apps"
cp -R "$folder" "$deb/opt/qalam"
ln -s /opt/qalam/qalam "$deb/usr/bin/qalam"
ln -s /opt/qalam/qalam-set-default "$deb/usr/bin/qalam-set-default"
sed 's|^Exec=.*|Exec=/opt/qalam/qalam %f|' tools/linux/qalam.desktop > "$deb/usr/share/applications/qalam.desktop"
cp assets/icon.png "$deb/usr/share/icons/hicolor/512x512/apps/qalam.png"
SIZE=$(du -sk "$deb" | cut -f1)
cat > "$deb/DEBIAN/control" <<CTRL
Package: qalam
Version: $VERSION
Section: editors
Priority: optional
Architecture: $DEBARCH
Installed-Size: $SIZE
Depends: libgtk-3-0, libxkbcommon0, libc6
Recommends: xdg-utils
Maintainer: Woosool <hello@woosool.com>
Homepage: https://woosool.com/en/products/qalam
Description: Qalam قلم word processor
 A word processor for .docx files in English, French and Arabic, written in Rust.
 Run qalam-set-default to make it the default app for its file types.
CTRL
# Refresh the menu and file-type caches (registers Qalam in "Open with"; never changes defaults).
for s in postinst postrm; do
  printf '#!/bin/sh\nset -e\ncommand -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true\ncommand -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true\n' > "$deb/DEBIAN/$s"
  chmod 755 "$deb/DEBIAN/$s"
done
dpkg-deb --root-owner-group --build "$deb" "dist/qalam_${VERSION}_${DEBARCH}.deb" >/dev/null

# AppImage, when the tool is available.
if command -v appimagetool >/dev/null 2>&1; then
  appdir=$(mktemp -d)/Qalam.AppDir
  mkdir -p "$appdir/usr/bin"
  cp -R "$folder"/* "$appdir/usr/bin/"
  cp tools/linux/qalam.desktop "$appdir/qalam.desktop"
  cp assets/icon.png "$appdir/qalam.png"
  printf '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/usr/bin/qalam" "$@"\n' > "$appdir/AppRun"
  chmod +x "$appdir/AppRun"
  ARCH=$ARCH appimagetool "$appdir" "$base.AppImage" >/dev/null
fi
echo "Built $(ls dist | grep -E 'linux|\.deb' | tr '\n' ' ')"
