//! Ootle Game Engine | engine-tests/tests/deploy_manifests.rs
//! Version: v0.3.0
//! Executes the exact manifests that scripts/deploy_local.py submits (rendered through its
//! --render test hook) in the Tari engine harness, step by step, signed by the same roles.
//! This checks the deployment manifest syntax, variable bindings, string/integer/bool literals,
//! address!() arguments, per-seat signing for register + claim, and the start transaction.
//! v0.3.0: a pool of games, each with its economics set and every seat's cash settled to the seat start.

use std::{collections::HashMap, process::Command, str::FromStr};

use serde_json::{json, Value};
use tari_ootle_common_types::substate_type::SubstateType;
use tari_template_test_tooling::{
    crypto::RistrettoSecretKey,
    engine_types::substate::SubstateId,
    template_lib_types::{constants::TARI_TOKEN, ComponentAddress, NonFungibleAddress},
    TemplateTest,
};
use tari_transaction_manifest::{parse_manifest, ManifestValue};

struct Role { addr: ComponentAddress, proof: NonFungibleAddress, key: RistrettoSecretKey }

fn script(args: &[&str]) -> Value {
    let out = Command::new("python3").arg("../scripts/deploy_local.py").args(args).output().expect("python3");
    assert!(out.status.success(), "deploy_local.py failed: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).expect("script JSON")
}

fn hex_of(template: impl std::fmt::Display) -> String {
    let s = template.to_string();
    s.strip_prefix("template_").unwrap_or(&s).to_string()
}

fn balance(t: &mut TemplateTest, r: &Role) -> u64 {
    let amt: tari_template_test_tooling::template_lib_types::Amount =
        t.call_method(r.addr, "balance", tari_template_test_tooling::transaction::args![TARI_TOKEN], vec![r.proof.clone()]);
    u64::try_from(amt.to_u128()).unwrap()
}

fn run_deploy(players: u8, games: u8, mode: &str) {
    let mut t = TemplateTest::new("..", ["contracts/registry", "contracts/lobby", "contracts/treasury", "contracts/town_square"]);
    let mut roles: HashMap<String, Role> = HashMap::new();
    let mut names = vec!["global_house", "global_community", "global_chance"].into_iter().map(String::from).collect::<Vec<_>>();
    for g in 1..=games {
        names.extend([format!("g{g}_house"), format!("g{g}_community"), format!("g{g}_chance")]);
        names.extend((1..=players).map(|i| format!("g{g}_seat{i}")));
    }
    for n in &names {
        let (addr, proof, key) = t.create_funded_account();
        roles.insert(n.clone(), Role { addr, proof, key });
    }
    let mut accounts = serde_json::Map::new();
    for (n, r) in &roles { accounts.insert(n.clone(), json!(r.addr.to_string())); }
    accounts.insert("operator".into(), json!(roles["global_house"].addr.to_string()));
    let templates = json!({
        "registry": hex_of(t.get_template_address("OotleRegistry")),
        "lobby": hex_of(t.get_template_address("OotleLobby")),
        "treasury": hex_of(t.get_template_address("OotleTreasury")),
        "town_square": hex_of(t.get_template_address("OotleTownSquare")),
    });
    let mut components = serde_json::Map::new();
    let steps = script(&["--list-steps", "--players", &players.to_string(), "--games", &games.to_string()]);
    let ctx_path = std::env::temp_dir().join(format!("oge_ctx_{players}_{games}_{mode}.json"));
    let house_of = |g: &str| if mode == "separate" { format!("g{g}_house") } else { "global_house".to_string() };
    for step in steps.as_array().unwrap() {
        let step = step.as_str().unwrap();
        let mut cash = serde_json::Map::new();
        if let Some(rest) = step.strip_prefix("seat:settle:") {
            let (g, i) = rest.split_once(':').unwrap();
            let name = format!("g{g}_seat{i}");
            let b = balance(&mut t, &roles[&name]);
            cash.insert(name, json!(b));
        }
        let ctx = json!({"templates": templates, "components": components, "accounts": accounts,
            "resource": TARI_TOKEN.to_string(), "players": players, "games": games, "treasury_mode": mode,
            "seat_cash": cash, "seat_start": 20_000_000u64});
        std::fs::write(&ctx_path, ctx.to_string()).unwrap();
        let r = script(&["--render", step, "--context", ctx_path.to_str().unwrap()]);
        if r["skip"] == json!(true) { eprintln!("SKIP {step}"); continue; }
        let mut globals = HashMap::new();
        for (k, v) in r["variables"].as_object().unwrap() {
            globals.insert(k.clone(), ManifestValue::SubstateId(SubstateId::from_str(v.as_str().unwrap()).unwrap()));
        }
        let parsed = parse_manifest(r["manifest"].as_str().unwrap(), globals, HashMap::new(), HashMap::new())
            .unwrap_or_else(|e| panic!("{step}: manifest parse failed: {e}\n{}", r["manifest"]));
        let signer = r["signer"].as_str().unwrap();
        let parts: Vec<&str> = signer.split(':').collect();
        let (seal, proofs) = match parts[0] {
            "operator" => (&roles["global_house"], vec![roles["global_house"].proof.clone()]),
            "seat" => { let s = &roles[&format!("g{}_seat{}", parts[1], parts[2])]; (s, vec![s.proof.clone()]) }
            _ => {
                let h = &roles[&house_of(parts[1])];
                let s = &roles[&format!("g{}_seat{}", parts[1], parts[2])];
                (h, vec![h.proof.clone(), s.proof.clone()])
            }
        };
        let tx = t.transaction()
            .with_fee_instructions(parsed.fee_instructions)
            .with_instructions(parsed.instructions)
            .build_and_seal(&seal.key);
        t.execute_expect_success(tx, proofs);
        if let Some(c) = r["creates"].as_str() {
            let id = t.get_previous_output_address(SubstateType::Component);
            components.insert(c.to_string(), json!(id.to_string()));
        }
        eprintln!("OK {step}");
    }
    let treasury = ComponentAddress::from_str(components["treasury"].as_str().unwrap()).unwrap();
    let registered: Vec<(u64, ComponentAddress, ComponentAddress, ComponentAddress, ComponentAddress, bool, bool, bool)> =
        t.extract_component_value(treasury, "$.6");
    assert_eq!(registered.len(), usize::from(games));
    for g in 1..=games {
        let session = ComponentAddress::from_str(components[&format!("session{g}")].as_str().unwrap()).unwrap();
        let started: bool = t.extract_component_value(session, "$.16");
        let seats: Vec<ComponentAddress> = t.extract_component_value(session, "$.14");
        let game_id: u64 = t.extract_component_value(session, "$.0");
        let build_on_landing: bool = t.extract_component_value(session, "$.44");
        assert!(started, "game {g} should be started");
        assert_eq!((seats.len(), game_id), (usize::from(players), u64::from(g)));
        assert!(build_on_landing, "deploy turns the landing rule on");
        for i in 1..=players {
            let r = &roles[&format!("g{g}_seat{i}")];
            let (a, p) = (r.addr, r.proof.clone());
            assert_eq!(balance(&mut t, &Role { addr: a, proof: p, key: r.key.clone() }), 20_000_000, "game {g} seat {i} holds the seat start");
        }
        let reg = &registered[usize::from(g - 1)];
        if mode == "separate" {
            assert_eq!((reg.2, reg.3, reg.4), (roles[&format!("g{g}_house")].addr, roles[&format!("g{g}_community")].addr,
                roles[&format!("g{g}_chance")].addr));
        } else {
            assert_eq!((reg.2, reg.3, reg.4), (roles["global_house"].addr, roles["global_community"].addr, roles["global_chance"].addr));
            assert!(reg.5 && reg.6 && reg.7);
        }
    }
    let session = ComponentAddress::from_str(components["session1"].as_str().unwrap()).unwrap();

    // A first turn submitted as manifest text, the way the backend will build it (docs/MANIFESTS.md):
    // House pays the fee and supplies rewards, seat 1 buys Asset B (square 3) with dice 1 + 2.
    let house = &roles[&house_of("1")];
    let seat1 = &roles["g1_seat1"];
    let manifest = format!(r#"
fn fee_main() {{ let house = var!["house"]; house.pay_fee(1000000u64); }}
fn main() {{
    let house = var!["house"];
    let seat = var!["seat"];
    let game = var!["game"];
    let house_funds = house.withdraw(address!("{res}"), 0u64);
    let seat_funds = seat.withdraw(address!("{res}"), 20000000u64);
    game.play_turn(1u8, 1u8, 2u8, 1u64, 0u64, true, 65535u16, seat_funds, house_funds);
}}"#, res = TARI_TOKEN);
    let mut globals = HashMap::new();
    globals.insert("house".to_string(), ManifestValue::SubstateId(SubstateId::Component(house.addr)));
    globals.insert("seat".to_string(), ManifestValue::SubstateId(SubstateId::Component(seat1.addr)));
    globals.insert("game".to_string(), ManifestValue::SubstateId(SubstateId::Component(session)));
    let parsed = parse_manifest(&manifest, globals, HashMap::new(), HashMap::new()).expect("turn manifest parses");
    let tx = t.transaction().with_fee_instructions(parsed.fee_instructions).with_instructions(parsed.instructions)
        .build_and_seal(&house.key);
    t.execute_expect_success(tx, vec![house.proof.clone(), seat1.proof.clone()]);
    let owners: Vec<u8> = t.extract_component_value(session, "$.24");
    assert_eq!(owners[3], 1, "seat 1 should own square 3 after the manifest turn");
    eprintln!("OK turn manifest");
}

#[test]
fn deploy_script_manifests_two_games_four_seats_separate() { run_deploy(4, 2, "separate"); }

#[test]
fn deploy_script_manifests_one_game_six_seats_global() { run_deploy(6, 1, "global"); }

// End of engine-tests/tests/deploy_manifests.rs
// Location: engine-tests/tests/deploy_manifests.rs · Created: 2026-11-16 04:06:01 UTC
