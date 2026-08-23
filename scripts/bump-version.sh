#!/usr/bin/env bash
# Bump the crate version everywhere it is written down.
#
# The library and the binary are one crate, so a release is a single number
# and a single tag. Two places have to agree, and this script is the only
# thing allowed to change them:
#
#   1. [package] version   — the crate itself
#   2. Cargo.lock          — refreshed via `cargo update`
#
# The dev-dependency on this same path carries no version, so it needs no
# bump; neither do the `version = "0.1"` pins inside doc examples, which name
# a compatible range rather than this release.
#
# Usage:
#   scripts/bump-version.sh patch|minor|major|<explicit-version> [--dry-run]
#
# Prints the new version on stdout and nothing else, so callers can capture it.

set -euo pipefail

cd "$(dirname "$0")/.."

level="${1:-}"
dry_run="${2:-}"

if [ -z "$level" ]; then
  echo "usage: $0 patch|minor|major|<version> [--dry-run]" >&2
  exit 2
fi

current=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
if ! printf '%s' "$current" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "cannot read a semver version from Cargo.toml (found '$current')" >&2
  exit 1
fi

IFS='.' read -r major minor patch <<< "$current"

case "$level" in
  major) next="$((major + 1)).0.0" ;;
  minor) next="$major.$((minor + 1)).0" ;;
  patch) next="$major.$minor.$((patch + 1))" ;;
  *)
    if ! printf '%s' "$level" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
      echo "'$level' is neither a bump level nor a semver version" >&2
      exit 2
    fi
    next="$level"
    ;;
esac

if [ "$next" = "$current" ]; then
  echo "version is already $next" >&2
  exit 1
fi

if [ "$dry_run" = "--dry-run" ]; then
  printf '%s\n' "$next"
  exit 0
fi

# Only the package version line: never a third-party version, and never a
# version inside a doc example.
sed -i.bak -E "0,/^version = \"$current\"$/s//version = \"$next\"/" Cargo.toml
rm -f Cargo.toml.bak

# Keep the lockfile in step without touching any other dependency.
cargo update --package dedalo --quiet

if ! grep -q "^version = \"$next\"$" Cargo.toml; then
  echo "the version line did not change to $next" >&2
  exit 1
fi

printf '%s\n' "$next"
