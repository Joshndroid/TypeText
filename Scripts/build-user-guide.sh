#!/usr/bin/env bash
set -euo pipefail

VERSION=""
SOURCE=""
OUTPUT_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --source) SOURCE="$2"; shift 2 ;;
    --output-dir) OUTPUT_DIR="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done
if [[ -z "$VERSION" || -z "$SOURCE" || -z "$OUTPUT_DIR" ]]; then
  echo "Usage: build-user-guide.sh --version VERSION --source FILE --output-dir DIR" >&2
  exit 2
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT_DIR/docs/user-guide.template.html"
HTML_PATH="$OUTPUT_DIR/TypeText-User-Guide.html"
PDF_PATH="$OUTPUT_DIR/TypeText-User-Guide.pdf"
mkdir -p "$OUTPUT_DIR"

awk -v version="$VERSION" -v source="$SOURCE" '
  $0 == "{{GUIDE_SOURCE}}" {
    while ((getline line < source) > 0) {
      gsub(/\{\{VERSION\}\}/, version, line)
      print line
    }
    close(source)
    next
  }
  { gsub(/\{\{VERSION\}\}/, version); print }
' "$TEMPLATE" > "$HTML_PATH"

BROWSER=""
for candidate in \
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge" \
  "$(command -v google-chrome 2>/dev/null || true)" \
  "$(command -v chromium 2>/dev/null || true)"; do
  if [[ -n "$candidate" && -x "$candidate" ]]; then
    BROWSER="$candidate"
    break
  fi
done
if [[ -z "$BROWSER" ]]; then
  echo "PDF generation requires Google Chrome, Microsoft Edge, or Chromium." >&2
  exit 1
fi

PROFILE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/typetext-guide.XXXXXX")"
trap 'rm -rf "$PROFILE_DIR"' EXIT
"$BROWSER" --headless=new --disable-gpu --no-first-run \
  --no-default-browser-check --no-pdf-header-footer --print-to-pdf-no-header \
  "--user-data-dir=$PROFILE_DIR" "--print-to-pdf=$PDF_PATH" "file://$HTML_PATH"
[[ -f "$PDF_PATH" ]] || { echo "Browser PDF generation failed." >&2; exit 1; }

echo "Built $HTML_PATH"
echo "Built $PDF_PATH"
