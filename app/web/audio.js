/*
  File:      audio.js
  Location:  app/web/audio.js
  Version:   v0.4.1
  Purpose:   Game sound, synthesised with the Web Audio API (no audio files): the approved sound board
             effects, the intro theme on the four-note contract chord (D major: Registry D4, Lobby F#4,
             Town Square A4, Treasury D5) cued from intro.js, and the ambient sound of each landing scene
             (a district base plus the business layer; each special square has its own).
  Notes:     Browsers allow no sound before a click or key on the page: the context starts on the first
             gesture (unlock). Until then every call is silent.
             Settings are stored per browser (localStorage `oge_sound`): sound on/off (on by default),
             volume (70%), other seats' volume (50% of the main volume), ambient on/off (on by default).
             Buses: master ← effects, others (other seats' moves), intro (fades out on skip), ambient.
             OGEAudio.fx(name, {other})  plays one effect; names are listed in OGEAudio.names.
             OGEAudio.cue(step, arg)      intro theme step (called by intro.js).
             OGEAudio.ambient(recipe)     starts a scene ambience {base, layer}; ambient(null) stops it.
             v0.4.1: unlock() returns a promise that settles once the audio is running, so the lobby can
             start the intro only then (in v0.4.0 the intro started before the audio was running and its
             music stayed off).
*/
'use strict';
window.OGEAudio = (function () {
  const KEY = 'oge_sound';
  const S = Object.assign({on: true, volume: 0.7, others: 0.5, ambient: true}, load());
  let ac = null, master = null, fxBus = null, othersBus = null, introBus = null, ambBus = null, dIn = null, nb = null;
  let amb = null; // current ambience {stop()}

  function load() { try { return JSON.parse(localStorage.getItem(KEY) || '{}'); } catch (e) { return {}; } }
  function save() { try { localStorage.setItem(KEY, JSON.stringify(S)); } catch (e) { /* in-memory only */ } }

  // ---------------- context and buses ----------------
  function init() {
    if (ac) return true;
    const AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) return false;
    ac = new AC();
    master = ac.createGain(); master.gain.value = S.on ? S.volume : 0;
    const comp = ac.createDynamicsCompressor(); master.connect(comp); comp.connect(ac.destination);
    fxBus = ac.createGain(); fxBus.connect(master);
    othersBus = ac.createGain(); othersBus.gain.value = S.others; othersBus.connect(master);
    introBus = ac.createGain(); introBus.connect(master);
    ambBus = ac.createGain(); ambBus.gain.value = S.ambient ? 0.35 : 0; ambBus.connect(master);
    const d = ac.createDelay(); d.delayTime.value = .27;
    const fb = ac.createGain(); fb.gain.value = .33; d.connect(fb); fb.connect(d);
    const out = ac.createGain(); out.gain.value = .45; d.connect(out); out.connect(master); dIn = d;
    nb = ac.createBuffer(1, ac.sampleRate * 2, ac.sampleRate);
    const ch = nb.getChannelData(0); for (let i = 0; i < ch.length; i++) ch[i] = Math.random() * 2 - 1;
    return true;
  }
  /** Called on the first click or key (and by the lobby's Enter buttons). Settles when the audio runs. */
  function unlock() {
    if (!init()) return Promise.resolve();
    return ac.state === 'suspended' ? ac.resume().catch(() => {}) : Promise.resolve();
  }
  const ready = () => ac && ac.state === 'running';
  ['pointerdown', 'keydown'].forEach(ev => document.addEventListener(ev, unlock, {capture: true, passive: true}));

  // ---------------- primitives (sound board) ----------------
  let out = null; // the bus the current effect plays into
  function tone(f, t, d, o = {}) {
    const {type = 'sine', g = .2, a = .005, to = null, send = 0} = o;
    const os = ac.createOscillator(), v = ac.createGain(); os.type = type; os.frequency.setValueAtTime(f, t);
    if (to) os.frequency.exponentialRampToValueAtTime(to, t + d);
    v.gain.setValueAtTime(0.0001, t); v.gain.linearRampToValueAtTime(g, t + a); v.gain.exponentialRampToValueAtTime(.0001, t + d);
    os.connect(v); v.connect(out);
    if (send) { const s = ac.createGain(); s.gain.value = send; v.connect(s); s.connect(dIn); }
    os.start(t); os.stop(t + d + .05);
  }
  function bell(f, t, d = 1.2, g = .16, send = .2) { tone(f, t, d, {g, send}); tone(f * 2.01, t, d * .6, {g: g * .35}); tone(f * 3.02, t, d * .35, {g: g * .15}); }
  function noise(t, d, o = {}) {
    const {g = .2, type = 'bandpass', f = 1000, f2 = null, q = 1} = o;
    const s = ac.createBufferSource(); s.buffer = nb;
    const fl = ac.createBiquadFilter(); fl.type = type; fl.frequency.setValueAtTime(f, t);
    if (f2) fl.frequency.exponentialRampToValueAtTime(f2, t + d); fl.Q.value = q;
    const v = ac.createGain(); v.gain.setValueAtTime(.0001, t); v.gain.linearRampToValueAtTime(g, t + .004); v.gain.exponentialRampToValueAtTime(.0001, t + d);
    s.connect(fl); fl.connect(v); v.connect(out); s.start(t, Math.random() * .5); s.stop(t + d + .05);
  }
  const N = {D3: 146.83, D4: 293.66, Fs4: 369.99, A4: 440, D5: 587.33, Fs5: 739.99, A5: 880, D6: 1174.66, Fs6: 1479.98, A6: 1760};
  const CH = [N.D4, N.Fs4, N.A4, N.D5];

  // ---------------- effects (approved sound board) ----------------
  const FX = {
    turn: t => { bell(N.A5, t, .8, .14); bell(N.D6, t + .18, 1, .14); },
    dice: t => {
      for (let i = 0; i < 9; i++) noise(t + i * .045 + Math.random() * .02, .03, {g: .25, f: 2500 + Math.random() * 2000, q: 3});
      [0, .11].forEach(d => { noise(t + .5 + d, .05, {g: .5, f: 1800, q: 4}); tone(220, t + .5 + d, .06, {g: .15, type: 'square'}); });
    },
    tick: t => { tone(1250, t, .03, {type: 'square', g: .05}); },
    land: t => { tone(140, t, .2, {g: .35, to: 55}); },
    go: t => { bell(N.D6, t, 1.4, .18, .3); bell(N.Fs6, t + .12, 1.2, .1, .3); },
    purchase: t => { noise(t, .08, {g: .3, f: 900, q: 2}); bell(2093, t + .08, .5, .12, .1); bell(2637, t + .2, .7, .12, .2); },
    build1: t => { [0, .18, .36].forEach(d => { noise(t + d, .05, {g: .45, f: 1400, q: 3}); tone(180, t + d, .07, {g: .18}); }); [N.D5, N.Fs5, N.A5].forEach((f, i) => bell(f, t + .62 + i * .1, .8, .1, .2)); },
    build2: t => { [0, .16, .32].forEach(d => { noise(t + d, .05, {g: .45, f: 1400, q: 3}); tone(180, t + d, .07, {g: .18}); }); [N.D5, N.Fs5, N.A5, N.D6].forEach((f, i) => tone(f, t + .55 + i * .07, 1.1, {type: 'triangle', g: .08, send: .25})); },
    hotel: t => {
      [[N.D4, 0, .18], [N.D4, .2, .12], [N.Fs4, .34, .18], [N.A4, .54, .18], [N.D5, .76, .9]].forEach(([f, s, d]) => { tone(f, t + s, d, {type: 'sawtooth', g: .07}); tone(f * 2, t + s, d, {type: 'triangle', g: .05, send: .2}); });
      [N.D4, N.Fs4, N.A4].forEach(f => tone(f, t + .76, 1, {type: 'triangle', g: .06}));
    },
    rent_paid: t => { for (let i = 0; i < 6; i++) bell(2600 - i * 260, t + i * .07, .25, .07, 0); },
    rent_earned: t => { for (let i = 0; i < 6; i++) bell(1300 + i * 260, t + i * .07, .25, .07, .15); },
    chance: t => { noise(t, .18, {g: .2, f: 1500, f2: 6000, q: 1}); for (let i = 0; i < 6; i++) tone(2600 + Math.random() * 1800, t + .2 + i * .06, .2, {g: .04, send: .4}); },
    community: t => { noise(t, .18, {g: .2, f: 1500, f2: 6000, q: 1}); bell(N.A5, t + .22, 1, .12, .25); },
    fee: t => { tone(110, t, .25, {g: .3, to: 70}); tone(90, t + .05, .3, {type: 'square', g: .05}); },
    maintenance: t => { [0, .32].forEach(d => { tone(330, t + d, .26, {type: 'square', g: .06}); tone(415, t + d, .26, {type: 'square', g: .05}); }); [1.27, 2.41, 3.9].forEach(m => tone(900 * m, t + .75, .25, {g: .06})); },
    released: t => { for (let i = 0; i < 7; i++) noise(t + i * .035, .02, {g: .3, f: 3000, q: 5}); bell(N.A5, t + .32, .9, .13, .2); },
    regenerate: t => { noise(t, .7, {g: .25, f: 300, f2: 5000, q: 1.5}); tone(300, t + .1, .6, {g: .06, to: 900, send: .3}); },
    news: t => { tone(880, t, .06, {g: .07}); },
    error: t => { [0, .16].forEach(d => tone(110, t + d, .12, {type: 'square', g: .08})); },
  };
  /** Plays an effect. {other: true} routes it to the other seats' bus (reduced volume). {at: seconds from now}. */
  function fx(name, o = {}) {
    if (!ready() || !S.on || !FX[name]) return;
    out = o.other ? othersBus : fxBus;
    FX[name](ac.currentTime + .02 + (o.at || 0));
  }

  // ---------------- intro theme (cued from intro.js at its real beats) ----------------
  let introOn = false;
  function introPad(t, dur) {
    [N.D3, N.D3 * 1.005, N.A4 / 2].forEach(f => {
      const o = ac.createOscillator(), fl = ac.createBiquadFilter(), v = ac.createGain();
      o.type = 'sawtooth'; o.frequency.value = f; fl.type = 'lowpass';
      fl.frequency.setValueAtTime(180, t); fl.frequency.exponentialRampToValueAtTime(1300, t + 1); fl.frequency.exponentialRampToValueAtTime(500, t + dur);
      v.gain.setValueAtTime(.0001, t); v.gain.linearRampToValueAtTime(.05, t + .9); v.gain.setValueAtTime(.05, t + dur - 1.2); v.gain.exponentialRampToValueAtTime(.0001, t + dur);
      o.connect(fl); fl.connect(v); v.connect(out); o.start(t); o.stop(t + dur + .1);
    });
  }
  const AR = [N.D4, N.Fs4, N.A4, N.D5, N.Fs5, N.A5, N.D6, N.Fs6];
  const SEAT_NOTES = [N.D5, N.Fs5, N.A5, N.D6, N.Fs6, N.A6];
  /**
   * Intro steps, in the order intro.js reaches them:
   *   start · node(i 0..3) · link(i 0..2) · pulses · tile(i 0..39) · fade · title · piece(k) · fly · hand · end · skip
   * The pad runs for the full 13.92 s of the intro; skip fades everything out in 0.4 s.
   */
  function cue(step, arg) {
    if (step === 'start') {
      introOn = ready() && S.on;
      if (!introOn) return;
      introBus.gain.cancelScheduledValues(ac.currentTime); introBus.gain.setValueAtTime(1, ac.currentTime);
    }
    if (!introOn || !ready()) return;
    out = introBus;
    const t = ac.currentTime + .01;
    switch (step) {
      case 'start': introPad(t, 13.92); break;
      case 'node': bell(CH[arg] || CH[0], t, 2.4, .13, .25); break;
      case 'link': tone(CH[(arg * 3 + 1) % 4] * 2, t, .22, {type: 'triangle', g: .07, send: .4}); break;
      case 'pulses': for (let k = 0; k < 8; k++) tone(CH[(k * 3) % 4] * 2, t + k * .13, .22, {type: 'triangle', g: .06, send: .4}); break;
      case 'tile': tone(AR[arg % 8], t, .12, {type: 'triangle', g: .045 + .03 * (arg / 40), send: .15}); break;
      case 'fade': tone(N.D3, t, .4, {type: 'triangle', g: .04}); break;
      case 'title':
        [N.D3, N.D4, N.Fs4, N.A4, N.D5].forEach(f => { tone(f, t, 1.8, {type: 'triangle', g: .1}); tone(f * 2, t, 1.2, {g: .04}); });
        noise(t, .6, {g: .06, type: 'highpass', f: 6000});
        for (let k = 0; k < 10; k++) tone(2349 + Math.random() * 1600, t + .15 + k * .09, .25, {g: .03, send: .4});
        break;
      case 'piece': tone(SEAT_NOTES[arg % SEAT_NOTES.length] * (arg >= SEAT_NOTES.length ? 2 : 1), t, .35, {type: 'triangle', g: .11, send: .3}); break;
      case 'fly': tone(1200, t, .55, {g: .08, to: 300}); noise(t, .5, {g: .05, f: 2500, f2: 400, q: 2}); break;
      case 'hand': bell(N.A5, t, .9, .14); bell(N.D6, t + .2, 1.1, .14); break;
      case 'end': introBus.gain.setValueAtTime(1, t); introBus.gain.linearRampToValueAtTime(0.0001, t + .6); introOn = false; break;
      case 'skip': introBus.gain.cancelScheduledValues(t); introBus.gain.setValueAtTime(introBus.gain.value, t); introBus.gain.linearRampToValueAtTime(0.0001, t + .4); introOn = false; break;
    }
  }

  // ---------------- ambience ----------------
  // Continuous beds are filtered noise with a slow swell; layers are short events repeated at random intervals.
  function bed(o) {
    const s = ac.createBufferSource(); s.buffer = nb; s.loop = true;
    const fl = ac.createBiquadFilter(); fl.type = o.type || 'lowpass'; fl.frequency.value = o.f; fl.Q.value = o.q || .7;
    const v = ac.createGain(); v.gain.value = o.g;
    s.connect(fl); fl.connect(v); v.connect(ambBus);
    let lfo = null;
    if (o.lfo) {
      lfo = ac.createOscillator(); lfo.frequency.value = o.lfo; const lg = ac.createGain(); lg.gain.value = o.g * (o.depth || .8);
      lfo.connect(lg); lg.connect(v.gain); lfo.start();
    }
    s.start();
    return () => { try { s.stop(); lfo && lfo.stop(); } catch (e) { /* already stopped */ } };
  }
  function every(min, max, play) {
    let timer = null, live = true;
    const go = () => { if (!live) return; if (ready()) { out = ambBus; play(ac.currentTime + .02); } timer = setTimeout(go, (min + Math.random() * (max - min)) * 1000); };
    timer = setTimeout(go, (min * .3 + Math.random() * min) * 1000);
    return () => { live = false; clearTimeout(timer); };
  }
  const R = (a, b) => a + Math.random() * (b - a);
  const LAYERS = {
    // businesses
    pages: () => every(2.5, 5, t => noise(t, .25, {g: .12, f: 3500, f2: 1800, q: .8})),
    bees: () => bed({f: 220, type: 'bandpass', q: 8, g: .05, lfo: .7, depth: .9}),
    espresso: () => every(6, 11, t => { noise(t, 1.4, {g: .12, type: 'highpass', f: 2500}); tone(120, t, .2, {g: .05}); }),
    cups: () => every(2, 5, t => bell(R(2400, 3400), t, .25, .05, .1)),
    owl: () => every(6, 12, t => { tone(420, t, .35, {g: .07, to: 380}); tone(400, t + .5, .5, {g: .06, to: 340}); }),
    chatter: () => bed({f: 700, type: 'bandpass', q: 1.2, g: .12, lfo: .3, depth: .5}),
    gulls: () => every(4, 9, t => { for (let k = 0; k < 3; k++) tone(R(1300, 1700), t + k * .22, .2, {g: .05, to: R(800, 1000)}); }),
    chimes: () => every(1.5, 4, t => bell([N.D6, N.Fs6, N.A6, N.D5 * 2][Math.floor(Math.random() * 4)], t, 1.6, .04, .3)),
    fire: () => { const a = bed({f: 400, type: 'lowpass', g: .06}); const b = every(.15, .6, t => noise(t, .02, {g: R(.05, .15), f: R(2000, 5000), q: 4})); return () => { a(); b(); }; },
    bell: () => every(7, 14, t => bell(N.A4, t, 2.2, .08, .3)),
    water: () => bed({f: 1200, type: 'bandpass', q: .9, g: .09, lfo: .5, depth: .4}),
    'music-box': () => every(.35, .6, t => bell([N.D6, N.A5, N.Fs6, N.A6, N.D6, N.Fs5][Math.floor(Math.random() * 6)], t, .6, .035, .3)),
    clock: () => every(1, 1, t => tone(2000, t, .02, {type: 'square', g: .03})),
    bubbling: () => every(.2, .7, t => tone(R(300, 700), t, .08, {g: .05, to: R(800, 1200)})),
    steam: () => every(4, 9, t => noise(t, R(1, 2), {g: .1, type: 'highpass', f: 3000})),
    hum: () => { const o = ac.createOscillator(), v = ac.createGain(); o.type = 'sawtooth'; o.frequency.value = 60; v.gain.value = .015; o.connect(v); v.connect(ambBus); o.start(); return () => { try { o.stop(); } catch (e) { /* stopped */ } }; },
    projector: () => every(.09, .09, t => noise(t, .02, {g: .06, f: 1800, q: 3})),
    saw: () => every(4, 8, t => { for (let k = 0; k < 6; k++) noise(t + k * .18, .14, {g: .1, f: 2200, f2: 3200, q: 3}); }),
    chains: () => every(3, 7, t => { for (let k = 0; k < 5; k++) bell(R(3000, 4200), t + k * .06, .12, .03, 0); }),
    horn: () => every(10, 18, t => { tone(110, t, 1.4, {type: 'sawtooth', g: .04}); tone(165, t, 1.4, {type: 'sawtooth', g: .03}); }),
    'tram-bell': () => every(8, 16, t => { bell(N.A5, t, .6, .07, .2); bell(N.A5, t + .25, .6, .07, .2); }),
    engine: () => bed({f: 90, type: 'lowpass', q: 4, g: .14, lfo: 6, depth: .3}),
    whistle: () => every(9, 16, t => { tone(1400, t, .5, {g: .04}); tone(1750, t, .5, {g: .03}); }),
    ratchet: () => every(3, 7, t => { for (let k = 0; k < 8; k++) noise(t + k * .045, .02, {g: .15, f: 3000, q: 5}); }),
    clank: () => every(2.5, 6, t => { bell(R(500, 900), t, .4, .05, .1); noise(t, .05, {g: .15, f: 1500, q: 2}); }),
    hammer: () => every(1.2, 3, t => { [0, .35].forEach(d => { bell(R(1800, 2400), t + d, .3, .05, .1); noise(t + d, .04, {g: .2, f: 2000, q: 2}); }); }),
    creak: () => every(5, 10, t => tone(R(160, 220), t, .6, {type: 'sawtooth', g: .02, to: R(120, 160)})),
    fizz: () => bed({f: 7000, type: 'highpass', g: .03, lfo: .2, depth: .5}),
    rigging: () => every(2, 5, t => bell(R(700, 1100), t, .5, .03, .2)),
    foghorn: () => every(14, 22, t => { tone(82, t, 2.2, {type: 'sawtooth', g: .05}); tone(123, t, 2.2, {type: 'sine', g: .04}); }),
    murmur: () => bed({f: 500, type: 'bandpass', q: 1, g: .1, lfo: .2, depth: .4}),
    applause: () => every(12, 20, t => { for (let k = 0; k < 40; k++) noise(t + R(0, 1.6), .02, {g: .06, f: R(1500, 4000), q: 2}); }),
    strings: () => every(5, 8, t => [N.D4, N.Fs4, N.A4].forEach(f => tone(f, t, 3, {type: 'sawtooth', g: .012, a: 1}))),
    waves: () => bed({f: 600, type: 'lowpass', g: .16, lfo: .12, depth: .9}),
    tools: () => every(2, 5, t => { noise(t, .04, {g: .15, f: 2600, q: 3}); bell(R(1500, 2500), t, .2, .03, 0); }),
    // special squares
    clicks: () => every(.5, 1.2, t => noise(t, .015, {g: .12, f: 3500, q: 6})),
    station: () => { const a = bed({f: 300, type: 'lowpass', g: .07}); const b = every(14, 20, t => [N.Fs5, N.D5, N.A4].forEach((f, k) => bell(f, t + k * .5, 1, .06, .2))); return () => { a(); b(); }; },
    crickets: () => every(.6, 1.4, t => { for (let k = 0; k < 4; k++) tone(4400, t + k * .05, .03, {g: .02}); }),
  };
  const BASES = {
    'Market Lane': () => bed({f: 500, type: 'bandpass', q: .8, g: .05, lfo: .15, depth: .5}),
    'Main Street': () => { const a = bed({f: 350, type: 'lowpass', g: .07}); const b = every(9, 15, t => noise(t, 1.6, {g: .05, f: 300, f2: 900, q: 1})); return () => { a(); b(); }; },
    'Works Row': () => { const a = bed({f: 120, type: 'lowpass', g: .1}); const b = every(3, 7, t => { noise(t, .06, {g: .1, f: 900, q: 2}); }); return () => { a(); b(); }; },
    'Harbour Front': () => bed({f: 500, type: 'lowpass', g: .12, lfo: .1, depth: .8}),
    vacant: () => LAYERS.crickets(),
  };
  /** Starts a scene's ambience: {base: district name or 'vacant' or null, layer: layer name}. null stops it. */
  function ambient(recipe) {
    if (amb) { const old = amb; amb = null; fadeAmb(old); }
    if (!recipe || !ready() || !S.on) return;
    ambBus.gain.cancelScheduledValues(ac.currentTime);
    ambBus.gain.setValueAtTime(0.0001, ac.currentTime);
    ambBus.gain.linearRampToValueAtTime(S.ambient ? 0.35 : 0.0001, ac.currentTime + 1.2);
    const stops = [];
    if (recipe.base && BASES[recipe.base]) stops.push(BASES[recipe.base]());
    if (recipe.layer && LAYERS[recipe.layer]) stops.push(LAYERS[recipe.layer]());
    amb = {stop: () => stops.forEach(f => f())};
  }
  function fadeAmb(a) {
    if (!ac) { a.stop(); return; }
    const t = ac.currentTime;
    ambBus.gain.cancelScheduledValues(t); ambBus.gain.setValueAtTime(ambBus.gain.value, t); ambBus.gain.linearRampToValueAtTime(0.0001, t + .8);
    setTimeout(() => a.stop(), 850);
  }

  // ---------------- settings ----------------
  function apply() {
    if (!ac) return;
    const t = ac.currentTime;
    master.gain.setTargetAtTime(S.on ? S.volume : 0, t, .05);
    othersBus.gain.setTargetAtTime(S.others, t, .05);
    if (amb) ambBus.gain.setTargetAtTime(S.ambient ? 0.35 : 0.0001, t, .2);
  }
  function set(k, v) { S[k] = v; save(); apply(); }
  return {
    unlock, fx, cue, ambient, set, settings: () => Object.assign({}, S), ready,
    names: Object.keys(FX), layers: Object.keys(LAYERS), bases: Object.keys(BASES),
  };
})();
/* End of app/web/audio.js · Location: app/web/audio.js · Created: 2026-11-16 15:53:22 UTC */
