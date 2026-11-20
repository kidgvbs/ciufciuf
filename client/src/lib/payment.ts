// Aiuti del controller per scegliere le carte da pagare (rotta, stazione, carte extra del tunnel).
//
// Sono SOLO guardie di UX: dicono se la mano copre il pagamento scelto e propongono un
// default sensato. Le regole (quali mosse sono ammesse, quanto costano) arrivano da
// `legalMoves` e l'arbitro resta il server.

import type { CardColor } from './proto';

/** Colori delle carte non-locomotiva, nell'ordine usato per i default. */
export const PLAIN_CARD_COLORS: readonly CardColor[] = [
  'RED', 'ORANGE', 'YELLOW', 'GREEN', 'BLUE', 'PURPLE', 'BLACK', 'WHITE',
];

/** Un pagamento di `total` carte: `colorCount` carte del colore scelto in mano, `locoInHand` locomotive in mano. */
export interface PaymentSpec {
  total: number;
  /** Locomotive obbligatorie (traghetti); 0 altrove. */
  minLocos: number;
  colorCount: number;
  locoInHand: number;
}

/** Quante carte `color` ci sono in `hand`. */
export function countCards(hand: readonly CardColor[], color: CardColor): number {
  let n = 0;
  for (const c of hand) if (c === color) n++;
  return n;
}

/** Colore non-locomotiva più presente in mano (a parità il primo di `PLAIN_CARD_COLORS`); `null` se non ce ne sono. */
export function mostCommonColor(hand: readonly CardColor[]): CardColor | null {
  let best: CardColor | null = null;
  let bestN = 0;
  for (const c of PLAIN_CARD_COLORS) {
    const n = countCards(hand, c);
    if (n > bestN) {
      bestN = n;
      best = c;
    }
  }
  return best;
}

/**
 * Locomotive proposte di default: quante servono a completare il pagamento col colore scelto,
 * mai meno delle obbligatorie e, se la mano lo permette, non più di quelle in mano o del totale.
 */
export function suggestLocos({ total, minLocos, colorCount, locoInHand }: PaymentSpec): number {
  const want = Math.max(minLocos, total - colorCount);
  const cap = Math.max(minLocos, Math.min(locoInHand, total));
  return Math.min(want, cap);
}

/** `true` se la mano copre il pagamento con `locos` locomotive e le restanti del colore scelto. */
export function paymentCovered(spec: PaymentSpec, locos: number): boolean {
  const { total, minLocos, colorCount, locoInHand } = spec;
  if (!Number.isInteger(locos) || locos < minLocos || locos > total || locos > locoInHand) return false;
  return total - locos === 0 || colorCount >= total - locos;
}

/** Colore carta corrispondente al colore di una rotta del JSON (`null` = grigia). */
export function routeColorToCard(color: string): CardColor | null {
  const m: Record<string, CardColor> = {
    Red: 'RED', Orange: 'ORANGE', Yellow: 'YELLOW', Green: 'GREEN',
    Blue: 'BLUE', Purple: 'PURPLE', Black: 'BLACK', White: 'WHITE',
  };
  return m[color] ?? null;
}

/** Dati di una rotta che servono alla guardia di pagamento. */
export interface PayableRoute {
  len: number;
  color: string;
  /** Locomotive obbligatorie (traghetti); assente = 0. */
  ferries?: number;
  /** Tunnel (le locomotive possono essere ammesse solo qui, `locomotives.usableOn`). */
  tunnel?: boolean;
  /** Ogni spazio si paga anche con questo numero di carte qualsiasi (Murmansk–Lieksa). */
  anyCardsPerSpace?: number;
}

/** Regole di pagamento del ruleset dal catalogo del server (assenti = regole classiche). */
export interface RulesetPayment {
  /** `locomotives.usableOn` (`tunnel`, `ferry`); `null` = locomotive ammesse su ogni rotta. */
  usableOn: readonly string[] | null;
  /** `ferries.anyCardsPerLocomotive`; `null` = nessuna sostituzione sui traghetti. */
  ferryAnyCards: number | null;
  /** `ferryCards.wavesPerCard`: spazi onda che una carta Traghetto copre al massimo (Italy); assente = nessuna carta Traghetto. */
  ferryCardWaves?: number;
}

/** Regole di pagamento di una rotta, risolte da rotta e ruleset (specchio di `PaymentSpec` del motore). */
export interface RoutePaymentRules {
  len: number;
  /** Spazi locomotiva (traghetto). */
  ferries: number;
  /** Una locomotiva può pagare da sola uno spazio colore. */
  singleLoco: boolean;
  /** Carte qualsiasi (locomotive comprese) che pagano uno spazio; `null` = nessuna sostituzione. */
  groupSize: number | null;
  /** Spazi onda che una carta Traghetto copre al massimo su questa rotta; assente = carte Traghetto non ammesse. */
  ferryCardWaves?: number;
}

/**
 * Regole di pagamento di `route`. Se la rotta ammette due dimensioni di gruppo (`anyCardsPerSpace` e
 * traghetto) vale `anyCardsPerSpace`: i dati non hanno rotte così e il server resta l'arbitro.
 */
export function routePaymentRules(
  route: PayableRoute,
  ruleset: RulesetPayment | null = null,
): RoutePaymentRules {
  const ferries = route.ferries ?? 0;
  const usableOn = ruleset?.usableOn ?? null;
  const singleLoco =
    usableOn === null ||
    (route.tunnel === true && usableOn.includes('tunnel')) ||
    (ferries > 0 && usableOn.includes('ferry'));
  const ferryAny = ferries > 0 ? (ruleset?.ferryAnyCards ?? null) : null;
  const ferryCardWaves = ferries > 0 ? (ruleset?.ferryCardWaves ?? 0) : 0;
  return {
    len: route.len,
    ferries,
    singleLoco,
    groupSize: route.anyCardsPerSpace ?? ferryAny,
    ...(ferryCardWaves > 0 ? { ferryCardWaves } : {}),
  };
}

/**
 * Un pagamento strutturato: `locos` locomotive singole, `groups` gruppi di carte qualsiasi e le
 * altre `len - locos - groups` carte singole del colore scelto.
 */
export interface RoutePayment {
  locos: number;
  groups: number;
}

/**
 * `true` se la mano copre il pagamento (`color` nullo = nessuna carta singola colorata): le carte
 * singole del colore, le locomotive e le carte dei gruppi (prese dal resto della mano, locomotive
 * comprese) esistono, e gli spazi locomotiva del traghetto sono coperti da locomotive o gruppi.
 */
export function routePaymentValid(
  rules: RoutePaymentRules,
  hand: readonly CardColor[],
  color: CardColor | null,
  { locos, groups }: RoutePayment,
): boolean {
  const { len, ferries, singleLoco, groupSize } = rules;
  if (!Number.isInteger(locos) || !Number.isInteger(groups) || locos < 0 || groups < 0) return false;
  if (groups > 0 && groupSize === null) return false;
  const singles = len - locos - groups;
  if (singles < 0) return false;
  // Spazi locomotiva: locomotive (sempre ammesse) o gruppi; sugli spazi colore le locomotive solo se ammesse.
  if (singleLoco ? locos + groups < ferries : locos > ferries || ferries - locos > groups) return false;
  const colorCount = color !== null && color !== 'LOCOMOTIVE' ? countCards(hand, color) : 0;
  if (singles > colorCount || locos > countCards(hand, 'LOCOMOTIVE')) return false;
  return hand.length - singles - locos >= groups * (groupSize ?? 0);
}

/** Primo pagamento valido col colore dato (meno gruppi, poi meno locomotive); `null` se la mano non basta. */
export function findRoutePayment(
  rules: RoutePaymentRules,
  hand: readonly CardColor[],
  color: CardColor | null,
): RoutePayment | null {
  const maxGroups = rules.groupSize === null ? 0 : rules.len;
  for (let groups = 0; groups <= maxGroups; groups++) {
    for (let locos = 0; locos + groups <= rules.len; locos++) {
      if (routePaymentValid(rules, hand, color, { locos, groups })) return { locos, groups };
    }
  }
  return null;
}

/** Carte Traghetto giocabili su una rotta con le `owned` in mano: al più una per spazio onda (ognuna ne copre almeno uno). */
export function maxFerryCards(rules: RoutePaymentRules, owned: number): number {
  if ((rules.ferryCardWaves ?? 0) <= 0) return 0;
  return Math.max(0, Math.min(owned, rules.ferries));
}

/**
 * Onde coperte da `ferryCards` carte Traghetto: fino a `ferryCardWaves` a carta, mai più delle onde
 * della rotta. Il giocatore potrebbe sceglierne di meno, ma coprirne il più possibile costa meno carte.
 */
export function ferryCoveredWaves(rules: RoutePaymentRules, ferryCards: number): number {
  if (ferryCards <= 0) return 0;
  return Math.min(ferryCards * (rules.ferryCardWaves ?? 0), rules.ferries);
}

/**
 * Regole per le carte treno quando `ferryCards` carte Traghetto coprono le onde: la rotta ha tante
 * carte e tanti spazi onda (locomotive) in meno quante sono le onde coperte.
 */
export function rulesWithFerryCards(rules: RoutePaymentRules, ferryCards: number): RoutePaymentRules {
  const covered = ferryCoveredWaves(rules, ferryCards);
  return { ...rules, len: rules.len - covered, ferries: rules.ferries - covered };
}

/**
 * Primo pagamento valido col colore dato usando il minimo di carte Traghetto (al più `owned`); `null`
 * se la mano non basta nemmeno con tutte. Il pagamento riguarda le carte treno, con le regole di
 * `rulesWithFerryCards`.
 */
export function findRoutePaymentWithFerry(
  rules: RoutePaymentRules,
  hand: readonly CardColor[],
  color: CardColor | null,
  owned: number,
): (RoutePayment & { ferryCards: number }) | null {
  for (let ferryCards = 0; ferryCards <= maxFerryCards(rules, owned); ferryCards++) {
    const found = findRoutePayment(rulesWithFerryCards(rules, ferryCards), hand, color);
    if (found) return { ...found, ferryCards };
  }
  return null;
}

/** Limiti del selettore di locomotive singole: da `min` (traghetto senza sostituzioni) a `max`. */
export function locoBounds(
  rules: RoutePaymentRules,
  locoInHand: number,
): { min: number; max: number } {
  const min = rules.groupSize === null ? rules.ferries : 0;
  const cap = rules.singleLoco ? rules.len : rules.ferries;
  return { min, max: Math.min(locoInHand, cap) };
}

/** Massimo di gruppi di carte qualsiasi componibili con la mano (0 senza sostituzioni). */
export function maxGroups(rules: RoutePaymentRules, hand: readonly CardColor[]): number {
  if (rules.groupSize === null) return 0;
  return Math.min(rules.len, Math.floor(hand.length / rules.groupSize));
}

/**
 * Pagamento proposto di default: il primo valido (`findRoutePayment`); se la mano non basta, le
 * locomotive minime classiche (il popup mostra che non è valido).
 */
export function suggestRoutePayment(
  rules: RoutePaymentRules,
  hand: readonly CardColor[],
  color: CardColor | null,
): RoutePayment {
  const found = findRoutePayment(rules, hand, color);
  if (found) return found;
  const locoInHand = countCards(hand, 'LOCOMOTIVE');
  const { min, max } = locoBounds(rules, locoInHand);
  const locos = suggestLocos({
    total: rules.len,
    minLocos: min,
    colorCount: color ? countCards(hand, color) : 0,
    locoInHand,
  });
  return { locos: Math.min(Math.max(locos, min), Math.max(min, max)), groups: 0 };
}

/**
 * Guardia di UX per l'evidenziazione: `true` se la mano può pagare `route` usando il colore
 * `selected` più le locomotive. `selected` nullo = nessun filtro; `LOCOMOTIVE` = sole locomotive;
 * un colore = la rotta deve essere di quel colore (o grigia) e le carte del colore, le locomotive
 * ammesse e i gruppi di carte qualsiasi devono coprire la lunghezza, locomotive dei traghetti comprese.
 * Non sostituisce `legalMoves.routes`: va applicata solo a rotte già legali.
 */
export function routePayableWith(
  route: PayableRoute,
  hand: readonly CardColor[],
  selected: CardColor | null,
  ruleset: RulesetPayment | null = null,
  ferryOwned = 0,
): boolean {
  if (selected === null) return true;
  const rules = routePaymentRules(route, ruleset);
  if (selected === 'LOCOMOTIVE') return findRoutePaymentWithFerry(rules, hand, null, ferryOwned) !== null;
  const routeColor = routeColorToCard(route.color);
  if (routeColor !== null && routeColor !== selected) return false;
  return findRoutePaymentWithFerry(rules, hand, selected, ferryOwned) !== null;
}

/**
 * Delle rotte legali `routeIds`, quelle da evidenziare sulla mappa con il colore `selected`
 * (vedi `routePayableWith`); senza selezione restano tutte. Una rotta sconosciuta non passa.
 */
export function payableRouteIds(
  routeIds: readonly string[],
  routes: ReadonlyMap<string, PayableRoute>,
  hand: readonly CardColor[],
  selected: CardColor | null,
  ruleset: RulesetPayment | null = null,
  ferryOwned = 0,
): string[] {
  if (selected === null) return [...routeIds];
  return routeIds.filter((id) => {
    const route = routes.get(id);
    return route !== undefined && routePayableWith(route, hand, selected, ruleset, ferryOwned);
  });
}

/**
 * Carte da inviare: `total - locos` del colore `color` e `locos` locomotive, prese dalla mano.
 * Con `color` nullo (solo locomotive) non si prendono carte colorate.
 */
export function paymentCards(
  hand: readonly CardColor[],
  color: CardColor | null,
  total: number,
  locos: number,
): CardColor[] {
  const cards: CardColor[] = [];
  const coloredNeeded = total - locos;
  if (color !== null && color !== 'LOCOMOTIVE') {
    for (const c of hand) {
      if (c === color && cards.length < coloredNeeded) cards.push(c);
    }
  }
  let taken = 0;
  for (const c of hand) {
    if (c === 'LOCOMOTIVE' && taken < locos) {
      cards.push(c);
      taken++;
    }
  }
  return cards;
}

/**
 * Carte da inviare per un pagamento strutturato: le singole del colore, le locomotive singole e,
 * per ogni gruppo, `groupSize` carte qualsiasi prese dal resto della mano: prima le carte dei
 * colori meno presenti, le locomotive per ultime (le più utili restano in mano).
 */
export function routePaymentCards(
  rules: RoutePaymentRules,
  hand: readonly CardColor[],
  color: CardColor | null,
  { locos, groups }: RoutePayment,
): CardColor[] {
  const cards = paymentCards(hand, color, rules.len - groups, locos);
  const rest = [...hand];
  for (const used of cards) rest.splice(rest.indexOf(used), 1);
  const needed = groups * (rules.groupSize ?? 0);
  const copies = new Map(PLAIN_CARD_COLORS.map((c) => [c, countCards(rest, c)]));
  // Le locomotive per ultime; tra i colori, prima quelli con meno carte.
  const rank = (c: CardColor) => (c === 'LOCOMOTIVE' ? hand.length + 1 : (copies.get(c) ?? 0));
  rest.sort((a, b) => rank(a) - rank(b));
  return cards.concat(rest.slice(0, needed));
}

/**
 * Colore delle carte colorate ammesse per il costo extra di un tunnel: quello del tunnel,
 * oppure `null` se il tunnel è `LOCOMOTIVE` (solo locomotive).
 */
export function tunnelPayColor(color: CardColor): CardColor | null {
  return color === 'LOCOMOTIVE' ? null : color;
}

/** `true` se la carta rivelata aumenta il costo del tunnel (stesso colore o locomotiva; solo locomotiva se il colore è `LOCOMOTIVE`). */
export function tunnelCardRaisesCost(card: CardColor, color: CardColor): boolean {
  return card === 'LOCOMOTIVE' || (color !== 'LOCOMOTIVE' && card === color);
}
