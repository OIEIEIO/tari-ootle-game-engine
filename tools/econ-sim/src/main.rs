//! Ootle Game Engine | tools/econ-sim/src/main.rs
//! Version: v0.3.2
//! Monte Carlo economics of Town Square Classic. Each game starts a fresh session (all seats on
//! GO, nothing owned) and plays until the round limit or until one seat is left. Every action is
//! settled by ootle_app::sim::simulate, so amounts follow the contract to the base unit.
//!
//! v0.2.0 adds three economic levers that the contract does not have yet (modelled here only):
//!   - GO scaling: GO = max(floor, base × unowned assets ÷ 22), set before every turn;
//!   - rent fee: a percentage of each rent payment, taken from the owner's receipt and routed
//!     through the Treasury split (House 90 / Community 5 / Chance 5, each share floored);
//!   - fee-square multiplier: scales the fee squares (4, 24, 37) and the penalty square (7).
//! A trade fee is not modelled: the player behaviour below makes no private sales.
//!
//! Player behaviour (fixed in this version):
//!   - buy an unowned landing asset whenever the seat can fund the purchase;
//!   - no construction, no private sales, no shop fee gifts;
//!   - a held seat rolls for doubles; the third failure forces payment;
//!   - shortfall: one asset sold inside the turn (cheapest buyback that covers it); if none covers
//!     it alone, assets are sold to the House one by one, cheapest first, until the turn is funded;
//!   - still short with nothing left to sell: the seat is out (remove_seat semantics).
//! Exit value of a seat at the end: cash plus its assets at buyback value (80%).
//! The House pays a fixed network fee per transaction. The House may go negative in the model.
//!
//! Single run:  econ-sim --games 20000 --go-base 20 --go-floor 20 --rent-fee 0 --fee-mult 1
//! Grid search: econ-sim --search --games 4000
//! Seat mixes (v0.3.0): econ-sim --mixes HHHH,HAAA,HHAA,HHHA,AAAA --games 20000   (H human, B human builder, A AI; --land-build: build only on the owned asset landed on; --regen: regeneration; see Kind)
//! Regeneration (v0.3.2, seat mixes only, off unless --regen is given): a seat that cannot fund its
//! turn after selling everything is not removed. Its assets go back to the House (unowned, no
//! improvements), it returns to GO, its hold state clears, and the House tops its cash up to the seat
//! start (House cost = seat start minus the cash the seat still had). Its turn ends there.
//! Amounts are tTARI with up to 6 decimals; percentages are whole numbers. JSON on stdout.

use ootle_app::{decode::Session, sim::{self, Action}};
use ootle_town_square::rules::{self, NO_SQUARE};
use serde_json::{json, Value};

const UNIT: i128 = 1_000_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 { let mut x = self.0; x ^= x << 13; x ^= x >> 7; x ^= x << 17; self.0 = x; x.wrapping_mul(0x2545F4914F6CDD1D) }
    fn die(&mut self) -> u8 { loop { let v = self.next() >> 61; if v < 6 { return v as u8 + 1; } } }
}

fn units(s: &str) -> i128 {
    let (w, f) = s.split_once('.').unwrap_or((s, ""));
    assert!(f.len() <= 6 && w.bytes().chain(f.bytes()).all(|b| b.is_ascii_digit()), "bad amount {s}");
    w.parse::<i128>().unwrap() * UNIT + format!("{f:0<6}").parse::<i128>().unwrap()
}
fn t(u: i128) -> String { let s = if u < 0 { "-" } else { "" }; let a = u.abs(); format!("{s}{}.{:06}", a / UNIT, a % UNIT) }
fn pct(v: &mut [i128], p: f64) -> i128 { v.sort_unstable(); v[((v.len() - 1) as f64 * p).round() as usize] }

#[derive(Clone, Copy, Debug)]
struct Econ { go_base: i128, go_floor: i128, rent_fee_pct: i128, fee_mult: u64 }

#[derive(Clone, Copy)]
struct Setup { seats: usize, rounds: u64, house0: i128, seat0: i128, net_fee: i128 }

fn fresh(n: usize, e: &Econ) -> Session {
    let addr = |i: usize| format!("component_{:064x}", i);
    Session {
        game_id: 1, lobby: addr(9001), treasury: addr(9002), resource: format!("resource_{}", "01".repeat(32)),
        go_reward: e.go_base as u64, square_reward: 2_000_000, fee_amount: 1_000_000 * e.fee_mult,
        penalty_amount: 2_000_000 * e.fee_mult, shop_fee: 5_000_000, storage_fee: 2_000_000,
        house_cost: 5_000_000, hotel_cost: 15_000_000, buyback_pct: 80,
        capacity: n as u8, seats: (1..=n).map(addr).collect(), out: vec![false; n], started: true,
        round: 1, active: 1, turn_count: 0, doubles_streak: 0, positions: vec![0; n], last_die1: 0, last_die2: 0,
        owners: vec![0; 40], improvements: vec![0; 40], held: vec![false; n], held_attempts: vec![0; n], held_owed: vec![0; n],
        event_kind: 0, event_card: 255, event_amount: 0, event_positive: false, event_seat: 0, event_square: 0, event_recipient: 0,
        offer_square: NO_SQUARE, offer_price: 0, offer_seller: 0, offer_buyer: 0, offer_nonce: 0,
        // v0.3.0 session fields: the economics are modelled here (Econ), so the contract's own stay off.
        go_floor: e.go_base as u64, go_scaled: false, rent_fee_bps: 0, build_on_landing: false,
        landed_seat: 0, landed_square: NO_SQUARE, seat_since: vec![1; n], resets: vec![0; n],
    }
}

/// remove_seat, as in contracts/town_square/src/lib.rs.
fn remove_seat(s: &mut Session, seat: u8) {
    let i = usize::from(seat - 1);
    s.out[i] = true; s.held[i] = false; s.held_attempts[i] = 0; s.held_owed[i] = 0;
    for q in 0..40 { if s.owners[q] == seat { s.owners[q] = 0; s.improvements[q] = 0; } }
    if s.active == seat {
        s.doubles_streak = 0;
        let (next, wrapped) = rules::next_seat(s.active, &s.out);
        if wrapped { s.round += 1; }
        s.active = next;
    }
}

#[derive(Default, Clone)]
struct Flows { go: i128, square_reward: i128, card_plus: i128, card_minus: i128, square_fees: i128, maintenance: i128,
    purchases: i128, buybacks: i128, rent: i128, rent_fees: i128, tx_fees: i128, txs: i128, forced_sales: i128, eliminated: i128,
    regens: i128, regen_cost: i128 }

struct Game { s: Session, bal: Vec<i128>, house: i128, community: i128, chance: i128, f: Flows, house_min: i128 }

impl Game {
    fn apply(&mut self, a: &Action, net_fee: i128, rent_fee_pct: i128) {
        let o = sim::simulate(&self.s, 500, 500, a).expect("planned action must be valid");
        for (b, d) in self.bal.iter_mut().zip(&o.ledger.seats) { *b += d; }
        self.house += o.ledger.house - net_fee;
        self.community += o.ledger.community;
        self.chance += o.ledger.chance;
        let st = &o.story;
        if st.rent > 0 && rent_fee_pct > 0 {
            // Rent fee: taken from the owner's receipt, routed through the Treasury split.
            let fee = i128::from(st.rent) * rent_fee_pct / 100;
            self.bal[usize::from(st.rent_owner - 1)] -= fee;
            let c = fee * 500 / 10_000;
            let h = fee * 500 / 10_000;
            self.community += c; self.chance += h; self.house += fee - c - h;
            self.f.rent_fees += fee;
        }
        self.house_min = self.house_min.min(self.house);
        let f = &mut self.f;
        f.txs += 1; f.tx_fees += net_fee;
        f.go += i128::from(st.go_reward); f.square_reward += i128::from(st.square_reward);
        if st.card_kind != 0 { if st.card_positive { f.card_plus += i128::from(st.card_amount) } else { f.card_minus += i128::from(st.card_amount) } }
        let neg_card = if st.card_kind != 0 && !st.card_positive { st.card_amount } else { 0 };
        f.square_fees += i128::from(st.penalty - neg_card);
        f.maintenance += i128::from(st.maintenance_fee);
        f.purchases += i128::from(st.purchase); f.buybacks += i128::from(st.sale); f.rent += i128::from(st.rent);
        self.s = o.after;
    }
}

/// Regeneration (v0.3.2): the seat stays in play, back on GO with the seat start; see the header.
fn regen_seat(g: &mut Game, seat: u8, seat0: i128) {
    let i = usize::from(seat - 1);
    {
        let s = &mut g.s;
        s.held[i] = false; s.held_attempts[i] = 0; s.held_owed[i] = 0;
        for q in 0..40 { if s.owners[q] == seat { s.owners[q] = 0; s.improvements[q] = 0; } }
        s.positions[i] = 0;
        if s.active == seat {
            s.doubles_streak = 0;
            let (next, wrapped) = rules::next_seat(s.active, &s.out);
            if wrapped { s.round += 1; }
            s.active = next;
        }
    }
    g.house -= seat0 - g.bal[i];
    g.f.regen_cost += seat0 - g.bal[i];
    g.f.regens += 1;
    g.bal[i] = seat0;
    g.house_min = g.house_min.min(g.house);
}

struct Outcome { g: Game, house_by_round: Vec<i128>, go_by_round: Vec<i128> }

fn play(st: &Setup, e: &Econ, rng: &mut Rng) -> Outcome {
    let n = st.seats;
    let mut g = Game { s: fresh(n, e), bal: vec![st.seat0; n], house: st.house0, community: 0, chance: 0, f: Flows::default(), house_min: st.house0 };
    let mut house_by_round = Vec::with_capacity(st.rounds as usize);
    let mut go_by_round = Vec::with_capacity(st.rounds as usize);
    let mut go_mark = 0;
    while g.s.round <= st.rounds && g.s.out.iter().filter(|o| !**o).count() > 1 {
        let unowned = g.s.owners.iter().enumerate().filter(|(q, o)| rules::is_asset(*q as u16) && **o == 0).count() as i128;
        g.s.go_reward = (e.go_base * unowned / 22).max(e.go_floor) as u64;
        let a = g.s.active;
        let i = usize::from(a - 1);
        let (d1, d2) = (rng.die(), rng.die());
        let turn = |buy, sell: Option<u16>| Action::Turn { seat: a, die1: d1, die2: d2, pay: false, buy, sell_square: sell };
        let base = sim::simulate(&g.s, 500, 500, &turn(false, None)).expect("turn");
        let mut action = turn(false, None);
        let mut skip = false;
        if (base.seat_withdraw as i128) > g.bal[i] {
            let mut owned: Vec<u16> = (0..40u16).filter(|q| g.s.owners[usize::from(*q)] == a).collect();
            owned.sort_by_key(|q| rules::buyback_value(*q, g.s.improvements[usize::from(*q)], &g.s.params()));
            let inside = owned.iter().copied().find(|q| {
                sim::simulate(&g.s, 500, 500, &turn(false, Some(*q))).map(|o| (o.seat_withdraw as i128) <= g.bal[i]).unwrap_or(false)
            });
            if let Some(q) = inside {
                action = turn(false, Some(q));
                g.f.forced_sales += 1;
            } else {
                let mut funded = false;
                for q in owned {
                    g.apply(&Action::Sell { seat: a, square: q }, st.net_fee, e.rent_fee_pct);
                    g.f.forced_sales += 1;
                    let o = sim::simulate(&g.s, 500, 500, &turn(false, None)).expect("turn");
                    if (o.seat_withdraw as i128) <= g.bal[i] { funded = true; break; }
                }
                if !funded { remove_seat(&mut g.s, a); g.f.eliminated += 1; skip = true; }
            }
        } else if base.story.buy_option.is_some() {
            let b = sim::simulate(&g.s, 500, 500, &turn(true, None)).expect("buy");
            if (b.seat_withdraw as i128) <= g.bal[i] { action = turn(true, None); }
        }
        if !skip { g.apply(&action, st.net_fee, e.rent_fee_pct); }
        while (house_by_round.len() as u64) < (g.s.round - 1).min(st.rounds) {
            house_by_round.push(g.house);
            go_by_round.push(g.f.go - go_mark);
            go_mark = g.f.go;
        }
    }
    while (house_by_round.len() as u64) < st.rounds { house_by_round.push(g.house); go_by_round.push(0); }
    Outcome { g, house_by_round, go_by_round }
}

/// Exit value: cash plus assets at buyback value.
fn exit_value(g: &Game, seat: usize) -> i128 {
    let s = &g.s;
    let p = s.params();
    g.bal[seat] + (0..40u16).filter(|q| s.owners[usize::from(*q)] as usize == seat + 1)
        .map(|q| i128::from(rules::buyback_value(q, s.improvements[usize::from(q)], &p))).sum::<i128>()
}

struct Summary { v: Value, never_below: f64, players_above: f64, margin_mean: i128 }

fn run(st: &Setup, e: &Econ, games: usize, seed: u64, detail: bool) -> Summary {
    let mut rng = Rng(seed | 1);
    let r = st.rounds as usize;
    let mut per_round: Vec<Vec<i128>> = vec![Vec::with_capacity(games); r];
    let mut go_round: Vec<i128> = vec![0; r];
    let (mut end, mut min, mut dip) = (vec![], vec![], vec![]);
    let mut never_below = 0usize;
    let mut tot = Flows::default();
    let mut ranks: Vec<Vec<i128>> = vec![vec![]; st.seats];
    let (mut above, mut seats_total) = (0usize, 0usize);
    let (mut comm, mut chan) = (0i128, 0i128);
    for _ in 0..games {
        let o = play(st, e, &mut rng);
        let g = &o.g;
        if g.house_min >= st.house0 { never_below += 1; }
        end.push(g.house); min.push(g.house_min); dip.push((st.house0 - g.house_min).max(0));
        comm += g.community; chan += g.chance;
        let mut ev: Vec<i128> = (0..st.seats).map(|k| exit_value(g, k)).collect();
        for v in &ev { seats_total += 1; if *v > st.seat0 { above += 1; } }
        ev.sort_unstable_by(|a, b| b.cmp(a));
        for (k, v) in ev.into_iter().enumerate() { ranks[k].push(v); }
        for k in 0..r { per_round[k].push(o.house_by_round[k]); go_round[k] += o.go_by_round[k]; }
        let f = &g.f;
        for (a, b) in [(&mut tot.go, f.go), (&mut tot.square_reward, f.square_reward), (&mut tot.card_plus, f.card_plus),
            (&mut tot.card_minus, f.card_minus), (&mut tot.square_fees, f.square_fees), (&mut tot.maintenance, f.maintenance),
            (&mut tot.purchases, f.purchases), (&mut tot.buybacks, f.buybacks), (&mut tot.rent, f.rent), (&mut tot.rent_fees, f.rent_fees),
            (&mut tot.tx_fees, f.tx_fees), (&mut tot.txs, f.txs), (&mut tot.forced_sales, f.forced_sales), (&mut tot.eliminated, f.eliminated)] { *a += b; }
    }
    let gm = games as i128;
    let mean = |x: i128| t(x / gm);
    let margin_mean = end.iter().sum::<i128>() / gm - st.house0;
    let rank_names = ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth"];
    let mut rank_json = serde_json::Map::new();
    for (k, v) in ranks.iter_mut().enumerate().take(8) {
        rank_json.insert(rank_names[k].into(), json!({"p50": t(pct(v, 0.5)), "p5": t(pct(v, 0.05)), "p95": t(pct(v, 0.95))}));
    }
    let never = never_below as f64 / games as f64;
    let players_above = above as f64 / seats_total as f64;
    let mut v = json!({
        "econ": {"go_base": t(e.go_base), "go_floor": t(e.go_floor), "rent_fee_pct": e.rent_fee_pct, "fee_square_multiplier": e.fee_mult},
        "games": games,
        "house": {"never_below_start_share": format!("{never:.4}"), "margin_mean": t(margin_mean),
                  "end_p5": t(pct(&mut end.clone(), 0.05)), "end_p50": t(pct(&mut end.clone(), 0.5)), "end_p95": t(pct(&mut end.clone(), 0.95)),
                  "lowest_p5": t(pct(&mut min.clone(), 0.05)), "lowest_p50": t(pct(&mut min.clone(), 0.5)),
                  "largest_dip_p95": t(pct(&mut dip.clone(), 0.95)), "largest_dip_max": t(*dip.iter().max().unwrap())},
        "players": {"exit_value_above_stake_share": format!("{players_above:.4}"),
                    "seats_out_per_game": format!("{:.4}", tot.eliminated as f64 / games as f64),
                    "exit_value_by_rank": rank_json},
        "pools": {"community_end_mean": mean(comm), "chance_end_mean": mean(chan)},
    });
    if detail {
        v["flows_mean_per_game"] = json!({"go_rewards": mean(tot.go), "square_9_rewards": mean(tot.square_reward),
            "positive_cards": mean(tot.card_plus), "negative_cards": mean(tot.card_minus), "fee_and_penalty_squares": mean(tot.square_fees),
            "maintenance_payments": mean(tot.maintenance), "purchases": mean(tot.purchases), "buybacks": mean(tot.buybacks),
            "rent_paid": mean(tot.rent), "rent_fees": mean(tot.rent_fees), "network_fees": mean(tot.tx_fees),
            "transactions": format!("{:.4}", tot.txs as f64 / games as f64), "forced_sales": format!("{:.4}", tot.forced_sales as f64 / games as f64)});
        v["per_round"] = Value::Array((0..r).map(|k| {
            let mut h = per_round[k].clone();
            let below = h.iter().filter(|x| **x < st.house0).count();
            json!({"round": k + 1, "house_mean": t(h.iter().sum::<i128>() / gm), "house_p5": t(pct(&mut h, 0.05)),
                   "house_p50": t(pct(&mut h, 0.5)), "house_p95": t(pct(&mut h, 0.95)),
                   "share_below_start": format!("{:.4}", below as f64 / games as f64), "go_paid_mean": t(go_round[k] / gm)})
        }).collect());
    }
    Summary { v, never_below: never, players_above, margin_mean }
}

// ---------------------------------------------------------------- v0.3.0: mixed seat types

/// Seat behaviour.
///   H (human model, "Baseline"): buys any unowned landing asset it can fund; never builds.
///   B (human model, "Human builder", v0.3.1): buys as H; builds one level when allowed and at least
///     `human_build_floor` tTARI stays after building.
///   A (AI model, "Builder"): buys only if at least `reserve` tTARI stays after the purchase; builds
///     one level when allowed and at least `build_floor` tTARI stays after building.
/// When builders may build:
///   - default: before each of its turns, on the owned asset with the largest rent increase per tTARI
///     of construction cost (any owned asset, as the current contract allows);
///   - `land_build` (v0.3.1): only on the owned asset the seat lands on this turn, one level per landing.
///     The contract has no build-on-landing step yet, so the build is applied just before the move onto
///     that square; the cost and level change are identical, and no rent is due on one's own asset.
/// All: held seats roll for doubles; shortfall handling as in `play`; no private sales.
/// `regen` (v0.3.2): a seat out of funds regenerates (`regen_seat`) instead of leaving the game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind { H, B, A }

#[derive(Clone, Copy)]
struct AiCfg { reserve: i128, build_floor: i128, human_build_floor: i128, land_build: bool, regen: bool }

impl AiCfg {
    fn floor(&self, k: Kind) -> Option<i128> {
        match k { Kind::H => None, Kind::B => Some(self.human_build_floor), Kind::A => Some(self.build_floor) }
    }
}

#[derive(Default, Clone)]
struct SeatStats { rent_paid: i128, rent_received: i128, builds: i128, purchases: i128, regens: i128 }

/// The owned asset with the largest rent increase per unit of construction cost (cross-multiplied, exact).
fn best_build(s: &Session, seat: u8, cash: i128, floor: i128) -> Option<u16> {
    let p = s.params();
    let mut best: Option<(u16, u64, u64)> = None; // (square, rent gain, cost)
    for q in 0..40u16 {
        let i = usize::from(q);
        if s.owners[i] != seat || s.improvements[i] >= 3 { continue; }
        let lvl = s.improvements[i];
        let cost = rules::next_improvement_cost(lvl, &p);
        if cash - i128::from(cost) < floor { continue; }
        let gain = rules::rent_amount(q, lvl + 1) - rules::rent_amount(q, lvl);
        if best.map_or(true, |(_, g, c)| u128::from(gain) * u128::from(c) > u128::from(g) * u128::from(cost)) {
            best = Some((q, gain, cost));
        }
    }
    best.map(|b| b.0)
}

fn play_mixed(st: &Setup, e: &Econ, kinds: &[Kind], ai: &AiCfg, rng: &mut Rng) -> (Game, Vec<SeatStats>) {
    let n = st.seats;
    let mut g = Game { s: fresh(n, e), bal: vec![st.seat0; n], house: st.house0, community: 0, chance: 0, f: Flows::default(), house_min: st.house0 };
    let mut ss = vec![SeatStats::default(); n];
    while g.s.round <= st.rounds && g.s.out.iter().filter(|o| !**o).count() > 1 {
        let unowned = g.s.owners.iter().enumerate().filter(|(q, o)| rules::is_asset(*q as u16) && **o == 0).count() as i128;
        g.s.go_reward = (e.go_base * unowned / 22).max(e.go_floor) as u64;
        let a = g.s.active;
        let i = usize::from(a - 1);
        if let (Some(floor), false) = (ai.floor(kinds[i]), ai.land_build) {
            if let Some(q) = best_build(&g.s, a, g.bal[i], floor) {
                g.apply(&Action::Build { seat: a, square: q }, st.net_fee, e.rent_fee_pct);
                ss[i].builds += 1;
            }
        }
        let (d1, d2) = (rng.die(), rng.die());
        let turn = |buy, sell: Option<u16>| Action::Turn { seat: a, die1: d1, die2: d2, pay: false, buy, sell_square: sell };
        let mut base = sim::simulate(&g.s, 500, 500, &turn(false, None)).expect("turn");
        if let (Some(floor), true) = (ai.floor(kinds[i]), ai.land_build) {
            let to = usize::from(base.story.to);
            if base.story.moved && !base.story.sent_to_maintenance && g.s.owners[to] == a && g.s.improvements[to] < 3 {
                let cost = i128::from(rules::next_improvement_cost(g.s.improvements[to], &g.s.params()));
                if g.bal[i] - cost >= floor {
                    g.apply(&Action::Build { seat: a, square: base.story.to }, st.net_fee, e.rent_fee_pct);
                    ss[i].builds += 1;
                    base = sim::simulate(&g.s, 500, 500, &turn(false, None)).expect("turn");
                }
            }
        }
        let mut action = turn(false, None);
        let mut skip = false;
        if (base.seat_withdraw as i128) > g.bal[i] {
            let mut owned: Vec<u16> = (0..40u16).filter(|q| g.s.owners[usize::from(*q)] == a).collect();
            owned.sort_by_key(|q| rules::buyback_value(*q, g.s.improvements[usize::from(*q)], &g.s.params()));
            let inside = owned.iter().copied().find(|q| {
                sim::simulate(&g.s, 500, 500, &turn(false, Some(*q))).map(|o| (o.seat_withdraw as i128) <= g.bal[i]).unwrap_or(false)
            });
            if let Some(q) = inside {
                action = turn(false, Some(q));
                g.f.forced_sales += 1;
            } else {
                let mut funded = false;
                for q in owned {
                    g.apply(&Action::Sell { seat: a, square: q }, st.net_fee, e.rent_fee_pct);
                    g.f.forced_sales += 1;
                    let o = sim::simulate(&g.s, 500, 500, &turn(false, None)).expect("turn");
                    if (o.seat_withdraw as i128) <= g.bal[i] { funded = true; break; }
                }
                if !funded {
                    if ai.regen { regen_seat(&mut g, a, st.seat0); ss[i].regens += 1; }
                    else { remove_seat(&mut g.s, a); g.f.eliminated += 1; }
                    skip = true;
                }
            }
        } else if base.story.buy_option.is_some() {
            let b = sim::simulate(&g.s, 500, 500, &turn(true, None)).expect("buy");
            let keep = if kinds[i] == Kind::A { ai.reserve } else { 0 };
            if (b.seat_withdraw as i128) + keep <= g.bal[i] { action = turn(true, None); }
        }
        if !skip {
            let o = sim::simulate(&g.s, 500, 500, &action).expect("turn");
            if o.story.rent > 0 {
                ss[i].rent_paid += i128::from(o.story.rent);
                ss[usize::from(o.story.rent_owner - 1)].rent_received += i128::from(o.story.rent);
            }
            if o.story.purchase > 0 { ss[i].purchases += 1; }
            g.apply(&action, st.net_fee, e.rent_fee_pct);
        }
    }
    (g, ss)
}

/// Runs `games` games of one table mix. Seat order rotates each game so no type always moves first.
fn run_mixed(st: &Setup, e: &Econ, mix: &str, ai: &AiCfg, games: usize, seed: u64) -> Value {
    let base: Vec<Kind> = mix.chars().map(|c| match c { 'A' => Kind::A, 'B' => Kind::B, _ => Kind::H }).collect();
    assert_eq!(base.len(), st.seats, "mix length must equal seats");
    let mut rng = Rng(seed | 1);
    let types = [(Kind::H, "human"), (Kind::B, "human_builder"), (Kind::A, "ai")];
    let mut wins = [0f64; 3];
    let mut exits: [Vec<i128>; 3] = [vec![], vec![], vec![]];
    let mut out = [0i128; 3];
    let mut stats = [SeatStats::default(), SeatStats::default(), SeatStats::default()];
    let mut house_end = vec![];
    let mut house_low = vec![];
    let (mut regens, mut regen_cost, mut net_fees, mut rent_fees) = (0i128, 0i128, 0i128, 0i128);
    let mut type_seats = [0i128; 3];
    for gi in 0..games {
        let r = gi % st.seats;
        let kinds: Vec<Kind> = (0..st.seats).map(|k| base[(k + r) % st.seats]).collect();
        let (g, ss) = play_mixed(st, e, &kinds, ai, &mut rng);
        house_end.push(g.house);
        house_low.push(g.house_min);
        regens += g.f.regens; regen_cost += g.f.regen_cost; net_fees += g.f.tx_fees; rent_fees += g.f.rent_fees;
        let ev: Vec<i128> = (0..st.seats).map(|k| exit_value(&g, k)).collect();
        let top = *ev.iter().max().unwrap();
        let winners: Vec<usize> = (0..st.seats).filter(|k| ev[*k] == top).collect();
        for k in 0..st.seats {
            let t = types.iter().position(|(kk, _)| *kk == kinds[k]).unwrap();
            type_seats[t] += 1;
            if winners.contains(&k) { wins[t] += 1.0 / winners.len() as f64; }
            exits[t].push(ev[k]);
            if g.s.out[k] { out[t] += 1; }
            let s = &ss[k];
            stats[t].rent_paid += s.rent_paid; stats[t].rent_received += s.rent_received;
            stats[t].builds += s.builds; stats[t].purchases += s.purchases; stats[t].regens += s.regens;
        }
    }
    let mut per_type = serde_json::Map::new();
    for (t, (_, name)) in types.iter().enumerate() {
        let n = type_seats[t];
        if n == 0 { continue; }
        let ex = &mut exits[t];
        let seats_per_game = n as f64 / games as f64;
        per_type.insert((*name).into(), json!({
            "seats_per_game": seats_per_game,
            "win_rate_per_seat": format!("{:.4}", wins[t] / n as f64),
            "win_rate_any_seat_of_type": format!("{:.4}", wins[t] / games as f64),
            "exit_mean": t_(ex.iter().sum::<i128>() / n), "exit_p5": t_(pct(ex, 0.05)), "exit_p50": t_(pct(ex, 0.5)), "exit_p95": t_(pct(ex, 0.95)),
            "above_stake_share": format!("{:.4}", ex.iter().filter(|v| **v > st.seat0).count() as f64 / n as f64),
            "out_share": format!("{:.4}", out[t] as f64 / n as f64),
            "rent_paid_mean": t_(stats[t].rent_paid / n), "rent_received_mean": t_(stats[t].rent_received / n),
            "builds_mean": format!("{:.4}", stats[t].builds as f64 / n as f64),
            "purchases_mean": format!("{:.4}", stats[t].purchases as f64 / n as f64),
            "regens_mean": format!("{:.4}", stats[t].regens as f64 / n as f64),
        }));
    }
    let gm = games as i128;
    let below = house_low.iter().filter(|x| **x < st.house0).count();
    json!({"mix": mix, "games": games, "per_type": per_type,
           "house_end_mean": t_(house_end.iter().sum::<i128>() / gm),
           "house_change_mean": t_(house_end.iter().sum::<i128>() / gm - st.house0),
           "house_end_p5": t_(pct(&mut house_end.clone(), 0.05)), "house_end_p50": t_(pct(&mut house_end.clone(), 0.5)),
           "house_end_p95": t_(pct(&mut house_end.clone(), 0.95)),
           "house_lowest_p5": t_(pct(&mut house_low.clone(), 0.05)), "house_lowest_min": t_(*house_low.iter().min().unwrap()),
           "house_ever_below_start_share": format!("{:.4}", below as f64 / games as f64),
           "regens_per_game": format!("{:.4}", regens as f64 / games as f64), "regen_cost_mean": t_(regen_cost / gm),
           "network_fees_mean": t_(net_fees / gm), "rent_fees_mean": t_(rent_fees / gm),
           "chance_win_rate_per_seat": format!("{:.4}", 1.0 / st.seats as f64)})
}

fn t_(u: i128) -> String { t(u) }

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |k: &str, d: &str| args.iter().position(|a| a == k).map(|i| args[i + 1].clone()).unwrap_or(d.to_owned());
    if args.iter().any(|a| a == "--mixes") {
        let games: usize = get("--games", "20000").parse().unwrap();
        let seed: u64 = get("--seed", "7").parse().unwrap();
        let st = Setup { seats: 4, rounds: get("--rounds", "100").parse().unwrap(), house0: units(&get("--house", "1000")),
            seat0: units(&get("--seat-start", "20")), net_fee: units(&get("--fee", "0.009085")) };
        let e = Econ { go_base: units(&get("--go-base", "20")), go_floor: units(&get("--go-floor", "20")),
            rent_fee_pct: get("--rent-fee", "0").parse().unwrap(), fee_mult: get("--fee-mult", "1").parse().unwrap() };
        let ai = AiCfg { reserve: units(&get("--ai-reserve", "5")), build_floor: units(&get("--ai-build-floor", "30")),
            human_build_floor: units(&get("--human-build-floor", "10")), land_build: args.iter().any(|a| a == "--land-build"),
            regen: args.iter().any(|a| a == "--regen") };
        let mixes = get("--mixes", "HHHH,HAAA,HHAA,HHHA,AAAA");
        let rows: Vec<Value> = mixes.split(',').map(|m| {
            let v = run_mixed(&st, &e, m, &ai, games, seed);
            eprintln!("{m} done");
            v
        }).collect();
        println!("{}", serde_json::to_string_pretty(&json!({"inputs": {"seats": 4, "rounds": st.rounds, "seat_start": t(st.seat0),
            "house_start": t(st.house0), "network_fee_per_tx": t(st.net_fee),
            "go_base": t(e.go_base), "go_floor": t(e.go_floor), "rent_fee_pct": e.rent_fee_pct, "fee_square_multiplier": e.fee_mult, "ai_reserve": t(ai.reserve), "ai_build_floor": t(ai.build_floor),
            "human_build_floor": t(ai.human_build_floor), "build_only_on_landing": ai.land_build, "regeneration": ai.regen,
            "games_per_mix": games, "seed": seed}, "mixes": rows})).unwrap());
        return;
    }
    let games: usize = get("--games", "20000").parse().unwrap();
    let seed: u64 = get("--seed", "7").parse().unwrap();
    let st = Setup { seats: get("--seats", "4").parse().unwrap(), rounds: get("--rounds", "100").parse().unwrap(),
        house0: units(&get("--house", "1000")), seat0: units(&get("--seat-start", "20")), net_fee: units(&get("--fee", "0.009085")) };
    assert!((2..=20).contains(&st.seats));
    let inputs = json!({"seats": st.seats, "rounds": st.rounds, "house_start": t(st.house0), "seat_start": t(st.seat0),
        "network_fee_per_tx": t(st.net_fee), "seed": seed});
    if args.iter().any(|a| a == "--search") {
        let mut rows = vec![];
        for go_base in [20, 15, 10] {
            for go_floor in [0, 2, 5, go_base] {
                for rent_fee_pct in [0, 5, 10, 15] {
                    for fee_mult in [1u64, 2] {
                        let e = Econ { go_base: go_base * UNIT, go_floor: go_floor * UNIT, rent_fee_pct, fee_mult };
                        let s = run(&st, &e, games, seed, false);
                        eprintln!("GO {go_base}/{go_floor} rent {rent_fee_pct}% fees x{fee_mult}: never-below {:.4} players-above {:.4} margin {}",
                            s.never_below, s.players_above, t(s.margin_mean));
                        rows.push(s.v);
                    }
                }
            }
        }
        println!("{}", serde_json::to_string_pretty(&json!({"inputs": inputs, "games_per_set": games, "sets": rows})).unwrap());
    } else {
        let e = Econ { go_base: units(&get("--go-base", "20")), go_floor: units(&get("--go-floor", "20")),
            rent_fee_pct: get("--rent-fee", "0").parse().unwrap(), fee_mult: get("--fee-mult", "1").parse().unwrap() };
        let mut s = run(&st, &e, games, seed, true);
        s.v["inputs"] = inputs;
        println!("{}", serde_json::to_string_pretty(&s.v).unwrap());
    }
}

// End of tools/econ-sim/src/main.rs
// Location: tools/econ-sim/src/main.rs · Created: 2026-11-16 04:06:01 UTC
