#!/usr/bin/env bash
# Ootle Game Engine | scripts/build-all.sh
# Version: v0.1.0
# Builds the four templates to wasm32-unknown-unknown. Build only; publishes nothing.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || { echo 'cargo not installed'; exit 1; }
rustup target list --installed | grep -qx wasm32-unknown-unknown || { echo 'Run: rustup target add wasm32-unknown-unknown'; exit 1; }
for name in registry lobby treasury town_square; do
  echo "===== $name ====="
  ( cd "contracts/$name" && cargo build --release --target wasm32-unknown-unknown )
done
ls -l contracts/*/target/wasm32-unknown-unknown/release/*.wasm
echo 'Build complete. No transaction submitted.'
# End of scripts/build-all.sh
# Location: scripts/build-all.sh · Created: 2026-11-15 21:49:02 UTC
