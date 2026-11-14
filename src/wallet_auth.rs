//! Backend-only wallet authentication for local Swarm and cookie-enabled Ootle walletd.
//! Set OOTLE_WALLET_COOKIE_PATH only on the trusted application host. Do not expose
//! cookie bytes to clients, logs, source control, or frontend JavaScript.
use serde_json::{json, Value};
use std::{env, fs};

/// Obtain a bearer credential, preserving legacy unauthenticated Swarm behavior.
/// A configured cookie path is authoritative: never silently fall back to Swarm
/// authentication if the file is inaccessible or empty.
pub async fn token(client: &reqwest::Client, wallet_rpc_url: &str) -> Result<String, String> {
    if let Some(path) = env::var_os("OOTLE_WALLET_COOKIE_PATH") {
        if path.is_empty() { return Err("OOTLE_WALLET_COOKIE_PATH is empty".into()); }
        let cookie = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read wallet auth cookie: {e}"))?;
        let cookie = cookie.trim();
        if cookie.is_empty() { return Err("wallet auth cookie is empty".into()); }
        return Ok(cookie.to_owned());
    }

    let reply = client.post(wallet_rpc_url)
        .json(&json!({
            "jsonrpc":"2.0", "id":1, "method":"auth.request",
            "params":{"permissions":["Admin"],"credentials":"None"}
        }))
        .send().await.map_err(|e| format!("wallet auth transport: {e}"))?;
    let status = reply.status();
    let result: Value = reply.json().await
        .map_err(|e| format!("wallet auth invalid JSON ({status}): {e}"))?;
    if !status.is_success() { return Err(format!("wallet auth HTTP {status}: {result}")); }
    if let Some(error) = result.get("error") { return Err(format!("wallet auth RPC: {error}")); }
    result.pointer("/result/token").and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_owned).ok_or_else(|| "wallet auth token missing".into())
}
