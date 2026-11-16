//! Ootle Game Engine | contracts/town_square/src/lib.rs
//! Version: v0.3.0
//! Game type "Town Square Classic": one component per game, 2..20 seats.
//! Rules are v0.6.18 generalised to N seats plus the agreed Maintenance rule:
//!   - three doubles in a row, or landing on square 30, holds the seat on square 10;
//!   - a held seat pays the shop fee plus storage and then moves, or rolls for doubles;
//!   - a double releases and moves (no extra roll); a failed roll adds storage;
//!   - the third failed roll forces payment, then the seat moves by that roll;
//!   - any seat may pay a held seat's shop fee during its own turn.
//! Seat identity: each seat's claim lives in the Lobby; every seat action asserts that the
//! claimed identity badge is in scope (the seat key signed the transaction).
//! Dice are supplied by the caller (the backend under House control) and range-checked here.
//! State field order is fixed and documented in docs/STATE_LAYOUT.md; the backend decodes by position.
//! v0.3.0 (fields 41..48 appended; 0..40 unchanged):
//!   - economics, owner-set with `set_economics` and fixed defaults that keep v0.1.0 behaviour
//!     (GO not scaled, rent fee 0, build anywhere on the active turn);
//!   - scaled GO: when on, GO = max(go_floor, go_reward × unowned assets ÷ 22), from the board as the turn starts;
//!   - rent fee: basis points of each rent, taken from the owner's receipt and routed as a fee (category 1);
//!   - build on landing: when on, a seat builds one level only on the owned asset it has just landed on,
//!     until the next move; it may be in the same transaction as the turn or before the next seat moves;
//!   - `reset_seat` (owner): the seat's assets return to the House, it goes back to GO, its hold clears,
//!     it is back in play, and its stint starts again. Cash is settled by the operator in the same transaction.

use tari_template_lib::prelude::*;

pub mod rules;

#[template]
mod ootle_town_square {
    use super::*;
    use crate::rules::{self, Params, NO_SQUARE};

    pub struct OotleTownSquare {
        // 0..3: identity of this game
        game_id: u64,
        lobby: ComponentAddress,
        treasury: ComponentAddress,
        resource: ResourceAddress,
        // 4..12: game-type parameters, fixed for the life of the game
        go_reward: u64,
        square_reward: u64,
        fee_amount: u64,
        penalty_amount: u64,
        shop_fee: u64,
        storage_fee: u64,
        house_cost: u64,
        hotel_cost: u64,
        buyback_pct: u8,
        // 13..16: seats
        capacity: u8,
        seats: Vec<ComponentAddress>,
        out: Vec<bool>,
        started: bool,
        // 17..23: turn state
        round: u64,
        active: u8,
        turn_count: u64,
        doubles_streak: u8,
        positions: Vec<u16>,
        last_die1: u8,
        last_die2: u8,
        // 24..25: board
        owners: Vec<u8>,
        improvements: Vec<u8>,
        // 26..28: Maintenance, per seat
        held: Vec<bool>,
        held_attempts: Vec<u8>,
        held_owed: Vec<u64>,
        // 29..35: last event
        event_kind: u8,     // 0 none, 1 Chance, 2 Community, 3 rent, 4 sent to Maintenance, 5 held after failed roll, 6 seat reset
        event_card: u8,     // 0..3 for cards, 255 otherwise
        event_amount: u64,
        event_positive: bool,
        event_seat: u8,
        event_square: u16,
        event_recipient: u8,
        // 36..40: pending private offer (offer_square == u16::MAX means none)
        offer_square: u16,
        offer_price: u64,
        offer_seller: u8,
        offer_buyer: u8,
        offer_nonce: u64,
        // 41..44: economics (v0.3.0), owner-set
        go_floor: u64,
        go_scaled: bool,
        rent_fee_bps: u16,
        build_on_landing: bool,
        // 45..46: the asset the last mover landed on (0 / u16::MAX means none)
        landed_seat: u8,
        landed_square: u16,
        // 47..48: per seat, the round its current stint started and how many times it was reset
        seat_since: Vec<u64>,
        resets: Vec<u32>,
    }

    impl OotleTownSquare {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            game_id: u64, lobby: ComponentAddress, treasury: ComponentAddress, resource: ResourceAddress,
            capacity: u8,
            go_reward: u64, square_reward: u64, fee_amount: u64, penalty_amount: u64,
            shop_fee: u64, storage_fee: u64, house_cost: u64, hotel_cost: u64, buyback_pct: u8,
        ) -> Component<Self> {
            let owner = CallerContext::transaction_signer_public_key();
            assert!((2..=20).contains(&capacity), "capacity outside 2..20");
            let p = Params { go_reward, square_reward, fee_amount, penalty_amount, shop_fee,
                storage_fee, house_cost, hotel_cost, buyback_pct };
            if let Err(e) = p.validate() { panic!("{}", e); }
            Component::new(Self {
                game_id, lobby, treasury, resource,
                go_reward, square_reward, fee_amount, penalty_amount, shop_fee, storage_fee,
                house_cost, hotel_cost, buyback_pct,
                capacity, seats: vec![], out: vec![], started: false,
                round: 1, active: 1, turn_count: 0, doubles_streak: 0, positions: vec![],
                last_die1: 0, last_die2: 0,
                owners: vec![0; rules::BOARD_SIZE as usize],
                improvements: vec![0; rules::BOARD_SIZE as usize],
                held: vec![], held_attempts: vec![], held_owed: vec![],
                event_kind: 0, event_card: 255, event_amount: 0, event_positive: false,
                event_seat: 0, event_square: 0, event_recipient: 0,
                offer_square: NO_SQUARE, offer_price: 0, offer_seller: 0, offer_buyer: 0, offer_nonce: 0,
                go_floor: go_reward, go_scaled: false, rent_fee_bps: 0, build_on_landing: false,
                landed_seat: 0, landed_square: NO_SQUARE, seat_since: vec![], resets: vec![],
            })
            .with_access_rules(ComponentAccessRules::new()
                .method("add_seat", rule!(public_key(owner)))
                .method("start", rule!(public_key(owner)))
                .method("remove_seat", rule!(public_key(owner)))
                .method("reset_seat", rule!(public_key(owner)))
                .method("set_economics", rule!(public_key(owner)))
                .method("play_turn", rule!(allow_all))
                .method("maintenance_turn", rule!(allow_all))
                .method("sell_to_house", rule!(allow_all))
                .method("pay_shop_fee", rule!(allow_all))
                .method("propose_sale", rule!(allow_all))
                .method("accept_sale", rule!(allow_all))
                .method("decline_sale", rule!(allow_all))
                .method("build_improvement", rule!(allow_all))
                .method("get_setup", rule!(allow_all))
                .method("get_summary", rule!(allow_all))
                .default(rule!(deny_all)))
            .with_owner_rule(OwnerRule::OwnedBySigner)
            .create()
        }

        // ---------- setup (owner only) ----------

        /// Adds the next seat's account. Called once per seat, in seat order, before `start`.
        pub fn add_seat(&mut self, account: ComponentAddress) {
            assert!(!self.started, "cannot add a seat after start");
            assert!(self.seats.len() < usize::from(self.capacity), "game at capacity");
            assert!(!self.seats.contains(&account), "duplicate seat account");
            self.seats.push(account);
            self.out.push(false);
            self.positions.push(0);
            self.held.push(false);
            self.held_attempts.push(0);
            self.held_owed.push(0);
            self.seat_since.push(1);
            self.resets.push(0);
        }

        /// Starts the game once every seat has an account and a claimed identity in the Lobby.
        pub fn start(&mut self) {
            assert!(!self.started, "already started");
            assert!(self.seats.len() == usize::from(self.capacity), "not all seats added");
            for seat in 1..=self.capacity {
                assert!(self.seat_identity(seat).is_some(), "unclaimed seat");
            }
            self.started = true;
        }

        /// Removes a seat from play (quit or elimination). Its assets return to the House unowned.
        /// Funds are not moved here; the operator settles them separately before removal.
        pub fn remove_seat(&mut self, seat: u8) {
            self.assert_seat(seat);
            let i = usize::from(seat - 1);
            assert!(!self.out[i], "seat already removed");
            self.out[i] = true;
            self.held[i] = false;
            self.held_attempts[i] = 0;
            self.held_owed[i] = 0;
            for sq in 0..usize::from(rules::BOARD_SIZE) {
                if self.owners[sq] == seat { self.owners[sq] = 0; self.improvements[sq] = 0; }
            }
            if self.offer_square != NO_SQUARE && (self.offer_seller == seat || self.offer_buyer == seat) {
                self.clear_offer();
            }
            if self.landed_seat == seat { self.clear_landed(); }
            if self.active == seat { self.end_turn(); }
        }

        /// Sets the economics. Takes effect from the next action. `rent_fee_bps` is 0..5000.
        /// `fee_amount` and `penalty_amount` are the fee squares (4, 24, 37) and the penalty square (7).
        pub fn set_economics(&mut self, go_floor: u64, go_scaled: bool, rent_fee_bps: u16, build_on_landing: bool,
                             fee_amount: u64, penalty_amount: u64) {
            assert!(rent_fee_bps <= 5000, "rent fee above 50%");
            assert!(go_floor <= self.go_reward, "GO floor above the GO reward");
            self.go_floor = go_floor;
            self.go_scaled = go_scaled;
            self.rent_fee_bps = rent_fee_bps;
            self.build_on_landing = build_on_landing;
            self.fee_amount = fee_amount;
            self.penalty_amount = penalty_amount;
        }

        /// Resets a seat (regeneration, or a player starting over). `pass_turn` ends the seat's turn
        /// and is only valid for the active seat. Funds are not moved here: the operator settles the
        /// seat's cash in the same transaction.
        pub fn reset_seat(&mut self, seat: u8, pass_turn: bool, expected_round: u64, expected_turn: u64) {
            assert!(self.started, "game not started");
            self.assert_seat(seat);
            assert!(self.round == expected_round && self.turn_count == expected_turn, "stale turn");
            assert!(!pass_turn || self.active == seat, "pass_turn only for the active seat");
            let i = usize::from(seat - 1);
            self.out[i] = false;
            self.held[i] = false;
            self.held_attempts[i] = 0;
            self.held_owed[i] = 0;
            self.positions[i] = 0;
            for sq in 0..usize::from(rules::BOARD_SIZE) {
                if self.owners[sq] == seat { self.owners[sq] = 0; self.improvements[sq] = 0; }
            }
            if self.offer_square != NO_SQUARE && (self.offer_seller == seat || self.offer_buyer == seat) {
                self.clear_offer();
            }
            if self.landed_seat == seat { self.clear_landed(); }
            self.seat_since[i] = self.round;
            self.resets[i] = self.resets[i].checked_add(1).expect("reset overflow");
            self.record_event(6, 255, 0, false, seat, 0, 0);
            if self.active == seat {
                if pass_turn { self.finish_turn(false); } else { self.doubles_streak = 0; }
            }
        }

        // ---------- reads ----------

        /// Read by the Lobby when the session is attached: (game_id, lobby, capacity, params...).
        pub fn get_setup(&self) -> (u64, ComponentAddress, u8, u64, u64, u64, u64, u64, u64, u64, u64, u8) {
            (self.game_id, self.lobby, self.capacity, self.go_reward, self.square_reward, self.fee_amount,
             self.penalty_amount, self.shop_fee, self.storage_fee, self.house_cost, self.hotel_cost, self.buyback_pct)
        }

        /// (game_id, round, active seat, turn_count, started, seats added)
        pub fn get_summary(&self) -> (u64, u64, u8, u64, bool, u8) {
            (self.game_id, self.round, self.active, self.turn_count, self.started, self.seats.len() as u8)
        }

        // ---------- turns ----------

        /// A normal (not held) turn: roll, move, settle the landing in one atomic transaction.
        #[allow(clippy::too_many_arguments)]
        pub fn play_turn(&mut self, seat: u8, die1: u8, die2: u8, expected_round: u64, expected_turn: u64,
                         buy: bool, sell_square: u16, seat_payment: Bucket, house_payment: Bucket) {
            self.assert_turn(seat, expected_round, expected_turn);
            let i = usize::from(seat - 1);
            assert!(!self.held[i], "seat is held in Maintenance; use maintenance_turn");
            self.clear_landed();
            assert!(rules::valid_die(die1) && rules::valid_die(die2), "invalid dice");
            let doubles = die1 == die2;
            self.last_die1 = die1;
            self.last_die2 = die2;
            if doubles && self.doubles_streak + 1 >= 3 {
                // Third double in a row: held on square 10, nothing charged, turn passes.
                assert!(!buy && sell_square == NO_SQUARE, "no purchase or sale when sent to Maintenance");
                self.expect_zero(&seat_payment, &house_payment);
                seat_payment.drop_empty();
                house_payment.drop_empty();
                self.send_to_maintenance(seat);
                self.record_event(4, 255, 0, false, seat, rules::MAINTENANCE_SQUARE, 0);
                self.finish_turn(false);
                return;
            }
            let sent = self.move_and_settle(seat, die1, die2, buy, sell_square, 0, seat_payment, house_payment);
            self.finish_turn(doubles && !sent);
        }

        /// A held seat's turn. `pay` = pay shop fee + storage, then move by the dice.
        /// Otherwise roll for doubles; a double releases and moves; a failure adds storage;
        /// the third failure forces payment and moves.
        #[allow(clippy::too_many_arguments)]
        pub fn maintenance_turn(&mut self, seat: u8, pay: bool, die1: u8, die2: u8, expected_round: u64,
                                expected_turn: u64, buy: bool, sell_square: u16,
                                seat_payment: Bucket, house_payment: Bucket) {
            self.assert_turn(seat, expected_round, expected_turn);
            let i = usize::from(seat - 1);
            assert!(self.held[i], "seat is not held in Maintenance");
            self.clear_landed();
            assert!(rules::valid_die(die1) && rules::valid_die(die2), "invalid dice");
            let doubles = die1 == die2;
            self.last_die1 = die1;
            self.last_die2 = die2;
            if pay {
                let fee = self.held_owed[i];
                self.release(seat);
                let sent = self.move_and_settle(seat, die1, die2, buy, sell_square, fee, seat_payment, house_payment);
                self.finish_turn(doubles && !sent);
                return;
            }
            if doubles {
                // Released for free; moves by this roll; no extra roll.
                self.release(seat);
                self.move_and_settle(seat, die1, die2, buy, sell_square, 0, seat_payment, house_payment);
                self.finish_turn(false);
                return;
            }
            self.held_attempts[i] += 1;
            self.held_owed[i] = self.held_owed[i].checked_add(self.storage_fee).expect("owed overflow");
            if self.held_attempts[i] < rules::MAX_MAINTENANCE_ATTEMPTS {
                assert!(!buy && sell_square == NO_SQUARE, "no purchase or sale while still held");
                self.expect_zero(&seat_payment, &house_payment);
                seat_payment.drop_empty();
                house_payment.drop_empty();
                self.record_event(5, 255, self.held_owed[i], false, seat, rules::MAINTENANCE_SQUARE, 0);
                self.finish_turn(false);
                return;
            }
            // Third failed roll: payment is forced, then the seat moves by this roll.
            let fee = self.held_owed[i];
            self.release(seat);
            self.move_and_settle(seat, die1, die2, buy, sell_square, fee, seat_payment, house_payment);
            self.finish_turn(false);
        }

        /// The active seat sells one of its properties back to the House at the buyback value.
        /// Several calls may precede a turn in the same transaction (e.g. a forced sale).
        pub fn sell_to_house(&mut self, seat: u8, square: u16, expected_round: u64, expected_turn: u64,
                             house_payment: Bucket) {
            self.assert_turn(seat, expected_round, expected_turn);
            assert!(self.offer_square == NO_SQUARE, "resolve the pending offer first");
            assert!(rules::is_asset(square), "not an asset");
            assert!(self.owners[square as usize] == seat, "seat does not own this asset");
            let value = rules::buyback_value(square, self.improvements[square as usize], &self.params());
            assert!(house_payment.resource_address() == self.resource, "wrong resource");
            assert!(house_payment.amount() == Amount::from(value), "incorrect buyback amount");
            self.deposit(self.seats[usize::from(seat - 1)], house_payment);
            self.owners[square as usize] = 0;
            self.improvements[square as usize] = 0;
        }

        /// The active seat pays a held seat's shop fee plus storage as a gift; the held seat is released.
        pub fn pay_shop_fee(&mut self, payer: u8, held_seat: u8, expected_round: u64, expected_turn: u64,
                            payment: Bucket) {
            self.assert_turn(payer, expected_round, expected_turn);
            self.assert_seat(held_seat);
            assert!(payer != held_seat, "use maintenance_turn for your own shop fee");
            let h = usize::from(held_seat - 1);
            assert!(!self.out[h] && self.held[h], "seat is not held in Maintenance");
            let owed = self.held_owed[h];
            assert!(payment.resource_address() == self.resource, "wrong resource");
            assert!(payment.amount() == Amount::from(owed), "incorrect shop fee amount");
            self.distribute(1, payment, owed);
            self.release(held_seat);
        }

        // ---------- private sales ----------

        pub fn propose_sale(&mut self, seller: u8, buyer: u8, square: u16, price: u64,
                            expected_round: u64, expected_turn: u64) {
            self.assert_turn(seller, expected_round, expected_turn);
            self.assert_seat(buyer);
            assert!(buyer != seller && !self.out[usize::from(buyer - 1)], "invalid buyer");
            assert!(self.offer_square == NO_SQUARE, "offer already pending");
            assert!(rules::is_asset(square) && self.owners[square as usize] == seller, "not the seller's asset");
            assert!(price > 0, "price must be positive");
            self.offer_square = square;
            self.offer_price = price;
            self.offer_seller = seller;
            self.offer_buyer = buyer;
            self.offer_nonce = self.offer_nonce.checked_add(1).expect("nonce overflow");
        }

        pub fn accept_sale(&mut self, buyer: u8, expected_nonce: u64, expected_round: u64, expected_turn: u64,
                           payment: Bucket) {
            self.assert_offer(buyer, expected_nonce, expected_round, expected_turn);
            assert!(payment.resource_address() == self.resource, "wrong resource");
            assert!(payment.amount() == Amount::from(self.offer_price), "incorrect sale price");
            let seller_account = self.seats[usize::from(self.offer_seller - 1)];
            self.deposit(seller_account, payment);
            self.owners[self.offer_square as usize] = buyer;
            self.clear_offer();
        }

        pub fn decline_sale(&mut self, buyer: u8, expected_nonce: u64, expected_round: u64, expected_turn: u64) {
            self.assert_offer(buyer, expected_nonce, expected_round, expected_turn);
            self.clear_offer();
        }

        // ---------- development ----------

        /// Build on landing on: only `landed_square`, by `landed_seat`, before the next move; one level.
        /// Off: the active seat on any owned asset during its turn.
        pub fn build_improvement(&mut self, seat: u8, square: u16, expected_round: u64, expected_turn: u64,
                                 expected_level: u8, payment: Bucket) {
            if self.build_on_landing {
                assert!(self.started, "game not started");
                self.assert_seat(seat);
                assert!(!self.out[usize::from(seat - 1)], "seat is out of the game");
                assert!(self.round == expected_round && self.turn_count == expected_turn, "stale turn");
                assert!(self.landed_seat == seat && self.landed_square == square, "build only on the asset just landed on");
                self.assert_signer(seat);
            } else {
                self.assert_turn(seat, expected_round, expected_turn);
            }
            assert!(self.offer_square == NO_SQUARE, "resolve the pending offer first");
            assert!(rules::is_asset(square) && self.owners[square as usize] == seat, "seat does not own this asset");
            let level = self.improvements[square as usize];
            assert!(level == expected_level && level < 3, "invalid or stale development level");
            let cost = rules::next_improvement_cost(level, &self.params());
            assert!(payment.resource_address() == self.resource, "wrong resource");
            assert!(payment.amount() == Amount::from(cost), "incorrect construction payment");
            self.distribute(0, payment, cost);
            self.improvements[square as usize] = level + 1;
            if self.build_on_landing { self.clear_landed(); }
        }

        // ---------- internal ----------

        fn params(&self) -> Params {
            Params { go_reward: self.go_reward, square_reward: self.square_reward, fee_amount: self.fee_amount,
                penalty_amount: self.penalty_amount, shop_fee: self.shop_fee, storage_fee: self.storage_fee,
                house_cost: self.house_cost, hotel_cost: self.hotel_cost, buyback_pct: self.buyback_pct }
        }

        fn seat_identity(&self, seat: u8) -> Option<NonFungibleAddress> {
            ComponentManager::get(self.lobby).call("seat_identity", args![self.game_id, seat])
        }

        fn assert_seat(&self, seat: u8) {
            assert!(seat >= 1 && usize::from(seat) <= self.seats.len(), "invalid seat");
        }

        /// Seat exists, is in play, signed this transaction, and the turn guards match.
        fn assert_turn(&self, seat: u8, expected_round: u64, expected_turn: u64) {
            assert!(self.started, "game not started");
            self.assert_seat(seat);
            assert!(!self.out[usize::from(seat - 1)], "seat is out of the game");
            assert!(self.active == seat, "not this seat's turn");
            assert!(self.round == expected_round && self.turn_count == expected_turn, "stale turn");
            self.assert_signer(seat);
        }

        fn assert_signer(&self, seat: u8) {
            let identity = self.seat_identity(seat).expect("seat has no claimed identity");
            CallerContext::assert_badge_in_scope(&identity);
        }

        fn assert_offer(&self, buyer: u8, nonce: u64, round: u64, turn: u64) {
            assert!(self.started, "game not started");
            assert!(self.offer_square != NO_SQUARE, "no pending offer");
            assert!(self.offer_buyer == buyer, "wrong recipient");
            assert!(self.offer_nonce == nonce && self.round == round && self.turn_count == turn, "stale offer");
            assert!(self.owners[self.offer_square as usize] == self.offer_seller, "ownership changed");
            self.assert_signer(buyer);
        }

        fn clear_landed(&mut self) {
            self.landed_seat = 0;
            self.landed_square = NO_SQUARE;
        }

        /// Parameters for a landing: the stored ones, with GO scaled from the board when enabled.
        fn landing_params(&self) -> Params {
            let mut p = self.params();
            if self.go_scaled {
                p.go_reward = rules::scaled_go(self.go_reward, self.go_floor, rules::unowned_assets(&self.owners));
            }
            p
        }

        fn clear_offer(&mut self) {
            self.offer_square = NO_SQUARE;
            self.offer_price = 0;
            self.offer_seller = 0;
            self.offer_buyer = 0;
        }

        fn expect_zero(&self, a: &Bucket, b: &Bucket) {
            assert!(a.resource_address() == self.resource && b.resource_address() == self.resource, "wrong resource");
            assert!(a.amount() == Amount::from(0u64) && b.amount() == Amount::from(0u64), "no payment expected");
        }

        fn send_to_maintenance(&mut self, seat: u8) {
            let i = usize::from(seat - 1);
            self.positions[i] = rules::MAINTENANCE_SQUARE;
            self.held[i] = true;
            self.held_attempts[i] = 0;
            self.held_owed[i] = self.shop_fee;
        }

        fn release(&mut self, seat: u8) {
            let i = usize::from(seat - 1);
            self.held[i] = false;
            self.held_attempts[i] = 0;
            self.held_owed[i] = 0;
        }

        fn record_event(&mut self, kind: u8, card: u8, amount: u64, positive: bool, seat: u8, square: u16, recipient: u8) {
            self.event_kind = kind;
            self.event_card = card;
            self.event_amount = amount;
            self.event_positive = positive;
            self.event_seat = seat;
            self.event_square = square;
            self.event_recipient = recipient;
        }

        /// Moves the seat, settles the landing and any `extra_fee` (Maintenance payment) atomically.
        /// Returns true when the landing sent the seat to Maintenance (square 30).
        #[allow(clippy::too_many_arguments)]
        fn move_and_settle(&mut self, seat: u8, die1: u8, die2: u8, buy: bool, sell_square: u16, extra_fee: u64,
                           seat_payment: Bucket, house_payment: Bucket) -> bool {
            assert!(self.offer_square == NO_SQUARE, "resolve the pending offer first");
            let i = usize::from(seat - 1);
            let p = self.params();
            let lp = self.landing_params();
            let from = self.positions[i];
            let to = rules::destination(from, die1, die2);

            let sale = if sell_square != NO_SQUARE {
                assert!(rules::is_asset(sell_square) && self.owners[sell_square as usize] == seat, "seat does not own the sale asset");
                rules::buyback_value(sell_square, self.improvements[sell_square as usize], &p)
            } else { 0 };
            if sell_square != NO_SQUARE {
                self.owners[sell_square as usize] = 0;
                self.improvements[sell_square as usize] = 0;
            }

            let landing = rules::land(from, to, seat, &self.owners, &self.improvements, self.turn_count, &lp);
            let purchase = if buy {
                assert!(!landing.sent_to_maintenance && rules::is_asset(to), "destination is not an asset");
                assert!(self.owners[to as usize] == 0, "asset already owned");
                rules::base_price(to)
            } else { 0 };
            let fees = landing.penalty.checked_add(extra_fee).expect("fee overflow");
            let f = rules::funding(sale, landing.rewards(), purchase, fees, landing.rent);

            assert!(seat_payment.resource_address() == self.resource, "wrong seat resource");
            assert!(house_payment.resource_address() == self.resource, "wrong house resource");
            assert!(seat_payment.amount() == Amount::from(f.seat_funding), "incorrect seat funding");
            assert!(house_payment.amount() == Amount::from(f.incoming), "incorrect house funding");

            let mut funds = house_payment.join(seat_payment);
            if purchase > 0 {
                let b = funds.take(Amount::from(purchase));
                self.distribute(0, b, purchase);
            }
            if fees > 0 {
                let b = funds.take(Amount::from(fees));
                self.distribute(1, b, fees);
            }
            if landing.rent > 0 {
                let mut b = funds.take(Amount::from(landing.rent));
                let fee = rules::rent_fee(landing.rent, self.rent_fee_bps);
                if fee > 0 {
                    let f = b.take(Amount::from(fee));
                    self.distribute(1, f, fee);
                }
                self.deposit(self.seats[usize::from(landing.rent_owner - 1)], b);
            }
            if funds.is_empty() { funds.drop_empty(); } else { self.deposit(self.seats[i], funds); }

            if buy { self.owners[to as usize] = seat; }
            if landing.sent_to_maintenance {
                self.send_to_maintenance(seat);
                self.record_event(4, 255, 0, false, seat, rules::BACK_TO_MAINTENANCE, 0);
            } else {
                self.positions[i] = to;
                self.landed_seat = seat;
                self.landed_square = to;
                if landing.rent > 0 {
                    self.record_event(3, 255, landing.rent, false, seat, to, landing.rent_owner);
                } else {
                    self.record_event(landing.card_kind, landing.card_id, landing.card_amount,
                        landing.card_kind != 0 && landing.card_positive, seat, to, 0);
                }
            }
            landing.sent_to_maintenance
        }

        /// Ends the move. `again` keeps the same seat (doubles); otherwise rotate to the next seat in play.
        fn finish_turn(&mut self, again: bool) {
            self.turn_count = self.turn_count.checked_add(1).expect("turn overflow");
            if again {
                self.doubles_streak += 1;
            } else {
                self.end_turn();
            }
        }

        fn end_turn(&mut self) {
            self.doubles_streak = 0;
            let (next, wrapped) = rules::next_seat(self.active, &self.out);
            if wrapped { self.round = self.round.checked_add(1).expect("round overflow"); }
            self.active = next;
        }

        fn deposit(&self, account: ComponentAddress, bucket: Bucket) {
            ComponentManager::get(account).invoke("deposit", args![bucket]);
        }

        fn distribute(&self, category: u8, bucket: Bucket, units: u64) {
            ComponentManager::get(self.treasury).invoke("distribute", args![self.game_id, category, bucket, units]);
        }
    }
}

// End of contracts/town_square/src/lib.rs
// Location: contracts/town_square/src/lib.rs · Created: 2026-11-16 04:06:01 UTC
