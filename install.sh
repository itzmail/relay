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

if install -m 755 "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME" 2>/dev/null; then
  :
elif command -v sudo >/dev/null 2>&1; then
  echo "Permission denied. Retrying with sudo..."
  sudo install -m 755 "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
else
  # Fallback: install to ~/.local/bin
  INSTALL_DIR="$HOME/.local/bin"
  mkdir -p "$INSTALL_DIR"
  install -m 755 "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
  echo "Installed to $INSTALL_DIR (no sudo available). Ensure it is in your PATH."
fi

echo "relay ${LATEST} installed to ${INSTALL_DIR}/${BIN_NAME}"
echo "Run 'relay init' in your project to get started."
