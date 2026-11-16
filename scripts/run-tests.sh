#!/usr/bin/env bash
# Ootle Game Engine | scripts/run-tests.sh
# Version: v0.3.0
# Runs every local test: rules unit tests, deploy script tests, backend unit tests, and the engine
# harness tests (contracts, deploy manifests, and the backend's own manifests and expected state).
# The engine tests compile the Tari engine on first run (several minutes) and need the wasm target.
set -euo pipefail
cd "$(dirname "$0")/.."
echo '===== rules unit tests (contracts/town_square) ====='
( cd contracts/town_square && cargo test --lib )
echo '===== deploy script tests ====='
python3 -m unittest discover -s tests -v
echo '===== backend unit tests (app) ====='
( cd app && cargo test --lib )
echo '===== engine harness tests ====='
( cd engine-tests && cargo test -- --test-threads=2 )
# End of scripts/run-tests.sh
# Location: scripts/run-tests.sh · Created: 2026-11-16 04:06:01 UTC
