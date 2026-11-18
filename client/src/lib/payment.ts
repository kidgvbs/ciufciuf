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
}

/**
 * Guardia di UX per l'evidenziazione: `true` se la mano può pagare `route` usando il colore
 * `selected` più le locomotive. `selected` nullo = nessun filtro; `LOCOMOTIVE` = sole locomotive;
 * un colore = la rotta deve essere di quel colore (o grigia) e le carte del colore più le
 * locomotive devono coprire la lunghezza, locomotive dei traghetti comprese.
 * Non sostituisce `legalMoves.routes`: va applicata solo a rotte già legali.
 */
export function routePayableWith(
  route: PayableRoute,
  hand: readonly CardColor[],
  selected: CardColor | null,
): boolean {
  if (selected === null) return true;
  const locoInHand = countCards(hand, 'LOCOMOTIVE');
  const minLocos = route.ferries ?? 0;
  if (selected === 'LOCOMOTIVE') {
    return paymentCovered({ total: route.len, minLocos, colorCount: 0, locoInHand }, route.len);
  }
  const routeColor = routeColorToCard(route.color);
  if (routeColor !== null && routeColor !== selected) return false;
  const spec = { total: route.len, minLocos, colorCount: countCards(hand, selected), locoInHand };
  return paymentCovered(spec, suggestLocos(spec));
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
): string[] {
  if (selected === null) return [...routeIds];
  return routeIds.filter((id) => {
    const route = routes.get(id);
    return route !== undefined && routePayableWith(route, hand, selected);
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
