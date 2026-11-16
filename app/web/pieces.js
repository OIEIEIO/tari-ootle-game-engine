/*
  File:      pieces.js
  Location:  app/web/pieces.js
  Version:   v0.4.0
  Purpose:   The 20 game pieces as single-colour SVG silhouettes (the approved preview sheet). A piece
             takes the colour of its seat; the names carry no colour words.
  Notes:     Ids 1..20 match board::PIECES in app/src/board.rs. Each silhouette is drawn in a 32 x 32
             box with `currentColor`; the cut-outs use the token disc colour #111a2b.
             OGEPieces.svg(id, colour, px)   → an inline <svg> string of the silhouette
             OGEPieces.token(id, colour, px) → the silhouette on a dark disc with a ring in the seat colour
             OGEPieces.name(id)              → the piece name
*/
'use strict';
window.OGEPieces = (function () {
  const CUT = '#111a2b';
  const NAMES = ['Roadster', 'Sailboat', 'Jet', 'Helicopter', 'Locomotive', 'Racer', 'Tractor', 'Scooter', 'Speedboat', 'Bus',
    'Rocket', 'Pickup', 'Bicycle', 'Liner', 'Fire engine', 'Motorcycle', 'Cable car', 'Drone', 'Tram', 'Saucer'];
  const P = {
    1: `<g fill="currentColor"><path d="M3 21 L5 15 L12 13 L17 9 L23 9 L26 13 L29 14 L30 21 Z"/><circle cx="9" cy="22" r="3.6" stroke="${CUT}" stroke-width="1.5"/><circle cx="24" cy="22" r="3.6" stroke="${CUT}" stroke-width="1.5"/></g><path d="M18 11 H22.5 L24.5 13.5 H15.5 Z" fill="${CUT}"/>`,
    2: `<g fill="currentColor"><path d="M3 22 H29 L25 27 H7 Z"/><path d="M16 3 V20 H27 Z"/><path d="M14 7 V20 H6 Z"/></g>`,
    3: `<path fill="currentColor" d="M16 2 L18 10 L30 17 V20 L18 17.5 L17.5 25 L21 28 V30 L16 28.5 L11 30 V28 L14.5 25 L14 17.5 L2 20 V17 L14 10 Z"/>`,
    4: `<g fill="currentColor"><ellipse cx="13" cy="18" rx="8" ry="5.5"/><rect x="19" y="16" width="10" height="3"/><rect x="27" y="11" width="2.5" height="9"/><rect x="2" y="7.5" width="22" height="2"/><rect x="12" y="9" width="2" height="4"/><rect x="5" y="26" width="16" height="1.8"/><rect x="8.5" y="22.5" width="1.5" height="4"/><rect x="16" y="22.5" width="1.5" height="4"/></g><path d="M7 16 C8 13.5 10 13 12 13 V17 H7 Z" fill="${CUT}"/>`,
    5: `<g fill="currentColor"><rect x="4" y="14" width="18" height="9"/><rect x="20" y="8" width="9" height="15"/><rect x="7" y="6" width="4" height="8"/><rect x="19" y="6" width="11" height="2.5"/><path d="M4 22 L1 27 H4 Z"/><circle cx="8" cy="25" r="3" stroke="${CUT}" stroke-width="1.3"/><circle cx="15" cy="25" r="3" stroke="${CUT}" stroke-width="1.3"/><circle cx="25" cy="24.5" r="4" stroke="${CUT}" stroke-width="1.3"/></g><rect x="22" y="10.5" width="5" height="4.5" fill="${CUT}"/>`,
    6: `<g fill="currentColor"><path d="M16 2 L18.5 8 L19 22 L21.5 27 H10.5 L13 22 L13.5 8 Z"/><rect x="9" y="5.5" width="14" height="2.5"/><rect x="8" y="27" width="16" height="3"/><rect x="5" y="8" width="4" height="6" rx="1"/><rect x="23" y="8" width="4" height="6" rx="1"/><rect x="4.5" y="18.5" width="4.5" height="7" rx="1"/><rect x="23" y="18.5" width="4.5" height="7" rx="1"/></g><circle cx="16" cy="15" r="2" fill="${CUT}"/>`,
    7: `<g fill="currentColor"><rect x="4" y="13" width="14" height="7"/><rect x="15" y="5" width="9" height="10"/><rect x="8" y="7" width="2" height="6"/><circle cx="21" cy="22" r="7" stroke="${CUT}" stroke-width="1.5"/><circle cx="8" cy="24" r="4" stroke="${CUT}" stroke-width="1.5"/></g><rect x="17" y="7" width="5" height="5" fill="${CUT}"/><circle cx="21" cy="22" r="2.2" fill="${CUT}"/>`,
    8: `<g fill="currentColor"><circle cx="7" cy="24" r="4" stroke="${CUT}" stroke-width="1.3"/><circle cx="25" cy="24" r="4" stroke="${CUT}" stroke-width="1.3"/><rect x="7" y="20.5" width="16" height="3"/><path d="M22.5 22 L25 22 L21.5 6 L19 6 Z"/><rect x="16" y="4" width="8" height="2.5" rx="1"/><path d="M4 15 H13 L11 21 H6 Z"/></g>`,
    9: `<g fill="currentColor"><path d="M2 18 H24 L30 14 L28 22 L22 25 H5 Z"/><path d="M12 18 L15 11 H18 L19.5 18 Z"/><rect x="2" y="27" width="11" height="1.6"/><rect x="5" y="29.5" width="15" height="1.6"/></g>`,
    10: `<g fill="currentColor"><rect x="2" y="7" width="28" height="16" rx="3"/><circle cx="9" cy="24" r="3.5" stroke="${CUT}" stroke-width="1.5"/><circle cx="23" cy="24" r="3.5" stroke="${CUT}" stroke-width="1.5"/></g><g fill="${CUT}"><rect x="5" y="10" width="5" height="5"/><rect x="11.5" y="10" width="5" height="5"/><rect x="18" y="10" width="5" height="5"/><rect x="24.5" y="10" width="3" height="8"/></g>`,
    11: `<g fill="currentColor"><path d="M16 2 C21 7 22 13 21 22 H11 C10 13 11 7 16 2 Z"/><path d="M11 15 L5 24 L11 23 Z"/><path d="M21 15 L27 24 L21 23 Z"/><path d="M13 23.5 L16 30 L19 23.5 Z"/></g><circle cx="16" cy="11" r="2.5" fill="${CUT}"/>`,
    12: `<g fill="currentColor"><path d="M2 22 V15 H16 V9 H24 L28 15 H30 V22 Z"/><circle cx="8" cy="22" r="3.6" stroke="${CUT}" stroke-width="1.5"/><circle cx="24" cy="22" r="3.6" stroke="${CUT}" stroke-width="1.5"/></g><path d="M18 11 H23 L26 15 H18 Z" fill="${CUT}"/>`,
    13: `<g fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" stroke-linecap="round"><circle cx="8" cy="21" r="5.5"/><circle cx="24" cy="21" r="5.5"/><path d="M8 21 L12.5 12 H22 L24 21"/><path d="M12.5 12 L16 21 L22 12"/><path d="M8 21 H16"/><path d="M22 12 L21 8 H24.5"/><path d="M11 9.5 H15"/></g>`,
    14: `<g fill="currentColor"><path d="M1 19 H31 L27 27 H5 Z"/><rect x="6" y="14" width="20" height="5"/><rect x="10" y="6" width="4" height="8"/><rect x="18" y="6" width="4" height="8"/></g><g fill="${CUT}"><circle cx="9" cy="22.5" r="1.1"/><circle cx="13" cy="22.5" r="1.1"/><circle cx="17" cy="22.5" r="1.1"/><circle cx="21" cy="22.5" r="1.1"/></g>`,
    15: `<g fill="currentColor"><rect x="2" y="12" width="20" height="11"/><path d="M22 23 V9 H27 L30 14 V23 Z"/><rect x="3" y="6" width="19" height="1.5"/><rect x="3" y="9.5" width="19" height="1.5"/><rect x="5" y="6" width="1.5" height="5"/><rect x="9.5" y="6" width="1.5" height="5"/><rect x="14" y="6" width="1.5" height="5"/><rect x="18.5" y="6" width="1.5" height="5"/><circle cx="7" cy="24" r="3.5" stroke="${CUT}" stroke-width="1.5"/><circle cx="25" cy="24" r="3.5" stroke="${CUT}" stroke-width="1.5"/></g><path d="M24 11 H26.5 L28.5 14 H24 Z" fill="${CUT}"/>`,
    16: `<g fill="currentColor"><circle cx="7" cy="22" r="5" stroke="${CUT}" stroke-width="1.5"/><circle cx="25" cy="22" r="5" stroke="${CUT}" stroke-width="1.5"/><path d="M7 22 L13 14 H21 L25 22 L19 19.5 H12 Z"/><rect x="11" y="11.5" width="8" height="2.5" rx="1"/><path d="M20.5 14.5 L24 8 H27.5 V9.8 H25 L22.5 15.5 Z"/></g><circle cx="7" cy="22" r="1.8" fill="${CUT}"/><circle cx="25" cy="22" r="1.8" fill="${CUT}"/>`,
    17: `<path d="M0 5 L32 2.5" stroke="currentColor" stroke-width="1.6"/><g fill="currentColor"><rect x="15" y="3.5" width="2" height="7"/><rect x="6" y="10" width="20" height="17" rx="3"/></g><g fill="${CUT}"><rect x="9" y="13" width="6" height="6"/><rect x="17" y="13" width="6" height="6"/></g>`,
    18: `<g stroke="currentColor" fill="none"><path d="M8 8 L24 24 M24 8 L8 24" stroke-width="3"/><circle cx="7" cy="7" r="5" stroke-width="2"/><circle cx="25" cy="7" r="5" stroke-width="2"/><circle cx="7" cy="25" r="5" stroke-width="2"/><circle cx="25" cy="25" r="5" stroke-width="2"/></g><rect x="12" y="12" width="8" height="8" rx="2" fill="currentColor"/>`,
    19: `<g fill="none" stroke="currentColor" stroke-width="1.6"><path d="M11 2 H21"/><path d="M16 2 V7"/></g><g fill="currentColor"><rect x="3" y="7" width="26" height="16" rx="4"/><rect x="1" y="28" width="30" height="1.6"/><circle cx="9" cy="25" r="2.6" stroke="${CUT}" stroke-width="1.2"/><circle cx="23" cy="25" r="2.6" stroke="${CUT}" stroke-width="1.2"/></g><g fill="${CUT}"><rect x="6" y="10" width="6" height="6"/><rect x="13" y="10" width="6" height="6"/><rect x="20" y="10" width="6" height="6"/></g>`,
    20: `<g fill="currentColor"><path d="M10 15 C10 8 22 8 22 15 Z"/><ellipse cx="16" cy="18" rx="14" ry="4.5"/><path d="M12 23 L10 28 M20 23 L22 28" stroke="currentColor" stroke-width="1.5"/></g><g fill="${CUT}"><circle cx="8" cy="18" r="1.2"/><circle cx="16" cy="19.5" r="1.2"/><circle cx="24" cy="18" r="1.2"/></g>`,
  };
  const ok = id => (id >= 1 && id <= 20 ? id : 1);
  /** Silhouette only. `px` may be a number or a CSS length such as "100%". */
  function svg(id, colour, px = 24) {
    const s = typeof px === 'number' ? px + 'px' : px;
    return `<svg class="pcs" viewBox="0 0 32 32" width="${s}" height="${s}" style="color:${colour}" aria-hidden="true">${P[ok(id)]}</svg>`;
  }
  /** Silhouette on the token disc (#111a2b) with a ring in the seat colour, as on the preview sheet. */
  function token(id, colour, px = 34) {
    return `<svg class="pct" viewBox="0 0 34 34" width="${px}" height="${px}" style="color:${colour}" aria-hidden="true">` +
      `<circle cx="17" cy="17" r="16" fill="${CUT}" stroke="${colour}" stroke-width="2"/><g transform="translate(5 5) scale(.75)">${P[ok(id)]}</g></svg>`;
  }
  /** Bare inner markup for embedding inside another SVG at (x, y) with size `size`. */
  function inner(id, colour, x, y, size) {
    const k = size / 32;
    return `<g transform="translate(${x} ${y}) scale(${k})" style="color:${colour}">${P[ok(id)]}</g>`;
  }
  return {svg, token, inner, name: id => NAMES[ok(id) - 1], count: 20};
})();
/* End of app/web/pieces.js · Location: app/web/pieces.js · Created: 2026-11-16 15:53:22 UTC */
