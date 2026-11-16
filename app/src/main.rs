//! Ootle Game Engine | app/src/main.rs
//! Version: v0.4.4
//! Axum server on 127.0.0.1: serves the single game page and a JSON API over the backend.
//! Reads are always live from the wallet; writes need OOTLE_ENABLE_LIVE_TESTS=1.
//! v0.3.0: seat claiming (player token in the `x-player-token` header; one game per token),
//! release, reset and regeneration, open games, leaderboard, receiving addresses on the Developer
//! tab, and the AI / auto-roll driver (started only when live transactions are enabled).
//! v0.4.0: the Town Square catalogue (/api/catalogue), the start-gate health check (/api/health),
//! receipts with their verification detail (/api/game/{id}/receipts), the lot store
//! (/api/game/{id}/lots), a purchase's business choice on /turn, and the page's new scripts
//! (pieces.js, scenes.js, audio.js).
//! Run from app/: cargo run --release   (config/network.toml, deployment pointers from the deploy script)

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use ootle_app::{
    ai, api_error, board, chain,
    claims::Holder,
    config::Config,
    dice,
    ops::{self, Actor, App, OpError, Request, Shared},
    sim::Action,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::{Arc, OnceLock}, time::{Duration, Instant}};
use tokio::sync::Mutex;

const INDEX: &str = include_str!("../web/index.html");
const CSS: &str = include_str!("../web/app.css");
const JS: &str = include_str!("../web/app.js");
const INTRO_JS: &str = include_str!("../web/intro.js");
const PIECES_JS: &str = include_str!("../web/pieces.js");
const SCENES_JS: &str = include_str!("../web/scenes.js");
const AUDIO_JS: &str = include_str!("../web/audio.js");

async fn index() -> Html<&'static str> { Html(INDEX) }
async fn css() -> impl IntoResponse { ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS) }
fn script(body: &'static str) -> Response { ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], body).into_response() }
async fn js() -> Response { script(JS) }
async fn intro_js() -> Response { script(INTRO_JS) }
async fn pieces_js() -> Response { script(PIECES_JS) }
async fn scenes_js() -> Response { script(SCENES_JS) }
async fn audio_js() -> Response { script(AUDIO_JS) }

fn ok(v: Value) -> Response { Json(json!({"success": true, "data": v})).into_response() }
fn read(r: Result<Value, String>, op: &str) -> Response { match r { Ok(v) => ok(v), Err(e) => api_error::response(e, op, None) } }
fn write(r: Result<Value, OpError>, op: &str) -> Response {
    match r { Ok(v) => ok(v), Err(e) => api_error::response(e.reason, op, e.tx.as_deref()) }
}

/// The browser's player token (32 hex characters), if sent.
fn token(h: &HeaderMap) -> Option<String> {
    h.get("x-player-token").and_then(|v| v.to_str().ok()).map(str::trim)
        .filter(|t| t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_ascii_lowercase)
}

fn player(h: &HeaderMap) -> Result<Actor, String> {
    token(h).map(Actor::Player).ok_or_else(|| "not your seat: no player token".to_string())
}

fn new_token() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).map_err(|e| format!("random source unavailable: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

async fn info(State(app): State<Shared>) -> Response {
    let d = &app.cfg.deployment;
    let n = &app.cfg.network;
    ok(json!({"name": "Ootle Game Engine", "version": env!("CARGO_PKG_VERSION"), "network": n.network_name,
        "wallet_rpc": app.wallet.url(), "live_transactions_enabled": ops::live_enabled(), "live_env": ops::LIVE_ENV,
        "engine_version": d.engine_version, "resource": d.resource, "max_fee": n.max_fee,
        "decision_window_secs": n.decision_window_secs, "auto_roll_secs": n.auto_roll_secs,
        "ai_move_delay_secs": n.ai_move_delay_secs, "ai_tx_per_minute": n.ai_tx_per_minute,
        "seat_start_ttari": board::ttari(n.seat_start), "house_float_ttari": board::ttari(n.house_float),
        "house_top_up_below_ttari": board::ttari(n.house_top_up_below), "milestone_rounds": n.milestone_rounds,
        "reconcile_reads": n.reconcile_reads, "reconcile_read_delay_ms": n.reconcile_read_delay_ms,
        "receipts_store": n.receipts_store, "lots_store": n.lots_store, "catalogue_file": n.catalogue_file,
        "started_at": app.started_at,
        "seat_colours": board::SEAT_COLOURS, "pieces": board::PIECES,
        "contracts": {"registry": d.registry, "lobby": d.lobby, "treasury": d.treasury},
        "games": d.games.iter().map(|g| json!({"id": g.id, "capacity": g.capacity, "treasury_mode": g.treasury_mode,
            "session": g.session.component})).collect::<Vec<_>>()}))
}

async fn catalogue(State(app): State<Shared>) -> Response { ok(json!(app.catalogue)) }

/// One row per game: round, who holds each seat, open seats, and the leading net worth.
/// v0.4.4: from the game's shared read (ops.rs `snapshot`). The game list (`with_balances` false)
/// shows no balances, so it reads only the game's state; the leaderboard reads both.
async fn game_row(app: &Shared, id: u64, with_balances: bool) -> Value {
    let g = match app.cfg.game(id) { Ok(g) => g.clone(), Err(e) => return json!({"id": id, "readable": false, "error": e}) };
    let read = if with_balances { app.snapshot(id).await.map(|(s, b)| (s, Some(b))) }
               else { app.snapshot_state(id).await.map(|s| (s, None)) };
    let (snap, bal) = match read {
        Ok(r) => r,
        Err(e) => return json!({"id": id, "capacity": g.capacity, "readable": false, "error": e}),
    };
    let s = &snap.session;
    let claims = app.claims.lock().await.seats(id, s.capacity);
    let seats: Vec<Value> = (1..=s.capacity).map(|n| {
        let i = usize::from(n - 1);
        let (kind, name) = match &claims[i].holder {
            Holder::Human { name, .. } => ("human", Some(name.clone())), Holder::Ai => ("ai", None), Holder::Open => ("open", None) };
        let pc = board::piece(n, claims[i].vehicle);
        let net = bal.as_ref().map(|b| b.seats[i] + s.buyback_total(n));
        json!({"seat": n, "kind": kind, "name": name, "piece_id": pc.id, "piece": pc.name, "color": board::seat_colour(n),
            "active": s.active == n, "net_worth": net, "net_worth_ttari": net.map(board::ttari), "position": s.positions[i],
            "stint_rounds": s.round.saturating_sub(s.seat_since[i]), "assets": s.owners.iter().filter(|o| **o == n).count()})
    }).collect();
    let humans = seats.iter().filter(|x| x["kind"] == "human").count();
    let open = seats.iter().filter(|x| x["kind"] == "open").count();
    let fresh = s.round == 1 && s.turn_count == 0;
    json!({"id": id, "capacity": s.capacity, "readable": true, "round": s.round, "turn": s.turn_count, "active": s.active,
        "humans": humans, "open_seats": open, "fresh": fresh, "seats": seats,
        "house_ttari": bal.as_ref().map(|b| board::ttari(b.house)), "build_on_landing": s.build_on_landing})
}

async fn games(State(app): State<Shared>) -> Response {
    let mut set = tokio::task::JoinSet::new();
    for id in app.cfg.game_ids() { let a = app.clone(); set.spawn(async move { game_row(&a, id, false).await }); }
    let mut rows = vec![];
    while let Some(r) = set.join_next().await { if let Ok(v) = r { rows.push(v); } }
    rows.sort_by_key(|r| r["id"].as_u64().unwrap_or(0));
    ok(json!(rows))
}

#[derive(Deserialize)]
struct HealthQuery { game: Option<u64> }
/// Start-gate health check: the wallet answers, the game's four components read and verify, live
/// transactions on or off, catalogue and stores loaded. Each check reports its own time and error.
async fn health(State(app): State<Shared>, Query(q): Query<HealthQuery>) -> Response {
    let game = q.game.filter(|g| app.cfg.game(*g).is_ok()).or_else(|| app.cfg.game_ids().first().copied()).unwrap_or(1);
    let t = Instant::now();
    let w = app.wallet.rpc("substates.get", json!({"substate_id": app.cfg.deployment.lobby.component})).await;
    let wallet = json!({"ok": w.is_ok(), "ms": t.elapsed().as_millis(), "error": w.err()});
    let t = Instant::now();
    let c = chain::read(&app.cfg, &app.wallet, game).await;
    let chain_v = match &c {
        Ok(s) => json!({"ok": true, "ms": t.elapsed().as_millis(), "game": game, "round": s.session.round, "turn": s.session.turn_count,
            "templates_verified": s.templates.iter().all(|x| x.ok)}),
        Err(e) => json!({"ok": false, "ms": t.elapsed().as_millis(), "game": game, "error": e}),
    };
    let cat = &app.catalogue;
    ok(json!({"version": env!("CARGO_PKG_VERSION"), "network": app.cfg.network.network_name, "wallet": wallet, "chain": chain_v,
        "live_transactions_enabled": ops::live_enabled(),
        "catalogue": {"lots": cat.lots.len(), "businesses": cat.lots.iter().map(|l| l.businesses.len()).sum::<usize>(), "specials": cat.specials.len()},
        "receipts_in_memory": app.receipts.lock().await.len(), "at": dice::now()}))
}

async fn me(State(app): State<Shared>, h: HeaderMap) -> Response {
    let Some(t) = token(&h) else { return ok(json!({"token": null, "game": null, "seats": []})) };
    let held = app.claims.lock().await.of_token(&t);
    ok(json!({"token": t, "game": held.as_ref().map(|x| x.0), "seats": held.map(|x| x.1).unwrap_or_default()}))
}

#[derive(Deserialize)]
struct ClaimReq { game: u64, seats: Vec<u8>, #[serde(default)] ai: u8, name: String, #[serde(default)] vehicles: Vec<(u8, u8)> }
async fn claim(State(app): State<Shared>, h: HeaderMap, Json(b): Json<ClaimReq>) -> Response {
    let r = async {
        let g = app.cfg.game(b.game)?.clone();
        let t = match token(&h) { Some(t) => t, None => new_token()? };
        app.claims.lock().await.claim(&t, &b.name, g.id, g.capacity, &b.seats, b.ai, &b.vehicles, dice::now())?;
        Ok(json!({"token": t, "game": g.id, "seats": b.seats}))
    }.await;
    read(r, "claim_seat")
}

async fn release(State(app): State<Shared>, h: HeaderMap) -> Response {
    let r = async {
        let t = token(&h).ok_or("not your seat: no player token")?;
        let game = app.claims.lock().await.release(&t)?;
        Ok(json!({"released": game.is_some(), "game": game}))
    }.await;
    read(r, "release_seat")
}

static BOARD_CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();

/// Live standings of every seat (net worth = cash + assets at buyback value) and the recorded milestones.
async fn leaderboard(State(app): State<Shared>) -> Response {
    let cache = BOARD_CACHE.get_or_init(|| Mutex::new(None));
    if let Some((t, v)) = cache.lock().await.as_ref() { if t.elapsed() < Duration::from_secs(10) { return ok(v.clone()); } }
    let mut set = tokio::task::JoinSet::new();
    for id in app.cfg.game_ids() { let a = app.clone(); set.spawn(async move { game_row(&a, id, true).await }); }
    let mut live = vec![];
    while let Some(Ok(r)) = set.join_next().await {
        if r["readable"] != json!(true) { continue; }
        for s in r["seats"].as_array().cloned().unwrap_or_default() {
            if s["kind"] == "open" { continue; }
            let name = s["name"].as_str().map(str::to_owned).unwrap_or_else(|| format!("AI {}", s["seat"]));
            live.push(json!({"game": r["id"], "seat": s["seat"], "kind": s["kind"], "name": name, "piece_id": s["piece_id"],
                "color": s["color"], "net_worth": s["net_worth"], "net_worth_ttari": s["net_worth_ttari"],
                "stint_rounds": s["stint_rounds"], "round": r["round"]}));
        }
    }
    live.sort_by_key(|x| std::cmp::Reverse(x["net_worth"].as_u64().unwrap_or(0)));
    let mut ms: Vec<Value> = app.board.lock().await.milestones().iter().map(|m| json!({"game": m.game, "seat": m.seat,
        "stint": m.stint, "kind": m.kind, "name": m.name, "rounds": m.rounds, "round": m.round, "net_worth": m.net_worth,
        "net_worth_ttari": board::ttari(m.net_worth), "cash_ttari": board::ttari(m.cash), "at": m.at})).collect();
    ms.sort_by_key(|x| std::cmp::Reverse(x["net_worth"].as_u64().unwrap_or(0)));
    let v = json!({"live": live, "milestones": ms, "milestone_rounds": app.cfg.network.milestone_rounds, "at": dice::now()});
    *cache.lock().await = Some((Instant::now(), v.clone()));
    ok(v)
}

async fn state(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap) -> Response {
    read(app.state(id, token(&h).as_deref()).await, "game_state")
}

#[derive(Deserialize, Default)]
struct RollReq { #[serde(default)] pay: bool }
async fn roll(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, body: Option<Json<RollReq>>) -> Response {
    let r = async {
        let actor = player(&h)?;
        app.roll(id, body.map(|b| b.0).unwrap_or_default().pay, &actor).await?;
        app.state(id, token(&h).as_deref()).await
    }.await;
    read(r, "roll")
}

async fn act(app: &Shared, id: u64, h: &HeaderMap, req: Request, op: &str) -> Response {
    match player(h) { Ok(a) => write(app.execute(id, req, a).await, op), Err(e) => api_error::response(e, op, None) }
}

#[derive(Deserialize)]
struct TurnReq { #[serde(default)] buy: bool, sell_square: Option<u16>, #[serde(default)] build: bool, #[serde(default)] business: Option<String> }
async fn turn(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<TurnReq>) -> Response {
    act(&app, id, &h, Request::Turn { buy: b.buy, sell_square: b.sell_square, build: b.build, business: b.business }, "turn_settlement").await
}

#[derive(Deserialize)]
struct SeatSquare { seat: u8, square: u16 }
async fn sell(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<SeatSquare>) -> Response {
    act(&app, id, &h, Request::Action(Action::Sell { seat: b.seat, square: b.square }), "sell_to_house").await
}
async fn build(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<SeatSquare>) -> Response {
    act(&app, id, &h, Request::Action(Action::Build { seat: b.seat, square: b.square }), "build_improvement").await
}

#[derive(Deserialize)]
struct OfferReq { seller: u8, buyer: u8, square: u16, price: u64 }
async fn offer(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<OfferReq>) -> Response {
    act(&app, id, &h, Request::Action(Action::Propose { seller: b.seller, buyer: b.buyer, square: b.square, price: b.price }), "propose_sale").await
}

#[derive(Deserialize)]
struct BuyerReq { buyer: u8 }
async fn accept(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<BuyerReq>) -> Response {
    act(&app, id, &h, Request::Action(Action::Accept { buyer: b.buyer }), "accept_sale").await
}
async fn decline(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<BuyerReq>) -> Response {
    act(&app, id, &h, Request::Action(Action::Decline { buyer: b.buyer }), "decline_sale").await
}

#[derive(Deserialize)]
struct GiftReq { payer: u8, held_seat: u8 }
async fn shop_fee(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<GiftReq>) -> Response {
    act(&app, id, &h, Request::Action(Action::PayShopFee { payer: b.payer, held_seat: b.held_seat }), "pay_shop_fee").await
}

#[derive(Deserialize)]
struct SeatReq { seat: u8 }
async fn reset(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<SeatReq>) -> Response {
    act(&app, id, &h, Request::Reset { seat: b.seat, pass_turn: false }, "reset_seat").await
}
async fn regenerate(State(app): State<Shared>, Path(id): Path<u64>, h: HeaderMap, Json(b): Json<SeatReq>) -> Response {
    act(&app, id, &h, Request::Reset { seat: b.seat, pass_turn: true }, "regenerate").await
}

async fn history(State(app): State<Shared>, Path(id): Path<u64>) -> Response { ok(json!(app.history(id).await)) }

#[derive(Deserialize)]
struct ReceiptsQuery { limit: Option<usize>, unverified: Option<bool> }
/// Developer tab: the game's receipts with their full detail (verification reads, field differences,
/// pipeline steps, manifest, news events), newest first. `unverified=true` keeps only UNVERIFIED ones.
async fn receipts(State(app): State<Shared>, Path(id): Path<u64>, Query(q): Query<ReceiptsQuery>) -> Response {
    let limit = q.limit.unwrap_or(100).clamp(1, 1000);
    let only_bad = q.unverified.unwrap_or(false);
    let v: Vec<_> = app.receipts.lock().await.iter().rev().filter(|r| r.game == id)
        .filter(|r| !only_bad || r.status.contains("UNVERIFIED")).take(limit).cloned().collect();
    ok(json!(v))
}

/// Developer tab: the lot store for this game (square → business, who chose it and how).
async fn lots(State(app): State<Shared>, Path(id): Path<u64>) -> Response {
    let l = app.lots.lock().await.game(id);
    let rows: Vec<Value> = l.iter().map(|(sq, c)| {
        let b = app.catalogue.business(*sq, &c.business);
        json!({"square": sq, "zone": app.catalogue.lot(*sq).map(|x| x.zone.clone()), "business": c.business,
            "name": b.map(|x| x.name.clone()), "seat": c.seat, "source": c.source, "at": c.at})
    }).collect();
    ok(json!(rows))
}

/// Receiving address and component of every account the deployment uses (Developer tab).
async fn addresses(State(app): State<Shared>, Path(id): Path<u64>) -> Response {
    let g = match app.cfg.game(id) { Ok(g) => g.clone(), Err(e) => return api_error::response(e, "addresses", None) };
    let mut list: Vec<(String, String)> = vec![("Main House (operator)".into(), app.cfg.deployment.treasury.global_house.clone()),
        (format!("Game {id} House"), g.house.clone()), (format!("Game {id} Community"), g.community.clone()),
        (format!("Game {id} Chance"), g.chance.clone())];
    list.extend(g.seats.iter().map(|s| (format!("Game {id} Seat {}", s.seat), s.component.clone())));
    let mut set = tokio::task::JoinSet::new();
    for (i, (label, comp)) in list.into_iter().enumerate() {
        let w = app.wallet.clone();
        let res = app.cfg.deployment.resource.clone();
        set.spawn(async move {
            let (addr, bal) = tokio::join!(w.receiving_address(&comp), w.balance(&comp, &res));
            (i, json!({"label": label, "component": comp, "receiving_address": addr, "balance_ttari": bal.ok().map(board::ttari)}))
        });
    }
    let mut rows = vec![];
    while let Some(Ok(r)) = set.join_next().await { rows.push(r); }
    rows.sort_by_key(|r| r.0);
    ok(json!(rows.into_iter().map(|r| r.1).collect::<Vec<_>>()))
}

/// Developer tab: decoded contracts, template checks, last pipeline, receipts with reconciliation.
async fn dev(State(app): State<Shared>, Path(id): Path<u64>) -> Response {
    let snap = match chain::read(&app.cfg, &app.wallet, id).await { Ok(s) => s, Err(e) => return api_error::response(e, "developer", None) };
    let bal = chain::balances(&app.wallet, &snap.game, &snap.session.resource).await;
    let receipts: Vec<_> = app.receipts.lock().await.iter().rev().filter(|r| r.game == id).take(25).cloned().collect();
    let pipeline = app.pipelines.lock().await.get(&id).cloned();
    let n = &app.cfg.network;
    ok(json!({"pointers": snap.game, "contracts": {"registry": app.cfg.deployment.registry, "lobby": app.cfg.deployment.lobby,
            "treasury": app.cfg.deployment.treasury}, "templates": snap.templates,
        "session": snap.session, "session_version": snap.session_version, "lobby": snap.lobby, "treasury": snap.treasury,
        "registry": snap.registry, "balances": bal.as_ref().ok(), "balances_error": bal.err(),
        "pipeline": pipeline, "receipts": receipts,
        "verification": {"reconcile_reads": n.reconcile_reads, "reconcile_read_delay_ms": n.reconcile_read_delay_ms,
            "receipts_store": n.receipts_store},
        "raw": snap.raw.iter().map(|(k, v)| json!({"contract": k, "substate": v})).collect::<Vec<_>>()}))
}

#[derive(Deserialize)]
struct InspectReq { transaction_id: String }
async fn inspect(State(app): State<Shared>, Json(b): Json<InspectReq>) -> Response {
    let id = b.transaction_id.trim();
    if id.is_empty() || id.len() > 128 || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return api_error::response("invalid transaction id".into(), "inspect", None);
    }
    read(app.wallet.rpc("transactions.get", json!({"transaction_id": id})).await, "inspect")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into())).init();
    let dir = std::env::current_dir().expect("current directory");
    let cfg = match Config::load(&dir) { Ok(c) => c, Err(e) => { eprintln!("configuration error: {e}"); std::process::exit(2); } };
    let addr: SocketAddr = format!("{}:{}", cfg.network.listen_host, cfg.network.port).parse().expect("listen address");
    let app: Shared = Arc::new(App::new(cfg).unwrap_or_else(|e| { eprintln!("startup error: {e}"); std::process::exit(2); }));
    let router = Router::new()
        .route("/", get(index))
        .route("/app.css", get(css))
        .route("/app.js", get(js))
        .route("/intro.js", get(intro_js))
        .route("/pieces.js", get(pieces_js))
        .route("/scenes.js", get(scenes_js))
        .route("/audio.js", get(audio_js))
        .route("/api/system/info", get(info))
        .route("/api/health", get(health))
        .route("/api/catalogue", get(catalogue))
        .route("/api/games", get(games))
        .route("/api/me", get(me))
        .route("/api/claim", post(claim))
        .route("/api/release", post(release))
        .route("/api/leaderboard", get(leaderboard))
        .route("/api/game/{id}/state", get(state))
        .route("/api/game/{id}/roll", post(roll))
        .route("/api/game/{id}/turn", post(turn))
        .route("/api/game/{id}/sell", post(sell))
        .route("/api/game/{id}/build", post(build))
        .route("/api/game/{id}/offer", post(offer))
        .route("/api/game/{id}/offer/accept", post(accept))
        .route("/api/game/{id}/offer/decline", post(decline))
        .route("/api/game/{id}/pay-shop-fee", post(shop_fee))
        .route("/api/game/{id}/reset", post(reset))
        .route("/api/game/{id}/regenerate", post(regenerate))
        .route("/api/game/{id}/history", get(history))
        .route("/api/game/{id}/receipts", get(receipts))
        .route("/api/game/{id}/lots", get(lots))
        .route("/api/game/{id}/addresses", get(addresses))
        .route("/api/game/{id}/dev", get(dev))
        .route("/api/tx/inspect", post(inspect))
        .with_state(app.clone());
    let n = &app.cfg.network;
    tracing::info!(%addr, live = ops::live_enabled(), network = %n.network_name, games = app.cfg.game_ids().len(),
        auto_roll_secs = n.auto_roll_secs, ai_move_delay_secs = n.ai_move_delay_secs, ai_tx_per_minute = n.ai_tx_per_minute,
        reconcile_reads = n.reconcile_reads, reconcile_read_delay_ms = n.reconcile_read_delay_ms,
        receipts_loaded = app.receipts.lock().await.len(), "Ootle Game Engine listening");
    ai::spawn(app.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, router).await.expect("server");
}

// End of app/src/main.rs
// Location: app/src/main.rs · Created: 2026-11-16 15:40:30 UTC
