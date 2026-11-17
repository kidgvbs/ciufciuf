// Catalogo mappe/ruleset del server (selettore "Nuova partita" e nome del bonus a fine partita).
//
// Il catalogo arriva da `GET /api/rulesets`: nessun elenco statico lato client. La forma della
// risposta viene validata qui (come `isMapData` per la mappa); le card mostrano solo dati del server.
// Vincolo regola d'oro 2 (zero IP del titolare del gioco originale): nessun asset o nome ufficiale —
// le thumbnail sono mini-render SVG originali dalle nostre coordinate (MapCard).

import { isRecord } from './guards';
import { LobbyClient } from './lobby';

/** Bonus di fine partita del ruleset (tipi sconosciuti ammessi: i consumatori li ignorano). */
export interface RulesetBonus {
  /** `longest_path` o `globetrotter` (altri tipi futuri sono ignorati). */
  type: string;
  label: string;
  points: number;
}

export interface RulesetInfo {
  /** Id passato a POST /api/games (esiste in data/rulesets/). */
  id: string;
  /** Nome mostrato sulla card. */
  name: string;
  /** Id della mappa (usato per layout e thumbnail). */
  mapId: string;
  /** Modalità di gioco sulla mappa («Standard», «1910», «Grandi Città», …). */
  mode: string;
  /** Ordine delle modalità sulla stessa mappa (la più bassa è la Standard). */
  modeOrder: number;
  cities: number;
  routes: number;
  /** Biglietti in gioco in questa modalità. */
  tickets: number;
  minPlayers: number;
  maxPlayers: number;
  /** Bonus di fine partita (può essere vuoto). */
  bonuses: RulesetBonus[];
  /** Città da marcare come «grandi» (solo visualizzazione). */
  bigCities: string[];
  /** Biglietti distribuiti nel draft iniziale (somma dei mazzi). */
  initialDealt: number;
  /** Biglietti da tenere nel draft iniziale. */
  initialKeepMin: number;
  /** Biglietti offerti quando se ne pescano. */
  drawCount: number;
  /** Biglietti da tenere quando se ne pescano. */
  drawKeepMin: number;
  /** La mappa ha almeno una rotta tunnel. */
  tunnels: boolean;
  /** La mappa ha almeno una rotta con traghetti. */
  ferries: boolean;
  /** Il ruleset prevede le stazioni. */
  stations: boolean;
  /** Pesca dei biglietti con la scelta tra corti e lunghi (Germany); assente su server vecchi. */
  ticketDrawChoice?: boolean;
  /** Carte Paese (Poland); assente su server vecchi. */
  countryCards?: boolean;
  /** Passeggeri (Germany 2017); assente su server vecchi. */
  passengers?: boolean;
}

/** Mappa con le sue modalità (ruleset) ordinate per `modeOrder`. */
export interface MapGroup {
  mapId: string;
  rulesets: RulesetInfo[];
}

/** Ruleset preselezionato nel form di creazione, se presente nel catalogo. */
export const DEFAULT_RULESET_ID = 'usa_v2';

const LONGEST_PATH = 'longest_path';
const GLOBETROTTER = 'globetrotter';
const STANDARD_MODE = 'Standard';

/** Nome del bonus percorso più lungo quando il catalogo non lo conosce. */
export const FALLBACK_LONGEST_PATH_LABEL = 'Longest Path';

/** Nome del bonus Globetrotter quando il catalogo non lo conosce. */
export const FALLBACK_GLOBETROTTER_LABEL = 'Globetrotter';

const UNEXPECTED_RESPONSE = 'Risposta inattesa dal server';

function isNonEmptyString(v: unknown): v is string {
  return typeof v === 'string' && v !== '';
}

function isCount(v: unknown): v is number {
  return typeof v === 'number' && Number.isInteger(v) && v >= 0;
}

function isRulesetBonus(v: unknown): v is RulesetBonus {
  return isRecord(v) && isNonEmptyString(v.type) && isNonEmptyString(v.label) && typeof v.points === 'number' && Number.isInteger(v.points);
}

/** `true` se `v` ha la forma di `RulesetInfo` (conteggi interi ≥ 0, giocatori 1 ≤ min ≤ max, bonus con tipo/etichetta/punti). */
export function isRulesetInfo(v: unknown): v is RulesetInfo {
  if (!isRecord(v)) return false;
  return (
    isNonEmptyString(v.id) &&
    isNonEmptyString(v.name) &&
    isNonEmptyString(v.mapId) &&
    isCount(v.cities) &&
    isCount(v.routes) &&
    isCount(v.tickets) &&
    isCount(v.minPlayers) &&
    isCount(v.maxPlayers) &&
    v.minPlayers >= 1 &&
    v.minPlayers <= v.maxPlayers &&
    isNonEmptyString(v.mode) &&
    isCount(v.modeOrder) &&
    Array.isArray(v.bonuses) &&
    v.bonuses.every(isRulesetBonus) &&
    Array.isArray(v.bigCities) &&
    v.bigCities.every(isNonEmptyString) &&
    isCount(v.initialDealt) &&
    isCount(v.initialKeepMin) &&
    isCount(v.drawCount) &&
    isCount(v.drawKeepMin) &&
    typeof v.tunnels === 'boolean' &&
    typeof v.ferries === 'boolean' &&
    typeof v.stations === 'boolean' &&
    ['ticketDrawChoice', 'countryCards', 'passengers'].every(
      (k) => v[k] === undefined || typeof v[k] === 'boolean',
    )
  );
}

/** Valida il corpo di `GET /api/rulesets` (`{ rulesets: [...] }`, id univoci); altrimenti lancia «Risposta inattesa dal server». */
export function parseRulesetCatalog(v: unknown): RulesetInfo[] {
  if (!isRecord(v) || !Array.isArray(v.rulesets) || !v.rulesets.every(isRulesetInfo)) {
    throw new Error(UNEXPECTED_RESPONSE);
  }
  const rulesets = v.rulesets as RulesetInfo[];
  if (new Set(rulesets.map((r) => r.id)).size !== rulesets.length) {
    throw new Error(UNEXPECTED_RESPONSE);
  }
  return rulesets;
}

/** Catalogo dei ruleset serviti da `host`. */
export async function loadRulesets(host: string): Promise<RulesetInfo[]> {
  return parseRulesetCatalog(await new LobbyClient(host).getRulesets());
}

/** Sottotitolo della card derivato dai dati, es. «47 città, 101 rotte, con tunnel, traghetti e stazioni». */
export function rulesetBlurb(r: RulesetInfo): string {
  const features: string[] = [];
  if (r.tunnels) features.push('tunnel');
  if (r.ferries) features.push('traghetti');
  if (r.stations) features.push('stazioni');
  const base = `${r.cities} città, ${r.routes} rotte`;
  if (features.length === 0) return base;
  const list =
    features.length === 1
      ? features[0]
      : `${features.slice(0, -1).join(', ')} e ${features[features.length - 1]}`;
  return `${base}, con ${list}`;
}

/** Ruleset da preselezionare: `DEFAULT_RULESET_ID` se nel catalogo, altrimenti il primo; `null` se il catalogo è vuoto. */
export function defaultRulesetId(catalog: readonly RulesetInfo[]): string | null {
  return (catalog.find((r) => r.id === DEFAULT_RULESET_ID) ?? catalog[0])?.id ?? null;
}

/** Etichetta del bonus di tipo `type` per `rulesetId`; `null` se il ruleset non lo prevede o è assente. */
function bonusOf(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
  type: string,
): RulesetBonus | null {
  return catalog.find((r) => r.id === rulesetId)?.bonuses.find((b) => b.type === type) ?? null;
}

/** Nome del bonus percorso più lungo per `rulesetId`; 'Longest Path' se il ruleset è assente dal catalogo o non lo prevede. */
export function longestPathLabel(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): string {
  return bonusOf(catalog, rulesetId, LONGEST_PATH)?.label ?? FALLBACK_LONGEST_PATH_LABEL;
}

/** Nome del bonus Globetrotter per `rulesetId`; 'Globetrotter' se assente dal catalogo o non previsto. */
export function globetrotterLabel(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): string {
  return bonusOf(catalog, rulesetId, GLOBETROTTER)?.label ?? FALLBACK_GLOBETROTTER_LABEL;
}

/** `true` se il ruleset prevede il bonus percorso più lungo. Catalogo non pronto o ruleset ignoto → `true` (comportamento storico). */
export function hasLongestPath(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): boolean {
  return catalog.some((r) => r.id === rulesetId) ? bonusOf(catalog, rulesetId, LONGEST_PATH) !== null : true;
}

/** `true` se il ruleset prevede il bonus Globetrotter (ruleset ignoto → `false`). */
export function hasGlobetrotter(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): boolean {
  return bonusOf(catalog, rulesetId, GLOBETROTTER) !== null;
}

/** Ids delle grandi città da marcare sulla mappa di `rulesetId` (vuoto se assente o senza). */
export function bigCities(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): string[] {
  return catalog.find((r) => r.id === rulesetId)?.bigCities ?? [];
}

/** Titolo del ruleset: «USA · 1910»; solo il nome se la modalità è «Standard». */
export function rulesetTitle(r: Pick<RulesetInfo, 'name' | 'mode'>): string {
  return r.mode === STANDARD_MODE ? r.name : `${r.name} · ${r.mode}`;
}

/** Titolo del ruleset `rulesetId` dal catalogo; `null` se il ruleset non è noto. */
export function rulesetTitleById(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): string | null {
  const r = catalog.find((x) => x.id === rulesetId);
  return r ? rulesetTitle(r) : null;
}

/** Mappe nell'ordine di prima comparsa nel catalogo, ciascuna con i suoi ruleset ordinati per `modeOrder`. */
export function groupByMap(catalog: readonly RulesetInfo[]): MapGroup[] {
  const groups = new Map<string, RulesetInfo[]>();
  for (const r of catalog) {
    const list = groups.get(r.mapId);
    if (list) list.push(r);
    else groups.set(r.mapId, [r]);
  }
  return [...groups].map(([mapId, rulesets]) => ({
    mapId,
    // `sort` è stabile: a parità di `modeOrder` resta l'ordine del catalogo.
    rulesets: [...rulesets].sort((a, b) => a.modeOrder - b.modeOrder),
  }));
}

/** Sottotitolo della modalità derivato dai dati, es. «35 biglietti in gioco · ricevi 4, tieni 2 · pesca 3, tieni 1 · Globetrotter +15». */
export function modeBlurb(r: RulesetInfo): string {
  const choose = r.ticketDrawChoice === true ? ' tra corti e lunghi' : '';
  const parts = [
    `${r.tickets} biglietti in gioco`,
    `ricevi ${r.initialDealt}${choose}, tieni ${r.initialKeepMin}`,
    `pesca ${r.drawCount}${choose}, tieni ${r.drawKeepMin}`,
  ];
  if (r.passengers === true) parts.push('passeggeri');
  if (r.countryCards === true) parts.push('carte Paese');
  const known = r.bonuses.filter((b) => b.type === LONGEST_PATH || b.type === GLOBETROTTER);
  if (known.length === 0) parts.push('nessun bonus');
  else for (const b of known) parts.push(`${b.label} +${b.points}`);
  return parts.join(' · ');
}
