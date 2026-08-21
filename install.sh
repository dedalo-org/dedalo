#!/bin/sh
# Install the dedalo CLI from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/4137314/dedalo/main/install.sh | sh
#
# Environment:
#   DEDALO_VERSION   version to install, e.g. v0.1.0 (default: latest)
#   DEDALO_INSTALL   directory to install into (default: ~/.local/bin)
#
# The download is verified against the .sha256 published beside it. If the
# checksum does not match, nothing is installed. A tool that moves money has
# no business skipping that check.

set -eu

REPO="4137314/dedalo"
BIN="dedalo"
INSTALL_DIR="${DEDALO_INSTALL:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "'$1' is required but not installed"; }

detect_target() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os" in
    Linux) os_part="unknown-linux-musl" ;;   # static: no glibc version to match
    Darwin) os_part="apple-darwin" ;;
    MINGW*|MSYS*|CYGWIN*)
      die "on Windows, download the .zip from https://github.com/$REPO/releases" ;;
    *) die "unsupported operating system: $os" ;;
  esac
  case "$arch" in
    x86_64|amd64) arch_part="x86_64" ;;
    arm64|aarch64) arch_part="aarch64" ;;
    *) die "unsupported architecture: $arch" ;;
  esac
  printf '%s-%s\n' "$arch_part" "$os_part"
}

latest_version() {
  # Follow the /releases/latest redirect: no API token, no rate limit surprise.
  url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest") \
    || die "cannot reach GitHub to resolve the latest release"
  version=${url##*/}
  case "$version" in
    v*) printf '%s\n' "$version" ;;
    *) die "could not parse a version from '$url'" ;;
  esac
}

verify_checksum() {
  archive="$1"
  expected_file="$2"
  expected=$(cut -d' ' -f1 < "$expected_file")
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$archive" | cut -d' ' -f1)
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$archive" | cut -d' ' -f1)
  else
    die "neither sha256sum nor shasum is available; cannot verify the download"
  fi
  [ "$actual" = "$expected" ] || die "checksum mismatch: expected $expected, got $actual"
}

main() {
  need curl
  need tar

  target=$(detect_target)
  version="${DEDALO_VERSION:-$(latest_version)}"
  name="$BIN-$version-$target"
  base="https://github.com/$REPO/releases/download/$version"

  say "Installing $BIN $version for $target"

  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM

  curl -fsSL "$base/$name.tar.gz" -o "$tmp/$name.tar.gz" \
    || die "no build for $target in $version — see https://github.com/$REPO/releases"
  curl -fsSL "$base/$name.tar.gz.sha256" -o "$tmp/$name.tar.gz.sha256" \
    || die "no checksum published for $name.tar.gz; refusing to install unverified"

  verify_checksum "$tmp/$name.tar.gz" "$tmp/$name.tar.gz.sha256"
  say "Checksum verified"

  tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
  mkdir -p "$INSTALL_DIR"
  install -m 0755 "$tmp/$name/$BIN" "$INSTALL_DIR/$BIN" 2>/dev/null \
    || { cp "$tmp/$name/$BIN" "$INSTALL_DIR/$BIN" && chmod 0755 "$INSTALL_DIR/$BIN"; }

  say "Installed $INSTALL_DIR/$BIN"

  case ":$PATH:" in
    *":$INSTALL_DIR:"*) "$INSTALL_DIR/$BIN" --version ;;
    *)
      say ""
      say "$INSTALL_DIR is not on your PATH. Add it with:"
      say "  export PATH=\"$INSTALL_DIR:\$PATH\""
      ;;
  esac
}

main "$@"
