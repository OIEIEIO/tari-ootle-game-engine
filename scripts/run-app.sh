#!/usr/bin/env bash
# Ootle Game Engine | scripts/run-app.sh
# Version: v0.3.0
# Starts the backend and page on http://127.0.0.1:3020 (app/config/network.toml, or the file named by
# OOTLE_CONFIG relative to app/, e.g. OOTLE_CONFIG=config/esmeralda.toml).
# Reads the deployment pointers that config names (written by deploy-local.sh / deploy-esmeralda.sh).
# Chain reads are always live. Transactions, and the AI driver, are off unless started with:
#   OOTLE_ENABLE_LIVE_TESTS=1 ./scripts/run-app.sh
# Wallet cookie authentication: set OOTLE_WALLET_COOKIE_PATH as for v0.6.18.
set -euo pipefail
cd "$(dirname "$0")/../app"
CONF="${OOTLE_CONFIG:-config/network.toml}"
[ -f "$CONF" ] || { echo "$CONF not found (relative to app/)"; exit 1; }
DEP="$(sed -n 's/^deployment_file *= *"\(.*\)"/\1/p' "$CONF")"
[ -f "$DEP" ] || { echo "$DEP missing: deploy first (./scripts/deploy-local.sh or ./scripts/deploy-esmeralda.sh)"; exit 1; }
exec cargo run --release
# End of scripts/run-app.sh
# Location: scripts/run-app.sh · Created: 2026-11-16 04:06:01 UTC
