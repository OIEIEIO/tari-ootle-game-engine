//! Ootle Asset Game | WASM GameSession v0.6.10
//! 40-square persistent board with two-dice turns, GO payout, and doubles replay.
//! v0.6.10: 23-field v0.6.8 state followed by 5 persisted private-offer fields.
use tari_template_lib::prelude::*;

#[template]
mod ootle_game_session {
    use super::*;

    const BOARD_SIZE: u16 = 40;
    const ASSET_COUNT: usize = 22;
    const GO_REWARD: u64 = 20_000_000;
    const HOUSE_COST: u64 = 5_000_000;
    const HOTEL_COST: u64 = 15_000_000;
    // Development level 0: empty; 1: house; 2: two houses; 3: two houses + hotel.
    // Incremental rent: 10% of base + 5% per house + 15% for the hotel.
    // kinds: 0 GO, 1 ASSET, 2 CHANCE, 3 COMMUNITY, 4 FEE,
    // 5 PENALTY, 6 REWARD, 7 UTILITY, 8 TRANSIT, 9 CORNER
    const KINDS: [u8; 40] = [0,1,2,1,4,3,1,5,1,6,9,1,7,1,1,8,1,3,1,1,9,1,2,1,4,1,1,3,1,1,9,1,1,2,1,8,1,4,1,1];
    const ASSETS: [(u16, u64); ASSET_COUNT] = [
        (1,10_000_000),(3,20_000_000),(6,30_000_000),(8,40_000_000),
        (11,18_000_000),(13,20_000_000),(14,22_000_000),(16,24_000_000),
        (18,26_000_000),(19,28_000_000),(21,30_000_000),(23,32_000_000),
        (25,34_000_000),(26,36_000_000),(28,38_000_000),(29,40_000_000),
        (31,42_000_000),(32,44_000_000),(34,46_000_000),(36,48_000_000),
        (38,50_000_000),(39,52_000_000),
    ];

    pub struct OotleGameSession {
        player1: ComponentAddress,
        player2: ComponentAddress,
        board_size: u16,
        round: u64,
        active_player: u8,
        position1: u16,
        position2: u16,
        last_roll1: u8,
        last_roll2: u8,
        turn_count: u64,
        square_types: Vec<u8>,
        square_prices: Vec<u64>,
        owners: Vec<u8>,
        house: ComponentAddress,
        controller: ComponentAddress,
        resource: ResourceAddress,
        last_event_kind: u8,       // 0 none, 1 Chance, 2 Community, 3 rent
        last_card_id: u8,          // 0-3 for cards, 255 for none/rent
        last_event_amount: u64,    // unsigned base units
        last_event_positive: u8,   // 1 earned, 0 paid
        last_event_player: u8,     // visitor / card recipient
        last_event_square: u16,
        last_event_recipient: u8,  // rent owner; 0 for house-funded card
        offer_square: u16,        // u16::MAX means no pending trade
        offer_price: u64,
        offer_seller: u8,
        offer_buyer: u8,
        offer_nonce: u64,         // increments whenever a proposal is created
        improvements: Vec<u8>,    // 40 entries: 0, 1, 2 or 3 stages
    }

    impl OotleGameSession {
        fn price(&self, square: u16) -> u64 { self.square_prices[square as usize] }
        fn owner(&self, square: u16) -> u8 { self.owners[square as usize] }
        fn set_owner(&mut self, square: u16, owner: u8) {
            assert!(owner <= 2 && self.price(square) > 0, "invalid asset ownership");
            self.owners[square as usize] = owner;
        }
        fn account(&self, p: u8) -> ComponentAddress {
            if p == 1 { self.player1 } else { self.player2 }
        }
        fn deposit(&self, account: ComponentAddress, bucket: Bucket) {
            ComponentManager::get(account).invoke("deposit", args![bucket]);
        }
        fn distribute(&self, category: u8, bucket: Bucket, units: u64) {
            ComponentManager::get(self.controller).invoke("distribute", args![category, bucket, units]);
        }
        fn go_reward(from: u16, to: u16) -> u64 { if to < from { GO_REWARD } else { 0 } }
        fn board_reward(square: u16) -> u64 { if square == 9 { 2_000_000 } else { 0 } }
        fn card(square: u16, turn_count: u64) -> (u8, u8, u64, bool) {
            let kind = if square == 2 || square == 22 || square == 33 { 1u8 }
                       else if square == 5 || square == 17 || square == 27 { 2u8 }
                       else { 0u8 };
            if kind == 0 { return (0, 255, 0, false); }
            // Deterministic local-lab card cycle; not secure random draws.
            let id = ((turn_count + u64::from(square)) % 4) as u8;
            let (amount, positive) = match (kind,id) {
                (1,0) => (2_000_000,true),
                (1,1) => (1_000_000,false),
                (1,2) => (3_000_000,true),
                (1,3) => (2_000_000,false),
                (2,0) => (1_000_000,true),
                (2,1) => (1_000_000,false),
                (2,2) => (2_000_000,true),
                _ => (2_000_000,false),
            };
            (kind,id,amount,positive)
        }
        fn improvement_cost(level: u8) -> u64 {
            match level { 0 => 0, 1 => HOUSE_COST, 2 => 2 * HOUSE_COST,
                3 => 2 * HOUSE_COST + HOTEL_COST, _ => panic!("invalid development") }
        }
        fn developed_value(&self, square: u16) -> u64 {
            self.price(square).checked_add(Self::improvement_cost(self.improvements[square as usize]))
                .expect("property valuation overflow")
        }
        fn rent_amount(&self, square: u16) -> u64 {
            let price = self.price(square);
            let level = self.improvements[square as usize];
            // All prices in current game are divisible by 20.
            let percent = 10 + 5 * u64::from(level.min(2)) + if level == 3 { 15 } else { 0 };
            price.checked_mul(percent).expect("rent overflow") / 100
        }
        fn rent(&self, square: u16, visitor: u8) -> (u8,u64) {
            if self.price(square)==0 { return (0,0); }
            let owner=self.owner(square);
            if owner==0 || owner==visitor { (0,0) }
            else { (owner,self.rent_amount(square)) }
        }
        fn penalty(square: u16) -> u64 {
            match square { 4 | 24 | 37 => 1_000_000, 7 => 2_000_000, _ => 0 }
        }
        fn make_prices() -> Vec<u64> {
            let mut prices = vec![0u64; BOARD_SIZE as usize];
            for (square, price) in ASSETS.iter() { prices[*square as usize] = *price; }
            prices
        }
        fn create_session(
            player1: ComponentAddress, player2: ComponentAddress,
            house: ComponentAddress, controller: ComponentAddress, resource: ResourceAddress,
            round: u64, active_player: u8, position1: u16, position2: u16,
            last_roll1: u8, last_roll2: u8, turn_count: u64, owners: Vec<u8>,
            event: (u8, u8, u64, u8, u8, u16, u8),
            offer_nonce: u64,
        ) -> Component<Self> {
            assert!(player1 != player2 && player1 != house && player2 != house, "duplicate accounts");
            assert!(round > 0 && (1..=2).contains(&active_player), "invalid turn");
            assert!(position1 < BOARD_SIZE && position2 < BOARD_SIZE, "invalid positions");
            assert!(owners.len() == BOARD_SIZE as usize, "invalid owner vector length");
            assert!(owners.iter().all(|owner| *owner <= 2), "invalid ownership");
            for (idx, owner) in owners.iter().enumerate() {
                let is_asset = ASSETS.iter().any(|(square, _)| *square as usize == idx);
                assert!(is_asset || *owner == 0, "non-asset squares cannot have owners");
            }
            Component::new(Self {
                player1,
                player2,
                board_size: BOARD_SIZE,
                round,
                active_player,
                position1,
                position2,
                last_roll1,
                last_roll2,
                turn_count,
                square_types: KINDS.to_vec(),
                square_prices: Self::make_prices(),
                owners,
                house,
                controller,
                resource,
                last_event_kind: event.0,
                last_card_id: event.1,
                last_event_amount: event.2,
                last_event_positive: event.3,
                last_event_player: event.4,
                last_event_square: event.5,
                last_event_recipient: event.6,
                offer_square: u16::MAX,
                offer_price: 0,
                offer_seller: 0,
                offer_buyer: 0,
                offer_nonce,
                improvements: vec![0u8; BOARD_SIZE as usize],
            })
            .with_access_rules(ComponentAccessRules::new()
                .method("play_turn", rule!(allow_all))
                .method("propose_sale", rule!(allow_all))
                .method("accept_sale", rule!(allow_all))
                .method("decline_sale", rule!(allow_all))
                .method("build_improvement", rule!(allow_all))
                .default(rule!(deny_all)))
            .with_owner_rule(OwnerRule::OwnedBySigner)
            .create()
        }

        /// All 22 ownership flags use two bits each, packed into a single u64.
        /// This avoids the Tari WASM template 32-argument maximum.
        #[allow(clippy::too_many_arguments)]
        pub fn new_from_existing(
            player1: ComponentAddress, player2: ComponentAddress,
            house: ComponentAddress, controller: ComponentAddress, resource: ResourceAddress,
            round: u64, active_player: u8, position1: u16, position2: u16,
            last_roll1: u8, last_roll2: u8, turn_count: u64,
            packed_owners: u64,
            last_event_kind: u8, last_card_id: u8, last_event_amount: u64,
            last_event_positive: u8, last_event_player: u8,
            last_event_square: u16, last_event_recipient: u8,
            offer_nonce: u64,
        ) -> Component<Self> {
            assert!(packed_owners >> (2 * ASSET_COUNT) == 0, "unexpected owner bits");
            let mut owners = vec![0u8; BOARD_SIZE as usize];
            for (index, (square, _)) in ASSETS.iter().enumerate() {
                let owner = ((packed_owners >> (index * 2)) & 3) as u8;
                assert!(owner <= 2, "invalid owner in packed data");
                owners[*square as usize] = owner;
            }
            Self::create_session(
                player1, player2, house, controller, resource,
                round, active_player, position1, position2,
                last_roll1, last_roll2, turn_count, owners,
                (last_event_kind, last_card_id, last_event_amount,
                 last_event_positive, last_event_player, last_event_square, last_event_recipient),
                offer_nonce,
            )
        }

        /// Upgrade owned property one stage: house, house, hotel.
        /// Must occur before rolling, with no pending trade. Treasury receives construction funds.
        pub fn build_improvement(&mut self, player: u8, square: u16,
                                 expected_round: u64, expected_turn_count: u64,
                                 expected_level: u8, payment: Bucket) {
            assert!(self.offer_square == u16::MAX, "resolve trade before building");
            assert!(self.round == expected_round && self.turn_count == expected_turn_count,
                    "stale improvement request");
            assert!((1..=2).contains(&player) && self.active_player == player, "not active player");
            assert!(square < BOARD_SIZE && self.price(square) > 0, "not a property");
            assert!(self.owner(square) == player, "property not owned by player");
            let level = self.improvements[square as usize];
            assert!(level == expected_level && level < 3, "invalid or stale improvement level");
            let cost = if level < 2 { HOUSE_COST } else { HOTEL_COST };
            assert!(payment.resource_address() == self.resource, "incorrect improvement currency");
            assert!(payment.amount() == Amount::from(cost), "incorrect improvement payment");
            self.distribute(0, payment, cost);
            self.improvements[square as usize] = level + 1;
        }

        /// Offer is available only to the other player, before the dice are rolled.
        /// The backend signs with the seller wallet; see DEV auth limitation in README.
        pub fn propose_sale(&mut self, seller: u8, square: u16, price: u64,
                            expected_round: u64, expected_turn_count: u64) {
            assert!(self.offer_square == u16::MAX, "offer already pending");
            assert!(self.round == expected_round && self.turn_count == expected_turn_count,
                    "stale proposal");
            assert!(self.active_player == seller && (1..=2).contains(&seller), "not active seller");
            assert!(square < BOARD_SIZE && self.price(square) > 0, "not an asset");
            assert!(self.owner(square) == seller, "not seller's asset");
            assert!(price > 0, "price must be positive");
            self.offer_square = square;
            self.offer_price = price;
            self.offer_seller = seller;
            self.offer_buyer = 3 - seller;
            self.offer_nonce = self.offer_nonce.checked_add(1).expect("offer nonce overflow");
        }

        pub fn decline_sale(&mut self, buyer: u8, expected_nonce: u64,
                            expected_round: u64, expected_turn_count: u64) {
            self.assert_pending(buyer, expected_nonce, expected_round, expected_turn_count);
            self.clear_offer();
        }

        fn assert_pending(&self, buyer: u8, nonce: u64, round: u64, turns: u64) {
            assert!(self.offer_square != u16::MAX, "no pending offer");
            assert!(self.offer_buyer == buyer && buyer != self.offer_seller, "wrong recipient");
            assert!(self.offer_nonce == nonce && self.round == round && self.turn_count == turns,
                    "stale offer");
            assert!(self.active_player == self.offer_seller, "offer seller not active");
            assert!(self.owner(self.offer_square) == self.offer_seller, "ownership changed");
        }

        fn clear_offer(&mut self) {
            self.offer_square = u16::MAX;
            self.offer_price = 0;
            self.offer_seller = 0;
            self.offer_buyer = 0;
        }

        /// The buyer supplies the whole payment bucket; both settlement legs are atomic.
        pub fn accept_sale(&mut self, buyer: u8, expected_nonce: u64,
                           expected_round: u64, expected_turn_count: u64, payment: Bucket) {
            self.assert_pending(buyer, expected_nonce, expected_round, expected_turn_count);
            assert!(payment.resource_address() == self.resource, "wrong trade currency");
            assert!(payment.amount() == Amount::from(self.offer_price), "wrong trade price");
            self.deposit(self.account(self.offer_seller), payment);
            self.set_owner(self.offer_square, buyer);
            self.clear_offer();
        }

        #[allow(clippy::too_many_arguments)]
        pub fn play_turn(
            &mut self, die1: u8, die2: u8, expected_round: u64, expected_player: u8,
            buy: bool, sell_square: u16, player_payment: Bucket, house_payment: Bucket,
        ) {
            assert!((1..=6).contains(&die1) && (1..=6).contains(&die2), "invalid dice");
            assert!(self.round == expected_round && self.active_player == expected_player, "stale turn");
            assert!(self.offer_square == u16::MAX, "pending private offer must be resolved");
            assert!(sell_square == u16::MAX || (sell_square < BOARD_SIZE && self.price(sell_square) > 0), "invalid sale");
            let total_roll = u16::from(die1) + u16::from(die2);
            let doubles = die1 == die2;
            let player = self.account(expected_player);
            let old = if expected_player == 1 { self.position1 } else { self.position2 };
            let destination = (old + total_roll) % self.board_size;
            let sale = if sell_square != u16::MAX {
                assert!(self.owner(sell_square) == expected_player, "player does not own asset");
                self.developed_value(sell_square) * 8 / 10
            } else { 0 };
            let purchase = if buy {
                let price = self.price(destination);
                assert!(price > 0, "destination is not an asset");
                assert!(self.owner(destination) == 0 || (sell_square == destination && self.owner(destination) == expected_player), "asset already owned");
                price
            } else { 0 };
            let (card_kind, card_id, card_amount, card_positive) = Self::card(destination,self.turn_count);
            let (rent_owner, rent) = self.rent(destination,expected_player);
            let card_reward = if card_positive { card_amount } else { 0 };
            let card_penalty = if card_kind!=0 && !card_positive { card_amount } else { 0 };
            let penalty = Self::penalty(destination).checked_add(card_penalty).expect("penalty overflow");
            let reward = Self::board_reward(destination)
                .checked_add(Self::go_reward(old, destination)).expect("reward overflow")
                .checked_add(card_reward).expect("card reward overflow");
            let incoming = sale.checked_add(reward).expect("incoming overflow");
            let outgoing = purchase.checked_add(penalty).expect("outgoing overflow")
                .checked_add(rent).expect("rent overflow");
            let player_funding = outgoing.saturating_sub(incoming);
            assert!(player_payment.resource_address() == self.resource, "wrong player resource");
            assert!(house_payment.resource_address() == self.resource, "wrong house resource");
            assert!(player_payment.amount() == Amount::from(player_funding), "incorrect player funding");
            assert!(house_payment.amount() == Amount::from(incoming), "incorrect house funding");
            let mut funds = house_payment.join(player_payment);
            if purchase > 0 {
                let bucket = funds.take(Amount::from(purchase));
                self.distribute(0, bucket, purchase);
            }
            if penalty > 0 {
                let bucket = funds.take(Amount::from(penalty));
                self.distribute(1, bucket, penalty);
            }
            if rent > 0 {
                let bucket = funds.take(Amount::from(rent));
                self.deposit(self.account(rent_owner), bucket);
            }
            self.last_event_kind = if rent > 0 { 3 } else { card_kind };
            self.last_card_id = if rent > 0 { 255 } else { card_id };
            self.last_event_amount = if rent > 0 { rent } else { card_amount };
            self.last_event_positive = if rent == 0 && card_positive { 1 } else { 0 };
            self.last_event_player = expected_player;
            self.last_event_square = destination;
            self.last_event_recipient = rent_owner;
            if funds.is_empty() { funds.drop_empty(); } else { self.deposit(player, funds); }
            if sell_square != u16::MAX {
                self.set_owner(sell_square, 0);
                self.improvements[sell_square as usize] = 0;
            }
            if buy { self.set_owner(destination, expected_player); }
            if expected_player == 1 {
                self.position1 = destination;
                self.last_roll1 = total_roll as u8;
                if !doubles { self.active_player = 2; }
            } else {
                self.position2 = destination;
                self.last_roll2 = total_roll as u8;
                if doubles {
                    self.active_player = 2;
                } else {
                    self.active_player = 1;
                    self.round = self.round.checked_add(1).expect("round overflow");
                }
            }
            self.turn_count = self.turn_count.checked_add(1).expect("turn overflow");
        }
    }
}
