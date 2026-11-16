//! Ootle Game Engine | app/src/config.rs
//! Version: v0.4.5
//! Startup configuration: app/config/network.toml plus the deployment pointers written by
//! scripts/deploy_local.py (deployments/local-swarm.json). Read once at start; chain state is
//! always read live from the wallet.
//! v0.3.0 adds the free-play settings: seat start, game House float and top-up, AI pacing and
//! play strength, the auto-roll timeout (0 = off), and the claims and leaderboard stores.
//! Every setting that changes how the server acts on its own defaults to off or to the agreed value.
//! v0.4.0 adds: reconciliation re-reads (agreed: 4 reads, 500 ms apart, on a balance mismatch only),
//! the receipts store (JSON lines, survives restarts), the lot store and the Town Square catalogue.
//! v0.4.5 adds: `balance_refresh` (the `refresh` flag of every balance read; default true, the v0.4.4
//! behaviour) and `carry_verified_balances` (default true: a transaction's reconciled balances are the
//! next transaction's starting balances in that game, under the rules in ops.rs; false = v0.4.4).

use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

fn default_seat_start() -> u64 { 20_000_000 }
fn default_float() -> u64 { 100_000_000 }
fn default_top_up_below() -> u64 { 25_000_000 }
fn default_ai_delay() -> u64 { 5 }
fn default_ai_reserve() -> u64 { 5_000_000 }
fn default_ai_build_floor() -> u64 { 10_000_000 }
fn default_claims() -> String { "data/claims.json".into() }
fn default_leaderboard() -> String { "data/leaderboard.json".into() }
fn default_milestone() -> u64 { 100 }
fn default_reconcile_reads() -> u32 { 4 }
fn default_reconcile_delay() -> u64 { 500 }
fn default_receipts() -> String { "data/receipts.jsonl".into() }
fn default_lots() -> String { "data/lots.json".into() }
fn default_catalogue() -> String { "config/catalogue.json".into() }
fn default_true() -> bool { true }

#[derive(Clone, Debug, Deserialize)]
pub struct NetworkConfig {
    pub wallet_rpc_url: String,
    pub network_name: String,
    pub listen_host: String,
    pub port: u16,
    pub deployment_file: String,
    pub max_fee: u64,
    pub decision_window_secs: u64,
    pub roll_store: String,
    /// Cash a seat holds after a reset or regeneration, base units.
    #[serde(default = "default_seat_start")]
    pub seat_start: u64,
    /// Each game House is topped up from the main House to `house_float` when it falls below
    /// `house_top_up_below` (base units). `house_top_up_below = 0` turns top-ups off.
    #[serde(default = "default_float")]
    pub house_float: u64,
    #[serde(default = "default_top_up_below")]
    pub house_top_up_below: u64,
    /// Seconds between one AI (or auto-roll) move's confirmation and the next AI move in that game.
    #[serde(default = "default_ai_delay")]
    pub ai_move_delay_secs: u64,
    /// Server-wide cap on AI and auto-roll transactions per minute across all games. 0 = no cap.
    #[serde(default)]
    pub ai_tx_per_minute: u32,
    /// AI buys only when at least this much cash stays after the purchase.
    #[serde(default = "default_ai_reserve")]
    pub ai_reserve: u64,
    /// AI builds only when at least this much cash stays after building.
    #[serde(default = "default_ai_build_floor")]
    pub ai_build_floor: u64,
    /// A human (or open) seat that has not moved for this many seconds is passed automatically. 0 = off.
    #[serde(default)]
    pub auto_roll_secs: u64,
    #[serde(default = "default_claims")]
    pub claims_store: String,
    #[serde(default = "default_leaderboard")]
    pub leaderboard_store: String,
    /// Stint length, in rounds, recorded on the leaderboard as a milestone.
    #[serde(default = "default_milestone")]
    pub milestone_rounds: u64,
    /// Balance reads after an accepted transaction before a mismatch is reported (1 = a single read,
    /// no re-read). Further reads happen only while the balances do not match. v0.4.0.
    #[serde(default = "default_reconcile_reads")]
    pub reconcile_reads: u32,
    /// Milliseconds between those reads. v0.4.0.
    #[serde(default = "default_reconcile_delay")]
    pub reconcile_read_delay_ms: u64,
    /// Receipts of every operation, one JSON object per line, kept across restarts. v0.4.0.
    #[serde(default = "default_receipts")]
    pub receipts_store: String,
    /// Business chosen on each owned lot. v0.4.0.
    #[serde(default = "default_lots")]
    pub lots_store: String,
    /// The Town Square catalogue (relative to app/). v0.4.0.
    #[serde(default = "default_catalogue")]
    pub catalogue_file: String,
    /// `refresh` sent with every `accounts.get_balances` call. true = the v0.4.4 behaviour. v0.4.5.
    #[serde(default = "default_true")]
    pub balance_refresh: bool,
    /// After a RECONCILED transaction, its verified balances are kept as the starting balances of the
    /// next transaction in the same game instead of reading them again (ops.rs `Baseline`). false = the
    /// v0.4.4 behaviour (always read before a transaction). v0.4.5.
    #[serde(default = "default_true")]
    pub carry_verified_balances: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Pointer {
    pub component: String,
    pub template: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TreasuryPointer {
    pub component: String,
    pub template: String,
    pub global_house: String,
    pub global_community: String,
    pub global_chance: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GameTypePointer {
    pub id: u32,
    pub name: String,
    pub template: String,
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SeatPointer {
    pub seat: u8,
    pub account_name: String,
    pub component: String,
    pub key_index: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GamePointer {
    pub id: u64,
    pub type_id: u32,
    pub capacity: u8,
    pub session: Pointer,
    pub treasury_mode: String,
    pub house: String,
    pub community: String,
    pub chance: String,
    pub house_account_name: String,
    #[serde(default)]
    pub community_account_name: Option<String>,
    #[serde(default)]
    pub chance_account_name: Option<String>,
    pub seats: Vec<SeatPointer>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Deployment {
    pub network: String,
    pub engine_version: String,
    pub resource: String,
    /// Wallet account name of the main House (the operator; owns every component). v0.3.0.
    #[serde(default)]
    pub operator_account_name: Option<String>,
    pub registry: Pointer,
    pub treasury: TreasuryPointer,
    pub lobby: Pointer,
    pub game_types: Vec<GameTypePointer>,
    pub games: Vec<GamePointer>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub network: NetworkConfig,
    pub deployment: Deployment,
    pub base_dir: PathBuf,
}

fn is_component(s: &str) -> bool {
    s.len() == 74 && s.starts_with("component_") && s[10..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Config {
    /// Loads `<dir>/config/network.toml` (or `OOTLE_CONFIG`, relative to `dir`) and the deployment
    /// file it names (relative to `dir`).
    pub fn load(dir: &Path) -> Result<Self, String> {
        let rel = std::env::var("OOTLE_CONFIG").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "config/network.toml".into());
        let path = dir.join(rel);
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let network: NetworkConfig = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let dpath = dir.join(&network.deployment_file);
        let dtext = fs::read_to_string(&dpath)
            .map_err(|e| format!("deployment file {}: {e}. Run scripts/deploy-local.sh first.", dpath.display()))?;
        let deployment: Deployment = serde_json::from_str(&dtext)
            .map_err(|e| format!("deployment file {}: {e}", dpath.display()))?;
        let config = Config { network, deployment, base_dir: dir.to_path_buf() };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        let n = &self.network;
        if !(n.wallet_rpc_url.starts_with("http://127.0.0.1:") || n.wallet_rpc_url.starts_with("http://localhost:")) {
            return Err("wallet_rpc_url must be a loopback address".into());
        }
        if n.listen_host != "127.0.0.1" {
            return Err("listen_host must be 127.0.0.1".into());
        }
        if n.network_name.trim().is_empty() {
            return Err("network_name cannot be blank".into());
        }
        if n.house_top_up_below > 0 && n.house_float <= n.house_top_up_below {
            return Err("house_float must be above house_top_up_below".into());
        }
        if n.seat_start == 0 { return Err("seat_start must be positive".into()); }
        if !(1..=10).contains(&n.reconcile_reads) { return Err("reconcile_reads must be 1..10".into()); }
        if n.reconcile_read_delay_ms > 10_000 { return Err("reconcile_read_delay_ms must be 0..10000".into()); }
        let d = &self.deployment;
        for (label, p) in [("registry", &d.registry), ("lobby", &d.lobby)] {
            if !is_component(&p.component) || !is_hex64(&p.template) {
                return Err(format!("deployment {label} pointer is invalid"));
            }
        }
        if !is_component(&d.treasury.component) || !is_hex64(&d.treasury.template) || !is_component(&d.treasury.global_house) {
            return Err("deployment treasury pointer is invalid".into());
        }
        if d.games.is_empty() {
            return Err("deployment lists no games".into());
        }
        for g in &d.games {
            if !is_component(&g.session.component) || !is_hex64(&g.session.template) {
                return Err(format!("game {} session pointer is invalid", g.id));
            }
            if g.seats.len() != usize::from(g.capacity) || !(2..=20).contains(&g.capacity) {
                return Err(format!("game {} seat list does not match its capacity", g.id));
            }
            for (i, s) in g.seats.iter().enumerate() {
                if usize::from(s.seat) != i + 1 || !is_component(&s.component) {
                    return Err(format!("game {} seat {} pointer is invalid", g.id, i + 1));
                }
            }
            if !["separate", "global"].contains(&g.treasury_mode.as_str()) {
                return Err(format!("game {} treasury mode is invalid", g.id));
            }
        }
        Ok(())
    }

    pub fn game(&self, id: u64) -> Result<&GamePointer, String> {
        self.deployment.games.iter().find(|g| g.id == id).ok_or_else(|| format!("unknown game {id}"))
    }

    pub fn game_ids(&self) -> Vec<u64> { self.deployment.games.iter().map(|g| g.id).collect() }

    pub fn roll_store_path(&self) -> PathBuf { self.base_dir.join(&self.network.roll_store) }
    pub fn claims_path(&self) -> PathBuf { self.base_dir.join(&self.network.claims_store) }
    pub fn leaderboard_path(&self) -> PathBuf { self.base_dir.join(&self.network.leaderboard_store) }
    pub fn receipts_path(&self) -> PathBuf { self.base_dir.join(&self.network.receipts_store) }
    pub fn lots_path(&self) -> PathBuf { self.base_dir.join(&self.network.lots_store) }
    /// The catalogue path: relative to the base directory, or absolute.
    pub fn catalogue_path(&self) -> PathBuf { self.base_dir.join(&self.network.catalogue_file) }

    /// Whether every game has its own House (separate treasury mode). Games may then run in parallel.
    pub fn all_separate(&self) -> bool { self.deployment.games.iter().all(|g| g.treasury_mode == "separate") }
}

// End of app/src/config.rs
// Location: app/src/config.rs · Created: 2026-11-16 21:06:48 UTC
