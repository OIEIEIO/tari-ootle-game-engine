'use strict';
const $ = id => document.getElementById(id);
let game=null, snapshot=null, busy=false, liveEnabled=false, pendingTurn=null, animating=false;
const fmt = n => (Number(n)/1e6).toFixed(6);
const signed = n => `${n>0?'+':''}${fmt(n)} tTARI`;
const icons = ['🚙','⛵'];
const faces = ['⚀','⚁','⚂','⚃','⚄','⚅'];
const kinds = ['start','asset','chance','community','fee','penalty','reward','utility','transit','corner'];
const GO_REWARD = 20_000_000n;
function clearDice(){
  for(const id of ['dice-a','dice-b']){const die=$(id);die.classList.remove('rolling');die.classList.add('blank');die.textContent='';}
}
function put(id,val){const el=$(id);if(el)el.textContent=String(val);}
function notify(text,bad=false){put('message',text);$('message').className='message '+(bad?'error':'ok');}
async function api(path,method='GET',body){
  const r=await fetch(path,{method,headers:{'Content-Type':'application/json','Accept':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});
  const data=await r.json();
  if(!r.ok||data.error) throw Error(data.error||`HTTP ${r.status}`);
  return data;
}
function randDie(){
  const b=new Uint32Array(1), limit=Math.floor(4294967296/6)*6;
  do{crypto.getRandomValues(b);}while(b[0]>=limit);
  return b[0]%6+1;
}
function playerBalance(p){const a=snapshot?.accounts?.find(a=>a.id===`player${p}`);return a?BigInt(a.balance_base_units):null;}
function currentAssets(p){return game?.assets?.filter(a=>a.owner===p)||[];}
function displayPosition(p){
  if(pendingTurn && pendingTurn.player===p) return pendingTurn.landing;
  return p===1?game.position1:game.position2;
}
function coord(i){
  if(i===0)return [1,1];
  if(i<=9)return [1,i+1];
  if(i===10)return [1,11];
  if(i<=19)return [i-9,11];
  if(i===20)return [11,11];
  if(i<=29)return [11,31-i];
  if(i===30)return [11,1];
  return [41-i,1];
}
function stripSide(i){
  if(i<=10) return 'bottom';
  if(i<=20) return 'left';
  if(i<=30) return 'top';
  return 'right';
}
function penaltyFor(square){return [4,24,37].includes(square)?1_000_000n:square===7?2_000_000n:0n;}
function rewardFor(square){return square===9?2_000_000n:0n;}
function passGoReward(from,to){return to<from?GO_REWARD:0n;}
function cardFor(square,turnCount){
  const kind=[2,22,33].includes(square)?1:[5,17,27].includes(square)?2:0;
  if(!kind)return null;
  const id=(turnCount+square)%4;
  const descriptions=kind===1?
    ['Chance: Lucky find','Chance: Repair bill','Chance: Bonus payout','Chance: Inspection fee']:
    ['Community: Shared reward','Community: Contribution','Community: Community grant','Community: Community levy'];
  const amounts=kind===1?[2_000_000n,-1_000_000n,3_000_000n,-2_000_000n]:[1_000_000n,-1_000_000n,2_000_000n,-2_000_000n];
  return {kind,id,title:descriptions[id],amount:amounts[id]};
}
function rentFor(square,player){
  if(square.owner && square.owner!==player && square.price_base_units>0){
    return {owner:square.owner,amount:BigInt(square.rent_base_units)};
  }
  return {owner:0,amount:0n};
}
function eventText(ev){
  if(!ev||!ev.kind)return 'No card or rent event yet.';
  const sign=ev.kind===3?'−':ev.positive?'+':'−';
  return `Last confirmed: ${ev.label} · Player ${ev.player} · Square ${ev.square} · ${sign}${fmt(ev.amount_base_units)} tTARI${ev.kind===3?` → Player ${ev.recipient}`:''}`;
}
function pendingSquare(){
  if(!game||!pendingTurn) return null;
  return game.squares[pendingTurn.landing];
}
let detailSquare=null;
let featureSquare=null;
let eventDismissed=false;
let walkwayUpdateQueued=false;
let visitStory=null;
let autoSettling=false;
function exitVisit(){
  eventDismissed=true;
  closeAssetFeature();
  renderLandingStage();
  if(game){renderBoard();renderTown();}
}
const ASSET_THEMES={
  1:{venue:'The Grand Hotel',kicker:'Boutique stay · evening check-in',image:'/assets/asset-a-storefront.svg',
     blurb:'A warm boutique stay overlooking Town Square, with a glowing lobby, concierge desk, and late-night lounge.',
     acknowledgement:'We hope you enjoyed your stay with us this evening.',actions:['Explore suites',"View tonight's amenities",'Inspect property details']},
  13:{venue:'Harbor Table Bistro',kicker:"Kitchen service · chef's tasting room",image:'/assets/asset-f-bistro.svg',
     blurb:"A lively bistro with an open kitchen, candle-lit tables, and a chef's counter that turns each visit into an event.",
     acknowledgement:'Thanks for joining us for service. We hope the menu treated you well.',actions:['Browse the tasting menu','See house specials','Inspect property details']},
  23:{venue:'Lantern House Gallery',kicker:'Private gallery · lounge evening',image:'/assets/asset-l-lounge.svg',
     blurb:'A moody gallery and lounge with framed exhibits, soft light, and a premium evening room experience.',
     acknowledgement:'Thank you for stopping in. Take a moment to enjoy the collection before you continue on.',actions:['View the collection',"See tonight's featured room",'Inspect property details']},
};

function visibleSquareName(s){return s.square===10?'Maintenance Bay':s.square===30?'Go to Maintenance':s.label;}
function hideAssetDetails(){ $('asset-popover').hidden=true;detailSquare=null; }
function positionAssetDetails(e){
  if(detailSquare===null)return;
  const el=$('asset-popover');
  const width=el.offsetWidth||265,height=el.offsetHeight||220;
  el.style.left=`${Math.max(8,Math.min(window.innerWidth-width-8,(e.clientX||0)+16))}px`;
  el.style.top=`${Math.max(8,Math.min(window.innerHeight-height-8,(e.clientY||0)+16))}px`;
}
function showAssetDetails(s,e){
  const pop=$('asset-popover');detailSquare=s.square;pop.replaceChildren();
  const h=document.createElement('h3');h.textContent=`${visibleSquareName(s)} · Square ${s.square}`;pop.append(h);
  const add=(key,value)=>{const row=document.createElement('div');row.className='detail-row';
    const k=document.createElement('span');k.textContent=key;
    const v=document.createElement('strong');v.textContent=value;row.append(k,v);pop.append(row);};
  add('Type',kinds[s.kind]||'Square');
  if(s.price_base_units){
    add('Owner',s.owner?`Player ${s.owner}`:'Unowned');
    add('Base lot value',`${fmt(s.price_base_units)} tTARI`);
    add('Improvements',s.improvement_level===3?'2 houses + hotel':`${s.improvement_level} house${s.improvement_level===1?'':'s'}`);
    add('Property value',`${fmt(s.property_value_base_units)} tTARI`);
    add('Current rent',`${fmt(s.rent_base_units)} tTARI`);
    add('Property resale (80%)',`${fmt(Math.floor(s.property_value_base_units*0.8))} tTARI`);
  }
  pop.hidden=false;positionAssetDetails(e);
}
function renderImprovements(){
  const selector=$('build-asset'),prev=selector.value;
  selector.replaceChildren();
  const eligible=currentAssets(game.active_player);
  for(const asset of eligible){const opt=document.createElement('option');opt.value=String(asset.square);
    opt.textContent=`${asset.label} · ${asset.improvement_level}/3 improvements`;selector.append(opt);}
  if([...selector.options].some(o=>o.value===prev))selector.value=prev;
  const asset=eligible.find(a=>String(a.square)===selector.value);
  const level=asset?.improvement_level??3;
  const cost=level<2?5_000_000:level===2?15_000_000:0;
  $('build-price').value=cost?`${level===2?'Hotel':'House'} · ${fmt(cost)} tTARI`:'Fully developed';
  const canPay=(playerBalance(game.active_player)??-1n)>=BigInt(cost);
  $('build-submit').disabled=busy||!liveEnabled||Boolean(pendingTurn)||Boolean(game.pending_offer)||!asset||level>=3||!canPay;
}
async function buildImprovement(){
  if(!game||busy||pendingTurn||game.pending_offer)return;
  const asset=game.assets.find(a=>String(a.square)===$('build-asset').value && a.owner===game.active_player);
  if(!asset||asset.improvement_level>=3)return;
  if(window.prompt(`Build on ${asset.label}, stage ${asset.improvement_level+1}. Type exactly: LIVE BUILD`,'LIVE BUILD')!=='LIVE BUILD')return;
  busy=true;renderTown();notify('Submitting construction transaction…');
  try{const receipt=await api('/api/game/build','POST',{square:asset.square,expected_level:asset.improvement_level,
    expected_round:game.round,expected_turn_count:game.turn_count,confirmation:'LIVE BUILD'});
    put('technical',JSON.stringify(receipt,null,2));
    put('last-result',`Build · ${receipt.transaction_id||'not submitted'} · ${receipt.committed?(receipt.verified?'COMMITTED / RECONCILED':'COMMITTED / UNVERIFIED'):'NOT COMMITTED'}`);
    await refresh();notify(receipt.committed&&receipt.verified?'Property improvement verified on-chain.':'Build not completely verified: inspect receipt.',!(receipt.committed&&receipt.verified));
  }catch(e){await refresh();notify(`Build error: ${e.message}`,true)}
  finally{busy=false;if(game)renderTown();}
}
$('build-asset').addEventListener('change',renderImprovements);
$('build-submit').addEventListener('click',buildImprovement);

// v0.6.11: the board track is a view of confirmed positions, never a new source of state.
function scheduleWalkwayMarkers(){
  if(walkwayUpdateQueued)return;
  walkwayUpdateQueued=true;
  requestAnimationFrame(()=>{walkwayUpdateQueued=false;renderWalkwayMarkers();});
}
function renderWalkwayMarkers(){
  const track=$('walkway'), root=$('walkway-markers');
  if(!game||!track||!root)return;
  root.replaceChildren();
  const wr=track.getBoundingClientRect();
  const thickness=Math.max(28,Math.min(76,Math.min(wr.width,wr.height)*0.105));
  const places=[1,2].map(p=>({p,square:displayPosition(p)}));
  for(const {p,square} of places){
    const tile=[...$('board').querySelectorAll(':scope > .tile')].find(el=>Number(el.dataset.square)===square);
    if(!tile)continue;
    const r=tile.getBoundingClientRect(),side=stripSide(square);
    let x,y,edgeX,edgeY;
    if(side==='bottom'){
      edgeX=r.left+r.width/2-wr.left;edgeY=0;
      x=Math.max(thickness/2,Math.min(wr.width-thickness/2,edgeX));y=thickness/2;
    } else if(side==='top'){
      edgeX=r.left+r.width/2-wr.left;edgeY=wr.height;
      x=Math.max(thickness/2,Math.min(wr.width-thickness/2,edgeX));y=wr.height-thickness/2;
    } else if(side==='left'){
      edgeX=wr.width;edgeY=r.top+r.height/2-wr.top;
      x=wr.width-thickness/2;y=Math.max(thickness/2,Math.min(wr.height-thickness/2,edgeY));
    }else{
      edgeX=0;edgeY=r.top+r.height/2-wr.top;
      x=thickness/2;y=Math.max(thickness/2,Math.min(wr.height-thickness/2,edgeY));
    }
    // Separate visitors on the same square without changing the square connector.
    const together=places.filter(v=>v.square===square).map(v=>v.p);
    const offset=together.length>1?(together.indexOf(p)-(together.length-1)/2)*34:0;
    if(side==='bottom'||side==='top')x=Math.max(16,Math.min(wr.width-16,x+offset));
    else y=Math.max(16,Math.min(wr.height-16,y+offset));
    const owner=game.squares[square]?.owner||0;
    const carpet=document.createElement('div');carpet.className=`visitor-carpet owner${owner}`;
    const tx=edgeX-x,ty=edgeY-y,dist=Math.hypot(tx,ty);
    carpet.style.left=`${x}px`;carpet.style.top=`${y}px`;
    carpet.style.width=`${dist}px`;
    carpet.style.transform=`translateY(-50%) rotate(${Math.atan2(ty,tx)}rad)`;
    root.append(carpet);
    const token=document.createElement('div');token.className=`walkway-token p${p}`;
    token.style.left=`${x}px`;token.style.top=`${y}px`;
    token.textContent=icons[p-1];token.title=`Player ${p}: Square ${square}`;
    root.append(token);
  }
}
function closeAssetFeature(){
  featureSquare=null;
  $('asset-feature').hidden=true;$('center-player-view').hidden=false;
}
function openAssetFeature(square){
  if(![1,2].some(p=>displayPosition(p)===square.square))return;
  featureSquare=square.square;
  eventDismissed=false;
  $('center-player-view').hidden=true;$('asset-feature').hidden=false;
  hideAssetDetails();renderAssetFeature();
}
function renderAssetFeature(){
  if(featureSquare===null||!game)return;
  const square=game.squares.find(s=>s.square===featureSquare);
  if(!square||!square.price_base_units)return closeAssetFeature();
  const container=$('feature-content');container.replaceChildren();
  const heading=document.createElement('div');heading.className='feature-title';
  const h=document.createElement('h2');h.textContent=`${square.label} · Square ${square.square}`;
  const ownership=document.createElement('span');ownership.className=`feature-owner owner${square.owner||0}`;
  ownership.textContent=square.owner?`Owned by Player ${square.owner}`:'Unowned';
  heading.append(h,ownership);container.append(heading);
  put('feature-visit',`${[1,2].filter(p=>displayPosition(p)===square.square).map(p=>`Player ${p}`).join(' & ')} visiting`);
  const theme=assetTheme(square);
  if(theme.image){
    const showcase=document.createElement('div');showcase.className='feature-showcase';
    const img=document.createElement('img');img.src=theme.image;img.alt=`Illustrated venue for ${square.label}`;
    showcase.append(img);container.append(showcase);
  }
  const description=document.createElement('p');description.className='feature-description';
  description.textContent=theme.blurb;
  container.append(description);
  const level=square.improvement_level||0;
  const upgrades=document.createElement('div');upgrades.className='feature-upgrades';
  upgrades.textContent=`Development · ${level===3?'2 houses + hotel':level===1?'1 house':level===2?'2 houses':'Unimproved'}`;
  const slots=document.createElement('div');slots.className='feature-development';
  for(let n=0;n<3;n++){
    const item=document.createElement('span');item.className=`feature-slot ${n===2?'hotel':'house'} ${level>n?'built':''}`;
    item.textContent=n===2?'▦':'⌂';item.title=n===2?'Hotel':'House '+(n+1);slots.append(item);
  }
  upgrades.append(slots);container.append(upgrades);
  const details=document.createElement('div');details.className='feature-finances';
  const rows=[['Base lot value',fmt(square.price_base_units)],['Total property value',fmt(square.property_value_base_units)],['Current rent',fmt(square.rent_base_units)],['Property resale (80%)',fmt(Math.floor(square.property_value_base_units*.8))]];
  for(const [label,value] of rows){
    const row=document.createElement('div');row.className='feature-finance-row';
    const key=document.createElement('span');key.textContent=label;
    const val=document.createElement('strong');val.textContent=value+' tTARI';row.append(key,val);details.append(row);
  }
  container.append(details);
  const base=square.price_base_units/1e6;
  const values=[.10,.15,.20,.35].map(mult=>base*mult),chart=document.createElement('div');chart.className='feature-rent-chart';
  const ch=document.createElement('strong');ch.textContent='Rent by development · tTARI';chart.append(ch);
  const bars=document.createElement('div');bars.className='feature-bars';
  ['Lot','1 house','2 houses','Hotel'].forEach((label,i)=>{
    const bar=document.createElement('div');bar.className='feature-bar';
    const amount=document.createElement('span');amount.textContent=values[i].toFixed(1);
    const column=document.createElement('div');column.style.height=`${values[i]/Math.max(...values)*90}px`;
    if(i===level)column.classList.add('current');
    const caption=document.createElement('small');caption.textContent=label;
    bar.append(amount,column,caption);bars.append(bar);
  });chart.append(bars);container.append(chart);
  const note=document.createElement('p');note.className='feature-footnote';
  note.textContent='View only · Purchases, trades, improvements, and dice remain in Player & Turn.';container.append(note);
}
$('back-to-game').addEventListener('click',exitVisit);
window.addEventListener('resize',scheduleWalkwayMarkers);
if(typeof ResizeObserver!=='undefined')new ResizeObserver(scheduleWalkwayMarkers).observe($('board'));

function renderBoard(){
  const root=$('board');
  for(const tile of root.querySelectorAll(':scope > .tile')) tile.remove();
  for(const square of game.squares){
    const i=square.square,[row,col]=coord(i),tile=document.createElement('div');
    const corner=i%10===0;
    tile.className=`tile ${kinds[square.kind]||'unknown'} ${corner?'corner':''} ${square.owner?`owner${square.owner}`:''} marker-${stripSide(i)}`;
    if(pendingTurn && pendingTurn.landing===i){
      tile.classList.add('pending', pendingTurn.player===2?'pending-p2':'pending-p1');
    }
    if(!pendingTurn && visitStory && !eventDismissed && visitStory.square===i){
      tile.classList.add('visited');
    }
    tile.dataset.square=String(i);
    if(square.price_base_units && [1,2].some(p=>displayPosition(p)===i))tile.classList.add('visitable');
    tile.style.gridRow=String(row); tile.style.gridColumn=String(col);
    tile.tabIndex=0;
    tile.setAttribute('aria-label',`Square ${i}: ${square.label}, ${square.owner?`owned by Player ${square.owner}`:'unowned'}`);
    tile.addEventListener('pointerenter',e=>showAssetDetails(square,e));
    tile.addEventListener('pointermove',e=>positionAssetDetails(e));
    tile.addEventListener('pointerleave',hideAssetDetails);
    tile.addEventListener('focus',e=>showAssetDetails(square,{clientX:e.target.getBoundingClientRect().x+30,clientY:e.target.getBoundingClientRect().y+30}));
    tile.addEventListener('blur',hideAssetDetails);
    tile.addEventListener('click',e=>{
      if(featureSquare===i){exitVisit();}
      else if(square.price_base_units && [1,2].some(p=>displayPosition(p)===i)){openAssetFeature(square);renderLandingStage();}
      else if((pendingTurn && pendingTurn.landing===i) || (!pendingTurn && visitStory && visitStory.square===i)){eventDismissed=!eventDismissed;renderLandingStage();renderBoard();}
      else if(detailSquare===i){hideAssetDetails();}else{showAssetDetails(square,e);}
    });

    const strip=document.createElement('div'); strip.className=`owner-strip ${stripSide(i)}`;
    const inner=document.createElement('div'); inner.className='tile-inner';
    const num=document.createElement('div'); num.className='tile-number'; num.textContent=i===0?'GO':i;
    const name=document.createElement('div'); name.className='tile-name'; name.textContent=square.price_base_units?'':visibleSquareName(square);
    const detail=document.createElement('div'); detail.className='tile-price';
    detail.textContent='';
    if(square.price_base_units){
      const improvements=document.createElement('div'); improvements.className=`development-strip ${stripSide(i)}`;
      const level=square.improvement_level||0;
      for(let idx=0;idx<3;idx++){
        const slot=document.createElement('span');slot.className=`dev-slot ${idx===2?'hotel':'house'} ${level>idx?'built':''}`;
        slot.title=idx===2?'Hotel':'House '+(idx+1);
        slot.innerHTML=idx===2
          ? '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 22V3h16v19h-6v-5h-4v5H4zm3-15v3h3V7H7zm7 0v3h3V7h-3zM7 12v3h3v-3H7zm7 0v3h3v-3h-3z"/></svg>'
          : '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 11 12 2l10 9h-3v11h-6v-7h-3v7H5V11H2z"/></svg>';
        improvements.append(slot);
      }
      tile.append(improvements);
    }
    if(!pendingTurn && !autoSettling && game.last_event?.kind && game.last_event.square===i){
      tile.classList.add('revealed');
      const ev=document.createElement('div');ev.className='tile-reveal';
      ev.textContent=game.last_event.kind===3?`Rent ${fmt(game.last_event.amount_base_units)}`:
        `${game.last_event.positive?'+':'−'}${fmt(game.last_event.amount_base_units)} tTARI`;
      inner.append(ev);
    }
    inner.append(num,name,detail);
    tile.append(strip,inner);
    root.append(tile);
  }
  scheduleWalkwayMarkers();
  if(featureSquare!==null){
    const occupied=[1,2].some(p=>displayPosition(p)===featureSquare);
    if(!occupied)closeAssetFeature();
    else renderAssetFeature();
  }
}
function renderPlayers(){
  const root=$('players'); root.replaceChildren();
  for(const p of [1,2]){
    const div=document.createElement('div'); div.className=`player ${game.active_player===p?'active':''}`;
    const left=document.createElement('div');
    const n=document.createElement('div'); n.className='name'; n.textContent=`${icons[p-1]} Player ${p}${game.active_player===p?' · ACTIVE':''}`; left.append(n);
    const position=document.createElement('div'); position.className='pos'; position.textContent=`Square ${displayPosition(p)} · ${currentAssets(p).length} assets`; left.append(position);
    const bal=document.createElement('div'); bal.className='bal';
    const entry=snapshot?.accounts?.find(a=>a.id===`player${p}`);
    bal.textContent=entry?`${entry.balance} tTARI`:'Balance unavailable';
    div.append(left,bal); root.append(div);
  }
  const portfolio=$('player-portfolio');portfolio.replaceChildren();
  const header=document.createElement('div');header.className='portfolio-heading';
  header.textContent=`Player ${game.active_player} assets · ${currentAssets(game.active_player).length}`;
  portfolio.append(header);
  const owned=currentAssets(game.active_player);
  if(!owned.length){const empty=document.createElement('div');empty.className='portfolio-empty';empty.textContent='No assets owned.';portfolio.append(empty);}
  for(const asset of owned){
    const line=document.createElement('div');line.className='portfolio-item';
    const name=document.createElement('span');name.textContent=`${asset.label} · Square ${asset.square}`;
    const value=document.createElement('strong');value.textContent=`${fmt(asset.property_value_base_units)} tTARI`;
    line.append(name,value);portfolio.append(line);
  }
}
function improvementText(level){
  if(level>=3)return '2 houses + hotel';
  if(level===2)return '2 houses';
  if(level===1)return '1 house';
  return 'Unimproved';
}
function assetTheme(square){
  const theme=ASSET_THEMES[square.square];
  if(theme)return theme;
  return {venue:visibleSquareName(square),kicker:'Property visit · Town Square district',image:null,
    blurb:'A live property on the Ootle board. Inspect the tile for ownership, valuation, rent, and development details.',
    acknowledgement:'Thanks for stopping by.',actions:['Inspect property details','Return to Town Square']};
}
function buildScene(model){
  const scene=$('stage-scene');scene.replaceChildren();
  if(!model.scene?.src){scene.hidden=true;return;}
  const img=document.createElement('img');img.src=model.scene.src;img.alt=model.scene.alt||model.title;
  const caption=document.createElement('div');caption.className='stage-scene-caption';caption.textContent=model.scene.caption||'';
  scene.append(img,caption);scene.hidden=false;
}
function buildStageBody(lines){
  const root=$('stage-event');root.replaceChildren();
  for(const line of lines||[]){const p=document.createElement('p');p.textContent=line;root.append(p);}
}
function buildStageActions(actions){
  const root=$('stage-actions');root.replaceChildren();
  if(!actions||!actions.length){root.hidden=true;return;}
  const resolved=Boolean(visitStory && !pendingTurn);
  for(const action of actions){
    if(resolved && /return to town square/i.test(action)){
      const button=document.createElement('button');button.type='button';button.className='stage-chip stage-cta';
      button.textContent='Acknowledge · Return to Town Square';button.addEventListener('click',exitVisit);root.append(button);
    }else{
      const chip=document.createElement('span');chip.className='stage-chip';chip.textContent=action;
      chip.title='Preview only — select the occupied property square to inspect details';root.append(chip);
    }
  }
  if(resolved && !actions.some(a=>/return to town square/i.test(a))){
    const button=document.createElement('button');button.type='button';button.className='stage-chip stage-cta';
    button.textContent='Acknowledge · Return to Town Square';button.addEventListener('click',exitVisit);root.append(button);
  }
  root.hidden=false;
}
function applyStageModel(model){
  put('stage-kicker',model.kicker||'');
  put('stage-title',model.title||'');
  put('stage-subtitle',model.subtitle||'');
  buildScene(model);
  buildStageBody(model.lines||[]);
  buildStageActions(model.actions||[]);
  put('stage-hint',model.hint||'Select the highlighted square to inspect or close this visit.');
}
function evaluatePendingTurn(turn){
  const p=game.active_player,to=turn.landing,from=turn.from,square=game.squares[to],asset=game.assets.find(a=>a.square===to)||null;
  const penalty=penaltyFor(to),reward=rewardFor(to),goReward=passGoReward(from,to),card=cardFor(to,game.turn_count),owed=rentFor(square,p);
  const canBuy=Boolean(asset&&!asset.owner);
  const starting=playerBalance(p);
  const baseNet=-penalty+reward+goReward+(card?card.amount:0n)-owed.amount;
  const enoughBase=starting!==null && starting+baseNet>=0n;
  return {p,to,from,square,asset,penalty,reward,goReward,card,owed,canBuy,starting,baseNet,enoughBase};
}
function selectedProjection(base){
  const sold=game.assets.find(a=>String(a.square)===$('sell').value&&a.owner===base.p);
  const buy=base.canBuy&&$('purchase').value==='buy';
  const price=buy?BigInt(base.asset.price_base_units):0n;
  const proceeds=sold?BigInt(sold.resale_base_units):0n;
  const net=base.baseNet+proceeds-price;
  const enough=base.starting!==null && base.starting+net>=0n;
  return {sold,buy,price,proceeds,net,enough};
}
function pendingVisitModel(base,projection){
  const square=base.square;
  if(base.asset){
    const theme=assetTheme(square);
    if(base.canBuy){
      return {square:square.square,kicker:'Property available · player decision required',title:base.asset.label,
        subtitle:`Purchase price · ${fmt(base.asset.price_base_units)} tTARI`,scene:theme.image?{src:theme.image,alt:theme.venue,caption:theme.kicker}:null,
        lines:[theme.blurb,'This property is currently unowned. Choose whether to acquire it from the Player actions panel.'],
        actions:['Buy the property','Pass this visit','Click the tile for the full property profile'],
        hint:'Use the Player actions panel to buy or pass. Click the tile for the full property profile.'};
    }
    if(!projection.enough){
      return {square:square.square,kicker:'Financial recovery required',title:base.asset.label,
        subtitle:'Automatic settlement paused',scene:theme.image?{src:theme.image,alt:theme.venue,caption:theme.kicker}:null,
        lines:[`A charge of ${fmt(base.owed.amount||0n)} tTARI must settle before the turn can complete.`,
          'Select a recovery sale in the Player actions panel if you need liquidity before the transaction can be submitted.'],
        actions:['Choose a recovery sale','Complete the turn manually','Click the tile for full property details'],
        hint:'Resolve recovery from the Player actions panel, then complete the turn.'};
    }
    return {square:square.square,kicker:'Visit in progress',title:theme.venue,subtitle:'Preparing automatic settlement',
      scene:theme.image?{src:theme.image,alt:theme.venue,caption:theme.kicker}:null,
      lines:[theme.acknowledgement,'Straightforward visits settle automatically once the chain confirms the transaction.'],
      actions:theme.actions,hint:'Automatic settlement is being submitted.'};
  }
  const title=visibleSquareName(square);
  if(!projection.enough){
    return {square:square.square,kicker:'Financial recovery required',title,subtitle:'Automatic settlement paused',
      lines:['This landing needs a charge or transfer that your current bankroll cannot cover automatically.',
        'Use the Player actions panel to choose a recovery sale before completing the turn.'],
      actions:['Choose a recovery sale','Complete the turn manually'],hint:'Resolve recovery from the Player actions panel.'};
  }
  return {square:square.square,kicker:'Square event · automatic settlement',title,subtitle:'Preparing automatic settlement',
    lines:['This landing has no purchase decision. The board is settling the result automatically on-chain.'],
    actions:['Automatic settlement in progress'],hint:'Please wait for chain confirmation.'};
}
function buildResolvedVisit(turn,result){
  const after=result.after||game;
  const square=(after.squares||game.squares).find(s=>s.square===turn.landing)||game.squares[turn.landing];
  const asset=(after.assets||game.assets).find(a=>a.square===turn.landing)||game.assets.find(a=>a.square===turn.landing)||null;
  const theme=asset?assetTheme(square):null;
  const op=result.operation||{};
  const story={square:turn.landing,kicker:'Visit complete · confirmed on-chain',title:asset?asset.label:visibleSquareName(square),subtitle:'',lines:[],actions:[],scene:null,hint:''};
  if(asset && theme){
    story.kicker=theme.kicker;
    story.scene=theme.image?{src:theme.image,alt:theme.venue,caption:theme.venue}:null;
  }
  if(asset){
    if(op.buy){
      story.subtitle='Property acquired';
      story.lines.push(`You acquired ${asset.label} for ${fmt(op.purchase_base_units||asset.price_base_units)} tTARI. The deed is now reflected on-chain.`);
      story.lines.push('Open the highlighted tile to inspect the full property profile and development track.');
      story.actions=['New property added to your portfolio','Inspect property details'];
    } else if((op.rent_base_units||0)>0){
      story.subtitle=`Rent settled · ${fmt(op.rent_base_units)} tTARI`;
      story.lines.push((theme?.acknowledgement)||'Thanks for visiting.');
      story.lines.push(`${fmt(op.rent_base_units)} tTARI was transferred to Player ${op.rent_owner} and confirmed on-chain.`);
      story.actions=(theme?.actions)||['Inspect property details'];
    } else if(asset.owner===turn.player){
      story.subtitle='You arrived at your own property';
      story.lines.push('No rent was due. This visit was recorded and the board is ready for the next step.');
      if(theme) story.lines.push(theme.blurb);
      story.actions=(theme?.actions)||['Inspect property details'];
    } else if(asset.owner===0){
      story.subtitle='Visit complete';
      story.lines.push('You passed on purchasing this property this turn.');
      story.actions=['Inspect property details'];
    } else {
      story.subtitle='Visit complete';
      story.lines.push('The property visit has been settled and confirmed on-chain.');
      story.actions=['Inspect property details'];
    }
  } else {
    story.title=visibleSquareName(square);
    if((op.card_amount_base_units||0)>0){
      const amount=BigInt(op.card_positive ? op.card_amount_base_units : -op.card_amount_base_units);
      story.subtitle=game.last_event?.label||'Card resolved';
      story.lines.push(`${game.last_event?.label||'Card event'} settled on-chain for ${signed(amount)}.`);
    } else if((op.penalty_base_units||0)>0){
      story.subtitle='Fee settled';
      story.lines.push(`A fee of ${fmt(op.penalty_base_units)} tTARI was settled automatically.`);
    } else if((op.reward_base_units||0)>0 || (op.go_reward_base_units||0)>0){
      story.subtitle='Reward credited';
      if((op.reward_base_units||0)>0) story.lines.push(`${fmt(op.reward_base_units)} tTARI was credited from this square.`);
      if((op.go_reward_base_units||0)>0) story.lines.push(`Passing GO added ${fmt(op.go_reward_base_units)} tTARI to your bankroll.`);
    } else {
      story.subtitle='Visit complete';
      story.lines.push('No direct charge was required here. Enjoy the stop and continue when ready.');
    }
    story.actions=['Return to Town Square'];
  }
  if(op.doubles) story.lines.push('Lucky break — you rolled doubles, so you keep the next turn after settlement.');
  story.hint=asset?'Click the highlighted tile for the full property profile, or click it again to return to the default Town Square.':'Click the highlighted tile or roll again when ready.';
  return story;
}
async function autoSettlePendingTurn(){
  if(!pendingTurn||autoSettling||busy||!liveEnabled||game.pending_offer) return;
  const turn={...pendingTurn};
  autoSettling=true; busy=true; pendingTurn.autoStarted=true;
  $('execute').disabled=true;$('reset-roll').disabled=true;$('roll-dice').disabled=true;
  updateEventPanel();renderLandingStage();renderBoard();renderTown();
  notify(`Submitting automatic settlement for Player ${turn.player}…`);
  try{
    const result=await api('/api/game/turn','POST',{die1:turn.die1,die2:turn.die2,expected_round:game.round,expected_player:turn.player,buy:false,sell_square:null,confirmation:`LIVE TURN ${turn.player}`});
    put('technical',JSON.stringify(result,null,2));
    put('last-result',`Transaction ${result.transaction_id||'not submitted'} · ${result.committed?(result.verified?'COMMITTED / RECONCILED':'COMMITTED / UNVERIFIED'):'NOT COMMITTED'} · Fee ${result.fee_base_units??'—'} base units`);
    const verified=Boolean(result.committed&&result.verified);
    const story=verified?buildResolvedVisit(turn,result):null;
    pendingTurn=null;
    await refresh(story?{visitStory:story}:{});
    notify(verified?`Player ${turn.player}'s visit settled automatically and verified on-chain.`:'Automatic settlement completed but verification needs review.',!verified);
  }catch(e){
    pendingTurn=null;
    clearVisitStory();
    await refresh();
    notify(`Automatic settlement error: ${e.message}. Chain state reloaded.`,true);
  }finally{
    autoSettling=false;busy=false;if(game)updateChoices();
  }
}
function clearVisitStory(){ visitStory=null; }
function renderLandingStage(){
  const stage=$('landing-stage'),center=$('board-center');
  const visible=Boolean(game&&!eventDismissed&&featureSquare===null&&(pendingTurn||visitStory||autoSettling));
  stage.hidden=!visible;center.classList.toggle('event-mode',visible);
  if(!visible)return;
  let model=null;
  if(pendingTurn){
    const base=evaluatePendingTurn(pendingTurn), projection=selectedProjection(base);
    model=pendingVisitModel(base,projection);
  } else if(visitStory) {
    model=visitStory;
  }
  if(autoSettling&&pendingTurn){
    const base=evaluatePendingTurn(pendingTurn), theme=base.asset?assetTheme(base.square):null;
    model={square:base.to,kicker:'Automatic settlement · live submission',title:base.asset?base.asset.label:visibleSquareName(base.square),
      subtitle:'Submitting transaction to the chain…',scene:theme?.image?{src:theme.image,alt:theme.venue,caption:theme.kicker}:null,
      lines:['The board is resolving this visit automatically because no player decision is required.'],
      actions:['Submitting settlement…'],hint:'Please wait for confirmation.'};
  }
  if(model)applyStageModel(model);
}
function updateEventPanel(){
  const panel=$('event-panel');
  if(!game){panel.textContent='No game state loaded.'; return;}
  if(autoSettling&&pendingTurn){
    panel.textContent=`Automatic settlement in progress for Player ${pendingTurn.player} on Square ${pendingTurn.landing}.`;
    return;
  }
  if(!pendingTurn){
    if(visitStory){panel.textContent='Last landing settled automatically. The Town Square is showing the visitor experience; click the highlighted tile for full details.';return;}
    panel.textContent=game.last_event?.kind?eventText(game.last_event):'No pending move. Roll both dice to begin the active player turn.';
    return;
  }
  const base=evaluatePendingTurn(pendingTurn), projection=selectedProjection(base);
  const lines=[`Pending move: Player ${pendingTurn.player} → Square ${pendingTurn.landing} (${visibleSquareName(base.square)})`];
  if(base.canBuy) lines.push(`Decision required: ${base.asset.label} is unowned. Choose whether to buy for ${fmt(base.asset.price_base_units)} tTARI.`);
  else if(!projection.enough) lines.push('Decision required: recovery sale needed before the turn can settle.');
  else lines.push('No decision required: this landing will settle automatically.');
  if(base.owed.amount>0n) lines.push(`Rent impact: ${fmt(base.owed.amount)} tTARI to Player ${base.owed.owner}.`);
  if(base.penalty>0n) lines.push(`Fee impact: ${fmt(base.penalty)} tTARI.`);
  if(base.reward>0n) lines.push(`Reward impact: ${fmt(base.reward)} tTARI.`);
  if(base.goReward>0n) lines.push(`Passing GO: +${fmt(base.goReward)} tTARI.`);
  if(base.card) lines.push(`Card outcome: ${base.card.title} · ${signed(base.card.amount)}.`);
  if(pendingTurn.doubles) lines.push('Doubles were rolled; if settlement completes, the same player keeps the next turn.');
  panel.textContent=lines.join('\n');
}
function updateChoices(){
  if(!game){return;}
  if(!pendingTurn){
    put('landing','Roll the dice to begin the next visit.');
    put('estimate',visitStory?'Automatic settlement completed. Open the highlighted tile for details or roll when the next turn is ready.':'No roll selected.');
    $('purchase-panel').hidden=true;
    $('recovery-panel').hidden=true;
    $('execute').disabled=true;
    $('execute').textContent='Complete turn';
    updateEventPanel();
    renderLandingStage();
    renderTown();
    return;
  }
  const base=evaluatePendingTurn(pendingTurn);
  put('landing',`Player ${base.p} arrived at Square ${base.to} · ${visibleSquareName(base.square)}`);
  $('purchase-panel').hidden=!base.canBuy;
  if(!base.canBuy) $('purchase').value='pass';
  const sell=$('sell'), previous=sell.value; sell.replaceChildren();
  const none=document.createElement('option'); none.value=''; none.textContent='No sale'; sell.append(none);
  for(const a of currentAssets(base.p)) {const option=document.createElement('option'); option.value=String(a.square); option.textContent=`${a.label} — ${fmt(a.resale_base_units)} tTARI`; sell.append(option);}
  if([...sell.options].some(o=>o.value===previous)) sell.value=previous;
  const projection=selectedProjection(base);
  const manualRequired=base.canBuy || !base.enoughBase;
  $('recovery-panel').hidden=base.enoughBase;
  if(base.canBuy){
    put('estimate',`Purchase decision · bankroll after pass: ${base.starting===null?'unknown':fmt(base.starting+base.baseNet)} tTARI
If purchased: Δ ${signed(projection.net)} · bankroll ${base.starting===null?'unknown':fmt(base.starting+projection.net)} tTARI`);
    $('execute').textContent='Submit property decision';
  } else if(!base.enoughBase){
    put('estimate',`Automatic settlement paused · projected bankroll without recovery: ${base.starting===null?'unknown':fmt(base.starting+base.baseNet)} tTARI
Recovery sale +${fmt(projection.proceeds)} · resulting bankroll: ${base.starting===null?'unknown':fmt(base.starting+projection.net)} tTARI`);
    $('execute').textContent='Resolve recovery & execute turn';
  } else {
    put('estimate','No player decision required. This visit is settling automatically on-chain.');
    $('execute').textContent='Automatic settlement';
  }
  const canManualExecute=manualRequired && projection.enough && !busy && liveEnabled && !game.pending_offer;
  $('execute').disabled=!canManualExecute;
  put('dice-result',`Rolled ${pendingTurn.die1} + ${pendingTurn.die2}${pendingTurn.doubles?' · doubles':''}`);
  put('turn-outcome',visibleSquareName(base.square));
  updateEventPanel();
  renderLandingStage();
  renderBoard();
  renderPlayers();
  renderTown();
  if(manualRequired&&!$('layout').classList.contains('board-focus')){
    const actions=document.querySelector('.player-actions'), turn=document.querySelector('.player-turn');
    if(actions)actions.open=true;if(turn)turn.open=true;
  }
  if(!manualRequired && !pendingTurn.autoStarted) void autoSettlePendingTurn();
}

function renderState(v,options={}){
  game=v;
  put('round',game.round);
  put('round-label',`Round ${game.round}`);
  put('current',`Player ${game.active_player}`);
  put('turn-count',game.turn_count);
  put('substate-version',game.substate_version??'Unavailable');
  put('active-player',`Player ${game.active_player}`);
  put('center-icon',icons[game.active_player-1]||'🎲');
  put('addresses',`Game: ${game.component_address}
Template: ${game.template_address}
Player 1: ${game.player1_account}
Player 2: ${game.player2_account}
Source: ${game.source}`);
  put('center-round',`Round ${game.round} · Player ${game.active_player} to move`);
  pendingTurn=null;
  if(options.visitStory){visitStory=options.visitStory;eventDismissed=false;} else if(!visitStory){eventDismissed=false;}
  closeAssetFeature();
  renderLandingStage();
  renderBoard(); renderPlayers();
  renderTown();
  $('sell').value=''; $('purchase').value='pass';
  clearDice();
  put('dice-result','Roll to begin your turn');
  $('reset-roll').disabled=true;
  $('roll-dice').disabled=busy||!liveEnabled||Boolean(game.pending_offer);
  updateChoices();
}
async function refresh(options={}){
  notify('Refreshing on-chain game state and wallet balances…');
  try{
    const [state,balances]=await Promise.all([api('/api/game/state'),api('/api/live/balances')]);
    snapshot=balances; renderState(state.game,options);
    put('chain-light','LIVE / VERIFIED');
    notify(`Verified on-chain state refreshed · ${new Date().toLocaleTimeString()}`);
  }catch(e){
    game=null; $('execute').disabled=true; $('roll-dice').disabled=true; put('chain-light','UNAVAILABLE');
    notify(`On-chain read or UI error: ${e.message}`,true); console.error('Ootle refresh error',e);
  }
}

function renderTown(){
  if(!game) return;
  renderImprovements();
  const offer=game.pending_offer;
  if(offer){const actions=document.querySelector('.player-actions'), tradePanel=document.querySelector('.player-trades');
    if(actions)actions.open=true;if(tradePanel)tradePanel.open=true;}
  const compose=$('town-compose'), pending=$('town-pending');
  compose.hidden=Boolean(offer); pending.hidden=!offer;
  $('trade-asset').replaceChildren();
  const assets=currentAssets(game.active_player);
  for(const a of assets){
    const opt=document.createElement('option');opt.value=String(a.square);
    opt.textContent=`${a.label} · Square ${a.square} (value ${fmt(a.price_base_units)})`;
    $('trade-asset').append(opt);
  }
  $('trade-propose').textContent=`Offer to Player ${3-game.active_player}`;
  $('trade-propose').disabled=busy||!liveEnabled||Boolean(pendingTurn)||Boolean(offer)||!assets.length;
  if(offer){
    const asset=game.assets.find(a=>a.square===offer.square);
    put('town-status',`Pending offer #${offer.nonce}`);
    put('town-offer-summary',`Player ${offer.seller} offers ${asset?.label||'Asset'} (Square ${offer.square}) to Player ${offer.buyer} for ${fmt(offer.price_base_units)} tTARI.\nPlayer ${offer.buyer}: accept or decline to resume the active player's turn.`);
    const affordable=(playerBalance(offer.buyer)??-1n)>=BigInt(offer.price_base_units);
    $('trade-accept').disabled=busy||!liveEnabled||!affordable;
    $('trade-decline').disabled=busy||!liveEnabled;
  } else {put('town-status',`Player ${game.active_player} may offer`);}
  $('roll-dice').disabled=busy||!liveEnabled||Boolean(offer)||Boolean(pendingTurn);
}
async function trade(action){
  if(!game||busy||!liveEnabled||pendingTurn||animating) return;
  const offer=game.pending_offer;
  if(action==='propose'&&offer || action!=='propose'&&!offer) return;
  const square=action==='propose'?Number($('trade-asset').value):offer.square;
  let price;
  if(action==='propose'){
    if(!$('trade-asset').value) return;
    const entered=$('trade-price').value.trim();
    if(!/^(?:\d+)(?:\.\d{1,6})?$/.test(entered)) {notify('Price must have at most six decimal places.',true);return;}
    const [whole,fraction='']=entered.split('.');
    price=BigInt(whole)*1_000_000n+BigInt(fraction.padEnd(6,'0'));
    if(price<=0n || price>9_007_199_254_740_991n){notify('Price outside valid range.',true);return;}
  }
  const phrase=`LIVE TRADE ${action.toUpperCase()}`;
  const subject=action==='propose'?`Offer Square ${square} for ${$('trade-price').value} tTARI to Player ${3-game.active_player}`:
    `${action.toUpperCase()} Player ${offer.seller}'s offer of Square ${offer.square} for ${fmt(offer.price_base_units)} tTARI`;
  if(window.prompt(`${subject}\nOn-chain operation. Type exactly: ${phrase}`,phrase)!==phrase)return;
  busy=true; renderTown();
  notify(`Submitting ${action} to Ootle…`);
  try{
    const result=await api('/api/game/trade','POST',{
      action,square:action==='propose'?square:null,
      price_base_units:action==='propose'?Number(price):null,
      expected_round:game.round,expected_turn_count:game.turn_count,
      expected_nonce:offer?.nonce??null,confirmation:phrase
    });
    put('technical',JSON.stringify(result,null,2));
    put('last-result',`Trade ${action} · ${result.transaction_id||'not submitted'} · ${result.committed?(result.verified?'COMMITTED / RECONCILED':'COMMITTED / UNVERIFIED'):'NOT COMMITTED'}`);
    await refresh();
    notify(result.committed&&result.verified?`Trade ${action} verified on-chain.`:'Trade not fully verified; inspect receipt.',!(result.committed&&result.verified));
  }catch(e){await refresh();notify(`Trade error: ${e.message}`,true);}
  finally{busy=false;if(game)renderTown();}
}
$('trade-propose').addEventListener('click',()=>trade('propose'));
$('trade-accept').addEventListener('click',()=>trade('accept'));
$('trade-decline').addEventListener('click',()=>trade('decline'));

$('reload').addEventListener('click',()=>{clearVisitStory();refresh();});
$('roll-dice').addEventListener('click',()=>{
  if(!game||busy||animating||!liveEnabled||pendingTurn||game.pending_offer) return;
  clearVisitStory();eventDismissed=false;
  const die1=randDie(), die2=randDie(), total=die1+die2;
  animating=true; $('roll-dice').disabled=true;
  const a=$('dice-a'), b=$('dice-b'); a.classList.remove('blank'); b.classList.remove('blank'); a.classList.add('rolling'); b.classList.add('rolling');
  let ticks=0;
  const interval=setInterval(()=>{
    a.textContent=faces[(ticks)%6]; b.textContent=faces[(ticks+3)%6]; ticks++;
    if(ticks>=14){
      clearInterval(interval);
      a.classList.remove('rolling'); b.classList.remove('rolling');
      a.textContent=faces[die1-1]; b.textContent=faces[die2-1];
      animating=false;
      const p=game.active_player; const from=p===1?game.position1:game.position2; const landing=(from+total)%game.board_size;
      clearVisitStory();
      pendingTurn={player:p,from,landing,die1,die2,total,doubles:die1===die2};
      eventDismissed=false;
      closeAssetFeature();
      $('reset-roll').disabled=false;
      updateChoices();
    }
  },70);
});
$('reset-roll').addEventListener('click',()=>{
  if(busy||animating) return;
  pendingTurn=null;
  clearVisitStory();
  clearDice(); put('dice-result','Roll to begin your turn');
  $('roll-dice').disabled=!liveEnabled||Boolean(game.pending_offer); $('reset-roll').disabled=true;
  updateChoices(); renderBoard(); renderPlayers();
});
$('purchase').addEventListener('change',updateChoices);
$('sell').addEventListener('change',updateChoices);
$('focus-board').addEventListener('click',()=>{
  const hide=!$('layout').classList.contains('board-focus'); $('layout').classList.toggle('board-focus',hide);
  put('focus-board',hide?'Show sidebar':'Hide sidebar');
  scheduleWalkwayMarkers();
});
$('execute').addEventListener('click',async()=>{
  if(!game||busy||!liveEnabled||!pendingTurn) return;
  const p=game.active_player, round=game.round;
  const buy=!$('purchase-panel').hidden&&$('purchase').value==='buy';
  const sell_square=!$('recovery-panel').hidden&&$('sell').value!==''?Number($('sell').value):null;
  const phrase=`LIVE TURN ${p}`;
  const confirmation=window.prompt(`Round ${round} · Player ${p}
${buy?'BUY landing asset':'PASS landing asset'}${sell_square===null?'':' · SELL '+sell_square}
One atomic Ootle transaction, including any financial events.
Type exactly: ${phrase}`,phrase);
  if(confirmation!==phrase){notify(`Turn cancelled. Required confirmation: ${phrase}`,true);return;}
  busy=true; $('execute').disabled=true; notify(`Submitting Player ${p}'s on-chain turn…`);
  const turn={...pendingTurn};
  try{
    const result=await api('/api/game/turn','POST',{die1:turn.die1,die2:turn.die2,expected_round:round,expected_player:p,buy,sell_square,confirmation});
    put('technical',JSON.stringify(result,null,2));
    put('last-result',`Transaction ${result.transaction_id||'not submitted'} · ${result.committed?(result.verified?'COMMITTED / RECONCILED':'COMMITTED / UNVERIFIED'):'NOT COMMITTED'} · Fee ${result.fee_base_units??'—'} base units`);
    const verified=Boolean(result.committed&&result.verified);
    const story=verified?buildResolvedVisit(turn,result):null;
    pendingTurn=null;
    await refresh(story?{visitStory:story}:{});
    const extra=result.operation?.doubles ? ' Doubles kept the turn with the same player.' : '';
    notify(result.committed&&result.verified?`Player ${p}'s turn and balances verified on-chain.${extra}`:'Execution not completely verified; inspect transaction evidence.',!(result.committed&&result.verified));
  }catch(e){
    pendingTurn=null; clearVisitStory(); await refresh(); notify(`Transaction error: ${e.message}. Chain state reloaded.`,true);
  }finally{busy=false; if(game) updateChoices();}
});
(async()=>{try{const info=await api('/api/system/info'); put('version',`v${info.version}`); liveEnabled=Boolean(info.live_transactions_enabled);}catch{} await refresh(); if(!liveEnabled) notify('Read-only: restart server with OOTLE_ENABLE_LIVE_TESTS=1',true);})();
