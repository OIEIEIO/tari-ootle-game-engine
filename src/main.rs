//! Ootle Asset Game | src/main.rs
//! Axum development server: thin HTTP orchestration over Rust business logic.
//! Notes: localhost-only; all financial state is on-chain; test runner supports dry-run.

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use ootle_asset_game::{
    config::Config,
    ootle, contract_monitor, game_session::{self, TurnRequest, TradeRequest, ImprovementRequest}, diagnostics::{self, RunRequest, InspectRequest, TestRecord},
};
use serde_json::{json, Value};
use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, path::Path, sync::Arc};
use tokio::sync::Mutex;
use tracing::{error, info};

const INDEX: &str = include_str!("../web/index.html");
const GAME: &str = include_str!("../web/game.html");
const GAME_CSS: &str = include_str!("../web/game.css");
const GAME_JS: &str = include_str!("../web/game.js");
const ASSET_A_IMAGE: &str = include_str!("../web/assets/asset-a-storefront.svg");
const ASSET_F_IMAGE: &str = include_str!("../web/assets/asset-f-bistro.svg");
const ASSET_L_IMAGE: &str = include_str!("../web/assets/asset-l-lounge.svg");
const CSS: &str = include_str!("../web/style.css");
const JS: &str = include_str!("../web/app.js");

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    test_history: Arc<Mutex<Vec<TestRecord>>>,
    game_history: Arc<Mutex<Vec<Value>>>,
    live_test_lock: Arc<Mutex<()>>,
}

async fn index() -> Html<&'static str> { Html(INDEX) }
async fn game_page() -> Html<&'static str> { Html(GAME) }
async fn game_css() -> impl IntoResponse { ([(header::CONTENT_TYPE,"text/css; charset=utf-8")],GAME_CSS) }
async fn game_js() -> impl IntoResponse { ([(header::CONTENT_TYPE,"text/javascript; charset=utf-8")],GAME_JS) }
async fn asset_a_image() -> impl IntoResponse { ([(header::CONTENT_TYPE,"image/svg+xml; charset=utf-8")],ASSET_A_IMAGE) }
async fn asset_f_image() -> impl IntoResponse { ([(header::CONTENT_TYPE,"image/svg+xml; charset=utf-8")],ASSET_F_IMAGE) }
async fn asset_l_image() -> impl IntoResponse { ([(header::CONTENT_TYPE,"image/svg+xml; charset=utf-8")],ASSET_L_IMAGE) }
async fn css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS)
}
async fn js() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], JS)
}
async fn info_api(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "name": "Ootle Asset Game",
        "version": env!("CARGO_PKG_VERSION"),
        "build_mode": "development",
        "wallet_rpc": state.config.network.wallet_rpc_url,
        "wallet_access": "live chain reads, dry-run and operator-enabled live transactions",
        "simulation": false,
        "live_transactions_enabled": std::env::var("OOTLE_ENABLE_LIVE_TESTS").as_deref() == Ok("1"),
        "contract_template_included": true,
        "game_mode": "on-chain sequential economic turns and asset ownership",
        "contract_execution_verified": "verified by operator receipts; network-specific",
    }))
}
/// Report the optional local WASM artifact separately from verified on-chain deployment.
/// The backend queries the wallet's finalized publication and instantiation receipts.
async fn template_status(State(state): State<AppState>) -> Json<Value> {
    let wasm = Path::new("templates/treasury_controller/target/wasm32-unknown-unknown/release/ootle_treasury_controller.wasm");
    let metadata = std::fs::read_to_string("config/deployment.toml").unwrap_or_default();
    let field = |key: &str| -> Option<String> {
        metadata.lines().find_map(|line| {
            let (k, v) = line.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
        }).filter(|value| !value.is_empty())
    };
    let template = field("template_address");
    let component = field("component_address");
    let publication_id = field("publication_transaction_id");
    let instantiation_id = field("instantiation_transaction_id");
    let local = wasm.exists();
    let mut result = json!({
        "template_binary_present":local,
        "template_binary_path":wasm.to_string_lossy(),
        "template_address_configured":template,
        "component_address_configured":component,
        "template_published":false,
        "component_deployed":false,
        "deployment_verified_on_chain":false,
        "verification_error":null,
        "operation":"Querying finalized publication and instantiation receipts from local wallet"
    });
    let check = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build().map_err(|e|e.to_string())?;
        let url = &state.config.network.wallet_rpc_url;
        let request = |method: &str, params: Value, token: Option<&str>| {
            let builder = client.post(url).json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
            if let Some(token) = token {builder.bearer_auth(token)} else {builder}
        };
        let token = ootle_asset_game::wallet_auth::token(&client, url).await?;
        let query = |txid: &str| request("transactions.get_result",json!({"transaction_id":txid}),Some(&token));
        let pub_id = publication_id.as_deref().ok_or("missing publication transaction ID")?;
        let inst_id = instantiation_id.as_deref().ok_or("missing instantiation transaction ID")?;
        let publication: Value = query(pub_id).send().await.map_err(|e|e.to_string())?
            .json().await.map_err(|e|e.to_string())?;
        let instantiation: Value = query(inst_id).send().await.map_err(|e|e.to_string())?
            .json().await.map_err(|e|e.to_string())?;
        for response in [&publication,&instantiation] {
            if let Some(err)=response.get("error") {return Err(format!("wallet transaction lookup: {err}"));}
        }
        let accepted = |v: &Value| v.pointer("/result/status").and_then(Value::as_str)==Some("Accepted")
            && v.pointer("/result/result/result/Accept").is_some();
        let has_event = |v: &Value, topic: &str, id: &str| v.pointer("/result/result/events")
            .and_then(Value::as_array).is_some_and(|events|events.iter().any(|e|{
                e.get("topic").and_then(Value::as_str)==Some(topic)
                    && e.get("substate_id").and_then(Value::as_str)==Some(id)
            }));
        let template_id = format!("template_{}",template.as_deref().ok_or("template address not configured")?);
        let component_id = component.as_deref().ok_or("component address not configured")?;
        let published = accepted(&publication) && has_event(&publication,"std.template.publish",&template_id);
        let deployed = accepted(&instantiation) && has_event(&instantiation,"std.component.created",component_id)
            && instantiation.pointer("/result/result/events").and_then(Value::as_array)
            .is_some_and(|events|events.iter().any(|e|e.get("substate_id").and_then(Value::as_str)==Some(component_id)
                && e.get("template_address").and_then(Value::as_str)==template.as_deref()));
        Ok::<(bool,bool),String>((published,deployed))
    }.await;
    match check {
        Ok((published,deployed)) => {
            result["template_published"] = json!(published);
            result["component_deployed"] = json!(deployed);
            result["deployment_verified_on_chain"] = json!(published && deployed);
            result["operation"] = json!("Finalized transaction receipts checked through wallet JSON-RPC");
        }
        Err(err) => {
            result["verification_error"] = json!(err);
            result["operation"] = json!("On-chain verification unavailable; local WASM status is separate");
        }
    }
    // Read current controller state independently of the historical creation receipt.
    // If the wallet cannot provide a full remote substate, report the failure instead
    // of displaying configured values as though they were live.
    if result["component_deployed"] == true {
        if let (Some(component_id), Some(template_id)) = (component.as_deref(), template.as_deref()) {
            match contract_monitor::fetch_controller(&state.config, component_id, template_id).await {
                Ok(snapshot) => result["live_controller"] = snapshot,
                Err(err) => result["live_controller_error"] = json!(err),
            }
        }
    }
    // Test history belongs to this process; it is not a chain-wide event index.
    let history = state.test_history.lock().await;
    if let Some(record) = history.iter().rev().find(|r| r.mode == "live" && r.transaction_id.is_some()) {
        result["last_observed_transaction"] = json!({
            "transaction_id":record.transaction_id,
            "status":record.status,
            "verdict":record.verdict,
            "title":record.title,
            "fee_base_units":record.fee_base_units,
            "source":"current dashboard server session (not chain-wide latest)",
        });
    }
    Json(result)
}

/// Fixed, server-owned test catalog; browser cannot supply arbitrary manifest code.
async fn test_catalog() -> Json<Value> { Json(diagnostics::catalog()) }
async fn test_history(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.test_history.lock().await.clone()))
}
async fn run_test(State(state): State<AppState>, Json(request): Json<RunRequest>) -> Response {
    let _test_guard = state.live_test_lock.lock().await;
    let seq = {state.test_history.lock().await.len() as u64 + 1};
    match diagnostics::run(&state.config, &request, seq).await {
        Ok(record) => {
            let mut history = state.test_history.lock().await;
            history.push(record.clone());
            if history.len() > 100 { history.remove(0); }
            Json(json!(record)).into_response()
        },
        Err(err) => (StatusCode::BAD_REQUEST, Json(json!({"error":err}))).into_response(),
    }
}
async fn inspect_transaction(State(state): State<AppState>, Json(request): Json<InspectRequest>) -> Response {
    match diagnostics::inspect(&state.config, &request).await {
        Ok(data) => Json(data).into_response(),
        Err(err) => (StatusCode::BAD_GATEWAY, Json(json!({"error":err}))).into_response(),
    }
}

async fn live_balances(State(state): State<AppState>) -> Response {
    match ootle::fetch_balances(&state.config).await {
        Ok(data) => Json(json!(data)).into_response(),
        Err(e) => {
            error!("live read-only query failed: {}", e);
            (StatusCode::BAD_GATEWAY, Json(json!({"error":e}))).into_response()
        }
    }
}

/// Live game state is always fetched from the blockchain, not local process memory.
async fn game_history(State(state): State<AppState>) -> Json<Value> {
    Json(json!({"source":"live transaction receipts, current server process only", "turns":state.game_history.lock().await.clone()}))
}

async fn game_state(State(state): State<AppState>) -> Response {
    match game_session::fetch_state(&state.config).await {
        Ok(view) => Json(json!({"deployed":true,"game":view})).into_response(),
        Err(err) => (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":err,"deployed":false}))).into_response(),
    }
}

/// One player turn = one atomic economic transaction. Serialized within this process.
async fn game_turn(State(state): State<AppState>, Json(request): Json<TurnRequest>) -> Response {
    let _guard=state.live_test_lock.lock().await;
    match game_session::execute_turn(&state.config,&request).await {
        Ok(result) => {
            // Store the actual receipt, not a simulated ledger event. This history is
            // process-local; game state itself remains authoritative on-chain.
            let mut history = state.game_history.lock().await;
            history.push(result.clone());
            if history.len() > 100 { history.remove(0); }
            Json(result).into_response()
        },
        Err(err) => (StatusCode::BAD_REQUEST,Json(json!({"error":err}))).into_response(),
    }
}

/// Serialized with turns: all offer transitions read and settle against chain state.
async fn game_trade(State(state): State<AppState>, Json(request): Json<TradeRequest>) -> Response {
    let _guard=state.live_test_lock.lock().await;
    match game_session::execute_trade(&state.config,&request).await {
        Ok(receipt)=>Json(receipt).into_response(),
        Err(err)=>(StatusCode::BAD_REQUEST,Json(json!({"error":err}))).into_response(),
    }
}

async fn game_build(State(state): State<AppState>, Json(request): Json<ImprovementRequest>) -> Response {
    let _guard=state.live_test_lock.lock().await;
    match game_session::execute_improvement(&state.config,&request).await {
        Ok(receipt)=>Json(receipt).into_response(),
        Err(err)=>(StatusCode::BAD_REQUEST,Json(json!({"error":err}))).into_response(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "ootle_asset_game=info,tower_http=info".into())
    ).init();
    let config_dir = std::env::var("OOTLE_CONFIG_DIR").unwrap_or_else(|_| "config".to_owned());
    let config = Config::load(Path::new(&config_dir))?;
    let preferred = config.network.preferred_port;
    let state = AppState { config: Arc::new(config), test_history: Arc::new(Mutex::new(Vec::new())), game_history: Arc::new(Mutex::new(Vec::new())), live_test_lock: Arc::new(Mutex::new(())) };
    let wallet_url = state.config.network.wallet_rpc_url.clone();
    let app = Router::new()
        .route("/", get(game_page))
        .route("/developer", get(index))
        .route("/game.css", get(game_css))
        .route("/game.js", get(game_js))
        .route("/assets/asset-a-storefront.svg", get(asset_a_image))
        .route("/assets/asset-f-bistro.svg", get(asset_f_image))
        .route("/assets/asset-l-lounge.svg", get(asset_l_image))
        .route("/api/game/state", get(game_state))
        .route("/api/game/turn", post(game_turn))
        .route("/api/game/trade", post(game_trade))
        .route("/api/game/build", post(game_build))
        .route("/api/game/history", get(game_history))
        .route("/style.css", get(css))
        .route("/app.js", get(js))
        .route("/api/system/info", get(info_api))
        .route("/api/live/balances", get(live_balances))
        .route("/api/contract/status", get(template_status))
        .route("/api/tests/catalog", get(test_catalog))
        .route("/api/tests/history", get(test_history))
        .route("/api/tests/run", post(run_test))
        .route("/api/transactions/inspect", post(inspect_transaction))
        .with_state(state);
    let host = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let preferred_addr = SocketAddr::new(host, preferred);
    let listener = match tokio::net::TcpListener::bind(preferred_addr).await {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            info!("Port {} busy; requesting OS-selected free port", preferred);
            tokio::net::TcpListener::bind(SocketAddr::new(host, 0)).await?
        }
        Err(error) => return Err(error.into()),
    };
    let actual_addr = listener.local_addr()?;
    println!("\nOotle Asset Game v{}", env!("CARGO_PKG_VERSION"));
    println!("Dashboard: http://{actual_addr}");
    println!("Simulation: disabled; on-chain state only");
    println!("Ootle: live wallet RPC {}", wallet_url);
    println!("Template: on-chain controller; test API (live opt-in: {})\n", std::env::var("OOTLE_ENABLE_LIVE_TESTS").as_deref()==Ok("1"));
    axum::serve(listener, app).await?;
    Ok(())
}

// End of src/main.rs
