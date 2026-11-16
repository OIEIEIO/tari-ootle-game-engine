//! Ootle Game Engine | app/src/wallet.rs
//! Version: v0.4.5
//! Wallet daemon JSON-RPC client (backend only). Authentication is the v0.6.18 behaviour:
//! OOTLE_WALLET_COOKIE_PATH when set (read on every call, fails closed), otherwise local Swarm
//! `auth.request` with no credentials. Tokens never leave this process.
//! v0.4.5:
//!   - `owner_key` is cached per account (an account's owner key does not change); errors are not cached;
//!   - `accounts.get_balances` sends `refresh` from the `balance_refresh` setting (default true, the
//!     v0.4.4 behaviour) instead of a hard-coded `true`.

use serde_json::{json, Value};
use std::{collections::HashMap, env, fs, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Wallet {
    client: reqwest::Client,
    url: String,
    /// `refresh` sent with every `accounts.get_balances` call (v0.4.5).
    refresh: bool,
    names: Arc<Mutex<HashMap<String, String>>>,
    /// v0.4.5: owner key per account name or address.
    keys: Arc<Mutex<HashMap<String, Value>>>,
}

impl Wallet {
    pub fn new(url: &str, refresh: bool) -> Result<Self, String> {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(150)).build().map_err(|e| e.to_string())?;
        Ok(Self { client, url: url.to_owned(), refresh, names: Arc::new(Mutex::new(HashMap::new())),
            keys: Arc::new(Mutex::new(HashMap::new())) })
    }

    pub fn url(&self) -> &str { &self.url }

    /// Whether balance reads ask the wallet to refresh (v0.4.5).
    pub fn refresh(&self) -> bool { self.refresh }

    async fn token(&self) -> Result<String, String> {
        if let Some(path) = env::var_os("OOTLE_WALLET_COOKIE_PATH") {
            if path.is_empty() { return Err("OOTLE_WALLET_COOKIE_PATH is empty".into()); }
            let cookie = fs::read_to_string(&path).map_err(|e| format!("cannot read wallet auth cookie: {e}"))?;
            let cookie = cookie.trim();
            if cookie.is_empty() { return Err("wallet auth cookie is empty".into()); }
            return Ok(cookie.to_owned());
        }
        let reply = self.client.post(&self.url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"auth.request",
                          "params":{"permissions":["Admin"],"credentials":"None"}}))
            .send().await.map_err(|e| format!("wallet connection error: {e}"))?;
        let status = reply.status();
        let body: Value = reply.json().await.map_err(|e| format!("wallet auth invalid JSON ({status}): {e}"))?;
        if let Some(err) = body.get("error") { return Err(format!("wallet auth RPC: {err}")); }
        body.pointer("/result/token").and_then(Value::as_str).filter(|t| !t.is_empty())
            .map(str::to_owned).ok_or_else(|| "wallet auth token missing".into())
    }

    /// One JSON-RPC call; returns `result` or the wallet's error text.
    pub async fn rpc(&self, method: &str, params: Value) -> Result<Value, String> {
        let token = self.token().await?;
        let reply = self.client.post(&self.url).bearer_auth(token)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send().await.map_err(|e| format!("wallet connection error: {method}: {e}"))?;
        let status = reply.status();
        let body: Value = reply.json().await.map_err(|e| format!("{method}: HTTP {status}, invalid JSON: {e}"))?;
        if let Some(err) = body.get("error") { return Err(format!("{method}: {err}")); }
        if !status.is_success() { return Err(format!("{method}: HTTP {status}: {body}")); }
        body.get("result").cloned().ok_or_else(|| format!("{method}: missing result"))
    }

    pub async fn substate(&self, id: &str) -> Result<Value, String> {
        self.rpc("substates.get", json!({"substate_id": id})).await
    }

    /// Wallet account name for a component address (cached).
    pub async fn account_name(&self, address: &str) -> Result<String, String> {
        if let Some(n) = self.names.lock().await.get(address) { return Ok(n.clone()); }
        let r = self.rpc("accounts.get", json!({"name_or_address": address})).await?;
        let name = r.pointer("/account/name").or_else(|| r.get("name")).and_then(Value::as_str)
            .ok_or_else(|| format!("accounts.get returned no name for {address}"))?.to_owned();
        self.names.lock().await.insert(address.to_owned(), name.clone());
        Ok(name)
    }

    /// The derived key that owns an account, as the `{"Derived":{...}}` object the wallet expects.
    /// v0.4.5: cached per `name_or_address` after the first successful read.
    pub async fn owner_key(&self, name_or_address: &str) -> Result<Value, String> {
        if let Some(k) = self.keys.lock().await.get(name_or_address) { return Ok(k.clone()); }
        let r = self.rpc("accounts.get", json!({"name_or_address": name_or_address})).await?;
        for p in ["/account/owner_key_id", "/owner_key_id"] {
            if let Some(k) = r.pointer(p) {
                if k.get("Derived").is_some() {
                    self.keys.lock().await.insert(name_or_address.to_owned(), k.clone());
                    return Ok(k.clone());
                }
            }
        }
        Err(format!("wallet did not return a derived owner key for {name_or_address}"))
    }

    /// Balance of `resource` in base units. A missing resource entry is a zero balance.
    pub async fn balance(&self, address: &str, resource: &str) -> Result<u64, String> {
        let name = self.account_name(address).await?;
        let r = self.rpc("accounts.get_balances", json!({"account": name, "refresh": self.refresh})).await?;
        let list = r.get("balances").and_then(Value::as_array).ok_or_else(|| format!("{name}: missing balances list"))?;
        let entry = list.iter().find(|b| b.get("resource_address").and_then(Value::as_str) == Some(resource));
        match entry.and_then(|b| b.get("balance")) {
            None => Ok(0),
            Some(Value::String(s)) => s.parse().map_err(|e| format!("{name}: invalid balance {s}: {e}")),
            Some(Value::Number(n)) => n.as_u64().ok_or_else(|| format!("{name}: invalid balance {n}")),
            Some(other) => Err(format!("{name}: invalid balance {other}")),
        }
    }

    /// Receiving address, only when the wallet returns an `otl_` address explicitly.
    pub async fn receiving_address(&self, address: &str) -> Option<String> {
        let r = self.rpc("accounts.get", json!({"name_or_address": address})).await.ok()?;
        r.get("address").or_else(|| r.pointer("/account/address")).and_then(Value::as_str)
            .filter(|a| a.starts_with("otl_")).map(str::to_owned)
    }
}

// End of app/src/wallet.rs
// Location: app/src/wallet.rs · Created: 2026-11-16 21:06:48 UTC
