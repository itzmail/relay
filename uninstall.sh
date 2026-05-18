#!/bin/sh
set -e

BIN_NAME="relay"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BIN_PATH="$INSTALL_DIR/$BIN_NAME"

if [ ! -f "$BIN_PATH" ]; then
  echo "relay not found at $BIN_PATH"
  exit 0
fi

rm "$BIN_PATH"
echo "relay removed from $BIN_PATH"
