// Proiezione mappa e utilità geometriche per il rendering SVG.
//
// Le città hanno coordinate lat/lon reali; la proiezione è una semplice
// mappatura lineare con crop ai bordi (equirettangolare semplificata).

import type { CardColor } from './proto';
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

/**
 * Offset perpendicolare per rotte doppie: n = (−Δy, Δx) / |Δ|.
 * Restituisce l'offset per la rotta in posizione `index` (0 o 1)
 * tra le doppie della stessa coppia di città.
 */
export function doubleRouteOffset(
  from: Point,
  to: Point,
  index: number,
): Point {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.sqrt(dx * dx + dy * dy);
  if (len === 0) return { x: 0, y: 0 };

  // Normale perpendicolare normalizzata
  const nx = -dy / len;
  const ny = dx / len;

  // 3f+6: ±7 (era ±10) — offset proporzionato agli stroke (max sw 15):
  // ±10 risultava sproporzionato sul telefono (viewBox grande reso a ~400
  // CSS px). NOTA: MapSvg usa lo stesso valore hardcoded in offsetPoints.
  const sign = index === 0 ? 1 : -1;
  const distance = 7;

  return { x: nx * distance * sign, y: ny * distance * sign };
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