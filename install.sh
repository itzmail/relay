#!/bin/sh
set -e

REPO="itzmail/relay"
BIN_NAME="relay"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and arch
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64) ASSET="relay-linux-x86_64.tar.gz" ;;
      *) echo "Unsupported arch: $ARCH" && exit 1 ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      arm64)  ASSET="relay-macos-aarch64.tar.gz" ;;
      x86_64) ASSET="relay-macos-x86_64.tar.gz" ;;
      *) echo "Unsupported arch: $ARCH" && exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

# Get latest release tag
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$LATEST" ]; then
  echo "Failed to fetch latest release"
  exit 1
fi

echo "Installing relay ${LATEST}..."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

URL="https://github.com/${REPO}/releases/download/${LATEST}/${ASSET}"
curl -fsSL "$URL" -o "$TMP/$ASSET"
tar xzf "$TMP/$ASSET" -C "$TMP"

install -m 755 "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"

echo "relay ${LATEST} installed to ${INSTALL_DIR}/${BIN_NAME}"
echo "Run 'relay setup claude-code --global' to get started."
