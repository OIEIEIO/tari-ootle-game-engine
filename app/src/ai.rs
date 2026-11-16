//! Ootle Game Engine | app/src/ai.rs
//! Version: v0.4.4
//! AI seats and the auto-roll, run by the backend. One driver task per game.
//!
//! Who moves without a person:
//!   - an AI seat on its turn (only while the game has at least one human);
//!   - an offer addressed to an AI seat (the AI declines it);
//!   - a human or open seat whose turn has waited `auto_roll_secs` (0 = off, the default): it is
//!     passed: rolled and settled with no purchase and no building; settlement rules still apply.
//! The AI model is the one simulated in tools/econ-sim (v0.3.1 "A" with the landing rule):
//!   - buys an unowned landing asset only if at least `ai_reserve` stays after the purchase;
//!   - builds one level on the owned asset it lands on, if at least `ai_build_floor` stays after building
//!     (only while the game's build-on-landing rule is on; with it off the AI does not build);
//!   - a held seat rolls for doubles (third failure forces payment);
//!   - shortfall: sells the cheapest asset that settles the turn inside it; otherwise sells to the
//!     House one asset at a time, cheapest first; with nothing left to sell, the seat regenerates.
//! v0.4.0: an AI purchase names no business, so the backend picks one of the lot zone's three at random (lots.rs).
//! Pacing: after a confirmed AI or auto-roll transaction, the game waits `ai_move_delay_secs` before
//! the next one; `ai_tx_per_minute` (0 = no cap) limits these transactions across all games.

use crate::{
    claims::Holder,
    decode::Session,
    dice::{self, Roll},
    ops::{self, Actor, Request, Shared},
    sim::{self, Action},
};
use ootle_town_square::rules::{self, NO_SQUARE};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Strategy { pub buy: bool, pub build: bool, pub reserve: u64, pub build_floor: u64 }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Move {
    Turn { buy: bool, sell_square: Option<u16>, build: bool },
    /// Sell this asset to the House first (one transaction), then decide again.
    Sell(u16),
    /// Nothing left to sell and the turn cannot be funded: reset the seat and pass the turn.
    Regenerate,
}

/// Decides the active seat's move for its stored roll. `cash` is the seat's balance.
pub fn decide(s: &Session, cb: u16, hb: u16, roll: &Roll, cash: u64, st: Strategy) -> Result<Move, String> {
    let seat = roll.seat;
    let i = usize::from(seat - 1);
    let turn = |buy: bool, sell: Option<u16>| Action::Turn { seat, die1: roll.die1, die2: roll.die2, pay: roll.pay, buy, sell_square: sell };
    let base = sim::simulate(s, cb, hb, &turn(false, None))?;
    if base.seat_withdraw > cash {
        let p = s.params();
        let mut owned: Vec<u16> = (0..rules::BOARD_SIZE).filter(|q| s.owners[usize::from(*q)] == seat).collect();
        owned.sort_by_key(|q| (rules::buyback_value(*q, s.improvements[usize::from(*q)], &p), *q));
        let inside = owned.iter().copied().find(|q| {
            sim::simulate(s, cb, hb, &turn(false, Some(*q))).map(|o| o.seat_withdraw <= cash).unwrap_or(false)
        });
        return Ok(match (inside, owned.first()) {
            (Some(q), _) => Move::Turn { buy: false, sell_square: Some(q), build: false },
            (None, Some(q)) => Move::Sell(*q),
            (None, None) => Move::Regenerate,
        });
    }
    let mut chosen = base;
    let mut buy = false;
    if st.buy && chosen.story.buy_option.is_some() {
        let b = sim::simulate(s, cb, hb, &turn(true, None))?;
        let after = i128::from(cash) + b.ledger.seats[i];
        if b.seat_withdraw <= cash && after >= i128::from(st.reserve) { chosen = b; buy = true; }
    }
    let mut build = false;
    if st.build && s.build_on_landing {
        let a = &chosen.after;
        let q = a.landed_square;
        if a.landed_seat == seat && q != NO_SQUARE && rules::is_asset(q) && a.owners[usize::from(q)] == seat
            && a.improvements[usize::from(q)] < 3 {
            let cost = rules::next_improvement_cost(a.improvements[usize::from(q)], &a.params());
            let after = i128::from(cash) + chosen.ledger.seats[i] - i128::from(cost);
            build = after >= i128::from(st.build_floor);
        }
    }
    Ok(Move::Turn { buy, sell_square: None, build })
}

/// One driver step for `game`. Returns Ok(true) when a transaction was confirmed.
async fn step(app: &Shared, game: u64) -> Result<bool, String> {
    let cfg = &app.cfg.network;
    if app.claims.lock().await.humans(game) == 0 { return Ok(false); }
    if let Some(t) = app.last_auto.lock().await.get(&game) {
        if t.elapsed() < Duration::from_secs(cfg.ai_move_delay_secs) { return Ok(false); }
    }
    if cfg.ai_tx_per_minute > 0 {
        let mut w = app.auto_window.lock().await;
        while w.front().is_some_and(|t| t.elapsed() > Duration::from_secs(60)) { w.pop_front(); }
        if w.len() >= cfg.ai_tx_per_minute as usize { return Ok(false); }
    }
    if app.game_lock(game)?.try_lock().is_err() { return Ok(false); }
    // v0.4.4: the game's shared read (ops.rs `snapshot`), not a new chain read on every step;
    // `execute` still reads fresh before it submits.
    let (snap, bal) = app.snapshot(game).await?;
    let s = &snap.session;
    if !s.started { return Ok(false); }
    let claims = app.claims.lock().await.seats(game, s.capacity);
    let holder = |seat: u8| claims[usize::from(seat - 1)].holder.clone();

    let (req, actor) = if s.has_offer() {
        if holder(s.offer_buyer) != Holder::Ai { return Ok(false); }
        (Request::Action(Action::Decline { buyer: s.offer_buyer }), Actor::Ai)
    } else {
        let a = s.active;
        let (actor, st) = match holder(a) {
            Holder::Ai => (Actor::Ai, Strategy { buy: true, build: true, reserve: cfg.ai_reserve, build_floor: cfg.ai_build_floor }),
            Holder::Human { .. } | Holder::Open => {
                if cfg.auto_roll_secs == 0 { return Ok(false); }
                let seen = app.turn_seen(game, s.round, s.turn_count).await;
                if dice::now().saturating_sub(seen) < cfg.auto_roll_secs { return Ok(false); }
                (Actor::AutoRoll, Strategy { buy: false, build: false, reserve: 0, build_floor: 0 })
            }
        };
        let existing = app.rolls.lock().await.get(game, s.round, s.turn_count, a).cloned();
        let roll = match existing {
            Some(r) => r,
            None => app.rolls.lock().await.roll(game, s.round, s.turn_count, a, false)?,
        };
        let cash = bal.seats[usize::from(a - 1)];
        let m = decide(s, snap.treasury.community_bps, snap.treasury.chance_bps, &roll, cash, st)?;
        let req = match m {
            Move::Turn { buy, sell_square, build } => Request::Turn { buy, sell_square, build, business: None },
            Move::Sell(q) => Request::Action(Action::Sell { seat: a, square: q }),
            Move::Regenerate => Request::Reset { seat: a, pass_turn: true },
        };
        (req, actor)
    };
    let r = app.execute(game, req, actor).await.map_err(|e| e.reason)?;
    if r.get("committed").and_then(|v| v.as_bool()) != Some(true) {
        return Err(format!("transaction not committed: {}", r.get("transaction_id").cloned().unwrap_or_default()));
    }
    app.last_auto.lock().await.insert(game, Instant::now());
    app.auto_window.lock().await.push_back(Instant::now());
    Ok(true)
}

/// Starts one driver task per game. Does nothing unless live transactions are enabled.
pub fn spawn(app: Shared) {
    if !ops::live_enabled() { tracing::info!("AI driver not started: live transactions disabled"); return; }
    for game in app.cfg.game_ids() {
        let app = app.clone();
        tokio::spawn(async move {
            loop {
                match step(&app, game).await {
                    Ok(_) => {
                        app.driver_error.lock().await.remove(&game);
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                    Err(e) => {
                        tracing::warn!(game, cause = %e, "AI driver step failed; retrying in 15 s");
                        app.driver_error.lock().await.insert(game, e);
                        tokio::time::sleep(Duration::from_secs(15)).await;
                    }
                }
            }
        });
    }
    tracing::info!("AI driver started for every game");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::component;

    fn fixture() -> Session {
        let text = std::fs::read_to_string("tests/fixtures/session.json").unwrap();
        let s = Session::decode(&component(&serde_json::from_str(&text).unwrap()).unwrap().2).unwrap();
        sim::simulate(&s, 500, 500, &Action::Decline { buyer: 2 }).unwrap().after
    }
    fn roll(seat: u8, d1: u8, d2: u8) -> Roll { Roll { game: 1, round: 2, turn: 6, seat, die1: d1, die2: d2, pay: false, rolled_at: 0 } }
    const AI: Strategy = Strategy { buy: true, build: true, reserve: 5_000_000, build_floor: 10_000_000 };

    #[test]
    fn buys_with_reserve_and_builds_on_landing() {
        let mut s = fixture();
        s.build_on_landing = true;
        s.active = 2; // seat 2 on 3; 1 + 2 → 6 (Asset C, 30.000000, unowned)
        assert_eq!(decide(&s, 500, 500, &roll(2, 1, 2), 40_000_000, AI).unwrap(), Move::Turn { buy: true, sell_square: None, build: false });
        assert_eq!(decide(&s, 500, 500, &roll(2, 1, 2), 34_000_000, AI).unwrap(), Move::Turn { buy: false, sell_square: None, build: false },
            "4.000000 left would be under the 5.000000 reserve");
        s.owners[6] = 2;
        assert_eq!(decide(&s, 500, 500, &roll(2, 1, 2), 15_000_000, AI).unwrap(), Move::Turn { buy: false, sell_square: None, build: true });
        assert_eq!(decide(&s, 500, 500, &roll(2, 1, 2), 14_999_999, AI).unwrap(), Move::Turn { buy: false, sell_square: None, build: false });
        let pass = Strategy { buy: false, build: false, reserve: 0, build_floor: 0 };
        assert_eq!(decide(&s, 500, 500, &roll(2, 1, 2), 90_000_000, pass).unwrap(), Move::Turn { buy: false, sell_square: None, build: false });
    }

    #[test]
    fn shortfall_sells_then_regenerates() {
        let mut s = fixture();
        s.active = 2;
        s.owners[13] = 1; // Asset F (20.000000, rent 2.000000) owned by seat 1; seat 2 on 3 rolls 6 + 4 → 13
        assert_eq!(decide(&s, 500, 500, &roll(2, 6, 4), 0, AI).unwrap(), Move::Regenerate);
        s.owners[1] = 2; // Asset A, buyback 8.000000: settles the rent inside the turn
        assert_eq!(decide(&s, 500, 500, &roll(2, 6, 4), 0, AI).unwrap(), Move::Turn { buy: false, sell_square: Some(1), build: false });
    }
}

// End of app/src/ai.rs
// Location: app/src/ai.rs · Created: 2026-11-16 15:40:30 UTC
