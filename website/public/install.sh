#!/usr/bin/env bash
# otell install script
# Usage: curl -fsSL https://otell.dev/install.sh | sh

set -euo pipefail

REPO="runmat-org/otell"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

need_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    printf "error: required command not found: %s\n" "$1" >&2
    exit 1
  fi
}

need_cmd curl
need_cmd tar

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64) TARGET_SUFFIX="linux-x86_64" ;;
      *)
        printf "error: unsupported Linux architecture: %s\n" "$ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64) TARGET_SUFFIX="macos-x86_64" ;;
      arm64|aarch64) TARGET_SUFFIX="macos-aarch64" ;;
      *)
        printf "error: unsupported macOS architecture: %s\n" "$ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    printf "error: unsupported OS: %s\n" "$OS" >&2
    printf "Try manual install from https://github.com/%s/releases\n" "$REPO" >&2
    exit 1
    ;;
esac

printf "Fetching latest otell release metadata...\n"
LATEST_JSON="$(curl -fsSL "$API_URL")"
TAG="$(printf "%s" "$LATEST_JSON" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"

if [ -z "$TAG" ]; then
  printf "error: failed to resolve latest release tag\n" >&2
  exit 1
fi

ASSET="otell-${TAG}-${TARGET_SUFFIX}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

printf "Downloading %s...\n" "$ASSET"
curl -fL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET"

printf "Extracting archive...\n"
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"

BIN_PATH="$TMP_DIR/otell-${TAG}-${TARGET_SUFFIX}/otell"
if [ ! -f "$BIN_PATH" ]; then
  printf "error: extracted binary not found at %s\n" "$BIN_PATH" >&2
  exit 1
fi

if [ -w "/usr/local/bin" ]; then
  INSTALL_DIR="/usr/local/bin"
else
  INSTALL_DIR="$HOME/.local/bin"
  mkdir -p "$INSTALL_DIR"
fi

install -m 755 "$BIN_PATH" "$INSTALL_DIR/otell"

printf "Installed otell to %s/otell\n" "$INSTALL_DIR"
printf "Run: otell intro\n"

case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    printf "\nNote: %s is not currently in PATH.\n" "$INSTALL_DIR"
    printf "Add it, then open a new shell.\n"
    ;;
esac
