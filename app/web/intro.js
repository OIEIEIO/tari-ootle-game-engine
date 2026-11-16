/*
  File:      intro.js
  Location:  app/web/intro.js
  Version:   v0.4.0
  Purpose:   Town Square intro (preview v0.1.1 behaviour) on the live board with real data:
             outline trace → four contract nodes (Registry, Lobby, Town Square, Treasury) →
             links with pulses → light races from the Town Square node to GO and round the board,
             lighting tiles and owners' strips → title → seat pieces → pieces fly to their real
             squares → hand-over line ("SEAT n TO MOVE" or "ROUND r · SEAT n TO MOVE").
  Notes:     Plays on every page load and from "Replay intro". Click or any key skips to the end.
             New game vs game in progress comes from the backend (view.game.phase).
             v0.4.0: the animation and its timing are unchanged. The intro theme (audio.js) is cued at
             each step so the music follows the animation: start (pad), node ×4 (the four-note contract
             chord), link ×3, pulses, tile ×40 (on the race's eased timing), fade, title, piece per seat,
             fly, hand, end. A skip fades the music out with the animation. Pieces are the seat's SVG
             silhouette (pieces.js) in the seat colour instead of an emoji.
*/
'use strict';
window.OGEIntro = (function () {
  const NS = 'http://www.w3.org/2000/svg';
  const NODES = [
    {key: 'registry', label: 'REGISTRY', sub: 'player identities', glyph: '◉', c: '#4cc9f0', x: .22, y: .24},
    {key: 'lobby', label: 'LOBBY', sub: 'games and seats', glyph: '▦', c: '#b48cff', x: .78, y: .24},
    {key: 'town', label: 'TOWN SQUARE', sub: 'game rules', glyph: '⬢', c: '#ffd23f', x: .78, y: .76},
    {key: 'treasury', label: 'TREASURY', sub: '90 / 5 / 5', glyph: '◈', c: '#2ee6a6', x: .22, y: .76},
  ];
  const LINKS = [['registry', 'lobby'], ['lobby', 'town'], ['town', 'treasury']];
  let run = 0, active = false, ctx = null;
  const cue = (step, arg) => { try { window.OGEAudio && window.OGEAudio.cue(step, arg); } catch (e) { /* sound is optional */ } };

  const wait = (ms, id) => new Promise((res, rej) => setTimeout(() => id === run ? res() : rej('skip'), ms));
  function rel(el) {
    const b = ctx.board.getBoundingClientRect(), r = el.getBoundingClientRect();
    return {x: r.left - b.left + r.width / 2, y: r.top - b.top + r.height / 2, w: r.width, h: r.height, l: r.left - b.left, t: r.top - b.top};
  }
  const centerBox = () => rel(ctx.stage);
  function nodePoint(n) { const c = centerBox(); return {x: c.l + c.w * n.x, y: c.t + c.h * n.y}; }
  function svgEl(tag, attrs) { const e = document.createElementNS(NS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); ctx.svg.append(e); return e; }
  function drawStroke(el, ms) {
    const L = el.getTotalLength(); el.style.strokeDasharray = L; el.style.strokeDashoffset = L; el.getBoundingClientRect();
    el.style.transition = `stroke-dashoffset ${ms}ms linear`; el.style.strokeDashoffset = 0;
  }
  function easeInv(y) { // inverse of cubic-bezier(.45,.05,.55,.95), solved numerically
    const bez = (t, a, b) => 3 * a * t * (1 - t) * (1 - t) + 3 * b * t * t * (1 - t) + t * t * t;
    let lo = 0, hi = 1; for (let k = 0; k < 30; k++) { const m = (lo + hi) / 2; if (bez(m, .05, .95) < y) lo = m; else hi = m; }
    return bez((lo + hi) / 2, .45, .55);
  }
  function el(cls, html, parent) { const e = document.createElement('div'); e.className = cls; if (html !== undefined) e.innerHTML = html; (parent || ctx.board).append(e); return e; }
  const pieceSvg = (st, size) => window.OGEPieces.svg(st.piece_id, st.color, size);

  function clean() {
    ctx.board.querySelectorAll('.ifx,.inode,.piece').forEach(e => e.remove());
    ctx.tiles.forEach(t => t.classList.remove('lit', 'flash'));
  }
  function setup() {
    clean();
    ctx.board.classList.add('intro');
    ctx.tiles.forEach(t => { const s = t.querySelector('.strip'); if (s) s.style.background = ''; });
    ctx.svg = document.createElementNS(NS, 'svg'); ctx.svg.setAttribute('class', 'ifx'); ctx.board.append(ctx.svg);
    const v = ctx.view, g = v.game;
    ctx.title = el('ifx ititle', `<h1>OOTLE GAME ENGINE</h1><p>${g.type_name.toUpperCase()} · GAME #${g.id} · ${g.capacity} SEATS</p>`, ctx.stage);
    ctx.hand = el('ifx ihand', '', ctx.stage);
    ctx.skipnote = el('ifx iskip', 'Click or press any key to skip', ctx.stage);
  }
  function ownerColor(i) {
    const sq = ctx.view.squares[i]; if (!sq || !sq.owner) return null;
    return ctx.view.seats[sq.owner - 1].color;
  }
  function lightAll() {
    ctx.tiles.forEach((t, i) => { t.classList.add('lit'); const c = ownerColor(i), s = t.querySelector('.strip'); if (c && s) s.style.background = c; });
  }
  function makePiece(st, k) {
    const c = centerBox(), n = ctx.view.seats.length, e = el('piece');
    e.style.setProperty('--pc', st.color);
    const cols = n <= 4 ? n : Math.ceil(Math.sqrt(n * 1.5)), rows = Math.ceil(n / cols), row = Math.floor(k / cols), col = k % cols;
    const inRow = row === rows - 1 ? n - cols * (rows - 1) : cols;
    if (n > 12) e.style.setProperty('--sz', '38px'); else if (n > 6) e.style.setProperty('--sz', '48px');
    e.style.left = (c.l + c.w * (col + 1) / (inRow + 1)) + 'px';
    e.style.top = (c.t + c.h * (rows === 1 ? .62 : .5 + row * (.34 / (rows - 1)))) + 'px';
    e.innerHTML = `${pieceSvg(st, '62%')}<div class="cap"><b>${st.piece}</b><span>Seat ${st.seat}</span></div>`;
    e.dataset.seat = st.seat; e.getBoundingClientRect(); return e;
  }
  function place(e, st) {
    const seats = ctx.view.seats, pos = st.position, here = seats.filter(x => x.position === pos), k = here.indexOf(st), t = rel(ctx.tiles[pos]);
    const cols = Math.min(here.length, 3), row = Math.floor(k / 3), col = k % 3, rows = Math.ceil(here.length / 3);
    e.style.left = (t.x + (col - (cols - 1) / 2) * 22) + 'px'; e.style.top = (t.y + (row - (rows - 1) / 2) * 22) + 'px';
  }
  function handHtml() {
    const v = ctx.view, a = v.seats[v.active - 1];
    return (v.game.phase === 'new' ? '' : `ROUND ${v.round} · `) + `SEAT ${a.seat} <span class="ihp">${pieceSvg(a, '1.2em')}</span> TO MOVE`;
  }
  function finish(how) {
    if (!active) return;
    active = false; run++;
    cue(how === 'skip' ? 'skip' : 'end');
    clean(); ctx.board.classList.remove('intro');
    document.removeEventListener('keydown', onSkip, true);
    document.removeEventListener('click', onSkip, true);
    const done = ctx.onDone; ctx.onDone = null; if (done) done();
  }
  function onSkip(e) { if (e.target && e.target.id === 'replay') return; e.stopPropagation(); e.preventDefault(); skip(); }
  function skip() { if (active) finish('skip'); }

  async function sequence(id) {
    const c = centerBox();
    cue('start');
    const outline = svgEl('rect', {x: c.l + 6, y: c.t + 6, width: c.w - 12, height: c.h - 12, rx: 14, stroke: '#1f3a5c', 'stroke-width': 1.5});
    drawStroke(outline, 900);
    await wait(1000, id);
    const nodeEls = {};
    for (const [i, n] of NODES.entries()) {
      const p = nodePoint(n), e = el('inode', `<div class="orb">${n.glyph}</div><b>${n.label}</b><small>${n.sub}</small>`);
      e.style.setProperty('--c', n.c); e.style.left = p.x + 'px'; e.style.top = p.y + 'px';
      nodeEls[n.key] = e; e.getBoundingClientRect(); e.classList.add('in');
      cue('node', i);
      await wait(450, id);
    }
    await wait(200, id);
    const byKey = Object.fromEntries(NODES.map(n => [n.key, n])), lines = [];
    for (const [i, [a, b]] of LINKS.entries()) {
      const p = nodePoint(byKey[a]), q = nodePoint(byKey[b]);
      const ln = svgEl('line', {x1: p.x, y1: p.y, x2: q.x, y2: q.y, stroke: byKey[a].c, 'stroke-width': 2, opacity: .85});
      ln.style.filter = `drop-shadow(0 0 4px ${byKey[a].c})`; drawStroke(ln, 320); lines.push([ln, p, q, byKey[b].c]);
      cue('link', i);
      await wait(330, id);
    }
    const dots = lines.map(([, p, , col]) => svgEl('circle', {cx: p.x, cy: p.y, r: 4, fill: col}));
    const t0 = performance.now();
    cue('pulses');
    await new Promise((res, rej) => { (function step(now) {
      if (id !== run) return rej('skip');
      const k = ((now - t0) / 700) % 1;
      lines.forEach(([, p, q], i) => { dots[i].setAttribute('cx', p.x + (q.x - p.x) * k); dots[i].setAttribute('cy', p.y + (q.y - p.y) * k); });
      if (now - t0 < 1050) requestAnimationFrame(step); else { dots.forEach(d => d.remove()); res(); }
    })(t0); });
    const start = nodePoint(byKey.town), go = rel(ctx.tiles[0]);
    let d = `M${start.x},${start.y} L${go.x},${go.y}`;
    for (let i = 1; i <= 40; i++) { const t = rel(ctx.tiles[i % 40]); d += ` L${t.x},${t.y}`; }
    const race = svgEl('path', {d, stroke: '#4cc9f0', 'stroke-width': 2.5, opacity: .9});
    race.style.filter = 'drop-shadow(0 0 6px #4cc9f0)';
    const L = race.getTotalLength(), lead = Math.hypot(go.x - start.x, go.y - start.y);
    race.style.strokeDasharray = L; race.style.strokeDashoffset = L; race.getBoundingClientRect();
    const RACE_MS = 2000; race.style.transition = `stroke-dashoffset ${RACE_MS}ms cubic-bezier(.45,.05,.55,.95)`; race.style.strokeDashoffset = 0;
    const per = (L - lead) / 40;
    for (let i = 0; i < 40; i++) {
      setTimeout(() => {
        if (id !== run) return;
        const t = ctx.tiles[i]; t.classList.add('lit', 'flash');
        const col = ownerColor(i), s = t.querySelector('.strip'); if (col && s) s.style.background = col;
        cue('tile', i);
        setTimeout(() => t.classList.remove('flash'), 350);
      }, easeInv((lead + per * i) / L) * RACE_MS);
    }
    await wait(RACE_MS + 150, id);
    [race, outline, ...lines.map(l => l[0])].forEach(x => { x.style.transition = 'opacity .8s'; x.style.opacity = 0; });
    Object.values(nodeEls).forEach(e => e.classList.add('out'));
    cue('fade');
    await wait(350, id);
    ctx.title.classList.add('in');
    cue('title');
    await wait(1150, id);
    ctx.title.classList.add('up');
    await wait(500, id);
    const pieces = ctx.view.seats.map((st, k) => makePiece(st, k));
    for (const [k, e] of pieces.entries()) { e.classList.add('in'); cue('piece', k); await wait(170, id); }
    await wait(1400, id);
    ctx.title.classList.remove('in', 'up');
    pieces.forEach((e, k) => { place(e, ctx.view.seats[k]); e.classList.add('landed'); });
    cue('fly');
    await wait(950, id);
    ctx.hand.innerHTML = `<div class="ihl">${handHtml()}</div>`; ctx.hand.classList.add('in');
    ctx.board.querySelector(`.piece[data-seat="${ctx.view.active}"]`)?.classList.add('active');
    cue('hand');
    await wait(1500, id);
    ctx.hand.classList.remove('in');
    await wait(400, id);
  }

  /** Starts (or restarts) the intro. opts: {board, stage, tiles, view, onDone}. */
  function start(opts) {
    if (active) { run++; active = false; cue('skip'); }
    ctx = Object.assign({}, opts);
    const id = ++run; active = true;
    setup();
    document.addEventListener('keydown', onSkip, true);
    document.addEventListener('click', onSkip, true);
    sequence(id).then(() => { if (id === run) finish('end'); }).catch(e => { if (e !== 'skip') { console.error(e); finish('end'); } });
  }
  return {start, skip, isActive: () => active, lightAll};
})();
/* End of app/web/intro.js · Location: app/web/intro.js · Created: 2026-11-15 23:05:29 UTC */
