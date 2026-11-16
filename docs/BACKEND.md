<!-- Ootle Game Engine | docs/BACKEND.md · Version: v0.4.0 -->
# Backend (app/)

Smart backend, plain page: the page renders `/api/game/{id}/state` and sends button presses.

## Modules

| File | Role |
|---|---|
| `src/config.rs` | `config/network.toml` and `deployments/local-swarm.json`, validated at start |
| `src/wallet.rs` | Wallet JSON-RPC (cookie or local `auth.request`); tokens stay in the backend |
| `src/decode.rs` | Positional decoding of the four components (docs/STATE_LAYOUT.md); unknown layouts are refused |
| `src/chain.rs` | Reads one game: four components, template checks, Treasury pools checked against the pointers; balances |
| `src/sim.rs` | Simulates every session method: buckets, expected state, per-account ledger, story text; builds manifests |
| `src/dice.rs` | CSPRNG dice, stored per game/round/turn/seat before display |
| `src/view.rs` | Game view: seats, pieces, squares with value/rent/buyback/next cost, roll preview and decision |
| `src/ops.rs` | Write pipeline, verification reads, receipts (persisted), news events, last pipeline record |
| `src/api_error.rs` | Structured errors (v0.6.18 scheme) |
| `src/board.rs` | Square names, kinds, seat colours, the 20 pieces, exact tTARI formatting |
| `src/catalogue.rs` | The Town Square catalogue (`config/catalogue.json`): zones, businesses, special squares; validated at start |
| `src/lots.rs` | Lot store: the business on each owned lot, who chose it and how |
| `src/main.rs` | HTTP routes only |

## API

| Method | Path | Body |
|---|---|---|
| GET | `/api/system/info` | |
| GET | `/api/games` | |
| GET | `/api/game/{id}/state` | |
| POST | `/api/game/{id}/roll` | `{"pay": bool}` (held seat only) |
| POST | `/api/game/{id}/turn` | `{"buy": bool, "sell_square": u16 or null, "build": bool, "business": id or null}` |
| POST | `/api/game/{id}/sell` | `{"seat", "square"}` |
| POST | `/api/game/{id}/build` | `{"seat", "square"}` |
| POST | `/api/game/{id}/offer` | `{"seller", "buyer", "square", "price"}` (base units) |
| POST | `/api/game/{id}/offer/accept` | `{"buyer"}` |
| POST | `/api/game/{id}/offer/decline` | `{"buyer"}` |
| POST | `/api/game/{id}/pay-shop-fee` | `{"payer", "held_seat"}` |
| GET | `/api/game/{id}/history` | |
| GET | `/api/game/{id}/dev` | |
| POST | `/api/tx/inspect` | `{"transaction_id"}` |

Writes return `{"committed", "verified", "status", "transaction_id", "fee", "story",
"state_differences", "reconciliation", "cause", "verification", "business"}`. Errors return `{"success": false, "error": {"code",
"category", "message", "operation", "details", "technical_message", "transaction_id",
"submission_status"}}`; `submission_status` is `not_submitted` before submission and `unknown` after.

## Pipeline steps (Developer tab)

Read state (templates verified) → Roll (turns) → Prepare → Balances (seat funding and House reserve:
House bucket + max fee) → Signers → Dry run → Submit → Wait result → Verify (read 1) → Verify read 2…
(only while the state or a balance differs, up to `reconcile_reads`).

Locks: one per game in separate treasury mode (the games share no account and settle in parallel),
plus one for the main House (top-ups); in global treasury mode every game shares one lock, so
reconciliation stays exact when pools are shared. A rejected transaction leaves the stored roll in
place; the same dice apply on retry.

## v0.3.0 additions

Modules: `claims.rs` (who holds each seat: player token, name, vehicle, AI; leaderboard milestones;
JSON stores `claims_store`, `leaderboard_store`), `ai.rs` (AI decisions and one driver task per game).

| Method | Path | Notes |
|---|---|---|
| GET | `/api/me` | The token's game and seats |
| POST | `/api/claim` | `{game, seats, ai, name, vehicles:[[seat, 1..20]]}`; returns the token on first claim |
| POST | `/api/release` | Frees the token's seats; AI seats are freed when no human is left |
| GET | `/api/leaderboard` | Live net worth of every held seat and the recorded milestones (cached 10 s) |
| POST | `/api/game/{id}/reset` | `{seat}`: reset your seat to GO with the seat start |
| POST | `/api/game/{id}/regenerate` | `{seat}`: reset and pass the turn (seat cannot pay, owns nothing) |
| GET | `/api/game/{id}/addresses` | Receiving address, component and balance of every account |

Every write needs the header `x-player-token` and acts only for the token's seats. `/turn` takes
`build: true` to build on the asset landed on in the same transaction. The AI driver acts as System,
only while the game has a human, after `ai_move_delay_secs` since its last confirmation, and within
`ai_tx_per_minute` (0 = no cap). `auto_roll_secs` (0 = off) passes an idle human or open seat.

## v0.4.0 additions

Modules: `catalogue.rs`, `lots.rs` (above). Settings (`config/*.toml`, defaults in brackets):
`reconcile_reads` [4, 1..10], `reconcile_read_delay_ms` [500, at most 10000], `receipts_store`
[data/receipts.jsonl], `lots_store` [data/lots.json], `catalogue_file` [config/catalogue.json].

| Method | Path | Notes |
|---|---|---|
| GET | `/api/health` | `?game=N`: wallet answer time, chain read and template check, transactions on/off, catalogue counts (start gate, Developer tab) |
| GET | `/api/catalogue` | The catalogue as loaded |
| GET | `/api/game/{id}/receipts` | `?limit=1..1000` [100], `&unverified=true`: receipts with full detail, newest first |
| GET | `/api/game/{id}/lots` | The lot store for the game |

**Verification reads.** After `Accepted`, read 1 is taken at once; reads 2..N only while the state or
a balance differs, `reconcile_read_delay_ms` apart. The receipt records each read (`attempt`, `of`,
`at_ms`, `ok`, state differences or read error, `reconciliation_ok`, `sum`, `fee`, mismatched rows),
the last read's field differences and reconciliation, the pipeline steps and the manifest. `cause`
names why a receipt is UNVERIFIED: `reread_failed`, `state_fields_differ`, `balances_differ`,
`no_final_fee`, or `rejected`; `verification` says it in words (for example "reconciled on read 2 of 4").

**Receipts store.** One JSON line per receipt, appended on every receipt; the last 1000 are loaded at
start, and the file is rewritten to those when it passes 4000 lines. Receipt ids continue across restarts.

**News events.** Each receipt carries `events`: move, released, sell, go, maintenance_sent,
maintenance_held, buy (with `business`), rent (owner, earned, rent_fee, level), card (label), fee,
penalty, reward, build (level), shop_fee, offer, offer_accepted, offer_declined, reset, regenerate,
top_up. `/history` returns them; the page builds its news stack and sounds from them.

**Lots.** A purchase records the business: the player's choice from the zone's three (checked before
submission, `INVALID_BUSINESS` otherwise), or random for the AI and when none is given. On every state
read, owned lots without a business get one at random (`source: assigned`) and lots back with the
House are cleared. The state view adds to each square `zone`, `zone_name`, `district`, `business`,
`business_source` (lots) or `special`; each seat has `piece_id`, `piece`, `color` (the seat colour).

<!-- End of docs/BACKEND.md · Location: docs/BACKEND.md · Created: 2026-11-16 04:06:01 UTC -->
