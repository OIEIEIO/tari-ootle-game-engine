//! Ootle Asset Game | src/contract_monitor.rs
//! Reads the deployed controller's current component substate from wallet JSON-RPC.
//! This is read-only and does not execute manifests, pay fees, or change blockchain state.
//! Only values actually decoded from a matching live component are reported as verified.

use crate::config::Config;
use serde_json::{json, Value};
use std::time::Duration;

/// Current controller state as returned by the wallet's remote substate query.
/// The controller's CBOR state is a positional array defined by its published Rust template.
/// A layout change is treated as an error, not silently interpreted as a valid reading.
pub fn decode_controller_state(response: &Value, component: &str, template: &str, config: &Config) -> Result<Value, String> {
    let remote = response.get("substate_from_remote")
        .filter(|v| !v.is_null())
        .ok_or("wallet returned no remote component state (only a local record or no remote substate)")?;
    let component_node = remote.pointer("/substate/Component")
        .or_else(|| remote.pointer("/Component"))
        .or_else(|| remote.pointer("/substate/substate/Component"))
        .ok_or("remote substate did not contain a Component body")?;
    let actual_template = component_node.pointer("/header/template_address")
        .and_then(Value::as_str).ok_or("component header missing template address")?;
    if actual_template != template { return Err(format!("component template mismatch: {actual_template}")); }
    let state = component_node.pointer("/body/state")
        .and_then(Value::as_array).ok_or("component body.state is not an array")?;
    if state.len() != 11 { return Err(format!("unknown controller state layout: expected 11 fields, got {}", state.len())); }

    fn tagged_hex(item: &Value, expected_tag: u64) -> Result<String, String> {
        let tag = item.get("tag").and_then(Value::as_u64).ok_or("state address missing CBOR tag")?;
        if tag != expected_tag { return Err(format!("unexpected CBOR address tag {tag}, expected {expected_tag}")); }
        let hex = item.pointer("/value/hex").and_then(Value::as_str)
            .ok_or("state address is missing hex bytes")?;
        if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("state address is not a 32-byte hex value".into());
        }
        Ok(hex.to_ascii_lowercase())
    }
    let house = tagged_hex(&state[0], 128)?;
    let community = tagged_hex(&state[1], 128)?;
    let chance = tagged_hex(&state[2], 128)?;
    let resource = tagged_hex(&state[3], 131)?;
    for (id, expected) in [("house", &house), ("community", &community), ("chance", &chance)] {
        let configured = config.accounts.accounts.iter().find(|a| a.id == id)
            .ok_or_else(|| format!("missing configured treasury account {id}"))?;
        if configured.component_address.strip_prefix("component_") != Some(expected.as_str()) {
            return Err(format!("on-chain {id} address differs from configured treasury"));
        }
    }
    if config.accounts.resource_address.strip_prefix("resource_") != Some(resource.as_str()) {
        return Err("on-chain resource differs from configured tTARI resource".into());
    }
    let mut numbers = Vec::new();
    for field in &state[4..] {
        numbers.push(field.as_u64().ok_or("expected integer in controller allocation/counter field")?);
    }
    let categories = [
        ("Asset sale", 0u8, numbers[0], numbers[1]),
        ("Game fee", 1u8, numbers[2], numbers[3]),
        ("Sponsorship", 2u8, numbers[4], numbers[5]),
    ];
    let mut allocations = Vec::new();
    for (name, category, community_bps, chance_bps) in categories {
        if community_bps + chance_bps > 10_000 { return Err(format!("invalid on-chain allocation: {name}")); }
        allocations.push(json!({
            "category":category, "name":name,
            "house_bps":10_000-community_bps-chance_bps,
            "community_bps":community_bps, "chance_bps":chance_bps,
        }));
    }
    Ok(json!({
        "verified":true,
        "source":"wallet substates.get / remote component state",
        "component_address":component,
        "template_address":actual_template,
        "substate_version":remote.get("version").cloned().unwrap_or(Value::Null),
        "distribution_count":numbers[6],
        "allocations":allocations,
        "treasuries":{"house":format!("component_{house}"),"community":format!("component_{community}"),"chance":format!("component_{chance}")},
        "resource_address":format!("resource_{resource}"),
        "upgrade_rule":component_node.pointer("/header/upgrade_rule").cloned().unwrap_or(Value::Null)
    }))
}

pub async fn fetch_controller(config: &Config, component: &str, template: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20))
        .build().map_err(|e|e.to_string())?;
    let url = &config.network.wallet_rpc_url;
    async fn rpc(client: &reqwest::Client, url: &str, method: &str, params: Value, token: Option<&str>) -> Result<Value,String> {
        let mut request = client.post(url).json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
        if let Some(t) = token { request = request.bearer_auth(t); }
        let reply = request.send().await.map_err(|e|format!("{method}: transport: {e}"))?;
        let status = reply.status();
        let body: Value = reply.json().await.map_err(|e|format!("{method}: invalid JSON ({status}): {e}"))?;
        if !status.is_success() { return Err(format!("{method}: HTTP {status}: {body}")); }
        if let Some(err) = body.get("error") { return Err(format!("{method}: {err}")); }
        body.get("result").cloned().ok_or_else(||format!("{method}: missing result"))
    }
    let auth = rpc(&client,url,"auth.request",json!({"permissions":["Admin"],"credentials":"None"}),None).await?;
    let token = auth.get("token").and_then(Value::as_str).ok_or("auth.request missing token")?;
    let body = rpc(&client,url,"substates.get",json!({"substate_id":component}),Some(token)).await?;
    decode_controller_state(&body,component,template,config)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Config, Value, String, String) {
        let config = Config::load(std::path::Path::new("config")).unwrap();
        let template = "c1cfab3328fdd6e4f4330bebac9d6ebf4a5b2c0cdf34814eaffe30eae886bd37".to_string();
        let component = "component_179d6614c4e42a4742f2f499067ea0a5b7168c06700583057eddb73ffe3b2af0".to_string();
        let add = |s: &str, tag: u64| json!({"@cbor":"tag","tag":tag,"value":{"@cbor":"bytes","hex":s}});
        let account = |id: &str| config.accounts.accounts.iter().find(|a| a.id==id).unwrap().component_address[10..].to_string();
        let state=json!([
            add(&account("house"),128), add(&account("community"),128), add(&account("chance"),128),
            add(&config.accounts.resource_address[9..],131), 500,500,500,500,1500,500,7
        ]);
        let response=json!({"substate_from_remote":{"version":8,"substate":{"Component":{"header":{"template_address":template},"body":{"state":state}}}}});
        (config,response,component,template)
    }
    #[test]
    fn decodes_verified_live_state() {
        let (config,response,component,template)=fixture();
        let result=decode_controller_state(&response,&component,&template,&config).unwrap();
        assert_eq!(result["distribution_count"],7);
        assert_eq!(result["allocations"][2]["house_bps"],8000);
        assert_eq!(result["allocations"][2]["community_bps"],1500);
        assert_eq!(result["substate_version"],8);
    }
    #[test]
    fn rejects_missing_remote_state() {
        let (config,_,component,template)=fixture();
        let result=decode_controller_state(&json!({"local_record":{"version":5},"substate_from_remote":null}),&component,&template,&config);
        assert!(result.is_err());
    }
    #[test]
    fn rejects_unknown_layout() {
        let (config,mut response,component,template)=fixture();
        response["substate_from_remote"]["substate"]["Component"]["body"]["state"]=json!([1,2]);
        assert!(decode_controller_state(&response,&component,&template,&config).is_err());
    }
}
// End of src/contract_monitor.rs
