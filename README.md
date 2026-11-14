# Ootle Asset Game v0.6.12 — Dark Stage & Player Sidebar

Baseline: v0.6.11. This is a **frontend-only** upgrade; `templates/` and all Rust transaction handlers are unchanged. The existing v0.6.10 GameSession and Treasury components remain deployed and in use. **Do not run any deployment or migration script.**

## Changes

- Plain near-black interior walkway; no diagonal stripes and no bordered Town Square panel. Player tokens and owner-coloured visitor carpets remain on their established track.
- Minimal default Town Square: active player, dice, and roll button, floating on the dark stage.
- Rolling populates a centered landing experience (property, rent, financial effect), while settlement is handled in the right-hand **Player actions & turn settlement** panel.
- Clicking the occupied asset opens the existing detailed asset presentation; clicking the same square again dismisses it. Clicking an active non-asset square toggles its landing view. Closing a panel does **not** cancel or settle a pending transaction.
- Right sidebar now houses player balances and owned property summary, required turn settlement controls, optional private sales and construction, and collapsible technical diagnostics. Backend endpoint requests and confirmation prompts are unchanged.
- Sidebar can be hidden, but **must be shown again to settle a pending turn**.

## Install and run on Ubuntu

```bash
cd ~/Downloads
unzip ootle-asset-game-v0.6.12.zip
cd ootle-asset-game-v0.6.12
cargo test
node --check web/game.js
python3 tests/check_static_integration.py
cp ../ootle-asset-game-v0.6.11/config/{accounts,deployment,network,game_session}.toml config/
# Stop the prior backend; leave Tari Swarm running.
OOTLE_ENABLE_LIVE_TESTS=1 cargo run
```

Visit http://127.0.0.1:3000/game. Verify initial chain snapshot, roll display, land-event view, sidebar settlement, asset click toggle, and technical receipts before treating it as live-validated.


### v0.6.15 visit navigation patch

After a verified automatic or manual turn settlement, the visit acknowledgement remains visible for approximately 6.5 seconds, then automatically returns to the dice view. A functional **Return to Town Square** button exits immediately. Opening an asset profile cancels automatic dismissal, and the profile's Town Square button also returns directly to dice. No smart contract modifications.


## v0.6.15 visit acknowledgements

Visit confirmations stay visible until the player explicitly acknowledges them with **Acknowledge · Return to Town Square**. There is no timed dismissal. Asset inspection can still be opened by clicking the occupied perimeter tile. No on-chain migration is required.
