#!/usr/bin/env bash

typetext_version() {
  local root_dir="$1"

  printf 'v%s\n' "$(awk -F '"' '/^version = / { print $2; exit }' "$root_dir/Cargo.toml")"
}

typetext_package_version() {
  local version="$1"
  printf '%s\n' "${version#v}"
}

typetext_write_sha256_checksum() {
  local artifact_path="$1"
  local checksum_path="${2:-$artifact_path.sha256}"
  local artifact_dir
  local artifact_name

  artifact_dir="$(cd "$(dirname "$artifact_path")" && pwd)"
  artifact_name="$(basename "$artifact_path")"

  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$artifact_dir" && sha256sum "$artifact_name") >"$checksum_path"
  elif command -v shasum >/dev/null 2>&1; then
    (cd "$artifact_dir" && shasum -a 256 "$artifact_name") >"$checksum_path"
  else
    echo "sha256sum or shasum is required to write a checksum for $artifact_path." >&2
    return 1
  fi

  echo "Wrote $checksum_path"
}
