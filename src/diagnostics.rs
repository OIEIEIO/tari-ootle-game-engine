//! Ootle Asset Game | src/diagnostics.rs
//! Source of truth for diagnostic classifications and fixed test manifests.
//! JSON-RPC execution is performed exclusively on the Rust backend.
use crate::config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub const CONTROLLER: &str = "component_179d6614c4e42a4742f2f499067ea0a5b7168c06700583057eddb73ffe3b2af0";
const MAX_FEE: u64 = 1_000_000;


#[derive(Clone, Deserialize)]
pub struct RunRequest {
    pub test_id: String,
    #[serde(default)] pub mode: String,
    #[serde(default)] pub confirmation: String,
}

#[derive(Clone, Deserialize)]
pub struct InspectRequest { pub transaction_id: String }

#[derive(Clone, Serialize)]
pub struct TestRecord {
    pub sequence: u64,
    pub test_id: String,
    pub mode: String,
    pub verdict: String,
    pub code: String,
    pub title: String,
    pub message: String,
    pub transaction_id: Option<String>,
    pub status: Option<String>,
    pub fee_base_units: Option<u64>,
    pub expected_rejection: bool,
    pub reconciliation: Option<Value>,
    pub evidence: Value,
}

pub fn catalog() -> Value {
    json!([
        {"id":"sale","label":"Asset sale · 90/5/5","kind":"distribution","description":"Withdraw 10 tTARI and inspect all 3 treasury deposits"},
        {"id":"sponsor","label":"Sponsorship · 80/15/5","kind":"distribution","description":"Verify the sponsorship allocation"},
        {"id":"rounding","label":"Integer rounding · 101 units","kind":"accounting","description":"Verify 91 + 5 + 5 base units and conservation"},
        {"id":"mismatch","label":"Amount mismatch rejection","kind":"validation","description":"Withdraw 10 but claim 11 tTARI"},
        {"id":"category","label":"Invalid category rejection","kind":"validation","description":"Use unsupported category 99"},
        {"id":"zero","label":"Zero revenue rejection","kind":"validation","description":"Supply 0 base units"}
    ])
}

fn test_config(id: &str) -> Result<(u8,u64,u64,bool,&'static str), String> {
    match id {
        "sale" => Ok((0,10_000_000,10_000_000,false,"")),
        "sponsor" => Ok((2,10_000_000,10_000_000,false,"")),
        "rounding" => Ok((0,101,101,false,"")),
        "mismatch" => Ok((0,10_000_000,11_000_000,true,"amount does not match bucket")),
        "category" => Ok((99,10_000_000,10_000_000,true,"unknown category")),
        "zero" => Ok((0,0,0,true,"zero revenue")),
        _ => Err("Unknown test ID".into()),
    }
}

fn manifest(id: &str, config: &Config) -> Result<String, String> {
    let (category, withdraw, claimed, _, _) = test_config(id)?;
    let resource = &config.accounts.resource_address;
    if !resource.starts_with("resource_") || resource.len() != 73 { return Err("Invalid resource address".into()); }
    Ok(format!(r#"fn fee_main() {{
    let house = var!["house"];
    house.pay_fee({MAX_FEE}u64);
}}
fn main() {{
    let house = var!["house"];
    let controller = var!["controller"];
    let revenue = house.withdraw(address!("{resource}"), {withdraw}u64);
    controller.distribute({category}u8, revenue, {claimed}u64);
}}"#, MAX_FEE=MAX_FEE, resource=resource, withdraw=withdraw, category=category, claimed=claimed))
}

async fn rpc(client: &reqwest::Client, url: &str, method: &str, params: Value, bearer: Option<&str>) -> Result<Value, String> {
    let mut req = client.post(url).json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
    if let Some(token) = bearer { req = req.bearer_auth(token); }
    let response = req.send().await.map_err(|e| format!("RPC transport: {e}"))?;
    response.json::<Value>().await.map_err(|e| format!("RPC JSON: {e}"))
}

async fn authorized(config: &Config) -> Result<(reqwest::Client,String),String> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(150)).build().map_err(|e| e.to_string())?;
    let auth = rpc(&client, &config.network.wallet_rpc_url,"auth.request",json!({"permissions":["Admin"],"credentials":"None"}),None).await?;
    if let Some(error) = auth.get("error") { return Err(format!("Auth failed: {error}")); }
    let token = auth.pointer("/result/token").and_then(Value::as_str).ok_or("Missing auth token")?.to_owned();
    Ok((client,token))
}

pub fn classify(status: Option<&str>, error: Option<&str>, timed_out: bool) -> (&'static str,&'static str) {
    if timed_out {return ("TRANSACTION_TIMEOUT", "Finalization not confirmed");}
    if let Some(e) = error {
        if e.contains("amount does not match bucket") {return ("TREASURY_AMOUNT_MISMATCH","Revenue amount mismatch");}
        if e.contains("unknown category") {return ("TREASURY_INVALID_CATEGORY","Invalid revenue category");}
        if e.contains("zero revenue") {return ("TREASURY_ZERO_REVENUE","Zero revenue rejected");}
        if e.contains("incorrect resource") {return ("TREASURY_INVALID_RESOURCE","Incorrect token resource");}
        if e.contains("Unauthorized") || e.contains("401") {return ("RPC_AUTH_FAILED","Wallet authorization failed");}
        return ("CONTRACT_REJECTED","Transaction execution rejected");
    }
    match status {
        Some("OnlyFeeAccepted") => ("TRANSACTION_FEE_ONLY","Contract rejected; fee committed"),
        Some("Accepted") => ("TRANSACTION_ACCEPTED","Transaction accepted"),
        _ => ("TRANSACTION_UNCONFIRMED","Transaction status requires inspection"),
    }
}

fn fmt_error(error: &Value) -> String { error.get("message").and_then(Value::as_str).unwrap_or("Unknown RPC error").to_owned() }

/// Fetch the three treasury account balances independently from the manifest execution.
/// Missing resources and failed wallet queries are errors, not zero balances.
async fn treasury_balances(config: &Config, client: &reqwest::Client, token: &str) -> Result<[u64; 3], String> {
    let mut balances = [0u64; 3];
    for (index, role) in ["house", "community", "chance"].iter().enumerate() {
        let account = config.accounts.accounts.iter()
            .find(|a| a.id == *role).ok_or_else(|| format!("Missing {role} account"))?;
        let reply = rpc(client, &config.network.wallet_rpc_url, "accounts.get_balances",
            json!({"account":account.name,"refresh":true}), Some(token)).await?;
        if let Some(error) = reply.get("error") {
            return Err(format!("{role} balance query: {}", fmt_error(error)));
        }
        let entries = reply.pointer("/result/balances").and_then(Value::as_array)
            .ok_or_else(|| format!("{role}: missing balances"))?;
        let matching: Vec<_> = entries.iter()
            .filter(|entry|entry.get("resource_address").and_then(Value::as_str)
                == Some(config.accounts.resource_address.as_str())).collect();
        if matching.len() != 1 { return Err(format!("{role}: expected one matching resource, found {}",matching.len())); }
        let amount = matching[0].get("balance").and_then(Value::as_str)
            .ok_or_else(|| format!("{role}: missing balance value"))?;
        balances[index] = amount.parse().map_err(|e| format!("{role}: invalid balance: {e}"))?;
    }
    Ok(balances)
}

fn reconcile(before: [u64;3], after: [u64;3], fee: u64, scenario: &str, accepted: bool, fee_only: bool) -> Value {
    let expected_deposits: [u64; 3] = match scenario {
        "sale" => [9_000_000,500_000,500_000],
        "sponsor" => [8_000_000,1_500_000,500_000],
        "rounding" => [91,5,5],
        _ => [0,0,0],
    };
    let revenue: i128 = match scenario { "rounding" => 101, "zero" => 0, _ => 10_000_000 };
    let actual: Vec<i128> = (0..3).map(|i| i128::from(after[i]) - i128::from(before[i])).collect();
    let expected = if accepted {
        vec![i128::from(expected_deposits[0])-revenue-i128::from(fee),
             i128::from(expected_deposits[1]),i128::from(expected_deposits[2])]
    } else if fee_only {
        vec![-i128::from(fee),0,0]
    } else { vec![0,0,0] };
    let passed = (accepted || fee_only) && actual == expected;
    json!({"verified":passed,"before_base_units":before,"after_base_units":after,
        "actual_delta_base_units":actual,"expected_delta_base_units":expected,
        "labels":["Game House","Community Treasury","Chance Treasury"],
        "fee_base_units":fee,"comparison":"independently refreshed wallet balances, inclusive of final fee"})
}

pub async fn run(config: &Config, request: &RunRequest, sequence: u64) -> Result<TestRecord, String> {
    let (_,withdraw,_,reject_expected,expected_message) = test_config(&request.test_id)?;
    let live = request.mode == "live";
    if request.mode != "dry" && request.mode != "live" {return Err("Mode must be dry or live".into());}
    if live {
        if std::env::var("OOTLE_ENABLE_LIVE_TESTS").as_deref() != Ok("1") { return Err("Live tests disabled: set OOTLE_ENABLE_LIVE_TESTS=1 at server startup".into()); }
        if request.confirmation != format!("LIVE {}", request.test_id) {return Err(format!("Confirmation required: LIVE {}",request.test_id));}
    }
    let manifest = manifest(&request.test_id,config)?;
    let house = config.accounts.accounts.iter().find(|a| a.id == "house").ok_or("House missing")?;
    let (client, token) = authorized(config).await?;
    // A live operation must have a trustworthy baseline before submission.
    let before = if live { Some(treasury_balances(config, &client, &token).await?) } else { None };
    let response = rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",json!({
        "manifest":manifest,
        "variables":{"house":house.component_address,"controller":CONTROLLER},
        "seal_signer_key_id":{"Derived":{"index":3,"key_branch":"account"}},"signing_key_ids":[],
        "max_fee":MAX_FEE,"dry_run":!live,"blobs":{}
    }),Some(&token)).await?;

    let mut id = None;
    let mut status = None;
    let mut fee = None;
    let mut error_text = None;
    let mut timed_out = false;
    let execution: Value;
    if let Some(err) = response.get("error") {
        error_text = Some(fmt_error(err));
        execution = json!({"rpc_error":err});
    } else {
        let result = response.get("result").ok_or("Missing result")?;
        id = result.get("transaction_id").and_then(Value::as_str).map(str::to_owned);
        if live {
            let txid = id.as_deref().ok_or("No transaction ID")?;
            let waited = rpc(&client,&config.network.wallet_rpc_url,"transactions.wait_result",json!({"transaction_id":txid,"timeout_secs":120}),Some(&token)).await?;
            if let Some(err) = waited.get("error") { error_text = Some(fmt_error(err)); execution = waited; }
            else {
                let finished = waited.get("result").ok_or("Missing wait result")?;
                status = finished.get("status").and_then(Value::as_str).map(str::to_owned);
                fee = finished.get("final_fee").and_then(Value::as_u64);
                timed_out = finished.get("timed_out").and_then(Value::as_bool).unwrap_or(false);
                execution = finished.get("result").cloned().unwrap_or(Value::Null);
            }
        } else {
            fee = result.get("required_fees").and_then(Value::as_u64);
            execution = result.get("result").cloned().unwrap_or(Value::Null);
            status = Some(if execution.pointer("/finalize/result/Accept").is_some() {"Accepted"} else {"Unknown"}.into());
        }
    }
    let outcome = if live {execution.get("result")} else {execution.pointer("/finalize/result")};
    let accepted = outcome.and_then(|o|o.get("Accept")).is_some() && (!live || status.as_deref()==Some("Accepted"));
    let fee_only = outcome.and_then(|o|o.get("AcceptFeeRejectRest")).is_some();
    let err_nested = outcome.and_then(|o|o.get("AcceptFeeRejectRest")).and_then(Value::as_array)
        .and_then(|a|a.iter().find_map(|x|x.pointer("/ExecutionFailure/message").and_then(Value::as_str))).map(str::to_owned);
    let reason = error_text.as_deref().or(err_nested.as_deref());
    let rejected = if live {fee_only && status.as_deref()==Some("OnlyFeeAccepted")} else {error_text.is_some()};
    let mut pass = if reject_expected {rejected && reason.is_some_and(|s|s.contains(expected_message))} else {accepted};
    let mut details = json!({"withdraw_base_units":withdraw,"execution":execution,"rpc_error":error_text,"expected_error":expected_message});
    if accepted {
        let events = if live { execution.get("events") } else { execution.pointer("/finalize/events") };
        let vaults = [
            ("house","vault_123c27ac1fdd81731c0947e3e63d7839d42adac3c3b2ad802302f97708a2f2c9"),
            ("community","vault_7885889cd16e0476cc8cb86fe1fd7e059e6658a24ed84d9735c3288c0aec5253"),
            ("chance","vault_008ca66e8230e9f9f3a9fd26629b061d47d3a54e871550edc71579d70e7fd56f"),
        ];
        let expected: [u64;3] = match request.test_id.as_str() {"sale"=>[9_000_000,500_000,500_000],"sponsor"=>[8_000_000,1_500_000,500_000],"rounding"=>[91,5,5],_=>[0,0,0]};
        let actual: Vec<u64> = vaults.iter().map(|(_,v)|{
            events.and_then(Value::as_array).map(|ev|ev.iter()
                .filter(|e|e.get("topic").and_then(Value::as_str)==Some("std.vault.deposit") && e.get("substate_id").and_then(Value::as_str)==Some(*v))
                .filter_map(|e|e.pointer("/payload/amount").and_then(Value::as_u64)).sum()).unwrap_or(0)
        }).collect();
        pass &= actual == expected;
        details["expected_deposits"] = json!(expected);
        details["actual_deposits"] = json!(actual);
    }
    let reconciliation = if live && !timed_out && (accepted || fee_only) {
        match (before, fee) {
            (Some(before),Some(fee)) => match treasury_balances(config,&client,&token).await {
                Ok(after) => Some(reconcile(before, after, fee, &request.test_id,accepted,fee_only)),
                Err(error) => Some(json!({"verified":false,"error":error,"before_base_units":before})),
            },
            _ => Some(json!({"verified":false,"error":"missing baseline or finalized fee"})),
        }
    } else { None };
    if live && (accepted || fee_only) {
        pass &= reconciliation.as_ref().and_then(|r|r.get("verified")).and_then(Value::as_bool)==Some(true);
    }
    let (code,title) = classify(status.as_deref(),reason,timed_out);
    let verdict = if timed_out || (live && id.is_some() && error_text.is_some() && status.is_none()) {
        "UNCONFIRMED"
    } else if pass {"PASS"} else {"FAIL"};
    Ok(TestRecord {
        sequence,test_id:request.test_id.clone(),mode:request.mode.clone(),verdict:verdict.into(),
        code:code.into(),title:title.into(),
        message: if reject_expected {format!("Expected rejection: {}. {}",expected_message,if pass {"Validated"} else {"Not validated"})}
                 else if pass {"All expected deposits confirmed in transaction events".into()}
                 else {"Unexpected outcome or treasury deposit mismatch; inspect evidence".into()},
        transaction_id:id,status,fee_base_units:fee,expected_rejection:reject_expected,reconciliation,evidence:details,
    })
}

pub async fn inspect(config: &Config, request: &InspectRequest) -> Result<Value,String> {
    if request.transaction_id.len()!=64 || !request.transaction_id.bytes().all(|x|x.is_ascii_hexdigit()) {return Err("Transaction ID must be 64 hexadecimal characters".into());}
    let (client,token) = authorized(config).await?;
    let data = rpc(&client,&config.network.wallet_rpc_url,"transactions.get_result",json!({"transaction_id":request.transaction_id}),Some(&token)).await?;
    if let Some(err) = data.get("error") {return Err(fmt_error(err));}
    let result = data.get("result").ok_or("Missing result")?;
    let status = result.get("status").and_then(Value::as_str);
    let outcome = result.pointer("/result/result");
    let err = outcome.and_then(|o|o.get("AcceptFeeRejectRest")).and_then(Value::as_array)
        .and_then(|a|a.iter().find_map(|v|v.pointer("/ExecutionFailure/message").and_then(Value::as_str)));
    let (code,title) = classify(status,err,false);
    Ok(json!({"status":status,"code":code,"title":title,"reason":err,"transaction_id":request.transaction_id,"result":result}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn error_classification() {
        assert_eq!(classify(Some("OnlyFeeAccepted"),Some("amount does not match bucket"),false).0,"TREASURY_AMOUNT_MISMATCH");
        assert_eq!(classify(Some("OnlyFeeAccepted"),None,false).0,"TRANSACTION_FEE_ONLY");
        assert_eq!(classify(Some("Accepted"),None,false).0,"TRANSACTION_ACCEPTED");
        assert_eq!(classify(None,None,true).0,"TRANSACTION_TIMEOUT");
    }
    #[test] fn balance_reconciliation() {
        assert_eq!(reconcile([100_000_000,5_000_000,5_000_000],
            [98_995_188,5_500_000,5_500_000],4_812,"sale",true,false)["verified"],true);
        assert_eq!(reconcile([100_000_000,5_000_000,5_000_000],
            [99_997_819,5_000_000,5_000_000],2_181,"mismatch",false,true)["verified"],true);
        assert_eq!(reconcile([100_000_000,5_000_000,5_000_000],
            [99_997_818,5_000_000,5_000_000],2_181,"mismatch",false,true)["verified"],false);
    }
    #[test] fn fixed_test_ids() {assert!(test_config("sale").is_ok());assert!(test_config("random").is_err());}
}
// End of src/diagnostics.rs
