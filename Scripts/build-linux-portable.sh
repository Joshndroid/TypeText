#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_BIN="${CARGO_BIN:-cargo}"
LINUX_TARGET="${TYPETEXT_LINUX_TARGET:-$(rustc -vV | awk '/host:/ { print $2 }')}"

DIST_DIR="$ROOT_DIR/dist/TypeText-Linux"
DATA_DIR="$DIST_DIR/data"
RELEASE_DIR="$ROOT_DIR/target/$LINUX_TARGET/release"
BIN_SOURCE="$RELEASE_DIR/typetext-desktop"
BIN_DEST="$DIST_DIR/TypeText"
ARCHIVE_PATH="$ROOT_DIR/dist/TypeText-Linux-${LINUX_TARGET}.tar.gz"

cd "$ROOT_DIR"
echo "Building TypeText for Linux target: $LINUX_TARGET"
"$CARGO_BIN" build --release --target "$LINUX_TARGET" -p typetext-desktop

rm -rf "$DIST_DIR"
mkdir -p "$DATA_DIR"

cp "$BIN_SOURCE" "$BIN_DEST"
chmod +x "$BIN_DEST"

if [[ -f "$ROOT_DIR/icon/typetext-appicon.png" ]]; then
  cp "$ROOT_DIR/icon/typetext-appicon.png" "$DIST_DIR/TypeText.png"
fi

if [[ -f "$ROOT_DIR/examples/snippets.json" ]]; then
  cp "$ROOT_DIR/examples/snippets.json" "$DATA_DIR/snippets.json"
fi

if [[ -f "$ROOT_DIR/examples/settings.json" ]]; then
  cp "$ROOT_DIR/examples/settings.json" "$DATA_DIR/settings.json"
fi

cat >"$DIST_DIR/build-info.txt" <<EOF
name=TypeText
version=0.1.0
target=$LINUX_TARGET
portable=true
entry=TypeText
EOF

rm -f "$ARCHIVE_PATH"
tar -C "$ROOT_DIR/dist" -czf "$ARCHIVE_PATH" "TypeText-Linux"

echo "Built $DIST_DIR"
echo "Archived $ARCHIVE_PATH"
echo "Run with: $BIN_DEST"
