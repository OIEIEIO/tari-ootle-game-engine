<!-- Ootle Game Engine | README.md · Version: v0.4.3 -->
# Ootle Game Engine v0.4.3

A free-play demonstration of a blockchain game engine on Tari Ootle: four Ootle templates, a Rust
backend that signs for every seat, and a single game page. The House is funded by the operator; there
is no buy-in and no cash-out. Independent of the v0.6.18 game and of every earlier engine deployment.

| Part | Folder |
|---|---|
| Contracts: Registry, Lobby, Treasury, Town Square (all unchanged since v0.3.0) | `contracts/` |
| Deployment script: a pool of games, local Swarm or Esmeralda | `scripts/deploy_local.py`, `deploy-local.sh`, `deploy-esmeralda.sh` |
| Backend (Rust, Axum), AI driver and page | `app/` |
| Town Square catalogue: zones, businesses, special squares | `app/config/catalogue.json` |
| Engine harness tests | `engine-tests/` |
| Monte Carlo economics | `tools/econ-sim/` (v0.3.2) |

## What changed in v0.4.3

Deploy script only (backend, page, contracts unchanged).
- **`--fund-from-house`** (off unless passed): funds every other account from the main House with
  walletd `accounts.transfer` instead of the Ootle faucet, for a network whose Ootle faucet is empty.
  Each transfer is journaled (`fund:<role>`) and a rerun skips the ones done. Amounts: 2.000000 per seat,
  88.000000 per game House (4 seats × (20.000000 + 2.000000)), 1.000000 per Community/Chance account.
  For 5 games × 4 seats that is 492.000000 to 37 accounts; the main House must hold 676.000000
  (those amounts, 37 transfer fees at 2.000000 at most, and a 110.000000 reserve for its own deploy
  fees) before anything is sent, or the script stops with nothing sent.
- The journal format is unchanged, so a deployment stopped at the faucet resumes with this option.

Esmeralda with an empty Ootle faucet: send tTARI to the main House (`OGE-<tag>-global_house`, its
`otl_esm_…` address from `accounts.get`), for example from a community faucet, then:

```bash
OOTLE_WALLET_RPC=http://127.0.0.1:<port>/json_rpc OOTLE_WALLET_COOKIE_PATH=<cookie> ./scripts/deploy-esmeralda.sh --fund-from-house
```

## What changed in v0.4.2

Page only (backend, contracts and deployment unchanged).
- **No board flash before the lobby.** The lobby is now the first element of the page and is shown from
  the first paint; its line reads "Connecting…" until the game is read, then where you are (or the
  read error). Enter pressed before the game has arrived starts the intro as soon as it arrives.
- **Darker backdrop.** The page and the board panel are near-black `#030509` (the Town Square's own
  colour), with no glow and no panel frame, so the tiles stand out; tile colours are unchanged.

## What changed in v0.4.1

Page only (backend, contracts and deployment unchanged; a v0.3.0 or v0.4.0 deployment and its `data/`
stores carry over as they are).
- **Roll works again.** In v0.4.0 the closed landing-scene layer was still laid out over the Town
  Square (its `display:flex` overrode `hidden`) and took every click there, including Roll.
- **The intro has its music.** In v0.4.0 the intro started before the browser audio was running, so its
  cues were dropped; it now starts once the audio runs.
- **Lobby on every load and refresh**, replacing the start gate: a black page with OOTLE GAME ENGINE,
  one line saying where you are (`Game 2 · 4 seats: 1 player, 3 AI, 0 open · round 1 · you: Seat 1`),
  and Enter with sound / Enter without sound. Then the intro, then the board.
- **Game selector label:** `Game 2 · 4 seats: 1 player, 3 AI, 0 open · round 1`.
- **Switching games in the selector** goes straight to that game's board, with no lobby and no intro,
  and sets `?game=N` in the address. Replay intro is unchanged.
- **The browser remembers the last game it was in.** A load opens the game in the address, else the
  last game, else the game you hold a seat in, else the first game.
- **Setup screen:** the panel is "Start or take over a game"; its button reads Start new game (fresh
  board) or Take over game N (a game in progress with no players).

## What changed in v0.4.0

No contract changes: a v0.3.0 deployment is reused as it is (same templates, same games, same
pointers). Only the backend and the page change.

**Verification after a transaction (the false UNVERIFIED of turns 297 and 301)**
- Cause: right after `Accepted`, the wallet could still serve balances from before the transaction,
  so reconciliation compared against stale reads although the chain was correct.
- After an accepted transaction the backend reads the state and every balance; only while they do
  not match the plan, it reads again: up to `reconcile_reads = 4` reads, `reconcile_read_delay_ms = 500`
  apart. Both settings are on and are in `app/config/*.toml`.
- Every receipt keeps its detail: each read (time, field differences, reconciliation rows), the
  pipeline steps, the manifest, the named cause (`reread_failed`, `state_fields_differ`,
  `balances_differ`, `no_final_fee`, `rejected`) and the move's news events.
- Receipts are stored as JSON lines (`receipts_store`) and survive restarts. On the Developer tab a
  receipt row opens its detail; an UNVERIFIED filter is next to the table.

**Seats and pieces**
- Colour belongs to the seat: seats 1–4 are blue `#3fa7ff`, orange `#ff9f43`, green `#2ee6a6`,
  violet `#b48cff`; seats 5–10 are the next most separated colours, 11–20 sit in between.
- 20 single-colour SVG pieces in the seat colour, with no colour words in their names (Roadster,
  Sailboat, Jet, … Saucer). Existing claims keep their piece number.

**Landing scenes (all 40 squares)**
- Shown when any human seat lands, in every browser watching. AI landings show none.
- Each lot has a fixed zone and three businesses (`app/config/catalogue.json`, 22 lots × 3 = 66).
  Stages: vacant lot with the business blueprint → bought (silhouette only, "Sold · Seat N") →
  1 house (coloured, lights on) → 2 houses (the extension) → hotel (the flagship, spotlights). Each
  level needs a landing (build on landing).
- The panel sits below the scene: owner, cash, development, rent, value, business; Buy / Pass with
  the three businesses, or Build / Skip; the stage row. Close or Escape closes it; a badge on the square
  reopens it while a human piece stands there. The hover card is unchanged.
- A human buyer chooses the business; the AI chooses at random; lots already owned when v0.4.0
  first starts are given one at random. The business stays through development and private sales and
  is cleared when the lot returns to the House (`lots_store`).
- The 18 special squares have their own scenes.

**News stack** (replaces the rotating news line)
- Up to N lines (N = seats), newest at the bottom and brightest, one line per move, each starting with
  the seat's piece; amounts in white; business names in their colour.
- Held as "SINCE YOUR LAST TURN" until you roll; your roll clears it and starts with your move.
  Moves that arrive together enter 0.4 s apart. Hidden while a landing scene is open.

**Sound** (synthesised in the browser, no audio files)
- On by default; speaker button, volume, and a settings panel with the volume of other seats' moves
  (starts at 50%) and ambient sound on or off. Stored per browser.
- Effects for turns, dice, moves, landings, GO, purchases, builds, rent, cards, fees, Maintenance,
  regeneration, news and errors. Ambient sound per landing scene (district base plus the business's
  layer; each special square has its own).
- Intro theme on the four-note contract chord (D major: Registry D4, Lobby F#4, Town Square A4,
  Treasury D5), cued from the existing intro's own steps, so it follows the animation exactly. The
  intro is otherwise unchanged; its existing skip fades the music out.

**Start gate** (first visit in a browser only; replaced in v0.4.1 by the lobby)
- The current state ghosted behind it, health lights (wallet, chain read, templates, transactions,
  catalogue), resume line, Start with sound / Start without sound. Then the intro. The health lights
  remain on the Developer tab.

**Fixes**
- The game selector is refreshed with every state poll (this game) and every 15 s (all games).
- `docs/BACKEND.md` no longer says one lock covers all games.

**New endpoints**: `/api/health`, `/api/catalogue`, `/api/game/{id}/receipts`, `/api/game/{id}/lots`;
`/turn` takes `business` with a purchase (`docs/BACKEND.md`).

## What changed in v0.3.0

**Contract (Town Square; state fields 41..48 appended, 0..40 unchanged — `docs/STATE_LAYOUT.md`)**
- `set_economics` (owner): GO floor and scaling, rent fee, build on landing, fee and penalty squares.
  The deploy script sets them; the operator can change them later. Defaults keep v0.2.0 play except
  build on landing, which is on.
- Scaled GO: when on, GO = max(floor, GO reward × unowned assets ÷ 22), from the board as the turn starts.
- Rent fee: basis points of each rent, taken from the owner's receipt and routed 90/5/5 as a fee.
- Build on landing: one level, only on the owned asset the seat has just landed on, in the same
  transaction as the turn or before the next move.
- `reset_seat` (owner): assets back to the House, back to GO, hold cleared, back in play, new stint.
  The backend settles the seat's cash to the seat start in the same transaction.

**Backend and page**
- A pool of 5 games, created by the deploy script and recycled. Each game has its own House,
  Community and Chance accounts, so games share no account and run in parallel (one lock per game).
- Seats are claimed from the page (seat keys stay in the backend). One game per browser; Release keeps
  the seat's cash, assets and position for the next player, who takes it over or resets it.
- Setup screen after the intro: seats you control (1–4, so two people can share one screen), AI
  opponents, vehicles. Join an open game. Watch without a seat.
- AI seats, run by the backend: buy if 5.000000 tTARI stays, build on landing if 10.000000 stays,
  decline offers, sell or regenerate when short. 5 s between AI moves after each confirmation.
- Regeneration: a seat that cannot pay and owns nothing goes back to GO with 20.000000 tTARI.
- Game House top-up: the main House refills a game House to 100.000000 tTARI when it falls below
  25.000000, or below the payout of the turn being settled.
- Leaderboard (net worth = cash + assets at buyback value) and 100-round stint milestones.
- Developer tab: receiving address, component and balance of every account (show, hide, copy);
  transaction ids in Receipts open in Transaction inspect, with copy buttons.

**Switched off unless you set them (`app/config/*.toml`)**
- `auto_roll_secs = 0`: auto-roll of an idle human or open seat. Set 300 for the agreed 5 minutes.
- `ai_tx_per_minute = 0`: no server-wide cap on AI transactions.

## A v0.2.0 deployment cannot be reused (a v0.3.0 deployment can)

v0.3.0 gave Town Square new state fields, so the templates are published again and the 5 games are new.
The deploy script refuses a journal written by an earlier version. Move the old `deployments/`
folder aside (or pass new `--state` / `--pointers` names); the earlier deployment is not changed.
v0.4.0 to v0.4.3 change no contract: keep the v0.3.0 `deployments/` folder (and `app/data/`) and start the new backend.

## Desktop: local Swarm (wallet on 127.0.0.1:12010)

```bash
./scripts/build-all.sh                            # four templates to WASM
./scripts/deploy-local.sh                         # 5 games × 4 seats; type DEPLOY when asked
./scripts/run-app.sh                              # http://127.0.0.1:3020, chain reads only
OOTLE_ENABLE_LIVE_TESTS=1 ./scripts/run-app.sh    # transactions and the AI driver on
./scripts/run-tests.sh                            # all local tests
./scripts/ui-check.sh --clear-offer               # page against a fake wallet on :3021, no Swarm
```

Deploy options: `python3 scripts/deploy_local.py --help` (`--games`, `--players`, `--seat-start`,
`--go-scaled --go-floor`, `--rent-fee-bps`, `--fee-amount`, `--penalty-amount`, `--build-anywhere`).
Every account is funded from the faucet, then each seat's cash is settled to the seat start: the
excess goes to its game House. The main House is the operator account `OGE-<tag>-global_house`.

## Montreal server: Esmeralda through your wallet daemon

```bash
./scripts/build-all.sh
OOTLE_WALLET_RPC=http://127.0.0.1:<port>/json_rpc OOTLE_WALLET_COOKIE_PATH=<cookie> ./scripts/deploy-esmeralda.sh
# set wallet_rpc_url in app/config/esmeralda.toml to the same port, then:
OOTLE_CONFIG=config/esmeralda.toml OOTLE_WALLET_COOKIE_PATH=<cookie> OOTLE_ENABLE_LIVE_TESTS=1 ./scripts/run-app.sh
```

This writes `deployments/esmeralda-state.json` (journal, git-ignored) and `deployments/esmeralda.json`
(pointers). After deploying, send your 5,000 tTARI to the main House: its receiving address is on the
Developer tab. The backend listens on 127.0.0.1 only; publish it through your reverse proxy.

## How a turn runs

1. **Roll** (`POST /api/game/{id}/roll`, your seat only). The backend draws both dice from the
   operating system's random source and stores them before showing them; asking again returns them.
2. **Preview.** The backend simulates the turn against the contract (`app/src/sim.rs`).
3. **Decision** (in the landing scene). None: settles automatically. Unowned asset: Buy (choosing one
   of the zone's three businesses) or Pass. Own asset: Build or Skip.
   Short of funds: recovery sale inside the turn. Short with nothing to sell: Regenerate.
4. **Settle**: top-up if needed → dry run → submit → wait → verification reads (state check and
   reconciliation, read again only while they differ, up to 4 reads 500 ms apart) → receipt.

Signing: the game House key seals and pays the fee; the acting seat's key (and for a reset the
operator key) is in `signing_key_ids`.

## What your Swarm and Esmeralda tests confirm

The engine harness runs every manifest the backend and the deploy script build. Live, the first
runs confirm: the faucet call and account creation on Esmeralda; the House top-up (operator seal);
`reset_seat` with the operator key in `signing_key_ids`; 5 games settling in parallel through one
wallet daemon; and `accounts.get` returning an `otl_` receiving address for the Developer tab.

<!-- End of README.md · Location: README.md · Created: 2026-11-16 04:06:01 UTC -->
