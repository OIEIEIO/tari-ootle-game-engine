#!/usr/bin/env bash
# Ootle Game Engine | scripts/deploy-local.sh
# Version: v0.3.0
# Local Swarm deployment: wrapper for scripts/deploy_local.py (5 games × 4 seats by default).
# Asks for DEPLOY before any on-chain write. Extra options are passed through (see --help).
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 scripts/deploy_local.py "$@"
# End of scripts/deploy-local.sh
# Location: scripts/deploy-local.sh · Created: 2026-11-16 04:06:01 UTC
