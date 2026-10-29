#!/bin/sh
# Linux packages (x86_64 or aarch64, built on that machine):
#   dist/Sard-<version>-linux-<arch>.tar.gz   portable folder
#   dist/Sard-<version>-linux-<arch>.zip      the same folder, the update archive (app/src/update.rs)
#   dist/sard_<version>_<debarch>.deb         installs to /opt/sard with a menu entry and file types
#   dist/Sard-<version>-linux-<arch>.AppImage when appimagetool is on PATH
# Making Sard the default app for its formats is opt-in: run `sard-set-default` as the user.
# Build needs: libgtk-3-dev libxkbcommon-dev libwayland-dev libxcb1-dev libpipewire-0.3-dev
#              libgbm-dev libegl-dev libasound2-dev dpkg-dev
set -e
cd "$(dirname "$0")/.."
VERSION=$(sh tools/version.sh)
ARCH=$(uname -m)
DEBARCH=$( [ "$ARCH" = aarch64 ] && echo arm64 || echo amd64 )
SHARED=../word/assets   # fonts, dictionary, translations and themes are shared with Qalam
cargo build --release -p slides-app --bin sard
mkdir -p dist
base="dist/Sard-$VERSION-linux-$ARCH"

# Portable folder: sard + resources/.
folder=dist/sard
rm -rf "$folder" && mkdir -p "$folder/resources"
cp target/release/sard "$folder/"
cp -R "$SHARED/fonts" "$SHARED/dict" "$SHARED/i18n" "$SHARED/themes" "$folder/resources/"
cp assets/icon.png "$folder/sard.png"
cp tools/linux/sard.desktop tools/linux/sard-set-default "$folder/"
chmod +x "$folder/sard-set-default"
cp LICENSE "$folder/"
tar -C dist -czf "$base.tar.gz" sard
(cd dist && rm -f "$(basename "$base").zip" && zip -qr "$(basename "$base").zip" sard)

# Debian package.
deb=$(mktemp -d)/sard
mkdir -p "$deb/DEBIAN" "$deb/opt" "$deb/usr/bin" "$deb/usr/share/applications" "$deb/usr/share/icons/hicolor/512x512/apps"
cp -R "$folder" "$deb/opt/sard"
ln -s /opt/sard/sard "$deb/usr/bin/sard"
ln -s /opt/sard/sard-set-default "$deb/usr/bin/sard-set-default"
sed 's|^Exec=.*|Exec=/opt/sard/sard %f|' tools/linux/sard.desktop > "$deb/usr/share/applications/sard.desktop"
cp assets/icon.png "$deb/usr/share/icons/hicolor/512x512/apps/sard.png"
SIZE=$(du -sk "$deb" | cut -f1)
cat > "$deb/DEBIAN/control" <<CTRL
Package: sard
Version: $VERSION
Section: editors
Priority: optional
Architecture: $DEBARCH
Installed-Size: $SIZE
Depends: libgtk-3-0, libxkbcommon0, libc6, libasound2 | libasound2t64
Recommends: xdg-utils
Maintainer: Woosool <hello@woosool.com>
Homepage: https://woosool.com/en/products/sard
Description: Sard سرد presentation editor
 A presentation editor for .pptx files in English, French and Arabic, written in Rust.
 Run sard-set-default to make it the default app for its file types.
CTRL
# Refresh the menu and file-type caches (registers Sard in "Open with"; never changes defaults).
for s in postinst postrm; do
  printf '#!/bin/sh\nset -e\ncommand -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true\ncommand -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true\n' > "$deb/DEBIAN/$s"
  chmod 755 "$deb/DEBIAN/$s"
done
dpkg-deb --root-owner-group --build "$deb" "dist/sard_${VERSION}_${DEBARCH}.deb" >/dev/null
rm -rf "$(dirname "$deb")"

# AppImage, when the tool is available. (Integration tools such as AppImageLauncher install its
# .desktop with the MimeType list; then `xdg-mime default <their desktop name> <mime>` opts in.)
if command -v appimagetool >/dev/null 2>&1; then
  appdir=$(mktemp -d)/Sard.AppDir
  mkdir -p "$appdir/usr/bin"
  cp -R "$folder"/* "$appdir/usr/bin/"
  cp tools/linux/sard.desktop "$appdir/sard.desktop"
  cp assets/icon.png "$appdir/sard.png"
  printf '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/usr/bin/sard" "$@"\n' > "$appdir/AppRun"
  chmod +x "$appdir/AppRun"
  ARCH=$ARCH appimagetool "$appdir" "$base.AppImage" >/dev/null
  rm -rf "$(dirname "$appdir")"
fi
echo "Built $(ls dist | grep -E 'linux|\.deb' | tr '\n' ' ')"
