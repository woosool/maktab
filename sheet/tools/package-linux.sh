#!/bin/sh
# Linux packages (x86_64 or aarch64, built on that machine):
#   dist/Jadwal-<version>-linux-<arch>.tar.gz   portable folder
#   dist/Jadwal-<version>-linux-<arch>.zip      the same folder, the update archive (app/src/update.rs)
#   dist/jadwal_<version>_<debarch>.deb         installs to /opt/jadwal with a menu entry and file types
#   dist/Jadwal-<version>-linux-<arch>.AppImage when appimagetool is on PATH
# Making Jadwal the default app for its formats is opt-in: run `jadwal-set-default` as the user.
# Build needs: libgtk-3-dev libxkbcommon-dev libwayland-dev libxcb1-dev libpipewire-0.3-dev
#              libgbm-dev libegl-dev libasound2-dev dpkg-dev
set -e
cd "$(dirname "$0")/.."
VERSION=$(sh tools/version.sh)
ARCH=$(uname -m)
DEBARCH=$( [ "$ARCH" = aarch64 ] && echo arm64 || echo amd64 )
SHARED=../word/assets   # fonts, dictionary, translations and themes are shared with Qalam
cargo build --release -p sheet-app --bin jadwal
mkdir -p dist
base="dist/Jadwal-$VERSION-linux-$ARCH"

# Portable folder: jadwal + resources/.
folder=dist/jadwal
rm -rf "$folder" && mkdir -p "$folder/resources"
cp target/release/jadwal "$folder/"
cp -R "$SHARED/fonts" "$SHARED/dict" "$SHARED/i18n" "$SHARED/themes" "$folder/resources/"
cp assets/icon.png "$folder/jadwal.png"
cp tools/linux/jadwal.desktop tools/linux/jadwal-set-default "$folder/"
chmod +x "$folder/jadwal-set-default"
cp LICENSE "$folder/"
tar -C dist -czf "$base.tar.gz" jadwal
(cd dist && rm -f "$(basename "$base").zip" && zip -qr "$(basename "$base").zip" jadwal)

# Debian package.
deb=$(mktemp -d)/jadwal
mkdir -p "$deb/DEBIAN" "$deb/opt" "$deb/usr/bin" "$deb/usr/share/applications" "$deb/usr/share/icons/hicolor/512x512/apps"
cp -R "$folder" "$deb/opt/jadwal"
ln -s /opt/jadwal/jadwal "$deb/usr/bin/jadwal"
ln -s /opt/jadwal/jadwal-set-default "$deb/usr/bin/jadwal-set-default"
sed 's|^Exec=.*|Exec=/opt/jadwal/jadwal %f|' tools/linux/jadwal.desktop > "$deb/usr/share/applications/jadwal.desktop"
cp assets/icon.png "$deb/usr/share/icons/hicolor/512x512/apps/jadwal.png"
SIZE=$(du -sk "$deb" | cut -f1)
cat > "$deb/DEBIAN/control" <<CTRL
Package: jadwal
Version: $VERSION
Section: editors
Priority: optional
Architecture: $DEBARCH
Installed-Size: $SIZE
Depends: libgtk-3-0, libxkbcommon0, libc6
Recommends: xdg-utils
Maintainer: Woosool <hello@woosool.com>
Homepage: https://woosool.com/en/products/jadwal
Description: Jadwal جدول spreadsheet
 A spreadsheet editor for .xlsx files in English, French and Arabic, written in Rust.
 Run jadwal-set-default to make it the default app for its file types.
CTRL
# Refresh the menu and file-type caches (registers Jadwal in "Open with"; never changes defaults).
for s in postinst postrm; do
  printf '#!/bin/sh\nset -e\ncommand -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true\ncommand -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true\n' > "$deb/DEBIAN/$s"
  chmod 755 "$deb/DEBIAN/$s"
done
dpkg-deb --root-owner-group --build "$deb" "dist/jadwal_${VERSION}_${DEBARCH}.deb" >/dev/null
rm -rf "$(dirname "$deb")"

# AppImage, when the tool is available. (Integration tools such as AppImageLauncher install its
# .desktop with the MimeType list; then `xdg-mime default <their desktop name> <mime>` opts in.)
if command -v appimagetool >/dev/null 2>&1; then
  appdir=$(mktemp -d)/Jadwal.AppDir
  mkdir -p "$appdir/usr/bin"
  cp -R "$folder"/* "$appdir/usr/bin/"
  cp tools/linux/jadwal.desktop "$appdir/jadwal.desktop"
  cp assets/icon.png "$appdir/jadwal.png"
  printf '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/usr/bin/jadwal" "$@"\n' > "$appdir/AppRun"
  chmod +x "$appdir/AppRun"
  ARCH=$ARCH appimagetool "$appdir" "$base.AppImage" >/dev/null
  rm -rf "$(dirname "$appdir")"
fi
echo "Built $(ls dist | grep -E 'linux|\.deb' | tr '\n' ' ')"
