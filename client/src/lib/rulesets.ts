// Catalogo mappe/ruleset del server (selettore "Nuova partita" e nome del bonus a fine partita).
//
// Il catalogo arriva da `GET /api/rulesets`: nessun elenco statico lato client. La forma della
// risposta viene validata qui (come `isMapData` per la mappa); le card mostrano solo dati del server.
// Vincolo regola d'oro 2 (zero IP del titolare del gioco originale): nessun asset o nome ufficiale —
// le thumbnail sono mini-render SVG originali dalle nostre coordinate (MapCard).

import { isRecord } from './guards';
import { LobbyClient } from './lobby';

export interface RulesetInfo {
  /** Id passato a POST /api/games (esiste in data/rulesets/). */
  id: string;
  /** Nome mostrato sulla card. */
  name: string;
  /** Id della mappa (usato per layout e thumbnail). */
  mapId: string;
  cities: number;
  routes: number;
  tickets: number;
  minPlayers: number;
  maxPlayers: number;
  /** Nome del bonus per il percorso più lungo (varia per mappa). */
  bonusLabel: string;
  /** La mappa ha almeno una rotta tunnel. */
  tunnels: boolean;
  /** La mappa ha almeno una rotta con traghetti. */
  ferries: boolean;
  /** Il ruleset prevede le stazioni. */
  stations: boolean;
}

/** Ruleset preselezionato nel form di creazione, se presente nel catalogo. */
export const DEFAULT_RULESET_ID = 'usa_v2';

/** Nome del bonus quando il catalogo non è disponibile o non conosce il ruleset. */
export const FALLBACK_BONUS_LABEL = 'Longest Path';

const UNEXPECTED_RESPONSE = 'Risposta inattesa dal server';

function isNonEmptyString(v: unknown): v is string {
  return typeof v === 'string' && v !== '';
}

function isCount(v: unknown): v is number {
  return typeof v === 'number' && Number.isInteger(v) && v >= 0;
}

/** `true` se `v` ha la forma di `RulesetInfo` (conteggi interi ≥ 0, giocatori 1 ≤ min ≤ max). */
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
    isNonEmptyString(v.bonusLabel) &&
    typeof v.tunnels === 'boolean' &&
    typeof v.ferries === 'boolean' &&
    typeof v.stations === 'boolean'
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

/** Nome del bonus percorso più lungo per `rulesetId`; 'Longest Path' se il ruleset è assente dal catalogo. */
export function bonusLabel(
  catalog: readonly RulesetInfo[],
  rulesetId: string | null | undefined,
): string {
  return catalog.find((r) => r.id === rulesetId)?.bonusLabel ?? FALLBACK_BONUS_LABEL;
}
