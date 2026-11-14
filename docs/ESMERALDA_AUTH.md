# Backend wallet authentication (v0.6.15 integration patch)

This patch changes only backend authentication. Default behavior remains local Swarm `auth.request` with `credentials: None`.

For Montreal, set the private environment variable `OOTLE_WALLET_COOKIE_PATH=/home/jorge/tari-ootle-esmeralda/esmeralda/.cookie` in the **Axum process environment** and configure `wallet_rpc_url = "http://127.0.0.1:5100/json_rpc"` in Montreal's `config/network.toml`. Do not commit a Montreal-specific config with account addresses, and never commit the cookie.

The wallet daemon must be started with `--cookie-auth true` and bound to `127.0.0.1:5100`. Cookie auth uses its preconfigured scopes; default `admin` allows transaction submission. This patch does NOT add frontend/player authentication, alter deployed component addresses, instantiate contracts, or make the public game safe for untrusted write requests. Keep transaction-writing routes behind access controls until per-player authorization is implemented.

On each backend operation the cookie is read from disk, so wallet daemon restarts that rotate the cookie are handled without restarting Axum. If the configured cookie is missing or unreadable, authentication fails closed.
