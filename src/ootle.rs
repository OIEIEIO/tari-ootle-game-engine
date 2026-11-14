//! Ootle Asset Game | src/ootle.rs
//! Backend-only, live READ-ONLY wallet JSON-RPC client.
//! Notes: obtain a short-lived Admin JWT per refresh; never return it to the browser.
//! This development version only connects to loopback wallet endpoints.

use crate::config::Config;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
pub struct LiveBalance {
    pub id: String,
    pub name: String,
    pub component_address: String,
    pub confirmed_on_chain: Option<bool>,
    pub balance: String,
    pub balance_base_units: String,
    pub token_symbol: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiveSnapshot {
    pub source: String,
    pub mode: String,
    pub accounts: Vec<LiveBalance>,
    pub note: String,
}

fn base_unit_display(source: &str, divisor: u64) -> Result<String, String> {
    let amount: u64 = source.parse().map_err(|_| "invalid wallet balance representation")?;
    Ok(format!("{}.{:06}", amount / divisor, amount % divisor))
}

async fn rpc(client: &reqwest::Client, url: &str, method: &str, params: Value, token: Option<&str>) -> Result<Value, String> {
    let mut request = client.post(url).json(&json!({
        "jsonrpc": "2.0", "id": 1, "method": method, "params": params
    }));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.map_err(|e| format!("wallet connection error: {e}"))?;
    let status = response.status();
    let payload: Value = response.json().await.map_err(|e| format!("wallet returned non-JSON HTTP {status}: {e}"))?;
    if !status.is_success() { return Err(format!("wallet HTTP {status}: {payload}")); }
    if let Some(error) = payload.get("error") { return Err(format!("wallet {method}: {error}")); }
    payload.get("result").cloned().ok_or_else(|| format!("wallet {method}: missing result"))
}

pub async fn fetch_balances(config: &Config) -> Result<LiveSnapshot, String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(12))
        .build().map_err(|e| e.to_string())?;
    let url = &config.network.wallet_rpc_url;
    let auth = rpc(&client, url, "auth.request", json!({
        "permissions": ["Admin"], "credentials": "None"
    }), None).await?;
    let token = auth.get("token").and_then(Value::as_str)
        .ok_or("auth.request did not return a bearer token")?;
    let mut accounts = Vec::new();
    for account in &config.accounts.accounts {
        let info = rpc(&client, url, "accounts.get", json!({
            "name_or_address": account.name
        }), Some(token)).await?;
        let on_chain = info.pointer("/account/is_confirmed_on_chain").and_then(Value::as_bool);
        let response = rpc(&client, url, "accounts.get_balances", json!({
            "account": account.name, "refresh": true
        }), Some(token)).await?;
        let balances = response.get("balances").and_then(Value::as_array)
            .ok_or_else(|| format!("{}: missing balances list", account.name))?;
        let resource = balances.iter().find(|item| item.get("resource_address").and_then(Value::as_str)
            == Some(config.accounts.resource_address.as_str()));
        let amount = resource.and_then(|item| item.get("balance")).and_then(Value::as_str).unwrap_or("0");
        let formatted = base_unit_display(amount, 1_000_000)?;
        accounts.push(LiveBalance {
            id: account.id.clone(),
            name: account.name.clone(),
            component_address: account.component_address.clone(),
            confirmed_on_chain: on_chain,
            balance: formatted,
            balance_base_units: amount.into(),
            token_symbol: config.accounts.resource_symbol.clone(),
            status: if resource.is_some() { "resource found" } else { "no resource balance" }.into(),
        });
    }
    Ok(LiveSnapshot {
        source: url.clone(),
        mode: "live-read-only".into(),
        accounts,
        note: "Fetched from wallet JSON-RPC. No blockchain writes. Admin token remains server-side.".into(),
    })
}

// End of src/ootle.rs
