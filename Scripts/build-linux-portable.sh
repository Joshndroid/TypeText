#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/Scripts/version.sh"
CARGO_BIN="${CARGO_BIN:-cargo}"
LINUX_TARGET="${TYPETEXT_LINUX_TARGET:-$(rustc -vV | awk '/host:/ { print $2 }')}"
VERSION="$(typetext_version "$ROOT_DIR")"
export TYPETEXT_VERSION="$VERSION"

LEGACY_DIST_DIR="$ROOT_DIR/dist/TypeText-Linux"
APPDIR="$ROOT_DIR/dist/TypeText.AppDir"
RELEASE_DIR="$ROOT_DIR/target/$LINUX_TARGET/release"
BIN_SOURCE="$RELEASE_DIR/typetext-desktop"
APPIMAGE_PATH="$ROOT_DIR/dist/TypeText-Linux-${LINUX_TARGET}.AppImage"
LINUXDEPLOY_VERSION="${LINUXDEPLOY_VERSION:-continuous}"
LINUXDEPLOY_ARCH="${LINUXDEPLOY_ARCH:-$(uname -m)}"
LINUXDEPLOY_BIN="${LINUXDEPLOY_BIN:-$ROOT_DIR/dist/linuxdeploy-${LINUXDEPLOY_ARCH}.AppImage}"

cd "$ROOT_DIR"
echo "Building TypeText for Linux target: $LINUX_TARGET"
echo "Version: $VERSION"
"$CARGO_BIN" build --release --target "$LINUX_TARGET" -p typetext-desktop

rm -rf "$LEGACY_DIST_DIR"
rm -rf "$APPDIR"
mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
  "$APPDIR/usr/share/typetext"

cp "$BIN_SOURCE" "$APPDIR/usr/bin/TypeText"
chmod +x "$APPDIR/usr/bin/TypeText"

if [[ -f "$ROOT_DIR/icon/typetext-appicon.png" ]]; then
  cp "$ROOT_DIR/icon/typetext-appicon.png" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps/typetext.png"
fi

cat >"$APPDIR/usr/share/applications/typetext.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=TypeText
Comment=Reusable text snippets
Exec=TypeText
Icon=typetext
Terminal=false
Categories=Utility;
EOF

cat >"$APPDIR/usr/share/typetext/build-info.txt" <<EOF
name=TypeText
version=$VERSION
target=$LINUX_TARGET
portable=appimage
entry=usr/bin/TypeText
EOF

if [[ ! -x "$LINUXDEPLOY_BIN" ]]; then
  mkdir -p "$(dirname "$LINUXDEPLOY_BIN")"
  echo "Downloading linuxdeploy for AppImage packaging..."
  curl -fsSL \
    "https://github.com/linuxdeploy/linuxdeploy/releases/download/$LINUXDEPLOY_VERSION/linuxdeploy-${LINUXDEPLOY_ARCH}.AppImage" \
    -o "$LINUXDEPLOY_BIN"
  chmod +x "$LINUXDEPLOY_BIN"
fi

rm -f "$APPIMAGE_PATH"
export APPIMAGE_EXTRACT_AND_RUN=1
export OUTPUT="$APPIMAGE_PATH"
"$LINUXDEPLOY_BIN" \
  --appdir "$APPDIR" \
  --executable "$APPDIR/usr/bin/TypeText" \
  --desktop-file "$APPDIR/usr/share/applications/typetext.desktop" \
  --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/typetext.png" \
  --output appimage
typetext_write_md5_checksum "$APPIMAGE_PATH"

echo "Built $APPIMAGE_PATH"
echo "Run with: $APPIMAGE_PATH"
