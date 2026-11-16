//! Ootle Game Engine | app/src/decode.rs
//! Version: v0.4.0
//! Decodes the four components from the wallet's `substates.get` JSON. Component state is a
//! positional CBOR array rendered with tari_bor's JSON form ("@cbor" tag/bytes sentinels).
//! Field order: docs/STATE_LAYOUT.md. Any other field count is refused, never guessed.
//! Tests run against fixtures dumped from the Tari engine (app/tests/fixtures).
//! v0.3.0: Town Square has 49 fields (41..48 appended: economics, landed asset, seat stints).
//! v0.4.0: unchanged layout; the session fixture test now expects the landed seat and square the
//! engine-dumped fixture actually holds (seat 4 on square 4).

use serde::Serialize;
use serde_json::Value;

pub const TAG_COMPONENT: u64 = 128;
pub const TAG_RESOURCE: u64 = 131;

/// (template hex, substate version, state array) from a `substates.get` result.
pub fn component(resp: &Value) -> Result<(String, Value, Vec<Value>), String> {
    let remote = resp.get("substate_from_remote").filter(|v| !v.is_null())
        .ok_or("wallet returned no remote substate for this component")?;
    let c = remote.pointer("/substate/Component").or_else(|| remote.pointer("/Component"))
        .or_else(|| remote.pointer("/substate/substate/Component"))
        .ok_or("substate is not a component")?;
    let template = c.pointer("/header/template_address").and_then(Value::as_str)
        .ok_or("component header has no template address")?.to_owned();
    let state = c.pointer("/body/state").and_then(Value::as_array).ok_or("component state is not an array")?.clone();
    Ok((template, remote.get("version").cloned().unwrap_or(Value::Null), state))
}

fn untag(v: &Value) -> (Option<u64>, &Value) {
    if v.get("@cbor").and_then(Value::as_str) == Some("tag") {
        return (v.get("tag").and_then(Value::as_u64), v.get("value").unwrap_or(&Value::Null));
    }
    (v.get("tag").and_then(Value::as_u64), v.get("value").unwrap_or(v))
}

fn bytes_hex(v: &Value) -> Result<String, String> {
    let h = v.get("hex").and_then(Value::as_str).ok_or("expected CBOR bytes")?;
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("expected 32 bytes".into()); }
    Ok(h.to_ascii_lowercase())
}

/// Tagged 32-byte address → "<prefix><hex>".
pub fn address(v: &Value, tag: u64, prefix: &str) -> Result<String, String> {
    let (t, inner) = untag(v);
    if t != Some(tag) { return Err(format!("expected CBOR tag {tag}")); }
    Ok(format!("{prefix}{}", bytes_hex(inner)?))
}

pub fn comp(v: &Value) -> Result<String, String> { address(v, TAG_COMPONENT, "component_") }

/// Option<ComponentAddress>: null is None.
pub fn opt_comp(v: &Value) -> Result<Option<String>, String> {
    if v.is_null() { Ok(None) } else { comp(v).map(Some) }
}

/// Public-key identity badge (NonFungibleAddress, tag 130) → public key hex.
pub fn identity(v: &Value) -> Result<String, String> {
    let (t, inner) = untag(v);
    if t != Some(130) { return Err("expected identity badge (CBOR tag 130)".into()); }
    let id = inner.get(1).and_then(|x| x.get(1)).and_then(|x| x.get(0)).ok_or("identity badge has no id bytes")?;
    bytes_hex(id)
}

pub fn u(v: &Value) -> Result<u64, String> { v.as_u64().ok_or_else(|| format!("expected unsigned integer, got {v}")) }
fn u8_(v: &Value) -> Result<u8, String> { u8::try_from(u(v)?).map_err(|e| e.to_string()) }
fn u16_(v: &Value) -> Result<u16, String> { u16::try_from(u(v)?).map_err(|e| e.to_string()) }
fn u32_(v: &Value) -> Result<u32, String> { u32::try_from(u(v)?).map_err(|e| e.to_string()) }
fn b(v: &Value) -> Result<bool, String> { v.as_bool().ok_or_else(|| format!("expected bool, got {v}")) }
fn arr(v: &Value) -> Result<&Vec<Value>, String> { v.as_array().ok_or_else(|| format!("expected array, got {v}")) }
fn vec_of<T>(v: &Value, f: fn(&Value) -> Result<T, String>) -> Result<Vec<T>, String> { arr(v)?.iter().map(f).collect() }

#[derive(Clone, Debug, Serialize)]
pub struct Session {
    pub game_id: u64,
    pub lobby: String,
    pub treasury: String,
    pub resource: String,
    pub go_reward: u64,
    pub square_reward: u64,
    pub fee_amount: u64,
    pub penalty_amount: u64,
    pub shop_fee: u64,
    pub storage_fee: u64,
    pub house_cost: u64,
    pub hotel_cost: u64,
    pub buyback_pct: u8,
    pub capacity: u8,
    pub seats: Vec<String>,
    pub out: Vec<bool>,
    pub started: bool,
    pub round: u64,
    pub active: u8,
    pub turn_count: u64,
    pub doubles_streak: u8,
    pub positions: Vec<u16>,
    pub last_die1: u8,
    pub last_die2: u8,
    pub owners: Vec<u8>,
    pub improvements: Vec<u8>,
    pub held: Vec<bool>,
    pub held_attempts: Vec<u8>,
    pub held_owed: Vec<u64>,
    pub event_kind: u8,
    pub event_card: u8,
    pub event_amount: u64,
    pub event_positive: bool,
    pub event_seat: u8,
    pub event_square: u16,
    pub event_recipient: u8,
    pub offer_square: u16,
    pub offer_price: u64,
    pub offer_seller: u8,
    pub offer_buyer: u8,
    pub offer_nonce: u64,
    pub go_floor: u64,
    pub go_scaled: bool,
    pub rent_fee_bps: u16,
    pub build_on_landing: bool,
    pub landed_seat: u8,
    pub landed_square: u16,
    pub seat_since: Vec<u64>,
    pub resets: Vec<u32>,
}

pub const SESSION_FIELDS: usize = 49;

impl Session {
    pub fn decode(s: &[Value]) -> Result<Self, String> {
        if s.len() != SESSION_FIELDS {
            return Err(format!("unknown Town Square state layout: {} fields (expected {SESSION_FIELDS})", s.len()));
        }
        let x = Session {
            game_id: u(&s[0])?, lobby: comp(&s[1])?, treasury: comp(&s[2])?,
            resource: address(&s[3], TAG_RESOURCE, "resource_")?,
            go_reward: u(&s[4])?, square_reward: u(&s[5])?, fee_amount: u(&s[6])?, penalty_amount: u(&s[7])?,
            shop_fee: u(&s[8])?, storage_fee: u(&s[9])?, house_cost: u(&s[10])?, hotel_cost: u(&s[11])?,
            buyback_pct: u8_(&s[12])?, capacity: u8_(&s[13])?,
            seats: vec_of(&s[14], comp)?, out: vec_of(&s[15], b)?, started: b(&s[16])?,
            round: u(&s[17])?, active: u8_(&s[18])?, turn_count: u(&s[19])?, doubles_streak: u8_(&s[20])?,
            positions: vec_of(&s[21], u16_)?, last_die1: u8_(&s[22])?, last_die2: u8_(&s[23])?,
            owners: vec_of(&s[24], u8_)?, improvements: vec_of(&s[25], u8_)?,
            held: vec_of(&s[26], b)?, held_attempts: vec_of(&s[27], u8_)?, held_owed: vec_of(&s[28], u)?,
            event_kind: u8_(&s[29])?, event_card: u8_(&s[30])?, event_amount: u(&s[31])?, event_positive: b(&s[32])?,
            event_seat: u8_(&s[33])?, event_square: u16_(&s[34])?, event_recipient: u8_(&s[35])?,
            offer_square: u16_(&s[36])?, offer_price: u(&s[37])?, offer_seller: u8_(&s[38])?,
            offer_buyer: u8_(&s[39])?, offer_nonce: u(&s[40])?,
            go_floor: u(&s[41])?, go_scaled: b(&s[42])?, rent_fee_bps: u16_(&s[43])?, build_on_landing: b(&s[44])?,
            landed_seat: u8_(&s[45])?, landed_square: u16_(&s[46])?,
            seat_since: vec_of(&s[47], u)?, resets: vec_of(&s[48], u32_)?,
        };
        x.check()?;
        Ok(x)
    }

    fn check(&self) -> Result<(), String> {
        let n = self.seats.len();
        if [self.out.len(), self.positions.len(), self.held.len(), self.held_attempts.len(), self.held_owed.len(),
            self.seat_since.len(), self.resets.len()]
            .iter().any(|l| *l != n) {
            return Err("per-seat vectors disagree in length".into());
        }
        if self.owners.len() != 40 || self.improvements.len() != 40 { return Err("board vectors are not 40 long".into()); }
        if self.positions.iter().any(|p| *p >= 40) { return Err("position outside the board".into()); }
        if self.owners.iter().any(|o| usize::from(*o) > n) { return Err("owner is not a seat".into()); }
        if self.improvements.iter().any(|l| *l > 3) { return Err("invalid development level".into()); }
        if self.started && (self.active == 0 || usize::from(self.active) > n) { return Err("invalid active seat".into()); }
        if usize::from(self.landed_seat) > n { return Err("landed seat is not a seat".into()); }
        Ok(())
    }

    pub fn params(&self) -> ootle_town_square::rules::Params {
        ootle_town_square::rules::Params {
            go_reward: self.go_reward, square_reward: self.square_reward, fee_amount: self.fee_amount,
            penalty_amount: self.penalty_amount, shop_fee: self.shop_fee, storage_fee: self.storage_fee,
            house_cost: self.house_cost, hotel_cost: self.hotel_cost, buyback_pct: self.buyback_pct,
        }
    }

    pub fn has_offer(&self) -> bool { self.offer_square != u16::MAX }

    /// Parameters for a landing: GO scaled from the board when enabled (contract `landing_params`).
    pub fn landing_params(&self) -> ootle_town_square::rules::Params {
        let mut p = self.params();
        if self.go_scaled {
            p.go_reward = ootle_town_square::rules::scaled_go(self.go_reward, self.go_floor,
                ootle_town_square::rules::unowned_assets(&self.owners));
        }
        p
    }

    /// Net worth of a seat: cash plus its assets at buyback value.
    pub fn buyback_total(&self, seat: u8) -> u64 {
        let p = self.params();
        (0..40u16).filter(|q| self.owners[usize::from(*q)] == seat)
            .map(|q| ootle_town_square::rules::buyback_value(q, self.improvements[usize::from(q)], &p)).sum()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LobbyType { pub id: u32, pub name: String, pub template: Option<String>, pub params: Vec<u64> }
#[derive(Clone, Debug, Serialize)]
pub struct LobbyGame { pub id: u64, pub type_id: u32, pub capacity: u8, pub session: Option<String>, pub status: u8 }
#[derive(Clone, Debug, Serialize)]
pub struct LobbyClaim { pub game_id: u64, pub seat: u8, pub identity: String }
#[derive(Clone, Debug, Serialize)]
pub struct Lobby { pub registry: String, pub treasury: String, pub types: Vec<LobbyType>, pub games: Vec<LobbyGame>, pub claims: Vec<LobbyClaim> }

impl Lobby {
    pub fn decode(s: &[Value]) -> Result<Self, String> {
        if s.len() != 5 { return Err(format!("unknown Lobby layout: {} fields (expected 5)", s.len())); }
        let types = arr(&s[2])?.iter().map(|t| {
            let t = arr(t)?;
            let template = if t[2].is_null() { None } else { Some(bytes_hex(&t[2])?) };
            Ok(LobbyType { id: u32::try_from(u(&t[0])?).map_err(|e| e.to_string())?,
                name: t[1].as_str().ok_or("type name")?.to_owned(), template,
                params: arr(&t[3])?.iter().map(u).collect::<Result<_, _>>()? })
        }).collect::<Result<Vec<_>, String>>()?;
        let games = arr(&s[3])?.iter().map(|g| {
            let g = arr(g)?;
            Ok(LobbyGame { id: u(&g[0])?, type_id: u32::try_from(u(&g[1])?).map_err(|e| e.to_string())?,
                capacity: u8_(&g[2])?, session: opt_comp(&g[3])?, status: u8_(&g[4])? })
        }).collect::<Result<Vec<_>, String>>()?;
        let claims = arr(&s[4])?.iter().map(|c| {
            let c = arr(c)?;
            Ok(LobbyClaim { game_id: u(&c[0])?, seat: u8_(&c[1])?, identity: identity(&c[2])? })
        }).collect::<Result<Vec<_>, String>>()?;
        Ok(Lobby { registry: comp(&s[0])?, treasury: comp(&s[1])?, types, games, claims })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TreasuryGame {
    pub id: u64, pub session: String, pub house: String, pub community: String, pub chance: String,
    pub house_global: bool, pub community_global: bool, pub chance_global: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Treasury {
    pub resource: String, pub global_house: String, pub global_community: String, pub global_chance: String,
    pub community_bps: u16, pub chance_bps: u16, pub games: Vec<TreasuryGame>,
}

impl Treasury {
    pub fn decode(s: &[Value]) -> Result<Self, String> {
        if s.len() != 7 { return Err(format!("unknown Treasury layout: {} fields (expected 7)", s.len())); }
        let games = arr(&s[6])?.iter().map(|g| {
            let g = arr(g)?;
            Ok(TreasuryGame { id: u(&g[0])?, session: comp(&g[1])?, house: comp(&g[2])?, community: comp(&g[3])?,
                chance: comp(&g[4])?, house_global: b(&g[5])?, community_global: b(&g[6])?, chance_global: b(&g[7])? })
        }).collect::<Result<Vec<_>, String>>()?;
        Ok(Treasury { resource: address(&s[0], TAG_RESOURCE, "resource_")?, global_house: comp(&s[1])?,
            global_community: comp(&s[2])?, global_chance: comp(&s[3])?,
            community_bps: u16_(&s[4])?, chance_bps: u16_(&s[5])?, games })
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Profile { pub identity: String, pub name: String, pub icon: u8 }
#[derive(Clone, Debug, Serialize)]
pub struct Registry { pub profiles: Vec<Profile> }

impl Registry {
    pub fn decode(s: &[Value]) -> Result<Self, String> {
        if s.len() != 1 { return Err(format!("unknown Registry layout: {} fields (expected 1)", s.len())); }
        let profiles = arr(&s[0])?.iter().map(|p| {
            let p = arr(p)?;
            Ok(Profile { identity: identity(&p[0])?, name: p[1].as_str().ok_or("profile name")?.to_owned(), icon: u8_(&p[2])? })
        }).collect::<Result<Vec<_>, String>>()?;
        Ok(Registry { profiles })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Vec<Value> {
        let text = std::fs::read_to_string(format!("tests/fixtures/{name}.json")).unwrap();
        component(&serde_json::from_str(&text).unwrap()).unwrap().2
    }

    #[test]
    fn session_fixture_decodes() {
        let s = Session::decode(&fixture("session")).unwrap();
        assert_eq!((s.game_id, s.capacity, s.seats.len(), s.started), (1, 4, 4, true));
        assert_eq!((s.round, s.active, s.turn_count), (2, 1, 6));
        assert_eq!(s.positions, vec![3, 3, 10, 4]);
        assert_eq!(s.owners[3], 1);
        assert_eq!(s.improvements[3], 1);
        assert_eq!(s.held, vec![false, false, true, false]);
        assert_eq!(s.held_owed[2], 5_000_000);
        assert!(s.has_offer());
        assert_eq!((s.offer_square, s.offer_price, s.offer_seller, s.offer_buyer, s.offer_nonce), (3, 25_000_000, 1, 2, 1));
        assert_eq!(s.resource, format!("resource_{}", "01".repeat(32)));
        assert_eq!(s.params().buyback_pct, 80);
        assert_eq!((s.go_floor, s.go_scaled, s.rent_fee_bps, s.build_on_landing), (20_000_000, false, 0, false));
        assert_eq!((s.landed_seat, s.landed_square), (4, 4));
        assert_eq!((s.seat_since.clone(), s.resets.clone()), (vec![1; 4], vec![0; 4]));
    }

    #[test]
    fn lobby_treasury_registry_fixtures_decode() {
        let l = Lobby::decode(&fixture("lobby")).unwrap();
        assert_eq!(l.types.len(), 1);
        assert_eq!(l.types[0].name, "Town Square Classic");
        assert!(l.types[0].template.is_some());
        assert_eq!(l.types[0].params, vec![20000000, 2000000, 1000000, 2000000, 5000000, 2000000, 5000000, 15000000, 80]);
        assert_eq!((l.games[0].id, l.games[0].capacity, l.games[0].status), (1, 4, 1));
        assert!(l.games[0].session.is_some());
        assert_eq!(l.claims.len(), 4);
        let t = Treasury::decode(&fixture("treasury")).unwrap();
        assert_eq!((t.community_bps, t.chance_bps, t.games.len()), (500, 500, 1));
        assert!(!t.games[0].house_global);
        let r = Registry::decode(&fixture("registry")).unwrap();
        assert_eq!(r.profiles.len(), 4);
        assert_eq!(r.profiles[1].name, "Seat 2");
        // Every claim's identity is a registered profile.
        for c in &l.claims { assert!(r.profiles.iter().any(|p| p.identity == c.identity)); }
    }

    #[test]
    fn wrong_layout_is_refused() {
        let mut s = fixture("session");
        s.pop();
        assert!(Session::decode(&s).unwrap_err().contains("expected 49"));
    }
}

// End of app/src/decode.rs
// Location: app/src/decode.rs · Created: 2026-11-16 04:06:01 UTC
