//! Ootle Game Engine | contracts/town_square/src/rules.rs
//! Version: v0.3.0
//! Pure board rules for the Town Square Classic game type. No engine calls, so every
//! function here is unit-tested natively (`cargo test` in contracts/town_square).
//! Rules carried over from Ootle Asset Game v0.6.18, generalised to any seat number, plus
//! the agreed Maintenance rule. Amounts are 6-decimal base units.
//! v0.3.0 adds the economics helpers used by the session: scaled GO and the rent fee.

pub const BOARD_SIZE: u16 = 40;
pub const MAINTENANCE_SQUARE: u16 = 10;
pub const BACK_TO_MAINTENANCE: u16 = 30;
pub const REWARD_SQUARE: u16 = 9;
pub const MAX_MAINTENANCE_ATTEMPTS: u8 = 3;
pub const NO_SQUARE: u16 = u16::MAX;

/// Square kinds: 0 GO, 1 asset, 2 Chance, 3 Community, 4 fee, 5 penalty, 6 reward,
/// 7 utility, 8 transit, 9 corner. Identical to v0.6.18.
pub const KINDS: [u8; 40] = [
    0, 1, 2, 1, 4, 3, 1, 5, 1, 6, 9, 1, 7, 1, 1, 8, 1, 3, 1, 1,
    9, 1, 2, 1, 4, 1, 1, 3, 1, 1, 9, 1, 1, 2, 1, 8, 1, 4, 1, 1,
];

/// Asset squares and base prices. Identical to v0.6.18.
pub const ASSETS: [(u16, u64); 22] = [
    (1, 10_000_000), (3, 20_000_000), (6, 30_000_000), (8, 40_000_000),
    (11, 18_000_000), (13, 20_000_000), (14, 22_000_000), (16, 24_000_000),
    (18, 26_000_000), (19, 28_000_000), (21, 30_000_000), (23, 32_000_000),
    (25, 34_000_000), (26, 36_000_000), (28, 38_000_000), (29, 40_000_000),
    (31, 42_000_000), (32, 44_000_000), (34, 46_000_000), (36, 48_000_000),
    (38, 50_000_000), (39, 52_000_000),
];

/// Game-type parameters, copied into each game at creation and fixed for its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    pub go_reward: u64,
    pub square_reward: u64,
    pub fee_amount: u64,
    pub penalty_amount: u64,
    pub shop_fee: u64,
    pub storage_fee: u64,
    pub house_cost: u64,
    pub hotel_cost: u64,
    pub buyback_pct: u8,
}

impl Params {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.buyback_pct == 0 || self.buyback_pct > 100 {
            return Err("buyback percentage must be 1..100");
        }
        if self.house_cost == 0 || self.hotel_cost == 0 || self.shop_fee == 0 {
            return Err("house, hotel and shop fee must be positive");
        }
        Ok(())
    }
}

pub fn base_price(square: u16) -> u64 {
    ASSETS.iter().find(|(s, _)| *s == square).map(|(_, p)| *p).unwrap_or(0)
}

pub fn is_asset(square: u16) -> bool {
    base_price(square) > 0
}

pub fn asset_index(square: u16) -> Option<usize> {
    ASSETS.iter().position(|(s, _)| *s == square)
}

/// Total construction cost for a development level: 0 none, 1 house, 2 houses, 3 houses + hotel.
pub fn improvement_total(level: u8, p: &Params) -> u64 {
    match level {
        0 => 0,
        1 => p.house_cost,
        2 => p.house_cost * 2,
        3 => p.house_cost * 2 + p.hotel_cost,
        _ => panic!("invalid development level"),
    }
}

/// Cost of the next development step from `level`.
pub fn next_improvement_cost(level: u8, p: &Params) -> u64 {
    match level {
        0 | 1 => p.house_cost,
        2 => p.hotel_cost,
        _ => panic!("fully developed"),
    }
}

pub fn developed_value(square: u16, level: u8, p: &Params) -> u64 {
    base_price(square)
        .checked_add(improvement_total(level, p))
        .expect("value overflow")
}

/// House buyback of a developed property. Integer division; any remainder stays with the House.
pub fn buyback_value(square: u16, level: u8, p: &Params) -> u64 {
    developed_value(square, level, p) * u64::from(p.buyback_pct) / 100
}

/// Rent: 10% of base, +5% per house (up to 2), +15% for the hotel. Same as v0.6.18.
pub fn rent_amount(square: u16, level: u8) -> u64 {
    let percent = 10 + 5 * u64::from(level.min(2)) + if level == 3 { 15 } else { 0 };
    base_price(square).checked_mul(percent).expect("rent overflow") / 100
}

pub fn square_penalty(square: u16, p: &Params) -> u64 {
    match square {
        4 | 24 | 37 => p.fee_amount,
        7 => p.penalty_amount,
        _ => 0,
    }
}

pub fn square_reward(square: u16, p: &Params) -> u64 {
    if square == REWARD_SQUARE { p.square_reward } else { 0 }
}

/// Deterministic card cycle, as v0.6.18: (kind, id, amount, positive). Kind 1 Chance, 2 Community.
pub fn card(square: u16, turn_count: u64) -> (u8, u8, u64, bool) {
    let kind = match square {
        2 | 22 | 33 => 1u8,
        5 | 17 | 27 => 2u8,
        _ => return (0, 255, 0, false),
    };
    let id = ((turn_count + u64::from(square)) % 4) as u8;
    let (amount, positive) = match (kind, id) {
        (1, 0) => (2_000_000, true),
        (1, 1) => (1_000_000, false),
        (1, 2) => (3_000_000, true),
        (1, 3) => (2_000_000, false),
        (2, 0) => (1_000_000, true),
        (2, 1) => (1_000_000, false),
        (2, 2) => (2_000_000, true),
        _ => (2_000_000, false),
    };
    (kind, id, amount, positive)
}

/// Assets with no owner (the House holds them).
pub fn unowned_assets(owners: &[u8]) -> u64 {
    ASSETS.iter().filter(|(sq, _)| owners[usize::from(*sq)] == 0).count() as u64
}

/// Scaled GO reward: max(floor, base × unowned assets ÷ 22), integer division.
pub fn scaled_go(base: u64, floor: u64, unowned: u64) -> u64 {
    let scaled = (u128::from(base) * u128::from(unowned) / ASSETS.len() as u128) as u64;
    scaled.max(floor)
}

/// Rent fee in basis points of the rent, taken from the owner's receipt. Integer division.
pub fn rent_fee(rent: u64, bps: u16) -> u64 {
    (u128::from(rent) * u128::from(bps) / 10_000) as u64
}

pub fn destination(from: u16, die1: u8, die2: u8) -> u16 {
    (from + u16::from(die1) + u16::from(die2)) % BOARD_SIZE
}

pub fn passed_go(from: u16, to: u16) -> bool {
    to < from
}

pub fn valid_die(d: u8) -> bool {
    (1..=6).contains(&d)
}

/// Outcome of landing on `to` after moving from `from`, for seat `seat`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Landing {
    pub to: u16,
    pub go_reward: u64,
    pub square_reward: u64,
    pub card_kind: u8,
    pub card_id: u8,
    pub card_amount: u64,
    pub card_positive: bool,
    pub penalty: u64,
    pub rent_owner: u8,
    pub rent: u64,
    pub sent_to_maintenance: bool,
}

pub fn land(
    from: u16,
    to: u16,
    seat: u8,
    owners: &[u8],
    improvements: &[u8],
    turn_count: u64,
    p: &Params,
) -> Landing {
    let go = if passed_go(from, to) { p.go_reward } else { 0 };
    if to == BACK_TO_MAINTENANCE {
        return Landing {
            to: MAINTENANCE_SQUARE, go_reward: go, square_reward: 0, card_kind: 0, card_id: 255,
            card_amount: 0, card_positive: false, penalty: 0, rent_owner: 0, rent: 0,
            sent_to_maintenance: true,
        };
    }
    let (card_kind, card_id, card_amount, card_positive) = card(to, turn_count);
    let owner = owners[to as usize];
    let (rent_owner, rent) = if is_asset(to) && owner != 0 && owner != seat {
        (owner, rent_amount(to, improvements[to as usize]))
    } else {
        (0, 0)
    };
    let card_penalty = if card_kind != 0 && !card_positive { card_amount } else { 0 };
    Landing {
        to,
        go_reward: go,
        square_reward: square_reward(to, p),
        card_kind,
        card_id,
        card_amount,
        card_positive,
        penalty: square_penalty(to, p).checked_add(card_penalty).expect("penalty overflow"),
        rent_owner,
        rent,
        sent_to_maintenance: false,
    }
}

impl Landing {
    /// Money paid out by the House to the seat for this landing.
    pub fn rewards(&self) -> u64 {
        let card = if self.card_kind != 0 && self.card_positive { self.card_amount } else { 0 };
        self.go_reward
            .checked_add(self.square_reward).expect("reward overflow")
            .checked_add(card).expect("reward overflow")
    }
}

/// Funding split for one settlement: the House supplies `incoming`, the seat supplies `seat_funding`.
/// `incoming` = sale proceeds + rewards. `outgoing` = purchase + fees/penalties + rent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Funding {
    pub incoming: u64,
    pub outgoing: u64,
    pub seat_funding: u64,
}

pub fn funding(sale: u64, rewards: u64, purchase: u64, fees: u64, rent: u64) -> Funding {
    let incoming = sale.checked_add(rewards).expect("incoming overflow");
    let outgoing = purchase
        .checked_add(fees).expect("outgoing overflow")
        .checked_add(rent).expect("outgoing overflow");
    Funding { incoming, outgoing, seat_funding: outgoing.saturating_sub(incoming) }
}

/// Next seat after `active` that is still in the game. Returns (seat, wrapped_past_last_seat).
pub fn next_seat(active: u8, out: &[bool]) -> (u8, bool) {
    let n = out.len() as u8;
    let mut s = active;
    let mut wrapped = false;
    for _ in 0..n {
        if s == n { s = 1; wrapped = true; } else { s += 1; }
        if !out[(s - 1) as usize] {
            return (s, wrapped);
        }
    }
    (active, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> Params {
        Params {
            go_reward: 20_000_000, square_reward: 2_000_000, fee_amount: 1_000_000,
            penalty_amount: 2_000_000, shop_fee: 5_000_000, storage_fee: 2_000_000,
            house_cost: 5_000_000, hotel_cost: 15_000_000, buyback_pct: 80,
        }
    }

    #[test]
    fn board_shape_matches_v0618() {
        assert_eq!(KINDS.len(), 40);
        for (sq, price) in ASSETS { assert_eq!(KINDS[sq as usize], 1); assert!(price > 0); }
        assert_eq!([0, 10, 20, 30].map(|i| KINDS[i]), [0, 9, 9, 9]);
        assert_eq!(KINDS.iter().filter(|k| **k == 1).count(), 22);
    }

    #[test]
    fn rent_and_values_match_v0618() {
        let p = params();
        assert_eq!(rent_amount(6, 0), 3_000_000);
        assert_eq!(rent_amount(6, 1), 4_500_000);
        assert_eq!(rent_amount(6, 2), 6_000_000);
        assert_eq!(rent_amount(6, 3), 10_500_000);
        assert_eq!(developed_value(6, 3, &p), 55_000_000);
        assert_eq!(buyback_value(6, 3, &p), 44_000_000);
        assert_eq!(buyback_value(3, 0, &p), 16_000_000);
        assert_eq!(next_improvement_cost(0, &p), 5_000_000);
        assert_eq!(next_improvement_cost(2, &p), 15_000_000);
    }

    #[test]
    fn cards_are_the_v0618_cycle() {
        assert_eq!(card(13, 5), (0, 255, 0, false));
        assert_eq!(card(2, 2), (1, 0, 2_000_000, true));
        assert_eq!(card(2, 3), (1, 1, 1_000_000, false));
        assert_eq!(card(5, 1), (2, 2, 2_000_000, true));
        assert_eq!(card(5, 2), (2, 3, 2_000_000, false));
    }

    #[test]
    fn landing_on_30_is_sent_to_maintenance_without_charges() {
        let p = params();
        let l = land(25, 30, 1, &[0; 40], &[0; 40], 0, &p);
        assert!(l.sent_to_maintenance);
        assert_eq!(l.to, MAINTENANCE_SQUARE);
        assert_eq!((l.rent, l.penalty, l.rewards()), (0, 0, 0));
    }

    #[test]
    fn rent_only_to_another_owner() {
        let p = params();
        let mut owners = [0u8; 40];
        owners[13] = 2;
        assert_eq!(land(5, 13, 1, &owners, &[0; 40], 0, &p).rent, 2_000_000);
        assert_eq!(land(5, 13, 1, &owners, &[0; 40], 0, &p).rent_owner, 2);
        assert_eq!(land(5, 13, 2, &owners, &[0; 40], 0, &p).rent, 0);
    }

    #[test]
    fn go_reward_on_wrap_and_reward_square() {
        let p = params();
        let l = land(35, 9, 1, &[0; 40], &[0; 40], 0, &p);
        assert_eq!(l.go_reward, 20_000_000);
        assert_eq!(l.square_reward, 2_000_000);
        assert_eq!(l.rewards(), 22_000_000);
        assert_eq!(land(35, 0, 1, &[0; 40], &[0; 40], 0, &p).go_reward, 20_000_000);
    }

    #[test]
    fn funding_matches_v0618_examples() {
        assert_eq!(funding(8_000_000, 0, 20_000_000, 0, 0).seat_funding, 12_000_000);
        assert_eq!(funding(16_000_000, 0, 10_000_000, 0, 0).seat_funding, 0);
        assert_eq!(funding(0, 0, 0, 2_000_000, 0).seat_funding, 2_000_000);
        assert_eq!(funding(0, 2_000_000, 0, 0, 0).seat_funding, 0);
        let f = funding(0, 20_000_000, 0, 1_000_000, 4_500_000);
        assert_eq!((f.incoming, f.outgoing, f.seat_funding), (20_000_000, 5_500_000, 0));
    }

    #[test]
    fn rotation_skips_removed_seats() {
        assert_eq!(next_seat(1, &[false; 6]), (2, false));
        assert_eq!(next_seat(6, &[false; 6]), (1, true));
        assert_eq!(next_seat(2, &[false, false, true, false]), (4, false));
        assert_eq!(next_seat(4, &[true, false, false, false]), (2, true));
    }

    #[test]
    fn scaled_go_and_rent_fee() {
        assert_eq!(unowned_assets(&[0; 40]), 22);
        let mut owners = [0u8; 40];
        owners[1] = 1; owners[3] = 2; owners[9] = 1; // square 9 is not an asset
        assert_eq!(unowned_assets(&owners), 20);
        assert_eq!(scaled_go(20_000_000, 0, 22), 20_000_000);
        assert_eq!(scaled_go(20_000_000, 0, 11), 10_000_000);
        assert_eq!(scaled_go(10_000_000, 0, 7), 3_181_818);
        assert_eq!(scaled_go(10_000_000, 5_000_000, 7), 5_000_000);
        assert_eq!(scaled_go(10_000_000, 0, 0), 0);
        assert_eq!(rent_fee(4_500_000, 1500), 675_000);
        assert_eq!(rent_fee(3_000_000, 0), 0);
        assert_eq!(rent_fee(1_000_001, 1500), 150_000);
    }

    #[test]
    fn params_validation() {
        let mut p = params();
        assert!(p.validate().is_ok());
        p.buyback_pct = 0;
        assert!(p.validate().is_err());
    }
}

// End of contracts/town_square/src/rules.rs
// Location: contracts/town_square/src/rules.rs · Created: 2026-11-16 04:06:01 UTC
