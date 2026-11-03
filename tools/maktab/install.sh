#!/bin/sh
# Install (or update) Maktab on macOS, Linux or ChromeOS from the latest GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/woosool/maktab/main/tools/maktab/install.sh | sh
#
# Why this way: the apps aren't signed with a paid Apple Developer ID yet, and macOS refuses
# to open unsigned apps that a browser downloaded ("can't be opened" / "unidentified
# developer"). Files fetched by curl aren't marked as downloaded, so apps installed by this
# script open normally. Nothing here needs a password on macOS; Linux asks for one to install
# the package (or installs into your home folder with --user).
#
# Options: --version 0.1.1 (a given release), --user (Linux: no sudo, into ~/.local),
#          --apps "Qalam Sard" (macOS: only these apps).
set -eu
REPO="woosool/maktab"
VERSION=""
USER_INSTALL=0
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --user) USER_INSTALL=1; shift ;;
    --apps) ONLY="$2"; shift 2 ;;
    *) echo "Unknown option $1" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1m%s\033[0m\n' "$*"; }
fail() { printf 'Maktab: %s\n' "$*" >&2; exit 1; }
command -v curl >/dev/null 2>&1 || fail "curl is needed."

if [ -z "$VERSION" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=30" \
    | grep -o '"tag_name": *"maktab-v[^"]*"' | head -1 | sed 's/.*maktab-v//; s/"//')
  [ -n "$VERSION" ] || fail "couldn't find a Maktab release on github.com/$REPO."
fi
BASE="https://github.com/$REPO/releases/download/maktab-v$VERSION"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
ARCH=$(uname -m | sed 's/arm64/aarch64/; s/amd64/x86_64/')

case "$(uname -s)" in
  Darwin)
    file="Maktab-$VERSION-macos-$ARCH.dmg"
    say "Downloading ${file}…"
    curl -fL --progress-bar -o "$TMP/$file" "$BASE/$file" || fail "no macOS download for $ARCH in release $VERSION."
    mnt="$TMP/mnt"
    mkdir -p "$mnt"
    hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$TMP/$file" >/dev/null
    dest=/Applications
    [ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
    for app in "$mnt"/*.app; do
      name=$(basename "$app" .app)
      if [ -n "$ONLY" ] && ! echo " $ONLY " | grep -qi " $name "; then continue; fi
      say "Installing $name to ${dest}…"
      rm -rf "$dest/$name.app"
      ditto "$app" "$dest/$name.app"
      # Copies made by ditto from a curl download carry no quarantine; clear it anyway.
      xattr -dr com.apple.quarantine "$dest/$name.app" 2>/dev/null || true
    done
    hdiutil detach "$mnt" -quiet || true
    say "Maktab $VERSION is installed in $dest. Open the apps from Launchpad or $dest."
    ;;
  Linux)
    debarch=$( [ "$ARCH" = aarch64 ] && echo arm64 || echo amd64 )
    if [ "$USER_INSTALL" = 0 ] && command -v dpkg >/dev/null 2>&1; then
      file="maktab_${VERSION}_${debarch}.deb"
      say "Downloading ${file}…"
      curl -fL --progress-bar -o "$TMP/$file" "$BASE/$file" || fail "no Linux package for $ARCH in release $VERSION."
      say "Installing (your password may be asked)…"
      if command -v apt-get >/dev/null 2>&1; then sudo apt-get install -y "$TMP/$file"; else sudo dpkg -i "$TMP/$file"; fi
      say "Maktab $VERSION is installed. Find the apps in your applications menu."
    else
      file="Maktab-$VERSION-linux-$ARCH.tar.gz"
      say "Downloading ${file}…"
      curl -fL --progress-bar -o "$TMP/$file" "$BASE/$file" || fail "no Linux download for $ARCH in release $VERSION."
      dest="$HOME/.local/share/maktab"
      rm -rf "$dest" && mkdir -p "$dest" "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/512x512/apps"
      tar -xzf "$TMP/$file" -C "$dest" --strip-components=1
      for d in "$dest"/*/; do
        slug=$(basename "$d")
        for exe in "$d"*; do [ -f "$exe" ] && [ -x "$exe" ] && case "$exe" in *-set-default) ;; *) ln -sf "$exe" "$HOME/.local/bin/$(basename "$exe")" ;; esac; done
        [ -f "$d$slug.png" ] && cp "$d$slug.png" "$HOME/.local/share/icons/hicolor/512x512/apps/"
        [ -f "$d$slug.desktop" ] && sed "s|^Exec=\([a-z]*\) |Exec=$d\1 |" "$d$slug.desktop" > "$HOME/.local/share/applications/$slug.desktop"
      done
      command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$HOME/.local/share/applications" || true
      say "Maktab $VERSION is installed in $dest (commands in ~/.local/bin)."
    fi
    ;;
  *) fail "this script is for macOS, Linux and ChromeOS; on Windows run install.ps1 (see the README)." ;;
esac
