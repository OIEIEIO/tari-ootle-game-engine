#!/usr/bin/env bash
# Ootle Asset Game v0.6.10 | explicit one-time on-chain 40-square migration.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
./scripts/build-game-wasm.sh
python3 scripts/deploy-economy-game.py "$@"
