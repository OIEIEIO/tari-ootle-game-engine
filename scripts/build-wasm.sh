#!/usr/bin/env bash
# Ootle Asset Game | scripts/build-wasm.sh
# Build the standalone Tari Ootle controller WASM; never deploy implicitly.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/templates/treasury_controller"
rustup target list --installed | grep -qx 'wasm32-unknown-unknown' || { echo 'Missing wasm target: rustup target add wasm32-unknown-unknown'; exit 1; }
cargo build --target wasm32-unknown-unknown --release
WASM="$PWD/target/wasm32-unknown-unknown/release/ootle_treasury_controller.wasm"
ls -lh "$WASM"
echo 'Build complete. No blockchain transaction has been submitted.'
# End of scripts/build-wasm.sh
