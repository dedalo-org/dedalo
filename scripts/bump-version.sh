#!/usr/bin/env bash
# Bump the workspace version everywhere it is written down.
#
# The workspace keeps one version for both crates, so a release is a single
# number and a single tag. Three places have to agree, and this script is the
# only thing allowed to change them:
#
#   1. [workspace.package] version   — what both crates inherit
#   2. [workspace.dependencies]      — dedalo-cli's pin on dedalo-core
#   3. Cargo.lock                    — refreshed via `cargo update`
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

# Only the workspace version line and the internal path dependency: never a
# third-party version, and never a version inside a doc example.
sed -i.bak -E "0,/^version = \"$current\"$/s//version = \"$next\"/" Cargo.toml
sed -i.bak -E "s|^(dedalo-core = \{ path = \"crates/dedalo-core\", version = )\"$current\"|\1\"$next\"|" Cargo.toml
rm -f Cargo.toml.bak

# Keep the lockfile in step without touching any other dependency.
cargo update --workspace --quiet

changed=$(grep -c "\"$next\"" Cargo.toml || true)
if [ "$changed" -lt 2 ]; then
  echo "expected two version occurrences after the bump, found $changed" >&2
  exit 1
fi

printf '%s\n' "$next"
