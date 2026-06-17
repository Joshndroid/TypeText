#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_BIN="${CARGO_BIN:-cargo}"

if ! command -v "$CARGO_BIN" >/dev/null 2>&1; then
  RUSTUP_CARGO="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo"
  if [[ -x "$RUSTUP_CARGO" ]]; then
    CARGO_BIN="$RUSTUP_CARGO"
  else
    echo "cargo not found. Install Rust or set CARGO_BIN=/path/to/cargo." >&2
    exit 1
  fi
fi

RUSTUP_TOOLCHAIN_BIN="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin"
if [[ -d "$RUSTUP_TOOLCHAIN_BIN" ]]; then
  export PATH="$RUSTUP_TOOLCHAIN_BIN:$PATH"
fi

cd "$ROOT_DIR"
"$CARGO_BIN" build --release -p typetext-desktop

APP_DIR="$ROOT_DIR/dist/TypeText.app"
ZIP_PATH="$ROOT_DIR/dist/TypeText-macOS.zip"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$MACOS_DIR/data"

cp "$ROOT_DIR/target/release/typetext-desktop" "$MACOS_DIR/TypeText"
chmod +x "$MACOS_DIR/TypeText"
cp "$ROOT_DIR/apps/typetext-desktop/macos/Info.plist" "$CONTENTS_DIR/Info.plist"

if [[ -f "$ROOT_DIR/icon/TypeText.icns" ]]; then
  cp "$ROOT_DIR/icon/TypeText.icns" "$RESOURCES_DIR/TypeText.icns"
fi

if [[ -f "$ROOT_DIR/examples/snippets.json" ]]; then
  cp "$ROOT_DIR/examples/snippets.json" "$MACOS_DIR/data/snippets.json"
fi

if [[ -f "$ROOT_DIR/examples/settings.json" ]]; then
  cp "$ROOT_DIR/examples/settings.json" "$MACOS_DIR/data/settings.json"
fi

rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_DIR" "$ZIP_PATH"

echo "Built $APP_DIR"
echo "Archived $ZIP_PATH"
echo "Open with: open \"$APP_DIR\""
