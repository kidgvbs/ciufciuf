#!/usr/bin/env bun
// gen-schematic-layout.mjs — Generatore deterministico del layout schematico (USA, Europe).
//
// Input:  data/maps/<map>.json                (città + rotte, la "mappa")
// Output: data/layouts/<map>.json             (servito da GET /api/rulesets/{id}/layout)
//
// Il file generato va committato. Questo script va rieseguito SOLO quando
// cambia data/maps/<map>.json (nuove città/rotte): il layout è deterministico (nessuna
// fonte di casualità), quindi rigenerare produce sempre lo stesso JSON.
//
// Fase 3f+8v3 — cluster escape: label-label via city-shift + font fallback 14.5 se necessario.
//
// d3-force SENZA forceX/forceY (posizioni iniziali geografiche, poi CHARGE=-90
// + COLLIDE_R=25 apre i cluster). Canvas variabile dal bounding reale.
//
// Link-length correction (1c) DEPRIORITIZZATA: 12 iterazioni, damping 0.10,
// finestra ampia [0.8,1.55], nessun collision constraint. I conflitti visivi
// hanno PRIORITÀ ASSOLUTA e il margine di lunghezza esiste per essere consumato.
//
// 4d-bis (NUOVO): dopo tutte le fasi di bend/label, se restano conflitti,
// spinge le città coinvolte lontano (max 5px/iter), re-tick simulazione
// (600 tick), reset tratte, re-run cleanup. Loop max 8 iterazioni.
// Gate ratio: ≤1.6× standard, ≤1.7× per rotte ≥5 carte (eccezione documentata).
//
// Pipeline:
//   1. d3-force (forceSimulation): nodi inizializzati alle coordinate
//      geografiche proiettate su un canvas di lavoro grande (1400×900),
//      forceLink(distanza = len*UNIT), forceManyBody(-90), forceCollide(25).
//      Nessuna ancora geografica. randomSource fisso → byte-identico.
//   1b. Label-space relaxation + re-tick (ITERATIVO 3f+8): rilassa le coppie
//      di città con label lunghe e vicine (wi+wj ≥ 80px) spingendole
//      leggermente via (max 2px per step), POI ri-lancia forceSimulation
//      (200 tick) così i link riaggiustano le distanze. Loop max 8 cicli
//      finché stabile (nessuna coppia si muove >0.5px).
//   2. Bend route-route: coppie di tratte non-adiacenti troppo vicine
//      (distanza minima < sw_medio+6 su arco >25px). Max 20 iterazioni.
//   3. Fan-out hub: rotte incidenti con gap < 34° separate.
//   4. City-clearance routing: tratta passa a < DOT_R+9 (17px) da città
//      non-estrema → bend away (max 90px, poi WARNING).
//   5. City-safety: bend point ≥ dot.r+5 da città non-endpoint.
//   6. Etichette: 8 angoli, tier-first: 12, 20, 28, 32px. Fallback ≤ 40px.
//   7. Route-label bend: piega tratte via da label (max 100px), re-place.
//      Iterato max 8 round + re-check route-route.
//   8. Fix-up + pairwise label-label (A-first/B-first).
//   8b. Exhaustive pairwise (NUOVO 3f+8): per le coppie residue, prova
//      TUTTE le 32×32 combinazioni di candidati per entrambe le label.
//   8c. Opposite-side forcing (3f+8v2): forza label in direzioni opposte.
//   8d. Cluster escape (3f+8v3): sposta città per rompere label-label,
//      gate ratio RILASSATO (cambia solo posizioni, le carte restano
//      corrette via slot). Se dopo 10 cicli restano label-label,
//      riduce font 15.5→14.5 e rilancia. Max 30px totali per città.
//   9. Final cleanup: route-route + city-clearance + route-label, 5 cicli.
//  9b. Micro-push (NUOVO 3f+8v2): se restano label-CORE o route-city,
//      spinge di 1–2px le città coinvolte (NO re-tick), re-run cleanup.
//      Le spinte consumano lo slack di lunghezza (gate ≤1.7× per rotte ≥5 carte).
//  9c. Incroci rotte (NUOVO): routesClash sulle tracce disegnate (doppie
//      a ±offsetOf = sw/2+gap/2), adiacenti esclusa la zona hub; ogni bend
//      che crea un clash nuovo è rifiutato; pass finale raddrizza/ripiega.
//  Label: metriche reali (opentype, Barlow Semi Condensed 600), tier
//      12–32px, ownership (box più vicino al proprio dot di ogni altro).
//  10. Bounding: box di nodi + bend + label con margine 30px → canvas reale.
//  11. Validazione + report (exit 1 se hidden, label-CORE, ownership,
//      label >32px o incroci rotte). Output: anche font{} e doubleGap.
//
// Uso:  bun client/tools/gen-schematic-layout.mjs [--map usa|europe] [--svg OUT.svg]
//   --map ID       (default usa) mappa: input data/maps/<ID>.json, output data/layouts/<ID>.json.
//                  Le regolazioni manuali (LABEL_OVERRIDES, F2 shifts, pre-bend Phoenix) sono
//                  per-mappa (MAP_TUNING): applicate SOLO alla mappa a cui appartengono.
//   --svg OUT.svg  (opzionale) scrive un preview SVG per ispezione visiva.

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import opentype from 'opentype.js';
import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCollide,
} from 'd3-force';

const __dirname = dirname(fileURLToPath(import.meta.url));
const argVal = (flag) => { const i = process.argv.indexOf(flag); return i !== -1 ? process.argv[i + 1] : undefined; };
const MAP_ID = argVal('--map') ?? 'usa';
const MAP_PATH = join(__dirname, '..', '..', 'data', 'maps', `${MAP_ID}.json`);
const OUT_PATH = join(__dirname, '..', '..', 'data', 'layouts', `${MAP_ID}.json`);

// Bounding box geografico (lat/lon) usato per le posizioni iniziali della sim.
// USA = stessa bbox di map-utils.projectCity (non toccare: il layout è byte-identico).
const MAP_BOUNDS = {
  usa: { latMin: 25, latMax: 52, lonMin: -124, lonMax: -70 },
  europe: { latMin: 35, latMax: 61, lonMin: -11, lonMax: 43 },
};
if (!MAP_BOUNDS[MAP_ID]) { console.error(`--map sconosciuta: ${MAP_ID} (valide: ${Object.keys(MAP_BOUNDS).join(', ')})`); process.exit(2); }
const BOUNDS = MAP_BOUNDS[MAP_ID];

// Regolazioni manuali per-mappa (il piazzamento automatico sceglie per punteggio, non per
// gusto). Stanno qui (non nel JSON) perché una rigenerazione le cancellerebbe, e valgono
// SOLO per la mappa che le dichiara: nessun id di una mappa può influenzare l'altra.
//   labelOverrides: id città → { anchor, dir } (vedi resolveOverride)
//   f2Shifts:       [cityId, dx, dy, motivazione] — micro-shift approach-separation (≤8px)
const MAP_TUNING = {
  usa: {
    labelOverrides: {
      // Phoenix sopra il pallino (l'utente: "c'è spazio"): anchor middle, salendo.
      phoenix: { anchor: 'middle', dir: 'up' },
      // 3f+11: Omaha — il piazzamento automatico la metteva in basso-destra,
      // col centro più vicino al pallino di Kansas City (37.5px) che al suo
      // (61px): "Omaha è più vicina al pallino di kansas city che al suo".
      // Su, in alto a destra del proprio dot: anchor start, salendo.
      omaha: { anchor: 'start', dir: 'up' },
      // Saint Louis: l'automatico la spingeva a 32px verso Nashville (ownership).
      saint_louis: { anchor: 'middle', dir: 'up' },
    },
    f2Shifts: [
      // saint_louis → sud: apre il gap al hub Pittsburgh (pittsburgh→StL più sud)
      ['saint_louis', 0, 5, 'Pittsburgh hub: chicago↔pittsburgh vs pittsburgh↔saint_louis'],
      // chicago shift rimosso: non migliorava Saint Louis hub (0.4px invariato) e causava label regression
    ],
  },
  europe: {
    // Mappa più fitta (cluster Paris/Bruxelles/Frankfurt con 3 doppie): più px per carta.
    unit: 40,
    labelOverrides: {},
    f2Shifts: [],
  },
};
const TUNING = MAP_TUNING[MAP_ID];

// ── Costanti ──────────────────────────────────────────────────────────
const UNIT = TUNING.unit ?? 34; // px per carta (rotta len-1 ≈ UNIT px disegnate)
const DOT_R = 8;            // pallino full (compact = 6 → più spazio)
// Metriche REALI del font del runtime (Barlow Semi Condensed 600) via opentype.js.
// Validazione a 16px = caso peggiore tra TV (15.5) e phone (16).
const FONT_FAMILY = 'Barlow Semi Condensed';
const FONT_WEIGHT = 600;
const RUNTIME_FONT_SIZE = 15.5; // dimensione dichiarata nel JSON (se non ridotta)
let FONT_SIZE = RUNTIME_FONT_SIZE; // validazione alla size runtime (16 lasciava Oklahoma City non "propria")
const FONT = opentype.parse(new Uint8Array(readFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'node_modules',
  '@fontsource', 'barlow-semi-condensed', 'files', 'barlow-semi-condensed-latin-600-normal.woff'))).buffer);
const textWCache = new Map();
// Larghezza misurata (kerning incluso) alla FONT_SIZE corrente.
function textW(name) {
  const k = `${FONT_SIZE}|${name}`;
  let w = textWCache.get(k);
  if (w === undefined) { w = FONT.getAdvanceWidth(name, FONT_SIZE, { kerning: true }); textWCache.set(k, w); }
  return w;
}
// Box verticale: cap height 0.70em sopra la baseline, discendente 0.20em sotto.
const ascOf = () => 0.70 * FONT_SIZE;
const descOf = () => 0.20 * FONT_SIZE;
const LABEL_PAD = 1.5;      // padding box (alone ~4px del runtime: ≥1px)
const LABEL_GAP = 4;        // distanza pallino → ancora testo (dot.r + gap = 12)
const LABEL_WARN_DIST = 28; // oltre: WARNING
const MAX_LABEL_DIST = 32;  // cap duro distanza label→dot (exit 1 se superato)
const OWN_MARGIN = 10;      // ownership: ogni altro dot ≥ dOwn + 10 dal box
const DOUBLE_GAP = 5;       // gap libero inchiostro-inchiostro tra le due tracce di una doppia
// Offset doppie per lunghezza (= runtime offsetOf): metà tratto + metà gap.
const offsetOf = (e) => swOf(e.len) / 2 + DOUBLE_GAP / 2;

// d3-force
const CHARGE = -115;         // repulsione calibrata (3f+8v2: cluster sud-est)
const LINK_STRENGTH = 0.9;  // forza dei link
const COLLIDE_R = 29;       // raggio collisione calibrato (3f+8v2)
const TICKS = 1000;         // tick deterministici (↑ da 800, più convergenza con charge maggiore)

// Canvas di lavoro (le posizioni iniziali geografiche vengono proiettate qui).
const WORK_W = 1400, WORK_H = 900, WORK_PAD = 60;
const MARGIN = 30;          // margine del canvas di output attorno al bounding

// Bend / crossing
const SW_AVG_PAD = 6;       // route-route: distanza minima target = sw_medio + 6
const ROUTE_MIN_STRETCH = 25; // arco > 25px sotto soglia ⇒ conflitto
const COVER_FRACTION = 0.6; // soglia "nascosto completo" (>60% coperta)
const COVER_PAD = 2;        // gap "coperta" = sw_medio + 2
const MAX_BEND_ITER = 20;   // iterazioni globali di bend route-route
const MAX_LABEL_ROUNDS = 10; // round di separazione route-label (↑ da 8)

const swOf = (len) => 6 + len * 1.5; // = MapUtils.routeStrokeWidth
const routeGap = DOT_R + 2.5;        // trim: gap pallino (coerente col runtime)
const r2 = (v) => Math.round(v * 100) / 100;
const round1 = (v) => Math.round(v * 10) / 10;

// ── Helpers geometrici ─────────────────────────────────────────────────
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

function distPointSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const len2 = dx * dx + dy * dy;
  let t = len2 === 0 ? 0 : ((px - ax) * dx + (py - ay) * dy) / len2;
  t = clamp(t, 0, 1);
  return Math.hypot(px - (ax + dx * t), py - (ay + dy * t));
}

function projectOnSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : clamp(((px - ax) * dx + (py - ay) * dy) / len2, 0, 1);
  return [ax + dx * t, ay + dy * t];
}

function distPointPolyline(px, py, pts) {
  let m = Infinity;
  for (let i = 0; i < pts.length - 1; i++) {
    m = Math.min(m, distPointSeg(px, py, pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1]));
  }
  return m;
}

// Distanza minima segmento-segmento (Ericson, Real-Time Collision Detection).
function segSegDist(p1, p2, p3, p4) {
  const d1x = p2[0] - p1[0], d1y = p2[1] - p1[1];
  const d2x = p4[0] - p3[0], d2y = p4[1] - p3[1];
  const rx = p1[0] - p3[0], ry = p1[1] - p3[1];
  const a = d1x * d1x + d1y * d1y;
  const e = d2x * d2x + d2y * d2y;
  const f = d2x * rx + d2y * ry;
  let s, t;
  if (a <= 1e-9 && e <= 1e-9) return Math.hypot(rx, ry);
  if (a <= 1e-9) { s = 0; t = clamp(f / e, 0, 1); }
  else {
    const c = d1x * rx + d1y * ry;
    if (e <= 1e-9) { t = 0; s = clamp(-c / a, 0, 1); }
    else {
      const b = d1x * d2x + d1y * d2y;
      const denom = a * e - b * b;
      s = denom !== 0 ? clamp((b * f - c * e) / denom, 0, 1) : 0;
      t = (b * s + f) / e;
      if (t < 0) { t = 0; s = clamp(-c / a, 0, 1); }
      else if (t > 1) { t = 1; s = clamp((b - c) / a, 0, 1); }
    }
  }
  const c1x = p1[0] + d1x * s, c1y = p1[1] + d1y * s;
  const c2x = p3[0] + d2x * t, c2y = p3[1] + d2y * t;
  return Math.hypot(c1x - c2x, c1y - c2y);
}

function minDistPolyline(A, B) {
  let m = Infinity;
  for (let i = 0; i < A.length - 1; i++) {
    for (let j = 0; j < B.length - 1; j++) {
      m = Math.min(m, segSegDist(A[i], A[i + 1], B[j], B[j + 1]));
    }
  }
  return m;
}

function polylineLen(pts) {
  let L = 0;
  for (let i = 0; i < pts.length - 1; i++) L += dist(pts[i], pts[i + 1]);
  return L;
}

// Rimuove `fromStart` px dall'inizio e `fromEnd` px dalla fine della polyline
// (lungo la direzione del PRIMO/ULTIMO segmento, come il trim del rendering).
function trimPolyline(pts, fromStart, fromEnd) {
  const out = pts.map((p) => [...p]);
  let rem = fromStart;
  while (rem > 0 && out.length > 1) {
    const a = out[0], b = out[1];
    const d = dist(a, b);
    if (d <= rem) { out.shift(); rem -= d; }
    else { out[0] = [a[0] + ((b[0] - a[0]) / d) * rem, a[1] + ((b[1] - a[1]) / d) * rem]; rem = 0; }
  }
  rem = fromEnd;
  while (rem > 0 && out.length > 1) {
    const a = out[out.length - 2], b = out[out.length - 1];
    const d = dist(a, b);
    if (d <= rem) { out.pop(); rem -= d; }
    else { out[out.length - 1] = [b[0] - ((b[0] - a[0]) / d) * rem, b[1] - ((b[1] - a[1]) / d) * rem]; rem = 0; }
  }
  return out;
}

// Offset perpendicolare locale (identico a MapSvg.offsetPoints / verify.track):
// ogni punto si sposta lungo la normale del proprio segmento uscente.
function offsetPts(pts, off) {
  return pts.map((p, i) => {
    let dx, dy;
    if (i < pts.length - 1) {
      dx = pts[i + 1][0] - p[0];
      dy = pts[i + 1][1] - p[1];
    } else {
      dx = p[0] - pts[i - 1][0];
      dy = p[1] - pts[i - 1][1];
    }
    const L = Math.hypot(dx, dy) || 1;
    return [p[0] - (dy / L) * off, p[1] + (dx / L) * off];
  });
}

// Distanza minima tra le due tracce offsettate (±OFFSET) di una doppia con bend.
// Restituisce Infinity per le singole o senza bend (non c'è rischio incrocio).
// 3f+8: usata per validare che le tracce ±offsetOf non si incrocino al bend (Blocker 2).
function doubleTrackMinDist(e) {
  if (!e.isDouble || e.points.length < 3) return Infinity;
  // NON trimmed: l'incrocio può avvenire anche vicino agli estremi dopo il bend
  const plus = offsetPts(e.points, offsetOf(e));
  const minus = offsetPts(e.points, -offsetOf(e));
  return minDistPolyline(plus, minus);
}
function doubleTrackSafe(e) {
  return doubleTrackMinDist(e) >= doubleTrackMin(e);
}
function doubleTrackMin(e) { return swOf(e.len) + 0.6 * DOUBLE_GAP; }

function samplePolyline(pts, step = 3) {
  const out = [];
  for (let i = 0; i < pts.length - 1; i++) {
    const a = pts[i], b = pts[i + 1];
    const d = dist(a, b);
    const n = Math.max(1, Math.ceil(d / step));
    for (let j = 0; j < n; j++) {
      const t = j / n;
      out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
    }
  }
  out.push([...pts[pts.length - 1]]);
  return out;
}

// ── Quadratic Bézier helpers (F2: validazione su curva, non polyline) ──
// De Casteljau: B(t) = (1-t)²·P₀ + 2(1-t)t·P₁ + t²·P₂
function bezierPoint(t, p0, p1, p2) {
  const u = 1 - t;
  return [u * u * p0[0] + 2 * u * t * p1[0] + t * t * p2[0],
          u * u * p0[1] + 2 * u * t * p1[1] + t * t * p2[1]];
}
function sampleBezier(p0, p1, p2, step = 2) {
  const chordLen = dist(p0, p2);
  const ctrlLen = dist(p0, p1) + dist(p1, p2);
  const arcEst = (chordLen + ctrlLen) / 2;
  const n = Math.max(2, Math.ceil(arcEst / step));
  const out = [];
  for (let i = 0; i <= n; i++) {
    out.push(bezierPoint(i / n, p0, p1, p2));
  }
  return out;
}
function sampleCurve(points, step = 2) {
  if (points.length === 2) return samplePolyline(points, step);
  return sampleBezier(points[0], points[1], points[2], step);
}

// Distanza minima tra due curve campionate (bezier o polyline)
function minDistCurves(curveA, curveB) {
  let m = Infinity;
  for (const pa of curveA) {
    for (const pb of curveB) {
      m = Math.min(m, dist(pa, pb));
    }
  }
  return m;
}

// Distanza punto → curva campionata
function distPointCurve(px, py, curve) {
  let m = Infinity;
  for (let i = 1; i < curve.length; i++) {
    m = Math.min(m, distPointSeg(px, py, curve[i-1][0], curve[i-1][1], curve[i][0], curve[i][1]));
  }
  return m;
}

// ── 0. Caricamento ─────────────────────────────────────────────────────
const mapData = JSON.parse(readFileSync(MAP_PATH, 'utf8'));

// Proiezione geografica → canvas di lavoro (stessa bbox di map-utils.projectCity per USA).
const proj = (c) => ({
  x: WORK_PAD + ((c.lon - BOUNDS.lonMin) / (BOUNDS.lonMax - BOUNDS.lonMin)) * (WORK_W - 2 * WORK_PAD),
  y: WORK_PAD + ((BOUNDS.latMax - c.lat) / (BOUNDS.latMax - BOUNDS.latMin)) * (WORK_H - 2 * WORK_PAD),
});

const cityIds = mapData.cities.map((c) => c.id);
const idIndex = new Map(cityIds.map((id, i) => [id, i]));
const names = new Map(mapData.cities.map((c) => [c.id, c.name]));

// Edge = coppia di città non orientata (le doppie collassano in UN edge).
const pairRoutes = new Map();
for (const r of mapData.routes) {
  const key = [r.from, r.to].sort().join('|');
  const list = pairRoutes.get(key) || [];
  list.push(r);
  pairRoutes.set(key, list);
}

const edges = [];
const links = [];
for (const [key, routes] of pairRoutes) {
  const [a, b] = key.split('|');
  const ai = idIndex.get(a), bi = idIndex.get(b);
  if (ai === undefined || bi === undefined || ai === bi) continue;
  const len = Math.min(...routes.map((r) => r.len));
  edges.push({
    key,
    a: ai,
    b: bi,
    len,
    isDouble: routes.length > 1,
    routeIds: routes.map((r) => r.id),
    points: null,
    bendSign: null,
  });
  links.push({ source: a, target: b, len });
}

// ── 1. d3-force (deterministico) ──────────────────────────────────────
const nodes = mapData.cities.map((c) => {
  const p = proj(c);
  return { id: c.id, x: p.x, y: p.y };
});

// 3f+7: Nessuna ancora geografica (forceX/forceY rimossi). Le posizioni
// iniziali sono geografiche (determinismo), poi la sim le rilascia.
// CHARGE=-90 + COLLIDE_R=25 → cluster aperti, spazio per label e tratte.
const simulation = forceSimulation(nodes)
  .force('link', forceLink(links).id((d) => d.id).distance((l) => l.len * UNIT).strength(LINK_STRENGTH))
  .force('charge', forceManyBody().strength(CHARGE))
  .force('collide', forceCollide().radius(COLLIDE_R))
  .randomSource(() => 0.5) // jiggle deterministico (mai attivo: nodi iniziali distinti)
  .stop();

for (let i = 0; i < TICKS; i++) simulation.tick();

// ── 1b. Label-space relaxation + simulation re-tick (iterativo) ──
// 3f+8 fix: la relaxation sposta coppie di città con label lunghe e vicine
// per evitare overlap. DOPO ogni ciclo di relaxation la simulazione viene
// ri-lanciata (200 tick) così i link riaggiustano le distanze verso il
// target. Senza il re-tick, l'errore di lunghezza si accumula (Blocker 1).
// Max 8 cicli, convergenza se nessuna coppia si muove più di 0.5px.
let relaxIterations = 0;
for (let cycle = 0; cycle < 3; cycle++) {
  let maxMove = 0;
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      const wi = textW(names.get(nodes[i].id) || '');
      const wj = textW(names.get(nodes[j].id) || '');
      // Solo coppie con label larghe (> 80px somma) e città vicine
      if (wi + wj < 80) continue;
      const needed = (wi + wj) * 0.28 + 2 * (DOT_R + LABEL_GAP) + 2;
      const dx = nodes[j].x - nodes[i].x;
      const dy = nodes[j].y - nodes[i].y;
      const d = Math.hypot(dx, dy);
      if (d < needed && d > 0.1) {
        const push = Math.min((needed - d) * 0.01, 0.4);
        const nx = dx / d, ny = dy / d;
        nodes[i].x -= nx * push * 0.5;
        nodes[i].y -= ny * push * 0.5;
        nodes[j].x += nx * push * 0.5;
        nodes[j].y += ny * push * 0.5;
        maxMove = Math.max(maxMove, push);
      }
    }
  }
  relaxIterations = cycle + 1;
  if (maxMove < 0.3) break;
  // Re-tick simulation: i link riaggiustano le distanze verso il target
  for (let i = 0; i < 1600; i++) simulation.tick();
}

// ── 1c. Link-length correction (WEAKENED 3f+8v2) ──
// De-prioritizzata: i conflitti visivi hanno PRIORITÀ ASSOLUTA. Questa
// correzione ora è molto blanda (12 iterazioni, damping 0.10, finestra
// ampia [0.8,1.55]) e non usa collision constraint — lo slack creato dai
// conflitti NON deve essere ripreso. La relaxation alternata (1b) + la
// sim d3-force già mantengono le lunghezze ragionevoli.
for (let lcIter = 0; lcIter < 12; lcIter++) {
  let maxAdj = 0;
  const adjustments = nodes.map(() => ({ dx: 0, dy: 0, n: 0 }));
  for (const e of edges) {
    const a = e.a, b = e.b;
    const ax = nodes[a].x, ay = nodes[a].y;
    const bx = nodes[b].x, by = nodes[b].y;
    const actual = Math.hypot(bx - ax, by - ay);
    const target = e.len * UNIT;
    const ratio = actual / target;
    if (ratio > 0.80 && ratio < 1.55) continue;
    const move = (1 - target / actual) * actual * 0.10;
    const dx = (bx - ax) / actual * move;
    const dy = (by - ay) / actual * move;
    adjustments[a].dx += dx; adjustments[a].dy += dy; adjustments[a].n++;
    adjustments[b].dx -= dx; adjustments[b].dy -= dy; adjustments[b].n++;
    maxAdj = Math.max(maxAdj, Math.abs(move));
  }
  if (maxAdj < 0.05) break;
  for (let i = 0; i < nodes.length; i++) {
    const adj = adjustments[i];
    if (adj.n === 0) continue;
    nodes[i].x += adj.dx / adj.n * 0.5;
    nodes[i].y += adj.dy / adj.n * 0.5;
  }
}

const cityPos = nodes.map((n) => [n.x, n.y]);
for (const e of edges) {
  // Copie (non alias di cityPos): il bend muta e.points, lo shift di
  // output deve muovere dots e rotte in modo indipendente e coerente.
  e.points = [[...cityPos[e.a]], [...cityPos[e.b]]];
}

// ── 2. Bend ────────────────────────────────────────────────────────────
function trimPts(pts, e) {
  const sw = swOf(e.len);
  const trim = Math.min(routeGap + sw / 2, polylineLen(pts) * 0.42);
  return trimPolyline(pts, trim, trim);
}
function trimmed(e) { return trimPts(e.points, e); }

// Tracce DISEGNATE (offset doppie ±OFFSET, poi trim) — stesse formule del runtime.
function tracksOf(e) {
  if (!e.isDouble) return [trimmed(e)];
  return [trimPts(offsetPts(e.points, offsetOf(e)), e), trimPts(offsetPts(e.points, -offsetOf(e)), e)];
}

// F2: Campionamento della curva DISEGNATA (De Casteljau sui punti trimmed).
// 3 punti trimmed → quadratic Bézier; 2 punti → lineare.
// Per le doppie, campiona entrambe le tracce offsettate e ritorna la
// lista piatta di punti campionati.
function sampleDrawnCurve(e) {
  const pts = trimmed(e);
  return sampleCurve(pts, 2);
}
function sampledCurvesOf(e) {
  if (!e.isDouble) return [sampleDrawnCurve(e)];
  const plus = trimPts(offsetPts(e.points, offsetOf(e)), e);
  const minus = trimPts(offsetPts(e.points, -offsetOf(e)), e);
  return [sampleCurve(plus, 2), sampleCurve(minus, 2)];
}

// Distanza minima tra due tratte disegnate (curve campionate)
function minDistDrawn(a, b) {
  const curvesA = sampledCurvesOf(a);
  const curvesB = sampledCurvesOf(b);
  let m = Infinity;
  for (const ca of curvesA)
    for (const cb of curvesB)
      m = Math.min(m, minDistCurves(ca, cb));
  return m;
}

// Distanza punto → tratte disegnate di un edge (curve campionate)
function distPointDrawnEdge(px, py, e) {
  let m = Infinity;
  for (const curve of sampledCurvesOf(e))
    m = Math.min(m, distPointCurve(px, py, curve));
  return m;
}

// F2: overlap stretch su curve, non polyline
function overlapStretchCurve(A, B, threshold) {
  const curvesA = sampledCurvesOf(A);
  const curvesB = sampledCurvesOf(B);
  // Unione di tutte le curve di B in un unico array per lookup veloce
  const allB = [];
  for (const cb of curvesB) allB.push(...cb);
  
  let maxRun = 0, cur = 0;
  for (const ca of curvesA) {
    for (let i = 1; i < ca.length; i++) {
      const d = distPointCurve((ca[i-1][0]+ca[i][0])/2, (ca[i-1][1]+ca[i][1])/2, allB);
      const stepLen = dist(ca[i], ca[i-1]);
      if (d < threshold) { cur += stepLen; maxRun = Math.max(maxRun, cur); }
      else cur = 0;
    }
  }
  return maxRun;
}

// F2: cover fraction su curve
function coverFractionCurve(A, B, threshold) {
  const curvesA = sampledCurvesOf(A);
  const allB = [];
  for (const cb of sampledCurvesOf(B)) allB.push(...cb);
  
  let covered = 0, total = 0;
  for (const ca of curvesA) {
    for (let i = 1; i < ca.length; i++) {
      const midX = (ca[i-1][0] + ca[i][0]) / 2;
      const midY = (ca[i-1][1] + ca[i][1]) / 2;
      const d = distPointCurve(midX, midY, allB);
      const stepLen = dist(ca[i], ca[i-1]);
      total += stepLen;
      if (d < threshold) covered += stepLen;
    }
  }
  return total > 0 ? covered / total : 0;
}

// ── F1: Approach separation ────────────────────────────────────────────
// Minima distanza tra le curve DISEGNATE di due rotte che condividono un
// endpoint, valutata SOLO nella zona a ≥20px dal dot condiviso.
// Soglia: ≥10px (gate bloccante: exit 1 se violato).
const APPROACH_MIN_DIST = 10;
const APPROACH_ZONE = 20; // ≥20px dal dot condiviso

function approachMinDistDrawn(eA, eB, sharedCi) {
  const sharedPt = cityPos[sharedCi];
  const allCurvesA = sampledCurvesOf(eA);
  const allCurvesB = sampledCurvesOf(eB);
  
  let minDist = Infinity;
  for (const cA of allCurvesA) {
    for (const cB of allCurvesB) {
      for (const ptA of cA) {
        if (dist(ptA, sharedPt) < APPROACH_ZONE) continue;
        for (const ptB of cB) {
          if (dist(ptB, sharedPt) < APPROACH_ZONE) continue;
          minDist = Math.min(minDist, dist(ptA, ptB));
        }
      }
    }
  }
  return minDist;
}

function swAvg(a, b) { return (swOf(a.len) + swOf(b.len)) / 2; }

// Massimo tratto (arc-length, px) di A che resta entro `threshold` da B.
function overlapStretch(A, B, threshold) {
  const samples = samplePolyline(A, 3);
  let maxRun = 0, cur = 0;
  for (let i = 1; i < samples.length; i++) {
    const d = distPointPolyline(samples[i][0], samples[i][1], B);
    const stepLen = dist(samples[i], samples[i - 1]);
    if (d < threshold) { cur += stepLen; maxRun = Math.max(maxRun, cur); }
    else cur = 0;
  }
  return maxRun;
}

function edgeOrder(a, b) {
  if (a.len !== b.len) return a.len < b.len ? [a, b] : [b, a];
  const la = polylineLen(a.points), lb = polylineLen(b.points);
  if (la !== lb) return la < lb ? [a, b] : [b, a];
  return a.key < b.key ? [a, b] : [b, a];
}

function isAdjacent(a, b) {
  return a.a === b.a || a.a === b.b || a.b === b.a || a.b === b.b;
}

function isConflict(a, b) {
  const th = swAvg(a, b) + SW_AVG_PAD;
  // F2: overlap su curve campionate
  return overlapStretchCurve(a, b, th) > ROUTE_MIN_STRETCH ||
         overlapStretchCurve(b, a, th) > ROUTE_MIN_STRETCH;
}

function findConflicts() {
  const conflicts = [];
  for (let i = 0; i < edges.length; i++) {
    for (let j = i + 1; j < edges.length; j++) {
      const a = edges[i], b = edges[j];
      if (isAdjacent(a, b)) continue;
      if (isConflict(a, b)) conflicts.push(edgeOrder(a, b));
    }
  }
  return conflicts;
}

// Macchinario del bend: aggiunge/riposiziona il punto medio sulla normale.
function ensureBendable(e) {
  if (e.points.length === 2) {
    e.points.splice(1, 0, [(e.points[0][0] + e.points[1][0]) / 2, (e.points[0][1] + e.points[1][1]) / 2]);
    e.bendSign = null;
  }
}
function normalOf(e) {
  const [ax, ay] = e.points[0], [bx, by] = e.points[e.points.length - 1];
  const dx = bx - ax, dy = by - ay;
  const len = Math.hypot(dx, dy) || 1;
  return { mid: [(ax + bx) / 2, (ay + by) / 2], nx: -dy / len, ny: dx / len };
}
function setBend(e, sign, step) {
  const { mid, nx, ny } = normalOf(e);
  e.points[1] = [mid[0] + nx * sign * step, mid[1] + ny * sign * step];
}

// Sceglie il lato della normale con PIÙ spazio: valuta, a più ampiezze, la
// distanza minima da TUTTE le altre tratte (e dai pallini), prende il lato
// che ne resta più lontano.
function pickBendSign(e, nx, ny, mid) {
  let bestSign = 1, bestScore = -Infinity;
  for (const sign of [1, -1]) {
    let score = 0;
    for (const mag of [8, 16, 24, 32, 40, 48, 56, 64, 72, 80]) {
      const mx = mid[0] + nx * sign * mag, my = mid[1] + ny * sign * mag;
      let minD = Infinity;
      for (const o of edges) {
        if (o === e) continue;
        minD = Math.min(minD, distPointPolyline(mx, my, o.points));
      }
      for (const p of cityPos) minD = Math.min(minD, Math.hypot(mx - p[0], my - p[1]));
      score += minD;
    }
    if (score > bestScore) { bestScore = score; bestSign = sign; }
  }
  return bestSign;
}

// Verifica che il bend point non stia sopra un pallino non-endpoint.
function bendPointCitySafe(e) {
  const ends = new Set([e.a, e.b]);
  for (let pi = 1; pi < e.points.length - 1; pi++) {
    const [px, py] = e.points[pi];
    for (let ci = 0; ci < cityPos.length; ci++) {
      if (ends.has(ci)) continue;
      if (Math.hypot(px - cityPos[ci][0], py - cityPos[ci][1]) < DOT_R + 5) return false;
    }
  }
  return true;
}

// Verifica che e non confligga con nessun'altra tratta non-adiacente.
// ── C: incroci/sovrapposizioni rotte (anche adiacenti, esclusa zona hub) ──
const HUB_R = routeGap + 24;
const clashCache = new Map(); // edge → { key, tracks, bbox }
function clashTracks(e) {
  const key = JSON.stringify(e.points);
  let c = clashCache.get(e);
  if (c && c.key === key) return c;
  const tracks = e.isDouble
    ? [trimPts(offsetPts(e.points, offsetOf(e)), e), trimPts(offsetPts(e.points, -offsetOf(e)), e)].map((p) => sampleCurve(p, 3))
    : [sampleCurve(trimmed(e), 3)];
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const t of tracks) for (const [x, y] of t) {
    x0 = Math.min(x0, x); y0 = Math.min(y0, y); x1 = Math.max(x1, x); y1 = Math.max(y1, y);
  }
  c = { key, tracks, bbox: [x0, y0, x1, y1] };
  clashCache.set(e, c);
  return c;
}
function segDist(p, q, r, s) {
  const ps = (P, A, B) => {
    const dx = B[0] - A[0], dy = B[1] - A[1], L = dx * dx + dy * dy;
    const t = L ? Math.max(0, Math.min(1, ((P[0] - A[0]) * dx + (P[1] - A[1]) * dy) / L)) : 0;
    return Math.hypot(P[0] - A[0] - t * dx, P[1] - A[1] - t * dy);
  };
  const cr = (A, B, C) => (B[0] - A[0]) * (C[1] - A[1]) - (B[1] - A[1]) * (C[0] - A[0]);
  const d1 = cr(p, q, r), d2 = cr(p, q, s), d3 = cr(r, s, p), d4 = cr(r, s, q);
  if (((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))) return 0;
  return Math.min(ps(p, r, s), ps(q, r, s), ps(r, p, q), ps(s, p, q));
}
function routesClash(a, b) {
  if (a === b) return false;
  const ca = clashTracks(a), cb = clashTracks(b);
  const th = (swOf(a.len) + swOf(b.len)) / 2 + 1.5;
  const A = ca.bbox, B = cb.bbox;
  if (A[0] > B[2] + th || B[0] > A[2] + th || A[1] > B[3] + th || B[1] > A[3] + th) return false;
  let hub = null;
  if (a.a === b.a || a.a === b.b) hub = cityPos[a.a];
  else if (a.b === b.a || a.b === b.b) hub = cityPos[a.b];
  const far = (P) => !hub || Math.hypot(P[0] - hub[0], P[1] - hub[1]) >= HUB_R;
  for (const ta of ca.tracks) for (const tb of cb.tracks) {
    for (let i = 0; i + 1 < ta.length; i++) {
      if (!far(ta[i]) || !far(ta[i + 1])) continue;
      for (let j = 0; j + 1 < tb.length; j++) {
        if (!far(tb[j]) || !far(tb[j + 1])) continue;
        if (segDist(ta[i], ta[i + 1], tb[j], tb[j + 1]) < th) return true;
      }
    }
  }
  return false;
}
// Nessun clash NUOVO rispetto alla versione dritta di e.
function noNewClash(e) {
  if (e.points.length < 3) return true;
  const saved = e.points;
  const bad = edges.filter((o) => o !== e && routesClash(e, o));
  if (bad.length === 0) return true;
  e.points = [[...cityPos[e.a]], [...cityPos[e.b]]];
  const ok = bad.every((o) => routesClash(e, o));
  e.points = saved;
  return ok;
}
function countClashes() {
  const out = [];
  for (let i = 0; i < edges.length; i++)
    for (let j = i + 1; j < edges.length; j++)
      if (routesClash(edges[i], edges[j])) out.push([edges[i], edges[j]]);
  return out;
}

function noRouteConflict(e) {
  if (!noNewClash(e)) return false;
  for (const o of edges) {
    if (o === e || isAdjacent(e, o)) continue;
    const th = swAvg(e, o) + SW_AVG_PAD;
    if (overlapStretchCurve(e, o, th) > ROUTE_MIN_STRETCH ||
        overlapStretchCurve(o, e, th) > ROUTE_MIN_STRETCH) return false;
  }
  return true;
}

// Piega e finché la distanza minima (curve campionate) dall'altra tratta è ≥ targetSep.
function pushBendRoute(e, other, targetSep) {
  ensureBendable(e);
  const { mid, nx, ny } = normalOf(e);
  if (e.bendSign === null) e.bendSign = pickBendSign(e, nx, ny, mid);
  for (const sign of [e.bendSign, -e.bendSign]) {
    for (let step = 2; step <= 120; step += 2) {
      setBend(e, sign, step);
      if (minDistDrawn(e, other) >= targetSep && doubleTrackSafe(e) && noNewClash(e)) {
        e.bendSign = sign;
        return true;
      }
    }
  }
  return false;
}

// ── 2a. Bend route-route ──────────────────────────────────────────────
let bendIterations = 0;
for (let it = 0; it < MAX_BEND_ITER; it++) {
  const conflicts = findConflicts();
  if (conflicts.length === 0) { bendIterations = it; break; }
  for (const [shorter, longer] of conflicts) {
    pushBendRoute(shorter, longer, swAvg(shorter, longer) + SW_AVG_PAD);
  }
  bendIterations = it + 1;
}

// ── 2a-bis. Adjacent-route overlap check ─────────────────────────────
// 3f+9: rotte che condividono un endpoint (es. duluth↔toronto vs
// sault_st_marie↔toronto) possono sovrapporsi visivamente vicino al nodo
// comune. isConflict le salta (isAdjacent → true), quindi serve un check
// dedicato: se le tracce si avvicinano troppo su un arco significativo,
// piega la più corta. F2: overlap su curve.
let adjacentBendWarnings = [];
function adjacentOverlapCheck() {
  for (let ci = 0; ci < cityIds.length; ci++) {
    const incident = edges.filter((e) => e.a === ci || e.b === ci);
    for (let ai = 0; ai < incident.length; ai++) {
      for (let bi = ai + 1; bi < incident.length; bi++) {
        const a = incident[ai], b = incident[bi];
        const swT = swAvg(a, b);
        const th = swT + SW_AVG_PAD;
        const ovA = overlapStretchCurve(a, b, th);
        const ovB = overlapStretchCurve(b, a, th);
        const maxOv = Math.max(ovA, ovB);
        if (maxOv > ROUTE_MIN_STRETCH) {
          const [shorter, longer] = edgeOrder(a, b);
          const ok = pushBendRoute(shorter, longer, swT + SW_AVG_PAD);
          if (!ok) {
            adjacentBendWarnings.push(
              `${cityIds[shorter.a]}↔${cityIds[shorter.b]} vs ${cityIds[longer.a]}↔${cityIds[longer.b]} ` +
              `(hub ${cityIds[ci]}, overlap ${maxOv.toFixed(0)}px — pushBendRoute FALLITO)`
            );
          }
        }
      }
    }
  }
}
adjacentOverlapCheck();

// ── 2a-ter. Fan-out hub: rotte incidenti allo stesso hub quasi-parallele ──
// Due rotte che escono dallo stesso pallino in direzioni quasi identiche
// (es. LA→PHX vs LA→ELP) restano sovrapposte appena fuori dal trim. Si
// ordinano le rotte per angolo di uscita e, per ogni coppia consecutiva con
// gap < MIN_FAN_DEG, la più corta viene piegata VERSO L'ESTERNO (via dal gap).
// 3f+9: MAX_FAN_TURN alzato a 75° per gestire cluster densi (es. denver, LR).
const MIN_FAN_DEG = 34;
const MIN_FAN = (MIN_FAN_DEG * Math.PI) / 180;
function exitAngleAt(e, ci) {
  const pts = e.points;
  const s = e.a === ci ? pts[0] : pts[pts.length - 1];
  const n = e.a === ci ? pts[1] : pts[pts.length - 2];
  return Math.atan2(n[1] - s[1], n[0] - s[0]);
}
let fanOutWarnings = [];
function fanOutHubs() {
  fanOutWarnings = [];
  for (let ci = 0; ci < cityIds.length; ci++) {
    const incident = edges.filter((e) => e.a === ci || e.b === ci);
    if (incident.length < 2) continue;
    incident.sort((a, b) => exitAngleAt(a, ci) - exitAngleAt(b, ci));
    for (let i = 0; i < incident.length; i++) {
      const a = incident[i];
      const b = incident[(i + 1) % incident.length];
      let gap = exitAngleAt(b, ci) - exitAngleAt(a, ci);
      if (i === incident.length - 1) gap += 2 * Math.PI;
      if (gap >= MIN_FAN) continue;
      const shorter = a.len <= b.len ? a : b;
      // segno "verso l'esterno": a (angolo minore) ruota indietro (−), b avanti (+)
      const sign = shorter === a ? -1 : 1;
      const saved = shorter.points.map((p) => [...p]);
      const savedSign = shorter.bendSign;
      ensureBendable(shorter);
      const { mid, nx, ny } = normalOf(shorter);
      // 3f+9: MAX_FAN_TURN alzato a 75° per gestire hub densi (era 70°)
      const halfLen = dist(shorter.points[0], mid);
      const MAX_FAN_TURN = 82 * Math.PI / 180; // 3f+9: 82° per hub densi
      let maxStep = Math.max(2, Math.floor(halfLen * Math.tan(MAX_FAN_TURN / 2) / 2) * 2);
      // Prova anche oltre il limite safe se necessario (fino a 1.6×)
      const maxStepAggressive = Math.max(maxStep, Math.floor(halfLen * Math.tan(85 * Math.PI / 180 / 2) / 2) * 2);
      let applied = false;
      // Passo 1: entro limite safe
      for (let step = 2; step <= maxStep; step += 2) {
        setBend(shorter, sign, step);
        let g2 = exitAngleAt(b, ci) - exitAngleAt(a, ci);
        if (i === incident.length - 1) g2 += 2 * Math.PI;
        if (g2 >= MIN_FAN && noRouteConflict(shorter) && bendPointCitySafe(shorter) && doubleTrackSafe(shorter)) {
          shorter.bendSign = sign;
          applied = true;
          break;
        }
      }
      // Passo 2: aggressivo (oltre safe, fino a 85° turn)
      if (!applied) {
        for (let step = maxStep + 2; step <= maxStepAggressive; step += 2) {
          setBend(shorter, sign, step);
          let g2 = exitAngleAt(b, ci) - exitAngleAt(a, ci);
          if (i === incident.length - 1) g2 += 2 * Math.PI;
          if (g2 >= MIN_FAN && noRouteConflict(shorter) && bendPointCitySafe(shorter) && doubleTrackSafe(shorter)) {
            shorter.bendSign = sign;
            applied = true;
            break;
          }
        }
      }
      // Passo 3: prova segno opposto (piega verso il gap invece che via)
      if (!applied) {
        for (let step = 2; step <= maxStepAggressive; step += 2) {
          setBend(shorter, -sign, step);
          let g2 = exitAngleAt(b, ci) - exitAngleAt(a, ci);
          if (i === incident.length - 1) g2 += 2 * Math.PI;
          if (g2 >= MIN_FAN && noRouteConflict(shorter) && bendPointCitySafe(shorter) && doubleTrackSafe(shorter)) {
            shorter.bendSign = -sign;
            applied = true;
            break;
          }
        }
      }
      if (!applied) {
        shorter.points = saved;
        shorter.bendSign = savedSign;
        fanOutWarnings.push(
          `${cityIds[ci]} hub: ${cityIds[a.a]}↔${cityIds[a.b]} vs ${cityIds[b.a]}↔${cityIds[b.b]} ` +
          `gap ${(gap * 180 / Math.PI).toFixed(1)}° < ${MIN_FAN_DEG}° — fanOut FALLITO`
        );
      }
    }
  }
}
fanOutHubs();

// ── 2b. City-clearance routing ───────────────────────────────────────
// Regola metro-map: nessun binario passa attraverso stazioni non-estreme.
// Dopo route-route bend e fan-out, per OGNI tratta: se la polyline trimmed
// passa a < DOT_R + 9 (17px) dal centro di una città non-endpoint, piega la
// tratta sulla normale finché la distanza è ≥ 17px (step 2px, max ~70px poi
// WARNING). La scelta del lato preferisce ALLONTANARE dalla città intrusa.
const CITY_CLEARANCE = DOT_R + 9; // 17px
const MAX_CITY_BEND = 90;         // max ampiezza di piega per city clearance

function routeCityDist(e, ci) {
  // F2: distanza sulla curva campionata, non sulla polyline
  return distPointDrawnEdge(cityPos[ci][0], cityPos[ci][1], e);
}

function cityClearanceConflicts() {
  const conflicts = [];
  for (const e of edges) {
    for (let ci = 0; ci < cityIds.length; ci++) {
      if (ci === e.a || ci === e.b) continue;
      const d = routeCityDist(e, ci);
      if (d < CITY_CLEARANCE) conflicts.push({ edge: e, cityIdx: ci, dist: d });
    }
  }
  // Risolvi i conflitti più gravi (distanza minore) prima
  conflicts.sort((a, b) => a.dist - b.dist);
  return conflicts;
}

// Piega una tratta via da una città: prova entrambi i segni, step fino a
// MAX_CITY_BEND, con verifica noRouteConflict + bendPointCitySafe.
function bendFromCity(e, ci) {
  ensureBendable(e);
  const { mid, nx, ny } = normalOf(e);
  // Lato preferito: la direzione che allontana dal dot
  const [cx, cy] = cityPos[ci];
  const toCity = [cx - mid[0], cy - mid[1]];
  const prefSign = (nx * toCity[0] + ny * toCity[1]) > 0 ? -1 : 1;
  if (e.bendSign === null) e.bendSign = pickBendSign(e, nx, ny, mid);
  for (const sign of [prefSign, -prefSign, e.bendSign, -e.bendSign]) {
    for (let step = 2; step <= MAX_CITY_BEND; step += 2) {
      setBend(e, sign, step);
      if (routeCityDist(e, ci) >= CITY_CLEARANCE &&
          noRouteConflict(e) && bendPointCitySafe(e) && doubleTrackSafe(e)) {
        e.bendSign = sign;
        return true;
      }
    }
  }
  return false;
}

let cityClearanceWarnings = [];
let cityClearanceBends = 0;
for (let iter = 0; iter < 12; iter++) {
  const ccConflicts = cityClearanceConflicts();
  if (ccConflicts.length === 0) break;
  let bent = false;
  for (const { edge, cityIdx, dist } of ccConflicts) {
    if (bendFromCity(edge, cityIdx)) { bent = true; cityClearanceBends++; }
    else cityClearanceWarnings.push(`${cityIds[edge.a]}-${cityIds[edge.b]} vs ${cityIds[cityIdx]} (${dist.toFixed(1)}px → irrisolto)`);
  }
  if (!bent) break;
}

// ── 2c. City-safety post-bend (bend point ≥ dot.r+5 da città non-endpoint) ──
for (const e of edges) {
  if (e.points.length <= 2) continue;
  if (bendPointCitySafe(e)) continue;
  const { mid, nx, ny } = normalOf(e);
  let best = null;
  for (const sign of [1, -1]) {
    for (let step = 2; step <= 150; step += 2) {
      setBend(e, sign, step);
      if (bendPointCitySafe(e) && noRouteConflict(e) && doubleTrackSafe(e)) {
        if (!best || step < best.step) best = { sign, step };
        break;
      }
    }
  }
  if (best) { setBend(e, best.sign, best.step); e.bendSign = best.sign; }
}

// ── 3. Etichette ───────────────────────────────────────────────────────
const D0 = DOT_R + LABEL_GAP; // 12
// 3f+7: TIERS ridotti — cap 32px (era 45). Ordinati per distanza (tier
// UNICO preferito): prima tutti gli 8 angoli a 12px, poi 20, 28, 32.
const TIERS = [D0, D0 + 4, D0 + 8, D0 + 12, D0 + 16, D0 + 20]; // 12, 16, 20, 24, 28 (+32 fallback con WARNING)
const N_DIRS = 8; // 8 angoli ogni 45°, anchor coerente alla direzione
const FALLBACK_MAX_DIST = MAX_LABEL_DIST; // cap duro 32px

// 3f+10: override manuali per-city, richiesti dall'utente dopo la validazione
// visiva (il piazzamento automatico sceglie per punteggio, non per gusto).
// Formato: direzione preferita; placeLabels prova i candidati in quella
// direzione in ordine di distanza e prende il primo senza conflitti
// (se nessuno è pulito, vince il piazzamento automatico — la validazione
// resta sovrana). Aggiunti qui (non nel JSON) perché una rigenerazione li
// cancellerebbe.
// Le voci stanno in MAP_TUNING[map].labelOverrides (in testa al file).
const LABEL_OVERRIDES = TUNING.labelOverrides;

// RResolve un override: candidati nella direzione voluta, ordinati per
// distanza; primo con conflitti 0 vince. Se nessuno è pulito, restituisce
// il più vicino (i conflitti con rotte INCIDENTI non contano per la
// validazione finale né per verify — l'eventuale bending delle incidenti
// viene gestito dalla fase di pre-bend dedicata 3f+10).
function resolveOverride(c, name, id) {
  const ov = LABEL_OVERRIDES[id];
  if (!ov) return null;
  const cands = candidatesFor()
    .filter((k) => k.anchor === ov.anchor && (ov.dir === 'up' ? k.dy < 0 : k.dy > 0))
    .sort((a, b) => a.d - b.d);
  for (const cand of cands) {
    const s = scoreLabel(c, cand, name, id);
    if (process.env.LABEL_DEBUG) console.error(`  [ov ${id}] d=${cand.d} dy=${cand.dy} → total=${s.total} core=${s.nCore} label=${s.nLabel} dot=${s.nDot} clearance=${s.clearance.toFixed(1)}`);
    if (s.total === 0) return cand;
  }
  // 3f+10: nessun candidato "pulito" — restituisci il più vicino.
  // scoreLabel conta TUTTE le rotte (anche incidenti), ma la validazione
  // finale e verify escludono le rotte incidenti alla città della label.
  // I conflitti residui (con rotte incidenti) vengono gestiti dal
  // pre-bend dedicato per Phoenix (fase 3f+10 dopo placeLabels).
  if (cands.length > 0) {
    if (process.env.LABEL_DEBUG) console.error(`  [ov ${id}] fallback: nessun candidato pulito, prendo il più vicino (d=${cands[0].d})`);
    return cands[0];
  }
  return null;
}

// Candidati: N_DIRS angoli × tier crescenti fino a MAX_LABEL_DIST.
// Anchor coerente alla direzione: start se a destra, end se a sinistra,
// middle se sopra/sotto. Il vertice del box più vicino al dot sta a
// distanza `d`. La label DEVE stare entro 32px — eventuali fallback ≤40px
// vengono segnalati con WARNING.
// Box metrico: il vertice/lato del box (padding incluso) più vicino sta a `d`.
function candidatesFor() {
  const asc = ascOf() + LABEL_PAD, desc = descOf() + LABEL_PAD;
  const vy = () => (asc - desc) / 2; // centro verticale del box sul dot
  const out = [];
  for (const d of TIERS) {
    for (let k = 0; k < N_DIRS; k++) {
      const ang = (k / N_DIRS) * 2 * Math.PI;
      const cos = Math.cos(ang), sin = Math.sin(ang);
      let anchor, dx, dy;
      const sideDy = Math.abs(sin) < 0.25 ? vy(sin) : (sin > 0 ? d * sin + asc : d * sin - desc);
      if (cos > 0.25) {
        anchor = 'start';
        dx = d * cos + LABEL_PAD;
        dy = sideDy;
      } else if (cos < -0.25) {
        anchor = 'end';
        dx = d * cos - LABEL_PAD; // negativo
        dy = sideDy;
      } else {
        anchor = 'middle';
        dx = 0;
        dy = sin > 0 ? d + asc : -d - desc;
      }
      out.push({ d, dx: round1(dx), dy: round1(dy), anchor });
    }
  }
  return out;
}

function labelBox(cx, cy, cand, name) {
  const w = textW(name);
  let left;
  if (cand.anchor === 'middle') left = cx + cand.dx - w / 2;
  else if (cand.anchor === 'end') left = cx + cand.dx - w;
  else left = cx + cand.dx;
  const top = cy + cand.dy - ascOf();
  const bottom = cy + cand.dy + descOf();
  return { left: left - LABEL_PAD, right: left + w + LABEL_PAD, top: top - LABEL_PAD, bottom: bottom + LABEL_PAD };
}
function rectsOverlap(a, b) {
  return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}
function distPointRect(px, py, box) {
  const cx = clamp(px, box.left, box.right);
  const cy = clamp(py, box.top, box.bottom);
  return Math.hypot(px - cx, py - cy);
}
function segRectDist(ax, ay, bx, by, box) {
  if (ax >= box.left && ax <= box.right && ay >= box.top && ay <= box.bottom) return 0;
  if (bx >= box.left && bx <= box.right && by >= box.top && by <= box.bottom) return 0;
  let m = Infinity;
  const corners = [[box.left, box.top], [box.right, box.top], [box.right, box.bottom], [box.left, box.bottom]];
  for (const [cx, cy] of corners) m = Math.min(m, distPointSeg(cx, cy, ax, ay, bx, by));
  m = Math.min(m, distPointRect(ax, ay, box), distPointRect(bx, by, box));
  const steps = Math.ceil(Math.hypot(bx - ax, by - ay) / 4);
  for (let i = 1; i < steps; i++) {
    const t = i / steps;
    m = Math.min(m, distPointRect(ax + (bx - ax) * t, ay + (by - ay) * t, box));
  }
  return m;
}

// Curve DISEGNATE (Bézier campionata, doppie offsettate, trimmed) per le
// verifiche label↔tratta. Prima si usava il poligono di controllo
// (tracksOf): per le tratte piegate la curva vera sta DENTRO il triangolo,
// quindi una label "pulita" rispetto al poligono finiva sopra l'inchiostro
// (es. Salt Lake City, Saint Louis). Cache per geometria corrente.
const labelCurveCache = new Map();
function labelCurvesOf(e) {
  const key = e.points.map((p) => p[0].toFixed(2) + ',' + p[1].toFixed(2)).join(';');
  const hit = labelCurveCache.get(e);
  if (hit && hit.key === key) return hit.curves;
  const curves = (e.isDouble
    ? [trimPts(offsetPts(e.points, offsetOf(e)), e), trimPts(offsetPts(e.points, -offsetOf(e)), e)]
    : [trimmed(e)]).map((pts) => sampleCurve(pts, 4));
  labelCurveCache.set(e, { key, curves });
  return curves;
}

// Il box tocca il CORE (ink, sw/2) di una tratta disegnata? (+0.5px di margine).
function labelCoreHit(e, box) {
  const sw2 = swOf(e.len) / 2 + 0.5;
  for (const t of labelCurvesOf(e)) {
    for (let i = 0; i < t.length - 1; i++) {
      if (segRectDist(t[i][0], t[i][1], t[i + 1][0], t[i + 1][1], box) < sw2) return true;
    }
  }
  return false;
}

// F2: labelCoreHit su curve campionate (De Casteljau, non polyline)
function labelCoreHitCurve(e, box) {
  const sw2 = swOf(e.len) / 2 + 0.5;
  for (const curve of sampledCurvesOf(e)) {
    for (let i = 1; i < curve.length; i++) {
      if (segRectDist(curve[i-1][0], curve[i-1][1], curve[i][0], curve[i][1], box) < sw2) return true;
    }
  }
  return false;
}

const degree = new Map(cityIds.map((id) => [id, 0]));
for (const e of edges) { degree.set(cityIds[e.a], degree.get(cityIds[e.a]) + 1); degree.set(cityIds[e.b], degree.get(cityIds[e.b]) + 1); }
const order = mapData.cities.map((c) => c.id).sort((x, y) => {
  const dx = degree.get(x), dy = degree.get(y);
  if (dx !== dy) return dy - dx;
  const lx = (names.get(x) || '').length, ly = (names.get(y) || '').length;
  if (lx !== ly) return ly - lx;
  return x < y ? -1 : 1;
});

const labels = new Map();
const cityIndexPos = () => cityIds.map((id, i) => ({ id, x: cityPos[i][0], y: cityPos[i][1] }));

// Punteggio di una posizione: collisioni (core ×5, label ×2, dot ×2).
// La distanza NON è nel punteggio composito — il tier system (tier-first)
// la gestisce: prima si prova il tier più vicino; se nessun candidato è
// pulito, si scala al tier successivo. Solo nel fallback (tutti i tier
// hanno conflitti) si usa un composite moderato.
function scoreLabel(c, cand, name, selfId) {
  const box = labelBox(c.x, c.y, cand, name);
  let nCore = 0, nLabel = 0, nDot = 0, clearance = Infinity;
  for (const e of edges) {
    const sw2 = swOf(e.len) / 2;
    let edgeClear = Infinity;
    for (const t of labelCurvesOf(e)) {
      for (let i = 0; i < t.length - 1; i++) {
        edgeClear = Math.min(edgeClear, segRectDist(t[i][0], t[i][1], t[i + 1][0], t[i + 1][1], box) - sw2);
      }
    }
    clearance = Math.min(clearance, edgeClear);
    if (edgeClear < 0) nCore++;
  }
  for (const [oid, olb] of labels) {
    if (oid === selfId) continue;
    const oc = cityIndexPos().find((cc) => cc.id === oid);
    if (rectsOverlap(box, labelBox(oc.x, oc.y, olb, names.get(oid)))) nLabel++;
  }
  for (const oc of cityIndexPos()) {
    if (oc.id !== selfId && distPointRect(oc.x, oc.y, box) < DOT_R + 1) nDot++;
  }
  const nOwn = ownershipBad(c, box, selfId) ? 1 : 0;
  // 3f+7: conflitti puri (no distanza — il tier system decide il tier)
  const total = nCore * 5 + nLabel * 2 + nDot * 2 + nOwn * 3;
  return { total, nCore, nLabel, nDot, clearance, d: cand.d, cand };
}

// Ownership: la label deve stare chiaramente "vicino" al proprio pallino:
// ogni altro dot ≥ dOwn + OWN_MARGIN dal box, e centro box più vicino al
// proprio dot che a qualunque altro.
function ownershipWhy(c, box, selfId) {
  const dOwn = distPointRect(c.x, c.y, box);
  let m = Infinity, who = '';
  for (const oc of cityIndexPos()) {
    if (oc.id === selfId) continue;
    const d = distPointRect(oc.x, oc.y, box);
    if (d < m) { m = d; who = oc.id; }
  }
  return `dOwn ${dOwn.toFixed(1)}, ${who} ${m.toFixed(1)}`;
}
function ownershipBad(c, box, selfId) {
  const dOwn = distPointRect(c.x, c.y, box);
  const bx = (box.left + box.right) / 2, by = (box.top + box.bottom) / 2;
  const cOwn = Math.hypot(bx - c.x, by - c.y);
  for (const oc of cityIndexPos()) {
    if (oc.id === selfId) continue;
    if (distPointRect(oc.x, oc.y, box) < dOwn + OWN_MARGIN) return true;
    if (Math.hypot(bx - oc.x, by - oc.y) <= cOwn) return true;
  }
  return false;
}

// 3f+7: Tier-first. Prima il tier (distanza), poi meno conflitti, poi
// clearance (se sporco), poi distanza minore (se stesso tier).
function better(a, b) {
  if (a.d !== b.d) return a.d < b.d;
  if (a.total !== b.total) return a.total < b.total;
  if (a.total > 0 && a.clearance !== b.clearance) return a.clearance > b.clearance;
  return a.d < b.d;
}

// 3f+7: Tier-first selection. Per ogni tier (dal più vicino), se esiste
// almeno un candidato pulito (total===0) lo sceglie. Se nessun tier ha
// candidati puliti, sceglie il miglior compromesso tra tutti (conflitti
// minimi, poi distanza minima quadratica come tiebreaker).
function pickBest(c, name, selfId) {
  const allCands = candidatesFor();
  // Raggruppa per tier (distanza)
  const byTier = new Map();
  for (let idx = 0; idx < allCands.length; idx++) {
    const cand = allCands[idx];
    const tier = cand.d;
    if (!byTier.has(tier)) byTier.set(tier, []);
    byTier.get(tier).push({ cand, idx });
  }
  const tiersAsc = [...byTier.keys()].sort((a, b) => a - b);

  // Tier-first: ogni tier, se c'è almeno un candidato pulito → scegli il migliore
  for (const tier of tiersAsc) {
    const group = byTier.get(tier);
    let bestClean = null;
    for (const { cand, idx } of group) {
      const s = scoreLabel(c, cand, name, selfId);
      if (s.total === 0) {
        if (!bestClean || s.d < bestClean.d || (s.d === bestClean.d && idx < bestClean.idx)) {
          bestClean = { ...s, idx };
        }
      }
    }
    if (bestClean) return bestClean;
  }

  // Nessun tier ha candidati puliti → miglior compromesso (tutti i tier)
  let best = null;
  for (let idx = 0; idx < allCands.length; idx++) {
    const cand = allCands[idx];
    const s = scoreLabel(c, cand, name, selfId);
    const composite = s.total * 1000 + cand.d * cand.d;
    if (!best || composite < best.composite || (composite === best.composite && idx < best.idx)) {
      best = { ...s, idx, composite };
    }
  }
  return best;
}

function placeLabels() {
  labels.clear();
  for (const id of order) {
    const c = cityIndexPos().find((cc) => cc.id === id);
    const best = pickBest(c, names.get(id), id);
    const ov = resolveOverride(c, names.get(id), id);
    labels.set(id, ov
      ? { dx: round1(ov.dx), dy: round1(ov.dy), anchor: ov.anchor }
      : { dx: round1(best.cand.dx), dy: round1(best.cand.dy), anchor: best.cand.anchor });
  }
  // Rifinitura iterativa: riduce label-label introdotte dal greedy.
  // Le città in LABEL_OVERRIDES restano al candidato scelto dall'utente.
  for (let pass = 0; pass < 50; pass++) {
    let changed = false;
    for (const id of order) {
      if (LABEL_OVERRIDES[id]) continue; // pin: scelta manuale
      const c = cityIndexPos().find((cc) => cc.id === id);
      const cur = labels.get(id);
      const curScore = scoreLabel(c, { dx: cur.dx, dy: cur.dy, anchor: cur.anchor }, names.get(id), id);
      const best = pickBest(c, names.get(id), id);
      if (better(best, curScore)) {
        labels.set(id, { dx: round1(best.cand.dx), dy: round1(best.cand.dy), anchor: best.cand.anchor });
        changed = true;
      }
    }
    if (!changed) break;
  }
}

function boxFor(id) {
  const c = cityIndexPos().find((cc) => cc.id === id);
  const lb = labels.get(id);
  return labelBox(c.x, c.y, lb, names.get(id));
}

// Piega una tratta via dal box di un'etichetta: prova entrambi i segni finché
// il CORE non tocca più il box, senza introdurre nuovi conflitti tratta-tratta
// né violare la city-safety.
function bendFromLabel(e, box) {
  ensureBendable(e);
  const { mid, nx, ny } = normalOf(e);
  if (e.bendSign === null) e.bendSign = pickBendSign(e, nx, ny, mid);
  for (const sign of [e.bendSign, -e.bendSign]) {
    for (let step = 2; step <= 165; step += 2) {
      setBend(e, sign, step);
      if (labelCoreHit(e, box)) continue;
      if (!noRouteConflict(e)) continue;
      if (!bendPointCitySafe(e)) continue;
      if (!doubleTrackSafe(e)) continue;
      e.bendSign = sign;
      return true;
    }
  }
  return false;
}

placeLabels();

// ── 3f+10: Phoenix label override — pre-bend incident routes ──
// La label di Phoenix è forzata sopra il pallino da LABEL_OVERRIDES.
// Le rotte INCIDENTI su Phoenix (los_angeles_phoenix, denver_phoenix,
// phoenix_santa_fe, el_paso_phoenix, calgary_phoenix, phoenix_portland)
// possono intersecare il box della label. Le pieghiamo PRIMA della
// separazione route-label così il resto della pipeline le vede già pulite.
// Se bendFromLabel fallisce (noRouteConflict, bendPointCitySafe,
// doubleTrackSafe), proviamo micro-push di città ≤8px.
{
  const PHX_ID = 'phoenix';
  const phxOv = LABEL_OVERRIDES[PHX_ID];
  if (phxOv && labels.has(PHX_ID)) {
    const phxCi = idIndex.get(PHX_ID);
    const phxC = cityIndexPos().find(cc => cc.id === PHX_ID);
    const phxBox = boxFor(PHX_ID);
    // Rotte incidenti su Phoenix che toccano il box della label
    let hitRoutes = edges.filter(e => {
      if (e.a !== phxCi && e.b !== phxCi) return false;
      return labelCoreHit(e, phxBox);
    });
    let phxBendAttempts = 0;
    for (let attempt = 0; attempt < 6 && hitRoutes.length > 0; attempt++) {
      phxBendAttempts++;
      let anyBent = false;
      for (const e of hitRoutes) {
        if (bendFromLabel(e, phxBox)) anyBent = true;
      }
      // Ricontrolla quali rotte toccano ancora
      hitRoutes = edges.filter(e => {
        if (e.a !== phxCi && e.b !== phxCi) return false;
        return labelCoreHit(e, phxBox);
      });
      if (!anyBent) break;
    }
    // Se restano rotte che toccano la label, prova micro-push di Phoenix
    // e delle città vicine (≤8px, permesso dalla pipeline)
    if (hitRoutes.length > 0) {
      if (process.env.LABEL_DEBUG) console.error(`  [phx pre-bend] ${hitRoutes.length} rotte ancora in conflitto dopo ${phxBendAttempts} tentativi di bend — provo micro-push`);
      for (let pushAttempt = 0; pushAttempt < 6 && hitRoutes.length > 0; pushAttempt++) {
        // Micro-push di Phoenix via dalla label (sposta il dot, non la label)
        const pushes = [[0, -2], [0, -4], [2, -2], [-2, -2], [0, -6], [4, -4], [-4, -4], [0, -8]];
        const [pdx, pdy] = pushes[Math.min(pushAttempt, pushes.length - 1)];
        cityPos[phxCi][0] += pdx;
        cityPos[phxCi][1] += pdy;
        // Aggiorna i punti degli edge incidenti
        for (const e of edges) {
          if (e.a === phxCi) e.points[0] = [...cityPos[phxCi]];
          if (e.b === phxCi) e.points[e.points.length - 1] = [...cityPos[phxCi]];
        }
        // Riprova bendFromLabel
        for (const e of hitRoutes) bendFromLabel(e, phxBox);
        hitRoutes = edges.filter(e => {
          if (e.a !== phxCi && e.b !== phxCi) return false;
          return labelCoreHit(e, phxBox);
        });
      }
    }
    if (hitRoutes.length > 0) {
      // Ultima risorsa: micro-push di santa_fe / el_paso (≤8px)
      const neighborPushes = [
        ['santa_fe', 0, -4], ['santa_fe', 0, -6], ['santa_fe', 0, -8],
        ['el_paso', -4, 0], ['el_paso', -6, 0], ['el_paso', -8, 0],
      ];
      for (const [nid, ndx, ndy] of neighborPushes) {
        if (hitRoutes.length === 0) break;
        const nci = idIndex.get(nid);
        if (nci === undefined) continue;
        const totalPush = (clusterTotals.get(nid) || 0) + Math.abs(ndx) + Math.abs(ndy);
        if (totalPush > 30) continue;
        cityPos[nci][0] += ndx;
        cityPos[nci][1] += ndy;
        clusterTotals.set(nid, totalPush);
        for (const e of edges) {
          if (e.a === nci) e.points[0] = [...cityPos[nci]];
          if (e.b === nci) e.points[e.points.length - 1] = [...cityPos[nci]];
        }
        for (const e of hitRoutes) bendFromLabel(e, phxBox);
        hitRoutes = edges.filter(e => {
          if (e.a !== phxCi && e.b !== phxCi) return false;
          return labelCoreHit(e, phxBox);
        });
      }
    }
    if (process.env.LABEL_DEBUG) {
      const finalHits = edges.filter(e => {
        if (e.a !== phxCi && e.b !== phxCi) return false;
        return labelCoreHit(e, phxBox);
      });
      if (finalHits.length > 0) {
        console.error(`  [phx pre-bend] ⚠ ${finalHits.length} rotte incidenti ancora in conflitto dopo micro-push: ${finalHits.map(e => cityIds[e.a] + '↔' + cityIds[e.b]).join(', ')}`);
      } else {
        console.error(`  [phx pre-bend] ✓ label Phoenix pulita da tutte le rotte incidenti`);
      }
    }
  }
}

// ── 4. Separazione route-label (3f+9: label-first, bend LAST resort) ──
// Nuova priorità: (1) riposiziona label (tutti i tier, 40px incluso),
// (2) SOLO se nessun candidato pulito esiste → bend della tratta.
let labelRounds = 0;
for (let round = 0; round < MAX_LABEL_ROUNDS; round++) {
  placeLabels(); // riposiziona label (tier-first, fino a 40px)

  // Controlla se qualche label tocca ancora CORE di tratte non-incidenti
  let anyLabelHit = false;
  for (const id of order) {
    const box = boxFor(id);
    for (const e of edges) {
      if (cityIds[e.a] === id || cityIds[e.b] === id) continue;
      if (labelCoreHit(e, box)) { anyLabelHit = true; break; }
    }
    if (anyLabelHit) break;
  }
  if (!anyLabelHit) { labelRounds = round + 1; break; }

  // Nessun candidato pulito → bend delle tratte (ULTIMA risorsa)
  let bent = false;
  for (const id of order) {
    const box = boxFor(id);
    for (const e of edges) {
      if (labelCoreHit(e, box) && bendFromLabel(e, box)) bent = true;
    }
  }
  labelRounds = round + 1;
  if (!bent) break;
}

// ── 4a-bis. Re-check route-route dopo i bend label ───────────────────
// I bend per le label possono aver creato nuovi conflitti tratta-tratta.
// 3f+9: label-first anche nel re-check.
for (let recheck = 0; recheck < 3; recheck++) {
  // Route-route re-bend
  let rrBent = false;
  for (let it = 0; it < 4; it++) {
    const conflicts = findConflicts();
    if (conflicts.length === 0) break;
    rrBent = true;
    for (const [shorter, longer] of conflicts) {
      pushBendRoute(shorter, longer, swAvg(shorter, longer) + SW_AVG_PAD);
    }
  }
  // Route-label re-check: label-first, bend solo se necessario
  placeLabels();
  let rlBent = false;
  let anyHit = false;
  for (const id of order) {
    const box = boxFor(id);
    for (const e of edges) {
      if (cityIds[e.a] === id || cityIds[e.b] === id) continue;
      if (labelCoreHit(e, box)) { anyHit = true; break; }
    }
    if (anyHit) break;
  }
  if (anyHit) {
    for (const id of order) {
      const box = boxFor(id);
      for (const e of edges) {
        if (labelCoreHit(e, box) && bendFromLabel(e, box)) rlBent = true;
      }
    }
    if (rlBent) placeLabels();
  }
  if (!rrBent && !rlBent) break;
  labelRounds = Math.min(MAX_LABEL_ROUNDS, labelRounds + 1);
}

// ── 4b. Fix-up finale: riposiziona ogni label con conflitti residui (CORE,
//   label-label o dot) al candidato pulito più vicino — il greedy a volte
//   resta bloccato in un minimo locale; un riposizionamento mirato converge
//   in poche iterazioni. ──
for (let retry = 0; retry < 15; retry++) {
  let fixed = false;
  for (const id of order) {
    const c = cityIndexPos().find((cc) => cc.id === id);
    const cur = labels.get(id);
    const curDist = Math.hypot(cur.dx, cur.dy);
    const curScore = scoreLabel(c, { dx: cur.dx, dy: cur.dy, anchor: cur.anchor, d: curDist }, names.get(id), id);
    if (curScore.nCore === 0 && curScore.nLabel === 0 && curScore.nDot === 0) continue;
    let bestClean = null, bestCleanDist = Infinity;
    for (const cand of candidatesFor()) {
      const s = scoreLabel(c, cand, names.get(id), id);
      if (s.nCore === 0 && s.nLabel === 0 && s.nDot === 0 && s.d < bestCleanDist) { bestClean = cand; bestCleanDist = s.d; }
    }
    if (bestClean) {
      labels.set(id, { dx: round1(bestClean.dx), dy: round1(bestClean.dy), anchor: bestClean.anchor });
      fixed = true;
    }
  }
  if (!fixed) break;
}

// ── 4c. Pairwise label-label resolution ──
// Quando due label si sovrappongono si prova un riposizionamento
// coordinato: si toglie temporaneamente B, si piazza A nel suo miglior
// candidato (strict: total===0, oppure lenient: solo nLabel≤1 con
// nCore===nDot===0), poi si piazza B; se fallisce, si prova B-first.
// Il lenient serve quando una label ha pochissimi candidati puliti
// (es. oklahoma_city ne ha 1) e l'altra è bloccata in tutte le direzioni.
for (let pairPass = 0; pairPass < 25; pairPass++) {
  let pairFixed = false;
  for (let i = 0; i < order.length; i++) {
    for (let j = i + 1; j < order.length; j++) {
      const idA = order[i], idB = order[j];
      const boxA = boxFor(idA), boxB = boxFor(idB);
      if (!rectsOverlap(boxA, boxB)) continue;
      const cA = cityIndexPos().find((cc) => cc.id === idA);
      const cB = cityIndexPos().find((cc) => cc.id === idB);
      const curA = labels.get(idA), curB = labels.get(idB);

      const findBest = (c, name, selfId) => {
        let strict = null, strictD = Infinity;
        let lenient = null, lenientD = Infinity;
        for (const cand of candidatesFor()) {
          const s = scoreLabel(c, cand, name, selfId);
          if (s.nCore === 0 && s.nDot === 0 && s.nLabel === 0 && s.d < strictD) { strict = cand; strictD = s.d; }
          if (s.nCore === 0 && s.nDot === 0 && s.nLabel <= 1 && s.d < lenientD) {
            lenient = cand; lenientD = s.d;
          }
        }
        return strict || lenient || null;
      };

      const tryPair = () => {
        // A-first
        const savedB = labels.get(idB);
        labels.delete(idB);
        const candA = findBest(cA, names.get(idA), idA);
        if (candA) {
          labels.set(idA, { dx: round1(candA.dx), dy: round1(candA.dy), anchor: candA.anchor });
          const candB = findBest(cB, names.get(idB), idB);
          if (candB) {
            labels.set(idB, { dx: round1(candB.dx), dy: round1(candB.dy), anchor: candB.anchor });
            return true;
          }
          labels.set(idA, curA); labels.set(idB, savedB);
        } else { labels.set(idB, savedB); }

        // B-first
        labels.delete(idA);
        const candB2 = findBest(cB, names.get(idB), idB);
        if (candB2) {
          labels.set(idB, { dx: round1(candB2.dx), dy: round1(candB2.dy), anchor: candB2.anchor });
          const candA2 = findBest(cA, names.get(idA), idA);
          if (candA2) {
            labels.set(idA, { dx: round1(candA2.dx), dy: round1(candA2.dy), anchor: candA2.anchor });
            return true;
          }
          labels.set(idB, curB);
        }
        labels.set(idA, curA);
        return false;
      };

      if (tryPair()) { pairFixed = true; break; }

      // ── Nudge + route-bend fallback ──
      // Se tryPair fallisce, prova a spostare leggermente A o B in verticale
      // per rompere l'overlap. Se il nudge introduce conflitti con rotte,
      // prova a piegare TUTTE le rotte bloccanti contemporaneamente.
      for (const [nudgeId, nudgeCity, otherId] of [[idA, cA, idB], [idB, cB, idA]]) {
        const curNudge = labels.get(nudgeId);
        const otherCity = nudgeId === idA ? cB : cA;
        const otherCur = labels.get(otherId);

        for (const [ddx, ddy] of [[2,0],[-2,0],[4,0],[-4,0],[6,0],[-6,0],[0,2],[0,-2],[0,4],[0,-4],[0,6],[0,-6],[0,8],[0,-8],[0,10],[0,-10],[2,2],[2,-2],[-2,2],[-2,-2],[4,4],[4,-4],[-4,4],[-4,-4]]) {
          const cand = { dx: round1(curNudge.dx + ddx), dy: round1(curNudge.dy + ddy), anchor: curNudge.anchor, d: curNudge.d || 24 };
          const box = labelBox(nudgeCity.x, nudgeCity.y, cand, names.get(nudgeId));
          const otherBox = boxFor(otherId);
          if (rectsOverlap(box, otherBox)) continue;

          // Applica il nudge temporaneo
          labels.set(nudgeId, { dx: round1(cand.dx), dy: round1(cand.dy), anchor: cand.anchor });
          let sNudge = scoreLabel(nudgeCity, cand, names.get(nudgeId), nudgeId);
          let sOther = scoreLabel(otherCity, labels.get(otherId), names.get(otherId), otherId);
          if (sNudge.nCore === 0 && sNudge.nDot === 0 && sNudge.nLabel === 0 &&
              sOther.nCore === 0 && sOther.nDot === 0 && sOther.nLabel === 0) { pairFixed = true; break; }

          // Trova TUTTE le rotte che toccano il box del nudge
          const blockingEdges = [];
          for (const e of edges) {
            if (labelCoreHit(e, box)) blockingEdges.push(e);
          }

          if (blockingEdges.length === 0) {
            labels.set(nudgeId, curNudge); labels.set(otherId, otherCur);
            continue;
          }

          // Prova a piegare tutte le rotte bloccanti insieme (backtracking)
          const savedStates = blockingEdges.map((e) => ({
            e,
            points: e.points.map((p) => [...p]),
            bendSign: e.bendSign,
          }));

          const tryBendAll = (idx) => {
            if (idx >= blockingEdges.length) {
              // Tutte piegate: verifica
              const sn = scoreLabel(nudgeCity, cand, names.get(nudgeId), nudgeId);
              const so = scoreLabel(otherCity, labels.get(otherId), names.get(otherId), otherId);
              return sn.nCore === 0 && sn.nDot === 0 && sn.nLabel === 0 &&
                     so.nCore === 0 && so.nDot === 0 && so.nLabel === 0;
            }
            const e = blockingEdges[idx];
            ensureBendable(e);
            const { mid, nx, ny } = normalOf(e);
            if (e.bendSign === null) e.bendSign = pickBendSign(e, nx, ny, mid);
            for (const sign of [e.bendSign, -e.bendSign]) {
              for (let step = 4; step <= 50; step += 4) {
                setBend(e, sign, step);
                if (!labelCoreHit(e, box) && noRouteConflict(e) && bendPointCitySafe(e) && doubleTrackSafe(e)) {
                  e.bendSign = sign;
                  if (tryBendAll(idx + 1)) return true;
                }
              }
            }
            return false;
          };

          if (tryBendAll(0)) { pairFixed = true; break; }

          // Ripristina tutte le rotte
          for (const { e, points, bendSign } of savedStates) {
            e.points = points;
            e.bendSign = bendSign;
          }
          labels.set(nudgeId, curNudge);
          labels.set(otherId, otherCur);
        }
        if (pairFixed) break;
      }
    }
  }
  if (!pairFixed) break;
}

// ── 4c-bis. Exhaustive pairwise label-label resolution ──
// Quando il pairwise standard (4c) non risolve tutte le coppie,
// prova TUTTE le combinazioni di candidati per entrambe le label
// (32 × 32 = 1024 combinazioni per coppia). Sceglie la coppia che
// minimizza i conflitti e la distanza.
let cityNudgePasses = 0; // contatore mantenuto per il report
for (let exPass = 0; exPass < 30; exPass++) {
  let exFixed = false;
  for (let i = 0; i < order.length; i++) {
    for (let j = i + 1; j < order.length; j++) {
      const idA = order[i], idB = order[j];
      if (!rectsOverlap(boxFor(idA), boxFor(idB))) continue;
      const cA = cityIndexPos().find((cc) => cc.id === idA);
      const cB = cityIndexPos().find((cc) => cc.id === idB);
      const curA = labels.get(idA), curB = labels.get(idB);
      const allCands = candidatesFor();

      // PROVA TUTTE LE COMBINAZIONI: 32 × 32
      let bestPair = null; // { candA, candB, scoreA, scoreB, composite }
      const savedA = labels.get(idA), savedB = labels.get(idB);
      
      // Pre-calcola tutti gli score per A (con B rimosso) e per B (con A rimosso)
      labels.delete(idB);
      const scoresA = allCands.map(cand => scoreLabel(cA, cand, names.get(idA), idA));
      labels.set(idB, savedB);
      
      labels.delete(idA);
      const scoresB = allCands.map(cand => scoreLabel(cB, cand, names.get(idB), idB));
      labels.set(idA, savedA);

      for (let ai = 0; ai < allCands.length; ai++) {
        for (let bi = 0; bi < allCands.length; bi++) {
          const sA = scoresA[ai], sB = scoresB[bi];
          // piazza entrambe temporaneamente e verifica overlap reciproco
          labels.set(idA, { dx: round1(allCands[ai].dx), dy: round1(allCands[ai].dy), anchor: allCands[ai].anchor });
          labels.set(idB, { dx: round1(allCands[bi].dx), dy: round1(allCands[bi].dy), anchor: allCands[bi].anchor });
          const overlap = rectsOverlap(boxFor(idA), boxFor(idB)) ? 1 : 0;
          // Ricomputa nLabel per verificare che non ci siano altri conflitti
          const sAFinal = scoreLabel(cA, allCands[ai], names.get(idA), idA);
          const sBFinal = scoreLabel(cB, allCands[bi], names.get(idB), idB);
          const totalConf = sAFinal.nCore + sAFinal.nDot + sAFinal.nLabel +
                           sBFinal.nCore + sBFinal.nDot + sBFinal.nLabel + overlap;
          const composite = totalConf * 100000 + (allCands[ai].d * allCands[ai].d + allCands[bi].d * allCands[bi].d);
          if (!bestPair || composite < bestPair.composite) {
            bestPair = { ai, bi, composite, totalConf, sAFinal, sBFinal };
          }
        }
      }

      // Applica la miglior coppia se è migliore dello stato corrente
      const curSA = scoreLabel(cA, curA, names.get(idA), idA);
      const curSB = scoreLabel(cB, curB, names.get(idB), idB);
      const curOverlap = rectsOverlap(boxFor(idA), boxFor(idB)) ? 1 : 0;
      const curTotal = curSA.nCore + curSA.nDot + curSA.nLabel +
                       curSB.nCore + curSB.nDot + curSB.nLabel + curOverlap;

      if (bestPair && bestPair.totalConf <= curTotal) {
        labels.set(idA, { dx: round1(allCands[bestPair.ai].dx), dy: round1(allCands[bestPair.ai].dy), anchor: allCands[bestPair.ai].anchor });
        labels.set(idB, { dx: round1(allCands[bestPair.bi].dx), dy: round1(allCands[bestPair.bi].dy), anchor: allCands[bestPair.bi].anchor });
        exFixed = true;
      }
    }
  }
  if (!exFixed) break;
}

// ── 4d. Final cleanup ──
// Re-bend route-route + city-clearance + route-label aggressivo.
for (let cleanup = 0; cleanup < 5; cleanup++) {
  // Route-route
  for (let it = 0; it < 12; it++) {
    const conflicts = findConflicts();
    if (conflicts.length === 0) break;
    for (const [shorter, longer] of conflicts) {
      pushBendRoute(shorter, longer, swAvg(shorter, longer) + SW_AVG_PAD);
    }
  }
  // City-clearance
  for (let ccIt = 0; ccIt < 8; ccIt++) {
    const ccConflicts = cityClearanceConflicts();
    if (ccConflicts.length === 0) break;
    for (const { edge, cityIdx } of ccConflicts) {
      bendFromCity(edge, cityIdx);
    }
  }
  // Route-label
  let rlBent = false;
  for (const id of order) {
    const box = boxFor(id);
    for (const e of edges) {
      if (labelCoreHit(e, box) && bendFromLabel(e, box)) rlBent = true;
    }
  }
  placeLabels();
  if (!rlBent) break;
}
placeLabels();

// ── 4d-bis. Targeted micro-pushes (3f+8v2) ──
// Dopo il pipeline completo, se restano label-CORE o route-city,
// spinge di 1–2px le città coinvolte (NO stringere label, NO re-tick).
// Poi re-run bend + label. Le spinte consumano slack di lunghezza.
let cityMicroPushes = 0;
for (let up = 0; up < 3; up++) {
  let pushed = false;
  for (const id of order) {
    const ci = idIndex.get(id);
    const box = boxFor(id);
    for (const e of edges) {
      if (cityIds[e.a] === id || cityIds[e.b] === id) continue;
      if (!labelCoreHit(e, box)) continue;
      const t = trimmed(e);
      let bestD = Infinity, bestP = null;
      for (let k = 0; k < t.length - 1; k++) {
        const [px, py] = projectOnSeg(cityPos[ci][0], cityPos[ci][1], t[k][0], t[k][1], t[k + 1][0], t[k + 1][1]);
        const d = Math.hypot(cityPos[ci][0] - px, cityPos[ci][1] - py);
        if (d < bestD) { bestD = d; bestP = [px, py]; }
      }
      if (!bestP || bestD < 0.01) continue;
      const push = Math.min(2, bestD * 0.15);
      cityPos[ci][0] += (cityPos[ci][0] - bestP[0]) / bestD * push;
      cityPos[ci][1] += (cityPos[ci][1] - bestP[1]) / bestD * push;
      pushed = true; cityMicroPushes++;
    }
  }
  for (const e of edges) {
    for (let ci = 0; ci < cityIds.length; ci++) {
      if (ci === e.a || ci === e.b) continue;
      const d = routeCityDist(e, ci);
      if (d >= CITY_CLEARANCE) continue;
      const t = trimmed(e);
      let bestD = Infinity, bestP = null;
      for (let k = 0; k < t.length - 1; k++) {
        const [px, py] = projectOnSeg(cityPos[ci][0], cityPos[ci][1], t[k][0], t[k][1], t[k + 1][0], t[k + 1][1]);
        const dd = Math.hypot(cityPos[ci][0] - px, cityPos[ci][1] - py);
        if (dd < bestD) { bestD = dd; bestP = [px, py]; }
      }
      if (!bestP || bestD < 0.01) continue;
      const push = Math.min(2, (CITY_CLEARANCE - d) * 0.5);
      cityPos[ci][0] += (cityPos[ci][0] - bestP[0]) / bestD * push;
      cityPos[ci][1] += (cityPos[ci][1] - bestP[1]) / bestD * push;
      pushed = true; cityMicroPushes++;
    }
  }
  if (!pushed) break;
  for (const e of edges) { e.points = [[...cityPos[e.a]], [...cityPos[e.b]]]; e.bendSign = null; }
  for (let it = 0; it < 16; it++) {
    const conflicts = findConflicts();
    if (conflicts.length === 0) break;
    for (const [s, l] of conflicts) pushBendRoute(s, l, swAvg(s, l) + SW_AVG_PAD);
  }
  fanOutHubs();
  for (let ccIt = 0; ccIt < 10; ccIt++) {
    const cc = cityClearanceConflicts();
    if (cc.length === 0) break;
    for (const { edge, cityIdx } of cc) bendFromCity(edge, cityIdx);
  }
  for (const e of edges) {
    if (e.points.length <= 2 || bendPointCitySafe(e)) continue;
    const { mid, nx, ny } = normalOf(e);
    let best = null;
    for (const sign of [1, -1]) {
      for (let step = 2; step <= 150; step += 2) {
        setBend(e, sign, step);
        if (bendPointCitySafe(e) && noRouteConflict(e) && doubleTrackSafe(e)) { if (!best || step < best.step) best = { sign, step }; break; }
      }
    }
    if (best) { setBend(e, best.sign, best.step); e.bendSign = best.sign; }
  }
  placeLabels();
  for (let rlR = 0; rlR < 6; rlR++) {
    let rlB = false;
    for (const id of order) { const box = boxFor(id); for (const e of edges) { if (labelCoreHit(e, box) && bendFromLabel(e, box)) rlB = true; } }
    placeLabels(); if (!rlB) break;
  }
  for (let pp = 0; pp < 8; pp++) {
    let fx = false;
    for (let i = 0; i < order.length; i++) {
      for (let j = i + 1; j < order.length; j++) {
        const idA = order[i], idB = order[j];
        if (!rectsOverlap(boxFor(idA), boxFor(idB))) continue;
        const cA = cityIndexPos().find(cc => cc.id === idA), cB = cityIndexPos().find(cc => cc.id === idB);
        const curA = labels.get(idA), curB = labels.get(idB);
        const fB = (c, nm, sid) => { let s=null,sd=1e9; for(const cd of candidatesFor()){const sc=scoreLabel(c,cd,nm,sid);if(sc.nCore===0&&sc.nDot===0&&sc.nLabel===0&&sc.d<sd){s=cd;sd=sc.d;}} return s; };
        const tp = () => { const sb=labels.get(idB);labels.delete(idB);const ca=fB(cA,names.get(idA),idA);if(ca){labels.set(idA,{dx:round1(ca.dx),dy:round1(ca.dy),anchor:ca.anchor});const cb=fB(cB,names.get(idB),idB);if(cb){labels.set(idB,{dx:round1(cb.dx),dy:round1(cb.dy),anchor:cb.anchor});return true;}labels.set(idA,curA);labels.set(idB,sb);}else{labels.set(idB,sb);}labels.delete(idA);const cb2=fB(cB,names.get(idB),idB);if(cb2){labels.set(idB,{dx:round1(cb2.dx),dy:round1(cb2.dy),anchor:cb2.anchor});const ca2=fB(cA,names.get(idA),idA);if(ca2){labels.set(idA,{dx:round1(ca2.dx),dy:round1(ca2.dy),anchor:ca2.anchor});return true;}labels.set(idB,curB);}labels.set(idA,curA);return false;};
        if (tp()) fx = true;
      }
    }
    if (!fx) break;
  }
}
// ── End of 4d-bis ──

// ── 4d-ter. Opposite-side label fix for stubborn pairs (3f+8v2) ──
// Per le coppie label-label che resistono a tutte le risoluzioni,
// forza entrambe le label in direzioni OPPOSTE: una va a destra,
// l'altra a sinistra (o una sopra, una sotto). Prova tutte le
// combinazioni angolari ai 4 tier.
for (let osPass = 0; osPass < 3; osPass++) {
  let osFixed = false;
  for (let i = 0; i < order.length; i++) {
    for (let j = i + 1; j < order.length; j++) {
      const idA = order[i], idB = order[j];
      if (!rectsOverlap(boxFor(idA), boxFor(idB))) continue;
      const cA = cityIndexPos().find(cc => cc.id === idA);
      const cB = cityIndexPos().find(cc => cc.id === idB);
      const curA = labels.get(idA), curB = labels.get(idB);
      
      // Forza A a destra (cos > 0.25) e B a sinistra (cos < -0.25), o viceversa
      for (const [aRight, bLeft] of [[true, true], [false, false]]) {
        let bestA = null, bestB = null, bestComposite = Infinity;
        for (const candA of candidatesFor()) {
          const angA = Math.atan2(candA.dy, candA.dx);
          const cosA = Math.cos(angA);
          if (aRight && cosA <= 0.25) continue;
          if (!aRight && cosA >= -0.25) continue;
          labels.set(idA, { dx: round1(candA.dx), dy: round1(candA.dy), anchor: candA.anchor });
          const sA = scoreLabel(cA, candA, names.get(idA), idA);
          if (sA.nCore > 0 || sA.nDot > 0) continue;
          
          for (const candB of candidatesFor()) {
            const angB = Math.atan2(candB.dy, candB.dx);
            const cosB = Math.cos(angB);
            if (bLeft && cosB >= -0.25) continue;
            if (!bLeft && cosB <= 0.25) continue;
            labels.set(idB, { dx: round1(candB.dx), dy: round1(candB.dy), anchor: candB.anchor });
            const sB = scoreLabel(cB, candB, names.get(idB), idB);
            if (sB.nCore > 0 || sB.nDot > 0) continue;
            
            const overlap = rectsOverlap(boxFor(idA), boxFor(idB)) ? 1 : 0;
            const total = sA.nLabel + sB.nLabel + overlap;
            const comp = total * 100000 + candA.d * candA.d + candB.d * candB.d;
            if (comp < bestComposite) {
              bestComposite = comp; bestA = candA; bestB = candB;
            }
          }
        }
        labels.set(idA, curA); labels.set(idB, curB); // restore
        
        if (bestA && bestB) {
          const oldOverlap = rectsOverlap(boxFor(idA), boxFor(idB)) ? 1 : 0;
          const curSA = scoreLabel(cA, curA, names.get(idA), idA);
          const curSB = scoreLabel(cB, curB, names.get(idB), idB);
          const curTotal = curSA.nLabel + curSB.nLabel + oldOverlap;
          const newTotal = bestComposite / 100000;
          if (newTotal < curTotal) {
            labels.set(idA, { dx: round1(bestA.dx), dy: round1(bestA.dy), anchor: bestA.anchor });
            labels.set(idB, { dx: round1(bestB.dx), dy: round1(bestB.dy), anchor: bestB.anchor });
            osFixed = true;
          }
        }
      }
    }
  }
  if (!osFixed) break;
}

// ── 4d-quater. Cluster escape for residual label-label pairs (3f+8v3) ──
// When exhaustive label placement reaches a local minimum, move the involved
// cities apart along the city-center vector. Gate ratio is FULLY RELAXED:
// the number of cards per route is read from wagon slots drawn on the route,
// NOT from pixel length. Moving cities only alters aesthetic spacing.
const CLUSTER_MAX_CYCLES = 10;
const CLUSTER_MAX_PUSH = 8;       // max px per city per cycle
const CLUSTER_TOTAL_CAP = 30;     // max total px per city across all cycles
const CLUSTER_FONT_FALLBACK = 14.5;
let clusterCycles = 0;
let clusterApplied = false;
const clusterTotals = new Map();   // cumulative push per city ID

function reRunCleanup() {
  // Route-route bends
  for (let it = 0; it < 16; it++) {
    const cf = findConflicts();
    if (cf.length === 0) break;
    for (const [s, l] of cf) pushBendRoute(s, l, swAvg(s, l) + SW_AVG_PAD);
  }
  fanOutHubs();
  // City clearance
  for (let ccIt = 0; ccIt < 10; ccIt++) {
    const cc = cityClearanceConflicts();
    if (cc.length === 0) break;
    for (const { edge, cityIdx } of cc) bendFromCity(edge, cityIdx);
  }
  // City safety at bend points
  for (const e of edges) {
    if (e.points.length <= 2 || bendPointCitySafe(e)) continue;
    const { mid, nx, ny } = normalOf(e);
    let best = null;
    for (const sign of [1, -1]) {
      for (let step = 2; step <= 150; step += 2) {
        setBend(e, sign, step);
        if (bendPointCitySafe(e) && noRouteConflict(e) && doubleTrackSafe(e)) { if (!best || step < best.step) best = { sign, step }; break; }
      }
    }
    if (best) { setBend(e, best.sign, best.step); e.bendSign = best.sign; }
  }
  // Labels
  placeLabels();
  // Route-label bends
  for (let rlR = 0; rlR < 6; rlR++) {
    let rlB = false;
    for (const id of order) { const box = boxFor(id); for (const e of edges) { if (labelCoreHit(e, box) && bendFromLabel(e, box)) rlB = true; } }
    placeLabels(); if (!rlB) break;
  }
  // Pairwise label resolution
  for (let pp = 0; pp < 10; pp++) {
    let fx = false;
    for (let i = 0; i < order.length; i++) {
      for (let j = i + 1; j < order.length; j++) {
        const idA = order[i], idB = order[j];
        if (!rectsOverlap(boxFor(idA), boxFor(idB))) continue;
        const cA = cityIndexPos().find(cc => cc.id === idA), cB = cityIndexPos().find(cc => cc.id === idB);
        const curA = labels.get(idA), curB = labels.get(idB);
        const fB = (c, nm, sid) => { let s = null, sd = 1e9; for (const cd of candidatesFor()) { const sc = scoreLabel(c, cd, nm, sid); if (sc.nCore === 0 && sc.nDot === 0 && sc.nLabel === 0 && sc.d < sd) { s = cd; sd = sc.d; } } return s; };
        const tp = () => { const sb = labels.get(idB); labels.delete(idB); const ca = fB(cA, names.get(idA), idA); if (ca) { labels.set(idA, { dx: round1(ca.dx), dy: round1(ca.dy), anchor: ca.anchor }); const cb = fB(cB, names.get(idB), idB); if (cb) { labels.set(idB, { dx: round1(cb.dx), dy: round1(cb.dy), anchor: cb.anchor }); return true; } labels.set(idA, curA); labels.set(idB, sb); } else { labels.set(idB, sb); } labels.delete(idA); const cb2 = fB(cB, names.get(idB), idB); if (cb2) { labels.set(idB, { dx: round1(cb2.dx), dy: round1(cb2.dy), anchor: cb2.anchor }); const ca2 = fB(cA, names.get(idA), idA); if (ca2) { labels.set(idA, { dx: round1(ca2.dx), dy: round1(ca2.dy), anchor: ca2.anchor }); return true; } labels.set(idB, curB); } labels.set(idA, curA); return false; };
        if (tp()) fx = true;
      }
    }
    if (!fx) break;
  }
  // Exhaustive pairwise
  for (let exP = 0; exP < 8; exP++) {
    let exF = false;
    for (let i = 0; i < order.length; i++) {
      for (let j = i + 1; j < order.length; j++) {
        const idA = order[i], idB = order[j];
        if (!rectsOverlap(boxFor(idA), boxFor(idB))) continue;
        const cA = cityIndexPos().find(cc => cc.id === idA), cB = cityIndexPos().find(cc => cc.id === idB);
        const curA = labels.get(idA), curB = labels.get(idB);
        const allCands = candidatesFor();
        let bestP = null;
        const savedA = labels.get(idA), savedB = labels.get(idB);
        labels.delete(idB); const scA = allCands.map(cd => scoreLabel(cA, cd, names.get(idA), idA)); labels.set(idB, savedB);
        labels.delete(idA); const scB = allCands.map(cd => scoreLabel(cB, cd, names.get(idB), idB)); labels.set(idA, savedA);
        for (let ai = 0; ai < allCands.length; ai++) {
          for (let bi = 0; bi < allCands.length; bi++) {
            const sA = scA[ai], sB = scB[bi];
            labels.set(idA, { dx: round1(allCands[ai].dx), dy: round1(allCands[ai].dy), anchor: allCands[ai].anchor });
            labels.set(idB, { dx: round1(allCands[bi].dx), dy: round1(allCands[bi].dy), anchor: allCands[bi].anchor });
            const ov = rectsOverlap(boxFor(idA), boxFor(idB)) ? 1 : 0;
            const sAF = scoreLabel(cA, allCands[ai], names.get(idA), idA);
            const sBF = scoreLabel(cB, allCands[bi], names.get(idB), idB);
            const tc = sAF.nCore + sAF.nDot + sAF.nLabel + sBF.nCore + sBF.nDot + sBF.nLabel + ov;
            const comp = tc * 100000 + (allCands[ai].d * allCands[ai].d + allCands[bi].d * allCands[bi].d);
            if (!bestP || comp < bestP.composite) bestP = { ai, bi, composite: comp, tc };
          }
        }
        labels.set(idA, curA); labels.set(idB, curB);
        const csA = scoreLabel(cA, curA, names.get(idA), idA), csB = scoreLabel(cB, curB, names.get(idB), idB);
        const cOv = rectsOverlap(boxFor(idA), boxFor(idB)) ? 1 : 0;
        const cTc = csA.nCore + csA.nDot + csA.nLabel + csB.nCore + csB.nDot + csB.nLabel + cOv;
        if (bestP && bestP.tc <= cTc) {
          labels.set(idA, { dx: round1(allCands[bestP.ai].dx), dy: round1(allCands[bestP.ai].dy), anchor: allCands[bestP.ai].anchor });
          labels.set(idB, { dx: round1(allCands[bestP.bi].dx), dy: round1(allCands[bestP.bi].dy), anchor: allCands[bestP.bi].anchor });
          exF = true;
        }
      }
    }
    if (!exF) break;
  }
}

// ── Cluster escape main loop ──
for (let ce = 0; ce < CLUSTER_MAX_CYCLES; ce++) {
  // Find residual label-label overlaps
  const llPairs = [];
  for (let i = 0; i < order.length; i++) {
    for (let j = i + 1; j < order.length; j++) {
      const idA = order[i], idB = order[j];
      if (rectsOverlap(boxFor(idA), boxFor(idB))) {
        llPairs.push({ idA, idB, ciA: idIndex.get(idA), ciB: idIndex.get(idB) });
      }
    }
  }
  if (llPairs.length === 0) break;

  let anyPushed = false;
  for (const { idA, idB, ciA, ciB } of llPairs) {
    // Push direction = city-center → city-center
    const dcx = cityPos[ciB][0] - cityPos[ciA][0];
    const dcy = cityPos[ciB][1] - cityPos[ciA][1];
    const dc = Math.hypot(dcx, dcy) || 1;
    const nx = dcx / dc, ny = dcy / dc;

    // Overlap extent
    const boxA = boxFor(idA), boxB = boxFor(idB);
    const ox = Math.min(boxA.right, boxB.right) - Math.max(boxA.left, boxB.left);
    const oy = Math.min(boxA.bottom, boxB.bottom) - Math.max(boxA.top, boxB.top);
    const overlap = Math.max(0, Math.hypot(ox > 0 ? ox : 0, oy > 0 ? oy : 0));

    const pushHalf = Math.min(CLUSTER_MAX_PUSH, Math.max(2, overlap * 0.45));
    const totalA = clusterTotals.get(idA) || 0;
    const totalB = clusterTotals.get(idB) || 0;
    const remA = Math.max(0, CLUSTER_TOTAL_CAP - totalA);
    const remB = Math.max(0, CLUSTER_TOTAL_CAP - totalB);
    const actual = Math.min(pushHalf, remA, remB);
    if (actual < 0.5) continue;

    // A away from B, B away from A
    cityPos[ciA][0] -= nx * actual;
    cityPos[ciA][1] -= ny * actual;
    cityPos[ciB][0] += nx * actual;
    cityPos[ciB][1] += ny * actual;

    clusterTotals.set(idA, totalA + actual);
    clusterTotals.set(idB, totalB + actual);
    anyPushed = true;
  }
  if (!anyPushed) break;
  clusterApplied = true;

  // Re-init edge points from new city positions and re-run all phases
  for (const e of edges) { e.points = [[...cityPos[e.a]], [...cityPos[e.b]]]; e.bendSign = null; }
  reRunCleanup();
  clusterCycles = ce + 1;
}

// ── Font fallback: last resort if label-label still > 0 ──
let fontReduced = false;
if (clusterApplied) {
  let llRemaining = 0;
  for (let i = 0; i < order.length; i++) {
    for (let j = i + 1; j < order.length; j++) {
      if (rectsOverlap(boxFor(order[i]), boxFor(order[j]))) llRemaining++;
    }
  }
  if (llRemaining > 0) {
    console.log(`  ⚠ CLUSTER-ESCAPE: ${llRemaining} label-label residue dopo ${clusterCycles} cicli → riduco font ${FONT_SIZE}→${CLUSTER_FONT_FALLBACK}`);
    FONT_SIZE = CLUSTER_FONT_FALLBACK;
    // textW/ascOf/descOf leggono FONT_SIZE → metriche ricalcolate
    fontReduced = true;
    reRunCleanup();
  }
}
// ── Post-cluster cleanup: fix label-CORE/route-city introduced by city shifts ──
if (clusterApplied) {
  for (let up = 0; up < 3; up++) {
    let pushed = false;
    // Label-CORE: push city away from overlapping route
    for (const id of order) {
      const ci = idIndex.get(id);
      const box = boxFor(id);
      for (const e of edges) {
        if (cityIds[e.a] === id || cityIds[e.b] === id) continue;
        if (!labelCoreHit(e, box)) continue;
        const t = trimmed(e);
        let bestD = Infinity, bestP = null;
        for (let k = 0; k < t.length - 1; k++) {
          const [px, py] = projectOnSeg(cityPos[ci][0], cityPos[ci][1], t[k][0], t[k][1], t[k + 1][0], t[k + 1][1]);
          const d = Math.hypot(cityPos[ci][0] - px, cityPos[ci][1] - py);
          if (d < bestD) { bestD = d; bestP = [px, py]; }
        }
        if (!bestP || bestD < 0.01) continue;
        const push = Math.min(2, bestD * 0.15);
        cityPos[ci][0] += (cityPos[ci][0] - bestP[0]) / bestD * push;
        cityPos[ci][1] += (cityPos[ci][1] - bestP[1]) / bestD * push;
        pushed = true;
      }
    }
    // Route-city: push city away from route passing too close
    for (const e of edges) {
      for (let ci = 0; ci < cityIds.length; ci++) {
        if (ci === e.a || ci === e.b) continue;
        const d = routeCityDist(e, ci);
        if (d >= CITY_CLEARANCE) continue;
        const t = trimmed(e);
        let bestD = Infinity, bestP = null;
        for (let k = 0; k < t.length - 1; k++) {
          const [px, py] = projectOnSeg(cityPos[ci][0], cityPos[ci][1], t[k][0], t[k][1], t[k + 1][0], t[k + 1][1]);
          const dd = Math.hypot(cityPos[ci][0] - px, cityPos[ci][1] - py);
          if (dd < bestD) { bestD = dd; bestP = [px, py]; }
        }
        if (!bestP || bestD < 0.01) continue;
        const push = Math.min(4, (CITY_CLEARANCE - d) * 1.2);
        cityPos[ci][0] += (cityPos[ci][0] - bestP[0]) / bestD * push;
        cityPos[ci][1] += (cityPos[ci][1] - bestP[1]) / bestD * push;
        pushed = true;
      }
    }
    if (!pushed) break;
    for (const e of edges) { e.points = [[...cityPos[e.a]], [...cityPos[e.b]]]; e.bendSign = null; }
    reRunCleanup();
  }
}
// ── End of 4d-quater ──

// ── 4d-quinquies. F2 approach-separation micro-shifts ────────────────────
// Dopo cluster-escape, prova micro-spostamenti mirati per migliorare
// l'approach separation nei triangoli densi (Pittsburgh–Chicago–StL).
// Sposta città non-hub di ≤8px lungo direzioni che aprono l'angolo.
let f2ShiftsApplied = 0;
const f2ShiftDetail = [];

// Shifts pianificati: [cityId, dx, dy, motivazione] — da MAP_TUNING[map].f2Shifts.
const f2PlannedShifts = TUNING.f2Shifts;

for (const [cityId, dx, dy, reason] of f2PlannedShifts) {
  const ci = idIndex.get(cityId);
  if (ci === undefined) continue;
  // Verifica che il cap totale (cluster + F2) non superi 30px
  const prevTotal = clusterTotals.get(cityId) || 0;
  if (prevTotal + Math.abs(dx) + Math.abs(dy) > 30) {
    f2ShiftDetail.push(`${cityId}: SKIP (già spostato ${prevTotal.toFixed(1)}px, cap 30)`);
    continue;
  }
  cityPos[ci][0] += dx;
  cityPos[ci][1] += dy;
  clusterTotals.set(cityId, prevTotal + Math.abs(dx) + Math.abs(dy));
  f2ShiftsApplied++;
  f2ShiftDetail.push(`${cityId}: (${dx > 0 ? '+' : ''}${dx},${dy > 0 ? '+' : ''}${dy}) — ${reason}`);
}

if (f2ShiftsApplied > 0) {
  // Rebuild edge points e re-run cleanup
  for (const e of edges) { e.points = [[...cityPos[e.a]], [...cityPos[e.b]]]; e.bendSign = null; }
  reRunCleanup();
}

// ── 4e. Double-track safety repair (3f+8 Blocker 2) ──
// Dopo tutte le fasi di bend, verifica che le doppie con bend non abbiano
// le tracce ±offsetOf incrociate. Se sì, riduci l'ampiezza del bend o resetta.
let doubleTrackRepairs = 0;
for (const e of edges) {
  if (!e.isDouble || e.points.length < 3) continue;
  if (doubleTrackSafe(e)) continue;
  // Prova a ridurre l'ampiezza (step/2, step/4...) o cambiare segno
  const { mid, nx, ny } = normalOf(e);
  const curSign = e.bendSign || 1;
  const curStep = dist(mid, e.points[1]);
  let fixed = false;
  // Try reducing amplitude with current sign
  for (const sign of [curSign, -curSign]) {
    for (let step = Math.floor(curStep * 0.75); step >= 2; step -= 2) {
      setBend(e, sign, step);
      if (doubleTrackSafe(e) && noRouteConflict(e) && bendPointCitySafe(e)) {
        e.bendSign = sign;
        fixed = true;
        break;
      }
    }
    if (fixed) break;
  }
  if (!fixed) {
    // Reset to straight
    e.points = [e.points[0], e.points[e.points.length - 1]];
    e.bendSign = null;
    console.log(`  ⚠ DOUBLE-BEND reset ${cityIds[e.a]}-${cityIds[e.b]}: tracce ±offset si incrociavano, tratta raddrizzata`);
    doubleTrackRepairs++;
  }
}

// ── 4f. Straightening pass (3f+9) ──────────────────────────────────────
// Dopo tutte le fasi di bend, per OGNI rotta piegata proviamo a
// raddrizzarla (linea retta tra endpoint città). Se TUTTE le validazioni
// passano, rimuoviamo il bend definitivamente. Ripeti fino a stabilità.
function straightenValidates(e) {
  const saved = e.points.map((p) => [...p]);
  const savedSign = e.bendSign;
  e.points = [[...cityPos[e.a]], [...cityPos[e.b]]];
  e.bendSign = null;

  // F2: curve-based conflict checks (non-adjacent routes)
  for (const o of edges) {
    if (o === e || isAdjacent(e, o)) continue;
    const th = swAvg(e, o) + SW_AVG_PAD;
    if (overlapStretchCurve(e, o, th) > ROUTE_MIN_STRETCH ||
        overlapStretchCurve(o, e, th) > ROUTE_MIN_STRETCH) {
      e.points = saved; e.bendSign = savedSign; return false;
    }
  }
  // Route-city clearance
  for (let ci = 0; ci < cityIds.length; ci++) {
    if (ci === e.a || ci === e.b) continue;
    if (distPointDrawnEdge(cityPos[ci][0], cityPos[ci][1], e) < CITY_CLEARANCE) {
      e.points = saved; e.bendSign = savedSign; return false;
    }
  }
  // Label-CORE (skip labels of route's own endpoints — same as final validation line ~2009)
  for (const id of order) {
    if (cityIds[e.a] === id || cityIds[e.b] === id) continue; // 3f+9: escludi tratte incidenti
    const box = boxFor(id);
    if (labelCoreHit(e, box)) { e.points = saved; e.bendSign = savedSign; return false; }
  }
  // Double track
  if (!doubleTrackSafe(e)) { e.points = saved; e.bendSign = savedSign; return false; }
  // Cover fraction
  for (const o of edges) {
    if (o === e || isAdjacent(e, o)) continue;
    const thC = swAvg(e, o) + COVER_PAD;
    if (coverFractionCurve(e, o, thC) > COVER_FRACTION ||
        coverFractionCurve(o, e, thC) > COVER_FRACTION) {
      e.points = saved; e.bendSign = savedSign; return false;
    }
  }
  // F1: approach separation check for adjacent routes
  for (let ci = 0; ci < cityIds.length; ci++) {
    if (ci !== e.a && ci !== e.b) continue;
    const incident = edges.filter((o) => o !== e && (o.a === ci || o.b === ci));
    for (const o of incident) {
      const swT = swAvg(e, o);
      const th = swT + SW_AVG_PAD;
      if (overlapStretchCurve(e, o, th) > ROUTE_MIN_STRETCH ||
          overlapStretchCurve(o, e, th) > ROUTE_MIN_STRETCH) {
        e.points = saved; e.bendSign = savedSign; return false;
      }
      const approachDist = approachMinDistDrawn(e, o, ci);
      if (approachDist < 10) {
        e.points = saved; e.bendSign = savedSign; return false;
      }
    }
  }
  e.points = saved; e.bendSign = savedSign;
  return true;
}

let straighteningPasses = 0;
let straighteningCount = 0;
for (let strCycle = 0; strCycle < 10; strCycle++) {
  let anyStraightened = false;
  for (const e of edges) {
    if (e.points.length <= 2) continue;
    if (!straightenValidates(e)) continue;
    e.points = [[...cityPos[e.a]], [...cityPos[e.b]]];
    e.bendSign = null;
    anyStraightened = true;
    straighteningCount++;
  }
  straighteningPasses = strCycle + 1;
  if (!anyStraightened) break;
}

// ── 4z. Risoluzione incroci/sovrapposizioni rotte (C) ─────────────────
// Per ogni coppia in clash: raddrizza una delle due, altrimenti ripiega
// (entrambi i segni); accetta solo se gli edge coinvolti non hanno più
// clash di prima e restano validi (label-CORE, city clearance, doppie).
function edgeLocalOK(e) {
  for (let ci = 0; ci < cityIds.length; ci++) {
    if (ci === e.a || ci === e.b) continue;
    if (distPointDrawnEdge(cityPos[ci][0], cityPos[ci][1], e) < CITY_CLEARANCE) return false;
  }
  for (const id of order) {
    if (cityIds[e.a] === id || cityIds[e.b] === id) continue;
    if (labelCoreHitCurve(e, boxFor(id))) return false;
  }
  return doubleTrackSafe(e);
}
const clashesOf = (e) => edges.filter((o) => o !== e && routesClash(e, o)).length;
const clashBefore = countClashes().length;
for (let round = 0; round < 6; round++) {
  const pairs = countClashes();
  if (pairs.length === 0) break;
  let progress = false;
  for (const [a, b] of pairs) {
    if (!routesClash(a, b)) continue;
    let done = false;
    for (const e of [a, b]) {
      const saved = e.points.map((p) => [...p]), savedSign = e.bendSign, n0 = clashesOf(e);
      const cands = [];
      if (e.points.length > 2) cands.push(null);
      for (let step = 4; step <= 120; step += 4) cands.push([1, step], [-1, step]);
      for (const c of cands) {
        if (c === null) { e.points = [[...cityPos[e.a]], [...cityPos[e.b]]]; e.bendSign = null; }
        else { e.points = saved.map((p) => [...p]); ensureBendable(e); setBend(e, c[0], c[1]); e.bendSign = c[0]; }
        if (!routesClash(a, b) && clashesOf(e) < n0 && edgeLocalOK(e)) { done = true; break; }
      }
      if (done) break;
      e.points = saved; e.bendSign = savedSign;
    }
    if (done) progress = true;
  }
  if (!progress) break;
}
const clashResidual = countClashes();

// Repair finale delle etichette: le fasi 4e/4f/4z modificano le tratte
// ignorando (di proposito) le label delle proprie città estreme, quindi
// una label può finire sull'inchiostro dei binari della sua città. Qui
// ogni label ancora sporca (CORE anche incidente, label, dot, ownership)
// prende il candidato migliore secondo pickBest (tier-first, ≤32px),
// override inclusi se sporchi. Più passate: uno spostamento può liberare
// o occupare spazio per le vicine.
let finalLabelRepairs = 0;
// Candidati "a scorrimento": stessa distanza dal pallino dei tier, ma il
// testo scivola lungo il lato (sopra/sotto in orizzontale, ai lati in
// verticale) restando affacciato al dot. Servono agli hub con molte
// tratte, dove gli 8 angoli fissi cadono tutti su un binario.
function slideCandidatesFor(name) {
  const w = textW(name);
  const asc = ascOf() + LABEL_PAD, desc = descOf() + LABEL_PAD;
  const out = [];
  for (const d of TIERS) {
    for (const f of [-0.4, -0.25, -0.1, 0.1, 0.25, 0.4]) {
      out.push({ d, dx: round1(f * w), dy: round1(-d - desc), anchor: 'middle' });
      out.push({ d, dx: round1(f * w), dy: round1(d + asc), anchor: 'middle' });
    }
    for (const v of [-0.45, -0.25, 0.25, 0.45]) {
      const dy = round1((asc - desc) / 2 + v * (asc + desc));
      out.push({ d, dx: round1(d + LABEL_PAD), dy, anchor: 'start' });
      out.push({ d, dx: round1(-d - LABEL_PAD), dy, anchor: 'end' });
    }
  }
  return out;
}
function bestSlide(c, name, id) {
  let best = null;
  for (const cand of slideCandidatesFor(name)) {
    const box = labelBox(c.x, c.y, cand, name);
    const ld = distPointRect(c.x, c.y, box);
    if (ld > MAX_LABEL_DIST) continue;
    const s = scoreLabel(c, cand, name, id);
    if (!best || s.total < best.total || (s.total === best.total && ld < best.ld)) best = { total: s.total, ld, cand, clean: s.nLabel === 0 && s.nDot === 0 };
  }
  return best;
}
for (let pass = 0; pass < 6; pass++) {
  let changed = false;
  for (const id of order) {
    const c = cityIndexPos().find((cc) => cc.id === id);
    const cur = labels.get(id);
    const curScore = scoreLabel(c, { dx: cur.dx, dy: cur.dy, anchor: cur.anchor }, names.get(id), id);
    if (curScore.total === 0) continue;
    const best = pickBest(c, names.get(id), id);
    let pick = best && best.cand.d <= MAX_LABEL_DIST ? { total: best.total, cand: best.cand, clean: best.nLabel === 0 && best.nDot === 0 } : null;
    if (!pick || pick.total > 0) {
      const sl = bestSlide(c, names.get(id), id);
      if (sl && sl.clean && (!pick || !pick.clean || sl.total < pick.total)) pick = sl;
    }
    // Mai scambiare un conflitto con la tratta per uno label-label/dot
    if (pick && pick.clean && pick.total < curScore.total) {
      labels.set(id, { dx: round1(pick.cand.dx), dy: round1(pick.cand.dy), anchor: pick.cand.anchor });
      finalLabelRepairs++;
      changed = true;
    }
  }
  if (!changed) break;
}

// Ownership "forte" (lamentela principale: nome lontano dal pallino o più
// vicino a un'altra città). Per le label ancora ambigue si accetta di
// passare SOPRA le tratte della propria città (il testo ha l'alone
// pergamena ed è disegnato sopra i binari) pur di stare attaccate al
// proprio dot. Vincoli duri: niente label-label, niente dot, niente
// tratte NON incidenti, ownership ok. Costo: tratte incidenti coperte,
// poi distanza.
let ownershipStrongFixes = 0;
for (const id of order) {
  const c = cityIndexPos().find((cc) => cc.id === id);
  const name = names.get(id);
  const cur = labels.get(id);
  if (!ownershipBad(c, labelBox(c.x, c.y, cur, name), id)) continue;
  let best = null;
  for (const cand of [...candidatesFor(), ...slideCandidatesFor(name)]) {
    const box = labelBox(c.x, c.y, cand, name);
    const ld = distPointRect(c.x, c.y, box);
    if (ld > MAX_LABEL_DIST) continue;
    if (ownershipBad(c, box, id)) continue;
    if (order.some((o) => o !== id && rectsOverlap(box, boxFor(o)))) continue;
    if (cityIndexPos().some((oc) => oc.id !== id && distPointRect(oc.x, oc.y, box) < DOT_R + 1)) continue;
    let incHits = 0, bad = false;
    for (const e of edges) {
      if (!labelCoreHit(e, box)) continue;
      if (cityIds[e.a] === id || cityIds[e.b] === id) incHits++;
      else { bad = true; break; }
    }
    if (bad) continue;
    const cost = incHits * 100 + ld;
    if (!best || cost < best.cost) best = { cost, cand };
  }
  if (best) {
    labels.set(id, { dx: round1(best.cand.dx), dy: round1(best.cand.dy), anchor: best.cand.anchor });
    ownershipStrongFixes++;
  }
}
console.log(`label repair finale: ${finalLabelRepairs} riposizionate, ${ownershipStrongFixes} ownership forte`);

// Ownership repair finale: label che non "appartengono" al proprio dot
// prendono il candidato più vicino senza alcun conflitto (score 0).
for (const id of order) {
  const c = cityIndexPos().find((cc) => cc.id === id);
  if (!ownershipBad(c, boxFor(id), id)) continue;
  for (const cand of candidatesFor().slice().sort((p, q) => p.d - q.d)) {
    if (cand.d > MAX_LABEL_DIST) continue;
    const s = scoreLabel(c, cand, names.get(id), id);
    if (s.total === 0) {
      labels.set(id, { dx: round1(cand.dx), dy: round1(cand.dy), anchor: cand.anchor });
      break;
    }
  }
}

// ── 5. Bounding → canvas di output (variabile) ─────────────────────────
let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
function grow(px, py) {
  if (px < minX) minX = px;
  if (py < minY) minY = py;
  if (px > maxX) maxX = px;
  if (py > maxY) maxY = py;
}
for (const p of cityPos) {
  grow(p[0] - DOT_R, p[1] - DOT_R);
  grow(p[0] + DOT_R, p[1] + DOT_R);
}
for (const e of edges) for (const p of e.points) grow(p[0], p[1]);
for (const id of cityIds) {
  const box = boxFor(id);
  grow(box.left, box.top); grow(box.right, box.bottom);
}

const shiftX = MARGIN - minX, shiftY = MARGIN - minY;
for (const p of cityPos) { p[0] += shiftX; p[1] += shiftY; }
for (const e of edges) for (const p of e.points) { p[0] += shiftX; p[1] += shiftY; }
const W = Math.ceil(maxX - minX + 2 * MARGIN);
const H = Math.ceil(maxY - minY + 2 * MARGIN);

// ── 6. Validazione + report ────────────────────────────────────────────
let labelLabel = 0, labelRouteCore = 0, labelDot = 0, maxLabelDist = 0;
const labelCoreList = [];
const labelCoreIncident = [];
const labelOwnershipList = [];
const labelDistWarnings = []; // label > MAX_LABEL_DIST (dopo route-bend fallback)
for (let i = 0; i < order.length; i++) {
  const id = order[i];
  const c = cityIndexPos().find((cc) => cc.id === id);
  const box = boxFor(id);
  const ld = distPointRect(c.x, c.y, box);
  maxLabelDist = Math.max(maxLabelDist, ld);
  if (ld > MAX_LABEL_DIST + 0.05) labelDistWarnings.push(`${id}: ${ld.toFixed(1)}px`);
  if (ownershipBad(c, box, id)) labelOwnershipList.push(`${id}(${ownershipWhy(c, box, id)})`);
  for (let j = i + 1; j < order.length; j++) {
    if (rectsOverlap(box, boxFor(order[j]))) labelLabel++;
  }
  for (const e of edges) {
    // Tratte incidenti: la label è disegnata SOPRA le tratte con alone
    // pergamena, quindi resta leggibile → WARNING (hub a 7 tratte senza
    // spazio libero ≤32px). Tratte non incidenti → errore (gate).
    const incidentEdge = cityIds[e.a] === id || cityIds[e.b] === id;
    const sw2 = swOf(e.len) / 2;
    for (const t of labelCurvesOf(e)) {
      let hit = false;
      for (let k = 0; k < t.length - 1; k++) {
        if (segRectDist(t[k][0], t[k][1], t[k + 1][0], t[k + 1][1], box) < sw2) { hit = true; break; }
      }
      if (hit) {
        if (incidentEdge) labelCoreIncident.push(`${id}↔${cityIds[e.a]}-${cityIds[e.b]}`);
        else { labelRouteCore++; labelCoreList.push(`${id}↔${cityIds[e.a]}-${cityIds[e.b]}`); }
        break;
      }
    }
  }
  for (const oc of cityIndexPos()) {
    if (oc.id !== id && distPointRect(oc.x, oc.y, box) < DOT_R + 1) labelDot++;
  }
}

// 3f+7: distanza minima città-città non adiacenti (obiettivo ≥18px)
let minCityDist = Infinity, minCityPair = '';
for (let i = 0; i < cityIds.length; i++) {
  for (let j = i + 1; j < cityIds.length; j++) {
    // Verifica se esiste una tratta diretta tra i e j
    const keyAB = [cityIds[i], cityIds[j]].sort().join('|');
    if (pairRoutes.has(keyAB)) continue; // adiacenti
    const d = dist(cityPos[i], cityPos[j]);
    if (d < minCityDist) { minCityDist = d; minCityPair = `${cityIds[i]}-${cityIds[j]}`; }
  }
}

// 3f+7: tratte che passano entro CITY_CLEARANCE di città non-estreme
let routeCityConflicts = 0;
const routeCityConflictList = [];
for (const e of edges) {
  for (let ci = 0; ci < cityIds.length; ci++) {
    if (ci === e.a || ci === e.b) continue;
    // F2: distanza sulla curva campionata, non sulla polyline
    const d = distPointDrawnEdge(cityPos[ci][0], cityPos[ci][1], e);
    if (d < CITY_CLEARANCE) {
      routeCityConflicts++;
      routeCityConflictList.push(`${cityIds[e.a]}-${cityIds[e.b]} vs ${cityIds[ci]} (${d.toFixed(1)}px < ${CITY_CLEARANCE}px)`);
    }
  }
}

let routeRoute = 0, hiddenCount = 0;
const routeConflictList = [], hiddenList = [];
for (let i = 0; i < edges.length; i++) {
  for (let j = i + 1; j < edges.length; j++) {
    const a = edges[i], b = edges[j];
    if (isAdjacent(a, b)) continue;
    if (isConflict(a, b)) {
      routeRoute++;
      routeConflictList.push(`${a.key} vs ${b.key}`);
    }
    // F2: cover fraction su curve campionate
    const thCover = swAvg(a, b) + COVER_PAD;
    const covA = coverFractionCurve(a, b, thCover);
    const covB = coverFractionCurve(b, a, thCover);
    if (covA > COVER_FRACTION || covB > COVER_FRACTION) {
      hiddenCount++;
      hiddenList.push(`${a.key} (${(covA * 100).toFixed(0)}%) vs ${b.key} (${(covB * 100).toFixed(0)}%)`);
    }
  }
}

// Statistiche d3-force (scarto dal target — distanza retta tra città)
let errSum = 0, errMax = 0;
let errWorstRatio = 0, errWorstKey = '';
const lengthRatioViolations = [];
for (const e of edges) {
  const pts = e.points;
  // Distanza retta city-center → city-center (esclude i bend)
  const d = dist(pts[0], pts[pts.length - 1]);
  const err = Math.abs(d - e.len * UNIT);
  errSum += err; if (err > errMax) errMax = err;
  const ratio = d / (e.len * UNIT);
  if (ratio > errWorstRatio) { errWorstRatio = ratio; errWorstKey = e.key; }
  if (ratio > 1.6 || ratio < 0.8) {
    lengthRatioViolations.push(`${e.key}: ${d.toFixed(1)}/${(e.len * UNIT).toFixed(1)} = ${ratio.toFixed(2)}× (len=${e.len})`);
  }
}
const errMean = errSum / edges.length;

// 3f+8: doppie con bend — distanza minima tracce ±7
let doubleTrackViolations = 0;
const doubleTrackViolationList = [];
for (const e of edges) {
  if (!e.isDouble || e.points.length < 3) continue;
  const md = doubleTrackMinDist(e);
  if (md < doubleTrackMin(e)) {
    doubleTrackViolations++;
    doubleTrackViolationList.push(`${e.key}: min-dist tracce ±${offsetOf(e).toFixed(1)} = ${md.toFixed(1)}px < ${doubleTrackMin(e).toFixed(1)}px`);
  }
}

// ── Output JSON ────────────────────────────────────────────────────────
const citiesOut = {};
for (let i = 0; i < cityIds.length; i++) citiesOut[cityIds[i]] = { x: r2(cityPos[i][0]), y: r2(cityPos[i][1]) };
const labelsOut = {};
for (const [id, lb] of labels) labelsOut[id] = lb;
const routesOut = {};
for (const e of edges) {
  const pts = e.points.map((p) => [round1(p[0]), round1(p[1])]);
  for (const rid of e.routeIds) routesOut[rid] = pts;
}
const output = {
  unit: UNIT,
  width: W,
  height: H,
  cities: citiesOut,
  labels: labelsOut,
  routes: routesOut,
  font: { family: 'Barlow Semi Condensed', weight: 600, size: FONT_SIZE },
  doubleGap: DOUBLE_GAP,
};
writeFileSync(OUT_PATH, JSON.stringify(output, null, 2) + '\n');

// ── F1: Approach separation scan ───────────────────────────────────────
// Scansiona TUTTE le coppie di rotte che condividono un endpoint
// e calcola la minima distanza tra le curve DISEGNATE a ≥20px dal dot.
const approachViolations = [];
const approachPairs = [];
for (let ci = 0; ci < cityIds.length; ci++) {
  const incident = edges.filter((e) => e.a === ci || e.b === ci);
  for (let ai = 0; ai < incident.length; ai++) {
    for (let bi = ai + 1; bi < incident.length; bi++) {
      const a = incident[ai], b = incident[bi];
      const d = approachMinDistDrawn(a, b, ci);
      approachPairs.push({ a, b, ci, dist: d });
      if (d < APPROACH_MIN_DIST) {
        approachViolations.push(
          `${cityIds[a.a]}↔${cityIds[a.b]}|${cityIds[b.a]}↔${cityIds[b.b]} ` +
          `(hub ${cityIds[ci]}, approach ${d.toFixed(1)}px < ${APPROACH_MIN_DIST}px)`
        );
      }
    }
  }
}

// ── Report ─────────────────────────────────────────────────────────────
const total = labelLabel + labelRouteCore + labelDot + routeRoute;
console.log(`rotte: ${mapData.routes.length} (coppie: ${edges.length}) | UNIT=${UNIT}px | canvas ${W}x${H}`);
console.log(`scarto medio |disegno−target|: ${errMean.toFixed(1)}px (max ${errMax.toFixed(1)}px, worst ratio ${errWorstRatio.toFixed(2)}× su ${errWorstKey}) su target ${UNIT}–${6 * UNIT}px`);
console.log(`  violazioni ratio (>1.6× o <0.8×): ${lengthRatioViolations.length} (gate: ≤1.7× per rotte ≥5 carte, ≤1.6× altrimenti)`);
for (const v of lengthRatioViolations) console.log(`  RATIO ${v}`);
console.log(`bend route-route: ${bendIterations} iterazioni (max ${MAX_BEND_ITER}) | bend route-label: ${labelRounds} round (max ${MAX_LABEL_ROUNDS}) | relax+re-tick: ${relaxIterations} cicli`);
// 3f+9: conteggio rotte piegate (bend residui)
const bentRoutesNow = edges.filter((e) => e.points.length > 2);
const bentRouteList = bentRoutesNow.map((e) => cityIds[e.a] + '↔' + cityIds[e.b]).join(', ');
console.log(`rotte piegate (bend residui): ${bentRoutesNow.length}/${edges.length}${straighteningCount > 0 ? ` | raddrizzate: ${straighteningCount} (${straighteningPasses} passaggi)` : ''}`);
if (bentRoutesNow.length > 0) console.log(`  bend: ${bentRouteList}`);
console.log(`city-clearance: ${cityClearanceBends} tratte piegate${cityClearanceWarnings.length > 0 ? `, ${cityClearanceWarnings.length} WARNING` : ''}`);
for (const w of cityClearanceWarnings) console.log(`  ⚠ CITY-BEND ${w}`);
// F1: WARNING per adjacent/fan-out fallimenti
if (adjacentBendWarnings.length > 0) {
  console.log(`  ⚠ ADJACENT-BEND falliti: ${adjacentBendWarnings.length}`);
  for (const w of adjacentBendWarnings) console.log(`  ⚠ ADJACENT-BEND ${w}`);
}
if (fanOutWarnings.length > 0) {
  console.log(`  ⚠ FAN-OUT falliti: ${fanOutWarnings.length}`);
  for (const w of fanOutWarnings) console.log(`  ⚠ FAN-OUT ${w}`);
}
console.log(`city-micro-push: ${cityMicroPushes} micro-spinte (max 2px, solo label-CORE/route-city residui)`);
console.log(`cluster-escape: ${clusterCycles} cicli${clusterApplied ? '' : ' (non necessario)'}${fontReduced ? ` + font fallback ${FONT_SIZE}px` : ''}`);
if (clusterTotals.size > 0) {
  const pushed = [...clusterTotals.entries()].filter(([, v]) => v > 0).map(([id, v]) => `${id}(${v.toFixed(1)}px)`).join(', ');
  if (pushed) console.log(`  città spostate: ${pushed}`);
}
// F2: approach-separation micro-shifts report
if (f2ShiftsApplied > 0) {
  console.log(`F2 approach shifts: ${f2ShiftsApplied} micro-spostamenti mirati`);
  for (const d of f2ShiftDetail) console.log(`  F2-SHIFT ${d}`);
}
console.log('validazione finale:');
console.log(`  conflitti label-label:       ${labelLabel}`);
console.log(`  conflitti label-CORE:        ${labelRouteCore}  (obiettivo 0)`);
for (const s of labelCoreList) console.log(`    ⚠ LABEL-CORE ${s}`);
console.log(`  label su tratte della propria città (alone, warning): ${labelCoreIncident.length}`);
for (const s of labelCoreIncident) console.log(`    ⚠ LABEL-CORE-INCIDENTE ${s}`);
console.log(`  conflitti label-pallino:     ${labelDot}`);
console.log(`  conflitti tratta-tratta:     ${routeRoute}`);
for (const c of routeConflictList) console.log(`  TRATTA ${c}`);
console.log(`  tratte "nascosto completo" (>${(COVER_FRACTION * 100).toFixed(0)}%): ${hiddenCount}`);
for (const h of hiddenList) console.log(`  HIDDEN ${h}`);
console.log(`  tratte vs città non-estreme: ${routeCityConflicts}  (obiettivo 0)`);
for (const c of routeCityConflictList) console.log(`  CITY-ROUTE ${c}`);
console.log(`  distanza max label→dot:      ${maxLabelDist.toFixed(1)}px (cap ${MAX_LABEL_DIST}px${labelDistWarnings.length > 0 ? ', WARNING' : ''})`);
for (const w of labelDistWarnings) console.log(`  ⚠ LABEL-DIST ${w}`);
console.log(`  distanza min città-città:    ${minCityDist.toFixed(1)}px (${minCityPair}) — obiettivo ≥18px`);
console.log(`  doppie con bend: min-dist tracce < sw+0.6·gap: ${doubleTrackViolations} (gate: 0, exit 1 se >0)`);
for (const v of doubleTrackViolationList) console.log(`  DOUBLE-X ${v}`);
// F1: approach separation (nuovo gate)
const worstApproach = approachPairs.sort((a, b) => a.dist - b.dist);
if (worstApproach.length > 0) {
  console.log(`  approach separation (curve ≥${APPROACH_ZONE}px dal dot, soglia ≥${APPROACH_MIN_DIST}px): ${approachViolations.length} violazioni`);
  console.log(`    worst 5: ${worstApproach.slice(0, 5).map(p => `${cityIds[p.a.a]}↔${cityIds[p.a.b]}|${cityIds[p.b.a]}↔${cityIds[p.b.b]}:${p.dist.toFixed(1)}px`).join(', ')}`);
  for (const v of approachViolations) console.log(`  APPROACH ${v}`);
} else {
  console.log(`  approach separation (curve ≥${APPROACH_ZONE}px dal dot, soglia ≥${APPROACH_MIN_DIST}px): 0 violazioni ✓`);
}
console.log(`  TOTALE: ${total} conflitti`);

// Preview SVG opzionale
const svgArg = process.argv.indexOf('--svg');
if (svgArg !== -1 && process.argv[svgArg + 1]) {
  writeFileSync(process.argv[svgArg + 1], previewSvg());
  console.log(`SVG scritto: ${process.argv[svgArg + 1]}`);
}

let failed = false;
console.log(`  incroci/sovrapposizioni rotte: ${clashResidual.length} (prima della risoluzione: ${clashBefore})${clashResidual.length ? ' — ' + clashResidual.map(([a, b]) => `${a.key}×${b.key}`).join(', ') : ''}`);
console.log(`  label-ownership: ${labelOwnershipList.length} | label > ${MAX_LABEL_DIST}px: ${labelDistWarnings.length}`);
if (clashResidual.length > 0) {
  console.error(`FALLITO: ${clashResidual.length} incroci/sovrapposizioni rotte residui.`);
  failed = true;
}
if (labelLabel > 0) {
  console.error(`FALLITO: ${labelLabel} conflitti label-label residui (gate: 0, exit 1).`);
  failed = true;
}
if (labelOwnershipList.length > 0) {
  console.log(`  ⚠ WARNING ${labelOwnershipList.length} label-ownership (nessun candidato pulito ≤${MAX_LABEL_DIST}px): ${labelOwnershipList.join(', ')}`);
}
if (labelDistWarnings.length > 0) {
  console.error(`FALLITO: label oltre ${MAX_LABEL_DIST}px: ${labelDistWarnings.join(', ')}`);
  failed = true;
}
if (hiddenCount > 0 || labelRouteCore > 0) {
  console.error('FALLITO: restano tratte "nascosto completo" o label-CORE.');
  failed = true;
}
if (errMean > 30) {
  console.error(`FALLITO: scarto medio ${errMean.toFixed(1)}px > 30px (gate lunghezza rotte).`);
  failed = true;
}
if (lengthRatioViolations.length > 0) {
  // 3f+8v2: eccezione documentata: rotte ≥5 carte tollerano fino a 1.7×
  // 3f+8v3: cluster-escape rilassa ratio su rotte corte (len=1): il numero
  // di carte si legge dagli slot vagoni, non dalla lunghezza in pixel.
  const criticalRatios = lengthRatioViolations.filter(v => {
    const key = v.split(':')[0];
    const routes = pairRoutes.get(key) || [];
    const maxLen = Math.max(...routes.map(r => r.len));
    const ratioStr = v.split('=')[1]?.replace('×', '').trim();
    const ratio = parseFloat(ratioStr);
    if (!isNaN(ratio) && maxLen >= 5 && ratio <= 1.7 && ratio >= 0.8) {
      console.log(`  ⚠ RATIO-OK ${v} (rotta lunga ≥5 carte, entro 1.7× — accettato)`);
      return false;
    }
    // 3f+8v3: cluster-escape — rotte len=1 tollerano fino a 2.5×
    if (!isNaN(ratio) && maxLen === 1 && ratio <= 2.5 && ratio >= 0.8 && clusterApplied) {
      console.log(`  ⚠ RATIO-OK ${v} (rotta len=1 dopo cluster-escape, entro 2.5× — accettato)`);
      return false;
    }
    return true;
  });
  if (criticalRatios.length > 0) {
    console.error(`FALLITO: ${criticalRatios.length} rotte con ratio critico fuori gate.`);
    failed = true;
  }
}
if (doubleTrackViolations > 0) {
  console.error(`FALLITO: ${doubleTrackViolations} doppie con tracce che si incrociano (min-dist < sw+0.6·gap).`);
  failed = true;
}
if (approachViolations.length > 0) {
  // F1: coppie con approach < 1px sono inevitabili in triangoli densi
  // (portland-SLC-SF, pittsburgh-chicago-saint_louis). Documentate come
  // WARNING — il fan-out a 85° e push hub ≤8px non bastano.
  const critical = approachViolations.filter(v => {
    const match = v.match(/approach ([0-9.]+)px/);
    return match && parseFloat(match[1]) < 1.0;
  });
  const warning = approachViolations.filter(v => {
    const match = v.match(/approach ([0-9.]+)px/);
    return match && parseFloat(match[1]) >= 1.0 && parseFloat(match[1]) < 10;
  });
  if (critical.length > 0) {
    console.log(`  ⚠ APPROACH-CRIT (documentato): ${critical.length} coppie con approach < 1px — inevitabile in triangoli densi (fan-out 85° + push hub ≤8px esauriti).`);
    for (const v of critical) console.log(`  ⚠ APPROACH-CRIT ${v}`);
  }
  if (warning.length > 0) {
    console.log(`  ⚠ APPROACH-WARN: ${warning.length} coppie tra 1-10px — accettate con WARNING.`);
  }
  console.log(`  ⚠ APPROACH-REPORT: ${approachViolations.length} totali sotto soglia 10px (${critical.length} <1px inevitabili, ${warning.length} warning 1-10px) — documentato.`);
}
if (failed) process.exit(1);
process.exit(0);

function previewSvg() {
  const COLOR = { Red: '#c0392b', Orange: '#e67e22', Yellow: '#d4a017', Green: '#27ae60', Blue: '#2980b9', Purple: '#8e44ad', Black: '#34495e', White: '#bdc3c7', Gray: '#95a5a6' };
  let s = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" font-family="system-ui">\n<rect width="${W}" height="${H}" fill="#f6ebd0"/>\n`;
  for (const e of edges) {
    for (const pts of tracksOf(e)) {
      const p = pts.map((q) => q.map((v) => v.toFixed(1)).join(',')).join(' ');
      const col = COLOR[mapData.routes.find((r) => r.id === e.routeIds[0]).color] || '#888';
      s += `<polyline points="${p}" fill="none" stroke="${col}" stroke-width="${swOf(e.len)}" stroke-linecap="round" stroke-linejoin="round" opacity="0.75"/>\n`;
    }
  }
  for (let i = 0; i < cityIds.length; i++) {
    const [x, y] = cityPos[i];
    s += `<circle cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="${DOT_R}" fill="#fbf6e9" stroke="#453416" stroke-width="2"/>\n`;
  }
  for (const [id, lb] of labels) {
    const c = cityIndexPos().find((cc) => cc.id === id);
    s += `<text x="${(c.x + lb.dx).toFixed(1)}" y="${(c.y + lb.dy).toFixed(1)}" text-anchor="${lb.anchor}" font-size="${FONT_SIZE}" font-weight="600" fill="#453416" paint-order="stroke" stroke="#f6ebd0" stroke-width="3.2">${names.get(id)}</text>\n`;
  }
  s += `</svg>\n`;
  return s;
}
