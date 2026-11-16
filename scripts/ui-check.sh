#!/usr/bin/env bash
# Ootle Game Engine | scripts/ui-check.sh
# Version: v0.4.0
# Page check without a Swarm: runs app/tests/fake_wallet/fake_wallet.py (engine-dumped fixtures, no
# state changes) on 127.0.0.1:12099 and the backend on http://127.0.0.1:3021 with a temporary config.
# Submitted operations report COMMITTED / UNVERIFIED because the fake wallet never changes state.
# Options are passed to the fake wallet, e.g. --clear-offer (serve the session without the pending offer).
# v0.3.0: the fixtures hold one game; claim a seat on the setup screen to see the player controls. AI seats
# would keep resubmitting against the unchanging fixtures, so this check runs with AI move delay 30 s.
# v0.4.0: the temporary config points at the shipped catalogue (absolute path) and keeps the receipts
# and lot stores in the temporary directory. The first visit shows the start gate; later loads play
# the intro directly (clear the site data to see the gate again). The fixtures' placeholder main House
# pointers are replaced in the temporary copy of pointers.json (see below).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
mkdir -p "$TMP/config"
cat > "$TMP/config/network.toml" <<TOML
wallet_rpc_url = "http://127.0.0.1:12099/json_rpc"
network_name = "Fake wallet (fixtures)"
listen_host = "127.0.0.1"
port = 3021
deployment_file = "$TMP/pointers.json"
max_fee = 1000000
decision_window_secs = 300
roll_store = "data/rolls.json"
ai_move_delay_secs = 30
claims_store = "data/claims.json"
leaderboard_store = "data/leaderboard.json"
receipts_store = "data/receipts.jsonl"
lots_store = "data/lots.json"
catalogue_file = "$ROOT/app/config/catalogue.json"
TOML
# The fixtures' main House pointers are placeholders ("component_00"), which the configuration check
# refuses; the copy used here points them at the fixture's Treasury component (read-only check).
python3 -I - "$ROOT/app/tests/fixtures/pointers.json" "$TMP/pointers.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
t = d["treasury"]
for k in ("global_house", "global_community", "global_chance"):
    if t.get(k) == "component_00":
        t[k] = t["component"]
json.dump(d, open(sys.argv[2], "w"), indent=2)
PY
( cd "$ROOT/app" && cargo build --release )
python3 -I "$ROOT/app/tests/fake_wallet/fake_wallet.py" --port 12099 "$@" &
FW=$!
trap 'kill $FW 2>/dev/null; rm -rf "$TMP"' EXIT
sleep 1
echo 'Open http://127.0.0.1:3021  (Ctrl+C to stop)'
cd "$TMP" && exec env OOTLE_ENABLE_LIVE_TESTS=1 "$ROOT/app/target/release/ootle-game-engine"
# End of scripts/ui-check.sh
# Location: scripts/ui-check.sh · Created: 2026-11-16 04:06:01 UTC
