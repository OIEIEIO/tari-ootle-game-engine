#!/usr/bin/env bash
# Ootle Game Engine | scripts/deploy-esmeralda.sh
# Version: v0.3.0
# Esmeralda deployment through the wallet daemon on this server (loopback only), same steps as the
# local Swarm. Writes deployments/esmeralda-state.json (journal) and deployments/esmeralda.json
# (pointers read by app/config/esmeralda.toml). Asks for DEPLOY before any on-chain write.
# Set OOTLE_WALLET_RPC to the daemon's JSON-RPC URL and OOTLE_WALLET_COOKIE_PATH to its auth cookie, e.g.
#   OOTLE_WALLET_RPC=http://127.0.0.1:12010/json_rpc OOTLE_WALLET_COOKIE_PATH=/path/to/cookie ./scripts/deploy-esmeralda.sh
# Extra options are passed through (see python3 scripts/deploy_local.py --help).
set -euo pipefail
cd "$(dirname "$0")/.."
: "${OOTLE_WALLET_RPC:?set OOTLE_WALLET_RPC to the wallet daemon JSON-RPC URL on this server}"
exec python3 scripts/deploy_local.py --network-name Esmeralda \
  --state deployments/esmeralda-state.json --pointers deployments/esmeralda.json "$@"
# End of scripts/deploy-esmeralda.sh
# Location: scripts/deploy-esmeralda.sh · Created: 2026-11-16 04:06:01 UTC
