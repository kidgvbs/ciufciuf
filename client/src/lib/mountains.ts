// Rotte di montagna (Legendary Asia): le X sulla rotta, i treni scartati e i punti per treno.
//
// Client scemo: il costo vero lo applica il server (treni = lunghezza + X, punti per treno dal
// ruleset); qui solo il testo del popup di reclamo, dai dati della mappa e del catalogo.

import type { RouteData } from './map-utils';

/** Simboli X della rotta (0 = rotta normale). */
export function mountainSymbols(route: Pick<RouteData, 'mountains'>): number {
  return route.mountains ?? 0;
}

/**
 * Testo del costo di montagna di una rotta, es. «⛰ −2 treni, +4 punti» (k = 2, 2 punti per treno);
 * `null` se la rotta non ha X. Con i punti per treno non ancora noti (catalogo non pronto) solo i treni.
 */
export function mountainClaimNote(
  route: Pick<RouteData, 'mountains'>,
  pointsPerTrain: number | null,
): string | null {
  const k = mountainSymbols(route);
  if (k === 0) return null;
  const trains = `−${k} ${k === 1 ? 'treno' : 'treni'}`;
  if (pointsPerTrain === null) return `⛰ ${trains}`;
  const points = k * pointsPerTrain;
  return `⛰ ${trains}, +${points} ${points === 1 ? 'punto' : 'punti'}`;
}
