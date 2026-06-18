#!/usr/bin/env bash

typetext_version() {
  local root_dir="$1"

  if [[ -n "${TYPETEXT_VERSION:-}" ]]; then
    printf '%s\n' "$TYPETEXT_VERSION"
    return 0
  fi

  local tag
  tag="$(git -C "$root_dir" describe --tags --exact-match 2>/dev/null || true)"
  if [[ -n "$tag" ]]; then
    printf '%s\n' "$tag"
    return 0
  fi

  if [[ -f "$root_dir/VERSION" ]]; then
    tr -d '[:space:]' <"$root_dir/VERSION"
    printf '\n'
    return 0
  fi

  awk -F '"' '/^version = / { print $2; exit }' "$root_dir/Cargo.toml"
}

typetext_package_version() {
  local version="$1"
  printf '%s\n' "${version#v}"
}

typetext_write_md5_checksum() {
  local artifact_path="$1"
  local checksum_path="${2:-$artifact_path.md5}"
  local artifact_dir
  local artifact_name

  artifact_dir="$(cd "$(dirname "$artifact_path")" && pwd)"
  artifact_name="$(basename "$artifact_path")"

  if command -v md5sum >/dev/null 2>&1; then
    (cd "$artifact_dir" && md5sum "$artifact_name") >"$checksum_path"
  elif command -v md5 >/dev/null 2>&1; then
    (cd "$artifact_dir" && md5 -r "$artifact_name") >"$checksum_path"
  else
    echo "md5sum or md5 is required to write a checksum for $artifact_path." >&2
    return 1
  fi

  echo "Wrote $checksum_path"
}
