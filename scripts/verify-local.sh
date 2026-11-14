#!/usr/bin/env bash
# Ootle Asset Game | scripts/verify-local.sh
# Native build, tests, and standalone template build (does not publish).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
cargo build
cargo test
"$ROOT/scripts/build-wasm.sh"
# End of scripts/verify-local.sh
