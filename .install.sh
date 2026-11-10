#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
BINARY="${HUH_BINARY:-target/release/huh}"

if [[ ! -f "$BINARY" ]]; then
    echo "error: prebuilt binary not found at $BINARY" >&2
    echo "build one first with: cargo build --release" >&2
    exit 1
fi

install -d "$BIN_DIR"
install -m 0755 "$BINARY" "$BIN_DIR/huh"

echo "Installed huh to $BIN_DIR/huh"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "note: add $BIN_DIR to your PATH to run 'huh' from anywhere" ;;
esac
