//! Ootle Game Engine | engine-tests/tests/backend_plans.rs
//! Version: v0.3.0
//! Runs the backend's own plans in the Tari engine. For every action the backend simulator
//! (app/src/sim.rs) builds the manifest text exactly as the live backend submits it; the test
//! parses and executes that text sealed by the House with the seat's proof, then checks:
//!   1. the decoded Session after the transaction equals the simulator's expected state, field by field;
//!   2. every account's balance change equals the simulator's ledger (fees are disabled in the harness);
//!   3. the decoder reads the engine's state (wallet-shaped CBOR JSON) without error.
//! Covers: purchase, rent, cards, fees, GO, three doubles, Back to Maintenance, every Maintenance
//! path, sale within a turn, sell_to_house, shop fee gift, private sale accept/decline, construction,
//! and a long seeded random walk over the same actions.
//! v0.3.0: plans of several actions in one transaction (turn + build on landing), `reset_seat` with its
//! cash settlement (operator signature), and the economics (scaled GO, rent fee, build on landing,
//! doubled fee squares) set with `set_economics`, including a seeded random walk with them all on.

use std::{collections::HashMap, str::FromStr};

use ootle_app::{decode::{component, Session}, sim::{self, Action}};
use ootle_town_square::rules;
use tari_template_test_tooling::{
    crypto::RistrettoSecretKey,
    engine_types::substate::SubstateId,
    template_lib_types::{constants::TARI_TOKEN, Amount, ComponentAddress, NonFungibleAddress},
    transaction::args,
    TemplateTest,
};
use tari_transaction_manifest::{parse_manifest, ManifestValue};

const UNIT: u64 = 1_000_000;

#[derive(Clone)]
struct Acc { addr: ComponentAddress, proof: NonFungibleAddress, key: RistrettoSecretKey }

struct World { t: TemplateTest, session: ComponentAddress, house: Acc, community: Acc, chance: Acc, seats: Vec<Acc> }

/// (go_floor, go_scaled, rent_fee_bps, build_on_landing, fee_amount, penalty_amount)
type Econ = (u64, bool, u16, bool, u64, u64);

fn funded(t: &mut TemplateTest) -> Acc { let (addr, proof, key) = t.create_funded_account(); Acc { addr, proof, key } }

impl World {
    fn new(n: u8) -> Self { Self::with(n, None) }

    fn with(n: u8, econ: Option<Econ>) -> Self {
        let mut t = TemplateTest::new("..", ["contracts/registry", "contracts/lobby", "contracts/treasury", "contracts/town_square"]);
        let (gh, gc, gx) = (funded(&mut t), funded(&mut t), funded(&mut t));
        let (house, community, chance) = (funded(&mut t), funded(&mut t), funded(&mut t));
        let seats: Vec<Acc> = (0..n).map(|_| funded(&mut t)).collect();
        let op = t.owner_proof();
        let registry: ComponentAddress = t.call_function("OotleRegistry", "new", args![], vec![op.clone()]);
        let treasury: ComponentAddress = t.call_function("OotleTreasury", "new", args![TARI_TOKEN, gh.addr, gc.addr, gx.addr], vec![op.clone()]);
        let lobby: ComponentAddress = t.call_function("OotleLobby", "new", args![registry, treasury], vec![op.clone()]);
        let p: [u64; 8] = [20 * UNIT, 2 * UNIT, UNIT, 2 * UNIT, 5 * UNIT, 2 * UNIT, 5 * UNIT, 15 * UNIT];
        let type_id: u32 = t.call_method(lobby, "add_game_type", args!["Town Square Classic", p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], 80u8], vec![op.clone()]);
        let game_id: u64 = t.call_method(lobby, "create_game", args![type_id, n], vec![op.clone()]);
        let session: ComponentAddress = t.call_function("OotleTownSquare", "new", args![game_id, lobby, treasury, TARI_TOKEN, n,
            p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], 80u8], vec![op.clone()]);
        t.call_method::<()>(lobby, "attach_session", args![game_id, session], vec![op.clone()]);
        t.call_method::<()>(treasury, "register_game", args![game_id, session, house.addr, community.addr, chance.addr, false, false, false], vec![op.clone()]);
        for s in &seats { t.call_method::<()>(session, "add_seat", args![s.addr], vec![op.clone()]); }
        for (i, a) in seats.iter().enumerate() {
            let s = i as u8 + 1;
            let tx = t.transaction().call_method(registry, "register", args![format!("Seat {s}"), s])
                .call_method(lobby, "claim_seat", args![game_id, s]).build_and_seal(&a.key);
            t.execute_expect_success(tx, vec![a.proof.clone()]);
        }
        if let Some(e) = econ {
            t.call_method::<()>(session, "set_economics", args![e.0, e.1, e.2, e.3, e.4, e.5], vec![op.clone()]);
        }
        let key = t.secret_key().clone();
        let tx = t.transaction().call_method(session, "start", args![]).call_method(lobby, "mark_started", args![game_id]).build_and_seal(&key);
        t.execute_expect_success(tx, vec![op]);
        World { t, session, house, community, chance, seats }
    }

    /// The Session as the backend sees it: engine state → wallet-shaped JSON → ootle_app decoder.
    fn session(&self) -> Session {
        let value = self.t.read_only_state_store().inspect_component(self.session).unwrap().into_value();
        let doc = serde_json::json!({"substate_from_remote": {"version": 1, "substate": {"Component": {
            "header": {"template_address": "00".repeat(32)}, "body": {"state": value}}}}});
        Session::decode(&component(&doc).unwrap().2).expect("decoder reads engine state")
    }

    fn balance(&mut self, a: &Acc) -> i128 {
        let amt: Amount = self.t.call_method(a.addr, "balance", args![TARI_TOKEN], vec![a.proof.clone()]);
        i128::try_from(amt.to_u128()).unwrap()
    }

    fn balances(&mut self) -> Vec<i128> {
        let mut accs: Vec<Acc> = self.seats.clone();
        accs.extend([self.house.clone(), self.community.clone(), self.chance.clone()]);
        accs.iter().map(|a| self.balance(a)).collect()
    }

    /// Plans with the backend, executes the backend's manifest text, verifies state and ledger.
    fn run(&mut self, a: &Action) -> sim::Outcome {
        self.run_plan(std::slice::from_ref(a)).steps[0].1.clone()
    }

    /// Several actions in one transaction, exactly as the live backend submits them.
    fn run_plan(&mut self, actions: &[Action]) -> sim::Plan {
        let a = actions;
        let before = self.session();
        let plan = sim::plan(&before, 500, 500, actions).unwrap_or_else(|e| panic!("{a:?}: simulator refused: {e}"));
        let m = sim::plan_manifest(&plan, 1_000_000);
        let signer = self.seats[usize::from(plan.signer() - 1)].clone();
        let mut globals = HashMap::new();
        for (k, v) in [("house", self.house.addr), ("seat_account", signer.addr), ("game", self.session)] {
            globals.insert(k.to_string(), ManifestValue::SubstateId(SubstateId::Component(v)));
        }
        let parsed = parse_manifest(&m, globals, HashMap::new(), HashMap::new())
            .unwrap_or_else(|e| panic!("{a:?}: manifest parse failed: {e}\n{m}"));
        let b0 = self.balances();
        let tx = self.t.transaction().with_fee_instructions(parsed.fee_instructions)
            .with_instructions(parsed.instructions).build_and_seal(&self.house.key);
        let mut proofs = vec![self.house.proof.clone(), signer.proof.clone()];
        if plan.is_reset() { proofs.push(self.t.owner_proof()); }
        self.t.execute_expect_success(tx, proofs);
        let after = self.session();
        assert_eq!(serde_json::to_value(&after).unwrap(), serde_json::to_value(plan.after()).unwrap(),
            "{a:?}: on-chain state differs from the backend's expected state\nmanifest:\n{m}");
        let b1 = self.balances();
        let d: Vec<i128> = b0.iter().zip(&b1).map(|(x, y)| y - x).collect();
        let n = self.seats.len();
        let l = plan.ledger();
        let mut expected = l.seats.clone();
        expected.extend([l.house, l.community, l.chance]);
        assert_eq!(d[..n], expected[..n], "{a:?}: seat deltas");
        assert_eq!(d[n..], expected[n..], "{a:?}: house/community/chance deltas");
        plan
    }

    /// Whether the signing seat can fund the plan (the harness accounts hold real balances).
    fn affordable(&mut self, actions: &[Action]) -> bool {
        let Ok(p) = sim::plan(&self.session(), 500, 500, actions) else { return true };
        let seat = self.seats[usize::from(p.signer() - 1)].clone();
        self.balance(&seat) >= i128::from(p.seat_needed())
    }

    /// A reset that settles the seat's cash to `start`, as the backend does.
    fn reset(&mut self, seat: u8, pass_turn: bool, start: i128) -> sim::Plan {
        let cash = self.balance(&self.seats[usize::from(seat - 1)].clone());
        self.run_plan(&[Action::Reset { seat, pass_turn, settle: start - cash }])
    }

    fn turn(&mut self, seat: u8, d1: u8, d2: u8, pay: bool, buy: bool, sell: Option<u16>) -> sim::Outcome {
        self.run(&Action::Turn { seat, die1: d1, die2: d2, pay, buy, sell_square: sell })
    }
}

#[test]
fn scripted_paths_match_the_backend_plans() {
    let mut w = World::new(4);
    // Seat 1: three doubles → Maintenance (no charge).
    let o = w.turn(1, 1, 1, false, false, None); assert!(o.story.again);           // 0 → 2 Chance
    w.turn(1, 2, 2, false, false, None);                                            // 2 → 6 Asset C
    let o = w.turn(1, 3, 3, false, false, None); assert!(o.story.third_double);     // held on 10
    // Seat 2 buys Asset B (3); seat 3 pays rent there; seat 4 lands on Fee 4.
    let o = w.turn(2, 1, 2, false, true, None); assert_eq!(o.story.purchase, 20 * UNIT);
    let o = w.turn(3, 2, 1, false, false, None); assert_eq!(o.story.rent, 2 * UNIT);
    w.turn(4, 3, 1, false, false, None);
    // Seat 1 held: failed roll adds storage.
    let o = w.turn(1, 1, 2, false, false, None); assert_eq!(o.story.held_owed, 7 * UNIT);
    // Seat 2 builds on Asset B, offers it to seat 3 (declined), then to seat 4 (accepted).
    w.run(&Action::Build { seat: 2, square: 3 });
    w.run(&Action::Propose { seller: 2, buyer: 3, square: 3, price: 30 * UNIT });
    w.run(&Action::Decline { buyer: 3 });
    w.run(&Action::Propose { seller: 2, buyer: 4, square: 3, price: 31 * UNIT });
    w.run(&Action::Accept { buyer: 4 });
    // Seat 2 gifts seat 1's shop fee, then moves.
    let o = w.run(&Action::PayShopFee { payer: 2, held_seat: 1 }); assert_eq!(o.story.amount, 7 * UNIT);
    w.turn(2, 4, 5, false, false, None);                                            // 3 → 12 Utility
    // Seat 3 on 3 rolls to 30 → Back to Maintenance.
    let o = w.turn(3, 6, 6, false, false, None); assert!(!o.story.sent_to_maintenance); // 3 → 15 doubles
    let o = w.turn(3, 6, 6, false, false, None); assert!(o.story.again);               // 15 → 27 doubles
    let o = w.turn(3, 1, 2, false, false, None); assert!(o.story.sent_to_maintenance); // 27 → 30
    // Seat 4 sells Asset B to the House, then buys on landing.
    w.run(&Action::Sell { seat: 4, square: 3 });
    w.turn(4, 2, 5, false, false, None);                                            // 4 → 11 Asset E unbought
    w.turn(1, 1, 2, false, true, None);                                             // 10 → 13 buys Asset F
    w.turn(2, 6, 1, false, false, None);                                            // 12 → 19
    // Seat 3 held: pays and moves the same turn.
    let o = w.turn(3, 2, 3, true, false, None); assert_eq!(o.story.maintenance_fee, 5 * UNIT);
    w.turn(4, 1, 3, false, false, None);                                            // 11 → 15 Transit
}

#[test]
fn held_doubles_and_forced_third_payment() {
    let mut w = World::new(2);
    w.turn(1, 1, 1, false, false, None);
    w.turn(1, 1, 1, false, false, None);
    w.turn(1, 1, 1, false, false, None);                                            // seat 1 held
    w.turn(2, 1, 2, false, true, None);                                             // seat 2 buys Asset B
    let o = w.turn(1, 4, 4, false, false, None); assert!(o.story.released && !o.story.again); // released by doubles, moves 10 → 18
    w.turn(2, 6, 4, false, false, None);                                            // 3 → 13
    let o = w.turn(1, 6, 6, false, false, None);                                    // 18 → 30: doubles, but sent and turn passes
    assert!(o.story.sent_to_maintenance && !o.story.again);
    w.turn(2, 1, 2, false, false, None);
    w.turn(1, 1, 2, false, false, None); w.turn(2, 1, 2, false, false, None);
    w.turn(1, 1, 2, false, false, None); w.turn(2, 1, 2, false, false, None);
    let o = w.turn(1, 1, 2, false, false, None);                                    // third failure: forced 11.000000, moves
    assert_eq!(o.story.maintenance_fee, 11 * UNIT);
    assert_eq!(o.after.positions[0], 13);
    // Sale inside a turn: seat 2 sells Asset B while moving.
    let s = w.session();
    let sq = (0..40u16).find(|q| s.owners[usize::from(*q)] == 2).unwrap();
    let o = w.turn(2, 2, 3, false, false, Some(sq));
    assert_eq!(o.story.sale, rules::buyback_value(sq, s.improvements[usize::from(sq)], &s.params()));
}

#[test]
fn seeded_random_walk_matches_the_backend_plans() { random_walk(World::new(5), 0x5eed_0071, false); }

/// Random walk over every action. With `econ`, builds follow the landing rule (turn + build in one
/// transaction, or a build before the next move) and seats are reset or regenerated now and then.
fn random_walk(mut w: World, seed: u64, econ: bool) {
    let mut x: u64 = seed;
    let mut rnd = move |m: u64| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x % m };
    let mut counts: HashMap<&'static str, u32> = HashMap::new();
    let start = 20 * UNIT as i128;
    for _ in 0..(if econ { 360 } else { 260 }) {
        let s = w.session();
        let a = s.active;
        let owned: Vec<u16> = (0..40u16).filter(|q| s.owners[usize::from(*q)] == a).collect();
        let landed_own = s.landed_seat != 0 && s.landed_square != u16::MAX
            && s.owners[usize::from(s.landed_square)] == s.landed_seat && s.improvements[usize::from(s.landed_square)] < 3;
        if econ && !s.has_offer() && rnd(50) == 0 {
            let seat = (rnd(u64::from(s.capacity)) as u8) + 1;
            let p = w.reset(seat, seat == a && rnd(2) == 0, start);
            *counts.entry(p.first().story.action).or_default() += 1;
            continue;
        }
        let action = if s.has_offer() {
            if rnd(2) == 0 { Action::Accept { buyer: s.offer_buyer } } else { Action::Decline { buyer: s.offer_buyer } }
        } else if let (true, Some(h)) = (rnd(4) == 0, (1..=s.capacity).find(|q| *q != a && s.held[usize::from(q - 1)])) {
            Action::PayShopFee { payer: a, held_seat: h }
        } else if econ && landed_own && rnd(3) == 0 {
            Action::Build { seat: s.landed_seat, square: s.landed_square }
        } else if !econ && !owned.is_empty() && rnd(6) == 0 {
            let sq = owned[rnd(owned.len() as u64) as usize];
            if s.improvements[usize::from(sq)] < 3 { Action::Build { seat: a, square: sq } } else { Action::Sell { seat: a, square: sq } }
        } else if !owned.is_empty() && rnd(10) == 0 {
            Action::Sell { seat: a, square: owned[rnd(owned.len() as u64) as usize] }
        } else if !owned.is_empty() && rnd(10) == 0 {
            let b = (a % s.capacity) + 1;
            let sq = owned[rnd(owned.len() as u64) as usize];
            Action::Propose { seller: a, buyer: b, square: sq, price: rules::base_price(sq) + rnd(5) * UNIT }
        } else {
            let (d1, d2) = (rnd(6) as u8 + 1, rnd(6) as u8 + 1);
            let held = s.held[usize::from(a - 1)];
            let pay = held && rnd(2) == 0;
            let sell = if !owned.is_empty() && rnd(8) == 0 { Some(owned[0]) } else { None };
            let probe = sim::simulate(&s, 500, 500, &Action::Turn { seat: a, die1: d1, die2: d2, pay, buy: false, sell_square: sell });
            let sell = if probe.is_err() { None } else { sell };
            let buy = matches!(sim::simulate(&s, 500, 500, &Action::Turn { seat: a, die1: d1, die2: d2, pay, buy: false, sell_square: sell }),
                Ok(ref o) if o.story.buy_option.is_some()) && rnd(3) != 0;
            let turn = Action::Turn { seat: a, die1: d1, die2: d2, pay, buy, sell_square: sell };
            if econ {
                let o = sim::simulate(&s, 500, 500, &turn).unwrap();
                let q = o.after.landed_square;
                let both = [turn.clone(), Action::Build { seat: a, square: q }];
                if o.after.landed_seat == a && q != u16::MAX && s.owners[usize::from(q)] == a && o.after.owners[usize::from(q)] == a && o.after.improvements[usize::from(q)] < 3
                    && w.affordable(&both) {
                    let p = w.run_plan(&both);
                    *counts.entry("turn+build").or_default() += 1;
                    if p.first().story.go_reward > 0 { *counts.entry("go").or_default() += 1; }
                    continue;
                }
            }
            turn
        };
        if econ && !w.affordable(std::slice::from_ref(&action)) {
            // A seat back at the 20.000000 start cannot always fund the move: regenerate it instead.
            let o = w.reset(a, true, start);
            *counts.entry("regenerate").or_default() += 1;
            let _ = o;
            continue;
        }
        let o = w.run(&action);
        *counts.entry(o.story.action).or_default() += 1;
        if o.story.sent_to_maintenance { *counts.entry("sent_to_maintenance").or_default() += 1; }
        if o.story.rent > 0 { *counts.entry("rent").or_default() += 1; }
        if o.story.rent_fee > 0 { *counts.entry("rent_fee").or_default() += 1; }
        if o.story.card_kind != 0 { *counts.entry("card").or_default() += 1; }
        if o.story.go_reward > 0 { *counts.entry("go").or_default() += 1; }
    }
    eprintln!("random walk coverage (economics {econ}): {counts:?}");
    let mut need = vec!["play_turn", "maintenance_turn", "sell_to_house", "propose_sale", "rent", "card", "go"];
    if econ { need.extend(["turn+build", "reset_seat", "rent_fee"]); } else { need.push("build_improvement"); }
    for k in need {
        assert!(counts.get(k).copied().unwrap_or(0) > 0, "random walk did not reach {k}");
    }
}

const ECON_ON: Econ = (0, true, 1500, true, 2 * UNIT, 4 * UNIT);

/// Non-double dice for a move of 3..=11 squares.
fn dice_for(dist: u16) -> (u8, u8) {
    match dist { 3 => (1, 2), 4 => (1, 3), 5 => (1, 4), 6 => (1, 5), 7 => (1, 6), 8 => (2, 6), 9 => (3, 6), 10 => (4, 6), 11 => (5, 6),
        _ => panic!("no non-double roll for {dist}") }
}

#[test]
fn economics_scaled_go_fee_squares_and_build_on_landing() {
    let mut w = World::with(2, Some(ECON_ON));
    let s = w.session();
    assert_eq!((s.go_floor, s.go_scaled, s.rent_fee_bps, s.build_on_landing, s.fee_amount, s.penalty_amount),
        (0, true, 1500, true, 2 * UNIT, 4 * UNIT));
    let o = w.turn(1, 1, 2, false, true, None); assert_eq!(o.story.purchase, 20 * UNIT);   // seat 1: 0 → 3 buys Asset B
    let o = w.turn(2, 1, 3, false, false, None); assert_eq!(o.story.penalty, 2 * UNIT);    // seat 2: 0 → 4 Fee ×2
    w.turn(1, 5, 6, false, false, None);                                                   // 3 → 14
    let o = w.turn(2, 1, 2, false, false, None); assert_eq!(o.story.penalty, 4 * UNIT);    // 4 → 7 Penalty ×2
    w.turn(1, 5, 6, false, false, None);                                                   // 14 → 25
    w.turn(2, 1, 3, false, false, None);                                                   // 7 → 11
    w.turn(1, 5, 6, false, false, None);                                                   // 25 → 36
    w.turn(2, 1, 3, false, false, None);                                                   // 11 → 15
    // Seat 1: 36 → 3 passes GO (scaled: 20 × 21 unowned ÷ 22) and builds on its own Asset B in the same transaction.
    let (d1, d2) = dice_for(7);
    let p = w.run_plan(&[Action::Turn { seat: 1, die1: d1, die2: d2, pay: false, buy: false, sell_square: None },
                         Action::Build { seat: 1, square: 3 }]);
    assert_eq!(p.first().story.go_reward, rules::scaled_go(20 * UNIT, 0, 21));
    assert_eq!(p.first().story.go_reward, 19_090_909);
    assert_eq!(p.after().improvements[3], 1);
    assert_eq!((p.after().landed_seat, p.after().landed_square), (0, u16::MAX), "one level per landing");
    let again = sim::simulate(p.after(), 500, 500, &Action::Build { seat: 1, square: 3 }).unwrap_err();
    assert_eq!(again, "build only on the asset just landed on");
    // Seat 2 lands on 18 (unowned): it cannot build there, and seat 1 can no longer build on 3.
    let o = w.turn(2, 1, 2, false, false, None);
    assert_eq!((o.after.landed_seat, o.after.landed_square), (2, 18));
    assert!(sim::simulate(&o.after, 500, 500, &Action::Build { seat: 2, square: 18 }).is_err());
}

#[test]
fn rent_fee_routes_part_of_the_rent() {
    let mut w = World::with(2, Some((20 * UNIT, false, 1500, true, UNIT, 2 * UNIT)));
    w.turn(1, 1, 2, false, true, None);                                      // seat 1 buys Asset B (3)
    let o = w.turn(2, 1, 2, false, false, None);                             // seat 2 pays rent 2.000000
    assert_eq!((o.story.rent, o.story.rent_fee), (2 * UNIT, 300_000));
    assert_eq!(o.ledger.seats, vec![2 * UNIT as i128 - 300_000, -(2 * UNIT as i128)]);
    assert_eq!((o.ledger.house, o.ledger.community, o.ledger.chance), (270_000, 15_000, 15_000));
}

#[test]
fn reset_seat_settles_cash_and_returns_assets() {
    let start = 20 * UNIT as i128;
    let mut w = World::new(3);
    w.turn(1, 1, 2, false, true, None);                                      // seat 1 buys Asset B
    w.turn(2, 1, 3, false, false, None);
    w.turn(3, 1, 4, false, false, None);
    // Seat 1 (active, round 2) starts over without passing its turn: excess cash returns to the House.
    let p = w.reset(1, false, start);
    let a = p.after();
    assert!(p.first().ledger.seats[0] < 0);
    assert_eq!((a.owners[3], a.positions[0], a.seat_since[0], a.resets[0], a.event_kind), (0, 0, 2, 1, 6));
    assert_eq!((a.active, a.turn_count), (1, 3));
    let seat1 = w.seats[0].clone();
    assert_eq!(w.balance(&seat1), start);
    // From GO it buys Asset B again with exactly its 20.000000: cash 0.
    w.turn(1, 1, 2, false, true, None);
    assert_eq!(w.balance(&seat1), 0);
    w.turn(2, 1, 2, false, false, None);                                     // 4 → 7 Penalty
    w.turn(3, 1, 3, false, false, None);
    // A non-active seat cannot pass the turn.
    assert_eq!(sim::simulate(&w.session(), 500, 500, &Action::Reset { seat: 2, pass_turn: true, settle: 0 }).unwrap_err(),
        "pass_turn only for the active seat");
    // Regeneration: seat 1 has 0, the House tops it up to 20.000000 and the turn passes.
    let p = w.reset(1, true, start);
    assert_eq!(p.first().ledger.seats[0], start);
    assert_eq!(p.first().ledger.house, -start);
    assert_eq!((p.after().active, p.after().turn_count, p.after().resets[0]), (2, 7, 2));
    assert_eq!(w.balance(&seat1), start);
}

#[test]
fn seeded_random_walk_with_economics_on() { random_walk(World::with(5, Some(ECON_ON)), 0x0e_c0_00_11, true); }

#[test]
fn manifest_variables_are_only_those_used() {
    // A proposal needs no withdrawals: the manifest must not bind or withdraw from accounts.
    let w = World::new(2);
    let mut s = w.session();
    s.owners[3] = 1;
    let a = Action::Propose { seller: 1, buyer: 2, square: 3, price: 5 * UNIT };
    let o = sim::simulate(&s, 500, 500, &a).unwrap();
    let m = sim::manifest(&a, &s, &o, 1_000_000);
    assert!(!m.contains("withdraw"), "{m}");
    assert!(m.contains("pay_fee(1000000u64)"));
    assert!(ComponentAddress::from_str(&w.session.to_string()).is_ok());
}

// End of engine-tests/tests/backend_plans.rs
// Location: engine-tests/tests/backend_plans.rs · Created: 2026-11-16 04:06:01 UTC
