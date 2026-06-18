#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/Scripts/version.sh"
VERSION="$(typetext_version "$ROOT_DIR")"
PACKAGE_VERSION="$(typetext_package_version "$VERSION")"
export TYPETEXT_VERSION="$VERSION"
ARCH="${TYPETEXT_DEB_ARCH:-amd64}"
LINUX_TARGET="${TYPETEXT_LINUX_TARGET:-$(rustc -vV | awk '/host:/ { print $2 }')}"
PACKAGE_ROOT="$ROOT_DIR/dist/deb-root"
DEB_PATH="$ROOT_DIR/dist/typetext_${PACKAGE_VERSION}_${ARCH}.deb"
BIN_SOURCE="$ROOT_DIR/target/$LINUX_TARGET/release/typetext-desktop"

"$ROOT_DIR/Scripts/build-linux-portable.sh"

rm -rf "$PACKAGE_ROOT"
mkdir -p \
  "$PACKAGE_ROOT/DEBIAN" \
  "$PACKAGE_ROOT/usr/bin" \
  "$PACKAGE_ROOT/usr/lib/typetext" \
  "$PACKAGE_ROOT/usr/share/applications" \
  "$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps"

cp "$BIN_SOURCE" "$PACKAGE_ROOT/usr/lib/typetext/TypeText"
chmod 0755 "$PACKAGE_ROOT/usr/lib/typetext/TypeText"
ln -s ../lib/typetext/TypeText "$PACKAGE_ROOT/usr/bin/typetext"

if [[ -f "$ROOT_DIR/icon/TypeText.iconset/icon_256x256.png" ]]; then
  cp "$ROOT_DIR/icon/TypeText.iconset/icon_256x256.png" \
    "$PACKAGE_ROOT/usr/share/icons/hicolor/256x256/apps/typetext.png"
fi

cat >"$PACKAGE_ROOT/usr/share/applications/typetext.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=TypeText
Comment=Reusable text snippets
Exec=typetext
Icon=typetext
Terminal=false
Categories=Utility;
EOF

cat >"$PACKAGE_ROOT/DEBIAN/control" <<EOF
Package: typetext
Version: $PACKAGE_VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: TypeText <noreply@example.com>
Depends: libc6, libgcc-s1, libx11-6, libxcb1, libxkbcommon0, libwayland-client0, libasound2 | libasound2t64, libgtk-3-0 | libgtk-3-0t64, libappindicator3-1 | libayatana-appindicator3-1
Description: Reusable text snippets desktop app
 TypeText stores reusable snippets and inserts them into the active app.
EOF

dpkg-deb --build "$PACKAGE_ROOT" "$DEB_PATH"
typetext_write_sha256_checksum "$DEB_PATH"

echo "Built $DEB_PATH"
