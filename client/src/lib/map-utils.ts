// Proiezione mappa e utilità geometriche per il rendering SVG.
//
// Le città hanno coordinate lat/lon reali; la proiezione è una semplice
// mappatura lineare con crop ai bordi (equirettangolare semplificata).

import { CARD_COLOR_CSS } from './colors';

// ── Tipi ───────────────────────────────────────────────────────────

export interface CityData {
  id: string;
  name: string;
  lat: number;
  lon: number;
}

export interface RouteData {
  id: string;
  from: string;
  to: string;
  len: number;
  color: string; // "Red" | "Gray" | ... come da JSON
  double: boolean;
}

export interface MapData {
  cities: CityData[];
  routes: RouteData[];
  tickets: { id: string; from: string; to: string; points: number; deck: string }[];
}

export interface Point {
  x: number;
  y: number;
}

/** Layout schematico di una mappa (`GET /api/rulesets/{id}/layout`, generato da `client/tools/gen-schematic-layout.mjs`). */
export interface MapLayout {
  unit?: number;
  width: number;
  height: number;
  cities: Record<string, Point>;
  labels: Record<string, { dx: number; dy: number; anchor: 'start' | 'middle' | 'end' }>;
  /** Polyline a 2 o 3 punti `[x, y]` per id rotta (il 3° è il "bend"). */
  routes: Record<string, [number, number][]>;
  /** Font con cui il generatore ha validato le etichette. */
  font?: { family: string; weight: number; size: number };
  /** Gap libero tra i due binari di una doppia (px viewBox). */
  doubleGap?: number;
}

// ── Proiezione ─────────────────────────────────────────────────────

/** Bounding box della mappa USA (dati: lat 25.76–51.04, lon −123.12–−71.06, con margine). */
const USA_BOUNDS = {
  latMin: 25,
  latMax: 52,
  lonMin: -124,
  lonMax: -70,
};

export interface ProjectionConfig {
  /** Larghezza viewport SVG. */
  width: number;
  /** Altezza viewport SVG. */
  height: number;
  /** Padding interno. */
  padding: number;
}

/**
 * Proiezione lat/lon → coordinate SVG (Y invertito: lat ↑).
 * Mappa lineare all'interno del bounding box USA.
 */
export function projectCity(
  city: CityData,
  config: ProjectionConfig,
): Point {
  const { latMin, latMax, lonMin, lonMax } = USA_BOUNDS;
  const { width, height, padding } = config;
  const usableW = width - padding * 2;
  const usableH = height - padding * 2;

  const x = padding + ((city.lon - lonMin) / (lonMax - lonMin)) * usableW;
  // Y invertito: più lat = più in alto = meno Y
  const y = padding + ((latMax - city.lat) / (latMax - latMin)) * usableH;

  return { x, y };
}

// ── Rotte doppie: offset ───────────────────────────────────────────

/** Gap libero (bordo-bordo) tra i due binari di una doppia, se il layout non lo specifica. */
export const DEFAULT_DOUBLE_GAP = 5;

/**
 * Distanza della mezzeria di ciascun binario di una doppia dalla linea
 * città-città: metà spessore (a stroke pieno) + metà gap. Con l'offset
 * fisso ±7 le doppie len≥4 avevano ~0.5px tra i due inchiostri e
 * sembravano una linea sola. Il generatore usa la stessa formula.
 * Segno: indice 0 → +offset, indice 1 → −offset (normale (−Δy, Δx)/|Δ|).
 */
export function doubleRouteOffset(len: number, index: number, gap = DEFAULT_DOUBLE_GAP): number {
  const mag = routeStrokeWidth(len) / 2 + gap / 2;
  return index === 1 ? -mag : mag;
}

// ── Rotte doppie: raggruppamento ───────────────────────────────────

export interface DoubleRouteGroup {
  routes: RouteData[];
  indices: Map<string, number>; // routeId → index (0 o 1)
}

/**
 * Raggruppa le rotte per coppia di città e assegna un indice (0 o 1)
 * alle doppie. Le singole hanno indice 0 per default.
 */
export function groupDoubleRoutes(routes: RouteData[]): {
  groups: Map<string, DoubleRouteGroup>;
  routeIndex: Map<string, number>;
} {
  const pairMap = new Map<string, RouteData[]>();
  for (const r of routes) {
    const key = [r.from, r.to].sort().join('|');
    const list = pairMap.get(key) || [];
    list.push(r);
    pairMap.set(key, list);
  }

  const groups = new Map<string, DoubleRouteGroup>();
  const routeIndex = new Map<string, number>();

  for (const [key, list] of pairMap) {
    const indices = new Map<string, number>();
    list.forEach((r, i) => {
      indices.set(r.id, list.length > 1 ? i : 0);
      routeIndex.set(r.id, list.length > 1 ? i : 0);
    });
    groups.set(key, { routes: list, indices });
  }

  return { groups, routeIndex };
}

// ── Colori rotta ───────────────────────────────────────────────────

/** Restituisce il colore CSS per una rotta non reclamata. */
export function routeStrokeColor(route: RouteData): string {
  if (route.color === 'Gray' || route.color === 'grey' || route.color === 'gray') {
    return '#95a5a6';
  }
  // Mappa nome colore mappa → CardColor → CSS
  const mapping: Record<string, string> = {
    Red: CARD_COLOR_CSS.RED,
    Orange: CARD_COLOR_CSS.ORANGE,
    Yellow: CARD_COLOR_CSS.YELLOW,
    Green: CARD_COLOR_CSS.GREEN,
    Blue: CARD_COLOR_CSS.BLUE,
    Purple: CARD_COLOR_CSS.PURPLE,
    Black: CARD_COLOR_CSS.BLACK,
    White: CARD_COLOR_CSS.WHITE,
  };
  return mapping[route.color] ?? '#95a5a6';
}

/** Restituisce la larghezza di una rotta in base alla lunghezza. */
export function routeStrokeWidth(len: number): number {
  return 6 + len * 1.5;
}

/** Restituisce il colore di sfondo del percorso dei vagoni. */
export function routeFillColor(route: RouteData): string {
  if (route.color === 'Gray' || route.color === 'grey' || route.color === 'gray') {
    return 'none';
  }
  const stroke = routeStrokeColor(route);
  return stroke + '22'; // trasparente
}

// ── Punteggio rotte ────────────────────────────────────────────────

const ROUTE_POINTS: Record<number, number> = {
  1: 1,
  2: 2,
  3: 4,
  4: 7,
  5: 10,
  6: 15,
};

export function routePoints(len: number): number {
  return ROUTE_POINTS[len] ?? 0;
}

// ── Biglietti evidenziati sulla mappa ──────────────────────────────

/** Palette dei biglietti: colore per indice (pin/linea sulla mappa, marcatore sulla card). */
export const TICKET_PIN_COLORS = ['#0f766e', '#b45309', '#7c3aed', '#a13d63', '#0369a1'];

/** Colore del biglietto in posizione `index` (la palette si ripete). */
export function ticketPinColor(index: number): string {
  return TICKET_PIN_COLORS[index % TICKET_PIN_COLORS.length] ?? '#0f766e';
}

/**
 * Coppie città da evidenziare per i biglietti offerti, nello stesso ordine: solo quelli in
 * `shown` (i selezionati). Gli altri, e quelli senza descrittore, danno una coppia vuota che
 * la mappa salta, così gli indici, e quindi i colori, restano allineati alle card.
 */
export function offeredTicketPins(
  offered: ReadonlyArray<{ id: string }>,
  tickets: ReadonlyArray<{ id: string; from: string; to: string }>,
  shown: ReadonlySet<string>,
): Array<{ from: string; to: string }> {
  const byId = new Map(tickets.map((t) => [t.id, t]));
  return offered.map((o) => {
    const t = shown.has(o.id) ? byId.get(o.id) : undefined;
    return { from: t?.from ?? '', to: t?.to ?? '' };
  });
}