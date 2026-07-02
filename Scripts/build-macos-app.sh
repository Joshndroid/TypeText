#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/Scripts/version.sh"
CARGO_BIN="${CARGO_BIN:-cargo}"
MACOS_TARGET="${MACOS_TARGET:-aarch64-apple-darwin}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
CODESIGN_TIMESTAMP="${CODESIGN_TIMESTAMP:-1}"
CODESIGN_TIMEOUT_SECONDS="${CODESIGN_TIMEOUT_SECONDS:-2700}"
NOTARIZE="${NOTARIZE:-0}"
APPLE_NOTARY_PROFILE="${APPLE_NOTARY_PROFILE:-}"
APPLE_ID="${APPLE_ID:-}"
APPLE_TEAM_ID="${APPLE_TEAM_ID:-}"
APPLE_APP_PASSWORD="${APPLE_APP_PASSWORD:-${APPLE_PASSWORD:-}}"
VERSION="$(typetext_version "$ROOT_DIR")"
PACKAGE_VERSION="$(typetext_package_version "$VERSION")"

set_notarytool_args() {
  if [[ -n "$APPLE_NOTARY_PROFILE" ]]; then
    NOTARY_ARGS=(--keychain-profile "$APPLE_NOTARY_PROFILE")
    return
  fi

  if [[ -z "$APPLE_ID" || -z "$APPLE_TEAM_ID" || -z "$APPLE_APP_PASSWORD" ]]; then
    echo "Set APPLE_NOTARY_PROFILE or APPLE_ID, APPLE_TEAM_ID, and APPLE_APP_PASSWORD to notarize." >&2
    exit 1
  fi

  NOTARY_ARGS=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD")
}

run_codesign() {
  if [[ "$CODESIGN_TIMEOUT_SECONDS" != "0" ]] && command -v perl >/dev/null 2>&1; then
    perl -e 'alarm shift @ARGV; exec @ARGV or die "exec failed: $!\n"' \
      "$CODESIGN_TIMEOUT_SECONDS" \
      codesign "$@"
  else
    codesign "$@"
  fi
}

codesign_timestamp_arg() {
  if [[ "$CODESIGN_TIMESTAMP" == "1" ]]; then
    printf '%s' "--timestamp"
  elif [[ "$CODESIGN_TIMESTAMP" == "0" ]]; then
    printf '%s' "--timestamp=none"
  else
    echo "CODESIGN_TIMESTAMP must be 1 or 0." >&2
    exit 1
  fi
}

if [[ "$NOTARIZE" == "1" ]]; then
  if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
    echo "NOTARIZE=1 requires CODESIGN_IDENTITY='Developer ID Application: ...'." >&2
    exit 1
  fi
  if [[ "$CODESIGN_IDENTITY" != Developer\ ID\ Application:* ]]; then
    echo "Direct macOS releases require a Developer ID Application certificate, not '$CODESIGN_IDENTITY'." >&2
    exit 1
  fi
fi

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
echo "Version: $VERSION"
echo "Target: $MACOS_TARGET"
"$CARGO_BIN" build --release --target "$MACOS_TARGET" -p typetext-desktop --locked

APP_DIR="$ROOT_DIR/dist/TypeText.app"
ZIP_PATH="$ROOT_DIR/dist/TypeText-macOS.zip"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
BINARY_PATH="$ROOT_DIR/target/$MACOS_TARGET/release/typetext-desktop"
ENTITLEMENTS_PATH="$ROOT_DIR/apps/typetext-desktop/macos/TypeText.entitlements"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

cp "$BINARY_PATH" "$MACOS_DIR/TypeText"
chmod +x "$MACOS_DIR/TypeText"
sed "s/__TYPETEXT_PACKAGE_VERSION__/$PACKAGE_VERSION/g" \
  "$ROOT_DIR/apps/typetext-desktop/macos/Info.plist" \
  >"$CONTENTS_DIR/Info.plist"
printf 'APPL????' >"$CONTENTS_DIR/PkgInfo"

if [[ -f "$ROOT_DIR/icon/TypeText.icns" ]]; then
  cp "$ROOT_DIR/icon/TypeText.icns" "$RESOURCES_DIR/TypeText.icns"
fi

if ! file "$MACOS_DIR/TypeText" | grep -q "arm64"; then
  file "$MACOS_DIR/TypeText" >&2
  echo "Expected a macOS arm64 executable in $MACOS_DIR/TypeText." >&2
  exit 1
fi

if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
  echo "Ad-hoc signing TypeText.app for local testing."
  run_codesign --force --sign "$CODESIGN_IDENTITY" "$MACOS_DIR/TypeText"
  run_codesign --force --sign "$CODESIGN_IDENTITY" "$APP_DIR"
else
  echo "Developer ID signing TypeText executable."
  TIMESTAMP_ARG="$(codesign_timestamp_arg)"
  run_codesign \
    --force \
    --options runtime \
    "$TIMESTAMP_ARG" \
    --entitlements "$ENTITLEMENTS_PATH" \
    --sign "$CODESIGN_IDENTITY" \
    "$MACOS_DIR/TypeText"

  echo "Developer ID signing TypeText.app."
  run_codesign \
    --force \
    --options runtime \
    "$TIMESTAMP_ARG" \
    --sign "$CODESIGN_IDENTITY" \
    "$APP_DIR"
fi
codesign --verify --strict --verbose=2 "$MACOS_DIR/TypeText"
codesign --verify --strict --verbose=2 "$APP_DIR"

rm -f "$ZIP_PATH"
ditto -c -k --keepParent "$APP_DIR" "$ZIP_PATH"

if [[ "$NOTARIZE" == "1" ]]; then
  NOTARY_ARGS=()
  set_notarytool_args
  echo "Submitting $ZIP_PATH for Apple notarization."
  xcrun notarytool submit "$ZIP_PATH" "${NOTARY_ARGS[@]}" --wait
  echo "Stapling notarization ticket to $APP_DIR."
  xcrun stapler staple "$APP_DIR"
  xcrun stapler validate "$APP_DIR"
  codesign --verify --strict --verbose=2 "$APP_DIR"

  rm -f "$ZIP_PATH"
  ditto -c -k --keepParent "$APP_DIR" "$ZIP_PATH"
fi

typetext_write_sha256_checksum "$ZIP_PATH"

echo "Built $APP_DIR"
echo "Archived $ZIP_PATH"
echo "Open with: open \"$APP_DIR\""
