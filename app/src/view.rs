//! Ootle Game Engine | app/src/view.rs
//! Version: v0.4.0
//! The game view the frontend renders. Everything the page shows is computed here: seat count,
//! pieces, balances, property values, rent, buyback, next construction cost, the pending roll
//! with its settlement preview, and whether the turn needs a decision. The page computes nothing.
//! v0.3.0: who holds each seat (human, AI, open), which seats the viewer controls, net worth and
//! stint, the economics, the build-on-landing option, and the regeneration option.
//! v0.4.0: seat colours belong to the seat and pieces are shapes (board.rs); every square carries its
//! catalogue entry (zone and business of a lot, the scene of a special square) for the landing scenes.

use crate::{
    board::{self, ttari},
    catalogue::Catalogue,
    lots::LotChoice,
    chain::{Balances, Snapshot},
    claims::{Holder, SeatClaim},
    dice::Roll,
    sim::{self, Action},
};
use ootle_town_square::rules;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Ctx<'a> {
    pub snap: &'a Snapshot,
    pub bal: &'a Balances,
    pub roll: Option<&'a Roll>,
    pub turn_seen_at: u64,
    pub now: u64,
    pub decision_window_secs: u64,
    pub auto_roll_secs: u64,
    pub max_fee: u64,
    pub seat_start: u64,
    pub live_enabled: bool,
    pub network: &'a str,
    pub history: Vec<Value>,
    pub claims: &'a [SeatClaim],
    pub mine: &'a [u8],
    pub driver_error: Option<String>,
    pub catalogue: &'a Catalogue,
    /// The lot store's records for this game (square → business).
    pub lots: &'a BTreeMap<u16, LotChoice>,
}

/// Profile (name, icon) of the identity that claimed `seat`, if registered.
pub fn profile(snap: &Snapshot, seat: u8) -> (Option<String>, Option<String>, Option<u8>) {
    let id = snap.lobby.claims.iter().find(|c| c.game_id == snap.game.id && c.seat == seat).map(|c| c.identity.clone());
    let p = id.as_ref().and_then(|i| snap.registry.profiles.iter().find(|p| &p.identity == i));
    (id.clone(), p.map(|p| p.name.clone()), p.map(|p| p.icon))
}

pub fn event_view(s: &crate::decode::Session) -> Value {
    let label = match s.event_kind {
        0 => String::new(),
        1 | 2 => sim::card_label(s.event_kind, s.event_card).to_owned(),
        3 => format!("Rent paid to Seat {}", s.event_recipient),
        4 => "Sent to Maintenance".to_owned(),
        5 => "Held in Maintenance after a failed roll".to_owned(),
        6 => format!("Seat {} reset to GO", s.event_seat),
        _ => "Unknown event".to_owned(),
    };
    json!({"kind": s.event_kind, "card": s.event_card, "amount": s.event_amount, "amount_ttari": ttari(s.event_amount),
        "positive": s.event_positive, "seat": s.event_seat, "square": s.event_square, "recipient": s.event_recipient, "label": label})
}

fn preview(c: &Ctx, r: &Roll) -> Value {
    let s = &c.snap.session;
    let (cb, hb) = (c.snap.treasury.community_bps, c.snap.treasury.chance_bps);
    let base = Action::Turn { seat: r.seat, die1: r.die1, die2: r.die2, pay: r.pay, buy: false, sell_square: None };
    let o = match sim::simulate(s, cb, hb, &base) {
        Ok(o) => o,
        Err(e) => return json!({"error": e}),
    };
    let seat_bal = c.bal.seats[usize::from(r.seat - 1)];
    let house_ok = c.bal.house >= o.house_withdraw + c.max_fee;
    let enough = seat_bal >= o.seat_withdraw;
    let buy = o.story.buy_option.and_then(|_| {
        let a = Action::Turn { seat: r.seat, die1: r.die1, die2: r.die2, pay: r.pay, buy: true, sell_square: None };
        sim::simulate(s, cb, hb, &a).ok()
    }).map(|b| json!({"square": b.story.to, "name": board::square_name(b.story.to), "price": b.story.purchase,
        "price_ttari": ttari(b.story.purchase), "seat_funding": b.seat_withdraw, "affordable": seat_bal >= b.seat_withdraw,
        "balance_after": seat_bal as i128 + b.ledger.seats[usize::from(r.seat - 1)]}));
    // Recovery sales: each owned asset that, sold inside this turn, settles it (the player chooses).
    let recovery: Vec<Value> = if enough { vec![] } else {
        (0..rules::BOARD_SIZE).filter(|q| s.owners[usize::from(*q)] == r.seat).filter_map(|q| {
            let a = Action::Turn { seat: r.seat, die1: r.die1, die2: r.die2, pay: r.pay, buy: false, sell_square: Some(q) };
            let x = sim::simulate(s, cb, hb, &a).ok()?;
            Some(json!({"square": q, "name": board::square_name(q), "buyback": x.story.sale, "buyback_ttari": ttari(x.story.sale),
                "settles": seat_bal >= x.seat_withdraw && c.bal.house >= x.house_withdraw + c.max_fee}))
        }).collect()
    };
    // Build on landing: the seat lands on an asset it already owns (one level, before the next move).
    let build = (s.build_on_landing && enough).then(|| {
        let a = &o.after;
        let q = a.landed_square;
        if a.landed_seat != r.seat || q == rules::NO_SQUARE || !rules::is_asset(q) || s.owners[usize::from(q)] != r.seat
            || a.improvements[usize::from(q)] >= 3 { return None; }
        let lvl = a.improvements[usize::from(q)];
        let cost = rules::next_improvement_cost(lvl, &a.params());
        let after_turn = seat_bal as i128 + o.ledger.seats[usize::from(r.seat - 1)];
        Some(json!({"square": q, "name": board::square_name(q), "label": if lvl < 2 { "house" } else { "hotel" },
            "cost": cost, "cost_ttari": ttari(cost), "affordable": after_turn >= i128::from(cost),
            "rent_now_ttari": ttari(rules::rent_amount(q, lvl)), "rent_after_ttari": ttari(rules::rent_amount(q, lvl + 1)),
            "balance_after": after_turn - i128::from(cost)}))
    }).flatten();
    let owns_any = (0..rules::BOARD_SIZE).any(|q| s.owners[usize::from(q)] == r.seat);
    let regenerate = !enough && !owns_any;
    let seat_delta = o.ledger.seats[usize::from(r.seat - 1)];
    let reason = if !house_ok { "house_reserve" } else if regenerate { "regenerate" } else if !enough { "recovery_sale" }
        else if buy.is_some() { "purchase" } else if build.is_some() { "build" } else { "" };
    json!({
        "story": o.story, "seat_funding": o.seat_withdraw, "house_funding": o.house_withdraw,
        "seat_balance": seat_bal, "seat_delta": seat_delta, "seat_delta_ttari": board::ttari_signed(seat_delta),
        "balance_after": seat_bal as i128 + seat_delta, "enough": enough,
        "house_ok": house_ok, "house_required": o.house_withdraw + c.max_fee, "house_available": c.bal.house,
        "buy": buy, "build": build, "recovery": recovery, "regenerate": regenerate,
        "decision_required": !reason.is_empty(), "decision_reason": reason,
    })
}

pub fn game_view(c: &Ctx) -> Value {
    let snap = c.snap;
    let s = &snap.session;
    let g = &snap.game;
    let p = s.params();
    let lobby_game = snap.lobby.games.iter().find(|x| x.id == g.id);
    let type_name = snap.lobby.types.iter().find(|t| Some(t.id) == lobby_game.map(|x| x.type_id))
        .map(|t| t.name.clone()).unwrap_or_else(|| "Town Square".into());
    let status = match lobby_game.map(|x| x.status) { Some(0) => "open", Some(1) => "started", Some(2) => "finished", _ => "unknown" };

    let squares: Vec<Value> = (0..rules::BOARD_SIZE).map(|q| {
        let i = usize::from(q);
        let (owner, level) = (s.owners[i], s.improvements[i]);
        let mut v = json!({"square": q, "name": board::square_name(q), "kind": board::square_kind(q), "owner": owner, "level": level});
        if let Some(lot) = c.catalogue.lot(q) {
            v["zone"] = json!(lot.zone);
            v["zone_name"] = json!(lot.zone_name);
            v["district"] = json!(lot.district);
            if let Some(ch) = c.lots.get(&q).filter(|_| owner != 0) {
                v["business"] = json!(c.catalogue.business(q, &ch.business));
                v["business_source"] = json!(ch.source);
            }
        }
        if let Some(sp) = c.catalogue.special(q) { v["special"] = json!(sp); }
        if rules::is_asset(q) {
            let value = rules::developed_value(q, level, &p);
            v["price"] = json!(rules::base_price(q));
            v["price_ttari"] = json!(ttari(rules::base_price(q)));
            v["value"] = json!(value);
            v["value_ttari"] = json!(ttari(value));
            v["rent"] = json!(rules::rent_amount(q, level));
            v["rent_ttari"] = json!(ttari(rules::rent_amount(q, level)));
            v["buyback"] = json!(rules::buyback_value(q, level, &p));
            v["buyback_ttari"] = json!(ttari(rules::buyback_value(q, level, &p)));
            if level < 3 {
                let cost = rules::next_improvement_cost(level, &p);
                v["next_cost"] = json!(cost);
                v["next_cost_ttari"] = json!(ttari(cost));
                v["next_label"] = json!(if level < 2 { "house" } else { "hotel" });
                v["rent_after_build"] = json!(ttari(rules::rent_amount(q, level + 1)));
            }
        } else {
            let effect = match q {
                0 if s.go_scaled => format!("Pass GO reward {} tTARI now (scaled: {} × unowned ÷ 22, floor {})",
                ttari(s.landing_params().go_reward), ttari(p.go_reward), ttari(s.go_floor)),
            0 => format!("Pass GO reward {} tTARI", ttari(p.go_reward)),
                4 | 24 | 37 => format!("Fee {} tTARI", ttari(p.fee_amount)),
                7 => format!("Penalty {} tTARI", ttari(p.penalty_amount)),
                9 => format!("Reward {} tTARI", ttari(p.square_reward)),
                10 => "Maintenance holding square".into(),
                30 => "Sends the seat to Maintenance".into(),
                2 | 22 | 33 => "Chance card".into(),
                5 | 17 | 27 => "Community card".into(),
                _ => String::new(),
            };
            v["effect"] = json!(effect);
        }
        v
    }).collect();

    let seats: Vec<Value> = (1..=s.capacity).map(|n| {
        let i = usize::from(n - 1);
        let (identity, pname, _icon) = profile(snap, n);
        let claim = c.claims.get(i).cloned().unwrap_or_default();
        let pc = board::piece(n, claim.vehicle);
        let color = board::seat_colour(n);
        let (kind, player) = match &claim.holder {
            Holder::Human { name, .. } => ("human", Some(name.clone())),
            Holder::Ai => ("ai", Some(format!("AI {n}"))),
            Holder::Open => ("open", None),
        };
        let net = c.bal.seats[i] + s.buyback_total(n);
        let owned: Vec<u16> = (0..rules::BOARD_SIZE).filter(|q| s.owners[usize::from(*q)] == n).collect();
        let worth: u64 = owned.iter().map(|q| rules::developed_value(*q, s.improvements[usize::from(*q)], &p)).sum();
        json!({"seat": n, "piece_id": pc.id, "piece": pc.name, "color": color,
            "profile": pname, "identity": identity, "account": g.seats[i].component, "account_name": g.seats[i].account_name,
            "balance": c.bal.seats[i], "balance_ttari": ttari(c.bal.seats[i]),
            "position": s.positions[i], "square_name": board::square_name(s.positions[i]),
            "held": s.held[i], "held_attempts": s.held_attempts[i], "held_owed": s.held_owed[i], "held_owed_ttari": ttari(s.held_owed[i]),
            "out": s.out[i], "active": s.started && s.active == n, "assets": owned, "property_value": worth,
            "property_value_ttari": ttari(worth), "kind": kind, "player": player, "mine": c.mine.contains(&n),
            "net_worth": net, "net_worth_ttari": ttari(net), "stint_rounds": s.round.saturating_sub(s.seat_since[i]),
            "stint_since": s.seat_since[i], "resets": s.resets[i]})
    }).collect();

    let offer = s.has_offer().then(|| json!({"square": s.offer_square, "name": board::square_name(s.offer_square),
        "price": s.offer_price, "price_ttari": ttari(s.offer_price), "seller": s.offer_seller, "buyer": s.offer_buyer, "nonce": s.offer_nonce}));
    let roll = c.roll.map(|r| json!({"die1": r.die1, "die2": r.die2, "pay": r.pay, "seat": r.seat, "rolled_at": r.rolled_at, "preview": preview(c, r)}));
    let owned = s.owners.iter().filter(|o| **o != 0).count();
    let next = if s.started { rules::next_seat(s.active, &s.out).0 } else { 1 };
    let elapsed = c.now.saturating_sub(c.turn_seen_at);
    let holder_of = |n: u8| c.claims.get(usize::from(n.max(1) - 1)).map(|x| x.holder.clone()).unwrap_or(Holder::Open);
    let active_kind = match holder_of(s.active) { Holder::Human { .. } => "human", Holder::Ai => "ai", Holder::Open => "open" };
    let offer_buyer_mine = s.has_offer() && c.mine.contains(&s.offer_buyer);
    json!({
        "game": {"id": g.id, "type_name": type_name, "capacity": s.capacity, "seats_added": s.seats.len(),
                 "treasury_mode": g.treasury_mode, "status": status, "started": s.started,
                 "phase": if s.round == 1 && s.turn_count == 0 { "new" } else { "in_progress" }},
        "network": c.network, "live_enabled": c.live_enabled,
        "round": s.round, "active": s.active, "next": next, "turn_count": s.turn_count, "doubles_streak": s.doubles_streak,
        "last_dice": [s.last_die1, s.last_die2],
        "decision": {"window_secs": c.decision_window_secs, "seen_at": c.turn_seen_at, "elapsed_secs": elapsed,
                     "remaining_secs": c.decision_window_secs.saturating_sub(elapsed),
                     "auto_roll_secs": c.auto_roll_secs, "enforced": c.auto_roll_secs > 0,
                     "auto_roll_in_secs": if c.auto_roll_secs > 0 { Some(c.auto_roll_secs.saturating_sub(elapsed)) } else { None }},
        "viewer": {"seats": c.mine, "my_turn": c.mine.contains(&s.active), "offer_for_me": offer_buyer_mine,
                   "active_kind": active_kind},
        "driver_error": c.driver_error,
        "economics": {"go_reward_now": s.landing_params().go_reward, "go_reward_now_ttari": ttari(s.landing_params().go_reward),
                      "go_base_ttari": ttari(p.go_reward), "go_floor_ttari": ttari(s.go_floor), "go_scaled": s.go_scaled,
                      "rent_fee_bps": s.rent_fee_bps, "build_on_landing": s.build_on_landing,
                      "fee_ttari": ttari(p.fee_amount), "penalty_ttari": ttari(p.penalty_amount),
                      "seat_start_ttari": ttari(c.seat_start)},
        "seats": seats, "squares": squares, "assets_owned": owned, "assets_total": rules::ASSETS.len(),
        "pools": {"house": c.bal.house, "house_ttari": ttari(c.bal.house), "community": c.bal.community,
                  "community_ttari": ttari(c.bal.community), "chance": c.bal.chance, "chance_ttari": ttari(c.bal.chance),
                  "community_bps": snap.treasury.community_bps, "chance_bps": snap.treasury.chance_bps},
        "params": {"go_reward": p.go_reward, "square_reward": p.square_reward, "fee_amount": p.fee_amount,
                   "penalty_amount": p.penalty_amount, "shop_fee": p.shop_fee, "storage_fee": p.storage_fee,
                   "house_cost": p.house_cost, "hotel_cost": p.hotel_cost, "buyback_pct": p.buyback_pct,
                   "shop_fee_ttari": ttari(p.shop_fee), "storage_fee_ttari": ttari(p.storage_fee)},
        "event": event_view(s), "offer": offer, "roll": roll,
        "landed": {"seat": s.landed_seat, "square": if s.landed_square == rules::NO_SQUARE { None } else { Some(s.landed_square) }},
        "substate_version": snap.session_version, "history": c.history,
    })
}

// End of app/src/view.rs
// Location: app/src/view.rs · Created: 2026-11-16 15:40:30 UTC
