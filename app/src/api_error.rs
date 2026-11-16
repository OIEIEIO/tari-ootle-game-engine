//! Ootle Game Engine | app/src/api_error.rs
//! Version: v0.4.0
//! Structured API errors (v0.6.18 scheme): code, category, message, HTTP status, quantified
//! details where the backend computed them, the original cause in `technical_message`, and the
//! submission status. Only conditions emitted by this backend or the contracts are classified.
//! v0.4.0: INVALID_BUSINESS for a purchase naming a business the lot does not offer (catalogue check).

use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::{json, Value};

/// Contract and simulator assert texts → (code, category, message, status).
fn rule(x: &str) -> Option<(&'static str, &'static str, &'static str, StatusCode)> {
    use StatusCode as H;
    let r = |c, k, m, s| Some((c, k, m, s));
    if x.contains("stale turn") || x.contains("stale offer") || x.contains("invalid or stale development level") || x.contains("stale state") {
        return r("GAME_STATE_STALE", "game", "The game state has changed. Refresh and try again.", H::CONFLICT);
    }
    if x.contains("resolve the pending offer first") || x.contains("offer already pending") {
        return r("PENDING_TRADE", "trade", "Resolve the pending property offer before continuing.", H::CONFLICT);
    }
    if x.contains("no pending offer") { return r("NO_PENDING_TRADE", "trade", "There is no pending property offer.", H::CONFLICT); }
    if x.contains("wrong recipient") || x.contains("invalid buyer") || x.contains("price must be positive") {
        return r("INVALID_SALE_REQUEST", "trade", "The property sale request is not valid for these seats or price.", H::BAD_REQUEST);
    }
    if x.contains("does not own") || x.contains("not the seller's asset") || x.contains("ownership changed") {
        return r("ASSET_NOT_OWNED", "asset", "The seat does not own this property.", H::CONFLICT);
    }
    if x.contains("asset already owned") { return r("ASSET_ALREADY_OWNED", "asset", "This property is already owned.", H::CONFLICT); }
    if x.contains("not an asset") { return r("INVALID_ASSET", "asset", "The selected square is not eligible for this action.", H::BAD_REQUEST); }
    if x.contains("fully developed") || x.contains("invalid development level") {
        return r("INVALID_DEVELOPMENT", "asset", "The property cannot be developed further.", H::BAD_REQUEST);
    }
    if x.contains("build only on the asset just landed on") || x.contains("nothing to build") {
        return r("BUILD_NOT_ALLOWED", "asset", "Building is only allowed on the owned asset just landed on, before the next move.", H::CONFLICT);
    }
    if x.contains("pass_turn only") { return r("NOT_SEAT_TURN", "game", "It is not this seat's turn.", H::CONFLICT); }
    if x.contains("not this seat's turn") { return r("NOT_SEAT_TURN", "game", "It is not this seat's turn.", H::CONFLICT); }
    if x.contains("seat is out of the game") { return r("SEAT_OUT", "game", "This seat is out of the game.", H::CONFLICT); }
    if x.contains("held in maintenance") || x.contains("not held in maintenance") || x.contains("own shop fee") {
        return r("MAINTENANCE_STATE", "game", "The Maintenance state does not allow this action.", H::CONFLICT);
    }
    if x.contains("no purchase or sale") { return r("INVALID_TURN_REQUEST", "game", "No purchase or sale is possible on this roll.", H::BAD_REQUEST); }
    if x.contains("invalid dice") { return r("INVALID_DICE", "validation", "Each die must have a value from 1 to 6.", H::BAD_REQUEST); }
    if x.contains("no roll for this turn") { return r("NO_ROLL", "game", "Roll the dice before settling the turn.", H::CONFLICT); }
    if x.contains("game not started") { return r("GAME_NOT_STARTED", "game", "The game has not started.", H::CONFLICT); }
    if x.contains("invalid seat") { return r("INVALID_SEAT", "validation", "The seat number is not part of this game.", H::BAD_REQUEST); }
    if x.contains("insufficient seat funds") {
        return r("PLAYER_INSUFFICIENT_FUNDS", "funds", "The seat does not have enough funds for this transaction.", H::CONFLICT);
    }
    if x.contains("house cannot cover") || x.contains("main house cannot fund") {
        return r("HOUSE_INSUFFICIENT_RESERVE", "treasury", "The Game House cannot cover the required payout and fee reserve.", H::CONFLICT);
    }
    None
}

pub fn classify(reason: &str) -> (&'static str, &'static str, &'static str, StatusCode) {
    use StatusCode as H;
    let s = reason.to_ascii_lowercase();
    let x = s.as_str();
    // Dry-run rejection first: it is the stage that failed; the contract reason goes in details.
    if x.starts_with("dry run rejected") {
        return ("TRANSACTION_DRY_RUN_REJECTED", "transaction",
            "The blockchain rejected the transaction dry run. No live transaction was submitted by this operation.", H::CONFLICT);
    }
    if x.contains("live transactions disabled") {
        return ("LIVE_TESTS_DISABLED", "configuration", "Live transactions are disabled on the server.", H::FORBIDDEN);
    }
    if x.contains("operation in progress") {
        return ("OPERATION_IN_PROGRESS", "system", "Another transaction is being processed. Wait for it to finish.", H::CONFLICT);
    }
    if x.contains("not your seat") {
        return ("SEAT_NOT_YOURS", "seat", "This seat is not held by your browser. Claim a seat first.", H::FORBIDDEN);
    }
    if x.contains("release your seat") {
        return ("ALREADY_SEATED", "seat", "Release your seat in your current game before joining another game.", H::CONFLICT);
    }
    if x.contains("is not open") || x.contains("seats are left for ai") || x.contains("ai opponents are chosen") || x.contains("choose at least one seat") {
        return ("SEAT_NOT_AVAILABLE", "seat", "That seat choice is not available in this game.", H::CONFLICT);
    }
    if x.contains("is not offered on square") || x.contains("is not a lot") {
        return ("INVALID_BUSINESS", "validation", "That business is not offered on this lot. Choose one of the lot's three businesses.", H::BAD_REQUEST);
    }
    if x.contains("player name") || x.contains("invalid player token") || x.contains("invalid vehicle") {
        return ("INVALID_PLAYER", "validation", "The player name, token or vehicle choice is not valid.", H::BAD_REQUEST);
    }
    if x.starts_with("unknown game") { return ("UNKNOWN_GAME", "game", "This game is not in the deployment.", H::NOT_FOUND); }
    if let Some(c) = rule(x) { return c; }
    if x.contains("cookie") && (x.contains("cannot read") || x.contains("empty")) {
        return ("WALLET_COOKIE_UNAVAILABLE", "wallet", "The wallet authentication cookie is unavailable.", H::BAD_GATEWAY);
    }
    if x.contains("wallet auth") || x.contains("unauthorized") {
        return ("WALLET_AUTH_FAILED", "wallet", "Wallet authentication failed.", H::BAD_GATEWAY);
    }
    if x.contains("wallet connection") || x.contains("connection refused") || x.contains("error sending request") {
        return ("WALLET_UNAVAILABLE", "wallet", "The Ootle wallet service is unavailable.", H::BAD_GATEWAY);
    }
    if x.contains("timed out") || x.contains("timeout") {
        return ("WALLET_REQUEST_TIMEOUT", "wallet",
            "The wallet request timed out. Transaction status may be unknown; check chain state before retrying.", H::GATEWAY_TIMEOUT);
    }
    if x.contains("not accepted") { return ("TRANSACTION_REJECTED", "transaction", "The transaction was not accepted by the network.", H::CONFLICT); }
    if x.contains("missing transaction id") {
        return ("TRANSACTION_ID_MISSING", "transaction", "The wallet did not return a transaction identifier. Check wallet status before retrying.", H::BAD_GATEWAY);
    }
    if x.contains("owner key") { return ("SIGNER_UNAVAILABLE", "wallet", "The wallet did not return the signing key for an account.", H::BAD_GATEWAY); }
    if x.contains("substate") || x.contains("layout") || x.contains("template") || x.contains("cbor") || x.contains("expected") {
        return ("CHAIN_STATE_INVALID", "chain", "The on-chain game state could not be read or validated.", H::SERVICE_UNAVAILABLE);
    }
    if x.contains("balance") { return ("BALANCE_READ_FAILED", "wallet", "Unable to read a required wallet balance.", H::BAD_GATEWAY); }
    ("OPERATION_FAILED", "system", "The operation failed. See the technical details for the original error.", H::BAD_REQUEST)
}

fn quantified(reason: &str) -> Value {
    let Some(rest) = reason.split("[available_base_units=").nth(1) else { return json!({}) };
    let Some((a, tail)) = rest.split_once(", required_base_units=") else { return json!({}) };
    let Some(r) = tail.split(']').next() else { return json!({}) };
    match (a.parse::<i128>(), r.parse::<i128>()) {
        (Ok(a), Ok(r)) => json!({"available_base_units": a, "required_base_units": r, "shortfall_base_units": (r - a).max(0)}),
        _ => json!({}),
    }
}

/// For a dry-run rejection, the contract assert that caused it (when it is one we know).
fn contract_reason(reason: &str) -> Value {
    let x = reason.to_ascii_lowercase();
    match rule(&x) {
        Some((code, _, message, _)) if x.starts_with("dry run rejected") => json!({"contract_code": code, "contract_message": message}),
        _ => json!({}),
    }
}

pub fn payload(reason: &str, operation: &str, tx: Option<&str>) -> Value {
    let (code, category, message, _) = classify(reason);
    let mut details = quantified(reason);
    if let (Some(d), Value::Object(c)) = (details.as_object_mut(), contract_reason(reason)) { d.extend(c); }
    let status = match tx { Some(_) => "unknown", None => "not_submitted" };
    json!({"code": code, "category": category, "message": message, "operation": operation, "details": details,
        "technical_message": reason, "transaction_id": tx, "submission_status": status})
}

pub fn response(reason: String, operation: &str, tx: Option<&str>) -> Response {
    let (code, _, _, status) = classify(&reason);
    tracing::warn!(operation, code, cause = %reason, "API operation failed");
    (status, Json(json!({"success": false, "error": payload(&reason, operation, tx)}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantified_details() {
        let v = payload("insufficient seat funds for this turn [available_base_units=1000000, required_base_units=3000000]", "turn", None);
        assert_eq!(v["code"], "PLAYER_INSUFFICIENT_FUNDS");
        assert_eq!(v["details"]["shortfall_base_units"], 2000000);
        assert_eq!(v["submission_status"], "not_submitted");
    }
    #[test]
    fn contract_and_backend_texts() {
        for (m, c) in [
            ("resolve the pending offer first", "PENDING_TRADE"),
            ("not this seat's turn", "NOT_SEAT_TURN"),
            ("seat does not own the sale asset", "ASSET_NOT_OWNED"),
            ("asset already owned", "ASSET_ALREADY_OWNED"),
            ("business pirate-cafe is not offered on square 1", "INVALID_BUSINESS"),
            ("no roll for this turn: roll first", "NO_ROLL"),
            ("house cannot cover the payout [available_base_units=1, required_base_units=2]", "HOUSE_INSUFFICIENT_RESERVE"),
            ("dry run rejected: ... stale turn ...", "TRANSACTION_DRY_RUN_REJECTED"),
            ("Live transactions disabled; set OOTLE_ENABLE_LIVE_TESTS=1", "LIVE_TESTS_DISABLED"),
            ("transactions.wait_result: timed out", "WALLET_REQUEST_TIMEOUT"),
            ("unknown game 9", "UNKNOWN_GAME"),
        ] { assert_eq!(classify(m).0, c, "{m}"); }
        let v = payload("dry run rejected: Panic: stale turn", "turn", None);
        assert_eq!(v["details"]["contract_code"], "GAME_STATE_STALE");
    }
}

// End of app/src/api_error.rs
// Location: app/src/api_error.rs · Created: 2026-11-16 04:06:01 UTC
