#!/bin/sh
# Maktab for Linux and ChromeOS (x86_64 or aarch64, built on that machine):
#   dist/maktab/Maktab-<version>-linux-<arch>.tar.gz   portable folder with every app
#   dist/maktab/maktab_<version>_<debarch>.deb         installs to /opt/maktab with menu entries;
#                                                      also the ChromeOS package (Linux on ChromeOS
#                                                      is Debian, and installs .deb files directly)
#   dist/maktab/<App>-<version>-linux-<arch>.zip       each app's update archive
# The shared fonts and dictionaries are hard links between the apps, so the archive and the
# package hold them once.
# Build needs: libgtk-3-dev libxkbcommon-dev libwayland-dev libxcb1-dev libgbm-dev libegl-dev libssl-dev libdbus-1-dev pkg-config dpkg-dev zip
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
VERSION=$(sh tools/maktab/version.sh)
ARCH=$(uname -m)
DEBARCH=$( [ "$ARCH" = aarch64 ] && echo arm64 || echo amd64 )
SHARED="$ROOT/qalam/assets"
DIST="$ROOT/dist/maktab"
mkdir -p "$DIST"
WORK=$(mktemp -d)
FOLDER="$WORK/maktab"
mkdir -p "$FOLDER"
first_shared=""

while IFS='|' read -r slug dir pkg bin name arabic shared bundle <&3; do
  case "$slug" in ''|\#*) continue ;; esac
  echo "== $name ($dir)"
  (cd "$dir" && cargo build --release -p "$pkg" --bin "$bin")
  app="$FOLDER/$slug"
  mkdir -p "$app"
  cp "$dir/target/release/$bin" "$app/"
  cp "$dir/assets/icon.png" "$app/$slug.png"
  cp LICENSE "$app/"
  for f in "$dir/tools/linux/"*; do [ -f "$f" ] && cp "$f" "$app/"; done
  chmod +x "$app/"*-set-default 2>/dev/null || true
  if [ "$shared" = 1 ]; then
    if [ -z "$first_shared" ]; then
      mkdir -p "$app/resources"
      cp -R "$SHARED/fonts" "$SHARED/dict" "$SHARED/i18n" "$SHARED/themes" "$app/resources/"
      first_shared="$app/resources"
    else
      cp -al "$first_shared" "$app/resources"   # hard links: stored once
    fi
  fi
  (cd "$FOLDER" && rm -f "$DIST/$name-$VERSION-linux-$ARCH.zip" && zip -qr "$DIST/$name-$VERSION-linux-$ARCH.zip" "$slug")
done 3< tools/maktab/apps.tsv

tar -C "$WORK" -czf "$DIST/Maktab-$VERSION-linux-$ARCH.tar.gz" maktab

# The Debian package (also for ChromeOS).
deb="$WORK/deb"
mkdir -p "$deb/DEBIAN" "$deb/opt" "$deb/usr/bin" "$deb/usr/share/applications" "$deb/usr/share/icons/hicolor/512x512/apps" "$deb/usr/share/mime/packages"
cp -a "$FOLDER" "$deb/opt/maktab"
while IFS='|' read -r slug dir pkg bin name arabic shared bundle <&3; do
  case "$slug" in ''|\#*) continue ;; esac
  ln -s "/opt/maktab/$slug/$bin" "$deb/usr/bin/$bin"
  [ -f "$FOLDER/$slug/$slug-set-default" ] && ln -s "/opt/maktab/$slug/$slug-set-default" "$deb/usr/bin/$slug-set-default"
  if [ -f "$FOLDER/$slug/$slug.desktop" ]; then
    sed "s|^Exec=$bin |Exec=/opt/maktab/$slug/$bin |" "$FOLDER/$slug/$slug.desktop" > "$deb/usr/share/applications/$slug.desktop"
  fi
  cp "$FOLDER/$slug/$slug.png" "$deb/usr/share/icons/hicolor/512x512/apps/$slug.png"
  for m in "$FOLDER/$slug/"*-mime.xml; do [ -f "$m" ] && cp "$m" "$deb/usr/share/mime/packages/"; done
done 3< tools/maktab/apps.tsv
SIZE=$(du -sk "$deb" | cut -f1)
cat > "$deb/DEBIAN/control" <<CTRL
Package: maktab
Version: $VERSION
Section: editors
Priority: optional
Architecture: $DEBARCH
Installed-Size: $SIZE
Depends: libgtk-3-0, libxkbcommon0, libc6, libdbus-1-3, libssl3 | libssl1.1
Recommends: xdg-utils
Conflicts: qalam, jadwal, sard
Replaces: qalam, jadwal, sard
Maintainer: Woosool <hello@woosool.com>
Homepage: https://woosool.com
Description: Maktab مكتب office suite
 Qalam (documents), Jadwal (spreadsheets), Sard (presentations), Saboura
 (whiteboard), Waraq (PDF) and Barid (mail), in English, French and Arabic.
CTRL
for s in postinst postrm; do
  printf '#!/bin/sh\nset -e\ncommand -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications || true\ncommand -v update-mime-database >/dev/null 2>&1 && update-mime-database /usr/share/mime || true\ncommand -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true\n' > "$deb/DEBIAN/$s"
  chmod 755 "$deb/DEBIAN/$s"
done
dpkg-deb -Zxz -z9 --root-owner-group --build "$deb" "$DIST/maktab_${VERSION}_${DEBARCH}.deb" >/dev/null
rm -rf "$WORK"
ls -lh "$DIST" | grep -E "linux|\.deb"
