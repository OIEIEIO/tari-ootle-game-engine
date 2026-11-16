//! Ootle Game Engine | engine-tests/tests/engine.rs
//! Version: v0.1.0
//! Engine-level tests for the four templates (Registry, Lobby, Treasury, Town Square) in the
//! Tari engine test harness. Every seat, the House and the pools are separate accounts with
//! separate keys, as in the local wallet. Fees are disabled so balance deltas are exact.
//! Note: the harness engine is tari_engine 0.53.4; the live local Swarm may be a different build.

use ootle_town_square::rules;
use tari_template_test_tooling::{
    crypto::RistrettoSecretKey,
    engine_types::commit_result::RejectReason,
    template_lib_types::{constants::TARI_TOKEN, Amount, ComponentAddress, NonFungibleAddress},
    transaction::{args, builder::{MainIntent, TransactionBuilder}},
    TemplateTest,
};

const NO_SQUARE: u16 = u16::MAX;
const UNIT: u64 = 1_000_000;

#[derive(Clone)]
struct Acc {
    addr: ComponentAddress,
    proof: NonFungibleAddress,
    key: RistrettoSecretKey,
}

struct World {
    t: TemplateTest,
    registry: ComponentAddress,
    treasury: ComponentAddress,
    lobby: ComponentAddress,
    session: ComponentAddress,
    game_id: u64,
    house: Acc,
    community: Acc,
    chance: Acc,
    seats: Vec<Acc>,
}

fn params() -> rules::Params {
    rules::Params {
        go_reward: 20 * UNIT, square_reward: 2 * UNIT, fee_amount: UNIT, penalty_amount: 2 * UNIT,
        shop_fee: 5 * UNIT, storage_fee: 2 * UNIT, house_cost: 5 * UNIT, hotel_cost: 15 * UNIT, buyback_pct: 80,
    }
}

fn funded(t: &mut TemplateTest) -> Acc {
    let (addr, proof, key) = t.create_funded_account();
    Acc { addr, proof, key }
}

impl World {
    /// Deploys all four components, creates game 1 with `n` seats, registers and claims every seat
    /// when `claim`, and starts the game when `start`.
    fn new(n: u8, claim: bool, start: bool) -> Self {
        let mut t = TemplateTest::new("..", ["contracts/registry", "contracts/lobby", "contracts/treasury", "contracts/town_square"]);
        let (gh, gc, gx) = (funded(&mut t), funded(&mut t), funded(&mut t));
        let (house, community, chance) = (funded(&mut t), funded(&mut t), funded(&mut t));
        let seats: Vec<Acc> = (0..n).map(|_| funded(&mut t)).collect();
        let op = t.owner_proof();

        let registry: ComponentAddress = t.call_function("OotleRegistry", "new", args![], vec![op.clone()]);
        let treasury: ComponentAddress = t.call_function("OotleTreasury", "new",
            args![TARI_TOKEN, gh.addr, gc.addr, gx.addr], vec![op.clone()]);
        let lobby: ComponentAddress = t.call_function("OotleLobby", "new", args![registry, treasury], vec![op.clone()]);
        let p = params();
        let type_id: u32 = t.call_method(lobby, "add_game_type", args!["Town Square Classic", p.go_reward,
            p.square_reward, p.fee_amount, p.penalty_amount, p.shop_fee, p.storage_fee, p.house_cost,
            p.hotel_cost, p.buyback_pct], vec![op.clone()]);
        assert_eq!(type_id, 1);
        let game_id: u64 = t.call_method(lobby, "create_game", args![type_id, n], vec![op.clone()]);
        assert_eq!(game_id, 1);
        let session: ComponentAddress = t.call_function("OotleTownSquare", "new", args![game_id, lobby, treasury,
            TARI_TOKEN, n, p.go_reward, p.square_reward, p.fee_amount, p.penalty_amount, p.shop_fee,
            p.storage_fee, p.house_cost, p.hotel_cost, p.buyback_pct], vec![op.clone()]);
        t.call_method::<()>(lobby, "attach_session", args![game_id, session], vec![op.clone()]);
        t.call_method::<()>(treasury, "register_game", args![game_id, session, house.addr, community.addr,
            chance.addr, false, false, false], vec![op.clone()]);
        for s in &seats {
            t.call_method::<()>(session, "add_seat", args![s.addr], vec![op.clone()]);
        }
        let mut w = World { t, registry, treasury, lobby, session, game_id, house, community, chance, seats };
        if claim {
            for i in 0..n {
                w.register_and_claim(i + 1);
            }
        }
        if start {
            w.start();
        }
        w
    }

    fn seat(&self, s: u8) -> Acc { self.seats[usize::from(s - 1)].clone() }

    fn ok(&mut self, b: TransactionBuilder<MainIntent>, sealer: &RistrettoSecretKey, proofs: Vec<NonFungibleAddress>) {
        let tx = b.build_and_seal(sealer);
        self.t.execute_expect_success(tx, proofs);
    }

    fn fail(&mut self, b: TransactionBuilder<MainIntent>, sealer: &RistrettoSecretKey, proofs: Vec<NonFungibleAddress>) -> String {
        let tx = b.build_and_seal(sealer);
        let r: RejectReason = self.t.execute_expect_failure(tx, proofs);
        format!("{r:?}")
    }

    fn register_and_claim(&mut self, s: u8) {
        let a = self.seat(s);
        let b = self.t.transaction()
            .call_method(self.registry, "register", args![format!("Seat {s}"), s])
            .call_method(self.lobby, "claim_seat", args![self.game_id, s]);
        self.ok(b, &a.key, vec![a.proof.clone()]);
    }

    fn start(&mut self) {
        let op = self.t.owner_proof();
        let b = self.t.transaction()
            .call_method(self.session, "start", args![])
            .call_method(self.lobby, "mark_started", args![self.game_id]);
        let key = self.t.secret_key().clone();
        self.ok(b, &key, vec![op]);
    }

    fn balance(&mut self, a: &Acc) -> u64 {
        let amt: Amount = self.t.call_method(a.addr, "balance", args![TARI_TOKEN], vec![a.proof.clone()]);
        u64::try_from(amt.to_u128()).unwrap()
    }

    fn val<T>(&self, idx: usize) -> T
    where T: serde::de::DeserializeOwned + for<'b> tari_bor::Decode<'b, ()> {
        self.t.extract_component_value(self.session, &format!("$.{idx}"))
    }
    fn round(&self) -> u64 { self.val(17) }
    fn active(&self) -> u8 { self.val(18) }
    fn turn(&self) -> u64 { self.val(19) }
    fn positions(&self) -> Vec<u16> { self.val(21) }
    fn owners(&self) -> Vec<u8> { self.val(24) }
    fn held(&self) -> Vec<bool> { self.val(26) }
    fn owed(&self) -> Vec<u64> { self.val(28) }

    /// Builds a turn transaction: House and seat withdrawals, then the session method.
    #[allow(clippy::too_many_arguments)]
    fn turn_tx(&self, method: &str, s: u8, pay: Option<bool>, d1: u8, d2: u8, buy: bool, sell: u16,
               seat_amt: u64, house_amt: u64) -> TransactionBuilder<MainIntent> {
        let a = self.seat(s);
        let b = self.t.transaction()
            .call_method(self.house.addr, "withdraw", args![TARI_TOKEN, house_amt])
            .put_last_instruction_output_on_workspace("house")
            .call_method(a.addr, "withdraw", args![TARI_TOKEN, seat_amt])
            .put_last_instruction_output_on_workspace("seat");
        let (r, n) = (self.round(), self.turn());
        match pay {
            None => b.call_method(self.session, method, args![s, d1, d2, r, n, buy, sell, Workspace("seat"), Workspace("house")]),
            Some(p) => b.call_method(self.session, method, args![s, p, d1, d2, r, n, buy, sell, Workspace("seat"), Workspace("house")]),
        }
    }

    /// Computes the funding exactly as the backend will, using the shared rules module.
    fn funding(&self, s: u8, d1: u8, d2: u8, buy: bool, extra_fee: u64) -> rules::Funding {
        let p = params();
        let from = self.positions()[usize::from(s - 1)];
        let to = rules::destination(from, d1, d2);
        let l = rules::land(from, to, s, &self.owners(), &self.val::<Vec<u8>>(25), self.turn(), &p);
        let purchase = if buy { rules::base_price(to) } else { 0 };
        rules::funding(0, l.rewards(), purchase, l.penalty + extra_fee, l.rent)
    }

    fn play(&mut self, s: u8, d1: u8, d2: u8, buy: bool) {
        let f = self.funding(s, d1, d2, buy, 0);
        let b = self.turn_tx("play_turn", s, None, d1, d2, buy, NO_SQUARE, f.seat_funding, f.incoming);
        let (sk, hp, sp) = (self.seat(s).key, self.house.proof.clone(), self.seat(s).proof);
        self.ok(b, &sk, vec![hp, sp]);
    }
}

#[test]
fn deploy_claim_and_start() {
    let w = World::new(4, true, true);
    assert_eq!((w.round(), w.active(), w.turn()), (1, 1, 0));
    assert_eq!(w.positions(), vec![0, 0, 0, 0]);
    let games: Vec<(u64, u32, u8, Option<ComponentAddress>, u8)> =
        w.t.extract_component_value(w.lobby, "$.3");
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].4, 1, "lobby game status should be started");
}

#[test]
fn start_requires_every_seat_claimed() {
    let mut w = World::new(3, false, false);
    w.register_and_claim(1);
    w.register_and_claim(2);
    let op = w.t.owner_proof();
    let key = w.t.secret_key().clone();
    let b = w.t.transaction().call_method(w.session, "start", args![]);
    let err = w.fail(b, &key, vec![op]);
    assert!(err.contains("unclaimed seat"), "{err}");
}

#[test]
fn purchase_then_rent_with_exact_splits() {
    let mut w = World::new(4, true, true);
    let (s1, s2) = (w.seat(1), w.seat(2));
    let (b1, b2) = (w.balance(&s1), w.balance(&s2));
    let (h0, c0, x0) = (w.balance(&w.house.clone()), w.balance(&w.community.clone()), w.balance(&w.chance.clone()));
    // Seat 1 rolls 1+2 -> square 3 (Asset B, 20 TARI) and buys it.
    w.play(1, 1, 2, true);
    assert_eq!(w.owners()[3], 1);
    assert_eq!(w.balance(&s1), b1 - 20 * UNIT);
    assert_eq!(w.balance(&w.house.clone()), h0 + 18 * UNIT);
    assert_eq!(w.balance(&w.community.clone()), c0 + UNIT);
    assert_eq!(w.balance(&w.chance.clone()), x0 + UNIT);
    assert_eq!(w.active(), 2);
    // Seat 2 rolls 2+1 -> square 3, pays 10% rent (2 TARI) directly to seat 1.
    w.play(2, 2, 1, false);
    assert_eq!(w.balance(&s2), b2 - 2 * UNIT);
    assert_eq!(w.balance(&s1), b1 - 20 * UNIT + 2 * UNIT);
    assert_eq!(w.balance(&w.house.clone()), h0 + 18 * UNIT, "rent does not touch the House");
}

#[test]
fn turn_needs_the_seat_signature_and_its_turn() {
    let mut w = World::new(3, true, true);
    // Seat 2 tries to play while seat 1 is active.
    let b = w.turn_tx("play_turn", 2, None, 1, 2, false, NO_SQUARE, 0, 0);
    let (k2, hp, sp2) = (w.seat(2).key, w.house.proof.clone(), w.seat(2).proof);
    let err = w.fail(b, &k2, vec![hp, sp2]);
    assert!(err.contains("not this seat's turn"), "{err}");
    // Seat 1's turn funded entirely from the House and signed without seat 1's key:
    // the withdrawals succeed, so the session's own identity check is what rejects it.
    let (r, n) = (w.round(), w.turn());
    let h = w.house.clone();
    let b = w.t.transaction()
        .call_method(h.addr, "withdraw", args![TARI_TOKEN, 0u64])
        .put_last_instruction_output_on_workspace("house")
        .call_method(h.addr, "withdraw", args![TARI_TOKEN, 0u64])
        .put_last_instruction_output_on_workspace("seat")
        .call_method(w.session, "play_turn", args![1u8, 1u8, 2u8, r, n, false, NO_SQUARE, Workspace("seat"), Workspace("house")]);
    let err = w.fail(b, &h.key, vec![h.proof.clone()]);
    eprintln!("WRONG-SIGNER REJECT: {err}");
    assert!(err.contains("does not carry the badge"), "{err}");
}

#[test]
fn treasury_rejects_direct_distribute() {
    let mut w = World::new(2, true, true);
    let h = w.house.clone();
    let b = w.t.transaction()
        .call_method(h.addr, "withdraw", args![TARI_TOKEN, 1_000_000u64])
        .put_last_instruction_output_on_workspace("b")
        .call_method(w.treasury, "distribute", args![w.game_id, 1u8, Workspace("b"), 1_000_000u64]);
    let err = w.fail(b, &h.key, vec![h.proof.clone()]);
    eprintln!("DIRECT-DISTRIBUTE REJECT: {err}");
    assert!(err.contains("does not carry the badge"), "{err}");
}

#[test]
fn three_doubles_then_maintenance_paths() {
    let mut w = World::new(3, true, true);
    // Seat 1: doubles, doubles, doubles -> held on square 10, nothing charged.
    w.play(1, 1, 1, false); // -> 2 (Chance)
    w.play(1, 2, 2, false); // -> 6
    assert_eq!(w.active(), 1);
    let b = w.turn_tx("play_turn", 1, None, 3, 3, false, NO_SQUARE, 0, 0);
    let (k1, hp, sp1) = (w.seat(1).key, w.house.proof.clone(), w.seat(1).proof);
    w.ok(b, &k1, vec![hp.clone(), sp1.clone()]);
    assert_eq!(w.positions()[0], 10);
    assert!(w.held()[0]);
    assert_eq!(w.owed()[0], 5 * UNIT);
    assert_eq!(w.active(), 2);
    w.play(2, 1, 2, false);
    w.play(3, 1, 2, false);
    // Seat 1 held turn: failed roll adds storage (owed 7), stays held.
    let b = w.turn_tx("maintenance_turn", 1, Some(false), 1, 2, false, NO_SQUARE, 0, 0);
    w.ok(b, &k1, vec![hp.clone(), sp1.clone()]);
    assert!(w.held()[0]);
    assert_eq!(w.owed()[0], 7 * UNIT);
    assert_eq!(w.positions()[0], 10);
    // Seat 2 pays seat 1's shop fee as a gift (7 TARI, category 1 -> 90/5/5).
    let h0 = w.balance(&w.house.clone());
    let s2 = w.seat(2);
    let (r, n) = (w.round(), w.turn());
    let b = w.t.transaction()
        .call_method(s2.addr, "withdraw", args![TARI_TOKEN, 7 * UNIT])
        .put_last_instruction_output_on_workspace("p")
        .call_method(w.session, "pay_shop_fee", args![2u8, 1u8, r, n, Workspace("p")]);
    w.ok(b, &s2.key, vec![s2.proof.clone()]);
    assert!(!w.held()[0]);
    assert_eq!(w.owed()[0], 0);
    assert_eq!(w.balance(&w.house.clone()), h0 + 7 * UNIT * 90 / 100);
}

#[test]
fn held_seat_pays_then_moves() {
    let mut w = World::new(2, true, true);
    // Seat 1 lands on 30 in steps: 6+6 -> 12 (doubles), 6+5 -> 23, then seat 2, then 4+3 -> 30.
    w.play(1, 6, 6, false);
    w.play(1, 6, 5, false);
    w.play(2, 1, 2, false);
    w.play(1, 4, 3, false);
    assert_eq!(w.positions()[0], 10);
    assert!(w.held()[0]);
    w.play(2, 1, 2, false);
    // Pay 5 TARI shop fee, then move 10 -> 13 (Asset F, unowned), no purchase.
    let f = w.funding(1, 1, 2, false, 5 * UNIT);
    assert_eq!(f.seat_funding, 5 * UNIT);
    let b = w.turn_tx("maintenance_turn", 1, Some(true), 1, 2, false, NO_SQUARE, f.seat_funding, f.incoming);
    let (k1, hp, sp1) = (w.seat(1).key, w.house.proof.clone(), w.seat(1).proof);
    w.ok(b, &k1, vec![hp, sp1]);
    assert!(!w.held()[0]);
    assert_eq!(w.positions()[0], 13);
}

#[test]
fn third_failed_roll_forces_payment_and_move() {
    let mut w = World::new(2, true, true);
    w.play(1, 1, 1, false);
    w.play(1, 2, 2, false);
    let (k1, hp, sp1) = (w.seat(1).key, w.house.proof.clone(), w.seat(1).proof);
    let b = w.turn_tx("play_turn", 1, None, 3, 3, false, NO_SQUARE, 0, 0);
    w.ok(b, &k1, vec![hp.clone(), sp1.clone()]);
    for _ in 0..2 {
        w.play(2, 1, 2, false);
        let b = w.turn_tx("maintenance_turn", 1, Some(false), 1, 2, false, NO_SQUARE, 0, 0);
        w.ok(b, &k1, vec![hp.clone(), sp1.clone()]);
    }
    assert_eq!(w.owed()[0], 9 * UNIT);
    w.play(2, 1, 2, false);
    // Third failure: owed 9 + 2 storage = 11 forced, then move 10 -> 13.
    let f = w.funding(1, 1, 2, false, 11 * UNIT);
    let b = w.turn_tx("maintenance_turn", 1, Some(false), 1, 2, false, NO_SQUARE, f.seat_funding, f.incoming);
    w.ok(b, &k1, vec![hp, sp1]);
    assert!(!w.held()[0]);
    assert_eq!(w.positions()[0], 13);
}

#[test]
fn seats_cannot_be_released_after_start() {
    let mut w = World::new(2, true, true);
    let a = w.seat(1);
    let b = w.t.transaction().call_method(w.lobby, "release_seat", args![w.game_id, 1u8]);
    let err = w.fail(b, &a.key, vec![a.proof.clone()]);
    assert!(err.contains("only be released while the game is open"), "{err}");
}

#[test]
fn sell_to_house_pays_buyback_value() {
    let mut w = World::new(2, true, true);
    w.play(1, 1, 2, true); // seat 1 buys Asset B (square 3, 20 TARI)
    w.play(2, 1, 2, false);
    let s1 = w.seat(1);
    let b1 = w.balance(&s1);
    let (r, n) = (w.round(), w.turn());
    let h = w.house.clone();
    let b = w.t.transaction()
        .call_method(h.addr, "withdraw", args![TARI_TOKEN, 16 * UNIT])
        .put_last_instruction_output_on_workspace("h")
        .call_method(w.session, "sell_to_house", args![1u8, 3u16, r, n, Workspace("h")]);
    w.ok(b, &s1.key, vec![h.proof.clone(), s1.proof.clone()]);
    assert_eq!(w.owners()[3], 0);
    assert_eq!(w.balance(&s1), b1 + 16 * UNIT);
}

#[test]
fn private_sale_between_seats() {
    let mut w = World::new(3, true, true);
    w.play(1, 1, 2, true); // seat 1 buys Asset B (square 3)
    w.play(2, 1, 2, false);
    w.play(3, 1, 2, false);
    let (s1, s3) = (w.seat(1), w.seat(3));
    let (r, n) = (w.round(), w.turn());
    let b = w.t.transaction().call_method(w.session, "propose_sale", args![1u8, 3u8, 3u16, 25 * UNIT, r, n]);
    w.ok(b, &s1.key, vec![s1.proof.clone()]);
    let nonce: u64 = w.val(40);
    let (b1, b3) = (w.balance(&s1), w.balance(&s3));
    let b = w.t.transaction()
        .call_method(s3.addr, "withdraw", args![TARI_TOKEN, 25 * UNIT])
        .put_last_instruction_output_on_workspace("p")
        .call_method(w.session, "accept_sale", args![3u8, nonce, r, n, Workspace("p")]);
    w.ok(b, &s3.key, vec![s3.proof.clone()]);
    assert_eq!(w.owners()[3], 3);
    assert_eq!(w.balance(&s1), b1 + 25 * UNIT);
    assert_eq!(w.balance(&s3), b3 - 25 * UNIT);
}

#[test]
fn build_improvement_raises_rent() {
    let mut w = World::new(2, true, true);
    w.play(1, 1, 2, true); // seat 1 buys Asset B (square 3)
    w.play(2, 1, 4, false);
    let s1 = w.seat(1);
    let (r, n) = (w.round(), w.turn());
    let b = w.t.transaction()
        .call_method(s1.addr, "withdraw", args![TARI_TOKEN, 5 * UNIT])
        .put_last_instruction_output_on_workspace("p")
        .call_method(w.session, "build_improvement", args![1u8, 3u16, r, n, 0u8, Workspace("p")]);
    w.ok(b, &s1.key, vec![s1.proof.clone()]);
    let imp: Vec<u8> = w.val(25);
    assert_eq!(imp[3], 1);
    assert_eq!(rules::rent_amount(3, 1), 3 * UNIT);
}

// End of engine-tests/tests/engine.rs
// Location: engine-tests/tests/engine.rs · Created: 2026-11-15 21:49:02 UTC

/// Writes wallet-shaped `substates.get` responses for all four components after a short game
/// (purchase, rent, construction, a seat held in Maintenance, a pending offer) to
/// app/tests/fixtures/. The backend decoder tests run against these files, so the decoder is
/// checked against the exact CBOR-JSON the engine produces (tari_bor's "@cbor" JSON form).
#[test]
fn dump_wallet_fixtures() {
    let mut w = World::new(4, true, true);
    w.play(1, 1, 2, true);            // seat 1 buys Asset B (square 3)
    w.play(2, 2, 1, false);           // seat 2 pays rent to seat 1
    w.play(3, 1, 1, false);           // seat 3 doubles
    w.play(3, 2, 2, false);           // seat 3 doubles
    let b = w.turn_tx("play_turn", 3, None, 3, 3, false, NO_SQUARE, 0, 0); // third double -> Maintenance
    let (k3, hp, sp3) = (w.seat(3).key, w.house.proof.clone(), w.seat(3).proof);
    w.ok(b, &k3, vec![hp, sp3]);
    w.play(4, 1, 3, false);           // seat 4 lands on Fee (square 4)
    let s1 = w.seat(1);
    let (r, n) = (w.round(), w.turn());
    let b = w.t.transaction()
        .call_method(s1.addr, "withdraw", args![TARI_TOKEN, 5 * UNIT])
        .put_last_instruction_output_on_workspace("p")
        .call_method(w.session, "build_improvement", args![1u8, 3u16, r, n, 0u8, Workspace("p")])
        .call_method(w.session, "propose_sale", args![1u8, 2u8, 3u16, 25 * UNIT, r, n]);
    w.ok(b, &s1.key, vec![s1.proof.clone()]);

    let dir = std::path::Path::new("../app/tests/fixtures");
    std::fs::create_dir_all(dir).unwrap();
    let hex = |name: &str| { let s = w.t.get_template_address(name).to_string(); s.strip_prefix("template_").unwrap_or(&s).to_string() };
    let dump = |file: &str, addr: ComponentAddress, template: String| {
        let value = w.t.read_only_state_store().inspect_component(addr).unwrap().into_value();
        let doc = serde_json::json!({"substate_from_remote": {"version": 7, "substate": {"Component": {
            "header": {"template_address": template}, "body": {"state": value}}}}});
        std::fs::write(dir.join(file), serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    };
    dump("session.json", w.session, hex("OotleTownSquare"));
    dump("lobby.json", w.lobby, hex("OotleLobby"));
    dump("treasury.json", w.treasury, hex("OotleTreasury"));
    dump("registry.json", w.registry, hex("OotleRegistry"));
    let seats: Vec<_> = (1..=4u8).map(|i| serde_json::json!({"seat": i, "account_name": format!("OGE-test-seat{i}"),
        "component": w.seat(i).addr.to_string(), "key_index": 18 + u32::from(i)})).collect();
    let pointers = serde_json::json!({
        "network": "Local Ootle Swarm", "engine_version": "0.1.0", "resource": TARI_TOKEN.to_string(),
        "registry": {"component": w.registry.to_string(), "template": hex("OotleRegistry")},
        "treasury": {"component": w.treasury.to_string(), "template": hex("OotleTreasury"),
            "global_house": "component_00", "global_community": "component_00", "global_chance": "component_00"},
        "lobby": {"component": w.lobby.to_string(), "template": hex("OotleLobby")},
        "game_types": [{"id": 1, "name": "Town Square Classic", "template": hex("OotleTownSquare"), "params": {
            "go_reward": 20000000u64, "square_reward": 2000000u64, "fee_amount": 1000000u64, "penalty_amount": 2000000u64,
            "shop_fee": 5000000u64, "storage_fee": 2000000u64, "house_cost": 5000000u64, "hotel_cost": 15000000u64, "buyback_pct": 80}}],
        "games": [{"id": 1, "type_id": 1, "capacity": 4,
            "session": {"component": w.session.to_string(), "template": hex("OotleTownSquare")},
            "treasury_mode": "separate", "house": w.house.addr.to_string(), "community": w.community.addr.to_string(),
            "chance": w.chance.addr.to_string(), "house_account_name": "OGE-test-house", "seats": seats}]
    });
    std::fs::write(dir.join("pointers.json"), serde_json::to_string_pretty(&pointers).unwrap()).unwrap();
}
