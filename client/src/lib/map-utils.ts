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
  /** Paese confinante (vicolo cieco): marcatore distinto dal pallino città. Assente = false. */
  country?: boolean;
}

export interface RouteData {
  id: string;
  from: string;
  to: string;
  len: number;
  color: string; // "Red" | "Gray" | ... come da JSON
  double: boolean;
  /** Europe: tunnel (carte extra dopo la rivelazione). Assente = false (mappa USA). */
  tunnel?: boolean;
  /** Europe: numero di locomotive obbligatorie (traghetto). Assente = 0. */
  ferries?: number;
  /** Germany/Poland: binari tutti aperti con qualunque numero di giocatori. Assente = false. */
  alwaysOpen?: boolean;
  /** Nordic: ogni spazio si paga anche con questo numero di carte qualsiasi (locomotive comprese). Assente = no. */
  anyCardsPerSpace?: number;
}

export interface MapData {
  cities: CityData[];
  routes: RouteData[];
  tickets: TicketDesc[];
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

/** Gap libero (bordo-bordo) tra due binari adiacenti di un gruppo parallelo, se il layout non lo specifica. */
export const DEFAULT_DOUBLE_GAP = 5;

/**
 * Distanza della mezzeria di un binario dalla linea città-città in un gruppo di
 * `count` = 2–4 binari paralleli: `k·passo`, passo = spessore pieno + gap e
 * k = (count−1)/2 … −(count−1)/2 (simmetrico). Con 2 binari è metà spessore + metà
 * gap (con l'offset fisso ±7 le doppie len≥4 avevano ~0.5px tra i due inchiostri e
 * sembravano una linea sola). Il generatore usa la stessa formula.
 * Segno: indice 0 → lato + (normale (−Δy, Δx)/|Δ|), ultimo indice → lato −.
 */
export function doubleRouteOffset(
  len: number,
  index: number,
  gap = DEFAULT_DOUBLE_GAP,
  count = 2,
): number {
  return ((count - 1) / 2 - index) * (routeStrokeWidth(len) + gap);
}

// ── Rotte doppie: raggruppamento ───────────────────────────────────

export interface DoubleRouteGroup {
  routes: RouteData[];
  indices: Map<string, number>; // routeId → indice del binario (0 … n−1)
}

/**
 * Raggruppa le rotte per coppia di città e assegna un indice (0 … n−1) ai binari dei
 * gruppi paralleli (n = 2–4) e la dimensione del gruppo. Le singole hanno indice 0 e
 * dimensione 1.
 */
export function groupDoubleRoutes(routes: RouteData[]): {
  groups: Map<string, DoubleRouteGroup>;
  routeIndex: Map<string, number>;
  routeGroupSize: Map<string, number>;
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
  const routeGroupSize = new Map<string, number>();

  for (const [key, list] of pairMap) {
    const indices = new Map<string, number>();
    list.forEach((r, i) => {
      indices.set(r.id, list.length > 1 ? i : 0);
      routeIndex.set(r.id, list.length > 1 ? i : 0);
      routeGroupSize.set(r.id, list.length);
    });
    groups.set(key, { routes: list, indices });
  }

  return { groups, routeIndex, routeGroupSize };
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

/** Biglietto della mappa: classico (`to` + `points`) oppure multi-destinazione (`destinations`, ≥ 2). */
export interface TicketDesc {
  id: string;
  from: string;
  to?: string;
  points?: number;
  destinations?: TicketDestination[];
  deck: string;
}

/** Destinazione di un biglietto con i punti che vale. */
export interface TicketDestination {
  to: string;
  points: number;
}

/** Destinazioni di un biglietto nell'ordine della mappa; una sola per il biglietto classico. */
export function ticketDestinations(t: Pick<TicketDesc, 'to' | 'points' | 'destinations'>): TicketDestination[] {
  if (t.destinations) return t.destinations;
  return t.to !== undefined ? [{ to: t.to, points: t.points ?? 0 }] : [];
}

/** Città da evidenziare per un biglietto: la partenza e tutte le destinazioni. */
export interface TicketPin {
  from: string;
  to: string[];
}

/** Pin di un biglietto: origine e tutte le sue destinazioni. */
export function ticketPin(t: Pick<TicketDesc, 'from' | 'to' | 'points' | 'destinations'>): TicketPin {
  return { from: t.from, to: ticketDestinations(t).map((d) => d.to) };
}

/** «Origine → Destinazione» o, con più destinazioni, «Origine → A / B / C». */
export function ticketLabel(
  t: Pick<TicketDesc, 'from' | 'to' | 'points' | 'destinations'>,
  cityName: (id: string) => string,
): string {
  return `${cityName(t.from)} → ${ticketDestinations(t).map((d) => cityName(d.to)).join(' / ')}`;
}

/**
 * Pin dei biglietti offerti, nello stesso ordine: solo quelli in `shown` (i selezionati).
 * Gli altri, e quelli senza descrittore, danno un pin vuoto che la mappa salta, così gli
 * indici, e quindi i colori, restano allineati alle card.
 */
export function offeredTicketPins(
  offered: ReadonlyArray<{ id: string }>,
  tickets: ReadonlyArray<Pick<TicketDesc, 'id' | 'from' | 'to' | 'points' | 'destinations'>>,
  shown: ReadonlySet<string>,
): TicketPin[] {
  const byId = new Map(tickets.map((t) => [t.id, t]));
  return offered.map((o) => {
    const t = shown.has(o.id) ? byId.get(o.id) : undefined;
    return t ? ticketPin(t) : { from: '', to: [] };
  });
}

/** Cosa disegnare per un biglietto evidenziato: pin su origine e destinazioni, una linea per destinazione. */
export interface TicketLayer {
  pins: Point[];
  /** Curve SVG `M … Q …` dall'origine a ciascuna destinazione. */
  paths: string[];
  color: string;
}

/**
 * Layer dei biglietti evidenziati: colore per indice, curva leggera (controllo quadratico
 * spostato sulla normale di ~8% della distanza). Salta i pin senza origine sulla mappa e le
 * destinazioni mancanti o coincidenti con l'origine; senza destinazioni disegnabili il
 * biglietto non dà alcun layer (l'indice, e quindi il colore, resta comunque quello del pin).
 */
export function ticketLayers(
  pins: ReadonlyArray<TicketPin>,
  positions: ReadonlyMap<string, Point>,
): TicketLayer[] {
  const layers: TicketLayer[] = [];
  pins.forEach((t, i) => {
    const fromPos = positions.get(t.from);
    if (!fromPos) return;
    const targets: Point[] = [];
    const paths: string[] = [];
    for (const id of t.to) {
      const toPos = positions.get(id);
      if (!toPos) continue;
      const dx = toPos.x - fromPos.x;
      const dy = toPos.y - fromPos.y;
      const dist = Math.hypot(dx, dy);
      if (dist === 0) continue;
      const mx = (fromPos.x + toPos.x) / 2 - (dy / dist) * dist * 0.08;
      const my = (fromPos.y + toPos.y) / 2 + (dx / dist) * dist * 0.08;
      targets.push(toPos);
      paths.push(`M ${fromPos.x} ${fromPos.y} Q ${mx} ${my} ${toPos.x} ${toPos.y}`);
    }
    if (paths.length === 0) return;
    layers.push({ pins: [fromPos, ...targets], paths, color: ticketPinColor(i) });
  });
  return layers;
}