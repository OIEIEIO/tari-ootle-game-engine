//! Ootle Asset Game | src/config.rs
//! Read-only startup configuration for network, accounts, and economy.
//! Notes: account balances are historical snapshots; blockchain data is queried separately.

use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

#[derive(Clone, Debug, Deserialize)]
pub struct NetworkConfig {
    pub wallet_rpc_url: String,
    pub listen_host: String,
    pub preferred_port: u16,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AccountsConfig {
    pub resource_address: String,
    pub resource_symbol: String,
    pub divisibility: u32,
    pub accounts: Vec<AccountConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    pub name: String,
    pub role: String,
    pub component_address: String,
    pub initial_balance: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EconomyConfig {
    pub revenue_rules: Vec<RevenueRule>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RevenueRule {
    pub category: String,
    pub label: String,
    pub house_bps: u16,
    pub community_bps: u16,
    pub chance_bps: u16,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub network: NetworkConfig,
    pub accounts: AccountsConfig,
    pub economy: EconomyConfig,
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))
}

impl Config {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let config = Self {
            network: read_toml(&dir.join("network.toml"))?,
            accounts: read_toml(&dir.join("accounts.toml"))?,
            economy: read_toml(&dir.join("economy.toml"))?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.accounts.divisibility != 6 {
            return Err("v0.3.0 expects 6-decimal tTARI amounts".into());
        }
        if !self.network.wallet_rpc_url.starts_with("http://127.0.0.1:")
            && !self.network.wallet_rpc_url.starts_with("http://localhost:")
        {
            return Err("v0.3.0 only allows a loopback wallet RPC URL".into());
        }
        if self.network.listen_host != "127.0.0.1" {
            return Err("v0.3.0 only binds its dashboard to 127.0.0.1".into());
        }
        if self.accounts.accounts.len() < 5 {
            return Err("expected five configured game accounts".into());
        }
        let mut ids: HashSet<&str> = HashSet::new();
        let mut addresses: HashSet<&str> = HashSet::new();
        for account in &self.accounts.accounts {
            if !ids.insert(account.id.as_str()) || !addresses.insert(account.component_address.as_str()) {
                return Err(format!("duplicate account id or address: {}", account.id));
            }
            if account.role != "treasury" && account.role != "player" {
                return Err(format!("invalid role for {}", account.id));
            }
            if !account.component_address.starts_with("component_") {
                return Err(format!("invalid component address for {}", account.id));
            }
        }
        for id in ["house", "community", "chance", "player1", "player2"] {
            if !ids.contains(id) {
                return Err(format!("missing account '{id}'"));
            }
        }
        if self.economy.revenue_rules.is_empty() {
            return Err("at least one revenue rule required".into());
        }
        let mut categories = HashSet::new();
        for rule in &self.economy.revenue_rules {
            if !categories.insert(&rule.category) {
                return Err(format!("duplicate revenue category: {}", rule.category));
            }
            let total = u32::from(rule.house_bps)
                + u32::from(rule.community_bps)
                + u32::from(rule.chance_bps);
            if total != 10_000 {
                return Err(format!("{} allocations total {total}, expected 10000 bps", rule.category));
            }
        }
        Ok(())
    }
}

// End of src/config.rs
