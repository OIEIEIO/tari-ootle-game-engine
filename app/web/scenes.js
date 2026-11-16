/*
  File:      scenes.js
  Location:  app/web/scenes.js
  Version:   v0.4.0
  Purpose:   Landing scenes for all 40 squares, drawn as SVG at a fixed 680 x 440 with the title inside.
             Lots (22 squares, 3 businesses each) go through the approved stages:
               vacant   fenced lot, "For sale" sign with the price, zone placard, survey grid, and the
                        chosen business's blueprint (cyan dashed)
               bought   (level 0) the same lot with only the business's silhouette; the sign reads
                        "Sold · Seat N · name"
               level 1  the silhouette fills in colour, the lot markings go, the lights come on
               level 2  the extension (shown as a blueprint, then filled when built)
               level 3  the flagship, with spotlights
             Special squares (18) have their own scenes and no levels.
  Notes:     The Copper Kettle, The Pirate Café and Lantern Noodle House are drawn exactly as in the
             approved previews. The other 63 businesses use their zone's building shell in the business's
             colours (catalogue `main` and `accent`), its sign, its motif, an extension and a flagship tower
             carrying the business's `short` name.
             OGEScenes.lot(lot, square, opts) / OGEScenes.special(special, square, opts) return SVG markup.
             OGEScenes.setStage(svg, stage, animate) moves a rendered lot scene between stages.
             OGEScenes.choose(svg, businessId) shows another business's blueprint on a vacant lot.
*/
'use strict';
window.OGEScenes = (function () {
  const W = 680, H = 440;
  const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
  function shade(hex, k) { // k < 1 darkens, k > 1 lightens (towards white)
    const n = parseInt(hex.slice(1), 16), c = [n >> 16, (n >> 8) & 255, n & 255];
    const m = c.map(v => k <= 1 ? Math.round(v * k) : Math.round(v + (255 - v) * (k - 1)));
    return '#' + m.map(v => Math.max(0, Math.min(255, v)).toString(16).padStart(2, '0')).join('');
  }
  function seeded(seed) { let s = seed; return () => { s = (s * 9301 + 49297) % 233280; return s / 233280; }; }
  const R = (x, y, w, h, f, extra = '') => `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${f}"${extra}/>`;
  const WIN = (x, y, w, h) => `<rect class="win" x="${x}" y="${y}" width="${w}" height="${h}"/>`;
  const T = (x, y, s, f, txt, extra = '') => `<text x="${x}" y="${y}" text-anchor="middle" font-size="${s}" font-weight="500" fill="${f}"${extra}>${esc(txt)}</text>`;

  // ---------------- backdrop ----------------
  function backdrop(square, district) {
    const r = seeded(square * 7 + 3);
    let stars = '';
    for (let i = 0; i < 70; i++) stars += `<circle cx="${(r() * 680).toFixed(1)}" cy="${(r() * 210).toFixed(1)}" r="${(r() * 1.2 + .3).toFixed(2)}" fill="#c9d2e3" opacity="${(r() * .6 + .2).toFixed(2)}"/>`;
    let water = '';
    if (district === 'Harbour Front') water = `<rect x="0" y="318" width="680" height="30" fill="#0a1626"/><g stroke="#1d3350" stroke-width="1.2">${[0, 1, 2, 3, 4, 5].map(i => `<path d="M${20 + i * 110} 330 q12 -5 24 0 t24 0" fill="none"/>`).join('')}</g>`;
    const sky = district === 'Works Row' ? '#090b12' : '#070b14';
    return `<rect width="${W}" height="${H}" fill="${sky}"/>${stars}` +
      `<circle cx="600" cy="56" r="18" fill="#e9e3c9" opacity=".85"/><circle cx="608" cy="50" r="16" fill="${sky}"/>` +
      `<g class="beams" data-part="beams" style="opacity:0"><polygon class="beam" points="150,350 120,0 190,0" style="transform-origin:150px 350px"/><polygon class="beam" points="540,350 510,0 580,0" style="transform-origin:540px 350px;animation-delay:-3s"/></g>` +
      `<g fill="#0e1628">${R(0, 250, 60, 100, '#0e1628')}${R(55, 220, 40, 130, '#0e1628')}${R(95, 265, 50, 85, '#0e1628')}${R(580, 260, 30, 90, '#0e1628')}${R(610, 215, 40, 135, '#0e1628')}${R(645, 255, 35, 95, '#0e1628')}</g>` +
      water +
      `${R(0, 350, 680, 90, '#0b111e')}${R(0, 346, 680, 6, '#1a2336')}` +
      `<g fill="#1f2a40">${[20, 110, 200, 290, 380, 470, 560].map(x => R(x, 400, 40, 3, '#1f2a40')).join('')}</g>` +
      `<g>${R(118, 262, 3, 88, '#2a3550')}<circle cx="119" cy="266" r="34" fill="#ffcf7a" opacity=".06"/><circle cx="119" cy="264" r="3" fill="#ffe08a"/></g>`;
  }
  function lotMarks(zoneName, price, owner) {
    const grid = [220, 260, 300, 340, 380, 420, 460].map(x => `<line x1="${x}" y1="180" x2="${x}" y2="338"/>`).join('') +
      [200, 240, 280, 320].map(y => `<line x1="200" y1="${y}" x2="480" y2="${y}"/>`).join('');
    const sale = owner
      ? `<g data-part="sold"><rect x="490" y="268" width="100" height="34" rx="3" fill="#13203a" stroke="${owner.color}" stroke-width="1.2"/>${T(540, 282, 11, owner.color, 'Sold')}${T(540, 296, 11, '#cbd5e1', `Seat ${owner.seat} · ${owner.name}`.slice(0, 18))}</g>`
      : `<g data-part="sale"><rect x="496" y="268" width="88" height="34" rx="3" fill="#13203a" stroke="#ff9f43" stroke-width="1.2"/>${T(540, 282, 11, '#ffb070', 'For sale')}${T(540, 296, 11, '#cbd5e1', `${price} tTARI`)}</g>`;
    return `<g class="fade" data-part="lot">${R(196, 338, 288, 12, '#1a1612')}` +
      `<g stroke="#4dd0e1" stroke-opacity=".12">${grid}</g>` +
      `<g stroke="#3a4560"><line x1="196" y1="318" x2="196" y2="346"/><line x1="484" y1="318" x2="484" y2="346"/><line x1="196" y1="322" x2="236" y2="322"/><line x1="444" y1="322" x2="484" y2="322"/><line x1="196" y1="334" x2="236" y2="334"/><line x1="444" y1="334" x2="484" y2="334"/><path d="M196 322 l8 12 l8 -12 l8 12 l8 -12 l8 12 M444 322 l8 12 l8 -12 l8 12 l8 -12 l8 12" fill="none"/></g>` +
      `${R(203, 306, 2, 34, '#4a5570')}<path d="M205 306 l9 3 l-9 3 z" fill="#ff9f43"/>${R(475, 306, 2, 34, '#4a5570')}<path d="M477 306 l9 3 l-9 3 z" fill="#ff9f43"/>` +
      `${R(512, 296, 3, 54, '#4a5570')}${sale}` +
      `${R(150, 314, 2, 36, '#4a5570')}<rect x="104" y="296" width="94" height="22" rx="3" fill="#13203a" stroke="#4dd0e1"/>${T(151, 311, 11, '#9ff3ff', `${zoneName} lot`)}</g>`;
  }

  // ---------------- the three approved restaurant drawings ----------------
  const RESTAURANT = {
    'copper-kettle': {
      b: `<rect x="250" y="260" width="180" height="90" fill="#8a3b26"/><g fill="#6e2d1c"><rect x="250" y="276" width="180" height="2"/><rect x="250" y="306" width="6" height="2"/><rect x="424" y="306" width="6" height="2"/><rect x="250" y="336" width="16" height="2"/><rect x="420" y="336" width="10" height="2"/></g><rect x="244" y="252" width="192" height="10" fill="#4a1f14"/>
<rect x="268" y="265" width="144" height="20" rx="4" fill="#2a120b"/><text x="340" y="279" text-anchor="middle" font-size="12" font-weight="500" fill="#ffb070">The Copper Kettle</text>
<rect x="256" y="290" width="168" height="12" fill="#d9822b"/><g fill="#a85a1f"><rect x="270" y="290" width="14" height="12"/><rect x="298" y="290" width="14" height="12"/><rect x="326" y="290" width="14" height="12"/><rect x="354" y="290" width="14" height="12"/><rect x="382" y="290" width="14" height="12"/><rect x="410" y="290" width="14" height="12"/></g>
<g fill="#d9822b"><circle cx="263" cy="302" r="7"/><circle cx="291" cy="302" r="7"/><circle cx="319" cy="302" r="7"/><circle cx="347" cy="302" r="7"/><circle cx="375" cy="302" r="7"/><circle cx="403" cy="302" r="7"/></g>
<rect class="win" x="266" y="312" width="70" height="36"/><rect x="350" y="308" width="26" height="42" fill="#3a170e"/><rect class="win" x="386" y="312" width="34" height="36"/><g fill="#4a1f14"><rect x="300" y="312" width="2" height="36"/><rect x="266" y="328" width="70" height="2"/></g>`,
      e2: `<rect x="250" y="190" width="180" height="62" fill="#7a3322"/><rect x="244" y="182" width="192" height="9" fill="#4a1f14"/>
<rect class="win" x="264" y="200" width="28" height="32"/><rect class="win" x="304" y="200" width="28" height="32"/><rect class="win" x="348" y="200" width="28" height="32"/><rect class="win" x="388" y="200" width="28" height="32"/>
<g fill="#2f7d5b"><circle cx="262" cy="176" r="7"/><circle cx="276" cy="174" r="9"/><circle cx="404" cy="175" r="8"/></g>
<rect x="436" y="340" width="100" height="10" fill="#7a5236"/><rect x="532" y="288" width="3" height="52" fill="#5e3e28"/>
<path class="keep" d="M430 284 Q482 306 533 290" fill="none" stroke="#6b7590"/>
<g class="bulb" fill="#ffe08a"><circle cx="442" cy="289" r="2"/><circle cx="456" cy="294" r="2"/><circle cx="470" cy="297" r="2"/><circle cx="484" cy="298" r="2"/><circle cx="498" cy="297" r="2"/><circle cx="512" cy="294" r="2"/></g>
<path d="M452 316 L466 306 L480 316 Z" fill="#d9822b"/><rect x="465" y="316" width="2" height="24" fill="#5e3e28"/><path d="M496 316 L510 306 L524 316 Z" fill="#2f7d5b"/><rect x="509" y="316" width="2" height="24" fill="#5e3e28"/>`,
      e3: `<rect x="198" y="140" width="52" height="210" fill="#6e2d1c"/><rect x="194" y="132" width="60" height="10" fill="#4a1f14"/><path d="M200 132 L224 102 L248 132 Z" fill="#d9822b"/><rect x="223" y="86" width="2" height="18" fill="#d9822b"/>
<rect x="210" y="158" width="28" height="150" rx="3" fill="#2a120b"/><g fill="#ffb070" font-size="16" font-weight="500" text-anchor="middle"><text x="224" y="182">G</text><text x="224" y="208">R</text><text x="224" y="234">A</text><text x="224" y="260">N</text><text x="224" y="286">D</text></g>
<rect class="win" x="204" y="316" width="40" height="28"/>
<path d="M300 184 C300 150 380 150 380 184 Z" fill="#d9822b"/><circle cx="340" cy="140" r="3" fill="#ffb070"/><rect x="339" y="140" width="2" height="12" fill="#a85a1f"/>`,
      beam: '#ffe9b0', title: '#e8894a',
      lines: ['Silhouette of the Copper Kettle on its lot.', 'Doors open: the original shop.', 'Patio and upper floor added.', 'The grand flagship.'],
    },
    'pirate-cafe': {
      b: `<path d="M246 350 L254 262 H426 L434 350 Z" fill="#1f4e5a"/><g fill="#163a44"><rect x="252" y="284" width="176" height="2"/><rect x="250" y="306" width="180" height="2"/><rect x="248" y="328" width="184" height="2"/></g>
<path d="M426 270 L470 252 L472 256 L430 276 Z" fill="#6b4a2a"/>
<rect x="336" y="170" width="4" height="92" fill="#6b4a2a"/><path d="M340 176 L372 184 L340 194 Z" fill="#d13b3b"/><rect x="318" y="206" width="40" height="3" fill="#6b4a2a"/>
<rect x="276" y="242" width="128" height="18" rx="3" fill="#2b1d10"/><text x="340" y="255" text-anchor="middle" font-size="12" font-weight="500" fill="#ffd36b">The Pirate Café</text>
<circle cx="282" cy="300" r="12" fill="#d4a84b"/><circle cx="318" cy="300" r="12" fill="#d4a84b"/><circle cx="398" cy="300" r="12" fill="#d4a84b"/>
<circle class="win" cx="282" cy="300" r="9"/><circle class="win" cx="318" cy="300" r="9"/><circle class="win" cx="398" cy="300" r="9"/>
<rect x="346" y="300" width="26" height="50" rx="12" fill="#0f2a31"/>
<path class="keep" d="M254 262 Q290 270 326 262 Q362 270 398 262 Q414 266 426 262" fill="none" stroke="#d4a84b" stroke-width="2"/>
<rect x="246" y="344" width="188" height="6" fill="#d4a84b"/>`,
      e2: `<rect x="258" y="222" width="76" height="40" fill="#1f4e5a"/><rect x="254" y="216" width="84" height="7" fill="#d4a84b"/>
<circle cx="276" cy="242" r="9" fill="#d4a84b"/><circle class="win" cx="276" cy="242" r="6.5"/><circle cx="314" cy="242" r="9" fill="#d4a84b"/><circle class="win" cx="314" cy="242" r="6.5"/>
<path d="M434 350 L500 330 L500 334 L436 352 Z" fill="#6b4a2a"/><rect x="496" y="300" width="3" height="50" fill="#6b4a2a"/>
<g class="bulb" fill="#ffd36b"><circle cx="440" cy="318" r="2"/><circle cx="456" cy="312" r="2"/><circle cx="472" cy="308" r="2"/><circle cx="488" cy="304" r="2"/></g>`,
      e3: `<rect x="452" y="160" width="34" height="190" fill="#1f4e5a"/><g fill="#d4a84b"><rect x="448" y="200" width="42" height="4"/><rect x="448" y="260" width="42" height="4"/><rect x="448" y="320" width="42" height="4"/></g>
<rect x="446" y="140" width="46" height="22" fill="#2b1d10"/><rect class="win" x="452" y="144" width="34" height="14"/><path d="M444 140 L469 118 L494 140 Z" fill="#d13b3b"/>
<rect x="294" y="160" width="3" height="80" fill="#6b4a2a"/><path d="M297 166 L322 172 L297 180 Z" fill="#d13b3b"/>`,
      beam: '#ffe9b0', title: '#ffd36b',
      lines: ['Silhouette of the Pirate Café on its lot.', 'Doors open: the ship-front café.', 'Captain’s cabin and gangplank added.', 'The lighthouse flagship.'],
    },
    'lantern-noodle': {
      b: `<rect x="262" y="282" width="156" height="68" fill="#8e2430"/>
<path d="M236 286 Q340 252 444 286 L428 274 Q340 238 252 274 Z" fill="#2f7d5b"/>
<path d="M290 252 Q340 230 390 252 L380 244 Q340 222 300 244 Z" fill="#3a9a6e"/><rect x="300" y="244" width="80" height="12" fill="#8e2430"/>
<rect x="282" y="288" width="116" height="16" rx="3" fill="#1a0d0f"/><text x="340" y="300" text-anchor="middle" font-size="11" font-weight="500" fill="#ffcf7a">Lantern Noodle House</text>
<rect class="win" x="276" y="312" width="40" height="30"/><rect class="win" x="364" y="312" width="40" height="30"/><rect x="326" y="310" width="28" height="40" fill="#4a1218"/>
<g fill="#c9973a"><rect x="295" y="312" width="2" height="30"/><rect x="276" y="326" width="40" height="2"/><rect x="383" y="312" width="2" height="30"/><rect x="364" y="326" width="40" height="2"/><rect x="326" y="308" width="28" height="3"/></g>
<g class="bulb"><line x1="256" y1="276" x2="256" y2="288" stroke="#c9973a"/><ellipse cx="256" cy="294" rx="6" ry="8" fill="#ff5a3c"/><line x1="424" y1="276" x2="424" y2="288" stroke="#c9973a"/><ellipse cx="424" cy="294" rx="6" ry="8" fill="#ff5a3c"/><ellipse cx="340" cy="270" rx="5" ry="7" fill="#ffb070"/></g>`,
      e2: `<rect x="418" y="296" width="84" height="54" fill="#8e2430"/><path d="M410 300 Q460 280 510 300 L500 290 Q460 270 420 290 Z" fill="#3a9a6e"/>
<rect class="win" x="430" y="310" width="24" height="26"/><rect class="win" x="466" y="310" width="24" height="26"/><g fill="#c9973a"><rect x="441" y="310" width="2" height="26"/><rect x="477" y="310" width="2" height="26"/></g>
<path class="keep" d="M440 272 Q470 286 500 274" fill="none" stroke="#c9973a"/>
<g class="bulb" fill="#ff5a3c"><circle cx="452" cy="279" r="3"/><circle cx="470" cy="282" r="3"/><circle cx="488" cy="279" r="3"/></g>`,
      e3: `<rect x="314" y="176" width="52" height="48" fill="#8e2430"/><path d="M296 182 Q340 160 384 182 L374 172 Q340 150 306 172 Z" fill="#2f7d5b"/>
<rect x="338" y="132" width="4" height="26" fill="#c9973a"/><circle cx="340" cy="130" r="4" fill="#c9973a"/>
<rect class="win" x="326" y="190" width="12" height="22"/><rect class="win" x="342" y="190" width="12" height="22"/>
<g class="bulb"><ellipse cx="300" cy="194" rx="5" ry="7" fill="#ff5a3c"/><ellipse cx="380" cy="194" rx="5" ry="7" fill="#ff5a3c"/></g>`,
      beam: '#ffd0b0', title: '#3a9a6e',
      lines: ['Silhouette of the Lantern Noodle House on its lot.', 'Doors open: the noodle house.', 'Jade-roofed side wing added.', 'The pagoda flagship.'],
    },
  };

  // ---------------- motifs (40 x 40 icons at x, y) ----------------
  function motif(name, x, y, M, A) {
    const L = shade(A, 1.35), D = shade(M, .55);
    const g = body => `<g transform="translate(${x} ${y})">${body}</g>`;
    const m = {
      papers: `<rect x="6" y="10" width="26" height="22" fill="${A}"/><rect x="10" y="6" width="26" height="22" fill="${L}"/><g fill="${D}"><rect x="13" y="10" width="18" height="2"/><rect x="13" y="15" width="12" height="2"/><rect x="13" y="20" width="16" height="2"/></g>`,
      flowers: `<rect x="10" y="24" width="20" height="14" fill="${D}"/><g fill="${M}"><circle cx="14" cy="18" r="5"/><circle cx="26" cy="16" r="5"/><circle cx="20" cy="10" r="5"/></g><g fill="${A}"><circle cx="14" cy="18" r="2"/><circle cx="26" cy="16" r="2"/><circle cx="20" cy="10" r="2"/></g>`,
      cart: `<rect x="4" y="12" width="30" height="16" fill="${M}"/><rect x="2" y="8" width="34" height="5" fill="${A}"/><circle cx="12" cy="32" r="5" fill="${D}"/><circle cx="28" cy="32" r="5" fill="${D}"/>`,
      mug: `<rect x="8" y="12" width="20" height="22" rx="3" fill="${A}"/><path d="M28 16 h5 a5 5 0 0 1 0 10 h-5" fill="none" stroke="${A}" stroke-width="3"/><path class="bulb" d="M14 8 q3 -4 0 -7 M20 8 q3 -4 0 -7" stroke="${L}" fill="none" stroke-width="1.5"/>`,
      scones: `<ellipse cx="20" cy="30" rx="16" ry="5" fill="${A}"/><path d="M8 28 q4 -14 12 -14 q8 0 12 14 z" fill="${L}"/><circle cx="20" cy="22" r="2" fill="${M}"/>`,
      owl: `<ellipse cx="20" cy="22" rx="12" ry="14" fill="${A}"/><circle cx="15" cy="18" r="4" fill="${L}"/><circle cx="25" cy="18" r="4" fill="${L}"/><circle cx="15" cy="18" r="1.6" fill="${D}"/><circle cx="25" cy="18" r="1.6" fill="${D}"/><path d="M18 24 l2 3 l2 -3 z" fill="${D}"/><path d="M8 10 l4 6 M32 10 l-4 6" stroke="${A}" stroke-width="3"/>`,
      produce: `<path d="M4 28 h32 l-4 10 h-24 z" fill="${D}"/><g><circle cx="12" cy="24" r="5" fill="${A}"/><circle cx="21" cy="22" r="5" fill="${M}"/><circle cx="29" cy="24" r="5" fill="${L}"/></g>`,
      fish: `<path d="M4 20 q14 -12 26 0 q-12 12 -26 0 z" fill="${A}"/><path d="M30 20 l8 -7 v14 z" fill="${A}"/><circle cx="11" cy="18" r="1.6" fill="${D}"/>`,
      spice: `<path d="M6 34 l4 -14 h6 l4 14 z" fill="${A}"/><path d="M20 34 l4 -18 h6 l4 18 z" fill="${M}"/><path d="M8 20 q5 -8 10 0 z" fill="${L}"/><path d="M22 16 q5 -8 10 0 z" fill="${A}"/>`,
      oven: `<path d="M4 36 v-14 a16 16 0 0 1 32 0 v14 z" fill="${D}"/><path d="M12 36 v-8 a8 8 0 0 1 16 0 v8 z" fill="#ff8a3d" class="bulb"/>`,
      cake: `<rect x="6" y="22" width="28" height="14" fill="${A}"/><rect x="10" y="12" width="20" height="10" fill="${L}"/><rect x="19" y="4" width="2" height="8" fill="${M}"/><circle cx="20" cy="4" r="2" fill="#ffcf7a" class="bulb"/>`,
      waterwheel: `<circle cx="20" cy="20" r="15" fill="none" stroke="${A}" stroke-width="3"/><path d="M20 5 v30 M5 20 h30 M9 9 l22 22 M31 9 l-22 22" stroke="${A}" stroke-width="2"/><circle cx="20" cy="20" r="3" fill="${L}"/>`,
      crates: `<rect x="4" y="20" width="16" height="16" fill="${A}"/><rect x="20" y="20" width="16" height="16" fill="${M}"/><rect x="12" y="6" width="16" height="14" fill="${L}"/><path d="M4 20 l16 16 M20 20 l16 16" stroke="${D}"/>`,
      tools: `<path d="M8 34 l18 -18" stroke="${A}" stroke-width="4"/><rect x="22" y="6" width="14" height="8" fill="${L}" transform="rotate(45 29 10)"/><path d="M30 34 l-16 -18" stroke="${M}" stroke-width="3"/><circle cx="12" cy="13" r="5" fill="none" stroke="${M}" stroke-width="3"/>`,
      toys: `<rect x="6" y="20" width="14" height="14" fill="${A}"/><rect x="20" y="24" width="14" height="10" fill="${M}"/><circle cx="27" cy="14" r="7" fill="${L}"/><path d="M13 20 v-8" stroke="${D}" stroke-width="2"/>`,
      books: `<rect x="6" y="10" width="7" height="26" fill="${A}"/><rect x="14" y="6" width="7" height="30" fill="${L}"/><rect x="22" y="12" width="6" height="24" fill="${M}"/><rect x="28" y="9" width="6" height="27" fill="${A}" transform="rotate(8 31 22)"/>`,
      inkwell: `<path d="M8 36 h24 l-4 -14 h-16 z" fill="${D}"/><rect x="14" y="16" width="12" height="6" fill="${A}"/><path d="M26 2 l-6 20" stroke="${L}" stroke-width="2.5"/><path d="M26 2 q8 8 0 12" fill="${L}"/>`,
      globe: `<circle cx="20" cy="18" r="13" fill="${A}"/><path d="M12 12 q6 4 4 10 q6 2 10 -4" fill="none" stroke="${D}" stroke-width="2"/><path d="M20 31 v5 M12 37 h16" stroke="${L}" stroke-width="2.5"/>`,
      icecream: `<path d="M12 18 l8 20 l8 -20 z" fill="${A}"/><circle cx="14" cy="15" r="6" fill="${M}"/><circle cx="26" cy="15" r="6" fill="${L}"/><circle cx="20" cy="9" r="6" fill="${shade(M, 1.3)}"/>`,
      candy: `<circle cx="20" cy="20" r="10" fill="${M}"/><path d="M20 10 a10 10 0 0 1 0 20 a5 5 0 0 1 0 -10 a5 5 0 0 0 0 -10" fill="${L}"/><path d="M10 20 l-8 -6 v12 z M30 20 l8 -6 v12 z" fill="${A}"/>`,
      chocolate: `<rect x="6" y="8" width="28" height="26" rx="2" fill="${D}"/><g fill="${M}"><rect x="9" y="11" width="10" height="9"/><rect x="21" y="11" width="10" height="9"/><rect x="9" y="22" width="10" height="9"/><rect x="21" y="22" width="10" height="9"/></g><path d="M26 34 l8 -8 v8 z" fill="${A}"/>`,
      kettle: `<path d="M8 34 q0 -18 12 -18 q12 0 12 18 z" fill="${A}"/><path d="M32 26 l6 -6" stroke="${A}" stroke-width="3"/><rect x="17" y="12" width="6" height="4" fill="${L}"/>`,
      pirate: `<path d="M4 28 h32 l-6 8 h-20 z" fill="${M}"/><rect x="19" y="4" width="2" height="24" fill="${D}"/><path d="M21 6 l12 6 l-12 6 z" fill="${A}"/>`,
      lantern: `<ellipse cx="20" cy="22" rx="10" ry="13" fill="${M}" class="bulb"/><rect x="15" y="7" width="10" height="3" fill="${A}"/><rect x="15" y="34" width="10" height="3" fill="${A}"/>`,
      mortar: `<path d="M6 20 h28 q0 16 -14 16 q-14 0 -14 -16 z" fill="${A}"/><path d="M24 22 l10 -18" stroke="${L}" stroke-width="5" stroke-linecap="round"/>`,
      cross: `<path d="M15 4 h10 v11 h11 v10 h-11 v11 h-10 v-11 h-11 v-10 h11 z" fill="${A}" class="bulb"/>`,
      herbs: `<rect x="10" y="26" width="20" height="12" fill="${D}"/><path d="M20 26 v-18 M20 18 q-8 -2 -10 -10 M20 14 q8 -2 10 -10 M20 22 q-6 0 -10 -6 M20 20 q6 0 10 -6" stroke="${A}" stroke-width="2.5" fill="none"/>`,
      marquee: `<rect x="2" y="10" width="36" height="20" fill="${D}" stroke="${A}" stroke-width="2"/><g class="bulb" fill="#ffe08a">${[6, 12, 18, 24, 30, 36].map(cx => `<circle cx="${cx - 2}" cy="8" r="1.6"/><circle cx="${cx - 2}" cy="32" r="1.6"/>`).join('')}</g><rect x="8" y="16" width="24" height="8" fill="${A}"/>`,
      neon: `<path d="M6 30 v-20 l14 20 v-20 M26 10 h10 M31 10 v20" stroke="${A}" stroke-width="3" fill="none" class="bulb" style="filter:drop-shadow(0 0 3px ${A})"/>`,
      deco: `<path d="M20 2 l14 14 v22 h-28 v-22 z" fill="${D}"/><path d="M20 8 v28 M12 16 v20 M28 16 v20" stroke="${A}" stroke-width="2"/>`,
      timber: `<rect x="2" y="26" width="36" height="8" rx="4" fill="${A}"/><rect x="4" y="16" width="32" height="8" rx="4" fill="${L}"/><rect x="8" y="6" width="24" height="8" rx="4" fill="${A}"/><circle cx="35" cy="30" r="3" fill="${D}"/>`,
      sparks: `<path d="M22 2 l-12 20 h8 l-4 16 l14 -22 h-8 z" fill="${A}" class="bulb"/>`,
      kiln: `<path d="M6 38 v-16 q14 -24 28 0 v16 z" fill="${D}"/><path d="M14 38 v-8 a6 6 0 0 1 12 0 v8 z" fill="#ff8a3d" class="bulb"/><rect x="18" y="2" width="4" height="10" fill="${A}"/>`,
      hoist: `<rect x="4" y="4" width="32" height="4" fill="${A}"/><path d="M30 8 v18" stroke="${L}" stroke-width="1.5"/><rect x="22" y="26" width="16" height="12" fill="${M}"/>`,
      ice: `<path d="M20 2 v36 M4 11 l32 18 M4 29 l32 -18" stroke="${A}" stroke-width="3"/><circle cx="20" cy="20" r="4" fill="${L}"/>`,
      containers: `<rect x="2" y="22" width="18" height="14" fill="${A}"/><rect x="20" y="22" width="18" height="14" fill="${M}"/><rect x="10" y="8" width="20" height="14" fill="${L}"/>`,
      tram: `<rect x="4" y="10" width="32" height="20" rx="4" fill="${A}"/><g fill="${D}"><rect x="8" y="14" width="7" height="7"/><rect x="17" y="14" width="7" height="7"/><rect x="26" y="14" width="7" height="7"/></g><path d="M20 10 v-8 M14 2 h12" stroke="${L}" stroke-width="2"/>`,
      bus: `<rect x="2" y="10" width="36" height="20" rx="3" fill="${A}"/><g fill="${D}"><rect x="6" y="14" width="7" height="7"/><rect x="15" y="14" width="7" height="7"/><rect x="24" y="14" width="7" height="7"/></g><circle cx="10" cy="31" r="4" fill="${D}"/><circle cx="30" cy="31" r="4" fill="${D}"/>`,
      rail: `<rect x="6" y="14" width="20" height="14" fill="${A}"/><rect x="24" y="8" width="12" height="20" fill="${M}"/><rect x="9" y="6" width="5" height="8" fill="${A}"/><circle cx="12" cy="31" r="4" fill="${D}"/><circle cx="28" cy="31" r="5" fill="${D}"/>`,
      lifts: `<rect x="4" y="4" width="4" height="34" fill="${A}"/><rect x="32" y="4" width="4" height="34" fill="${A}"/><rect x="4" y="22" width="32" height="4" fill="${L}"/><rect x="8" y="12" width="24" height="10" rx="4" fill="${M}"/>`,
      showroom: `<path d="M4 30 l4 -10 l8 -6 h10 l6 6 l4 10 z" fill="${A}"/><circle cx="12" cy="31" r="4" fill="${D}"/><circle cx="29" cy="31" r="4" fill="${D}"/>`,
      scrap: `<path d="M2 38 l8 -16 l6 6 l8 -14 l6 10 l8 -6 v20 z" fill="${A}"/><circle cx="14" cy="30" r="4" fill="none" stroke="${L}" stroke-width="2"/>`,
      anvil: `<path d="M4 14 h28 q4 0 6 -4 v8 q-4 4 -10 4 h-4 v6 h6 v8 h-24 v-8 h6 v-6 q-8 0 -8 -8 z" fill="${A}"/><g class="bulb" fill="#ffb070"><circle cx="34" cy="6" r="1.5"/><circle cx="28" cy="4" r="1.2"/></g>`,
      vats: `<rect x="4" y="12" width="14" height="24" rx="3" fill="${A}"/><rect x="22" y="8" width="14" height="28" rx="3" fill="${M}"/><path d="M18 18 h4" stroke="${L}" stroke-width="3"/><circle cx="11" cy="8" r="2" fill="${L}" class="bulb"/>`,
      bell: `<path d="M8 32 q0 -24 12 -24 q12 0 12 24 z" fill="${A}"/><rect x="5" y="32" width="30" height="3" fill="${L}"/><circle cx="20" cy="37" r="3" fill="${L}"/><rect x="18" y="4" width="4" height="5" fill="${L}"/>`,
      barrels: `<ellipse cx="12" cy="26" rx="9" ry="11" fill="${A}"/><ellipse cx="28" cy="26" rx="9" ry="11" fill="${M}"/><path d="M3 22 h18 M3 30 h18 M19 22 h18 M19 30 h18" stroke="${D}" stroke-width="1.5"/>`,
      apples: `<circle cx="13" cy="24" r="9" fill="${M}"/><circle cx="27" cy="24" r="9" fill="${A}"/><path d="M13 15 v-5 M27 15 v-5" stroke="${D}" stroke-width="2"/><path d="M27 10 q6 -4 8 0" fill="${L}"/>`,
      soda: `<rect x="12" y="10" width="16" height="28" rx="4" fill="${A}"/><rect x="15" y="4" width="10" height="6" fill="${L}"/><rect x="12" y="20" width="16" height="8" fill="${M}"/><g class="bulb" fill="${L}"><circle cx="17" cy="16" r="1.4"/><circle cx="22" cy="13" r="1.2"/></g>`,
      sailboats: `<path d="M2 32 h16 l-3 5 h-10 z M22 32 h16 l-3 5 h-10 z" fill="${M}"/><path d="M10 6 v24 h8 z M30 10 v20 h7 z" fill="${A}"/>`,
      lighthouse: `<path d="M14 38 l3 -28 h6 l3 28 z" fill="${L}"/><rect x="15" y="20" width="10" height="5" fill="${M}"/><rect x="15" y="4" width="10" height="6" fill="#ffe08a" class="bulb"/><path d="M14 4 l6 -4 l6 4 z" fill="${M}"/>`,
      pennants: `<path d="M2 6 q18 8 36 0" stroke="${L}" fill="none"/><g>${[4, 12, 20, 28].map((px, i) => `<path d="M${px} ${7 + (i % 2) * 2} l4 10 l4 -10" fill="${i % 2 ? A : M}"/>`).join('')}</g><rect x="18" y="20" width="4" height="18" fill="${D}"/>`,
      glass: `<path d="M6 36 l6 -28 h16 l6 28 z" fill="${A}" opacity=".85"/><path d="M12 8 l-2 28 M20 8 v28 M28 8 l2 28" stroke="${L}" stroke-width="1.5"/>`,
      easel: `<path d="M10 38 l10 -36 l10 36 M20 2 v36" stroke="${L}" stroke-width="2"/><rect x="8" y="8" width="24" height="18" fill="${A}"/><circle cx="16" cy="16" r="3" fill="${M}"/><path d="M10 24 l8 -6 l6 4 l6 -6 v8 z" fill="${D}"/>`,
      lightbeam: `<path d="M20 38 l-14 -34 h28 z" fill="${A}" opacity=".5" class="bulb"/><rect x="14" y="30" width="12" height="8" fill="${M}"/>`,
      curtains: `<rect x="2" y="2" width="36" height="5" fill="${A}"/><path d="M2 7 h17 q-4 18 -6 31 h-11 z M38 7 h-17 q4 18 6 31 h11 z" fill="${M}"/>`,
      puppets: `<path d="M8 2 v10 M20 2 v8 M32 2 v10" stroke="${L}"/><circle cx="20" cy="16" r="6" fill="${A}"/><path d="M12 22 h16 l-3 14 h-10 z" fill="${M}"/><path d="M8 12 l6 12 M32 12 l-6 12" stroke="${L}"/>`,
      dome: `<path d="M4 30 a16 16 0 0 1 32 0 z" fill="${A}"/><rect x="2" y="30" width="36" height="6" fill="${M}"/><rect x="19" y="6" width="2" height="8" fill="${L}"/>`,
      steampool: `<path d="M4 30 q16 10 32 0 v6 h-32 z" fill="${A}"/><path class="bulb" d="M12 24 q-4 -6 0 -12 q4 -6 0 -10 M20 24 q-4 -6 0 -12 q4 -6 0 -10 M28 24 q-4 -6 0 -12 q4 -6 0 -10" stroke="${L}" fill="none" stroke-width="2"/>`,
      salt: `<path d="M6 36 l14 -30 l14 30 z" fill="${L}"/><path d="M12 36 l8 -18 l8 18 z" fill="${A}"/>`,
      lotus: `<path d="M20 34 q-14 -6 -14 -20 q10 4 14 20 q4 -16 14 -20 q0 14 -14 20 z" fill="${A}"/><path d="M20 34 q-4 -14 0 -26 q4 12 0 26 z" fill="${L}"/>`,
      fireside: `<path d="M4 38 v-22 h32 v22 z" fill="${D}"/><path d="M12 38 v-12 h16 v12 z" fill="#2a120b"/><path d="M15 38 q-2 -6 5 -10 q-1 4 4 6 q2 2 0 4 z" fill="#ff8a3d" class="bulb"/>`,
      seaside: `<path d="M2 30 q6 -4 12 0 t12 0 t12 0" stroke="${A}" fill="none" stroke-width="3"/><circle cx="28" cy="12" r="7" fill="${L}"/><path d="M4 20 l10 -10 l2 2 l-10 10 z" fill="${M}"/>`,
      chimney: `<path d="M8 38 v-20 l12 -10 l12 10 v20 z" fill="${M}"/><path d="M24 10 l2 -8 h6 l-1 12" fill="${A}" transform="rotate(-8 28 8)"/><path class="bulb" d="M30 0 q4 -4 8 0" stroke="${L}" fill="none"/>`,
      goldtower: `<rect x="12" y="8" width="16" height="30" fill="${A}"/><path d="M12 8 l8 -8 l8 8 z" fill="${L}"/><g fill="${D}"><rect x="15" y="12" width="4" height="4"/><rect x="21" y="12" width="4" height="4"/><rect x="15" y="20" width="4" height="4"/><rect x="21" y="20" width="4" height="4"/></g>`,
      glassdome: `<path d="M4 34 a16 18 0 0 1 32 0 z" fill="${A}" opacity=".8"/><path d="M20 16 v18 M10 22 l10 12 M30 22 l-10 12" stroke="${L}" stroke-width="1.5"/>`,
      aurora: `<path d="M2 20 q10 -14 18 -4 t18 -6 v8 q-10 6 -18 2 t-18 6 z" fill="${A}" opacity=".85" class="bulb"/><path d="M2 30 q10 -10 18 -2 t18 -4" stroke="${L}" fill="none" stroke-width="2"/>`,
    };
    return g(m[name] || `<circle cx="20" cy="20" r="12" fill="${A}"/>`);
  }

  // ---------------- zone shells: {b, e2, e3}; the motif sits at the shell's slot ----------------
  function flagTower(x, b, D, short) {
    const letters = String(short || '').toUpperCase().slice(0, 7).split('');
    const top = 150, sp = Math.min(26, 124 / Math.max(1, letters.length)); // fixed height: the letters compress
    const fs = Math.min(16, Math.round(sp * .7));
    return `${R(x, top, 52, 350 - top, shade(b.M, .8))}${R(x - 4, top - 8, 60, 10, D)}<path d="M${x + 2} ${top - 8} L${x + 26} ${top - 38} L${x + 50} ${top - 8} Z" fill="${b.A}"/>${R(x + 25, top - 54, 2, 18, b.A)}` +
      `<rect x="${x + 12}" y="${top + 8}" width="28" height="${Math.round(letters.length * sp + 8)}" rx="3" fill="${shade(b.M, .3)}"/>` +
      `<g fill="${shade(b.A, 1.3)}" font-size="${fs}" font-weight="500" text-anchor="middle">${letters.map((c, i) => `<text x="${x + 26}" y="${Math.round(top + 8 + sp * (i + 1) - (sp - fs) / 2)}">${esc(c)}</text>`).join('')}</g>` +
      WIN(x + 6, 316, 40, 28);
  }
  function upper(x, y, w, M, D, n) {
    const step = w / n;
    return `${R(x, y, w, 252 - y + 2, shade(M, .88))}${R(x - 6, y - 8, w + 12, 9, D)}` +
      Array.from({length: n}, (_, i) => WIN(Math.round(x + i * step + step / 2 - 12), y + 10, 24, 30)).join('');
  }
  function annex(M, A, D) {
    return `${R(436, 300, 90, 50, shade(M, .9))}${R(432, 294, 98, 7, D)}${WIN(448, 312, 26, 26)}${WIN(486, 312, 26, 26)}` +
      `<path class="keep" d="M436 296 Q480 312 526 296" fill="none" stroke="${A}"/><g class="bulb" fill="#ffe08a">${[446, 462, 478, 494, 510].map(cx => `<circle cx="${cx}" cy="${303 + (cx === 478 ? 3 : 1)}" r="2"/>`).join('')}</g>`;
  }
  const SHELLS = {
    kiosk: b => ({
      b: `${R(290, 282, 100, 68, b.M)}<path d="M282 282 h116 l-8 -18 h-100 z" fill="${b.A}"/>${R(286, 262, 108, 6, b.D)}${R(296, 300, 88, 22, '#0d1320')}${WIN(300, 304, 80, 14)}${R(290, 322, 100, 6, b.A)}<rect x="300" y="244" width="80" height="16" rx="3" fill="${b.T}"/>${T(340, 256, 9, b.L, b.short)}`,
      slot: [346, 296], e2: `${R(390, 296, 54, 54, shade(b.M, .85))}<path d="M386 296 h62 l-6 -12 h-50 z" fill="${b.A}"/>${WIN(402, 306, 30, 22)}${R(236, 318, 54, 32, shade(b.M, .85))}${WIN(244, 324, 38, 16)}`, tower: 198,
    }),
    cafe: b => ({
      b: `${R(260, 262, 160, 88, b.M)}${R(254, 254, 172, 10, b.D)}<rect x="276" y="266" width="128" height="18" rx="3" fill="${b.T}"/>${T(340, 279, 11, b.L, b.name)}` +
        `${R(264, 290, 152, 10, b.A)}<g fill="${b.A}">${[270, 290, 310, 330, 350, 370, 390, 410].map(cx => `<circle cx="${cx}" cy="300" r="6"/>`).join('')}</g>${WIN(270, 312, 60, 36)}${R(342, 308, 24, 42, b.T)}${WIN(376, 312, 36, 36)}`,
      slot: [430, 304], e2: upper(260, 190, 160, b.M, b.D, 4) + annex(b.M, b.A, b.D), tower: 198,
    }),
    market: b => ({
      b: `${R(236, 270, 208, 80, b.M)}<path d="M228 272 Q340 214 452 272 Z" fill="${b.A}"/><path d="M240 272 Q340 226 440 272" fill="none" stroke="${b.D}" stroke-width="3"/>` +
        [252, 316, 380].map(x => `<path d="M${x} 350 v-34 a24 24 0 0 1 48 0 v34 z" fill="${b.T}"/><path class="win" d="M${x + 6} 350 v-30 a18 18 0 0 1 36 0 v30 z"/>`).join('') +
        `<rect x="290" y="276" width="100" height="16" rx="3" fill="${b.T}"/>${T(340, 288, 10, b.L, b.short)}`,
      slot: [320, 236], e2: `${R(140, 300, 96, 50, shade(b.M, .85))}<path d="M134 302 Q188 276 242 302 Z" fill="${b.A}"/>${WIN(152, 314, 30, 24)}${WIN(194, 314, 30, 24)}` + annex(b.M, b.A, b.D), tower: 450,
    }),
    bakery: b => ({
      b: `${R(262, 272, 156, 78, b.M)}<path d="M252 274 L340 222 L428 274 Z" fill="${b.D}"/>${R(392, 226, 16, 34, b.T)}<path class="bulb" d="M400 222 q-6 -10 2 -18 q6 -8 0 -16" stroke="#9aa7bd" fill="none" stroke-width="2"/>` +
        `<path d="M270 350 v-40 h56 v40" fill="${b.T}"/><path class="win" d="M274 346 v-32 h48 v32 z"/>${R(346, 306, 26, 44, b.T)}${WIN(382, 312, 28, 30)}<rect x="296" y="278" width="88" height="18" rx="3" fill="${b.T}"/>${T(340, 291, 10, b.L, b.short)}`,
      slot: [320, 234], e2: `${R(418, 292, 90, 58, shade(b.M, .88))}<path d="M412 294 L463 262 L514 294 Z" fill="${b.D}"/>${WIN(430, 306, 26, 30)}${WIN(470, 306, 26, 30)}${R(176, 300, 86, 50, shade(b.M, .85))}<path d="M170 302 l46 -26 l46 26 z" fill="${b.A}"/>${WIN(196, 312, 44, 26)}`, tower: 520,
    }),
    shop: b => ({
      b: `${R(262, 262, 156, 88, b.M)}${R(256, 254, 168, 10, b.D)}<rect x="278" y="266" width="124" height="18" rx="3" fill="${b.T}"/>${T(340, 279, 11, b.L, b.name)}` +
        `<path d="M262 292 h156 l-10 14 h-136 z" fill="${b.A}"/>${WIN(272, 310, 54, 38)}${R(334, 308, 24, 42, b.T)}${WIN(366, 310, 46, 38)}`,
      slot: [430, 300], e2: upper(262, 196, 156, b.M, b.D, 4) + annex(b.M, b.A, b.D), tower: 198,
    }),
    bookshop: b => ({
      b: `${R(258, 268, 164, 82, b.M)}<path d="M252 270 L268 240 H412 L428 270 Z" fill="${b.D}"/>${WIN(284, 248, 18, 16)}${WIN(330, 248, 18, 16)}${WIN(376, 248, 18, 16)}` +
        `<rect x="280" y="274" width="120" height="16" rx="3" fill="${b.T}"/>${T(340, 286, 10, b.L, b.name)}` +
        `${WIN(270, 300, 56, 44)}<g stroke="${b.T}" stroke-width="2">${[284, 298, 312].map(x => `<line x1="${x}" y1="300" x2="${x}" y2="344"/>`).join('')}<line x1="270" y1="322" x2="326" y2="322"/></g>${R(336, 302, 26, 48, b.T)}${WIN(372, 300, 40, 44)}`,
      slot: [430, 304], e2: upper(258, 178, 164, b.M, b.D, 4) + annex(b.M, b.A, b.D), tower: 198,
    }),
    sweets: b => ({
      b: `${R(262, 264, 156, 86, b.M)}<g fill="${b.A}">${[262, 290, 318, 346, 374, 402].map(x => R(x, 264, 14, 86, b.A)).join('')}</g>${R(256, 256, 168, 10, b.D)}` +
        `<path d="M260 296 q10 12 20 0 q10 12 20 0 q10 12 20 0 q10 12 20 0 q10 12 20 0 q10 12 20 0 q10 12 20 0 q10 12 20 0 v-10 h-160 z" fill="${b.L}"/>` +
        `<rect x="282" y="268" width="116" height="16" rx="8" fill="${b.T}"/>${T(340, 280, 10, b.L, b.name)}${WIN(272, 312, 56, 36)}${R(336, 308, 24, 42, b.T)}${WIN(368, 312, 44, 36)}`,
      slot: [432, 302], e2: upper(262, 196, 156, b.M, b.D, 4) + annex(b.M, b.A, b.D), tower: 198,
    }),
    pharmacy: b => ({
      b: `${R(258, 262, 164, 88, b.M)}${R(252, 254, 176, 10, b.D)}<rect x="276" y="266" width="128" height="18" rx="3" fill="${b.T}"/>${T(340, 279, 10, b.L, b.name)}` +
        `${WIN(268, 296, 60, 50)}${R(338, 300, 26, 50, b.T)}${WIN(374, 296, 38, 50)}${R(258, 290, 164, 4, b.A)}`,
      slot: [430, 270], e2: upper(258, 194, 164, b.M, b.D, 4) + annex(b.M, b.A, b.D), tower: 198,
    }),
    cinema: b => ({
      b: `${R(250, 238, 180, 112, b.M)}${R(244, 230, 192, 10, b.D)}<rect x="236" y="276" width="208" height="30" fill="${b.T}" stroke="${b.A}" stroke-width="2"/>` +
        `<g class="bulb" fill="#ffe08a">${Array.from({length: 17}, (_, i) => `<circle cx="${242 + i * 12.25}" cy="273" r="2"/><circle cx="${242 + i * 12.25}" cy="309" r="2"/>`).join('')}</g>${T(340, 296, 14, b.L, b.name)}` +
        `${R(268, 316, 36, 34, b.T)}${WIN(272, 320, 28, 26)}${R(322, 316, 36, 34, b.T)}${R(376, 316, 36, 34, b.T)}${WIN(380, 320, 28, 26)}${WIN(266, 248, 148, 18)}`,
      slot: [320, 190], e2: upper(250, 180, 180, b.M, b.D, 5) + annex(b.M, b.A, b.D), tower: 182,
    }),
    workshop: b => ({
      b: `${R(246, 272, 188, 78, b.M)}<path d="M246 272 l30 -26 v26 l30 -26 v26 l30 -26 v26 l30 -26 v26 l30 -26 v26 l18 -16 v16 z" fill="${b.D}"/>` +
        `${R(270, 296, 76, 54, b.T)}<g stroke="${shade(b.M, .4)}">${[304, 312, 320, 328, 336, 344].map(y => `<line x1="270" y1="${y}" x2="346" y2="${y}"/>`).join('')}</g>${WIN(360, 300, 60, 30)}` +
        `<rect x="356" y="280" width="68" height="14" rx="3" fill="${b.T}"/>${T(390, 291, 9, b.L, b.short)}`,
      slot: [372, 232], e2: `${R(140, 290, 106, 60, shade(b.M, .85))}<path d="M140 290 l26 -20 v20 l26 -20 v20 l26 -20 v20 l28 -20 v20 z" fill="${b.D}"/>${WIN(156, 304, 32, 26)}${WIN(200, 304, 32, 26)}` + annex(b.M, b.A, b.D), tower: 540,
    }),
    warehouse: b => ({
      b: `${R(236, 262, 208, 88, b.M)}<path d="M228 264 L340 212 L452 264 Z" fill="${b.D}"/>${R(330, 220, 46, 5, b.A)}<path d="M372 225 v24" stroke="#9aa7bd"/>${R(364, 249, 16, 10, b.A)}` +
        `${R(260, 290, 56, 60, b.T)}${R(364, 290, 56, 60, b.T)}<g stroke="${shade(b.M, .4)}"><line x1="288" y1="290" x2="288" y2="350"/><line x1="392" y1="290" x2="392" y2="350"/></g>${WIN(324, 296, 32, 22)}` +
        `<rect x="290" y="268" width="100" height="16" rx="3" fill="${b.T}"/>${T(340, 280, 9, b.L, b.short)}`,
      slot: [320, 226], e2: `${R(140, 284, 96, 66, shade(b.M, .85))}<path d="M134 286 l54 -24 l54 24 z" fill="${b.D}"/>${R(160, 300, 56, 50, b.T)}` + annex(b.M, b.A, b.D), tower: 540,
    }),
    depot: b => ({
      b: `${R(236, 274, 208, 76, b.M)}<path d="M228 276 Q340 210 452 276 Z" fill="${b.D}"/>` +
        [256, 352].map(x => `<path d="M${x} 350 v-50 a36 30 0 0 1 72 0 v50 z" fill="${b.T}"/>${WIN(x + 10, 304, 52, 16)}`).join('') +
        `<rect x="296" y="278" width="88" height="14" rx="3" fill="${b.T}"/>${T(340, 289, 9, b.L, b.short)}`,
      slot: [320, 224], e2: `${R(140, 292, 96, 58, shade(b.M, .85))}<path d="M134 294 Q188 262 242 294 Z" fill="${b.D}"/>${WIN(154, 306, 68, 18)}` + annex(b.M, b.A, b.D), tower: 540,
    }),
    garage: b => ({
      b: `${R(246, 266, 188, 84, b.M)}${R(240, 258, 200, 10, b.D)}<rect x="270" y="270" width="140" height="18" rx="3" fill="${b.T}"/>${T(340, 283, 10, b.L, b.name)}` +
        [258, 342].map(x => `${R(x, 296, 72, 54, b.T)}<g stroke="${shade(b.M, .45)}">${[304, 312, 320, 328, 336, 344].map(y => `<line x1="${x}" y1="${y}" x2="${x + 72}" y2="${y}"/>`).join('')}</g>`).join('') + R(246, 292, 188, 3, b.A),
      slot: [430, 300], e2: upper(246, 200, 188, b.M, b.D, 5) + annex(b.M, b.A, b.D), tower: 190,
    }),
    foundry: b => ({
      b: `${R(246, 262, 188, 88, b.M)}<path d="M240 264 L340 236 L440 264 Z" fill="${b.D}"/>${R(270, 180, 20, 70, b.T)}${R(390, 196, 20, 56, b.T)}${R(266, 176, 28, 6, b.A)}${R(386, 192, 28, 6, b.A)}` +
        `<path class="bulb" d="M280 172 q-8 -12 2 -22 q8 -10 0 -18 M400 188 q-8 -12 2 -22" stroke="#6b7590" fill="none" stroke-width="3"/>` +
        `${R(300, 292, 80, 58, b.T)}<path d="M308 350 v-40 a32 20 0 0 1 64 0 v40 z" fill="#ff8a3d" class="bulb" opacity=".75"/>${WIN(256, 296, 34, 30)}${WIN(390, 296, 34, 30)}` +
        `<rect x="294" y="268" width="92" height="16" rx="3" fill="${b.T}"/>${T(340, 280, 9, b.L, b.short)}`,
      slot: [320, 210], e2: `${R(140, 290, 106, 60, shade(b.M, .85))}<path d="M134 292 l60 -20 l58 20 z" fill="${b.D}"/>${R(160, 230, 16, 60, b.T)}${WIN(186, 306, 40, 26)}` + annex(b.M, b.A, b.D), tower: 540,
    }),
    brewery: b => ({
      b: `${R(246, 268, 150, 82, b.M)}<path d="M238 270 L321 226 L404 270 Z" fill="${b.D}"/>${R(400, 220, 44, 130, b.A)}<ellipse cx="422" cy="220" rx="22" ry="8" fill="${b.L}"/><g stroke="${b.D}">${[250, 280, 310].map(y => `<line x1="400" y1="${y}" x2="444" y2="${y}"/>`).join('')}</g>` +
        `${R(270, 300, 50, 50, b.T)}${WIN(330, 302, 50, 30)}<rect x="270" y="276" width="104" height="16" rx="3" fill="${b.T}"/>${T(322, 288, 9, b.L, b.short)}`,
      slot: [300, 232], e2: `${R(140, 290, 106, 60, shade(b.M, .85))}<path d="M134 292 l58 -26 l58 26 z" fill="${b.D}"/>${WIN(156, 304, 32, 26)}${WIN(200, 304, 32, 26)}${R(450, 250, 36, 100, b.A)}<ellipse cx="468" cy="250" rx="18" ry="7" fill="${b.L}"/>`, tower: 540,
    }),
    marina: b => ({
      b: `${R(236, 340, 208, 6, b.D)}${[244, 290, 336, 382, 428].map(x => R(x, 340, 6, 14, b.T)).join('')}${R(256, 272, 168, 68, b.M)}<path d="M246 274 L340 226 L434 274 Z" fill="${b.A}"/>` +
        `${R(300, 290, 80, 50, b.T)}<g stroke="${shade(b.M, .45)}">${[298, 306, 314, 322, 330].map(y => `<line x1="300" y1="${y}" x2="380" y2="${y}"/>`).join('')}</g>${WIN(266, 290, 26, 26)}${WIN(388, 290, 26, 26)}` +
        `<rect x="290" y="252" width="100" height="16" rx="3" fill="${b.T}"/>${T(340, 264, 9, b.L, b.short)}`,
      slot: [450, 286], e2: `${R(446, 340, 110, 5, b.D)}${R(456, 296, 70, 44, shade(b.M, .85))}<path d="M450 298 l41 -22 l41 22 z" fill="${b.A}"/>${WIN(468, 306, 46, 20)}${R(176, 300, 80, 40, shade(b.M, .85))}${WIN(190, 308, 52, 20)}`, tower: 560,
    }),
    gallery: b => ({
      b: `${R(246, 254, 188, 96, b.M)}${R(240, 246, 200, 10, b.D)}${WIN(258, 270, 164, 54)}<g stroke="${b.D}" stroke-width="2">${[299, 340, 381].map(x => `<line x1="${x}" y1="270" x2="${x}" y2="324"/>`).join('')}</g>` +
        `${R(326, 326, 28, 24, b.T)}<rect x="270" y="330" width="44" height="14" rx="3" fill="${b.T}"/>${T(292, 341, 8, b.L, b.short)}${R(246, 324, 188, 2, b.A)}`,
      slot: [380, 300], e2: `${R(270, 196, 140, 50, shade(b.M, .9))}${R(264, 190, 152, 8, b.D)}${WIN(282, 204, 116, 30)}` + annex(b.M, b.A, b.D), tower: 190,
    }),
    theatre: b => ({
      b: `${R(246, 270, 188, 80, b.M)}<path d="M238 272 L340 222 L442 272 Z" fill="${b.D}"/><circle cx="340" cy="254" r="10" fill="${b.A}" class="bulb"/>` +
        `<g fill="${b.L}">${[256, 292, 328, 364, 400].map(x => R(x, 282, 10, 68, b.L)).join('')}</g>${R(246, 270, 188, 8, b.A)}${WIN(270, 296, 18, 40)}${WIN(342, 296, 18, 40)}${WIN(306, 296, 18, 40)}${WIN(378, 296, 18, 40)}${WIN(414, 296, 12, 40)}` +
        ``,
      slot: [450, 300], e2: `${R(140, 288, 106, 62, shade(b.M, .85))}<path d="M134 290 l59 -22 l59 22 z" fill="${b.D}"/>${WIN(156, 302, 74, 30)}${R(434, 296, 80, 54, shade(b.M, .85))}${WIN(446, 306, 56, 30)}`, tower: 540,
    }),
    spa: b => ({
      b: `${R(236, 290, 208, 60, b.M)}<path d="M228 292 Q340 250 452 292 Z" fill="${b.A}"/>${WIN(252, 304, 50, 34)}${R(318, 300, 44, 50, b.T)}${WIN(378, 304, 50, 34)}` +
        `<rect x="290" y="270" width="100" height="16" rx="8" fill="${b.T}"/>${T(340, 282, 9, b.L, b.short)}`,
      slot: [322, 228], e2: `<path d="M276 270 Q340 186 404 270 Z" fill="${shade(b.A, .85)}"/>${WIN(318, 226, 44, 30)}${R(446, 316, 90, 34, '#12304a')}<path d="M446 316 q45 10 90 0" stroke="${b.L}" fill="none"/>`, tower: 186,
    }),
    inn: b => ({
      b: `${R(250, 268, 180, 82, b.M)}<path d="M244 270 L292 226 L340 270 Z M340 270 L388 226 L436 270 Z" fill="${b.D}"/><g stroke="${b.T}" stroke-width="4">${[262, 300, 380, 418].map(x => `<line x1="${x}" y1="268" x2="${x}" y2="350"/>`).join('')}<line x1="250" y1="296" x2="430" y2="296"/></g>` +
        `${WIN(270, 304, 24, 24)}${WIN(386, 304, 24, 24)}${R(326, 300, 28, 50, b.T)}${R(392, 220, 14, 30, b.T)}<rect x="292" y="274" width="96" height="16" rx="3" fill="${b.T}"/>${T(340, 286, 9, b.L, b.short)}`,
      slot: [450, 268], e2: `${R(176, 290, 74, 60, shade(b.M, .85))}<path d="M170 292 l43 -30 l43 30 z" fill="${b.D}"/>${WIN(196, 306, 34, 26)}` + annex(b.M, b.A, b.D), tower: 540,
    }),
    'grand-hotel': b => ({
      b: `${R(266, 206, 148, 144, b.M)}${R(258, 198, 164, 10, b.D)}` +
        [218, 246, 274].map(y => [282, 314, 350, 382].map(x => WIN(x, y, 18, 18)).join('')).join('') +
        `${R(266, 300, 148, 4, b.A)}<path d="M300 350 v-36 h80 v36 z" fill="${b.T}"/>${WIN(310, 318, 60, 32)}` +
        `<rect x="280" y="180" width="120" height="16" rx="3" fill="${b.T}"/>${T(340, 192, 9, b.L, b.short)}`,
      slot: [430, 300], e2: `${R(170, 262, 96, 88, shade(b.M, .88))}${[274, 302].map(y => [184, 214, 244].map(x => WIN(x, y, 14, 16)).join('')).join('')}${R(414, 262, 96, 88, shade(b.M, .88))}${[274, 302].map(y => [424, 454, 484].map(x => WIN(x, y, 14, 16)).join('')).join('')}`, tower: 540,
    }),
  };

  function generic(lot, b) {
    const M = b.main, A = b.accent;
    const ctx = {M, A, D: shade(M, .55), T: shade(M, .3), L: shade(A, 1.35), name: b.name, short: b.short};
    const sh = (SHELLS[lot.zone] || SHELLS.shop)(ctx);
    const [mx, my] = sh.slot;
    return {
      b: sh.b + motif(b.motif, mx, my, M, A),
      e2: sh.e2,
      e3: flagTower(sh.tower, ctx, ctx.D, b.short),
      beam: shade(A, 1.6), title: shade(A, 1.15),
      lines: [`Silhouette of ${b.name} on its lot.`, `Doors open: ${b.theme}.`, `${b.name} extended.`, `${b.name} · the flagship.`],
    };
  }
  function drawing(lot, b) { return RESTAURANT[b.id] || generic(lot, b); }

  // ---------------- lot scene ----------------
  /**
   * lot: catalogue lot; square: view square; opts: {business (chosen id or null), owner {seat,name,color} or null,
   * level 0..3, choice (blueprint id on a vacant lot)}. Returns SVG markup.
   */
  function lotScene(lot, square, opts) {
    const owned = !!opts.owner, choice = opts.choice || (lot.businesses[0] && lot.businesses[0].id);
    const shown = owned ? lot.businesses.filter(b => b.id === opts.business) : lot.businesses;
    const groups = shown.map(b => {
      const d = drawing(lot, b), vis = owned || b.id === choice ? 1 : 0;
      return `<g data-biz="${esc(b.id)}" data-beam="${d.beam}" data-title="${d.title}" data-lines="${esc(JSON.stringify(d.lines))}" data-name="${esc(b.name)}" style="opacity:${vis}" class="biz">` +
        `<g class="p" data-part="e3" style="opacity:0">${d.e3}</g><g class="p" data-part="e2" style="opacity:0">${d.e2}</g><g class="p bp" data-part="b">${d.b}</g></g>`;
    }).join('');
    const svg = `<svg class="scene" viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid meet" xmlns="http://www.w3.org/2000/svg" data-kind="lot" data-zone="${esc(lot.zone_name)}" data-square="${square.square}" data-asset="${esc(lot.asset)}">` +
      backdrop(square.square, lot.district) + lotMarks(lot.zone_name, square.price_ttari, opts.owner) + groups +
      `<text data-part="h1" x="22" y="34" font-size="12" letter-spacing=".06em" fill="#4dd0e1"></text>` +
      `<text data-part="h2" x="22" y="64" font-size="26" font-weight="500" fill="#f5efe6"></text>` +
      `<text data-part="h3" x="22" y="86" font-size="13" fill="#9aa7bd"></text></svg>`;
    return svg;
  }
  /** Stage: 'vacant' | 0 | 1 | 2 | 3. With animate, a newly reached extension shows as a blueprint first. */
  function setStage(svg, stage, animate) {
    const owned = stage !== 'vacant', L = owned ? stage : 0;
    const biz = [...svg.querySelectorAll('.biz')].find(g => g.style.opacity !== '0') || svg.querySelector('.biz');
    const lot = svg.querySelector('[data-part="lot"]');
    if (lot) lot.style.opacity = L >= 1 ? 0 : 1;
    svg.classList.toggle('lit', L >= 1);
    const beams = svg.querySelector('[data-part="beams"]');
    if (biz) {
      const part = p => biz.querySelector(`[data-part="${p}"]`);
      const mode = (el, m) => { if (!el) return; if (m === 'hide') { el.style.opacity = 0; return; } el.style.opacity = 1; el.classList.toggle('bp', m === 'bp'); el.classList.toggle('built', m === 'built'); };
      mode(part('b'), L >= 1 ? 'built' : 'bp');
      const e2 = part('e2'), e3 = part('e3');
      const reach = (el, need) => {
        if (L < need) return mode(el, 'hide');
        if (animate && L === need) { mode(el, 'bp'); setTimeout(() => mode(el, 'built'), 1300); } else mode(el, 'built');
      };
      reach(e2, 2); reach(e3, 3);
      if (beams) { beams.style.opacity = L >= 3 ? 1 : 0; beams.querySelectorAll('polygon').forEach(p => p.setAttribute('fill', biz.dataset.beam)); }
      const lines = JSON.parse(biz.dataset.lines || '[]');
      const h1 = svg.querySelector('[data-part="h1"]'), h2 = svg.querySelector('[data-part="h2"]'), h3 = svg.querySelector('[data-part="h3"]');
      const where = `Square ${svg.dataset.square} · Asset ${svg.dataset.asset}`;
      if (!owned) {
        h1.textContent = `Vacant lot · ${where}`; h1.setAttribute('fill', '#4dd0e1');
        h2.textContent = `${svg.dataset.zone} lot`; h3.textContent = `Zoned for a ${svg.dataset.zone.toLowerCase()}. Blueprint shown: ${biz.dataset.name}.`;
      } else {
        h1.textContent = `${svg.dataset.zone} · ${where}`; h1.setAttribute('fill', biz.dataset.title);
        h2.textContent = biz.dataset.name; h3.textContent = lines[L] || '';
      }
    }
  }
  /** On a vacant lot, shows the blueprint of another of the zone's businesses. */
  function choose(svg, id) {
    svg.querySelectorAll('.biz').forEach(g => { g.style.opacity = g.dataset.biz === id ? 1 : 0; });
    setStage(svg, 'vacant', false);
  }

  // ---------------- special squares ----------------
  const SPECIAL = {
    'town-gate': () => `${R(220, 170, 50, 180, '#33465f')}${R(410, 170, 50, 180, '#33465f')}<path d="M220 170 Q340 80 460 170 v30 Q340 120 220 200 z" fill="#3e5878"/>${R(214, 160, 62, 12, '#26364c')}${R(404, 160, 62, 12, '#26364c')}` +
      `<circle cx="340" cy="132" r="26" fill="#0d1626" stroke="#ffd23f" stroke-width="3"/><path d="M340 132 v-16 M340 132 l12 6" stroke="#ffd23f" stroke-width="3"/><path d="M270 300 h140" stroke="#42dd86" stroke-width="6" stroke-linecap="round"/><path d="M398 288 l16 12 l-16 12" fill="none" stroke="#42dd86" stroke-width="6" stroke-linecap="round"/>${T(340, 330, 16, '#9fdcbf', 'GO')}`,
    'fortune-booth': () => `${R(270, 220, 140, 130, '#3a2a5c')}<path d="M260 222 Q340 160 420 222 Z" fill="#b48cff"/><circle cx="340" cy="290" r="28" fill="#4cc9f0" opacity=".55" class="bulb"/>${R(310, 316, 60, 10, '#1b1430')}<g class="bulb" fill="#ffe08a"><circle cx="300" cy="200" r="2"/><circle cx="380" cy="200" r="2"/><circle cx="340" cy="178" r="2"/></g>${T(340, 246, 12, '#e9ddff', 'CHANCE')}`,
    'wishing-fountain': () => `<ellipse cx="340" cy="336" rx="130" ry="16" fill="#1a2c46"/>${R(214, 300, 252, 36, '#2b3f5c')}<ellipse cx="340" cy="300" rx="126" ry="14" fill="#12304a"/>${R(332, 220, 16, 80, '#3e5878')}<ellipse cx="340" cy="220" rx="40" ry="8" fill="#3e5878"/>` +
      `<path class="bulb" d="M340 214 q-30 -40 -60 80 M340 214 q30 -40 60 80 M340 214 v-40" stroke="#4cc9f0" fill="none" stroke-width="3" opacity=".8"/><g fill="#ffd23f"><circle cx="300" cy="302" r="3"/><circle cx="356" cy="298" r="3"/><circle cx="392" cy="304" r="3"/></g>`,
    'prize-wheel': () => `<circle cx="340" cy="240" r="90" fill="#1b2740" stroke="#ffd23f" stroke-width="4"/>${Array.from({length: 12}, (_, i) => { const a = i * Math.PI / 6, b = (i + 1) * Math.PI / 6; return `<path d="M340 240 L${340 + 86 * Math.cos(a)} ${240 + 86 * Math.sin(a)} A86 86 0 0 1 ${340 + 86 * Math.cos(b)} ${240 + 86 * Math.sin(b)} Z" fill="${['#4cc9f0', '#b48cff', '#ffd23f', '#2ee6a6'][i % 4]}" opacity=".75"/>`; }).join('')}` +
      `<circle cx="340" cy="240" r="12" fill="#0d1626"/><path d="M340 136 l-10 -16 h20 z" fill="#ff5d8f"/>${R(334, 330, 12, 20, '#33465f')}`,
    'notice-board': () => `${R(240, 190, 200, 130, '#5a3e28')}${R(250, 200, 180, 110, '#2a1e14')}${R(262, 212, 60, 44, '#e9e3c9')}${R(334, 214, 80, 34, '#f3e3b5')}${R(270, 266, 70, 34, '#cfe0f5')}${R(352, 258, 62, 42, '#e9e3c9')}` +
      `<g fill="#ff5d8f"><circle cx="292" cy="214" r="3"/><circle cx="374" cy="216" r="3"/><circle cx="305" cy="268" r="3"/><circle cx="383" cy="260" r="3"/></g>${R(262, 320, 8, 30, '#5a3e28')}${R(410, 320, 8, 30, '#5a3e28')}${T(340, 186, 13, '#cfe0f5', 'TOWN HALL NOTICES')}`,
    'community-garden': () => `${R(220, 300, 240, 50, '#2a1e14')}${[240, 300, 360, 420].map((x, i) => `<path d="M${x} 300 v-40" stroke="#2f7d5b" stroke-width="4"/><circle cx="${x}" cy="252" r="14" fill="${['#e86a9a', '#ffd23f', '#b48cff', '#ff9f43'][i]}"/><circle cx="${x}" cy="252" r="5" fill="#ffe08a"/>`).join('')}` +
      `<path d="M210 300 h260" stroke="#7a5236" stroke-width="5"/>${T(340, 220, 13, '#9fdcbf', 'COMMUNITY GARDEN')}`,
    bandstand: () => `<ellipse cx="340" cy="340" rx="130" ry="12" fill="#26364c"/>${R(230, 300, 220, 40, '#33465f')}${[240, 300, 380, 430].map(x => R(x, 220, 8, 80, '#cfe0f5')).join('')}<path d="M220 222 L340 160 L460 222 Z" fill="#b48cff"/>` +
      `<g class="bulb" fill="#ffe08a">${[240, 270, 300, 330, 360, 390, 420, 450].map(x => `<circle cx="${x}" cy="226" r="2.5"/>`).join('')}</g><path d="M320 270 q10 -20 20 0 q10 20 20 0" stroke="#ffd23f" fill="none" stroke-width="3"/>`,
    'toll-booth': () => `${R(300, 230, 80, 120, '#4a2a3d')}${R(292, 222, 96, 12, '#2a1620')}${WIN(312, 248, 56, 40)}${R(380, 310, 150, 8, '#e94950')}${[400, 440, 480, 520].map(x => R(x, 310, 14, 8, '#e9eff8')).join('')}${R(372, 300, 12, 50, '#33465f')}${T(340, 216, 12, '#ffb0c0', 'TOLL')}`,
    'parking-meters': () => [220, 290, 360, 430].map(x => `${R(x + 18, 270, 6, 80, '#5f7392')}<rect x="${x + 8}" y="230" width="26" height="40" rx="8" fill="#33465f"/>${WIN(x + 13, 240, 16, 12)}`).join('') + T(340, 210, 13, '#ffb0c0', 'PARKING METER LANE'),
    'tax-office': () => `${R(250, 220, 180, 130, '#3a3550')}<path d="M240 222 L340 170 L440 222 Z" fill="#26304a"/>${[262, 302, 342, 382, 412].map(x => R(x, 230, 10, 120, '#5f7392')).join('')}${R(320, 300, 40, 50, '#1b1430')}${T(340, 210, 12, '#ffb0c0', 'TAX OFFICE')}`,
    'parking-warden': () => `<circle cx="340" cy="210" r="18" fill="#cfa98a"/>${R(318, 190, 44, 10, '#1b2740')}${R(324, 228, 32, 70, '#1b2740')}${R(324, 298, 12, 52, '#1b2740')}${R(344, 298, 12, 52, '#1b2740')}${R(356, 240, 34, 24, '#f3e3b5')}<path d="M362 248 h22 M362 254 h18" stroke="#33465f"/>${T(340, 168, 12, '#ffb0c0', 'PENALTY')}`,
    'prize-stand': () => `${R(260, 260, 160, 90, '#1c4c3c')}<path d="M250 262 h180 l-12 -26 h-156 z" fill="#2ee6a6"/>${[280, 340, 400].map((x, i) => `<path d="M${x} 300 l-12 -24 h24 z" fill="${['#ffd23f', '#e9eff8', '#ff9f43'][i]}"/><rect x="${x - 4}" y="300" width="8" height="12" fill="#ffd23f"/>`).join('')}${T(340, 226, 12, '#9fdcbf', 'PRIZE STAND')}`,
    'maintenance-bay': () => `${R(230, 230, 220, 120, '#21405a')}${R(224, 222, 232, 10, '#172f43')}${R(256, 256, 168, 94, '#0d1626')}<g stroke="#2a3e5c">${[266, 276, 286, 296].map(y => `<line x1="256" y1="${y}" x2="424" y2="${y}"/>`).join('')}</g>` +
      `<path d="M300 330 l20 -20 m-6 -6 a10 10 0 1 1 12 12" stroke="#2ee6a6" stroke-width="5" fill="none"/>${R(360, 300, 50, 30, '#33465f')}<circle cx="372" cy="332" r="8" fill="#0b111e"/><circle cx="400" cy="332" r="8" fill="#0b111e"/>${T(340, 214, 12, '#9fdcbf', 'MAINTENANCE BAY')}`,
    waterworks: () => `${R(250, 250, 120, 100, '#21405a')}<ellipse cx="420" cy="230" rx="40" ry="12" fill="#2a4a68"/>${R(380, 230, 80, 120, '#2a4a68')}<path d="M300 250 v-60 h24" stroke="#5f7392" stroke-width="6" fill="none"/>` +
      `<path d="M270 200 l10 -24 l10 24 z" fill="#ffd23f" class="bulb"/>${R(220, 340, 260, 10, '#12304a')}${T(340, 176, 12, '#cfe0f5', 'WATERWORKS & POWER')}`,
    'north-station': () => `${R(220, 240, 240, 110, '#283a5c')}<path d="M210 242 Q340 170 470 242 Z" fill="#3e5878"/><circle cx="340" cy="214" r="14" fill="#0d1626" stroke="#ffd23f" stroke-width="2"/><path d="M340 214 v-8 M340 214 l6 4" stroke="#ffd23f" stroke-width="2"/>` +
      [252, 312, 372].map(x => `<path d="M${x} 350 v-50 a28 28 0 0 1 56 0 v50 z" fill="#0d1626"/>`).join('') + T(340, 268, 12, '#cfe0f5', 'NORTH STATION'),
    'harbour-station': () => `${R(0, 318, 680, 30, '#0a1626')}${R(230, 250, 220, 90, '#283a5c')}${R(224, 242, 232, 10, '#1b2942')}${WIN(250, 266, 50, 30)}${WIN(380, 266, 50, 30)}${R(318, 280, 44, 60, '#0d1626')}<path d="M470 300 h120 l-12 18 h-96 z" fill="#3e5878"/>${R(520, 270, 10, 30, '#5f7392')}${T(340, 236, 12, '#cfe0f5', 'HARBOUR STATION')}`,
    'free-park': () => `${R(200, 336, 280, 14, '#1c3a2a')}${[250, 420].map(x => `<rect x="${x - 3}" y="250" width="6" height="90" fill="#2a1e14"/><circle cx="${x}" cy="240" r="36" fill="#1f5a3a"/>`).join('')}${R(300, 306, 80, 8, '#7a5236')}${R(306, 314, 6, 22, '#5e3e28')}${R(368, 314, 6, 22, '#5e3e28')}${R(300, 290, 80, 6, '#7a5236')}${T(340, 230, 13, '#9fdcbf', 'FREE PARK')}`,
    'tow-yard': () => `${R(200, 330, 280, 20, '#1a1612')}<path d="M200 330 v-60 M480 330 v-60" stroke="#5f7392" stroke-width="3"/><path d="M200 280 h280" stroke="#5f7392" stroke-dasharray="4 4"/>${R(270, 290, 100, 40, '#4a2a3d')}${R(340, 270, 40, 30, '#4a2a3d')}<path d="M280 290 l-30 -40" stroke="#ffd23f" stroke-width="4"/><circle cx="292" cy="332" r="10" fill="#0b111e"/><circle cx="352" cy="332" r="10" fill="#0b111e"/>${T(340, 240, 12, '#ffb0c0', 'TOW YARD · GO TO MAINTENANCE')}`,
  };
  function specialScene(sp, square) {
    const body = (SPECIAL[sp.scene] || (() => ''))();
    const district = sp.square >= 30 ? 'Harbour Front' : sp.square >= 20 ? 'Works Row' : 'Main Street';
    return `<svg class="scene lit" viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMid meet" xmlns="http://www.w3.org/2000/svg" data-kind="special" data-square="${square.square}">` +
      backdrop(square.square, district) + `<g class="spc">${body}</g>` +
      `<text x="22" y="34" font-size="12" letter-spacing=".06em" fill="#4dd0e1">${esc(`${String(sp.type).replace(/-/g, ' ')} · Square ${square.square}`)}</text>` +
      `<text x="22" y="64" font-size="26" font-weight="500" fill="#f5efe6">${esc(sp.name)}</text>` +
      `<text x="22" y="86" font-size="13" fill="#9aa7bd">${esc(sp.shows || '')}</text></svg>`;
  }
  return {lot: lotScene, special: specialScene, setStage, choose, drawing, shade};
})();
/* End of app/web/scenes.js · Location: app/web/scenes.js · Created: 2026-11-16 15:53:22 UTC */
