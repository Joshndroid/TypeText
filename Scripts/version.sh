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
