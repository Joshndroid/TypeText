#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/Scripts/version.sh"
APP_DIR="$ROOT_DIR/dist/TypeText.app"
DMG_ROOT="$ROOT_DIR/dist/dmg-root"
DMG_PATH="$ROOT_DIR/dist/TypeText-macOS.dmg"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
CODESIGN_TIMESTAMP="${CODESIGN_TIMESTAMP:-1}"
CODESIGN_TIMEOUT_SECONDS="${CODESIGN_TIMEOUT_SECONDS:-2700}"
NOTARIZE="${NOTARIZE:-0}"
APPLE_NOTARY_PROFILE="${APPLE_NOTARY_PROFILE:-}"
APPLE_ID="${APPLE_ID:-}"
APPLE_TEAM_ID="${APPLE_TEAM_ID:-}"
APPLE_APP_PASSWORD="${APPLE_APP_PASSWORD:-${APPLE_PASSWORD:-}}"

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

"$ROOT_DIR/Scripts/build-macos-app.sh"

rm -rf "$DMG_ROOT"
mkdir -p "$DMG_ROOT"
cp -R "$APP_DIR" "$DMG_ROOT/TypeText.app"
ln -s /Applications "$DMG_ROOT/Applications"

rm -f "$DMG_PATH"
hdiutil create \
  -volname "TypeText" \
  -srcfolder "$DMG_ROOT" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

rm -rf "$DMG_ROOT"

if [[ "$CODESIGN_IDENTITY" != "-" ]]; then
  TIMESTAMP_ARG="$(codesign_timestamp_arg)"
  run_codesign --force "$TIMESTAMP_ARG" --sign "$CODESIGN_IDENTITY" "$DMG_PATH"
  codesign --verify --verbose=2 "$DMG_PATH"
fi

if [[ "$NOTARIZE" == "1" ]]; then
  NOTARY_ARGS=()
  set_notarytool_args
  echo "Submitting $DMG_PATH for Apple notarization."
  xcrun notarytool submit "$DMG_PATH" "${NOTARY_ARGS[@]}" --wait
  echo "Stapling notarization ticket to $DMG_PATH."
  xcrun stapler staple "$DMG_PATH"
  xcrun stapler validate "$DMG_PATH"
fi

typetext_write_md5_checksum "$DMG_PATH"

echo "Built $DMG_PATH"
