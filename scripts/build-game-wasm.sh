#!/usr/bin/env bash
# Ootle Asset Game | scripts/build-game-wasm.sh
# Build only. Does not publish or instantiate any contract.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/templates/game_session"
rustup target list --installed | grep -qx 'wasm32-unknown-unknown' || { echo 'Missing WASM target: rustup target add wasm32-unknown-unknown'; exit 1; }
cargo build --release --target wasm32-unknown-unknown
ls -lh target/wasm32-unknown-unknown/release/ootle_game_session.wasm
