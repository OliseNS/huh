#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

BIN_DIR="${BIN_DIR:-$HOME/.local/bin}"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found; install Rust from https://rustup.rs" >&2
    exit 1
fi

cargo build --release --locked

install -d "$BIN_DIR"
install -m 0755 target/release/idk "$BIN_DIR/idk"

echo "Installed idk to $BIN_DIR/idk"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "note: add $BIN_DIR to your PATH to run 'idk' from anywhere" ;;
esac
