#!/usr/bin/env bash
# Ootle Asset Game | scripts/run-tests.sh
# Purpose: Run Rust formatter check and automated tests.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo test --all-targets
# End of scripts/run-tests.sh
