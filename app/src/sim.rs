//! Ootle Game Engine | app/src/sim.rs
//! Version: v0.3.0
//! Off-chain simulation of every Town Square method, statement for statement against
//! contracts/town_square/src/lib.rs, using the shared rules module. For one action it yields:
//!   - the exact bucket amounts the manifest must withdraw (House and signing seat);
//!   - the expected Session state after the transaction (compared field by field after commit);
//!   - the expected balance change of every account, excluding the network fee (reconciliation);
//!   - a Story describing the outcome for the Town Square.
//! Error texts match the contract's assert messages so api_error classifies both the same way.
//! engine-tests/tests/backend_plans.rs runs these plans in the Tari engine and checks all three.
//! v0.3.0: scaled GO, rent fee, build on landing, `reset_seat` with its cash settlement, and plans of
//! several actions in one transaction (a turn followed by building on the asset landed on).

use crate::{board, decode::Session};
use ootle_town_square::rules::{self, NO_SQUARE};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Normal or Maintenance turn (chosen from the seat's held flag). `pay` applies only when held.
    Turn { seat: u8, die1: u8, die2: u8, pay: bool, buy: bool, sell_square: Option<u16> },
    Sell { seat: u8, square: u16 },
    PayShopFee { payer: u8, held_seat: u8 },
    Propose { seller: u8, buyer: u8, square: u16, price: u64 },
    Accept { buyer: u8 },
    Decline { buyer: u8 },
    Build { seat: u8, square: u16 },
    /// `reset_seat`. `settle` = seat start − the seat's cash: the House pays it when positive, the seat
    /// returns the excess when negative. Signed by the operator (owner method) with the seat and House keys.
    Reset { seat: u8, pass_turn: bool, settle: i128 },
}

/// Balance change per account (base units), excluding the network fee paid by the House.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Ledger { pub seats: Vec<i128>, pub house: i128, pub community: i128, pub chance: i128 }

#[derive(Clone, Debug, Default, Serialize)]
pub struct Story {
    pub action: &'static str,
    pub seat: u8,
    pub die1: u8,
    pub die2: u8,
    pub doubles: bool,
    pub from: u16,
    pub to: u16,
    pub moved: bool,
    pub passed_go: bool,
    pub go_reward: u64,
    pub square_reward: u64,
    pub card_kind: u8,
    pub card_id: u8,
    pub card_amount: u64,
    pub card_positive: bool,
    pub penalty: u64,
    pub maintenance_fee: u64,
    pub rent: u64,
    pub rent_owner: u8,
    /// Part of the rent routed as a fee (owner receives rent − rent_fee).
    pub rent_fee: u64,
    pub purchase: u64,
    pub sale: u64,
    pub sold_square: Option<u16>,
    pub sent_to_maintenance: bool,
    pub third_double: bool,
    pub released: bool,
    pub held_attempt: u8,
    pub held_owed: u64,
    pub again: bool,
    /// Unowned asset at the destination that the seat may buy this turn (when it did not).
    pub buy_option: Option<u16>,
    pub buy_price: u64,
    pub other_seat: u8,
    pub square: u16,
    pub amount: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub after: Session,
    /// Seat whose key must sign (identity badge in scope).
    pub signer: u8,
    pub house_withdraw: u64,
    pub seat_withdraw: u64,
    pub ledger: Ledger,
    pub story: Story,
    pub method: &'static str,
}

const S: fn(&str) -> String = str::to_owned;

struct Sim { s: Session, l: Ledger, cbps: u16, hbps: u16, story: Story }

impl Sim {
    fn i(seat: u8) -> usize { usize::from(seat - 1) }

    fn assert_seat(&self, seat: u8) -> Result<(), String> {
        if seat >= 1 && usize::from(seat) <= self.s.seats.len() { Ok(()) } else { Err(S("invalid seat")) }
    }
    fn assert_turn(&self, seat: u8) -> Result<(), String> {
        if !self.s.started { return Err(S("game not started")); }
        self.assert_seat(seat)?;
        if self.s.out[Self::i(seat)] { return Err(S("seat is out of the game")); }
        if self.s.active != seat { return Err(S("not this seat's turn")); }
        Ok(())
    }
    fn assert_offer(&self, buyer: u8) -> Result<(), String> {
        if !self.s.started { return Err(S("game not started")); }
        if !self.s.has_offer() { return Err(S("no pending offer")); }
        if self.s.offer_buyer != buyer { return Err(S("wrong recipient")); }
        if self.s.owners[usize::from(self.s.offer_square)] != self.s.offer_seller { return Err(S("ownership changed")); }
        Ok(())
    }
    fn distribute(&mut self, units: u64) {
        let c = (u128::from(units) * u128::from(self.cbps) / 10_000) as u64;
        let h = (u128::from(units) * u128::from(self.hbps) / 10_000) as u64;
        self.l.community += i128::from(c);
        self.l.chance += i128::from(h);
        self.l.house += i128::from(units - c - h);
    }
    fn pay(&mut self, seat: u8, amount: u64) { self.l.seats[Self::i(seat)] -= i128::from(amount); }
    fn credit(&mut self, seat: u8, amount: u64) { self.l.seats[Self::i(seat)] += i128::from(amount); }
    fn clear_offer(&mut self) {
        self.s.offer_square = NO_SQUARE; self.s.offer_price = 0; self.s.offer_seller = 0; self.s.offer_buyer = 0;
    }
    fn send_to_maintenance(&mut self, seat: u8) {
        let i = Self::i(seat);
        self.s.positions[i] = rules::MAINTENANCE_SQUARE;
        self.s.held[i] = true;
        self.s.held_attempts[i] = 0;
        self.s.held_owed[i] = self.s.shop_fee;
    }
    fn release(&mut self, seat: u8) {
        let i = Self::i(seat);
        self.s.held[i] = false; self.s.held_attempts[i] = 0; self.s.held_owed[i] = 0;
        self.story.released = true;
    }
    #[allow(clippy::too_many_arguments)]
    fn record_event(&mut self, kind: u8, card: u8, amount: u64, positive: bool, seat: u8, square: u16, recipient: u8) {
        let s = &mut self.s;
        s.event_kind = kind; s.event_card = card; s.event_amount = amount; s.event_positive = positive;
        s.event_seat = seat; s.event_square = square; s.event_recipient = recipient;
    }
    fn finish_turn(&mut self, again: bool) {
        self.s.turn_count += 1;
        self.story.again = again;
        if again { self.s.doubles_streak += 1; } else { self.end_turn(); }
    }
    fn end_turn(&mut self) {
        self.s.doubles_streak = 0;
        let (next, wrapped) = rules::next_seat(self.s.active, &self.s.out);
        if wrapped { self.s.round += 1; }
        self.s.active = next;
    }

    /// Mirrors `move_and_settle`. Returns (sent_to_maintenance, house_withdraw, seat_withdraw).
    fn clear_landed(&mut self) { self.s.landed_seat = 0; self.s.landed_square = NO_SQUARE; }

    fn move_and_settle(&mut self, seat: u8, d1: u8, d2: u8, buy: bool, sell: u16, extra_fee: u64)
        -> Result<(bool, u64, u64), String> {
        if self.s.has_offer() { return Err(S("resolve the pending offer first")); }
        let i = Self::i(seat);
        let p = self.s.params();
        let lp = self.s.landing_params();
        let from = self.s.positions[i];
        let to = rules::destination(from, d1, d2);
        let sale = if sell != NO_SQUARE {
            if !(rules::is_asset(sell) && self.s.owners[usize::from(sell)] == seat) {
                return Err(S("seat does not own the sale asset"));
            }
            rules::buyback_value(sell, self.s.improvements[usize::from(sell)], &p)
        } else { 0 };
        if sell != NO_SQUARE { self.s.owners[usize::from(sell)] = 0; self.s.improvements[usize::from(sell)] = 0; }
        let landing = rules::land(from, to, seat, &self.s.owners, &self.s.improvements, self.s.turn_count, &lp);
        let purchase = if buy {
            if landing.sent_to_maintenance || !rules::is_asset(to) { return Err(S("destination is not an asset")); }
            if self.s.owners[usize::from(to)] != 0 { return Err(S("asset already owned")); }
            rules::base_price(to)
        } else { 0 };
        let fees = landing.penalty + extra_fee;
        let f = rules::funding(sale, landing.rewards(), purchase, fees, landing.rent);

        // Money: House supplies `incoming`, the seat supplies `seat_funding`.
        self.l.house -= i128::from(f.incoming);
        self.pay(seat, f.seat_funding);
        let mut pot = f.incoming + f.seat_funding;
        if purchase > 0 { self.distribute(purchase); pot -= purchase; }
        if fees > 0 { self.distribute(fees); pot -= fees; }
        let rent_fee = rules::rent_fee(landing.rent, self.s.rent_fee_bps);
        if landing.rent > 0 {
            if rent_fee > 0 { self.distribute(rent_fee); }
            self.credit(landing.rent_owner, landing.rent - rent_fee);
            pot -= landing.rent;
        }
        self.credit(seat, pot);

        if buy { self.s.owners[usize::from(to)] = seat; }
        if landing.sent_to_maintenance {
            self.send_to_maintenance(seat);
            self.record_event(4, 255, 0, false, seat, rules::BACK_TO_MAINTENANCE, 0);
        } else {
            self.s.positions[i] = to;
            self.s.landed_seat = seat;
            self.s.landed_square = to;
            if landing.rent > 0 {
                self.record_event(3, 255, landing.rent, false, seat, to, landing.rent_owner);
            } else {
                self.record_event(landing.card_kind, landing.card_id, landing.card_amount,
                    landing.card_kind != 0 && landing.card_positive, seat, to, 0);
            }
        }
        let st = &mut self.story;
        st.from = from; st.to = to; st.moved = true; st.passed_go = rules::passed_go(from, to);
        st.go_reward = landing.go_reward; st.square_reward = landing.square_reward;
        st.card_kind = landing.card_kind; st.card_id = landing.card_id; st.card_amount = landing.card_amount;
        st.card_positive = landing.card_positive; st.penalty = landing.penalty; st.maintenance_fee = extra_fee;
        st.rent = landing.rent; st.rent_owner = landing.rent_owner; st.rent_fee = rent_fee; st.purchase = purchase; st.sale = sale;
        st.sold_square = (sell != NO_SQUARE).then_some(sell);
        st.sent_to_maintenance = landing.sent_to_maintenance;
        if !buy && !landing.sent_to_maintenance && rules::is_asset(to) && self.s.owners[usize::from(to)] == 0 {
            st.buy_option = Some(to);
            st.buy_price = rules::base_price(to);
        }
        Ok((landing.sent_to_maintenance, f.incoming, f.seat_funding))
    }
}

/// Simulates `action` against `s`. `community_bps` / `chance_bps` are the Treasury rates.
pub fn simulate(s: &Session, community_bps: u16, chance_bps: u16, action: &Action) -> Result<Outcome, String> {
    let n = s.seats.len();
    let mut x = Sim { s: s.clone(), l: Ledger { seats: vec![0; n], house: 0, community: 0, chance: 0 },
        cbps: community_bps, hbps: chance_bps, story: Story::default() };
    let (signer, method, hw, sw) = match *action {
        Action::Turn { seat, die1, die2, pay, buy, sell_square } => {
            x.assert_turn(seat)?;
            let i = Sim::i(seat);
            if !(rules::valid_die(die1) && rules::valid_die(die2)) { return Err(S("invalid dice")); }
            let sell = sell_square.unwrap_or(NO_SQUARE);
            let doubles = die1 == die2;
            x.s.last_die1 = die1; x.s.last_die2 = die2;
            x.story.seat = seat; x.story.die1 = die1; x.story.die2 = die2; x.story.doubles = doubles;
            x.story.from = x.s.positions[i];
            if !x.s.held[i] {
                x.clear_landed();
                x.story.action = "play_turn";
                if doubles && x.s.doubles_streak + 1 >= 3 {
                    if buy || sell != NO_SQUARE { return Err(S("no purchase or sale when sent to Maintenance")); }
                    x.send_to_maintenance(seat);
                    x.record_event(4, 255, 0, false, seat, rules::MAINTENANCE_SQUARE, 0);
                    x.story.third_double = true; x.story.sent_to_maintenance = true;
                    x.story.to = rules::MAINTENANCE_SQUARE;
                    x.finish_turn(false);
                    (seat, "play_turn", 0, 0)
                } else {
                    let (sent, hw, sw) = x.move_and_settle(seat, die1, die2, buy, sell, 0)?;
                    x.finish_turn(doubles && !sent);
                    (seat, "play_turn", hw, sw)
                }
            } else {
                x.clear_landed();
                x.story.action = "maintenance_turn";
                if pay {
                    let fee = x.s.held_owed[i];
                    x.release(seat);
                    let (sent, hw, sw) = x.move_and_settle(seat, die1, die2, buy, sell, fee)?;
                    x.finish_turn(doubles && !sent);
                    (seat, "maintenance_turn", hw, sw)
                } else if doubles {
                    x.release(seat);
                    let (_, hw, sw) = x.move_and_settle(seat, die1, die2, buy, sell, 0)?;
                    x.finish_turn(false);
                    (seat, "maintenance_turn", hw, sw)
                } else {
                    x.s.held_attempts[i] += 1;
                    x.s.held_owed[i] += x.s.storage_fee;
                    x.story.held_attempt = x.s.held_attempts[i];
                    if x.s.held_attempts[i] < rules::MAX_MAINTENANCE_ATTEMPTS {
                        if buy || sell != NO_SQUARE { return Err(S("no purchase or sale while still held")); }
                        x.story.held_owed = x.s.held_owed[i];
                        x.story.to = rules::MAINTENANCE_SQUARE;
                        x.record_event(5, 255, x.s.held_owed[i], false, seat, rules::MAINTENANCE_SQUARE, 0);
                        x.finish_turn(false);
                        (seat, "maintenance_turn", 0, 0)
                    } else {
                        let fee = x.s.held_owed[i];
                        x.release(seat);
                        let (_, hw, sw) = x.move_and_settle(seat, die1, die2, buy, sell, fee)?;
                        x.finish_turn(false);
                        (seat, "maintenance_turn", hw, sw)
                    }
                }
            }
        }
        Action::Sell { seat, square } => {
            x.assert_turn(seat)?;
            if x.s.has_offer() { return Err(S("resolve the pending offer first")); }
            if !rules::is_asset(square) { return Err(S("not an asset")); }
            if x.s.owners[usize::from(square)] != seat { return Err(S("seat does not own this asset")); }
            let value = rules::buyback_value(square, x.s.improvements[usize::from(square)], &x.s.params());
            x.l.house -= i128::from(value);
            x.credit(seat, value);
            x.s.owners[usize::from(square)] = 0;
            x.s.improvements[usize::from(square)] = 0;
            x.story = Story { action: "sell_to_house", seat, square, amount: value, sale: value,
                sold_square: Some(square), ..Story::default() };
            (seat, "sell_to_house", value, 0)
        }
        Action::PayShopFee { payer, held_seat } => {
            x.assert_turn(payer)?;
            x.assert_seat(held_seat)?;
            if payer == held_seat { return Err(S("use maintenance_turn for your own shop fee")); }
            let h = Sim::i(held_seat);
            if x.s.out[h] || !x.s.held[h] { return Err(S("seat is not held in Maintenance")); }
            let owed = x.s.held_owed[h];
            x.pay(payer, owed);
            x.distribute(owed);
            x.release(held_seat);
            x.story = Story { action: "pay_shop_fee", seat: payer, other_seat: held_seat, amount: owed,
                released: true, ..Story::default() };
            (payer, "pay_shop_fee", 0, owed)
        }
        Action::Propose { seller, buyer, square, price } => {
            x.assert_turn(seller)?;
            x.assert_seat(buyer)?;
            if buyer == seller || x.s.out[Sim::i(buyer)] { return Err(S("invalid buyer")); }
            if x.s.has_offer() { return Err(S("offer already pending")); }
            if !(rules::is_asset(square) && x.s.owners[usize::from(square)] == seller) {
                return Err(S("not the seller's asset"));
            }
            if price == 0 { return Err(S("price must be positive")); }
            x.s.offer_square = square; x.s.offer_price = price; x.s.offer_seller = seller; x.s.offer_buyer = buyer;
            x.s.offer_nonce += 1;
            x.story = Story { action: "propose_sale", seat: seller, other_seat: buyer, square, amount: price, ..Story::default() };
            (seller, "propose_sale", 0, 0)
        }
        Action::Accept { buyer } => {
            x.assert_offer(buyer)?;
            let (sq, price, seller) = (x.s.offer_square, x.s.offer_price, x.s.offer_seller);
            x.pay(buyer, price);
            x.credit(seller, price);
            x.s.owners[usize::from(sq)] = buyer;
            x.clear_offer();
            x.story = Story { action: "accept_sale", seat: buyer, other_seat: seller, square: sq, amount: price, ..Story::default() };
            (buyer, "accept_sale", 0, price)
        }
        Action::Decline { buyer } => {
            x.assert_offer(buyer)?;
            let (sq, price, seller) = (x.s.offer_square, x.s.offer_price, x.s.offer_seller);
            x.clear_offer();
            x.story = Story { action: "decline_sale", seat: buyer, other_seat: seller, square: sq, amount: price, ..Story::default() };
            (buyer, "decline_sale", 0, 0)
        }
        Action::Build { seat, square } => {
            if x.s.build_on_landing {
                if !x.s.started { return Err(S("game not started")); }
                x.assert_seat(seat)?;
                if x.s.out[Sim::i(seat)] { return Err(S("seat is out of the game")); }
                if !(x.s.landed_seat == seat && x.s.landed_square == square) {
                    return Err(S("build only on the asset just landed on"));
                }
            } else {
                x.assert_turn(seat)?;
            }
            if x.s.has_offer() { return Err(S("resolve the pending offer first")); }
            if !(rules::is_asset(square) && x.s.owners[usize::from(square)] == seat) {
                return Err(S("seat does not own this asset"));
            }
            let level = x.s.improvements[usize::from(square)];
            if level >= 3 { return Err(S("invalid or stale development level")); }
            let cost = rules::next_improvement_cost(level, &x.s.params());
            x.pay(seat, cost);
            x.distribute(cost);
            x.s.improvements[usize::from(square)] = level + 1;
            if x.s.build_on_landing { x.clear_landed(); }
            x.story = Story { action: "build_improvement", seat, square, amount: cost, ..Story::default() };
            (seat, "build_improvement", 0, cost)
        }
        Action::Reset { seat, pass_turn, settle } => {
            if !x.s.started { return Err(S("game not started")); }
            x.assert_seat(seat)?;
            if pass_turn && x.s.active != seat { return Err(S("pass_turn only for the active seat")); }
            let i = Sim::i(seat);
            x.s.out[i] = false; x.s.held[i] = false; x.s.held_attempts[i] = 0; x.s.held_owed[i] = 0;
            x.s.positions[i] = 0;
            for q in 0..40 { if x.s.owners[q] == seat { x.s.owners[q] = 0; x.s.improvements[q] = 0; } }
            if x.s.has_offer() && (x.s.offer_seller == seat || x.s.offer_buyer == seat) { x.clear_offer(); }
            if x.s.landed_seat == seat { x.clear_landed(); }
            x.s.seat_since[i] = x.s.round;
            x.s.resets[i] += 1;
            x.record_event(6, 255, 0, false, seat, 0, 0);
            if x.s.active == seat {
                if pass_turn { x.finish_turn(false); } else { x.s.doubles_streak = 0; }
            }
            x.l.seats[i] += settle;
            x.l.house -= settle;
            x.story = Story { action: "reset_seat", seat, amount: settle.unsigned_abs() as u64, ..Story::default() };
            let (hw, sw) = if settle > 0 { (settle as u64, 0) } else { (0, settle.unsigned_abs() as u64) };
            (seat, "reset_seat", hw, sw)
        }
    };
    x.story.text = describe(&x.story, &x.s);
    let total: i128 = x.l.seats.iter().sum::<i128>() + x.l.house + x.l.community + x.l.chance;
    debug_assert_eq!(total, 0, "ledger must conserve funds");
    Ok(Outcome { after: x.s, signer, house_withdraw: hw, seat_withdraw: sw, ledger: x.l, story: x.story, method })
}

pub fn card_label(kind: u8, id: u8) -> &'static str {
    const CH: [&str; 4] = ["Chance: Lucky find", "Chance: Repair bill", "Chance: Bonus payout", "Chance: Inspection fee"];
    const CO: [&str; 4] = ["Community: Shared reward", "Community: Contribution", "Community: Community grant", "Community: Community levy"];
    match kind { 1 => CH.get(usize::from(id)).copied().unwrap_or("Chance"), 2 => CO.get(usize::from(id)).copied().unwrap_or("Community"), _ => "" }
}

/// One-line plain description of the outcome (Town Square message and history).
pub fn describe(st: &Story, after: &Session) -> String {
    use board::{square_name, ttari};
    let seat = format!("Seat {}", st.seat);
    match st.action {
        "sell_to_house" => format!("{seat} sold {} to the House for {} tTARI", square_name(st.square), ttari(st.amount)),
        "pay_shop_fee" => format!("{seat} paid the shop fee of {} tTARI for Seat {} · Seat {} released from Maintenance",
            ttari(st.amount), st.other_seat, st.other_seat),
        "propose_sale" => format!("{seat} offered {} to Seat {} for {} tTARI", square_name(st.square), st.other_seat, ttari(st.amount)),
        "accept_sale" => format!("{seat} bought {} from Seat {} for {} tTARI", square_name(st.square), st.other_seat, ttari(st.amount)),
        "decline_sale" => format!("{seat} declined the offer of {} from Seat {}", square_name(st.square), st.other_seat),
        "reset_seat" => format!("{seat} reset · assets back to the House · back on GO · cash settled {} tTARI",
            ttari(st.amount)),
        "build_improvement" => {
            let lvl = after.improvements[usize::from(st.square)];
            let what = ["", "a house", "a second house", "a hotel"][usize::from(lvl.min(3))];
            format!("{seat} built {what} on {} for {} tTARI", square_name(st.square), ttari(st.amount))
        }
        _ => {
            let mut parts = vec![format!("{seat} rolled {} + {}{}", st.die1, st.die2, if st.doubles { " · doubles" } else { "" })];
            if st.third_double { parts.push("third double in a row · sent to Maintenance".into()); return parts.join(" · "); }
            if !st.moved {
                parts.push(format!("stays in Maintenance · attempt {}/3 · owes {} tTARI", st.held_attempt, ttari(st.held_owed)));
                return parts.join(" · ");
            }
            if st.released && st.maintenance_fee > 0 { parts.push(format!("paid {} tTARI · released from Maintenance", ttari(st.maintenance_fee))); }
            else if st.released { parts.push("released from Maintenance".into()); }
            if let Some(sq) = st.sold_square { parts.push(format!("sold {} to the House for {}", square_name(sq), ttari(st.sale))); }
            if st.go_reward > 0 { parts.push(format!("passed GO +{}", ttari(st.go_reward))); }
            if st.sent_to_maintenance { parts.push("landed on Back to Maintenance · sent to Maintenance".into()); return parts.join(" · "); }
            parts.push(format!("landed on {}", square_name(st.to)));
            if st.purchase > 0 { parts.push(format!("bought it for {} tTARI", ttari(st.purchase))); }
            if st.rent > 0 && st.rent_fee > 0 {
                parts.push(format!("rent {} tTARI to Seat {} (rent fee {})", ttari(st.rent), st.rent_owner, ttari(st.rent_fee)));
            } else if st.rent > 0 { parts.push(format!("rent {} tTARI to Seat {}", ttari(st.rent), st.rent_owner)); }
            if st.card_kind != 0 {
                parts.push(format!("{} {}{}", card_label(st.card_kind, st.card_id), if st.card_positive { "+" } else { "−" }, ttari(st.card_amount)));
            }
            let sq_fee = st.penalty - if st.card_kind != 0 && !st.card_positive { st.card_amount } else { 0 };
            if sq_fee > 0 { parts.push(format!("{} {} tTARI", if st.to == 7 { "penalty" } else { "fee" }, ttari(sq_fee))); }
            if st.square_reward > 0 { parts.push(format!("reward +{}", ttari(st.square_reward))); }
            if st.again { parts.push("rolls again".into()); }
            parts.join(" · ")
        }
    }
}

/// Manifest call text for the session method (arguments after the receiver).
pub fn call_text(a: &Action, s: &Session) -> String { call_text_k(a, s, "") }

/// As `call_text`, with bucket names suffixed by `k` (several calls in one manifest).
pub fn call_text_k(a: &Action, s: &Session, k: &str) -> String {
    let (r, t) = (s.round, s.turn_count);
    let (seat_funds, house_funds) = (format!("seat_funds{k}"), format!("house_funds{k}"));
    let text = match *a {
        Action::Turn { seat, die1, die2, pay, buy, sell_square } => {
            let sell = sell_square.unwrap_or(NO_SQUARE);
            if s.held[usize::from(seat - 1)] {
                format!("game.maintenance_turn({seat}u8, {pay}, {die1}u8, {die2}u8, {r}u64, {t}u64, {buy}, {sell}u16, seat_funds, house_funds);")
            } else {
                format!("game.play_turn({seat}u8, {die1}u8, {die2}u8, {r}u64, {t}u64, {buy}, {sell}u16, seat_funds, house_funds);")
            }
        }
        Action::Sell { seat, square } => format!("game.sell_to_house({seat}u8, {square}u16, {r}u64, {t}u64, house_funds);"),
        Action::PayShopFee { payer, held_seat } => format!("game.pay_shop_fee({payer}u8, {held_seat}u8, {r}u64, {t}u64, seat_funds);"),
        Action::Propose { seller, buyer, square, price } => format!("game.propose_sale({seller}u8, {buyer}u8, {square}u16, {price}u64, {r}u64, {t}u64);"),
        Action::Accept { buyer } => format!("game.accept_sale({buyer}u8, {}u64, {r}u64, {t}u64, seat_funds);", s.offer_nonce),
        Action::Decline { buyer } => format!("game.decline_sale({buyer}u8, {}u64, {r}u64, {t}u64);", s.offer_nonce),
        Action::Build { seat, square } => format!("game.build_improvement({seat}u8, {square}u16, {r}u64, {t}u64, {}u8, seat_funds);",
            s.improvements[usize::from(square)]),
        Action::Reset { seat, pass_turn, .. } => format!("game.reset_seat({seat}u8, {pass_turn}, {r}u64, {t}u64);"),
    };
    text.replace("seat_funds", &seat_funds).replace("house_funds", &house_funds)
}

/// Full transaction manifest for one action. Variables: `house` (game House account, pays the fee),
/// `seat_account` (signing seat's account) and `game` (session component).
pub fn manifest(a: &Action, s: &Session, o: &Outcome, max_fee: u64) -> String {
    let plan = Plan { before: s.clone(), steps: vec![(a.clone(), o.clone())] };
    plan_manifest(&plan, max_fee)
}

/// Several actions settled in one transaction, each against the state the previous one leaves.
#[derive(Clone, Debug)]
pub struct Plan { pub before: Session, pub steps: Vec<(Action, Outcome)> }

impl Plan {
    pub fn after(&self) -> &Session { &self.steps.last().expect("plan has steps").1.after }
    pub fn first(&self) -> &Outcome { &self.steps[0].1 }
    pub fn signer(&self) -> u8 { self.first().signer }
    pub fn house_withdraw(&self) -> u64 { self.steps.iter().map(|(_, o)| o.house_withdraw).sum() }
    pub fn method(&self) -> String { self.steps.iter().map(|(_, o)| o.method).collect::<Vec<_>>().join(" + ") }
    pub fn is_reset(&self) -> bool { matches!(self.steps[0].0, Action::Reset { .. }) }
    /// Total balance change per account, excluding the network fee.
    pub fn ledger(&self) -> Ledger {
        let mut l = self.first().ledger.clone();
        for (_, o) in &self.steps[1..] {
            for (a, b) in l.seats.iter_mut().zip(&o.ledger.seats) { *a += b; }
            l.house += o.ledger.house; l.community += o.ledger.community; l.chance += o.ledger.chance;
        }
        l
    }
    /// The story of the first step; later steps are appended to its text.
    pub fn story(&self) -> Story {
        let mut st = self.first().story.clone();
        for (_, o) in &self.steps[1..] { st.text = format!("{} · then {}", st.text, o.story.text); }
        st
    }
    /// Smallest seat balance at which every step's seat withdrawal is covered (signer's account).
    pub fn seat_needed(&self) -> u64 {
        let (mut need, mut net) = (0i128, 0i128);
        let i = usize::from(self.signer() - 1);
        for (_, o) in &self.steps {
            need = need.max(i128::from(o.seat_withdraw) - net);
            net += o.ledger.seats[i];
        }
        need.max(0) as u64
    }
}

/// Simulates `actions` in order; each must be valid against the state the previous one leaves.
pub fn plan(s: &Session, community_bps: u16, chance_bps: u16, actions: &[Action]) -> Result<Plan, String> {
    let mut cur = s.clone();
    let mut steps = vec![];
    for a in actions {
        let o = simulate(&cur, community_bps, chance_bps, a)?;
        cur = o.after.clone();
        steps.push((a.clone(), o));
    }
    if steps.is_empty() { return Err(S("empty plan")); }
    Ok(Plan { before: s.clone(), steps })
}

/// Transaction manifest for a plan. Buckets of step k (k ≥ 2) are named `seat_funds_k` / `house_funds_k`.
pub fn plan_manifest(p: &Plan, max_fee: u64) -> String {
    let res = &p.before.resource;
    let mut body = String::from("    let game = var![\"game\"];\n");
    let mut bound_house = false;
    let mut bound_seat = false;
    let mut state = p.before.clone();
    for (k, (a, o)) in p.steps.iter().enumerate() {
        let sfx = if k == 0 { String::new() } else { format!("_{}", k + 1) };
        let uses_house = matches!(a, Action::Turn { .. } | Action::Sell { .. } | Action::Reset { .. });
        let uses_seat = matches!(a, Action::Turn { .. } | Action::PayShopFee { .. } | Action::Accept { .. }
            | Action::Build { .. } | Action::Reset { .. });
        if uses_house && !bound_house { body += "    let house = var![\"house\"];\n"; bound_house = true; }
        if uses_seat && !bound_seat { body += "    let seat_account = var![\"seat_account\"];\n"; bound_seat = true; }
        if let Action::Reset { settle, .. } = *a {
            if settle > 0 {
                body += &format!("    let top_up{sfx} = house.withdraw(address!(\"{res}\"), {settle}u64);\n");
                body += &format!("    seat_account.deposit(top_up{sfx});\n");
            } else if settle < 0 {
                body += &format!("    let excess{sfx} = seat_account.withdraw(address!(\"{res}\"), {}u64);\n", settle.unsigned_abs());
                body += &format!("    house.deposit(excess{sfx});\n");
            }
        } else {
            if uses_house {
                body += &format!("    let house_funds{sfx} = house.withdraw(address!(\"{res}\"), {}u64);\n", o.house_withdraw);
            }
            if uses_seat {
                body += &format!("    let seat_funds{sfx} = seat_account.withdraw(address!(\"{res}\"), {}u64);\n", o.seat_withdraw);
            }
        }
        body += &format!("    {}\n", call_text_k(a, &state, &sfx));
        state = o.after.clone();
    }
    format!("fn fee_main() {{\n    let house = var![\"house\"];\n    house.pay_fee({max_fee}u64);\n}}\nfn main() {{\n{body}}}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{component, Session};

    fn fixture() -> Session {
        let text = std::fs::read_to_string("tests/fixtures/session.json").unwrap();
        Session::decode(&component(&serde_json::from_str(&text).unwrap()).unwrap().2).unwrap()
    }

    #[test]
    fn pending_offer_blocks_the_turn_until_resolved() {
        let s = fixture();
        let t = Action::Turn { seat: 1, die1: 1, die2: 2, pay: false, buy: false, sell_square: None };
        assert_eq!(simulate(&s, 500, 500, &t).unwrap_err(), "resolve the pending offer first");
        let o = simulate(&s, 500, 500, &Action::Accept { buyer: 2 }).unwrap();
        assert_eq!(o.after.owners[3], 2);
        assert_eq!(o.seat_withdraw, 25_000_000);
        assert_eq!(o.ledger.seats, vec![25_000_000, -25_000_000, 0, 0]);
        assert!(!o.after.has_offer());
        assert_eq!(o.after.offer_nonce, 1);
        assert!(o.after.improvements[3] == 1, "development stays with the asset");
    }

    #[test]
    fn turn_after_decline_settles_like_the_contract() {
        let s = fixture();
        let s2 = simulate(&s, 500, 500, &Action::Decline { buyer: 2 }).unwrap().after;
        // Seat 1 on 3 rolls 2 + 2 → 7 (Penalty 2.000000), doubles → rolls again.
        let o = simulate(&s2, 500, 500, &Action::Turn { seat: 1, die1: 2, die2: 2, pay: false, buy: false, sell_square: None }).unwrap();
        assert_eq!(o.after.positions[0], 7);
        assert_eq!((o.house_withdraw, o.seat_withdraw), (0, 2_000_000));
        assert_eq!(o.ledger.seats[0], -2_000_000);
        assert_eq!((o.ledger.house, o.ledger.community, o.ledger.chance), (1_800_000, 100_000, 100_000));
        assert_eq!((o.after.active, o.after.doubles_streak, o.after.turn_count), (1, 1, 7));
        assert!(o.story.again);
        assert!(manifest(&Action::Turn { seat: 1, die1: 2, die2: 2, pay: false, buy: false, sell_square: None }, &s2, &o, 1_000_000)
            .contains("game.play_turn(1u8, 2u8, 2u8, 2u64, 6u64, false, 65535u16, seat_funds, house_funds);"));
    }

    #[test]
    fn held_seat_paths() {
        let mut s = simulate(&fixture(), 500, 500, &Action::Decline { buyer: 2 }).unwrap().after;
        s.active = 3; // seat 3 is held, owes 5.000000
        let fail = simulate(&s, 500, 500, &Action::Turn { seat: 3, die1: 1, die2: 2, pay: false, buy: false, sell_square: None }).unwrap();
        assert_eq!((fail.after.held[2], fail.after.held_attempts[2], fail.after.held_owed[2]), (true, 1, 7_000_000));
        assert_eq!((fail.house_withdraw, fail.seat_withdraw), (0, 0));
        assert_eq!(fail.after.event_kind, 5);
        let paid = simulate(&s, 500, 500, &Action::Turn { seat: 3, die1: 1, die2: 2, pay: true, buy: false, sell_square: None }).unwrap();
        assert!(!paid.after.held[2]);
        assert_eq!(paid.after.positions[2], 13);
        assert_eq!(paid.ledger.seats[2], -5_000_000);
        let gift = simulate(&{ let mut g = s.clone(); g.active = 4; g }, 500, 500, &Action::PayShopFee { payer: 4, held_seat: 3 }).unwrap();
        assert_eq!(gift.ledger.seats[3], -5_000_000);
        assert!(!gift.after.held[2]);
        assert!(manifest(&Action::PayShopFee { payer: 4, held_seat: 3 }, &s, &gift, 1).contains("pay_shop_fee(4u8, 3u8"));
    }
}

// End of app/src/sim.rs
// Location: app/src/sim.rs · Created: 2026-11-16 04:06:01 UTC
