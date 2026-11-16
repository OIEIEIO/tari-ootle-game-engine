//! Ootle Game Engine | app/src/ops.rs
//! Version: v0.4.0
//! Application state and the write pipeline. One on-chain transaction per request:
//!   live gate → seat check → per-game lock → read state + templates → balances → House top-up
//!   when low → plan (sim.rs) → balance checks → signer keys → dry run → submit → wait result →
//!   verification reads (state check + reconciliation) → receipt → leaderboard milestones.
//! v0.3.0:
//!   - one lock per game (games in separate treasury mode share no account), plus one lock for the
//!     main House (top-ups); in global treasury mode every game shares one lock;
//!   - players act only for the seats their token holds (claims.rs); the AI driver acts as System;
//!   - a turn may include building on the asset just landed on (one transaction, sim::Plan);
//!   - `reset_seat` with its cash settlement to the seat start, signed by the operator (owner);
//!   - the game House is topped up from the main House to `house_float` when below `house_top_up_below`.
//! v0.4.0:
//!   - verification reads: after an accepted transaction the session state and every balance are read;
//!     only while they do not match the plan, they are read again, up to `reconcile_reads` reads in all,
//!     `reconcile_read_delay_ms` apart. A wallet that serves balances from before the transaction for a
//!     moment (seen live on turns 297 and 301 in v0.3.0) no longer produces a false UNVERIFIED;
//!   - every receipt keeps its detail: each verification read (time, field differences, reconciliation
//!     rows), the pipeline steps, the manifest, the named cause of an UNVERIFIED, and the move's news
//!     events; receipts are appended to `receipts_store` (JSON lines) and reloaded at start;
//!   - a purchase records the lot's business (lots.rs): the player's choice, or random for the AI.
//! Signing: the game House key seals and pays the fee; the acting seat's key (and, for a reset, the
//! operator key) is in `signing_key_ids`, which puts those badges in scope for the contract checks.

use crate::{
    board,
    catalogue::Catalogue,
    chain::{self, Balances, Snapshot},
    claims::{Claims, Holder, Leaderboard, Milestone},
    config::Config,
    decode::Session,
    dice::{self, RollStore},
    lots::Lots,
    sim::{self, Action, Ledger, Plan},
    view,
    wallet::Wallet,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, VecDeque}, fs, io::Write as _, path::PathBuf, sync::Arc, time::{Duration, Instant}};
use tokio::sync::Mutex;

pub const LIVE_ENV: &str = "OOTLE_ENABLE_LIVE_TESTS";

/// Receipts kept in memory (and reloaded from the store at start).
const RECEIPTS_KEPT: usize = 1000;

pub fn live_enabled() -> bool { std::env::var(LIVE_ENV).as_deref() == Ok("1") }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step { pub name: String, pub status: String, pub detail: String, pub ms: u128 }

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Pipeline { pub game: u64, pub operation: String, pub actor: String, pub started_at: u64, pub steps: Vec<Step>, pub manifest: String, pub variables: Value }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Receipt {
    /// Increasing receipt number (1, 2, ...), across restarts.
    #[serde(default)]
    pub id: u64,
    pub game: u64, pub round: u64, pub turn: u64, pub seat: u8, pub operation: String, pub actor: String, pub text: String,
    pub transaction_id: String, pub fee: Option<u64>, pub status: String, pub at: u64,
    /// The simulator's story of the operation (stored as JSON so the receipts store reloads).
    #[serde(default)]
    pub story: Value, pub reconciliation: Value,
    /// Why the operation is UNVERIFIED (None when reconciled): "reread_failed", "state_fields_differ",
    /// "balances_differ", "no_final_fee".
    #[serde(default)]
    pub cause: Option<String>,
    /// Plain description of the verification outcome, e.g. "reconciled on read 2 of 4".
    #[serde(default)]
    pub verification: String,
    /// Each verification read: attempt, milliseconds after the result, state check, reconciliation.
    #[serde(default)]
    pub reads: Vec<Value>,
    /// Field differences of the last read (empty when the state matched).
    #[serde(default)]
    pub state_differences: Vec<Value>,
    /// The operation's pipeline steps.
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub manifest: String,
    /// News events of this operation (rent, purchase, build, GO, card, fee, ...), for the Town Square.
    #[serde(default)]
    pub events: Vec<Value>,
}

/// Who asks for an operation: a browser's player token, or the backend itself (AI and auto-roll).
#[derive(Clone, Debug)]
pub enum Actor { Player(String), Ai, AutoRoll }

impl Actor {
    pub fn label(&self) -> &'static str { match self { Actor::Player(_) => "player", Actor::Ai => "ai", Actor::AutoRoll => "auto-roll" } }
    pub fn is_system(&self) -> bool { !matches!(self, Actor::Player(_)) }
}

pub struct App {
    pub cfg: Config,
    pub wallet: Wallet,
    pub catalogue: Catalogue,
    pub rolls: Mutex<RollStore>,
    pub claims: Mutex<Claims>,
    pub board: Mutex<Leaderboard>,
    pub lots: Mutex<Lots>,
    locks: HashMap<u64, Arc<Mutex<()>>>,
    operator_lock: Mutex<()>,
    pub receipts: Mutex<Vec<Receipt>>,
    receipt_seq: Mutex<u64>,
    receipts_path: PathBuf,
    pub pipelines: Mutex<HashMap<u64, Pipeline>>,
    pub seen: Mutex<HashMap<String, u64>>,
    /// Per game: when the last AI or auto-roll transaction was confirmed (pacing).
    pub last_auto: Mutex<HashMap<u64, Instant>>,
    /// Confirmation times of AI and auto-roll transactions in the last minute (server-wide cap).
    pub auto_window: Mutex<VecDeque<Instant>>,
    /// Per game: the last error the AI driver met, shown on the page until a move succeeds.
    pub driver_error: Mutex<HashMap<u64, String>>,
    pub started_at: u64,
}

pub type Shared = Arc<App>;

/// An operation failure: the cause and, once submitted, the transaction id.
pub struct OpError { pub reason: String, pub tx: Option<String> }
impl From<String> for OpError { fn from(reason: String) -> Self { OpError { reason, tx: None } } }

struct Rec { p: Pipeline, t: Instant }
impl Rec {
    fn step(&mut self, name: &str, status: &str, detail: impl Into<String>) {
        let ms = self.t.elapsed().as_millis();
        self.t = Instant::now();
        self.p.steps.push(Step { name: name.into(), status: status.into(), detail: detail.into(), ms });
    }
}

pub enum Request {
    /// The active seat's turn with its stored roll. `build` adds building on the asset landed on.
    /// `business` names the business for a purchase (lots.rs); None picks at random.
    Turn { buy: bool, sell_square: Option<u16>, build: bool, business: Option<String> },
    Action(Action),
    /// Reset a seat to GO with the seat start. `pass_turn` ends its turn (regeneration).
    Reset { seat: u8, pass_turn: bool },
}

impl Request {
    pub fn name(&self) -> &'static str {
        match self {
            Request::Turn { build: true, .. } => "turn + build",
            Request::Turn { .. } => "turn",
            Request::Reset { pass_turn: true, .. } => "regenerate",
            Request::Reset { .. } => "reset_seat",
            Request::Action(Action::Sell { .. }) => "sell_to_house",
            Request::Action(Action::PayShopFee { .. }) => "pay_shop_fee",
            Request::Action(Action::Propose { .. }) => "propose_sale",
            Request::Action(Action::Accept { .. }) => "accept_sale",
            Request::Action(Action::Decline { .. }) => "decline_sale",
            Request::Action(Action::Build { .. }) => "build_improvement",
            Request::Action(Action::Turn { .. }) => "turn",
            Request::Action(Action::Reset { .. }) => "reset_seat",
        }
    }

    /// The seat this request acts for (the seat that must belong to the player).
    pub fn seat(&self, s: &Session) -> u8 {
        match *self {
            Request::Turn { .. } => s.active,
            Request::Reset { seat, .. } => seat,
            Request::Action(Action::Sell { seat, .. } | Action::Build { seat, .. } | Action::Turn { seat, .. } | Action::Reset { seat, .. }) => seat,
            Request::Action(Action::PayShopFee { payer, .. }) => payer,
            Request::Action(Action::Propose { seller, .. }) => seller,
            Request::Action(Action::Accept { buyer } | Action::Decline { buyer }) => buyer,
        }
    }
}

/// Loads the last `RECEIPTS_KEPT` receipts from the JSON-lines store (unreadable lines are skipped)
/// and rewrites the store with only those when it has grown past four times that.
fn load_receipts(path: &PathBuf) -> Result<Vec<Receipt>, String> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut out: Vec<Receipt> = lines.iter().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if out.len() > RECEIPTS_KEPT { out.drain(..out.len() - RECEIPTS_KEPT); }
    if lines.len() > 4 * RECEIPTS_KEPT {
        let tmp = path.with_extension("tmp");
        let body: String = out.iter().filter_map(|r| serde_json::to_string(r).ok()).map(|l| l + "\n").collect();
        fs::write(&tmp, body).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

impl App {
    pub fn new(cfg: Config) -> Result<Self, String> {
        let wallet = Wallet::new(&cfg.network.wallet_rpc_url)?;
        let catalogue = Catalogue::load(&cfg.catalogue_path())?;
        let rolls = RollStore::open(cfg.roll_store_path())?;
        let claims = Claims::open(cfg.claims_path())?;
        let board = Leaderboard::open(cfg.leaderboard_path())?;
        let lots = Lots::open(cfg.lots_path())?;
        let receipts_path = cfg.receipts_path();
        let receipts = load_receipts(&receipts_path)?;
        let seq = receipts.iter().map(|r| r.id).max().unwrap_or(0);
        let shared = Arc::new(Mutex::new(()));
        let locks = cfg.game_ids().into_iter()
            .map(|g| (g, if cfg.all_separate() { Arc::new(Mutex::new(())) } else { shared.clone() })).collect();
        Ok(Self { cfg, wallet, catalogue, rolls: Mutex::new(rolls), claims: Mutex::new(claims), board: Mutex::new(board),
            lots: Mutex::new(lots), locks, operator_lock: Mutex::new(()), receipts: Mutex::new(receipts),
            receipt_seq: Mutex::new(seq), receipts_path, pipelines: Mutex::new(HashMap::new()),
            seen: Mutex::new(HashMap::new()), last_auto: Mutex::new(HashMap::new()), auto_window: Mutex::new(VecDeque::new()),
            driver_error: Mutex::new(HashMap::new()), started_at: dice::now() })
    }

    pub fn game_lock(&self, game: u64) -> Result<Arc<Mutex<()>>, String> {
        self.locks.get(&game).cloned().ok_or_else(|| format!("unknown game {game}"))
    }

    /// When the backend first saw this turn (decision window and auto-roll timer).
    pub async fn turn_seen(&self, game: u64, round: u64, turn: u64) -> u64 {
        let mut m = self.seen.lock().await;
        if m.len() > 2000 { m.clear(); }
        *m.entry(format!("{game}:{round}:{turn}")).or_insert_with(dice::now)
    }

    /// The game's latest receipts, newest first, as the page's history (with each receipt's news events).
    pub async fn history(&self, game: u64) -> Vec<Value> {
        self.receipts.lock().await.iter().rev().filter(|r| r.game == game).take(40)
            .map(|r| json!({"id": r.id, "round": r.round, "turn": r.turn, "seat": r.seat, "operation": r.operation, "actor": r.actor,
                "text": r.text, "transaction_id": r.transaction_id, "fee": r.fee, "status": r.status, "at": r.at,
                "events": r.events})).collect()
    }

    pub async fn state(&self, game: u64, token: Option<&str>) -> Result<Value, String> {
        let snap = chain::read(&self.cfg, &self.wallet, game).await?;
        let bal = chain::balances(&self.wallet, &snap.game, &snap.session.resource).await?;
        let s = &snap.session;
        let lots = {
            let mut l = self.lots.lock().await;
            l.sync(game, &s.owners, &self.catalogue, dice::now())?;
            l.game(game)
        };
        let roll = self.rolls.lock().await.get(game, s.round, s.turn_count, s.active).cloned();
        let seen = self.turn_seen(game, s.round, s.turn_count).await;
        let history = self.history(game).await;
        let claims = self.claims.lock().await.seats(game, s.capacity);
        let mine: Vec<u8> = match token {
            Some(t) => (1..=s.capacity).filter(|n| matches!(&claims[usize::from(n - 1)].holder, Holder::Human { token: x, .. } if x == t)).collect(),
            None => vec![],
        };
        let driver_error = self.driver_error.lock().await.get(&game).cloned();
        self.check_milestones(&snap.session, &bal, &claims).await;
        Ok(view::game_view(&view::Ctx { snap: &snap, bal: &bal, roll: roll.as_ref(), turn_seen_at: seen, now: dice::now(),
            decision_window_secs: self.cfg.network.decision_window_secs, auto_roll_secs: self.cfg.network.auto_roll_secs,
            max_fee: self.cfg.network.max_fee, seat_start: self.cfg.network.seat_start, live_enabled: live_enabled(),
            network: &self.cfg.network.network_name, history, claims: &claims, mine: &mine, driver_error,
            catalogue: &self.catalogue, lots: &lots }))
    }

    /// Draws (or returns the stored) roll for the active seat's current turn.
    pub async fn roll(&self, game: u64, pay: bool, actor: &Actor) -> Result<Session, String> {
        let snap = chain::read(&self.cfg, &self.wallet, game).await?;
        let s = snap.session;
        if !s.started { return Err("game not started".into()); }
        if s.has_offer() { return Err("resolve the pending offer first".into()); }
        if let Actor::Player(t) = actor {
            if !self.claims.lock().await.owns(t, game, s.active) { return Err("not your seat".into()); }
        }
        let held = s.held[usize::from(s.active - 1)];
        self.rolls.lock().await.roll(game, s.round, s.turn_count, s.active, held && pay)?;
        Ok(s)
    }

    pub async fn execute(&self, game: u64, req: Request, actor: Actor) -> Result<Value, OpError> {
        if !live_enabled() { return Err(format!("Live transactions disabled; set {LIVE_ENV}=1").into()); }
        let lock = self.game_lock(game)?;
        let _guard = lock.try_lock().map_err(|_| "operation in progress".to_string())?;
        let mut rec = Rec { p: Pipeline { game, operation: req.name().into(), actor: actor.label().into(),
            started_at: dice::now(), ..Default::default() }, t: Instant::now() };
        let r = self.run(game, req, &actor, &mut rec).await;
        if let Err(e) = &r { rec.step("Stopped", "error", e.reason.clone()); }
        self.pipelines.lock().await.insert(game, rec.p);
        r
    }

    async fn run(&self, game: u64, req: Request, actor: &Actor, rec: &mut Rec) -> Result<Value, OpError> {
        let cfg = &self.cfg;
        let before = chain::read(cfg, &self.wallet, game).await?;
        let acting = req.seat(&before.session);
        if let Actor::Player(t) = actor {
            if !self.claims.lock().await.owns(t, game, acting) { return Err("not your seat".to_string().into()); }
        }
        rec.step("Read state", "done", format!("session version {} · round {} · turn {} · active seat {} · acting seat {acting} · templates verified",
            before.session_version, before.session.round, before.session.turn_count, before.session.active));

        let mut b0 = chain::balances(&self.wallet, &before.game, &before.session.resource).await?;
        let s = &before.session;

        let mut business: Option<String> = None;
        let actions = match req {
            Request::Turn { buy, sell_square, build, business: b } => {
                let roll = self.rolls.lock().await.get(game, s.round, s.turn_count, s.active).cloned()
                    .ok_or_else(|| "no roll for this turn: roll first".to_string())?;
                rec.step("Roll", "done", format!("backend CSPRNG · {} + {} · stored for (game {game}, round {}, turn {}, seat {}){}",
                    roll.die1, roll.die2, roll.round, roll.turn, roll.seat, if s.held[usize::from(roll.seat - 1)] {
                        if roll.pay { " · pay chosen at roll time" } else { " · roll for doubles" } } else { "" }));
                let turn = Action::Turn { seat: roll.seat, die1: roll.die1, die2: roll.die2, pay: roll.pay, buy, sell_square };
                let mut v = vec![turn.clone()];
                if build {
                    let o = sim::simulate(s, before.treasury.community_bps, before.treasury.chance_bps, &turn)?;
                    let q = o.after.landed_square;
                    if o.after.landed_seat != roll.seat || q == u16::MAX || o.after.owners[usize::from(q)] != roll.seat {
                        return Err("nothing to build: this landing is not on an asset the seat owns".to_string().into());
                    }
                    v.push(Action::Build { seat: roll.seat, square: q });
                }
                business = b;
                v
            }
            Request::Reset { seat, pass_turn } => {
                if seat == 0 || usize::from(seat) > b0.seats.len() { return Err("invalid seat".to_string().into()); }
                let cash = b0.seats[usize::from(seat - 1)];
                vec![Action::Reset { seat, pass_turn, settle: i128::from(cfg.network.seat_start) - i128::from(cash) }]
            }
            Request::Action(a) => vec![a],
        };
        let (cb, hb) = (before.treasury.community_bps, before.treasury.chance_bps);
        let plan = sim::plan(s, cb, hb, &actions)?;
        let purchase = plan.first().story.purchase > 0;
        if purchase { Lots::check(plan.first().story.to, business.as_deref(), &self.catalogue)?; }
        rec.step("Prepare", "done", format!("{} · house funding {} · seat {} funding {}{}", plan.method(), plan.house_withdraw(),
            plan.signer(), plan.seat_needed(),
            if purchase { format!(" · business {}", business.as_deref().unwrap_or("chosen at random")) } else { String::new() }));

        // Game House top-up (main House → game House) when it is below the threshold or this payout.
        let need = plan.house_withdraw() + cfg.network.max_fee;
        if cfg.network.house_top_up_below > 0 && (b0.house < cfg.network.house_top_up_below || b0.house < need) {
            let amount = cfg.network.house_float.max(need).saturating_sub(b0.house);
            let tx = self.top_up(&before, amount).await?;
            rec.step("House top-up", "done", format!("main House → game House {} tTARI · {tx}", board::ttari(amount)));
            b0 = chain::balances(&self.wallet, &before.game, &s.resource).await?;
        }
        let seat_bal = b0.seats[usize::from(plan.signer() - 1)];
        let seat_need = plan.seat_needed();
        if seat_bal < seat_need {
            return Err(format!("insufficient seat funds for {} [available_base_units={seat_bal}, required_base_units={seat_need}]",
                plan.method()).into());
        }
        if b0.house < need {
            return Err(format!("house cannot cover the payout and fee reserve [available_base_units={}, required_base_units={need}]", b0.house).into());
        }
        rec.step("Balances", "done", format!("seat {seat_bal} ≥ {seat_need} · house {} ≥ {need}", b0.house));

        let seat_ptr = &before.game.seats[usize::from(plan.signer() - 1)];
        let seal = self.wallet.owner_key(&before.game.house_account_name).await?;
        let seat_key = match self.wallet.owner_key(&seat_ptr.account_name).await {
            Ok(k) => k,
            Err(_) => json!({"Derived": {"index": seat_ptr.key_index, "key_branch": "account"}}),
        };
        let mut signing = vec![seat_key];
        if plan.is_reset() { signing.push(self.wallet.owner_key(&self.operator_name().await?).await?); }
        let manifest = sim::plan_manifest(&plan, cfg.network.max_fee);
        let variables = json!({"house": before.game.house, "seat_account": seat_ptr.component, "game": before.game.session.component});
        rec.p.manifest = manifest.clone();
        rec.p.variables = variables.clone();
        let mut params = json!({"manifest": manifest, "variables": variables, "seal_signer_key_id": seal,
            "signing_key_ids": signing, "max_fee": cfg.network.max_fee, "dry_run": true, "blobs": {}});
        rec.step("Signers", "done", format!("seal: House {} · signer: {}{}", before.game.house_account_name, seat_ptr.account_name,
            if plan.is_reset() { " · operator (owner method)" } else { "" }));

        let dry = self.wallet.rpc("transactions.submit_manifest", params.clone()).await?;
        if dry.pointer("/result/finalize/result/Accept").is_none() {
            let reason = dry.pointer("/result/finalize/result").cloned().unwrap_or(dry.clone());
            return Err(format!("dry run rejected: {reason}").into());
        }
        rec.step("Dry run", "done", "Accept");

        params["dry_run"] = json!(false);
        let submitted = self.wallet.rpc("transactions.submit_manifest", params).await?;
        let tx = submitted.get("transaction_id").and_then(Value::as_str).ok_or_else(|| "missing transaction id".to_string())?.to_owned();
        rec.step("Submit", "done", tx.clone());
        // The stored roll is kept: if this transaction is rejected the turn has not advanced and the
        // same dice apply on retry. It is pruned once a roll for a later turn is drawn.
        let fail = |reason: String| OpError { reason, tx: Some(tx.clone()) };

        let res = self.wallet.rpc("transactions.wait_result", json!({"transaction_id": tx, "timeout_secs": 120})).await.map_err(fail)?;
        if res.get("timed_out").and_then(Value::as_bool) == Some(true) {
            return Err(fail(format!("transaction {tx} timed out; inspect chain state before retrying")));
        }
        let accepted = res.get("status").and_then(Value::as_str) == Some("Accepted") && res.pointer("/result/result/Accept").is_some();
        let fee = res.get("final_fee").and_then(Value::as_u64);
        if !accepted {
            rec.step("Wait result", "error", format!("status {}", res.get("status").cloned().unwrap_or(Value::Null)));
            let mut r = self.new_receipt(&before, &plan, actor, &tx, fee, "REJECTED", rec);
            r.cause = Some("rejected".into());
            r.verification = "the network did not accept the transaction".into();
            self.push_receipt(r).await;
            return Ok(json!({"committed": false, "verified": false, "transaction_id": tx, "result": res}));
        }
        rec.step("Wait result", "done", format!("Accepted · fee {}", fee.map(|f| f.to_string()).unwrap_or("unknown".into())));

        // The business on a purchased lot is recorded as soon as the purchase is accepted.
        let mut chosen = None;
        if purchase {
            let source = if actor.is_system() { "ai" } else { "player" };
            let name = if actor.is_system() { None } else { business.as_deref() };
            match self.lots.lock().await.choose(game, plan.first().story.to, plan.signer(), name, source, &self.catalogue, dice::now()) {
                Ok(c) => chosen = Some(c.business),
                Err(e) => rec.step("Lot", "error", format!("business not recorded: {e}")),
            }
        }

        let v = self.verify(game, &before, &b0, &plan, fee, rec).await;
        let status = if v.ok { "COMMITTED / RECONCILED" } else { "COMMITTED / UNVERIFIED" };
        let mut r = self.new_receipt(&before, &plan, actor, &tx, fee, status, rec);
        r.reconciliation = v.recon.clone();
        r.cause = v.cause.clone();
        r.verification = v.summary.clone();
        r.reads = v.reads.clone();
        r.state_differences = v.diffs.clone();
        if let (Some(b), Some(e)) = (&chosen, r.events.iter_mut().find(|e| e["kind"] == "buy")) { e["business"] = json!(b); }
        self.push_receipt(r).await;
        if let Some(after) = &v.after {
            let _ = self.lots.lock().await.sync(game, &after.owners, &self.catalogue, dice::now());
            if let Some(b1) = &v.b1 {
                let claims = self.claims.lock().await.seats(game, after.capacity);
                self.check_milestones(after, b1, &claims).await;
            }
        }
        Ok(json!({"committed": true, "verified": v.ok, "status": status, "transaction_id": tx, "fee": fee,
            "story": plan.story(), "state_differences": v.diffs, "reconciliation": v.recon, "cause": v.cause,
            "verification": v.summary, "business": chosen}))
    }

    /// Verification reads after an accepted transaction: the session state and every balance are read
    /// and compared with the plan; only while they do not match, they are read again, up to
    /// `reconcile_reads` reads, `reconcile_read_delay_ms` apart.
    async fn verify(&self, game: u64, before: &Snapshot, b0: &Balances, plan: &Plan, fee: Option<u64>, rec: &mut Rec) -> Verification {
        let n = self.cfg.network.reconcile_reads.max(1);
        let delay = Duration::from_millis(self.cfg.network.reconcile_read_delay_ms);
        let t0 = Instant::now();
        let mut out = Verification::default();
        for attempt in 1..=n {
            if attempt > 1 { tokio::time::sleep(delay).await; }
            let at_ms = t0.elapsed().as_millis();
            let after = chain::read(&self.cfg, &self.wallet, game).await;
            let b1 = chain::balances(&self.wallet, &before.game, &before.session.resource).await;
            let (diffs, state_err) = match &after {
                Ok(a) => (state_diffs(&a.session, plan.after()), None),
                Err(e) => (vec![], Some(e.clone())),
            };
            let recon = match (&b1, fee) {
                (Ok(b1), Some(f)) => reconcile(b0, b1, &plan.ledger(), f),
                (Err(e), _) => json!({"ok": false, "error": e}),
                (_, None) => json!({"ok": false, "error": "finalized result has no final_fee"}),
            };
            let recon_ok = recon["ok"] == json!(true);
            let ok = state_err.is_none() && diffs.is_empty() && recon_ok;
            let mismatched: Vec<Value> = recon["rows"].as_array().map(|r| r.iter().filter(|x| x["match"] != json!(true))
                .map(|x| json!({"account": x["account"], "expected": x["expected"], "actual": x["actual"]})).collect()).unwrap_or_default();
            out.reads.push(json!({"attempt": attempt, "of": n, "at_ms": at_ms, "ok": ok,
                "state": match &state_err { Some(e) => json!({"error": e}), None => json!({"differences": diffs.len()}) },
                "reconciliation_ok": recon_ok, "sum": recon["sum"], "fee": recon["fee"], "mismatched": mismatched,
                "error": recon.get("error").cloned().unwrap_or(Value::Null)}));
            let label = if attempt == 1 { "Verify".to_string() } else { format!("Verify read {attempt}") };
            rec.step(&label, if ok { "done" } else { "error" }, if ok {
                "state matches the expected state · all balance changes match; sum = −fee".to_string()
            } else {
                format!("{}{}", match &state_err { Some(e) => format!("state re-read failed: {e} · "), None if !diffs.is_empty() => format!("{} state field(s) differ · ", diffs.len()), None => String::new() },
                    if recon_ok { "balances match".to_string() } else { format!("{} balance row(s) differ", mismatched.len().max(1)) })
            });
            out.diffs = diffs;
            out.recon = recon;
            out.after = after.ok().map(|a| a.session);
            out.b1 = b1.ok();
            out.cause = if ok { None } else if state_err.is_some() { Some("reread_failed".into()) }
                else if !out.diffs.is_empty() { Some("state_fields_differ".into()) }
                else if fee.is_none() { Some("no_final_fee".into()) } else { Some("balances_differ".into()) };
            if ok {
                out.ok = true;
                out.summary = if attempt == 1 { "reconciled on the first read".into() }
                    else { format!("reconciled on read {attempt} of {n}: the earlier read(s) returned balances from before the transaction") };
                return out;
            }
        }
        out.summary = format!("not reconciled after {n} read(s), {} ms apart: {}", self.cfg.network.reconcile_read_delay_ms,
            match out.cause.as_deref() { Some("reread_failed") => "the state re-read failed", Some("state_fields_differ") => "state fields differ",
                Some("no_final_fee") => "the result has no final fee", _ => "balance changes differ" });
        out
    }

    fn new_receipt(&self, before: &Snapshot, plan: &Plan, actor: &Actor, tx: &str, fee: Option<u64>, status: &str, rec: &Rec) -> Receipt {
        let s = &before.session;
        let story = plan.story();
        Receipt { id: 0, game: before.game.id, round: s.round, turn: s.turn_count, seat: plan.signer(), operation: plan.method(),
            actor: actor.label().into(), text: story.text.clone(), transaction_id: tx.into(), fee, status: status.into(),
            at: dice::now(), story: json!(story), reconciliation: json!({}), cause: None, verification: String::new(), reads: vec![],
            state_differences: vec![], steps: rec.p.steps.clone(), manifest: rec.p.manifest.clone(), events: events(plan) }
    }

    /// Numbers the receipt, keeps it in memory and appends it to the receipts store.
    async fn push_receipt(&self, mut r: Receipt) {
        let id = { let mut q = self.receipt_seq.lock().await; *q += 1; *q };
        r.id = id;
        if let Some(d) = self.receipts_path.parent() { let _ = fs::create_dir_all(d); }
        match (serde_json::to_string(&r), fs::OpenOptions::new().create(true).append(true).open(&self.receipts_path)) {
            (Ok(line), Ok(mut f)) => { if let Err(e) = writeln!(f, "{line}") { tracing::warn!(cause = %e, "receipt not stored"); } }
            (Err(e), _) => tracing::warn!(cause = %e, "receipt not serialised"),
            (_, Err(e)) => tracing::warn!(cause = %e, path = %self.receipts_path.display(), "receipts store unavailable"),
        }
        let mut v = self.receipts.lock().await;
        v.push(r);
        let n = v.len();
        if n > RECEIPTS_KEPT { v.drain(..n - RECEIPTS_KEPT); }
    }

    /// Wallet account name of the main House (operator).
    pub async fn operator_name(&self) -> Result<String, String> {
        match &self.cfg.deployment.operator_account_name {
            Some(n) => Ok(n.clone()),
            None => self.wallet.account_name(&self.cfg.deployment.treasury.global_house).await,
        }
    }

    /// Moves `amount` from the main House to this game's House, signed and paid by the operator.
    async fn top_up(&self, snap: &Snapshot, amount: u64) -> Result<String, String> {
        let _g = self.operator_lock.lock().await;
        let op_name = self.operator_name().await?;
        let op = &self.cfg.deployment.treasury.global_house;
        if *op == snap.game.house { return Err("game House is the main House (global mode): no top-up".into()); }
        let main_bal = self.wallet.balance(op, &snap.session.resource).await?;
        if main_bal < amount + self.cfg.network.max_fee {
            return Err(format!("house cannot cover the payout and fee reserve: main House cannot fund the top-up \
                [available_base_units={main_bal}, required_base_units={}]", amount + self.cfg.network.max_fee));
        }
        let manifest = top_up_manifest(&snap.session.resource, amount, self.cfg.network.max_fee);
        let seal = self.wallet.owner_key(&op_name).await?;
        let mut params = json!({"manifest": manifest, "variables": {"operator": op, "house": snap.game.house},
            "seal_signer_key_id": seal, "signing_key_ids": [], "max_fee": self.cfg.network.max_fee, "dry_run": true, "blobs": {}});
        let dry = self.wallet.rpc("transactions.submit_manifest", params.clone()).await?;
        if dry.pointer("/result/finalize/result/Accept").is_none() {
            return Err(format!("dry run rejected: House top-up: {}", dry.pointer("/result/finalize/result").cloned().unwrap_or(dry.clone())));
        }
        params["dry_run"] = json!(false);
        let tx = self.wallet.rpc("transactions.submit_manifest", params).await?
            .get("transaction_id").and_then(Value::as_str).ok_or("missing transaction id")?.to_owned();
        let res = self.wallet.rpc("transactions.wait_result", json!({"transaction_id": tx, "timeout_secs": 120})).await?;
        if res.get("status").and_then(Value::as_str) != Some("Accepted") || res.pointer("/result/result/Accept").is_none() {
            return Err(format!("House top-up {tx} not accepted: {}", res.get("status").cloned().unwrap_or(Value::Null)));
        }
        let s = &snap.session;
        self.push_receipt(Receipt { id: 0, game: snap.game.id, round: s.round, turn: s.turn_count, seat: 0,
            operation: "house_top_up".into(), actor: "system".into(),
            text: format!("Main House topped up the game House by {} tTARI", board::ttari(amount)), transaction_id: tx.clone(),
            fee: res.get("final_fee").and_then(Value::as_u64), status: "COMMITTED".into(), at: dice::now(),
            story: Value::Null, reconciliation: json!({}), cause: None,
            verification: "top-up: not reconciled (operator transfer)".into(), reads: vec![], state_differences: vec![],
            steps: vec![], manifest, events: vec![json!({"kind": "top_up", "amount": amount})] }).await;
        Ok(tx)
    }

    /// Records each seat whose current stint has reached `milestone_rounds` (once per stint).
    async fn check_milestones(&self, s: &Session, bal: &Balances, claims: &[crate::claims::SeatClaim]) {
        let m = self.cfg.network.milestone_rounds;
        if m == 0 { return; }
        let mut board = self.board.lock().await;
        for i in 0..s.seats.len() {
            let rounds = s.round.saturating_sub(s.seat_since[i]);
            if rounds < m || s.out[i] { continue; }
            let seat = i as u8 + 1;
            let (kind, name) = match &claims[i].holder {
                Holder::Human { name, .. } => ("human", name.clone()),
                Holder::Ai => ("ai", format!("AI · seat {seat}")),
                Holder::Open => ("open", format!("Open · seat {seat}")),
            };
            let cash = bal.seats[i];
            let _ = board.record(Milestone { game: s.game_id, seat, stint: s.resets[i], kind: kind.into(), name, rounds,
                round: s.round, net_worth: cash + s.buyback_total(seat), cash, at: dice::now() });
        }
    }
}

#[derive(Default)]
struct Verification {
    ok: bool,
    cause: Option<String>,
    summary: String,
    reads: Vec<Value>,
    diffs: Vec<Value>,
    recon: Value,
    after: Option<Session>,
    b1: Option<Balances>,
}

/// The news events of a plan, one or more per step, for the Town Square news stack and sounds.
/// Amounts are base units. `square` events carry the square so the page can name the lot's business.
pub fn events(plan: &Plan) -> Vec<Value> {
    let mut out = vec![];
    for (a, o) in &plan.steps {
        let st = &o.story;
        let after = &o.after;
        match a {
            Action::Turn { .. } => {
                let seat = st.seat;
                if st.third_double {
                    out.push(json!({"kind": "maintenance_sent", "seat": seat, "reason": "third_double", "die1": st.die1, "die2": st.die2}));
                    continue;
                }
                if !st.moved {
                    out.push(json!({"kind": "maintenance_held", "seat": seat, "attempt": st.held_attempt, "owed": st.held_owed,
                        "die1": st.die1, "die2": st.die2}));
                    continue;
                }
                out.push(json!({"kind": "move", "seat": seat, "from": st.from, "to": st.to, "die1": st.die1, "die2": st.die2,
                    "doubles": st.doubles, "again": st.again}));
                if st.released { out.push(json!({"kind": "released", "seat": seat, "paid": st.maintenance_fee})); }
                if let Some(q) = st.sold_square { out.push(json!({"kind": "sell", "seat": seat, "square": q, "amount": st.sale})); }
                if st.go_reward > 0 { out.push(json!({"kind": "go", "seat": seat, "amount": st.go_reward})); }
                if st.sent_to_maintenance { out.push(json!({"kind": "maintenance_sent", "seat": seat, "reason": "square_30"})); continue; }
                if st.purchase > 0 { out.push(json!({"kind": "buy", "seat": seat, "square": st.to, "amount": st.purchase})); }
                if st.rent > 0 {
                    out.push(json!({"kind": "rent", "seat": seat, "owner": st.rent_owner, "square": st.to, "amount": st.rent,
                        "earned": st.rent - st.rent_fee, "rent_fee": st.rent_fee, "level": after.improvements[usize::from(st.to)]}));
                }
                if st.card_kind != 0 {
                    out.push(json!({"kind": "card", "seat": seat, "card_kind": st.card_kind, "card_id": st.card_id,
                        "label": sim::card_label(st.card_kind, st.card_id), "amount": st.card_amount, "positive": st.card_positive, "square": st.to}));
                }
                let sq_fee = st.penalty - if st.card_kind != 0 && !st.card_positive { st.card_amount } else { 0 };
                if sq_fee > 0 { out.push(json!({"kind": if st.to == 7 { "penalty" } else { "fee" }, "seat": seat, "amount": sq_fee, "square": st.to})); }
                if st.square_reward > 0 { out.push(json!({"kind": "reward", "seat": seat, "amount": st.square_reward, "square": st.to})); }
            }
            Action::Build { seat, square } => out.push(json!({"kind": "build", "seat": seat, "square": square,
                "level": after.improvements[usize::from(*square)], "amount": st.amount})),
            Action::Sell { seat, square } => out.push(json!({"kind": "sell", "seat": seat, "square": square, "amount": st.amount})),
            Action::PayShopFee { payer, held_seat } => out.push(json!({"kind": "shop_fee", "seat": payer, "held_seat": held_seat, "amount": st.amount})),
            Action::Propose { seller, buyer, square, price } => out.push(json!({"kind": "offer", "seat": seller, "buyer": buyer, "square": square, "amount": price})),
            Action::Accept { buyer } => out.push(json!({"kind": "offer_accepted", "seat": buyer, "seller": st.other_seat, "square": st.square, "amount": st.amount})),
            Action::Decline { buyer } => out.push(json!({"kind": "offer_declined", "seat": buyer, "seller": st.other_seat, "square": st.square})),
            Action::Reset { seat, pass_turn, .. } => out.push(json!({"kind": if *pass_turn { "regenerate" } else { "reset" }, "seat": seat})),
        }
    }
    out
}

/// Main House → game House transfer. Variables: `operator` (main House, pays the fee) and `house`.
pub fn top_up_manifest(resource: &str, amount: u64, max_fee: u64) -> String {
    format!("fn fee_main() {{\n    let operator = var![\"operator\"];\n    operator.pay_fee({max_fee}u64);\n}}\n\
fn main() {{\n    let operator = var![\"operator\"];\n    let house = var![\"house\"];\n    \
let funds = operator.withdraw(address!(\"{resource}\"), {amount}u64);\n    house.deposit(funds);\n}}\n")
}

/// Field-by-field comparison of the re-read Session with the expected Session.
pub fn state_diffs(after: &Session, expected: &Session) -> Vec<Value> {
    let (a, e) = (serde_json::to_value(after).unwrap_or_default(), serde_json::to_value(expected).unwrap_or_default());
    let (Some(a), Some(e)) = (a.as_object(), e.as_object()) else { return vec![json!({"field": "*", "error": "serialisation"})] };
    e.iter().filter(|(k, v)| a.get(*k) != Some(*v))
        .map(|(k, v)| json!({"field": k, "expected": v, "actual": a.get(k)})).collect()
}

/// Expected vs actual balance change for every account. The House also pays the network fee.
pub fn reconcile(b0: &Balances, b1: &Balances, ledger: &Ledger, fee: u64) -> Value {
    let (v0, v1) = (b0.as_vec(), b1.as_vec());
    let n = b0.seats.len();
    let mut expected: Vec<i128> = ledger.seats.clone();
    expected.extend([ledger.house - i128::from(fee), ledger.community, ledger.chance]);
    let rows: Vec<Value> = (0..n + 3).map(|i| {
        let actual = i128::from(v1[i]) - i128::from(v0[i]);
        let name = if i < n { format!("Seat {}", i + 1) } else { ["House", "Community", "Chance"][i - n].to_owned() };
        json!({"account": name, "expected": expected[i], "actual": actual, "match": expected[i] == actual,
            "expected_ttari": board::ttari_signed(expected[i]), "actual_ttari": board::ttari_signed(actual)})
    }).collect();
    let sum: i128 = (0..n + 3).map(|i| i128::from(v1[i]) - i128::from(v0[i])).sum();
    let ok = rows.iter().all(|r| r["match"] == json!(true)) && sum == -i128::from(fee);
    json!({"ok": ok, "rows": rows, "sum": sum, "fee": fee})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{component, Session};

    fn fixture() -> Session {
        let text = std::fs::read_to_string("tests/fixtures/session.json").unwrap();
        let s = Session::decode(&component(&serde_json::from_str(&text).unwrap()).unwrap().2).unwrap();
        sim::simulate(&s, 500, 500, &Action::Decline { buyer: 2 }).unwrap().after
    }

    #[test]
    fn top_up_manifest_text() {
        let m = top_up_manifest("resource_01", 75_000_000, 1_000_000);
        assert!(m.contains("operator.pay_fee(1000000u64);"));
        assert!(m.contains("operator.withdraw(address!(\"resource_01\"), 75000000u64);"));
        assert!(m.contains("house.deposit(funds);"));
    }

    #[test]
    fn stale_balances_do_not_reconcile_fresh_ones_do() {
        // Turn 301 of v0.3.0: only the House paid the fee (7315); the wallet returned the old balances.
        let b0 = Balances { seats: vec![67_800_000, 1, 2, 3], house: 4_609_726_054, community: 10, chance: 20 };
        let ledger = Ledger { seats: vec![0; 4], house: 0, community: 0, chance: 0 };
        let stale = reconcile(&b0, &b0, &ledger, 7315);
        assert_eq!(stale["ok"], json!(false));
        let mut b1 = b0.clone();
        b1.house = 4_609_718_739; // the vault's revealed_amount after the transaction
        let fresh = reconcile(&b0, &b1, &ledger, 7315);
        assert_eq!(fresh["ok"], json!(true));
        assert_eq!(fresh["sum"], json!(-7315));
    }

    #[test]
    fn news_events_of_a_turn_and_a_build() {
        let mut s = fixture();
        s.build_on_landing = true;
        s.active = 2;
        s.owners[13] = 1; // seat 2 on 3 rolls 6 + 4 → 13, owned by seat 1: rent 2.000000
        let p = sim::plan(&s, 500, 500, &[Action::Turn { seat: 2, die1: 6, die2: 4, pay: false, buy: false, sell_square: None }]).unwrap();
        let ev = events(&p);
        assert_eq!(ev[0]["kind"], "move");
        let rent = ev.iter().find(|e| e["kind"] == "rent").unwrap();
        assert_eq!((rent["owner"].as_u64(), rent["earned"].as_u64(), rent["square"].as_u64()), (Some(1), Some(2_000_000), Some(13)));
        // Seat 2 lands on its own asset 6 (1 + 2 from 3) and builds in the same transaction.
        let mut s2 = fixture();
        s2.build_on_landing = true;
        s2.active = 2;
        s2.owners[6] = 2;
        let p2 = sim::plan(&s2, 500, 500, &[Action::Turn { seat: 2, die1: 1, die2: 2, pay: false, buy: false, sell_square: None },
            Action::Build { seat: 2, square: 6 }]).unwrap();
        let b = events(&p2).into_iter().find(|e| e["kind"] == "build").unwrap();
        assert_eq!((b["square"].as_u64(), b["level"].as_u64(), b["amount"].as_u64()), (Some(6), Some(1), Some(5_000_000)));
    }

    #[test]
    fn receipts_store_reloads_and_skips_bad_lines() {
        let p = std::env::temp_dir().join(format!("oge-receipts-{}.jsonl", std::process::id()));
        let r = Receipt { id: 7, game: 1, round: 2, turn: 3, seat: 1, operation: "play_turn".into(), actor: "player".into(),
            text: "t".into(), transaction_id: "x".into(), fee: Some(1), status: "COMMITTED / RECONCILED".into(), at: 0,
            story: Value::Null, reconciliation: json!({}), cause: None, verification: "reconciled on the first read".into(),
            reads: vec![], state_differences: vec![], steps: vec![], manifest: String::new(), events: vec![] };
        fs::write(&p, format!("{}\nnot json\n", serde_json::to_string(&r).unwrap())).unwrap();
        let v = load_receipts(&p).unwrap();
        assert_eq!((v.len(), v[0].id), (1, 7));
        let _ = fs::remove_file(&p);
    }
}

// End of app/src/ops.rs
// Location: app/src/ops.rs · Created: 2026-11-16 15:40:30 UTC
