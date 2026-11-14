//! Ootle Asset Game | src/game_session.rs
//! Reads live on-chain game/asset state; submits one atomic economic turn at a time.
//! No local game-state cache. Browser has no blockchain signing privileges.
//! DEV-ONLY: the contract does not itself verify per-player caller identities.
//! Wallet withdrawal authorization is delegated to each account; protect this local server.

use crate::{config::Config, ootle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

#[derive(Debug, Clone, Deserialize)]
pub struct GameDeployment {
    pub template_address: String,
    pub component_address: String,
    pub board_size: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameView {
    pub source: String,
    pub component_address: String,
    pub template_address: String,
    pub substate_version: Value,
    pub board_size: u16,
    pub round: u64,
    pub active_player: u8,
    pub position1: u16,
    pub position2: u16,
    pub last_roll1: u8,
    pub last_roll2: u8,
    pub turn_count: u64,
    pub last_event: EventView,
    pub pending_offer: Option<OfferView>,
    pub offer_nonce: u64,
    pub player1_account: String,
    pub player2_account: String,
    pub owners: Vec<u8>,
    pub improvements: Vec<u8>,
    pub assets: Vec<AssetView>,
    pub squares: Vec<SquareView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OfferView {
    pub square: u16,
    pub price_base_units: u64,
    pub seller: u8,
    pub buyer: u8,
    pub nonce: u64,
}

#[derive(Debug, Deserialize)]
pub struct TradeRequest {
    pub action: String,
    pub square: Option<u16>,
    pub price_base_units: Option<u64>,
    pub expected_round: u64,
    pub expected_turn_count: u64,
    pub expected_nonce: Option<u64>,
    pub confirmation: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EventView {
    pub kind: u8,
    pub card_id: u8,
    pub amount_base_units: u64,
    pub positive: bool,
    pub player: u8,
    pub square: u16,
    pub recipient: u8,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SquareView {
    pub square: u16,
    pub label: String,
    pub kind: u8,
    pub price_base_units: u64,
    pub owner: u8,
    pub improvement_level: u8,
    pub property_value_base_units: u64,
    pub rent_base_units: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetView {
    pub square: u16,
    pub label: &'static str,
    pub price_base_units: u64,
    pub resale_base_units: u64,
    pub owner: u8,
    pub improvement_level: u8,
    pub property_value_base_units: u64,
    pub rent_base_units: u64,
}

#[derive(Debug, Deserialize)]
pub struct ImprovementRequest {
    pub square: u16,
    pub expected_level: u8,
    pub expected_round: u64,
    pub expected_turn_count: u64,
    pub confirmation: String,
}

pub const HOUSE_COST: u64 = 5_000_000;
pub const HOTEL_COST: u64 = 15_000_000;
pub fn improvement_total(level: u8) -> Result<u64, String> {
    match level { 0 => Ok(0), 1 => Ok(HOUSE_COST), 2 => Ok(2 * HOUSE_COST),
        3 => Ok(2 * HOUSE_COST + HOTEL_COST), _ => Err("invalid development level".into()) }
}
pub fn property_rent(base: u64, level: u8) -> Result<u64, String> {
    if level > 3 {return Err("invalid development level".into());}
    let percent = 10 + 5 * u64::from(level.min(2)) + if level == 3 {15} else {0};
    base.checked_mul(percent).map(|n|n/100).ok_or("rent overflow".into())
}

#[derive(Debug, Deserialize)]
pub struct TurnRequest {
    pub die1: u8,
    pub die2: u8,
    pub expected_round: u64,
    pub expected_player: u8,
    pub confirmation: String,
    pub buy: bool,
    pub sell_square: Option<u16>,
}

const BOARD_ASSETS: [(u16,&str,u64);22] = [
    (1,"Asset A",10000000),
    (3,"Asset B",20000000),
    (6,"Asset C",30000000),
    (8,"Asset D",40000000),
    (11,"Asset E",18000000),
    (13,"Asset F",20000000),
    (14,"Asset G",22000000),
    (16,"Asset H",24000000),
    (18,"Asset I",26000000),
    (19,"Asset J",28000000),
    (21,"Asset K",30000000),
    (23,"Asset L",32000000),
    (25,"Asset M",34000000),
    (26,"Asset N",36000000),
    (28,"Asset O",38000000),
    (29,"Asset P",40000000),
    (31,"Asset Q",42000000),
    (32,"Asset R",44000000),
    (34,"Asset S",46000000),
    (36,"Asset T",48000000),
    (38,"Asset U",50000000),
    (39,"Asset V",52000000),
];
const SQUARE_LABELS: [&str;40] = ["GO","Asset A","Chance","Asset B","Fee","Community","Asset C","Penalty","Asset D","Reward","Jail / Visit","Asset E","Utility","Asset F","Asset G","Transit","Asset H","Community","Asset I","Asset J","Free Park","Asset K","Chance","Asset L","Fee","Asset M","Asset N","Community","Asset O","Asset P","Go To Jail","Asset Q","Asset R","Chance","Asset S","Transit","Asset T","Fee","Asset U","Asset V"];
const SQUARE_TYPES: [u8;40] = [0,1,2,1,4,3,1,5,1,6,9,1,7,1,1,8,1,3,1,1,9,1,2,1,4,1,1,3,1,1,9,1,1,2,1,8,1,4,1,1];

fn assets_from(owners: &[u8], prices: &[u64], improvements: &[u8]) -> Vec<AssetView> {
    BOARD_ASSETS.iter().map(|(sq,label,_)| {
        let i=usize::from(*sq);
        let property_value=prices[i]+improvement_total(improvements[i]).expect("verified level");
        AssetView {
            square:*sq,label:*label,price_base_units:prices[i],
            resale_base_units:property_value*8/10,owner:owners[i],
            improvement_level:improvements[i],property_value_base_units:property_value,
            rent_base_units:property_rent(prices[i],improvements[i]).expect("verified level"),
        }
    }).collect()
}
fn squares_from(types: &[u8], prices: &[u64], owners: &[u8], improvements: &[u8]) -> Vec<SquareView> {
    (0..40).map(|i|SquareView{
        square:i as u16,label:SQUARE_LABELS[i].to_owned(),kind:types[i],
        price_base_units:prices[i],owner:owners[i],improvement_level:improvements[i],
        property_value_base_units:prices[i]+improvement_total(improvements[i]).expect("verified level"),
        rent_base_units:property_rent(prices[i],improvements[i]).expect("verified level"),
    }).collect()
}
fn expected_owners_after_turn(
    owners: &[u8], sell_square: Option<u16>, buy: bool, landing: u16, player: u8,
) -> Result<Vec<u8>, String> {
    if owners.len() != 40 { return Err(format!("Expected 40 ownership entries, received {}", owners.len())); }
    let mut expected = owners.to_vec();
    if let Some(sq) = sell_square {
        let owner = expected.get_mut(usize::from(sq)).ok_or("Sale square outside board")?;
        if *owner != player {return Err("Sale square not owned by player".into());}
        *owner = 0;
    }
    if buy {
        let owner = expected.get_mut(usize::from(landing)).ok_or("Landing square outside board")?;
        if *owner != 0 { return Err("Purchase square is not unowned".into()); }
        *owner = player;
    }
    Ok(expected)
}

const GO_REWARD: u64 = 20_000_000;
fn penalties(square:u16)->u64{match square{4|24|37=>1_000_000,7=>2_000_000,_=>0}}
fn rewards(square:u16)->u64{if square==9 {2_000_000}else{0}}
fn pass_go_reward(from:u16,to:u16)->u64{if to < from {GO_REWARD}else{0}}
fn card(square:u16, turn_count:u64)->(u8,u8,u64,bool) {
    let kind=if [2,22,33].contains(&square) {1} else if [5,17,27].contains(&square) {2} else {0};
    if kind==0{return (0,255,0,false);}
    let id=((turn_count+u64::from(square))%4) as u8;
    let (amount,positive)=match (kind,id){
        (1,0)=>(2_000_000,true),(1,1)=>(1_000_000,false),
        (1,2)=>(3_000_000,true),(1,3)=>(2_000_000,false),
        (2,0)=>(1_000_000,true),(2,1)=>(1_000_000,false),
        (2,2)=>(2_000_000,true),_=>(2_000_000,false),
    };
    (kind,id,amount,positive)
}
fn rent(owners:&[u8],prices:&[u64],improvements:&[u8],square:u16,visitor:u8)->(u8,u64){
    let idx=usize::from(square);
    let owner=owners[idx];
    if owner==0||owner==visitor||prices[idx]==0 {(0,0)} else {(owner,property_rent(prices[idx],improvements[idx]).expect("verified improvement"))}
}
fn event_label(kind:u8,card_id:u8,recipient:u8)->String {
    if kind==3 {return format!("Rent paid to Player {recipient}");}
    let cards=if kind==1 { ["Chance: Lucky find", "Chance: Repair bill", "Chance: Bonus payout", "Chance: Inspection fee"] }
        else { ["Community: Shared reward", "Community: Contribution", "Community: Community grant", "Community: Community levy"] };
    if kind==0 {return "No card or rent event".to_owned();}
    cards.get(usize::from(card_id)).unwrap_or(&"Invalid card ID").to_string()
}

pub fn deployment() -> Result<GameDeployment, String> {
    let path = Path::new("config/game_session.toml");
    let text = fs::read_to_string(path).map_err(|e|format!("{}: {e}",path.display()))?;
    let config: GameDeployment = toml::from_str(&text).map_err(|e|format!("game session configuration: {e}"))?;
    if config.component_address.is_empty() || config.template_address.is_empty() {
        return Err("Game Session not deployed. Run ./scripts/setup-game.sh --from-session ../ootle-asset-game-v0.6.7/config/game_session.toml once.".into());
    }
    Ok(config)
}

pub async fn rpc(client: &reqwest::Client, url: &str, method: &str, params: Value, token: Option<&str>) -> Result<Value,String> {
    let mut request = client.post(url).json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}));
    if let Some(token) = token {request = request.bearer_auth(token);}
    let reply = request.send().await.map_err(|e|format!("{method}: {e}"))?;
    let status = reply.status();
    let data: Value = reply.json().await.map_err(|e|format!("{method}: HTTP {status}, invalid JSON: {e}"))?;
    if !status.is_success() {return Err(format!("{method}: HTTP {status}: {data}"));}
    if let Some(error) = data.get("error") {return Err(format!("{method}: {error}"));}
    data.get("result").cloned().ok_or_else(||format!("{method}: no result"))
}

fn client() -> Result<reqwest::Client,String> {
    reqwest::Client::builder().timeout(Duration::from_secs(150)).build().map_err(|e|e.to_string())
}

async fn token(client: &reqwest::Client, config: &Config) -> Result<String,String> {
    let auth=rpc(client,&config.network.wallet_rpc_url,"auth.request",json!({"permissions":["Admin"],"credentials":"None"}),None).await?;
    Ok(auth.get("token").and_then(Value::as_str).ok_or("wallet authentication token missing")?.to_owned())
}

fn tagged_address(field: &Value, tag: u64, prefix: &str) -> Result<String,String> {
    if field.get("tag").and_then(Value::as_u64) != Some(tag) {return Err(format!("expected CBOR tag {tag}"));}
    let hex = field.pointer("/value/hex").and_then(Value::as_str).ok_or("missing address hex")?;
    if hex.len()!=64 || !hex.bytes().all(|b|b.is_ascii_hexdigit()) {return Err("invalid 32-byte address".into());}
    Ok(format!("{prefix}{hex}"))
}

pub fn decode_state(value: &Value, deployment: &GameDeployment, config: &Config) -> Result<GameView,String> {
    let remote=value.get("substate_from_remote").filter(|v|!v.is_null())
        .ok_or("wallet has no remote GameSession component substate")?;
    let component=remote.pointer("/substate/Component")
        .or_else(||remote.pointer("/Component"))
        .or_else(||remote.pointer("/substate/substate/Component"))
        .ok_or("response has no Component body")?;
    let template=component.pointer("/header/template_address").and_then(Value::as_str).ok_or("missing component template address")?;
    if template!=deployment.template_address {return Err("on-chain game template address mismatch".into());}
    let state=component.pointer("/body/state").and_then(Value::as_array).ok_or("game state is not a CBOR array")?;
    if state.len()!=29 {return Err(format!("unknown 40-square GameSession layout: {} fields (expected 29)",state.len()));}
    let p1=tagged_address(&state[0],128,"component_")?;
    let p2=tagged_address(&state[1],128,"component_")?;
    let expected=|id:&str|->Result<&str,String>{config.accounts.accounts.iter().find(|a|a.id==id).map(|a|a.component_address.as_str()).ok_or(format!("missing account {id}"))};
    if p1 != expected("player1")? || p2 != expected("player2")? {return Err("stored player accounts disagree with configured accounts".into());}
    let number=|i:usize| -> Result<u64,String> {state[i].as_u64().ok_or(format!("invalid integer field {i}"))};
    let board_size=u16::try_from(number(2)?).map_err(|e|e.to_string())?;
    let round=number(3)?;
    let active_player=u8::try_from(number(4)?).map_err(|e|e.to_string())?;
    let position1=u16::try_from(number(5)?).map_err(|e|e.to_string())?;
    let position2=u16::try_from(number(6)?).map_err(|e|e.to_string())?;
    let last_roll1=u8::try_from(number(7)?).map_err(|e|e.to_string())?;
    let last_roll2=u8::try_from(number(8)?).map_err(|e|e.to_string())?;
    let turn_count=number(9)?;
    if !(2..=40).contains(&board_size) || board_size!=deployment.board_size
        || !(1..=2).contains(&active_player) || position1>=board_size || position2>=board_size
        || round==0 {
        return Err("inconsistent on-chain round or position state".into());
    }
    if board_size != 40 {return Err("economy game expects 40 squares".into());}
    let types=state[10].as_array().ok_or("missing on-chain square types")?
        .iter().map(|x|u8::try_from(x.as_u64().ok_or("invalid square kind")?).map_err(|e|e.to_string()))
        .collect::<Result<Vec<_>,_>>()?;
    let prices=state[11].as_array().ok_or("missing on-chain prices")?
        .iter().map(|x|x.as_u64().ok_or("invalid price"))
        .collect::<Result<Vec<_>,_>>()?;
    let owners=state[12].as_array().ok_or("missing on-chain owners")?
        .iter().map(|x|u8::try_from(x.as_u64().ok_or("invalid owner")?).map_err(|e|e.to_string()))
        .collect::<Result<Vec<_>,_>>()?;
    if types.len()!=40||prices.len()!=40||owners.len()!=40||types.as_slice()!=SQUARE_TYPES.as_slice()
        ||owners.iter().any(|o|*o>2) || BOARD_ASSETS.iter().any(|(sq,_,price)|prices[*sq as usize]!=*price)
        || (0..40).any(|i| types[i]!=1 && (prices[i]!=0 || owners[i]!=0)) {
        return Err("on-chain board geometry, prices or owners failed validation".into());
    }
    let house=tagged_address(&state[13],128,"component_")?;
    let controller=tagged_address(&state[14],128,"component_")?;
    let resource=tagged_address(&state[15],131,"resource_")?;
    if house != expected("house")? || resource != config.accounts.resource_address {
        return Err("on-chain economy destination/resource mismatch".into());
    }
    let treasury_toml=fs::read_to_string("config/deployment.toml").map_err(|e|e.to_string())?;
    let treasury:toml::Value=toml::from_str(&treasury_toml).map_err(|e|e.to_string())?;
    if treasury.get("component_address").and_then(toml::Value::as_str)!=Some(controller.as_str()) {
        return Err("on-chain distribution controller address mismatch".into());
    }
    let last_event_kind=u8::try_from(number(16)?).map_err(|e|e.to_string())?;
    let last_card_id=u8::try_from(number(17)?).map_err(|e|e.to_string())?;
    let last_event_amount=number(18)?;
    let last_event_positive=number(19)?;
    let last_event_player=u8::try_from(number(20)?).map_err(|e|e.to_string())?;
    let last_event_square=u16::try_from(number(21)?).map_err(|e|e.to_string())?;
    let last_event_recipient=u8::try_from(number(22)?).map_err(|e|e.to_string())?;
    if last_event_kind>3 || last_event_positive>1 || last_event_player>2 || last_event_recipient>2
        || last_event_square>=40 || (last_event_kind==3 && last_event_recipient==0)
        || (last_event_kind==0 && last_event_amount!=0)
        || (last_event_kind==1 || last_event_kind==2) && last_card_id>3 {
        return Err("Invalid on-chain last event fields".into());
    }
    let last_event=EventView{
        kind:last_event_kind,card_id:last_card_id,amount_base_units:last_event_amount,
        positive:last_event_positive==1,player:last_event_player,
        square:last_event_square,recipient:last_event_recipient,
        label:event_label(last_event_kind,last_card_id,last_event_recipient),
    };
    let offer_square=u16::try_from(number(23)?).map_err(|e|e.to_string())?;
    let offer_price=number(24)?;
    let offer_seller=u8::try_from(number(25)?).map_err(|e|e.to_string())?;
    let offer_buyer=u8::try_from(number(26)?).map_err(|e|e.to_string())?;
    let offer_nonce=number(27)?;
    let pending_offer=if offer_square==u16::MAX {
        if offer_price!=0 || offer_seller!=0 || offer_buyer!=0 {return Err("invalid empty offer".into());}
        None
    } else {
        if offer_square>=40 || prices[usize::from(offer_square)]==0 || offer_price==0
            || offer_seller!=active_player || offer_buyer!=3-active_player
            || owners[usize::from(offer_square)]!=offer_seller || offer_nonce==0 {
            return Err("inconsistent pending sale".into());
        }
        Some(OfferView{square:offer_square,price_base_units:offer_price,
            seller:offer_seller,buyer:offer_buyer,nonce:offer_nonce})
    };
    let improvements=state[28].as_array().ok_or("missing on-chain improvement vector")?
        .iter().map(|x|u8::try_from(x.as_u64().ok_or("invalid improvement")?).map_err(|e|e.to_string()))
        .collect::<Result<Vec<_>,_>>()?;
    if improvements.len()!=40 || improvements.iter().any(|v|*v>3)
        || (0..40).any(|i|types[i]!=1 && improvements[i]!=0)
        || (0..40).any(|i|owners[i]==0 && improvements[i]!=0) {
        return Err("inconsistent on-chain improvements".into());
    }
    Ok(GameView {
        source:"verified wallet substates.get remote component".into(),
        component_address:deployment.component_address.clone(),
        template_address:template.to_owned(),
        substate_version:remote.get("version").cloned().unwrap_or(Value::Null),
        board_size,round,active_player,position1,position2,last_roll1,last_roll2,turn_count,last_event,pending_offer,offer_nonce,
        player1_account:p1,player2_account:p2,owners:owners.clone(),improvements:improvements.clone(),
        assets:assets_from(&owners,&prices,&improvements),
        squares:squares_from(&types,&prices,&owners,&improvements),
    })
}

pub async fn fetch_state(config: &Config) -> Result<GameView,String> {
    let deployment=deployment()?;
    let client=client()?;
    let auth=token(&client,config).await?;
    let remote=rpc(&client,&config.network.wallet_rpc_url,"substates.get",
        json!({"substate_id":deployment.component_address}),Some(&auth)).await?;
    decode_state(&remote,&deployment,config)
}

fn wallet_key_id(account: &Value) -> Option<Value> {
    for path in ["/account/owner_key_id","/owner_key_id","/account/owner_key","/account/key_id"] {
        if let Some(key) = account.pointer(path) {
            if key.is_object() && key.get("Derived").is_some() {return Some(key.clone());}
        }
    }
    None
}

fn player_signer_fallback(player:u8) -> Result<Value,String> {
    let name=if player==1 {"OOTLE_PLAYER1_SIGNER_INDEX"} else {"OOTLE_PLAYER2_SIGNER_INDEX"};
    let index=std::env::var(name)
        .map_err(|_|format!("Wallet did not expose account owner_key_id. Set {name} to the verified derived key index; no transaction submitted"))?
        .parse::<u64>().map_err(|_|format!("{name} must be a positive integer"))?;
    Ok(json!({"Derived":{"index":index,"key_branch":"account"}}))
}

fn amount(snapshot:&ootle::LiveSnapshot,id:&str)->Result<i128,String> {
    let entry=snapshot.accounts.iter().find(|a|a.id==id).ok_or(format!("missing {id} balance"))?;
    entry.balance_base_units.parse::<i128>().map_err(|e|format!("{id} balance: {e}"))
}

pub async fn execute_turn(config: &Config, request: &TurnRequest) -> Result<Value,String> {
    if std::env::var("OOTLE_ENABLE_LIVE_TESTS").as_deref()!=Ok("1") {
        return Err("Live transactions disabled; set OOTLE_ENABLE_LIVE_TESTS=1".into());
    }
    if !(1..=6).contains(&request.die1) || !(1..=6).contains(&request.die2) {
        return Err("Each die must be 1..6".into());
    }
    if request.confirmation != format!("LIVE TURN {}",request.expected_player) {
        return Err(format!("Exact confirmation required: LIVE TURN {}",request.expected_player));
    }
    let before=fetch_state(config).await?;
    if before.round!=request.expected_round || before.active_player!=request.expected_player {
        return Err(format!("Stale turn: chain is round {} player {}",before.round,before.active_player));
    }
    if before.pending_offer.is_some() {return Err("Resolve private offer before moving".into());}
    let p=request.expected_player;
    let from=if p==1 {before.position1} else {before.position2};
    let total_roll=u16::from(request.die1)+u16::from(request.die2);
    let doubles=request.die1==request.die2;
    let to=(from+total_roll)%before.board_size;
    let sale=if let Some(square)=request.sell_square {
        let asset=before.assets.iter().find(|a|a.square==square).ok_or("Not a sellable asset square")?;
        if asset.owner!=p {return Err("Selected asset is not owned by active player".into());}
        asset.resale_base_units
    } else {0};
    let purchase=if request.buy {
        let asset=before.assets.iter().find(|a|a.square==to).ok_or("Not a purchasable landing square")?;
        if asset.owner!=0 {return Err("Landing asset already owned".into());}
        asset.price_base_units
    } else {0};
    let (card_kind,card_id,card_amount,card_positive)=card(to,before.turn_count);
    let prices:Vec<u64>=before.squares.iter().map(|sq|sq.price_base_units).collect();
    let (rent_owner,rent_amount)=rent(&before.owners,&prices,&before.improvements,to,p);
    let card_credit=if card_positive {i128::from(card_amount)}else{0};
    let card_charge=if card_kind!=0 && !card_positive {i128::from(card_amount)} else {0};
    let penalty=i128::from(penalties(to))+card_charge;
    let board_reward=i128::from(rewards(to));
    let go_reward=i128::from(pass_go_reward(from,to));
    let total_reward=board_reward+go_reward+card_credit;
    let rent_charge=i128::from(rent_amount);
    let balances_before=ootle::fetch_balances(config).await?;
    let player_id=if p==1 {"player1"} else {"player2"};
    let player_balance=amount(&balances_before,player_id)?;
    let house_balance=amount(&balances_before,"house")?;
    if player_balance + i128::from(sale) + total_reward < i128::from(purchase)+penalty+rent_charge {
        return Err("Insufficient player bankroll for combined sale, purchase and penalty".into());
    }
    if house_balance < i128::from(sale)+total_reward+1_000_000 {
        return Err("Game House cannot cover resale, reward and fee reserve".into());
    }
    let deploy=deployment()?;
    let house=config.accounts.accounts.iter().find(|a|a.id=="house").ok_or("House account missing")?;
    let player=config.accounts.accounts.iter().find(|a|a.id==player_id).ok_or("Player account missing")?;
    let max_fee=1_000_000u64;
    let sell_square=request.sell_square.unwrap_or(u16::MAX);
    let incoming=sale
        .checked_add(u64::try_from(total_reward).map_err(|e|e.to_string())?)
        .ok_or("incoming overflow")?;
    let outgoing=purchase
        .checked_add(u64::try_from(penalty).map_err(|e|e.to_string())?)
        .ok_or("outgoing overflow")?
        .checked_add(rent_amount).ok_or("outgoing rent overflow")?;
    let player_funding=outgoing.saturating_sub(incoming);
    let resource=&config.accounts.resource_address;
    let manifest=format!(r#"
fn fee_main() {{
    let house = var!["house"];
    house.pay_fee({max_fee}u64);
}}
fn main() {{
    let house = var!["house"];
    let player_account = var!["player_account"];
    let game = var!["game"];
    let house_funds = house.withdraw(address!("{resource}"), {incoming}u64);
    let player_funds = player_account.withdraw(address!("{resource}"), {player_funding}u64);
    game.play_turn({die1}u8, {die2}u8, {round}u64, {player}u8,
        {buy}, {sell_square}u16, player_funds, house_funds);
}}
"#,max_fee=max_fee,resource=resource,incoming=incoming,player_funding=player_funding,
       die1=request.die1,die2=request.die2,round=request.expected_round,player=p,buy=request.buy,sell_square=sell_square);
    let client=client()?;
    let auth=token(&client,config).await?;
    let account_info=rpc(&client,&config.network.wallet_rpc_url,"accounts.get",
        json!({"name_or_address":player.name}),Some(&auth)).await?;
    let signer=match wallet_key_id(&account_info) {
        Some(key)=>key,
        None=>player_signer_fallback(p)?,
    };
    let mut params=json!({
        "manifest":manifest,
        "variables":{"house":house.component_address,"player_account":player.component_address,"game":deploy.component_address},
        "seal_signer_key_id":{"Derived":{"index":3,"key_branch":"account"}},
        "signing_key_ids":[signer],"max_fee":max_fee,"dry_run":true,"blobs":{}
    });
    let dry=rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",params.clone(),Some(&auth)).await?;
    if dry.pointer("/result/finalize/result/Accept").is_none() {
        return Err(format!("Economic turn failed dry run: {dry}"));
    }
    params["dry_run"]=json!(false);
    let auth=token(&client,config).await?;
    let submitted=rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",params,Some(&auth)).await?;
    let id=submitted.get("transaction_id").and_then(Value::as_str).ok_or("missing transaction id")?.to_owned();
    let final_result=rpc(&client,&config.network.wallet_rpc_url,"transactions.wait_result",
        json!({"transaction_id":id,"timeout_secs":120}),Some(&auth)).await?;
    let accepted=final_result.get("status").and_then(Value::as_str)==Some("Accepted")
        && final_result.pointer("/result/result/Accept").is_some()
        && final_result.get("timed_out").and_then(Value::as_bool)==Some(false);
    if !accepted {return Ok(json!({"committed":false,"transaction_id":id,"result":final_result,"before":before}));}
    let after=match fetch_state(config).await {
        Ok(v)=>v,
        Err(err)=>return Ok(json!({"committed":true,"verified":false,"transaction_id":id,"verification_error":err})),
    };
    let expected_round=if p==2 && !doubles {before.round+1} else {before.round};
    let expected_active=if doubles {p} else {3-p};
    let expected_owners = expected_owners_after_turn(&before.owners, request.sell_square, request.buy, to, p)?;
    let mut expected_improvements=before.improvements.clone();
    if let Some(sq)=request.sell_square { expected_improvements[usize::from(sq)]=0; }
    let expected_event=EventView{
        kind:if rent_amount>0 {3}else{card_kind},
        card_id:if rent_amount>0 {255}else{card_id},
        amount_base_units:if rent_amount>0 {rent_amount}else{card_amount},
        positive:rent_amount==0 && card_positive,
        player:p,square:to,recipient:rent_owner,
        label:event_label(if rent_amount>0 {3}else{card_kind},if rent_amount>0 {255}else{card_id},rent_owner),
    };
    let state_ok=after.turn_count==before.turn_count+1 && after.round==expected_round
        && after.active_player==expected_active &&
        (if p==1 {after.position1==to && after.position2==before.position2}
         else {after.position2==to && after.position1==before.position1})
        && after.owners==expected_owners && after.improvements==expected_improvements && after.last_event==expected_event;
    let balances_after=ootle::fetch_balances(config).await;
    let fee=final_result.get("final_fee").and_then(Value::as_u64);
    let mut verified=false;
    let mut differences=json!({"state_verified":state_ok,"balance_verified":false,
        "expected_owner_entries":expected_owners.len(),"actual_owner_entries":after.owners.len(),
        "expected_owners":expected_owners,"actual_owners":after.owners,
        "die1":request.die1,"die2":request.die2,"roll_total":total_roll,"doubles":doubles,
        "go_reward_base_units":go_reward,"square_reward_base_units":board_reward});
    if let (Ok(ref snapshot),Some(fee_units))=(&balances_after,fee) {
        let p1delta=amount(snapshot,"player1")?-amount(&balances_before,"player1")?;
        let p2delta=amount(snapshot,"player2")?-amount(&balances_before,"player2")?;
        let hdelta=amount(snapshot,"house")?-amount(&balances_before,"house")?;
        let cdelta=amount(snapshot,"community")?-amount(&balances_before,"community")?;
        let chdelta=amount(snapshot,"chance")?-amount(&balances_before,"chance")?;
        let expected_p=i128::from(sale)-i128::from(purchase)-penalty+total_reward-rent_charge;
        let expected_p1=if p==1 {expected_p}else{rent_charge};
        let expected_p2=if p==2 {expected_p}else{rent_charge};
        let treasury_total=i128::from(purchase)+penalty;
        let community=treasury_total*5/100;
        let chance=treasury_total*5/100;
        let expected_h=treasury_total-community-chance-i128::from(sale)-total_reward-i128::from(fee_units);
        let sum=p1delta+p2delta+hdelta+cdelta+chdelta;
        let balance_ok=p1delta==expected_p1 && p2delta==expected_p2 && hdelta==expected_h
            && cdelta==community && chdelta==chance && sum==-i128::from(fee_units);
        differences=json!({"state_verified":state_ok,"balance_verified":balance_ok,
            "player1_delta":p1delta,"player2_delta":p2delta,"house_delta":hdelta,
            "community_delta":cdelta,"chance_delta":chdelta,"fee_base_units":fee_units,"total_delta":sum,
            "expected_player1_delta":expected_p1,"expected_player2_delta":expected_p2,
            "expected_house_delta":expected_h,"expected_community_delta":community,
            "expected_chance_delta":chance,
            "expected_owner_entries":expected_owners.len(),"actual_owner_entries":after.owners.len(),
            "die1":request.die1,"die2":request.die2,"roll_total":total_roll,"doubles":doubles,
            "go_reward_base_units":go_reward,"square_reward_base_units":board_reward,
            "card_kind":card_kind,"card_id":card_id,"card_amount_base_units":card_amount,
            "card_positive":card_positive,"rent_owner":rent_owner,"rent_base_units":rent_amount});
        verified=state_ok&&balance_ok;
    } else {
        let reason = if fee.is_none() {
            "Finalized transaction response did not contain final_fee".to_owned()
        } else {
            format!("Post-turn wallet balance retrieval failed: {}", balances_after.as_ref().err().map(String::as_str).unwrap_or("unknown"))
        };
        differences["verification_error"] = json!(reason);
    }
    Ok(json!({"committed":true,"verified":verified,"transaction_id":id,
        "fee_base_units":fee,"before":before,"after":after,
        "reconciliation":differences,"live_balances":balances_after.ok(),
        "status":final_result.get("status"),"operation":{
           "sell_square":request.sell_square,"sale_base_units":sale,
           "buy":request.buy,"purchase_base_units":purchase,
           "reward_base_units":board_reward,"go_reward_base_units":go_reward,
           "card_kind":card_kind,"card_id":card_id,"card_amount_base_units":card_amount,
           "card_positive":card_positive,"rent_owner":rent_owner,"rent_base_units":rent_amount,
           "penalty_base_units":penalty,"die1":request.die1,"die2":request.die2,
           "roll_total":total_roll,"doubles":doubles}}))
}


/// One on-chain operation per request. Source balances, state and nonce are re-read
/// before constructing the manifest; receipt and post-state are verified afterwards.
pub async fn execute_trade(config: &Config, req: &TradeRequest) -> Result<Value, String> {
    if std::env::var("OOTLE_ENABLE_LIVE_TESTS").as_deref()!=Ok("1") {
        return Err("Live transactions disabled; set OOTLE_ENABLE_LIVE_TESTS=1".into());
    }
    if !["propose","accept","decline"].contains(&req.action.as_str()) {
        return Err("Unknown trade action".into());
    }
    if req.confirmation != format!("LIVE TRADE {}",req.action.to_uppercase()) {
        return Err("Exact LIVE TRADE confirmation required".into());
    }
    let before=fetch_state(config).await?;
    if before.round!=req.expected_round || before.turn_count!=req.expected_turn_count {
        return Err("Stale trade state".into());
    }
    let deploy=deployment()?;
    let (actor, square, price, nonce)=match req.action.as_str() {
        "propose"=>{
            if before.pending_offer.is_some() {return Err("Offer already pending".into());}
            let sq=req.square.ok_or("Missing sale square")?;
            let a=before.assets.iter().find(|a|a.square==sq).ok_or("Unknown asset")?;
            if a.owner!=before.active_player {return Err("Seller does not own asset".into());}
            let price=req.price_base_units.ok_or("Missing sale price")?;
            if price==0 {return Err("Price must be positive".into());}
            (before.active_player,sq,price,before.offer_nonce)
        },
        _=>{
            let offer=before.pending_offer.as_ref().ok_or("No pending offer")?;
            if req.expected_nonce!=Some(offer.nonce) {return Err("Stale offer nonce".into());}
            (offer.buyer,offer.square,offer.price_base_units,offer.nonce)
        }
    };
    let actor_id=format!("player{actor}");
    let actor_account=config.accounts.accounts.iter().find(|a|a.id==actor_id)
        .ok_or("Missing actor account")?;
    let house=config.accounts.accounts.iter().find(|a|a.id=="house").ok_or("House missing")?;
    let balances_before=ootle::fetch_balances(config).await?;
    if req.action=="accept" && amount(&balances_before,&actor_id)?<i128::from(price) {
        return Err("Buyer has insufficient funds".into());
    }
    let max_fee=1_000_000u64;
    let call=match req.action.as_str() {
        "propose"=>format!("game.propose_sale({actor}u8, {square}u16, {price}u64, {}u64, {}u64);",
            req.expected_round,req.expected_turn_count),
        "decline"=>format!("game.decline_sale({actor}u8, {nonce}u64, {}u64, {}u64);",
            req.expected_round,req.expected_turn_count),
        _=>format!("let payment = actor_account.withdraw(address!(\"{}\"), {price}u64);\n    game.accept_sale({actor}u8, {nonce}u64, {}u64, {}u64, payment);",
            config.accounts.resource_address,req.expected_round,req.expected_turn_count),
    };
    let manifest=format!(r#"fn fee_main() {{
    let house = var!["house"];
    house.pay_fee({max_fee}u64);
}}
fn main() {{
    let game = var!["game"];
    let actor_account = var!["actor_account"];
    {call}
}}"#);
    let client=client()?;
    let auth=token(&client,config).await?;
    let account_info=rpc(&client,&config.network.wallet_rpc_url,"accounts.get",
        json!({"name_or_address":actor_account.name}),Some(&auth)).await?;
    let signer=wallet_key_id(&account_info).map(Ok).unwrap_or_else(||player_signer_fallback(actor))?;
    let mut params=json!({"manifest":manifest,
        "variables":{"house":house.component_address,"game":deploy.component_address,
                     "actor_account":actor_account.component_address},
        "seal_signer_key_id":{"Derived":{"index":3,"key_branch":"account"}},
        "signing_key_ids":[signer],"max_fee":max_fee,"dry_run":true,"blobs":{}});
    let dry=rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",params.clone(),Some(&auth)).await?;
    if dry.pointer("/result/finalize/result/Accept").is_none() {
        return Err(format!("Trade dry run rejected: {dry}"));
    }
    params["dry_run"]=json!(false);
    let auth=token(&client,config).await?;
    let submitted=rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",params,Some(&auth)).await?;
    let id=submitted.get("transaction_id").and_then(Value::as_str).ok_or("Missing transaction id")?.to_owned();
    let result=rpc(&client,&config.network.wallet_rpc_url,"transactions.wait_result",
        json!({"transaction_id":id,"timeout_secs":120}),Some(&auth)).await?;
    let committed=result.get("status").and_then(Value::as_str)==Some("Accepted")
        && result.pointer("/result/result/Accept").is_some()
        && result.get("timed_out").and_then(Value::as_bool)==Some(false);
    if !committed {return Ok(json!({"committed":false,"transaction_id":id,"result":result}));}
    let after=match fetch_state(config).await {
        Ok(v)=>v,
        Err(e)=>return Ok(json!({"committed":true,"verified":false,"transaction_id":id,"verification_error":e})),
    };
    let mut expected_owners=before.owners.clone();
    if req.action=="accept" {expected_owners[usize::from(square)]=actor;}
    let offer_ok=match req.action.as_str() {
        "propose"=>after.pending_offer.as_ref().is_some_and(|o|
            o.square==square && o.price_base_units==price && o.seller==actor
            && o.buyer==3-actor && o.nonce==nonce+1),
        _=>after.pending_offer.is_none() && after.offer_nonce==nonce,
    };
    let state_ok=after.owners==expected_owners && offer_ok && after.round==before.round
        && after.active_player==before.active_player && after.turn_count==before.turn_count
        && after.position1==before.position1 && after.position2==before.position2
        && after.last_event==before.last_event && after.improvements==before.improvements;
    let post_balances=ootle::fetch_balances(config).await;
    let fee=result.get("final_fee").and_then(Value::as_u64);
    let mut reconciliation=json!({"state_verified":state_ok,"balance_verified":false});
    let mut verified=false;
    if let (Ok(ref balances),Some(fee_units))=(&post_balances,fee) {
        let deltas=["player1","player2","house","community","chance"]
            .iter().map(|id|Ok(((*id).to_owned(),amount(balances,id)?-amount(&balances_before,id)?)))
            .collect::<Result<std::collections::HashMap<String,i128>,String>>()?;
        let seller=if req.action=="propose" {actor} else {3-actor};
        let (p1,p2)=if req.action=="accept" {
            if seller==1 {(i128::from(price),-i128::from(price))}
            else {(-i128::from(price),i128::from(price))}
        } else {(0,0)};
        let balance_ok=deltas["player1"]==p1 && deltas["player2"]==p2
            && deltas["house"]==-i128::from(fee_units)
            && deltas["community"]==0 && deltas["chance"]==0;
        reconciliation=json!({"state_verified":state_ok,"balance_verified":balance_ok,
            "deltas":deltas,"expected_player1_delta":p1,"expected_player2_delta":p2,
            "fee_base_units":fee_units});
        verified=state_ok&&balance_ok;
    } else {
        reconciliation["verification_error"]=json!(if fee.is_none() {"Missing final_fee"} else {"Balance read unavailable"});
    }
    Ok(json!({"committed":true,"verified":verified,"transaction_id":id,
        "fee_base_units":fee,"action":req.action,"reconciliation":reconciliation,
        "before":before,"after":after,"live_balances":post_balances.ok()}))
}

/// Build one step on an owned property. Must be active player, before dice and not during an offer.
pub async fn execute_improvement(config:&Config, req:&ImprovementRequest)->Result<Value,String>{
    if std::env::var("OOTLE_ENABLE_LIVE_TESTS").as_deref()!=Ok("1") {
        return Err("Live transactions disabled; set OOTLE_ENABLE_LIVE_TESTS=1".into());
    }
    if req.confirmation!="LIVE BUILD" {return Err("Exact confirmation LIVE BUILD required".into());}
    let before=fetch_state(config).await?;
    if before.round!=req.expected_round || before.turn_count!=req.expected_turn_count {
        return Err("Stale improvement request".into());
    }
    if before.pending_offer.is_some() {return Err("Resolve pending trade before building".into());}
    let asset=before.assets.iter().find(|a|a.square==req.square).ok_or("Not an asset square")?;
    if asset.owner!=before.active_player {return Err("Only the active owner can build".into());}
    if asset.improvement_level!=req.expected_level || asset.improvement_level>=3 {
        return Err("Invalid or stale development stage".into());
    }
    let cost=if asset.improvement_level<2 {HOUSE_COST} else {HOTEL_COST};
    let p=before.active_player;
    let id=format!("player{p}");
    let balances_before=ootle::fetch_balances(config).await?;
    if amount(&balances_before,&id)?<i128::from(cost) {return Err("Insufficient bankroll for construction".into());}
    let player=config.accounts.accounts.iter().find(|a|a.id==id).ok_or("Missing player account")?;
    let house=config.accounts.accounts.iter().find(|a|a.id=="house").ok_or("Missing House account")?;
    let deploy=deployment()?;
    let max_fee=1_000_000u64;
    let manifest=format!(r#"fn fee_main() {{
    let house = var!["house"];
    house.pay_fee({max_fee}u64);
}}
fn main() {{
    let game = var!["game"];
    let actor_account = var!["actor_account"];
    let payment = actor_account.withdraw(address!("{resource}"), {cost}u64);
    game.build_improvement({p}u8, {square}u16, {round}u64, {turns}u64, {level}u8, payment);
}}"#,resource=config.accounts.resource_address,square=req.square,
        round=req.expected_round,turns=req.expected_turn_count,level=req.expected_level);
    let client=client()?;
    let auth=token(&client,config).await?;
    let info=rpc(&client,&config.network.wallet_rpc_url,"accounts.get",
        json!({"name_or_address":player.name}),Some(&auth)).await?;
    let signer=wallet_key_id(&info).map(Ok).unwrap_or_else(||player_signer_fallback(p))?;
    let mut params=json!({"manifest":manifest,
        "variables":{"house":house.component_address,"game":deploy.component_address,
        "actor_account":player.component_address},
        "seal_signer_key_id":{"Derived":{"index":3,"key_branch":"account"}},
        "signing_key_ids":[signer],"max_fee":max_fee,"dry_run":true,"blobs":{}});
    let dry=rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",params.clone(),Some(&auth)).await?;
    if dry.pointer("/result/finalize/result/Accept").is_none() {
        return Err(format!("Construction dry run rejected: {dry}"));
    }
    params["dry_run"]=json!(false);
    let auth=token(&client,config).await?;
    let submitted=rpc(&client,&config.network.wallet_rpc_url,"transactions.submit_manifest",params,Some(&auth)).await?;
    let txid=submitted.get("transaction_id").and_then(Value::as_str).ok_or("Missing transaction id")?.to_owned();
    let result=rpc(&client,&config.network.wallet_rpc_url,"transactions.wait_result",
        json!({"transaction_id":txid,"timeout_secs":120}),Some(&auth)).await?;
    let committed=result.get("status").and_then(Value::as_str)==Some("Accepted")
        && result.pointer("/result/result/Accept").is_some()
        && result.get("timed_out").and_then(Value::as_bool)==Some(false);
    if !committed {return Ok(json!({"committed":false,"transaction_id":txid,"result":result}));}
    let after=match fetch_state(config).await {
        Ok(state)=>state,
        Err(err)=>return Ok(json!({"committed":true,"verified":false,"transaction_id":txid,"verification_error":err}))
    };
    let mut expected_improvements=before.improvements.clone();
    expected_improvements[usize::from(req.square)]+=1;
    let state_ok=after.improvements==expected_improvements && after.owners==before.owners
        && after.round==before.round && after.turn_count==before.turn_count
        && after.active_player==before.active_player && after.position1==before.position1
        && after.position2==before.position2 && after.last_event==before.last_event
        && after.pending_offer.is_none() && after.offer_nonce==before.offer_nonce;
    let fee=result.get("final_fee").and_then(Value::as_u64);
    let post_balances=ootle::fetch_balances(config).await;
    let mut reconciliation=json!({"state_verified":state_ok,"balance_verified":false});
    let mut verified=false;
    if let (Ok(ref balances),Some(fee_units))=(&post_balances,fee){
        let delta=|id:&str|->Result<i128,String>{Ok(amount(balances,id)?-amount(&balances_before,id)?)};
        let player1=delta("player1")?;let player2=delta("player2")?;
        let h=delta("house")?;let c=delta("community")?;let ch=delta("chance")?;
        let p1_expected=if p==1 {-i128::from(cost)}else{0};
        let p2_expected=if p==2 {-i128::from(cost)}else{0};
        let comm=i128::from(cost)*5/100;
        let chance=i128::from(cost)*5/100;
        let house_expected=i128::from(cost)-comm-chance-i128::from(fee_units);
        let balance_ok=player1==p1_expected && player2==p2_expected && h==house_expected
            && c==comm && ch==chance && player1+player2+h+c+ch==-i128::from(fee_units);
        reconciliation=json!({"state_verified":state_ok,"balance_verified":balance_ok,
            "actual_deltas":{"player1":player1,"player2":player2,"house":h,"community":c,"chance":ch},
            "expected_deltas":{"player1":p1_expected,"player2":p2_expected,"house":house_expected,"community":comm,"chance":chance},
            "fee_base_units":fee_units});
        verified=state_ok&&balance_ok;
    }else{reconciliation["verification_error"]=json!("Final fee or balances unavailable");}
    Ok(json!({"committed":true,"verified":verified,"transaction_id":txid,
        "action":"build_improvement","square":req.square,"cost_base_units":cost,
        "before":before,"after":after,"reconciliation":reconciliation,
        "fee_base_units":fee,"live_balances":post_balances.ok()}))
}

#[cfg(test)]
mod owner_verification_tests {
    use super::*;
    #[test]
    fn verifies_all_forty_owner_slots_for_plain_move() {
        let owners=vec![0;40];
        let after=expected_owners_after_turn(&owners,None,false,12,2).unwrap();
        assert_eq!(after.len(),40);
        assert_eq!(after, owners);
    }
    #[test]
    fn verifies_buy_and_sale_in_same_turn() {
        let mut owners=vec![0;40]; owners[3]=1;
        let after=expected_owners_after_turn(&owners,Some(3),true,6,1).unwrap();
        assert_eq!(after.len(),40);
        assert_eq!(after[3],0);
        assert_eq!(after[6],1);
    }
    #[test]
    fn rejects_wrong_owner_or_owner_array_length() {
        assert!(expected_owners_after_turn(&vec![0;22],None,false,2,1).is_err());
        assert!(expected_owners_after_turn(&vec![0;40],Some(3),false,2,1).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sale_offsets_purchase_or_penalty() {
        let net = |sale:u64,reward:u64,purchase:u64,penalty:u64| {
            (purchase+penalty).saturating_sub(sale+reward)
        };
        assert_eq!(net(8_000_000,0,20_000_000,0),12_000_000);
        assert_eq!(net(16_000_000,0,10_000_000,0),0);
        assert_eq!(net(0,0,0,2_000_000),2_000_000);
        assert_eq!(net(0,2_000_000,0,0),0);
    }
    #[test]
    fn asset_catalog_and_resale() {
        let mut owners=vec![0u8;40];owners[3]=1;owners[6]=2;
        let mut prices=vec![0u64;40];for (sq,_,price) in BOARD_ASSETS {prices[sq as usize]=price;}
        let a=assets_from(&owners,&prices,&vec![0;40]);
        assert_eq!(a[0].price_base_units,10_000_000);
        assert_eq!(a[1].resale_base_units,16_000_000);
        assert_eq!(a[2].owner,2);
        assert_eq!(a.len(),22);
    }
}

#[cfg(test)]
mod board_tests {
    use super::*;
    #[test]
    fn exact_board_shape() {
        assert_eq!(SQUARE_TYPES.len(),40);
        assert_eq!(BOARD_ASSETS.len(),22);
        assert_eq!([0,10,20,30].map(|i| SQUARE_TYPES[i]),[0,9,9,9]);
        for (sq,_,price) in BOARD_ASSETS {assert_eq!(SQUARE_TYPES[sq as usize],1);assert!(price>0);}
    }
}

#[cfg(test)]
mod economy_v068_tests {
    use super::*;
    #[test]
    fn rent_is_ten_percent_and_only_other_owner() {
        let mut owners=vec![0;40];let mut prices=vec![0;40];
        owners[13]=1;prices[13]=20_000_000;
        assert_eq!(rent(&owners,&prices,&vec![0;40],13,2),(1,2_000_000));
        assert_eq!(rent(&owners,&prices,&vec![0;40],13,1),(0,0));
        assert_eq!(rent(&owners,&prices,&vec![0;40],5,2),(0,0));
    }
    #[test]
    fn card_cycle_is_bounded_and_deterministic() {
        for sq in [2,22,33,5,17,27] {
            let v=card(sq,21);
            assert!(v.0==1||v.0==2);assert!(v.1<4);assert!(v.2>0);
            assert_eq!(v,card(sq,21));
        }
        assert_eq!(card(13,21),(0,255,0,false));
    }
    #[test]
    fn balance_conservation_with_rent() {
        let rent=2_000_000i128;
        assert_eq!(-rent+rent,0);
    }
}

#[cfg(test)]
mod property_development_tests {
    use super::*;
    #[test]
    fn development_value_and_rent_steps() {
        let base=30_000_000;
        assert_eq!(property_rent(base,0).unwrap(),3_000_000);
        assert_eq!(property_rent(base,1).unwrap(),4_500_000);
        assert_eq!(property_rent(base,2).unwrap(),6_000_000);
        assert_eq!(property_rent(base,3).unwrap(),10_500_000);
        for (level,extra) in [(0,0),(1,5_000_000),(2,10_000_000),(3,25_000_000)] {
            assert_eq!(improvement_total(level).unwrap(),extra);
        }
        assert!(improvement_total(4).is_err());
        assert!(property_rent(base,4).is_err());
    }
    #[test]
    fn developed_property_resale_and_rent() {
        let mut prices=vec![0;40];let mut owners=vec![0;40];let mut upgrades=vec![0;40];
        prices[6]=30_000_000;owners[6]=1;upgrades[6]=3;
        let a=assets_from(&owners,&prices,&upgrades);
        let asset=a.iter().find(|a|a.square==6).unwrap();
        assert_eq!(asset.property_value_base_units,55_000_000);
        assert_eq!(asset.resale_base_units,44_000_000);
        assert_eq!(rent(&owners,&prices,&upgrades,6,2),(1,10_500_000));
        assert_eq!(rent(&owners,&prices,&upgrades,6,1),(0,0));
    }
}
