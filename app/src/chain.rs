//! Ootle Game Engine | app/src/chain.rs
//! Version: v0.2.0
//! Live reads: the four components of one game and the balances of every account the game
//! touches. Each component's template is checked against the deployment pointer, and the
//! Treasury's registered pools for the game are checked against the pointer, before any use.

use crate::{
    config::{Config, GamePointer},
    decode::{component, Lobby, Registry, Session, Treasury},
    wallet::Wallet,
};
use serde::Serialize;
use serde_json::Value;
use tokio::task::JoinSet;

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub game: GamePointer,
    pub session: Session,
    pub session_version: Value,
    pub lobby: Lobby,
    pub treasury: Treasury,
    pub registry: Registry,
    pub templates: Vec<TemplateCheck>,
    #[serde(skip)]
    pub raw: Vec<(String, Value)>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TemplateCheck { pub contract: &'static str, pub component: String, pub expected: String, pub actual: String, pub ok: bool }

/// Account balances in base units, in the order seats.., house, community, chance.
#[derive(Clone, Debug, Serialize)]
pub struct Balances { pub seats: Vec<u64>, pub house: u64, pub community: u64, pub chance: u64 }

impl Balances {
    pub fn as_vec(&self) -> Vec<u64> {
        let mut v = self.seats.clone();
        v.extend([self.house, self.community, self.chance]);
        v
    }
}

pub async fn read(cfg: &Config, wallet: &Wallet, game_id: u64) -> Result<Snapshot, String> {
    let g = cfg.game(game_id)?.clone();
    let d = &cfg.deployment;
    let targets = [
        ("Town Square", g.session.component.clone(), g.session.template.clone()),
        ("Lobby", d.lobby.component.clone(), d.lobby.template.clone()),
        ("Treasury", d.treasury.component.clone(), d.treasury.template.clone()),
        ("Registry", d.registry.component.clone(), d.registry.template.clone()),
    ];
    let (a, b, c, e) = tokio::join!(
        wallet.substate(&targets[0].1), wallet.substate(&targets[1].1),
        wallet.substate(&targets[2].1), wallet.substate(&targets[3].1));
    let raws = [a?, b?, c?, e?];
    let mut states = Vec::new();
    let mut templates = Vec::new();
    let mut version = Value::Null;
    for (i, raw) in raws.iter().enumerate() {
        let (template, v, state) = component(raw).map_err(|e| format!("{} substate: {e}", targets[i].0))?;
        let ok = template.eq_ignore_ascii_case(&targets[i].2);
        templates.push(TemplateCheck { contract: targets[i].0, component: targets[i].1.clone(),
            expected: targets[i].2.clone(), actual: template.clone(), ok });
        if !ok { return Err(format!("{} template mismatch: expected {}, chain has {template}", targets[i].0, targets[i].2)); }
        if i == 0 { version = v; }
        states.push(state);
    }
    let session = Session::decode(&states[0])?;
    let lobby = Lobby::decode(&states[1])?;
    let treasury = Treasury::decode(&states[2])?;
    let registry = Registry::decode(&states[3])?;
    if session.game_id != g.id { return Err(format!("session component reports game {}, deployment says {}", session.game_id, g.id)); }
    if session.capacity != g.capacity { return Err("session capacity differs from the deployment".into()); }
    if session.seats.len() != g.seats.len() || session.seats.iter().zip(&g.seats).any(|(a, p)| *a != p.component) {
        return Err("session seat accounts differ from the deployment".into());
    }
    if session.resource != d.resource { return Err("session resource differs from the deployment".into()); }
    let tg = treasury.games.iter().find(|t| t.id == g.id).ok_or("game is not registered in the Treasury")?;
    if tg.session != g.session.component || tg.house != g.house || tg.community != g.community || tg.chance != g.chance {
        return Err("Treasury pools for this game differ from the deployment".into());
    }
    let raw = targets.iter().zip(raws).map(|(t, r)| (t.0.to_owned(), r)).collect();
    Ok(Snapshot { game: g, session, session_version: version, lobby, treasury, registry, templates, raw })
}

pub async fn balances(wallet: &Wallet, g: &GamePointer, resource: &str) -> Result<Balances, String> {
    let mut addrs: Vec<String> = g.seats.iter().map(|s| s.component.clone()).collect();
    addrs.extend([g.house.clone(), g.community.clone(), g.chance.clone()]);
    let mut set = JoinSet::new();
    for (i, a) in addrs.into_iter().enumerate() {
        let (w, r) = (wallet.clone(), resource.to_owned());
        set.spawn(async move { (i, w.balance(&a, &r).await) });
    }
    let n = g.seats.len();
    let mut out = vec![0u64; n + 3];
    while let Some(j) = set.join_next().await {
        let (i, r) = j.map_err(|e| format!("balance task: {e}"))?;
        out[i] = r?;
    }
    Ok(Balances { seats: out[..n].to_vec(), house: out[n], community: out[n + 1], chance: out[n + 2] })
}

// End of app/src/chain.rs
// Location: app/src/chain.rs · Created: 2026-11-15 23:05:29 UTC
