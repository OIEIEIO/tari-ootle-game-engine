/*
  File:      app.js
  Location:  app/web/app.js
  Version:   v0.4.2
  Purpose:   Renders the backend's game view and sends player actions. Mock-up v0.1.5 layout and
             v0.6.18 behaviour: structured errors, developer tab, automatic settlement when no
             decision is needed, purchase decision, recovery sale, construction, private sales,
             sell to the House, shop fee gift for a held seat.
             v0.3.0: setup screen after the intro (seats you control, AI opponents, vehicles), open
             games to join, one game per browser (release to leave), reset and regeneration, build on
             landing, AI and other players' moves animated from polling, leaderboard, receiving
             addresses (show / hide / copy) and clickable transaction ids on the Developer tab.
             v0.4.0:
               - seat colours per seat and 20 SVG pieces (pieces.js);
               - landing scenes (scenes.js) when any human seat lands, in every browser: the panel below
                 the scene carries Buy / Pass with the zone's three businesses, or Build / Skip; Close
                 or Escape closes it; a badge on the square reopens it while a human piece stands there;
               - the news stack replaces the rotating news line: up to N lines (N = seats), newest at the
                 bottom, held as "SINCE YOUR LAST TURN" until you roll, built from every history entry
                 newer than the last one shown, entering 0.4 s apart;
               - sound (audio.js): effects, other seats at reduced volume, ambient per scene, settings in
                 the top bar;
               - the start gate on a browser's first visit (in v0.4.1 replaced by the lobby, below);
               - the game selector is refreshed from every state poll (this game) and every 15 s (all);
               - Developer tab: health, persisted receipts with their verification detail (click a row),
                 an UNVERIFIED filter, and the lot store.
             v0.4.1:
               - Roll works again (the closed landing-scene layer no longer covers the Town Square, app.css);
               - the lobby replaces the start gate: every load and refresh opens a black page with the title,
                 one line saying where you are, and Enter with sound / Enter without sound; the intro then
                 waits for the audio to start, so its music plays;
               - game selector label: "Game 2 · 4 seats: 1 player, 3 AI, 0 open · round 1";
               - choosing a game in the selector switches to its board in place (no lobby, no intro) and
                 sets ?game=N in the address; the browser remembers the last game it was in;
               - setup screen: the button says what it does: Start new game or Take over game N.
             v0.4.2: the lobby is visible from the first paint (index.html); its line reads "Connecting…"
               until the game is read, then where you are, or the read error. Enter pressed before the game
               state has arrived starts the intro as soon as it arrives.
  Notes:     Dumb frontend. Seat count, balances, prices, rent, previews and decisions all come from
             the backend. Dice come from the backend (/roll). The page computes no rules.
             The player token is kept in this browser's localStorage and sent as x-player-token.
*/
'use strict';
const $ = id => document.getElementById(id);
const FACES = ['⚀', '⚁', '⚂', '⚃', '⚄', '⚅'];
const HOLD_MS = 6000, POLL_MS = 3000, BOARD_MS = 15000, NEWS_GAP_MS = 400;
const S = {gameId: null, view: null, dev: null, busy: false, flow: false, override: {}, tiles: [], built: 0,
  info: null, lastError: null, remaining: 0, backTimer: null, pinned: false,
  token: null, me: {game: null, seats: []}, games: [], board: null, animating: false, showAddr: false,
  catalogue: null, newsId: null, stack: [], newsQueue: [], newsBusy: false, wasMyTurn: null,
  receipts: [], receiptSel: null};
const SC = {open: false, square: null, mode: 'view', choice: null, preview: null, seat: null, stage: null};
const P = window.OGEPieces, A = window.OGEAudio, SCN = window.OGEScenes;

const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const wait = ms => new Promise(r => setTimeout(r, ms));
function coord(i) { if (i <= 10) return [1, i + 1]; if (i <= 20) return [i - 9, 11]; if (i <= 30) return [11, 31 - i]; return [41 - i, 1]; }
function side(i) { if (i <= 10) return 'bottom'; if (i <= 20) return 'left'; if (i <= 30) return 'top'; return 'right'; }
function seat(n) { return S.view.seats[n - 1]; }
function sq(i) { return S.view.squares[i]; }
function who(s) { return s.kind === 'human' ? `${s.player}` : s.kind === 'ai' ? `AI ${s.seat}` : `Seat ${s.seat} (open)`; }
function label(s) { return `Seat ${s.seat} · ${s.piece} · ${who(s)}`; }
const pc = (s, px = 18) => P.svg(s.piece_id, s.color, px);
const tokenHtml = (s, px = 28) => P.token(s.piece_id, s.color, px);
const mine = n => S.view && S.view.viewer.seats.includes(n);
const lotOf = q => S.catalogue?.lots.find(l => l.square === q);
const bizTitle = (q, b) => { const l = lotOf(q); return l && b ? SCN.drawing(l, b).title : '#e7eef8'; };

// ---------------- token ----------------
function loadToken() { try { S.token = localStorage.getItem('oge_token'); } catch (e) { S.token = null; } }
function saveToken(t) { S.token = t; try { localStorage.setItem('oge_token', t); } catch (e) { /* in-memory only */ } }

// ---------------- API ----------------
async function api(path, method = 'GET', body) {
  const headers = {'Content-Type': 'application/json', Accept: 'application/json'};
  if (S.token) headers['x-player-token'] = S.token;
  const r = await fetch(path, {method, headers, body: body === undefined ? undefined : JSON.stringify(body)});
  let data; try { data = await r.json(); } catch (e) { data = {error: {code: 'HTTP_ERROR', message: `HTTP ${r.status}`}}; }
  if (!r.ok || data.success === false) {
    const p = data.error || {};
    const err = new Error(p.message || `HTTP ${r.status}`);
    err.code = p.code || 'HTTP_ERROR'; err.payload = p; err.httpStatus = r.status;
    throw err;
  }
  return data.data;
}
function notify(text, kind = '') { const m = $('message'); m.textContent = text; m.className = 'msg' + (kind ? ' ' + kind : ''); }
function showError(e, lbl, reloaded = false) {
  S.lastError = {http_status: e.httpStatus ?? null, error: e.payload || {code: e.code, technical_message: e.message}, frontend_state_reloaded: reloaded};
  $('dev-error').textContent = JSON.stringify(S.lastError, null, 2);
  const d = e.payload?.details || {};
  const extra = d.shortfall_base_units !== undefined ? ` Short by ${fmt(d.shortfall_base_units)} tTARI.` : (d.contract_message ? ` ${d.contract_message}` : '');
  notify(`${lbl}: ${e.message}${extra} [${e.code}]${reloaded ? ' Game state refreshed.' : ''}`, 'bad');
  A.fx('error');
}
function fmt(n) { n = Number(n); const neg = n < 0; n = Math.abs(n); return (neg ? '−' : '') + Math.floor(n / 1e6) + '.' + String(n % 1e6).padStart(6, '0'); }
function parseTtari(s) {
  s = String(s).trim(); if (!/^\d+(\.\d{1,6})?$/.test(s)) return null;
  const [w, f = ''] = s.split('.'); const v = Number(w) * 1e6 + Number(f.padEnd(6, '0'));
  return Number.isSafeInteger(v) && v > 0 ? v : null;
}
async function copy(text, what) {
  try { await navigator.clipboard.writeText(text); notify(`${what} copied`, 'ok'); }
  catch (e) { notify(`Copy is not available in this browser: ${text}`, 'bad'); }
}

// ---------------- board ----------------
function buildBoard() {
  const board = $('board');
  S.tiles.forEach(t => t.remove()); S.tiles = [];
  for (let i = 0; i < 40; i++) {
    const s = sq(i), [r, c] = coord(i), t = document.createElement('div');
    const asset = s.kind === 'asset';
    t.className = 'tile ' + (asset ? 'asset s-' + side(i) : s.kind);
    t.style.gridRow = r; t.style.gridColumn = c;
    t.innerHTML = `<span class="num">${i === 0 ? '' : i}</span>`;
    if (!asset) {
      const nm = document.createElement('div'); nm.className = 'nm'; nm.textContent = s.name;
      if (i === 0) { const a = document.createElement('span'); a.className = 'go-arrow'; a.textContent = '➜'; nm.append(a); }
      t.append(nm);
    } else {
      const st = document.createElement('div'); st.className = 'strip base ' + side(i); t.append(st);
      const d = document.createElement('div'); d.className = 'dev ' + side(i);
      d.innerHTML = '<span></span><span></span><span class="ht"></span>'; t.append(d);
    }
    t.addEventListener('mouseenter', e => { if (!S.pinned) showTile(i, e); });
    t.addEventListener('mousemove', e => { if (!S.pinned) placePop(e); });
    t.addEventListener('mouseleave', () => { if (!S.pinned) hidePop(); });
    t.addEventListener('click', e => { if (window.OGEIntro.isActive()) return; e.stopPropagation(); S.pinned = true; showTile(i, e); });
    S.tiles.push(t); board.append(t);
  }
  S.built = S.view.game.capacity;
}
function renderBoardState() {
  S.tiles.forEach((t, i) => {
    const s = sq(i), st = t.querySelector('.strip');
    if (st) st.style.background = s.owner ? seat(s.owner).color : '';
    const d = t.querySelectorAll('.dev span');
    if (d.length) { d[0].className = s.level >= 1 ? 'h' : ''; d[1].className = s.level >= 2 ? 'h' : ''; d[2].className = 'ht' + (s.level >= 3 ? ' b' : ''); }
  });
  renderTokens();
}
function pos(n) { return S.override[n] ?? seat(n).position; }
function renderTokens(hop) {
  const v = S.view;
  S.tiles.forEach((t, i) => {
    t.querySelector('.tokens')?.remove();
    t.querySelector('.scbtn')?.remove();
    const here = v.seats.filter(s => pos(s.seat) === i);
    if (here.length) {
      const tk = document.createElement('div'); tk.className = 'tokens' + (here.length > 2 ? ' many' : '');
      here.forEach(s => {
        const e = document.createElement('div');
        e.className = 'tok' + (s.seat === hop ? ' hop' : '') + (s.active && !S.flow ? ' active' : '') + (s.out ? ' out' : '') + (s.mine ? ' mine' : '');
        e.style.setProperty('--c', s.color); e.innerHTML = pc(s, '74%'); e.title = label(s);
        tk.append(e);
      });
      t.append(tk);
      // Landing-scene badge: shown while a human piece stands on the square (not during a move).
      if (!S.flow && !S.animating && here.some(s => s.kind === 'human')) {
        const b = document.createElement('button'); b.className = 'scbtn' + (SC.open && SC.square === i ? ' on' : '');
        b.title = SC.open && SC.square === i ? 'Close the landing scene' : 'Open the landing scene';
        b.textContent = '◱';
        b.addEventListener('click', e => { e.stopPropagation(); if (SC.open && SC.square === i) closeScene(); else openScene(i, {mode: 'view', seat: here.find(s => s.kind === 'human').seat}); });
        t.append(b);
      }
    }
    t.classList.remove('activetile');
  });
  if (v.game.started && !S.flow) { const a = seat(v.active), t = S.tiles[pos(v.active)]; t.style.setProperty('--c', a.color); t.classList.add('activetile'); }
  const anyHeld = v.seats.some(s => s.held);
  S.tiles[10].classList.toggle('held', anyHeld); S.tiles[10].classList.toggle('maintpulse', anyHeld);
}
function glow(i, color) {
  S.tiles.forEach(t => t.classList.remove('lastland', 'fade'));
  const t = S.tiles[i]; t.style.setProperty('--glow', color); t.classList.add('lastland');
  requestAnimationFrame(() => requestAnimationFrame(() => t.classList.add('fade')));
}
// Hover card: unchanged content (v0.3.0); the seat's piece name replaces the emoji.
function showTile(i, e) {
  const s = sq(i), pop = $('pop'), kv = $('pop-kv'); kv.replaceChildren();
  const add = (k, v) => { const a = document.createElement('span'); a.textContent = k; const b = document.createElement('b'); b.textContent = v; kv.append(a, b); };
  $('pop-h').textContent = `${s.name} · Square ${i}`;
  if (s.kind === 'asset') {
    add('Owner', s.owner ? label(seat(s.owner)) : 'Unowned');
    add('Base price', s.price_ttari + ' tTARI');
    add('Development', ['none', '1 house', '2 houses', '2 houses + hotel'][s.level]);
    add('Property value', s.value_ttari + ' tTARI');
    add('Current rent', s.rent_ttari + ' tTARI');
    if (S.view.economics.rent_fee_bps) add('Rent fee', `${S.view.economics.rent_fee_bps / 100}% of the rent`);
    add(`Buyback (${S.view.params.buyback_pct}%)`, s.buyback_ttari + ' tTARI');
    if (s.next_cost !== undefined) add('Next ' + s.next_label, s.next_cost_ttari + ' tTARI');
  } else {
    add('Type', s.kind); if (s.effect) add('Effect', s.effect);
    if (i === 10) S.view.seats.filter(x => x.held).forEach(x => add('Held', `Seat ${x.seat} · owes ${x.held_owed_ttari} tTARI`));
  }
  const here = S.view.seats.filter(x => x.position === i); if (here.length) add('Here', here.map(x => `Seat ${x.seat} ${x.piece}`).join(', '));
  pop.style.display = 'block'; placePop(e);
}
function placePop(e) { const pop = $('pop'); pop.style.left = Math.min(innerWidth - 290, e.clientX + 14) + 'px'; pop.style.top = Math.min(innerHeight - pop.offsetHeight - 10, e.clientY + 14) + 'px'; }
function hidePop() { S.pinned = false; $('pop').style.display = 'none'; }

// ---------------- Town Square ----------------
function renderStage() {
  const v = S.view, a = seat(v.active), nx = seat(v.next);
  $('ts-type').textContent = v.game.type_name; $('ts-game').textContent = v.game.id;
  $('ts-rt').textContent = `Round ${v.round} · Turn ${v.turn_count}`;
  $('ts-house').textContent = v.pools.house_ttari;
  $('ts-owned').textContent = `${v.assets_owned} / ${v.assets_total}`;
  $('ts-next').innerHTML = `Seat ${nx.seat} ${pc(nx, 14)}`;
  $('tb-icon').innerHTML = pc(a, 20); $('stage').style.setProperty('--ring', a.color);
  const myTurn = v.viewer.my_turn;
  let whoText = `${label(a)}` + (a.held ? ` · Maintenance · owes ${a.held_owed_ttari}` : '');
  if (!myTurn) whoText += a.kind === 'ai' ? ' · AI is moving' : a.kind === 'open' ? ' · open seat: waiting' : ' · their turn';
  $('tb-who').textContent = whoText;
  const roll = $('roll'), pay = $('roll-pay');
  const offerBlock = !!v.offer, started = v.game.started;
  if (v.roll && v.roll.seat === v.active) { roll.textContent = 'Show roll'; pay.hidden = true; }
  else if (a.held) { roll.textContent = 'Roll for doubles'; pay.hidden = false; pay.textContent = `Pay ${a.held_owed_ttari} and roll`; }
  else { roll.textContent = 'Roll'; pay.hidden = true; }
  roll.hidden = !myTurn; if (!myTurn) pay.hidden = true;
  roll.disabled = pay.disabled = S.busy || S.flow || offerBlock || !started;
  roll.title = offerBlock ? 'Resolve the pending private offer first' : '';
  S.remaining = v.decision.auto_roll_in_secs ?? v.decision.remaining_secs; tickRing();
}
function tickRing() {
  const d = S.view?.decision || {}, W = d.enforced ? d.auto_roll_secs : (d.window_secs || 300);
  const m = Math.floor(S.remaining / 60), s = String(S.remaining % 60).padStart(2, '0');
  $('tb-timer').textContent = d.enforced ? `${m}:${s} until auto-roll` : `${m}:${s} to decide`;
  $('ring').style.strokeDashoffset = String(106.814 * (1 - S.remaining / W));
}
setInterval(() => { if (S.view && S.remaining > 0) { S.remaining--; tickRing(); } }, 1000);
function showLayer(id) { $('idle').classList.toggle('in', id === 'idle'); $('act').classList.toggle('in', id === 'act'); $('turnbar').style.opacity = id === 'idle' ? 1 : 0; }

// ---------------- news stack ----------------
const seatTxt = n => `<b style="color:${seat(n)?.color || '#e7eef8'}">Seat ${n}</b>`;
const amt = (u, sign = '') => `<span class="amt">${sign}${fmt(u)}</span>`;
function bizName(q, id) {
  const l = lotOf(q), b = l?.businesses.find(x => x.id === id) || (id ? null : sq(q)?.business);
  return b ? `<b style="color:${bizTitle(q, b)}">${esc(b.name)}</b>` : `<b>${esc(sq(q)?.name || 'Square ' + q)}</b>`;
}
function assetName(q) { const b = sq(q)?.business; return b ? bizName(q, b.id) : `<b>${esc(sq(q)?.name || 'Square ' + q)}</b>`; }
/** One line per history entry (receipt): its main event, with passing GO added when both happened. */
function newsLine(h) {
  const ev = h.events || [], by = k => ev.find(e => e.kind === k), go = by('go');
  const piece = n => n && seat(n) ? pc(seat(n), 16) : '<span class="nopc">◆</span>';
  let s = h.seat, text = null;
  const e = by('buy') || by('build') || by('rent') || by('card') || by('fee') || by('penalty') || by('maintenance_sent') || by('released') ||
    by('sell') || by('regenerate') || by('reset') || by('offer_accepted') || by('offer_declined') || by('offer') || by('shop_fee') || by('reward') ||
    by('maintenance_held') || by('top_up');
  if (e) switch (e.kind) {
    case 'buy': text = `${seatTxt(e.seat)} bought the ${esc(sq(e.square)?.zone_name || sq(e.square)?.name)} lot` + (e.business ? ` · ${bizName(e.square, e.business)}` : ''); break;
    case 'build': text = `${assetName(e.square)} ${['', 'opened', 'expanded', 'became the flagship'][e.level] || 'was developed'}`; break;
    case 'rent': s = e.owner; text = `${assetName(e.square)} earned ${amt(e.earned)} tTARI rent`; break;
    case 'card': text = `${seatTxt(e.seat)} drew ${esc(e.label)} ${amt(e.amount, e.positive ? '+' : '−')}`; break;
    case 'fee': text = `${seatTxt(e.seat)} paid a ${amt(e.amount)} tTARI fee`; break;
    case 'penalty': text = `${seatTxt(e.seat)} paid a ${amt(e.amount)} tTARI penalty`; break;
    case 'maintenance_sent': text = `${seatTxt(e.seat)} was sent to Maintenance`; break;
    case 'maintenance_held': text = `${seatTxt(e.seat)} stayed in Maintenance · attempt ${e.attempt}`; break;
    case 'released': text = `${seatTxt(e.seat)} left Maintenance`; break;
    case 'sell': text = `${seatTxt(e.seat)} sold ${assetName(e.square)} to the House for ${amt(e.amount)}`; break;
    case 'regenerate': text = `${seatTxt(e.seat)} regenerated at GO`; break;
    case 'reset': text = `${seatTxt(e.seat)} was reset to GO`; break;
    case 'offer': text = `${seatTxt(e.seat)} offered ${assetName(e.square)} to ${seatTxt(e.buyer)} for ${amt(e.amount)}`; break;
    case 'offer_accepted': text = `${seatTxt(e.seat)} bought ${assetName(e.square)} from ${seatTxt(e.seller)} for ${amt(e.amount)}`; break;
    case 'offer_declined': text = `${seatTxt(e.seat)} declined the offer for ${assetName(e.square)}`; break;
    case 'shop_fee': text = `${seatTxt(e.seat)} paid ${seatTxt(e.held_seat)}'s shop fee ${amt(e.amount)}`; break;
    case 'reward': text = `${seatTxt(e.seat)} collected a reward ${amt(e.amount, '+')}`; break;
    case 'top_up': s = 0; text = `The main House topped up the game House ${amt(e.amount, '+')}`; break;
  }
  const mv = by('move');
  if (!text && mv) {
    const q = sq(mv.to);
    text = q?.kind === 'asset' && !q.owner ? `${seatTxt(mv.seat)} passed on the ${esc(q.zone_name || q.name)} lot` : `${seatTxt(mv.seat)} moved to ${esc(q?.name || 'Square ' + mv.to)}`;
    if (go) text = `${seatTxt(go.seat)} passed GO ${amt(go.amount, '+')}` + (q?.kind === 'asset' && !q.owner ? ` · passed on the ${esc(q.zone_name || q.name)} lot` : '');
  } else if (go && e && e.kind !== 'top_up') text += ` · passed GO ${amt(go.amount, '+')}`;
  if (!text) text = esc(h.text);
  return {id: h.id, html: `${piece(s)}<span>${text}</span>`};
}
/** Sounds of a history entry. Other seats' entries play at the reduced volume. */
function newsSounds(h) {
  const other = !mine(h.seat);
  (h.events || []).forEach(e => {
    switch (e.kind) {
      case 'buy': A.fx('purchase', {other}); break;
      case 'build': A.fx(e.level >= 3 ? 'hotel' : e.level === 2 ? 'build2' : 'build1', {other}); break;
      case 'rent': if (mine(e.owner)) A.fx('rent_earned'); else if (mine(e.seat)) A.fx('rent_paid'); else A.fx('rent_paid', {other: true}); break;
      case 'card': A.fx(e.card_kind === 1 ? 'chance' : 'community', {other}); break;
      case 'fee': case 'penalty': A.fx('fee', {other}); break;
      case 'go': A.fx('go', {other}); break;
      case 'maintenance_sent': A.fx('maintenance', {other}); break;
      case 'released': A.fx('released', {other}); break;
      case 'regenerate': case 'reset': A.fx('regenerate', {other}); break;
    }
  });
}
/** Takes every history entry newer than the last one shown (oldest first). The first call only seeds the stack. */
function processNews(v, seed = false) {
  const hist = (v.history || []).filter(h => h.id);
  if (S.newsId === null || seed) {
    S.newsId = hist.length ? Math.max(...hist.map(h => h.id)) : 0;
    S.stack = hist.slice(0, v.game.capacity).reverse().map(newsLine);
    renderNews(); return;
  }
  const fresh = hist.filter(h => h.id > S.newsId).sort((a, b) => a.id - b.id);
  if (!fresh.length) return;
  S.newsId = Math.max(S.newsId, ...fresh.map(h => h.id));
  fresh.forEach(h => S.newsQueue.push(h));
  pumpNews();
}
async function pumpNews() {
  if (S.newsBusy) return;
  S.newsBusy = true;
  while (S.newsQueue.length) {
    const h = S.newsQueue.shift();
    S.stack.push(Object.assign(newsLine(h), {fresh: true}));
    while (S.stack.length > (S.view?.game.capacity || 4)) S.stack.shift();
    renderNews();
    A.fx('news', {other: !mine(h.seat)}); newsSounds(h);
    await wait(NEWS_GAP_MS);
  }
  S.newsBusy = false;
}
function renderNews() {
  const root = $('news'); if (!root || !S.view) return;
  const n = S.stack.length, held = S.view.viewer.my_turn && n > 0 && S.view.viewer.seats.length > 0;
  root.innerHTML = (held ? '<div class="nhead">SINCE YOUR LAST TURN</div>' : '') +
    S.stack.map((l, i) => `<div class="nline${l.fresh ? ' fresh' : ''}" style="opacity:${(0.3 + 0.7 * (i + 1) / n).toFixed(2)}">${l.html}</div>`).join('');
  S.stack.forEach(l => { l.fresh = false; });
  root.classList.toggle('hide', SC.open);
}
function clearNewsForRoll() { S.stack = []; S.newsQueue = []; renderNews(); }

// ---------------- landing scenes ----------------
function sceneStage(q) { const s = sq(q); return s.owner ? s.level : 'vacant'; }
function openScene(q, o = {}) {
  const s = sq(q); if (!s || !S.catalogue) return;
  SC.open = true; SC.square = q; SC.mode = o.mode || 'view'; SC.preview = o.preview || null; SC.seat = o.seat ?? null;
  const lot = lotOf(q);
  if (lot && !s.owner) SC.choice = (o.keepChoice && SC.choice && lot.businesses.some(b => b.id === SC.choice)) ? SC.choice : lot.businesses[0].id;
  drawScene(o.fromStage);
  $('scene').hidden = false; requestAnimationFrame(() => $('scene').classList.add('in'));
  renderNews(); renderTokens();
  const sp = s.special, lv = s.owner ? s.level : -1;
  A.ambient(lot ? {base: lv >= 1 ? s.district : 'vacant', layer: lv >= 1 ? s.business?.layer : null} : sp ? {base: null, layer: sp.ambient} : null);
}
function drawScene(fromStage) {
  const q = SC.square, s = sq(q), lot = lotOf(q), box = $('scene-svg');
  if (lot) {
    const ow = s.owner ? seat(s.owner) : null;
    box.innerHTML = SCN.lot(lot, s, {owner: ow ? {seat: ow.seat, name: ow.player || `Seat ${ow.seat}`, color: ow.color} : null,
      business: s.business?.id, level: s.level, choice: SC.choice});
    const svg = box.querySelector('svg'), stage = sceneStage(q);
    if (fromStage !== undefined && fromStage !== stage) { SCN.setStage(svg, fromStage, false); setTimeout(() => SCN.setStage(svg, stage, true), 60); }
    else SCN.setStage(svg, stage, false);
    SC.stage = stage;
  } else if (s.special) {
    box.innerHTML = SCN.special(s.special, s); SC.stage = null;
  } else { box.innerHTML = ''; }
  renderScenePanel();
}
function closeScene() {
  if (!SC.open) return;
  SC.open = false; SC.mode = 'view'; SC.preview = null;
  $('scene').classList.remove('in'); setTimeout(() => { if (!SC.open) $('scene').hidden = true; }, 350);
  A.ambient(null);
  renderNews(); if (S.view) renderTokens();
}
function renderScenePanel() {
  const q = SC.square, s = sq(q), lot = lotOf(q), info = $('scene-info'), opts = $('scene-opts'), steps = $('scene-steps');
  const kv = (k, v, style = '') => `<div><div class="k">${esc(k)}</div><div class="v"${style}>${v}</div></div>`;
  const lander = SC.seat ? seat(SC.seat) : null;
  let html = '', btns = '';
  if (s.kind === 'asset') {
    const ow = s.owner ? seat(s.owner) : null;
    html += kv('Owner', ow ? `${pc(ow, 14)} ${esc(`Seat ${ow.seat} · ${who(ow)}`)}` : 'For sale · the House', ow ? ` style="color:${ow.color}"` : '');
    if (lander) html += kv(`Cash · Seat ${lander.seat}`, esc(lander.balance_ttari));
    html += kv('Development', s.owner ? ['Level 0 · silhouette', 'Level 1 · 1 house', 'Level 2 · 2 houses', 'Level 3 · hotel'][s.level] : 'vacant');
    html += kv('Rent', esc(s.rent_ttari) + (s.owner ? '' : ' once owned'));
    html += kv('Value', esc(s.value_ttari));
    if (lot) html += kv(s.owner ? 'Business' : 'Blueprint', esc((s.owner ? s.business : lot.businesses.find(b => b.id === SC.choice))?.name || '—'));
    const p = SC.preview;
    if (SC.mode === 'buy' && p) {
      const b = p.buy;
      btns = `<button class="act pri" id="sc-buy" ${b.affordable && !S.busy ? '' : 'disabled'}>Buy · ${esc(b.price_ttari)}</button><button class="act" id="sc-pass" ${S.busy ? 'disabled' : ''}>Pass</button>`;
    } else if (SC.mode === 'build' && p) {
      const b = p.build;
      btns = `<button class="act pri" id="sc-build" ${b.affordable && !S.busy ? '' : 'disabled'}>Build ${esc(b.label)} · ${esc(b.cost_ttari)}</button><button class="act" id="sc-skip" ${S.busy ? 'disabled' : ''}>Skip</button>`;
    }
  } else {
    html += kv('Square', esc(`${s.name} · ${q}`));
    if (s.effect) html += kv('Effect', esc(s.effect));
    if (lander) html += kv(`Cash · Seat ${lander.seat}`, esc(lander.balance_ttari));
  }
  info.innerHTML = html + '<span class="sp"></span>' + btns + '<button class="act" id="sc-close">Close</button>';
  opts.innerHTML = (lot && SC.mode === 'buy' && !s.owner) ? lot.businesses.map(b =>
    `<button class="opt ${b.id === SC.choice ? 'on' : ''}" data-biz="${esc(b.id)}">${esc(b.name)}<small>${esc(b.theme)}</small></button>`).join('') : '';
  opts.querySelectorAll('[data-biz]').forEach(b => b.onclick = () => { SC.choice = b.dataset.biz; SCN.choose($('scene-svg').querySelector('svg'), SC.choice); renderScenePanel(); });
  const ST = ['Vacant', 'Bought · silhouette', '1 house · opened', '2 houses · extended', 'Hotel · flagship'], cur = s.owner ? s.level + 1 : 0;
  steps.innerHTML = lot ? ST.map((t, i) => `<span class="st ${i === cur ? 'on' : i < cur ? 'done' : ''}">${t}</span>`).join('') : '';
  $('sc-close').onclick = closeScene;
  $('sc-buy') && ($('sc-buy').onclick = () => settle({buy: true, business: SC.choice}));
  $('sc-pass') && ($('sc-pass').onclick = () => { SC.mode = 'view'; renderScenePanel(); settle({buy: false}); });
  $('sc-build') && ($('sc-build').onclick = () => settle({buy: false, build: true}));
  $('sc-skip') && ($('sc-skip').onclick = () => { SC.mode = 'view'; renderScenePanel(); settle({buy: false}); });
}
/** After a state change: redraw an open scene, animating a newly built level. */
function refreshScene() {
  if (!SC.open || !S.view) return;
  const before = SC.stage, now = sceneStage(SC.square), lot = lotOf(SC.square);
  if (lot && before !== now) {
    drawScene(typeof before === 'number' && typeof now === 'number' && now > before ? before : undefined);
    const s = sq(SC.square);
    A.ambient({base: s.owner && s.level >= 1 ? s.district : 'vacant', layer: s.owner && s.level >= 1 ? s.business?.layer : null});
  } else renderScenePanel();
}
document.addEventListener('keydown', e => { if (e.key === 'Escape' && SC.open && !window.OGEIntro.isActive()) closeScene(); });

// ---------------- sidebar ----------------
function renderSeats() {
  const v = S.view, root = $('seats'); root.replaceChildren();
  $('seats-badge').textContent = `${v.game.capacity} seats · ${v.viewer.seats.length ? 'you: ' + v.viewer.seats.join(', ') : 'watching'}`;
  v.seats.forEach(s => {
    const d = document.createElement('details'); d.className = 'seat' + (s.active ? ' active' : '') + (s.mine ? ' mineseat' : ''); if (s.active || s.held || s.mine) d.open = true;
    const kind = s.kind === 'ai' ? '<span class="pill ai">AI</span> ' : s.kind === 'open' ? '<span class="pill open">OPEN</span> ' : (s.mine ? '<span class="pill you">YOU</span> ' : '<span class="pill human">PLAYER</span> ');
    const tag = kind + (s.active ? '<span class="pill act">ACTIVE</span> ' : '') + (s.held ? '<span class="pill held">MAINTENANCE</span>' : '') + (s.out ? '<span class="pill out">OUT</span>' : '');
    d.innerHTML = `<summary><div class="ic">${tokenHtml(s, 28)}</div><div class="nm2"><span style="color:${esc(s.color)}">Seat ${s.seat}</span> ${tag}<small>${esc(s.piece)} · ${esc(who(s))} · ${esc(s.square_name)} · ${s.assets.length} assets</small></div><div class="bal">${s.balance_ttari}<small>tTARI</small></div></summary>`;
    const p = document.createElement('div'); p.className = 'port';
    s.assets.forEach(q => { const a = sq(q), r = document.createElement('div');
      r.innerHTML = `<span>${esc(a.business ? a.business.name : a.name)} · Sq ${q}${a.level ? ' · ' + ['', '1 house', '2 houses', 'hotel'][a.level] : ''}</span><b>${a.value_ttari}</b>`; p.append(r); });
    const rows = [['Property value', s.property_value_ttari], ['Net worth (cash + buyback)', s.net_worth_ttari],
      ['Rounds this stint', `${s.stint_rounds} (since round ${s.stint_since}${s.resets ? ` · ${s.resets} reset${s.resets > 1 ? 's' : ''}` : ''})`]];
    rows.forEach(([k, x]) => { const r = document.createElement('div'); r.innerHTML = `<span>${esc(k)}</span><b>${esc(x)}</b>`; p.append(r); });
    if (s.held) { const h = document.createElement('div'); h.innerHTML = `<span>Failed attempts ${s.held_attempts}/3</span><b class="warn">owes ${s.held_owed_ttari}</b>`; p.append(h); }
    const acc = document.createElement('div'); acc.innerHTML = `<span>Account</span><b title="${esc(s.account)}">${esc(s.account_name)}</b>`; p.append(acc);
    if (s.mine) {
      const r = document.createElement('div'); r.className = 'row';
      const b = document.createElement('button'); b.textContent = `Reset seat ${s.seat} to GO with ${v.economics.seat_start_ttari} tTARI`;
      b.disabled = S.busy || S.flow || !v.live_enabled;
      b.onclick = () => { if (confirm(`Reset seat ${s.seat}? Its assets go back to the House, it returns to GO, and its cash becomes ${v.economics.seat_start_ttari} tTARI.`)) action('/reset', {seat: s.seat}, 'Reset seat'); };
      r.append(b); p.append(r);
    }
    d.append(p); root.append(d);
  });
}
function kvRows(rows) { return '<div class="kv">' + rows.map(([k, v]) => `<span>${esc(k)}</span><b>${esc(v)}</b>`).join('') + '</div>'; }
function renderSettle() {
  const v = S.view, root = $('settle'), a = seat(v.active);
  $('settle-badge').textContent = `Seat ${a.seat}`;
  if (!v.roll || v.roll.seat !== v.active || !v.viewer.my_turn) {
    const ev = v.event;
    const waiting = v.viewer.my_turn ? `Your turn: Seat ${a.seat} · ${a.piece}. Roll the dice on the board.` :
      `Waiting for ${label(a)}${a.held ? ' · held in Maintenance' : ''}.${a.kind === 'ai' ? ' The AI moves on its own.' : ''}`;
    root.innerHTML = `<div class="box">${esc(waiting)}</div>` +
      (v.driver_error ? `<div class="box err">AI paused: ${esc(v.driver_error)}</div>` : '') +
      (ev.kind ? `<div class="box">Last event: ${esc(ev.label)} · Seat ${ev.seat} · Square ${ev.square}${ev.kind <= 3 ? ` · ${ev.kind === 3 || !ev.positive ? '−' : '+'}${ev.amount_ttari} tTARI` : ''}</div>` : '');
    return;
  }
  const p = v.roll.preview;
  if (p.error) { root.innerHTML = `<div class="box err">${esc(p.error)}</div>`; return; }
  const st = p.story, rows = [['Dice', `${v.roll.die1} + ${v.roll.die2}${st.doubles ? ' · doubles' : ''}`]];
  if (st.moved) rows.push(['Move', `Square ${st.from} → ${st.sent_to_maintenance ? '30 → 10 (Maintenance)' : st.to + ' · ' + sq(st.to).name}`]);
  if (st.go_reward) rows.push(['GO reward', '+' + fmt(st.go_reward)]);
  if (st.square_reward) rows.push(['Square reward', '+' + fmt(st.square_reward)]);
  if (st.card_kind) rows.push(['Card', `${st.card_positive ? '+' : '−'}${fmt(st.card_amount)}`]);
  const sqFee = st.penalty - (st.card_kind && !st.card_positive ? st.card_amount : 0);
  if (sqFee) rows.push([st.to === 7 ? 'Penalty' : 'Fee', '−' + fmt(sqFee)]);
  if (st.maintenance_fee) rows.push(['Shop fee + storage', '−' + fmt(st.maintenance_fee)]);
  if (st.rent) rows.push([`Rent to Seat ${st.rent_owner}`, '−' + fmt(st.rent) + (st.rent_fee ? ` (rent fee ${fmt(st.rent_fee)})` : '')]);
  rows.push(['Seat funding', fmt(p.seat_funding)], ['Bankroll', `${fmt(p.seat_balance)} → ${fmt(p.balance_after)}`]);
  rows.push(['House reserve', `${fmt(p.house_required)} required · ${fmt(p.house_available)} available`]);
  let note = '';
  if (p.decision_reason === 'purchase') note = `<div class="box">Decision required: buy ${esc(p.buy.name)} for ${p.buy.price_ttari} tTARI or pass (in the landing scene).</div>`;
  else if (p.decision_reason === 'build') note = `<div class="box">Decision: build a ${esc(p.build.label)} on ${esc(p.build.name)} for ${p.build.cost_ttari} tTARI, or skip (in the landing scene).</div>`;
  else if (p.decision_reason === 'recovery_sale') note = `<div class="box err">Decision required: a recovery sale is needed before the turn can settle.</div>`;
  else if (p.decision_reason === 'regenerate') note = `<div class="box err">This seat cannot pay and owns nothing to sell: it regenerates (back to GO with ${v.economics.seat_start_ttari} tTARI).</div>`;
  else if (p.decision_reason === 'house_reserve') note = `<div class="box err">The Game House cannot cover this turn's payout and fee reserve; the backend tops it up from the main House when it can.</div>`;
  else note = `<div class="box good">No decision needed · settles automatically.</div>`;
  root.innerHTML = `<div class="box">${esc(st.text)}</div>` + kvRows(rows) + note +
    `<div class="row"><button class="primary" id="settle-now" ${p.decision_required || S.busy || !v.live_enabled ? 'disabled' : ''}>Settle turn</button></div>`;
  $('settle-now')?.addEventListener('click', () => settle({buy: false}));
}
function renderFeed() {
  const v = S.view, root = $('feed'); root.replaceChildren(); let actions = 0;
  const a = seat(v.active), myTurn = v.viewer.my_turn;
  v.seats.filter(s => s.held).forEach(s => {
    const can = v.live_enabled && myTurn && v.active !== s.seat && !S.busy && !S.flow && !(v.roll && v.roll.seat === v.active);
    if (v.active !== s.seat && myTurn) actions++;
    const n = document.createElement('div'); n.className = 'note maint';
    n.innerHTML = `<div class="ni">🔧</div><div><b>Seat ${s.seat} is in Maintenance</b> — owes ${s.held_owed_ttari} tTARI (shop fee ${v.params.shop_fee_ttari} + storage ${v.params.storage_fee_ttari} per failed roll). Failed attempts ${s.held_attempts}/3. Balance ${s.balance_ttari}.` +
      (v.active !== s.seat && myTurn ? `<br><button class="gold" ${can ? '' : 'disabled'}>Seat ${a.seat} · ${esc(a.piece)} pays the shop fee for Seat ${s.seat}</button>` : '') + '</div>';
    n.querySelector('button')?.addEventListener('click', () => action(`/pay-shop-fee`, {payer: v.active, held_seat: s.seat}, 'Shop fee'));
    root.append(n);
  });
  if (v.offer) { if (v.viewer.offer_for_me) actions++; const n = document.createElement('div'); n.className = 'note';
    n.innerHTML = `<div class="ni">🤝</div><div>Seat ${v.offer.seller} offers <b>${esc(v.offer.name)}</b> to Seat ${v.offer.buyer} for ${v.offer.price_ttari} tTARI. Awaiting Seat ${v.offer.buyer}.</div>`; root.append(n); }
  v.history.forEach(h => { const n = document.createElement('div'); n.className = 'note';
    const op = h.operation || '';
    const icon = op.includes('build') ? '🏠' : op.includes('sale') ? '🤝' : op.includes('sell') ? '🏷️' : op.includes('reset') ? '↺' : op.includes('top_up') ? '🏦' : op.includes('maintenance') || op.includes('shop') ? '🔧' : '🎲';
    const by = h.actor === 'ai' ? ' · AI' : h.actor === 'auto-roll' ? ' · auto-roll' : h.actor === 'system' ? ' · system' : '';
    n.innerHTML = `<div class="ni">${icon}</div><div>${esc(h.text)}<time>round ${h.round} · turn ${h.turn}${by} · ${esc(h.status)}</time></div>`; root.append(n); });
  if (!root.children.length) root.innerHTML = '<div class="note"><div class="ni">◆</div><div>No events recorded yet.</div></div>';
  $('feed-badge').textContent = actions ? `${actions} action${actions > 1 ? 's' : ''}` : ''; $('feed-badge').className = 'badge' + (actions ? ' alert' : '');
}
function fillSelect(sel, items, keep = true) {
  const prev = sel.value; sel.replaceChildren();
  items.forEach(([val, lbl]) => { const o = document.createElement('option'); o.value = val; o.textContent = lbl; sel.append(o); });
  if (keep && items.some(([v]) => String(v) === prev)) sel.value = prev;
}
function renderTrade() {
  const v = S.view, a = seat(v.active), myTurn = v.viewer.my_turn, myAssets = a.assets.map(q => sq(q));
  const blocked = S.busy || S.flow || !v.live_enabled || !myTurn || (v.roll && v.roll.seat === v.active);
  const pend = $('offer-pending');
  if (v.offer) {
    const b = seat(v.offer.buyer), forMe = v.viewer.offer_for_me;
    pend.innerHTML = `<div class="box">Pending: Seat ${v.offer.seller} → Seat ${v.offer.buyer} · ${esc(b.piece)} · ${esc(v.offer.name)} · ${v.offer.price_ttari} tTARI · Seat ${b.seat} balance ${b.balance_ttari}</div>` +
      (forMe ? `<div class="row"><button class="primary" id="offer-accept" ${S.busy || !v.live_enabled ? 'disabled' : ''}>Seat ${b.seat} accepts</button><button id="offer-decline" ${S.busy || !v.live_enabled ? 'disabled' : ''}>Seat ${b.seat} declines</button></div>` :
        `<div class="box">${b.kind === 'ai' ? 'The AI declines offers.' : `Waiting for ${esc(who(b))} to answer.`}</div>`);
    if (forMe) {
      $('offer-accept').onclick = () => action('/offer/accept', {buyer: v.offer.buyer}, 'Accept offer');
      $('offer-decline').onclick = () => action('/offer/decline', {buyer: v.offer.buyer}, 'Decline offer');
    }
    $('offer-badge').textContent = forMe ? 'for you' : 'pending'; if (forMe) $('offer-card').open = true;
  } else { pend.innerHTML = ''; $('offer-badge').textContent = ''; }
  fillSelect($('offer-asset'), myAssets.map(s => [s.square, `${s.business ? s.business.name : s.name} · Square ${s.square} · value ${s.value_ttari}`]));
  fillSelect($('offer-buyer'), v.seats.filter(s => s.seat !== a.seat && !s.out).map(s => [s.seat, `${label(s)} · ${s.balance_ttari}`]));
  $('offer-send').disabled = blocked || !!v.offer || !myAssets.length;
  $('offer-send').textContent = myTurn ? `Seat ${a.seat} sends offer` : 'Offers are made on your turn';
  // Construction: with build on landing, only the owned asset you have just landed on, before the next move.
  const lnd = v.landed, landing = v.economics.build_on_landing;
  let dev;
  if (landing) {
    const q = lnd.square, s = lnd.seat ? seat(lnd.seat) : null;
    dev = (s && s.mine && q !== null && sq(q).owner === s.seat && sq(q).level < 3) ? [sq(q)] : [];
  } else dev = myTurn ? myAssets.filter(s => s.level < 3) : [];
  fillSelect($('build-asset'), dev.map(s => [s.square, `${s.business ? s.business.name : s.name} · Square ${s.square} · ${s.level}/3`]));
  const bs = dev.find(s => String(s.square) === $('build-asset').value);
  $('build-info').textContent = bs ? `Next construction: ${bs.next_label} · ${bs.next_cost_ttari} tTARI · rent after build ${bs.rent_after_build} tTARI` :
    (landing ? 'Build-on-landing rule: you build one level on an asset you own when you land on it (offered during the turn), before the next seat moves.' : 'No property to develop on this turn.');
  const builder = landing ? lnd.seat : a.seat;
  $('build-go').disabled = S.busy || S.flow || !v.live_enabled || !!v.offer || !bs; $('build-go').textContent = bs ? `Seat ${builder} builds` : 'Build improvement';
  $('build-go').dataset.seat = builder;
  fillSelect($('sell-asset'), (myTurn ? myAssets : []).map(s => [s.square, `${s.business ? s.business.name : s.name} · Square ${s.square} · buyback ${s.buyback_ttari}`]));
  const ss = myAssets.find(s => String(s.square) === $('sell-asset').value);
  $('sell-info').textContent = ss && myTurn ? `House pays ${ss.buyback_ttari} tTARI (${v.params.buyback_pct}% of value ${ss.value_ttari}). The asset returns to the House unowned.` : 'Sales to the House are made on your turn.';
  $('sell-go').disabled = blocked || !!v.offer || !ss; $('sell-go').textContent = myTurn ? `Seat ${a.seat} sells` : 'Sell to the House';
}
function renderLive() {
  const v = S.view, e = v.economics, rows = [['Game', '#' + v.game.id], ['Type', v.game.type_name], ['Status', v.game.status], ['Phase', v.game.phase.replace('_', ' ')],
    ['Round', v.round], ['Active seat', v.active], ['Committed turns', v.turn_count], ['Doubles streak', v.doubles_streak],
    ['Last dice', v.last_dice.join(' + ')], ['Treasury', v.game.treasury_mode], ['Community', v.pools.community_ttari], ['Chance', v.pools.chance_ttari],
    ['GO reward now', e.go_reward_now_ttari + (e.go_scaled ? ` (scaled · base ${e.go_base_ttari} · floor ${e.go_floor_ttari})` : '')],
    ['Rent fee', `${e.rent_fee_bps / 100}%`], ['Fee / penalty squares', `${e.fee_ttari} / ${e.penalty_ttari}`],
    ['Build rule', e.build_on_landing ? 'on landing only' : 'any owned asset on your turn'], ['Seat start', e.seat_start_ttari],
    ['Auto-roll', v.decision.enforced ? `${v.decision.auto_roll_secs} s` : 'off'], ['Substate version', v.substate_version]];
  $('live-state').innerHTML = rows.map(([k, x]) => `<dt>${esc(k)}</dt><dd>${esc(x)}</dd>`).join('');
}
function renderChips() {
  const v = S.view;
  $('seat-count').textContent = v.game.seats_added; $('seat-cap').textContent = v.game.capacity;
  $('brand-sub').textContent = `${v.game.type_name} · ${v.network}`;
  $('chain-chip').textContent = 'LIVE CHAIN READ'; $('chain-chip').className = 'chip ok';
  $('live-chip').textContent = v.live_enabled ? 'TRANSACTIONS ON' : 'TRANSACTIONS OFF';
  $('live-chip').className = 'chip ' + (v.live_enabled ? 'ok' : 'warn');
  $('live-chip').title = v.live_enabled ? '' : `Set ${S.info?.live_env || 'OOTLE_ENABLE_LIVE_TESTS'}=1 on the server to submit transactions`;
  const me = S.me;
  $('me-chip').textContent = me.game ? `You · game #${me.game} · seat ${me.seats.join(', ')}` : 'Watching';
  $('me-chip').className = 'chip ' + (me.game ? 'ok' : '');
  $('release').hidden = !me.game;
  $('play').hidden = !!me.game;
}
/** Game selector: this game's entry from the current state (every poll); all games from /api/games. */
function gameLabel(g) {
  if (!g.readable) return `Game ${g.id} · unreadable`;
  const ai = Math.max(0, g.capacity - g.humans - g.open_seats);
  return `Game ${g.id} · ${g.capacity} seats: ${g.humans} player${g.humans === 1 ? '' : 's'}, ${ai} AI, ${g.open_seats} open · round ${g.round}`;
}
function renderGameSelect() {
  const sel = $('game-select'), v = S.view;
  if (v) {
    const row = {id: v.game.id, readable: true, round: v.round, capacity: v.game.capacity,
      humans: v.seats.filter(s => s.kind === 'human').length, open_seats: v.seats.filter(s => s.kind === 'open').length};
    const i = S.games.findIndex(g => g.id === row.id);
    if (i >= 0) S.games[i] = Object.assign({}, S.games[i], row); else S.games.push(row);
  }
  const items = S.games.map(g => [g.id, gameLabel(g)]), key = JSON.stringify(items);
  if (sel.dataset.key !== key) { fillSelect(sel, items, false); sel.dataset.key = key; } // an open list is not rebuilt needlessly
  sel.value = String(S.gameId);
}
/** Selector: go to another game's board in place, with no lobby and no intro. */
async function switchGame(id) {
  if (id === S.gameId) return;
  if (S.busy || S.flow || window.OGEIntro.isActive()) { renderGameSelect(); notify('Finish the current move before switching games.', 'bad'); return; }
  closeScene(); hidePop(); clearTimeout(S.backTimer);
  S.gameId = id; rememberGame(id);
  history.replaceState(null, '', '?game=' + id);
  S.view = null; S.newsId = null; S.stack = []; S.newsQueue = []; S.override = {}; S.wasMyTurn = null; S.receiptSel = null;
  if (await refresh()) {
    processNews(S.view, true); S.wasMyTurn = S.view.viewer.my_turn;
    render(); showLayer('idle');
    notify(S.me.game === id ? `Game ${id} · you hold seat ${S.me.seats.join(', ')}.` : `Game ${id} · watching.`, 'ok');
    if ($('view-developer').classList.contains('on')) loadDev();
  }
}
const LAST_KEY = 'oge_last_game';
function rememberGame(id) { try { localStorage.setItem(LAST_KEY, String(id)); } catch (e) { /* not remembered */ } }
function lastGame() { try { return Number(localStorage.getItem(LAST_KEY)); } catch (e) { return 0; } }
async function loadGames() { try { S.games = await api('/api/games'); renderGameSelect(); } catch (e) { /* keep the last list */ } }
function render() {
  if (!S.view) return;
  if (S.built !== S.view.game.capacity || !S.tiles.length) buildBoard();
  renderChips(); renderBoardState(); renderStage(); renderSeats(); renderSettle(); renderFeed(); renderTrade(); renderLive(); renderGameSelect(); renderNews();
  if (SC.open) refreshScene();
}

// ---------------- loading ----------------
/** Every new state goes through here: news, the your-turn chime. */
function takeView(nv) {
  S.view = nv;
  processNews(nv);
  const my = nv.viewer.my_turn;
  if (S.wasMyTurn === false && my && !S.flow) A.fx('turn');
  S.wasMyTurn = my;
}
async function refresh(quiet = false) {
  try {
    const nv = await api(`/api/game/${S.gameId}/state`);
    const old = S.view;
    if (!window.OGEIntro.isActive() && old && !S.flow && old.turn_count !== nv.turn_count) { await animateRemote(old, nv); return true; }
    takeView(nv);
    if (!window.OGEIntro.isActive() && !S.animating) render();
    return true;
  }
  catch (e) { $('chain-chip').textContent = 'CHAIN READ FAILED'; $('chain-chip').className = 'chip bad'; if (!quiet) showError(e, 'State read'); return false; }
}
setInterval(() => { if (S.gameId && !S.busy && !S.flow && !S.animating && !window.OGEIntro.isActive() && !gateOpen() && document.visibilityState === 'visible') refresh(true); }, POLL_MS);

/** Another seat moved (AI, auto-roll, or another browser): walk its piece over the squares, then render.
    A human seat's landing opens its landing scene here too (view only). */
async function animateRemote(old, nv) {
  S.animating = true;
  const moved = nv.seats.filter(s => old.seats[s.seat - 1] && old.seats[s.seat - 1].position !== s.position && !s.mine);
  S.view = {...nv, seats: nv.seats.map(s => ({...s}))};
  if (moved.length && SC.open && SC.mode === 'view') closeScene();
  render();
  let humanLanding = null;
  for (const s of moved) {
    const from = old.seats[s.seat - 1].position, steps = (s.position - from + 40) % 40;
    if (steps === 0 || steps > 12 || s.position === 10 && from >= 18 && from <= 29) { continue; }
    S.override[s.seat] = from;
    for (let k = 0; k < steps; k++) { S.override[s.seat] = (S.override[s.seat] + 1) % 40; renderTokens(s.seat); A.fx('tick', {other: true}); await wait(140); }
    A.fx('land', {other: true});
    glow(s.position, s.color);
    delete S.override[s.seat];
    if (s.kind === 'human') humanLanding = s;
  }
  S.override = {}; S.animating = false;
  takeView(nv); render();
  const h = nv.history[0];
  if (h && h.text) notify(h.text);
  if (humanLanding) openScene(humanLanding.position, {mode: 'view', seat: humanLanding.seat});
}

async function loadMe() {
  try { S.me = await api('/api/me'); } catch (e) { S.me = {game: null, seats: []}; }
}

function playIntro() {
  if (!S.view) return;
  closeScene();
  if (S.built !== S.view.game.capacity || !S.tiles.length) buildBoard();
  renderChips(); renderSeats(); renderFeed(); renderTrade(); renderLive();
  window.OGEIntro.start({board: $('board'), stage: $('stage'), tiles: S.tiles, view: S.view,
    onDone: () => { render(); showLayer('idle'); if (!S.me.game) openSetup(); }});
}

// ---------------- lobby (every load and refresh) ----------------
const gateOpen = () => !$('gate').hidden;
/** Fills the lobby line once the game is read. The lobby itself is shown from the first paint. */
function fillLobby() {
  const g = S.games.find(x => x.id === S.gameId);
  const where = S.me.game === S.gameId ? `you: Seat ${S.me.seats.join(', ')}` : S.me.game ? `watching · you hold seat ${S.me.seats.join(', ')} in game ${S.me.game}` : 'watching';
  $('gate-line').textContent = g ? `${gameLabel(g)} · ${where}` : `Game ${S.gameId} · ${where}`;
}
let lobbyReady = false, lobbyChoice = null;
/** Enter: the lobby stays until the game state has arrived; then the intro starts once the audio runs
    (v0.4.0 started it too early and its music was dropped). */
function startFromGate(sound) {
  A.set('on', sound); renderSound();
  const audio = sound ? A.unlock() : Promise.resolve();
  if (!lobbyReady) { lobbyChoice = audio; $('gate-line').textContent = 'Connecting… the game starts as soon as it is read'; return; }
  enterGame(audio);
}
function enterGame(audio) {
  $('gate').hidden = true;
  audio.then(() => setTimeout(playIntro, 0));
}
$('gate-sound').onclick = e => { e.stopPropagation(); startFromGate(true); };
$('gate-silent').onclick = e => { e.stopPropagation(); startFromGate(false); };

// ---------------- sound settings ----------------
function renderSound() {
  const s = A.settings();
  $('snd').textContent = s.on ? 'Sound on' : 'Sound off'; $('snd').classList.toggle('off', !s.on);
  $('snd-vol').value = Math.round(s.volume * 100); $('snd-others').value = Math.round(s.others * 100);
  $('snd-amb').checked = s.ambient;
}
$('snd').onclick = () => { A.unlock(); A.set('on', !A.settings().on); renderSound(); };
$('snd-vol').oninput = e => A.set('volume', e.target.value / 100);
$('snd-others').oninput = e => A.set('others', e.target.value / 100);
$('snd-amb').onchange = e => A.set('ambient', e.target.checked);
$('snd-more').onclick = e => { e.stopPropagation(); $('sndpanel').hidden = !$('sndpanel').hidden; };
$('sndpanel').addEventListener('click', e => e.stopPropagation());

// ---------------- setup screen ----------------
async function openSetup() {
  try { S.games = await api('/api/games'); renderGameSelect(); } catch (e) { showError(e, 'Games'); return; }
  $('setup').classList.add('in');
  renderSetup();
}
function closeSetup() { $('setup').classList.remove('in'); }
function pieceSelect(id, pick) {
  return `<select id="${id}">${Array.from({length: P.count}, (_, i) => `<option value="${i + 1}" ${i + 1 === pick ? 'selected' : ''}>${esc(P.name(i + 1))}</option>`).join('')}</select>`;
}
function renderSetup() {
  const games = S.games.filter(g => g.readable);
  const free = games.filter(g => g.humans === 0);
  const open = games.filter(g => g.humans > 0 && g.open_seats > 0);
  const live = S.info?.live_transactions_enabled;
  const start = S.info?.seat_start_ttari || '20.000000';
  const cap = games[0]?.capacity || 4;
  const colours = S.info?.seat_colours || [];
  const name = (() => { try { return localStorage.getItem('oge_name') || ''; } catch (e) { return ''; } })();
  $('setup-body').innerHTML = `
    <p class="lead">Free-play demonstration of a blockchain game engine on ${esc(S.info?.network || 'Ootle')}. Every seat starts with ${esc(start)} tTARI from the House;
    there is no buy-in and no cash-out. Every move is a real transaction, signed by the backend for your seat.
    AI seats move on their own${S.info?.auto_roll_secs ? `, and an idle seat is rolled automatically after ${Math.round(S.info.auto_roll_secs / 60)} minutes` : ''}.
    Games never end: the leaderboard ranks net worth, and a seat's ${S.info?.milestone_rounds || 100}th round is recorded as a milestone.</p>
    ${live ? '' : '<div class="box err">Transactions are off on this server: you can watch, but not play.</div>'}
    <div class="field">Your name<input id="su-name" maxlength="24" value="${esc(name)}" placeholder="Player name"></div>
    <div class="setupgrid">
      <div class="panel"><h3>Start or take over a game</h3>
        ${free.length ? `
        <div class="field">Game<select id="su-game">${free.map(g => `<option value="${g.id}">Game ${g.id} · ${g.fresh ? 'new board' : `round ${g.round} · take over where it was left`}</option>`).join('')}</select></div>
        <div class="field">Seats you control<select id="su-seats">${[1, 2, 3, 4].filter(n => n <= cap).map(n => `<option value="${n}">${n}</option>`).join('')}</select></div>
        <div class="field">AI opponents<select id="su-ai"></select></div>
        <div id="su-veh"></div>
        <div class="box" id="su-note"></div>
        <div class="row"><button class="primary" id="su-start" ${live ? '' : 'disabled'}>Start new game</button></div>` :
        '<div class="box">All games have players. Join an open game, or watch.</div>'}
      </div>
      <div class="panel"><h3>Join an open game</h3>
        ${open.length ? open.map(g => `<div class="gamerow"><div><b>Game ${g.id}</b> · round ${g.round}<br><small>${g.seats.map(s => `${P.svg(s.piece_id, s.color, 14)} ${s.kind === 'human' ? esc(s.name) : s.kind === 'ai' ? 'AI' : 'open'}`).join(' · ')}</small></div>
          <button data-join="${g.id}" ${live ? '' : 'disabled'}>Join</button></div>`).join('') : '<div class="box">No game has an open seat next to a player right now.</div>'}
        <div class="field">Your piece when joining${pieceSelect('su-joinveh', 6)}</div>
      </div>
    </div>
    <div class="row"><button class="ghost" id="su-watch">Just watch</button></div>`;
  const seatsSel = $('su-seats'), aiSel = $('su-ai');
  const pv = i => { const sel = $('su-v' + i); const c = colours[i] || '#3fa7ff'; $('su-pv' + i).innerHTML = P.token(Number(sel.value), c, 30); };
  const update = () => {
    if (!seatsSel) return;
    const n = Number(seatsSel.value), maxAi = cap - n, prev = Number(aiSel.value || maxAi);
    aiSel.innerHTML = Array.from({length: maxAi + 1}, (_, i) => `<option value="${i}" ${i === Math.min(prev, maxAi) ? 'selected' : ''}>${i}</option>`).join('');
    $('su-veh').innerHTML = Array.from({length: n}, (_, i) => `<div class="field">Piece for seat ${i + 1}<div class="pickrow"><span id="su-pv${i}"></span>${pieceSelect('su-v' + i, [1, 2, 3, 4][i] || i + 1)}</div></div>`).join('');
    for (let i = 0; i < n; i++) { pv(i); $('su-v' + i).onchange = () => pv(i); }
    const ai = Number(aiSel.value), left = cap - n - ai;
    $('su-note').textContent = `${n} seat${n > 1 ? 's' : ''} for you, ${ai} AI${left ? `, ${left} left open for others to join` : ''}. ` +
      (n > 1 ? 'Two or more seats from one browser are played on this screen in turn. ' : '') +
      'A taken-over game keeps its board; use Reset seat on a seat card to start that seat over. Each seat has its own colour; the piece is your choice.';
  };
  if (seatsSel) { seatsSel.onchange = update; aiSel.onchange = update; update(); }
  const startLabel = () => {
    const g = free.find(x => String(x.id) === $('su-game').value);
    $('su-start').textContent = !g || g.fresh ? 'Start new game' : `Take over game ${g.id}`;
  };
  if ($('su-game')) { $('su-game').addEventListener('change', startLabel); startLabel(); }
  $('su-start') && ($('su-start').onclick = () => {
    const g = Number($('su-game').value), n = Number(seatsSel.value), ai = Number(aiSel.value);
    const seats = Array.from({length: n}, (_, i) => i + 1), vehicles = seats.map((s, i) => [s, Number($('su-v' + i).value)]);
    claim({game: g, seats, ai, vehicles});
  });
  document.querySelectorAll('[data-join]').forEach(b => b.onclick = () => {
    const g = S.games.find(x => x.id === Number(b.dataset.join)), s = g.seats.find(x => x.kind === 'open');
    claim({game: g.id, seats: [s.seat], ai: 0, vehicles: [[s.seat, Number($('su-joinveh').value)]]});
  });
  $('su-watch').onclick = closeSetup;
}
async function claim(body) {
  const name = $('su-name').value.trim();
  if (!name) { $('su-name').focus(); notify('Enter your name first.', 'bad'); return; }
  try { localStorage.setItem('oge_name', name); } catch (e) { /* not stored */ }
  try {
    const r = await api('/api/claim', 'POST', {...body, name});
    saveToken(r.token);
    await loadMe(); closeSetup();
    if (S.gameId !== r.game) { await switchGame(r.game); notify(`You hold seat ${r.seats.join(', ')} in game ${r.game}.`, 'ok'); return; }
    notify(`You hold seat ${r.seats.join(', ')} in game #${r.game}.`, 'ok');
    await refresh(true);
  } catch (e) { showError(e, 'Claim seat'); }
}
async function release() {
  if (!confirm('Release your seats? They keep their cash, assets and position for the next player.')) return;
  try { await api('/api/release', 'POST', {}); await loadMe(); notify('Seats released.', 'ok'); await refresh(true); openSetup(); }
  catch (e) { showError(e, 'Release'); }
}

// ---------------- leaderboard ----------------
async function loadBoard() {
  try { S.board = await api('/api/leaderboard'); } catch (e) { return; }
  const b = S.board, live = b.live.slice(0, 10), ms = b.milestones.slice(0, 10);
  $('lb-badge').textContent = `${b.live.length} seats`;
  $('leaderboard').innerHTML =
    '<table class="t"><tr><th>#</th><th>Player</th><th>Game</th><th class="n">Net worth</th><th class="n">Rounds</th></tr>' +
    live.map((x, i) => `<tr><td>${i + 1}</td><td>${P.svg(x.piece_id, x.color, 14)} ${esc(x.name)}${x.kind === 'ai' ? ' <span class="pill ai">AI</span>' : ''}</td><td>#${x.game}·${x.seat}</td><td class="n">${esc(x.net_worth_ttari ?? '—')}</td><td class="n">${x.stint_rounds}</td></tr>`).join('') +
    '</table>' + `<div class="box">Milestones · ${b.milestone_rounds} rounds in one stint</div>` +
    (ms.length ? '<table class="t"><tr><th>Player</th><th>Game</th><th class="n">Net worth</th><th>Reached</th></tr>' +
      ms.map(m => `<tr><td>${esc(m.name)}${m.kind === 'ai' ? ' <span class="pill ai">AI</span>' : ''}</td><td>#${m.game}·${m.seat}</td><td class="n">${esc(m.net_worth_ttari)}</td><td>${new Date(m.at * 1000).toLocaleString()}</td></tr>`).join('') + '</table>'
      : '<div class="box">None recorded yet.</div>');
}
setInterval(() => { if (document.visibilityState === 'visible') { loadBoard(); loadGames(); } }, BOARD_MS);

// ---------------- turn flow ----------------
async function startRoll(pay) {
  if (S.busy || S.flow) return;
  const v = S.view;
  if (v.roll && v.roll.seat === v.active) return runFlow(v, false);
  S.busy = true; render();
  try {
    const nv = await api(`/api/game/${S.gameId}/roll`, 'POST', {pay});
    clearNewsForRoll(); closeScene();
    S.view = nv; S.busy = false; await runFlow(nv, true);
  }
  catch (e) { S.busy = false; showError(e, 'Roll'); await refresh(true); }
}
async function animateDice(d1, d2) {
  const a = $('d1'), b = $('d2'); a.classList.add('spin'); b.classList.add('spin');
  A.fx('dice');
  for (let k = 0; k < 14; k++) { a.textContent = FACES[k % 6]; b.textContent = FACES[(k + 3) % 6]; await wait(70); }
  a.classList.remove('spin'); b.classList.remove('spin'); a.textContent = FACES[d1 - 1]; b.textContent = FACES[d2 - 1];
}
async function animateMove(st) {
  const n = st.seat;
  if (st.third_double || !st.moved) { if (st.third_double) { S.override[n] = 10; renderTokens(n); glow(10, seat(n).color); A.fx('maintenance'); } return; }
  const steps = (st.to - st.from + 40) % 40;
  S.override[n] = st.from;
  for (let k = 0; k < steps; k++) { S.override[n] = (S.override[n] + 1) % 40; renderTokens(n); A.fx('tick'); await wait(170); }
  A.fx('land');
  if (st.sent_to_maintenance) { await wait(450); S.override[n] = 10; renderTokens(n); }
  glow(S.override[n], seat(n).color);
}
async function runFlow(v, spin) {
  S.flow = true; clearTimeout(S.backTimer);
  const r = v.roll, p = r.preview, a = seat(r.seat);
  $('act-who').innerHTML = `${pc(a, 26)} Seat ${a.seat} · ${esc(who(a))}`;
  $('act-msg').classList.remove('in'); $('back').classList.remove('in'); $('decide').classList.remove('in'); $('settling').textContent = '';
  showLayer('act'); renderTokens();
  if (spin) await animateDice(r.die1, r.die2); else { $('d1').textContent = FACES[r.die1 - 1]; $('d2').textContent = FACES[r.die2 - 1]; }
  await wait(300);
  if (p.error) { $('act-msg').textContent = p.error; $('act-msg').classList.add('in'); return endFlow(false); }
  await animateMove(p.story);
  $('act-msg').textContent = p.story.text; $('act-msg').classList.add('in');
  renderSettle();
  const landed = p.story.moved && !p.story.sent_to_maintenance ? p.story.to : null;
  if (!v.live_enabled) {
    $('settling').textContent = 'Transactions are off on the server · the roll is stored for this turn';
    if (landed !== null) openScene(landed, {mode: 'view', seat: a.seat});
    return endFlow(false);
  }
  if (p.decision_reason === 'purchase') { openScene(landed, {mode: 'buy', preview: p, seat: a.seat}); return; }
  if (p.decision_reason === 'build') { openScene(landed, {mode: 'build', preview: p, seat: a.seat}); return; }
  if (landed !== null) openScene(landed, {mode: 'view', seat: a.seat});
  if (p.decision_reason === 'recovery_sale') return offerRecovery(p);
  if (p.decision_reason === 'regenerate') return offerRegenerate(p);
  return settle({buy: false});
}
function decide(html, buttons) {
  $('dq').innerHTML = html; const box = $('dbtn'); box.replaceChildren();
  buttons.forEach(([lbl, cls, fn, disabled]) => { const b = document.createElement('button'); b.textContent = lbl; if (cls) b.className = cls; b.disabled = !!disabled; b.onclick = () => { $('decide').classList.remove('in'); fn(); }; box.append(b); });
  $('decide').classList.add('in');
}
function offerRecovery(p) {
  const st = p.story, a = seat(st.seat), opts = p.recovery.filter(o => o.settles);
  const need = fmt(p.seat_funding - p.seat_balance);
  if (SC.open) closeScene();
  if (!opts.length) {
    decide(`Seat ${a.seat} needs ${fmt(p.seat_funding)} tTARI for this turn and has ${fmt(p.seat_balance)}. No single sale inside the turn covers the shortfall of ${need} tTARI.<br>Sell assets to the House one at a time (Sell to the House card), or regenerate the seat.`,
      [['Regenerate seat', 'pass', () => regenerate(st.seat)]]);
    return endFlow(false);
  }
  decide(`Seat ${a.seat} needs ${fmt(p.seat_funding)} tTARI for this turn and has ${fmt(p.seat_balance)} (short ${need}).<br>Choose an asset to sell to the House at the buyback value inside this turn:`,
    opts.map(o => [`Sell ${o.name} · ${o.buyback_ttari}`, 'buy', () => settle({buy: false, sell_square: o.square})]));
}
function offerRegenerate(p) {
  const st = p.story, a = seat(st.seat);
  if (SC.open) closeScene();
  decide(`Seat ${a.seat} needs ${fmt(p.seat_funding)} tTARI for this turn, has ${fmt(p.seat_balance)} and owns nothing to sell.<br>The seat regenerates: back to GO with ${S.view.economics.seat_start_ttari} tTARI, and the turn passes.`,
    [['Regenerate seat', 'buy', () => regenerate(st.seat)]]);
}
async function regenerate(seatN) {
  await action('/regenerate', {seat: seatN}, 'Regenerate');
  endFlow(true);
}
async function settle(body) {
  if (S.busy) return;
  S.busy = true; S.flow = true; renderSettle(); if (SC.open) renderScenePanel();
  $('settling').textContent = 'Settling on-chain · dry run, submit, confirm, reconcile…';
  const seatN = S.view.active;
  try {
    const r = await api(`/api/game/${S.gameId}/turn`, 'POST', body);
    const okv = r.committed && r.verified;
    $('settling').textContent = `${r.status || (r.committed ? 'COMMITTED' : 'NOT COMMITTED')} · fee ${r.fee ?? '—'} base units`;
    if (r.story) $('act-msg').textContent = r.story.text;
    notify(okv ? `Seat ${seatN}'s turn settled and reconciled on-chain · ${r.transaction_id}` : `Turn committed=${r.committed} verified=${r.verified} · ${r.verification || 'review the Developer tab'} · ${r.transaction_id}`, okv ? 'ok' : 'bad');
    S.busy = false; S.override = {}; SC.mode = 'view'; SC.preview = null;
    takeView(await api(`/api/game/${S.gameId}/state`));
    if (SC.open) refreshScene();
  } catch (e) {
    S.busy = false; S.override = {}; SC.mode = 'view'; SC.preview = null; $('settling').textContent = `${e.code}`;
    await refresh(true); showError(e, 'Turn settlement', true);
    if (SC.open) renderScenePanel();
  }
  endFlow(true);
}
function endFlow(auto) {
  $('back').classList.add('in');
  if (auto) S.backTimer = setTimeout(backToSquare, HOLD_MS);
}
function backToSquare() {
  clearTimeout(S.backTimer); $('decide').classList.remove('in');
  S.flow = false; S.override = {}; showLayer('idle'); render();
}

// ---------------- other actions ----------------
async function action(path, body, lbl) {
  if (S.busy) return;
  S.busy = true; render(); notify(`${lbl}: submitting…`);
  try {
    const r = await api(`/api/game/${S.gameId}${path}`, 'POST', body);
    const okv = r.committed && r.verified;
    notify(`${lbl}: ${r.story?.text || ''} · ${r.status || ''} · ${r.transaction_id}`, okv ? 'ok' : 'bad');
    S.busy = false; takeView(await api(`/api/game/${S.gameId}/state`)); render();
  } catch (e) { S.busy = false; await refresh(true); showError(e, lbl, true); }
}
$('offer-send').onclick = () => {
  const price = parseTtari($('offer-price').value);
  if (price === null) return notify('Enter a price in tTARI with at most 6 decimals, greater than zero.', 'bad');
  action('/offer', {seller: S.view.active, buyer: Number($('offer-buyer').value), square: Number($('offer-asset').value), price}, 'Private offer');
};
$('build-asset').onchange = renderTrade; $('sell-asset').onchange = renderTrade;
$('build-go').onclick = () => action('/build', {seat: Number($('build-go').dataset.seat), square: Number($('build-asset').value)}, 'Construction');
$('sell-go').onclick = () => action('/sell', {seat: S.view.active, square: Number($('sell-asset').value)}, 'Sale to the House');
$('roll').onclick = () => startRoll(false);
$('roll-pay').onclick = () => startRoll(true);
$('back').onclick = backToSquare;
$('replay').onclick = e => { e.stopPropagation(); if (!S.flow && !S.busy) playIntro(); };
$('release').onclick = release;
$('play').onclick = openSetup;
document.addEventListener('click', () => { if (!window.OGEIntro.isActive()) { hidePop(); $('sndpanel').hidden = true; } });
$('pop-x').onclick = hidePop; $('pop').addEventListener('click', e => e.stopPropagation());
$('scene').addEventListener('click', e => e.stopPropagation());

// ---------------- developer ----------------
function table(el, head, rows) {
  el.innerHTML = '<tr>' + head.map(h => `<th${h.n ? ' class="n"' : ''}>${esc(h.t ?? h)}</th>`).join('') + '</tr>' +
    rows.map(r => '<tr>' + r.map((c, i) => `<td${head[i].n ? ' class="n"' : ''}${c && c.cls ? ` class="${c.cls}"` : ''}>${c && c.html !== undefined ? c.html : esc(c && c.v !== undefined ? c.v : c)}</td>`).join('') + '</tr>').join('');
}
const short = s => s && s.length > 22 ? s.slice(0, 14) + '…' + s.slice(-6) : s;
function txCell(id) {
  if (!id) return {v: '—'};
  return {html: `<span class="txid"><a href="#" data-tx="${esc(id)}" title="Inspect ${esc(id)}">${esc(short(id))}</a> <button class="copy" data-copy="${esc(id)}" title="Copy transaction id">⧉</button></span>`};
}
function wireTx(root) {
  root.querySelectorAll('[data-tx]').forEach(a => a.onclick = e => { e.preventDefault(); e.stopPropagation(); $('dev-txid').value = a.dataset.tx; $('dev-inspect').click(); $('dev-txid').scrollIntoView({block: 'center'}); });
  root.querySelectorAll('[data-copy]').forEach(b => b.onclick = e => { e.preventDefault(); e.stopPropagation(); copy(b.dataset.copy, 'Copied'); });
}
async function loadAddresses() {
  let rows = [];
  try { rows = await api(`/api/game/${S.gameId}/addresses`); } catch (e) { showError(e, 'Addresses'); return; }
  $('dev-addr-toggle').textContent = S.showAddr ? 'Hide addresses' : 'Show addresses';
  table($('dev-addr'), ['Account', 'Receiving address', 'Component', {t: 'Balance (tTARI)', n: 1}], rows.map(r => {
    const a = r.receiving_address, c = r.component;
    const shown = x => S.showAddr ? esc(x) : '••••••••' + esc(x.slice(-6));
    return [r.label,
      a ? {html: `<span class="txid"><code>${shown(a)}</code> <button class="copy" data-copy="${esc(a)}" title="Copy receiving address">⧉</button></span>`} : {v: 'wallet returned none'},
      {html: `<span class="txid"><code>${shown(c)}</code> <button class="copy" data-copy="${esc(c)}" title="Copy component address">⧉</button></span>`},
      r.balance_ttari ?? '—'];
  }));
  wireTx($('dev-addr'));
}
$('dev-addr-toggle').onclick = () => { S.showAddr = !S.showAddr; loadAddresses(); };
async function loadHealth() {
  try {
    const h = await api(`/api/health?game=${S.gameId}`);
    const yes = (ok, t) => ({v: t, cls: ok ? 'ok' : 'bad'});
    table($('dev-health'), ['Check', 'Result'], [
      ['Wallet', yes(h.wallet.ok, h.wallet.ok ? `reachable · ${h.wallet.ms} ms` : h.wallet.error)],
      ['Chain read', yes(h.chain.ok, h.chain.ok ? `game #${h.chain.game} · round ${h.chain.round} · turn ${h.chain.turn} · ${h.chain.ms} ms` : h.chain.error)],
      ['Templates', yes(h.chain.ok && h.chain.templates_verified, h.chain.ok && h.chain.templates_verified ? 'verified' : 'not verified')],
      ['Transactions', yes(h.live_transactions_enabled, h.live_transactions_enabled ? 'on' : 'off')],
      ['Catalogue', `${h.catalogue.lots} lots · ${h.catalogue.businesses} businesses · ${h.catalogue.specials} special squares`],
      ['Receipts in memory', h.receipts_in_memory], ['Server version', h.version]]);
  } catch (e) { $('dev-health').innerHTML = `<tr><td class="bad">${esc(e.message)}</td></tr>`; }
}
async function loadReceipts() {
  const only = $('dev-unverified').checked;
  try { S.receipts = await api(`/api/game/${S.gameId}/receipts?limit=200${only ? '&unverified=true' : ''}`); } catch (e) { showError(e, 'Receipts'); return; }
  const bad = S.receipts.filter(r => r.status.includes('UNVERIFIED')).length;
  $('dev-rc-badge').textContent = `${S.receipts.length} shown · ${bad} unverified`;
  table($('dev-receipts'), [{t: '#', n: 1}, {t: 'Round', n: 1}, {t: 'Turn', n: 1}, 'Seat', 'By', 'Action', 'Transaction', {t: 'Fee', n: 1}, 'Status', 'Verification'],
    S.receipts.map(r => [r.id, r.round, r.turn, r.seat || '—', r.actor, r.text, txCell(r.transaction_id), r.fee ?? '—',
      {v: r.status, cls: r.status.includes('RECONCILED') || r.status === 'COMMITTED' ? 'ok' : 'bad'}, r.verification || '—']));
  $('dev-receipts').querySelectorAll('tr').forEach((tr, i) => {
    if (i === 0) return; const r = S.receipts[i - 1];
    tr.classList.add('click'); tr.classList.toggle('sel', S.receiptSel === r.id);
    tr.onclick = () => { S.receiptSel = r.id; loadReceiptsSelect(); };
  });
  wireTx($('dev-receipts'));
  renderReceipt();
}
function loadReceiptsSelect() { $('dev-receipts').querySelectorAll('tr').forEach((tr, i) => { if (i) tr.classList.toggle('sel', S.receipts[i - 1]?.id === S.receiptSel); }); renderReceipt(); }
function renderReceipt() {
  const r = S.receipts.find(x => x.id === S.receiptSel), box = $('dev-receipt');
  if (!r) { box.innerHTML = '<div class="box">Click a receipt row to see its verification reads, state differences, pipeline steps, manifest and news events.</div>'; return; }
  const rc = r.reconciliation || {};
  const recon = rc.rows ? '<table class="t"><tr><th>Account</th><th class="n">Expected Δ</th><th class="n">Actual Δ</th><th>Result</th></tr>' +
    rc.rows.map(x => `<tr><td>${esc(x.account)}</td><td class="n">${x.expected}</td><td class="n">${x.actual}</td><td class="${x.match ? 'ok' : 'bad'}">${x.match ? 'match' : 'MISMATCH'}</td></tr>`).join('') + '</table>' : '<div class="box">No reconciliation recorded.</div>';
  const stateTxt = x => x.state?.error ? `re-read failed: ${x.state.error}` : x.state?.differences ? `${x.state.differences} field(s) differ` : 'match';
  const misTxt = x => (x.mismatched || []).map(m => `${m.account}: expected ${m.expected}, actual ${m.actual}`).join(' · ') || (x.error ? String(x.error) : '');
  const reads = (r.reads || []).length ? '<table class="t"><tr><th class="n">Read</th><th class="n">ms after result</th><th>State</th><th>Balances</th><th class="n">Sum</th><th class="n">Fee</th><th>Mismatched rows</th></tr>' +
    r.reads.map(x => `<tr><td class="n">${x.attempt} of ${x.of}</td><td class="n">${x.at_ms}</td><td class="${x.state?.error || x.state?.differences ? 'bad' : 'ok'}">${esc(stateTxt(x))}</td>` +
      `<td class="${x.reconciliation_ok ? 'ok' : 'bad'}">${x.reconciliation_ok ? 'match' : 'differ'}</td><td class="n">${x.sum ?? '—'}</td><td class="n">${x.fee ?? '—'}</td><td>${esc(misTxt(x))}</td></tr>`).join('') + '</table>' : '<div class="box">No verification reads recorded.</div>';
  const diffs = (r.state_differences || []).length ? '<table class="t"><tr><th>Field</th><th>Expected</th><th>Actual</th></tr>' +
    r.state_differences.map(d => `<tr><td>${esc(d.field)}</td><td><code>${esc(JSON.stringify(d.expected))}</code></td><td><code>${esc(JSON.stringify(d.actual))}</code></td></tr>`).join('') + '</table>' : '<div class="box good">The last read matched every state field.</div>';
  box.innerHTML = `<div class="kv"><span>Receipt</span><b>#${r.id} · ${esc(r.operation)} · ${esc(r.actor)}</b><span>Status</span><b class="${r.status.includes('UNVERIFIED') || r.status === 'REJECTED' ? 'bad' : 'ok'}">${esc(r.status)}</b>` +
    `<span>Cause</span><b>${esc(r.cause || 'none')}</b><span>Verification</span><b>${esc(r.verification || '—')}</b><span>Transaction</span><b>${esc(r.transaction_id)}</b><span>Fee</span><b>${r.fee ?? '—'}</b><span>When</span><b>${new Date(r.at * 1000).toLocaleString()}</b></div>` +
    `<h4>Verification reads</h4>${reads}<h4>State differences (last read)</h4>${diffs}<h4>Reconciliation (last read)</h4>${recon}` +
    `<h4>Pipeline steps</h4><div class="pipe">${(r.steps || []).map(x => `<div class="step ${x.status}"><b>${esc(x.name)}</b>${esc(x.detail)}<i>${x.ms} ms</i></div>`).join('') || '<div class="box">None recorded.</div>'}</div>` +
    `<details class="sub"><summary>Manifest</summary><pre class="code">${esc(r.manifest || '(not recorded)')}</pre></details>` +
    `<details class="sub"><summary>News events</summary><pre class="code">${esc(JSON.stringify(r.events || [], null, 2))}</pre></details>` +
    `<details class="sub"><summary>Raw receipt JSON</summary><pre class="code">${esc(JSON.stringify(r, null, 2))}</pre></details>`;
}
$('dev-unverified').onchange = loadReceipts;
async function loadLots() {
  try {
    const rows = await api(`/api/game/${S.gameId}/lots`);
    table($('dev-lots'), [{t: 'Square', n: 1}, 'Zone', 'Business', 'Seat', 'Chosen by', 'When'],
      rows.map(r => [r.square, r.zone || '—', r.name || r.business, r.seat, r.source, new Date(r.at * 1000).toLocaleString()]));
    $('dev-lots-badge').textContent = `${rows.length} lots with a business`;
  } catch (e) { $('dev-lots').innerHTML = `<tr><td class="bad">${esc(e.message)}</td></tr>`; }
}
async function loadDev() {
  try { S.dev = await api(`/api/game/${S.gameId}/dev`); } catch (e) { showError(e, 'Developer read'); return; }
  loadAddresses(); loadHealth(); loadReceipts(); loadLots();
  const d = S.dev, s = d.session;
  $('dev-title').textContent = `Developer · Game #${d.pointers.id}`;
  const tpl = Object.fromEntries(d.templates.map(t => [t.contract, t]));
  const cards = [['IDENTITY', 'Registry', d.contracts.registry.component, tpl.Registry, [`${d.registry.profiles.length} profiles`, 'register / claim by the signer key']],
    ['DIRECTORY', 'Lobby', d.contracts.lobby.component, tpl.Lobby, [`${d.lobby.types.length} game types · ${d.lobby.games.length} games`, `${d.lobby.claims.length} seat claims`]],
    ['MONEY', 'Treasury', d.contracts.treasury.component, tpl.Treasury, [`House ${100 - (d.treasury.community_bps + d.treasury.chance_bps) / 100} / Community ${d.treasury.community_bps / 100} / Chance ${d.treasury.chance_bps / 100}`, 'distribute: session badge only']],
    ['RULES', `Town Square #${d.pointers.id}`, d.pointers.session.component, tpl['Town Square'], [`${s.capacity} seats · round ${s.round}`, s.started ? 'started' : 'not started']]];
  $('dev-contracts').innerHTML = cards.map(([r, h, a, t, l]) => `<div class="ccard"><div class="role">${r}</div><h4>${esc(h)} <span class="${t && t.ok ? 'ok' : 'bad'}" style="font-size:11px">● ${t && t.ok ? 'template verified' : 'template mismatch'}</span></h4><code title="${esc(a)}">${esc(a)}</code><code>template ${esc(t ? t.actual : '?')}</code><ul>${l.map(x => '<li>' + esc(x) + '</li>').join('')}</ul></div>`).join('');
  $('dev-tpl').textContent = d.templates.every(t => t.ok) ? '4 components · all template identities verified' : 'TEMPLATE MISMATCH';
  const p = d.pipeline;
  if (p) {
    $('dev-op').textContent = `${p.operation} · ${p.actor} · ${new Date(p.started_at * 1000).toLocaleTimeString()}`;
    $('dev-pipe').innerHTML = p.steps.map(x => `<div class="step ${x.status}"><b>${esc(x.name)}</b>${esc(x.detail)}<i>${x.ms} ms</i></div>`).join('');
    $('dev-manifest').textContent = (p.manifest || '(not built)') + '\nvariables: ' + JSON.stringify(p.variables, null, 2);
  } else { $('dev-pipe').innerHTML = '<div class="step"><b>No operation</b>None in this server session.</div>'; }
  $('dev-ver').textContent = `version ${d.session_version}`;
  const owners = s.owners.map((o, i) => o ? `${i}:${o}` : null).filter(Boolean).join(' ');
  const lv = s.improvements.map((o, i) => o ? `${i}:${o}` : null).filter(Boolean).join(' ');
  table($('dev-session'), ['Field', 'Value'], [['game_id', s.game_id], ['capacity / seats', `${s.capacity} / ${s.seats.length}`], ['started', s.started],
    ['round · active · turn_count', `${s.round} · ${s.active} · ${s.turn_count}`], ['doubles_streak', s.doubles_streak], ['positions', JSON.stringify(s.positions)],
    ['last dice', `${s.last_die1} + ${s.last_die2}`], ['owners (square:seat)', owners || 'none'], ['development (square:level)', lv || 'none'],
    ['held', JSON.stringify(s.held)], ['held_attempts', JSON.stringify(s.held_attempts)], ['held_owed', JSON.stringify(s.held_owed)], ['out', JSON.stringify(s.out)],
    ['last event', `kind ${s.event_kind} · card ${s.event_card} · ${s.event_amount} · ${s.event_positive ? 'positive' : 'negative'} · seat ${s.event_seat} · square ${s.event_square} · recipient ${s.event_recipient}`],
    ['pending offer', s.offer_square === 65535 ? `none · nonce ${s.offer_nonce}` : `square ${s.offer_square} · ${s.offer_price} · seat ${s.offer_seller} → ${s.offer_buyer} · nonce ${s.offer_nonce}`],
    ['parameters', `GO ${s.go_reward} · reward ${s.square_reward} · fee ${s.fee_amount} · penalty ${s.penalty_amount} · shop ${s.shop_fee} · storage ${s.storage_fee} · house ${s.house_cost} · hotel ${s.hotel_cost} · buyback ${s.buyback_pct}%`],
    ['economics', `GO floor ${s.go_floor} · GO scaled ${s.go_scaled} · rent fee ${s.rent_fee_bps} bps · build on landing ${s.build_on_landing}`],
    ['landed', s.landed_seat ? `seat ${s.landed_seat} on square ${s.landed_square}` : 'none'],
    ['seat_since · resets', `${JSON.stringify(s.seat_since)} · ${JSON.stringify(s.resets)}`]]);
  table($('dev-types'), ['Type', 'Name', 'Template', 'Parameters'], d.lobby.types.map(t => [t.id, t.name, short(t.template || ''), t.params.join(' · ')]));
  const st = ['open', 'started', 'finished'];
  table($('dev-games'), ['Game', 'Type', {t: 'Capacity', n: 1}, 'Session', 'Status'], d.lobby.games.map(g => [g.id, g.type_id, g.capacity, short(g.session || 'none'), {v: st[g.status] || g.status, cls: g.status === 1 ? 'ok' : 'warn'}]));
  table($('dev-claims'), ['Seat', 'Profile', 'Icon', 'Identity (public key)', 'Account'], d.lobby.claims.filter(c => c.game_id === d.pointers.id).map(c => {
    const pr = d.registry.profiles.find(x => x.identity === c.identity), sp = d.pointers.seats[c.seat - 1];
    return [c.seat, pr ? pr.name : 'unregistered', pr ? pr.icon : '', short(c.identity), sp ? sp.account_name : ''];
  }));
  const tg = d.treasury.games.find(g => g.id === d.pointers.id) || {};
  $('dev-split').textContent = `${d.pointers.treasury_mode} · ${100 - (d.treasury.community_bps + d.treasury.chance_bps) / 100} / ${d.treasury.community_bps / 100} / ${d.treasury.chance_bps / 100}`;
  const b = d.balances || {};
  table($('dev-treasury'), ['Account', 'Mode', 'Component', {t: 'Balance (tTARI)', n: 1}], [
    ['House', tg.house_global ? 'global' : 'separate', short(tg.house), b.house !== undefined ? fmt(b.house) : '—'],
    ['Community', tg.community_global ? 'global' : 'separate', short(tg.community), b.community !== undefined ? fmt(b.community) : '—'],
    ['Chance', tg.chance_global ? 'global' : 'separate', short(tg.chance), b.chance !== undefined ? fmt(b.chance) : '—']]);
  const vf = d.verification || {};
  $('dev-verif').textContent = `verification: up to ${vf.reconcile_reads ?? '?'} reads, ${vf.reconcile_read_delay_ms ?? '?'} ms apart · store ${vf.receipts_store ?? '?'}`;
  const last = d.receipts.find(r => r.reconciliation && r.reconciliation.rows);
  if (last) {
    const rc = last.reconciliation;
    $('dev-recon-badge').textContent = `turn ${last.turn} · seat ${last.seat} · ${last.operation} · ${rc.ok ? 'RECONCILED' : 'MISMATCH'} · ${last.verification || ''}`;
    table($('dev-recon'), ['Account', {t: 'Expected Δ (base units)', n: 1}, {t: 'Actual Δ (base units)', n: 1}, 'Result'],
      rc.rows.map(r => [r.account, r.expected, r.actual, {v: r.match ? 'match' : 'MISMATCH', cls: r.match ? 'ok' : 'bad'}])
        .concat([['Sum', rc.sum, rc.sum, {v: rc.sum === -rc.fee ? `= −fee ${rc.fee}` : `≠ −fee ${rc.fee}`, cls: rc.sum === -rc.fee ? 'ok' : 'bad'}]]));
  } else { $('dev-recon-badge').textContent = ''; $('dev-recon').innerHTML = '<tr><td>No reconciled operation recorded.</td></tr>'; }
  $('dev-raw').textContent = JSON.stringify(d.raw, null, 2);
}
$('dev-refresh').onclick = loadDev;
$('dev-inspect').onclick = async () => {
  try { $('dev-txout').textContent = JSON.stringify(await api('/api/tx/inspect', 'POST', {transaction_id: $('dev-txid').value}), null, 2); }
  catch (e) { showError(e, 'Inspect'); }
};
$('dev-txcopy').onclick = () => { if ($('dev-txid').value.trim()) copy($('dev-txid').value.trim(), 'Transaction id'); };
document.querySelectorAll('.tabs button').forEach(b => b.addEventListener('click', () => {
  document.querySelectorAll('.tabs button').forEach(x => x.classList.toggle('on', x === b));
  document.querySelectorAll('.view').forEach(v => v.classList.toggle('on', v.id === 'view-' + b.dataset.view));
  if (b.dataset.view === 'developer') loadDev();
}));

// ---------------- start ----------------
async function init() {
  loadToken();
  renderSound();
  try { S.info = await api('/api/system/info'); } catch (e) { showError(e, 'System info'); }
  try { S.catalogue = await api('/api/catalogue'); } catch (e) { showError(e, 'Catalogue'); }
  await loadMe();
  try { S.games = await api('/api/games'); } catch (e) { showError(e, 'Games'); }
  // Address first, then the last game this browser was in, then the game it holds a seat in, then the first.
  const q = Number(new URLSearchParams(location.search).get('game')), last = lastGame(), has = id => S.games.some(g => g.id === id);
  S.gameId = has(q) ? q : has(last) ? last : (S.me.game ?? S.games[0]?.id ?? 1);
  rememberGame(S.gameId);
  renderGameSelect();
  $('game-select').onchange = () => switchGame(Number($('game-select').value));
  loadBoard();
  if (await refresh()) {
    processNews(S.view, true);
    S.wasMyTurn = S.view.viewer.my_turn;
    notify(S.me.game === S.gameId ? `Game #${S.gameId} · you hold seat ${S.me.seats.join(', ')}.` : `Game #${S.gameId} · state read live from the chain.`, 'ok');
    render(); showLayer('idle');
    lobbyReady = true; fillLobby();
    if (lobbyChoice) enterGame(lobbyChoice);
  } else {
    $('gate-line').textContent = `Game ${S.gameId} could not be read: ${S.lastError?.error?.message || 'see the Developer tab'}. Refresh to try again.`;
  }
}
init();
/* End of app/web/app.js · Location: app/web/app.js · Created: 2026-11-16 04:06:01 UTC */
