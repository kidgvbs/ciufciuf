// Classifica finale condivisa da TV e controller.

import type { BorrowedRoute, FinalScoreInfo, PlayerSummary, StationPlacement } from './proto';
import type { MapData } from './map-utils';

/** Punteggi ordinati: vincitori prima, poi per totale decrescente. */
export function rankedScores(scores: FinalScoreInfo[]): FinalScoreInfo[] {
  return [...scores].sort(
    (a, b) => Number(b.winner) - Number(a.winner) || b.total - a.total,
  );
}

/** Nomi dei vincitori (pari merito separati da " e "). */
export function winnerNames(scores: FinalScoreInfo[], players: PlayerSummary[]): string {
  return scores
    .filter((fs) => fs.winner)
    .map((fs) => players.find((p) => p.player_id === fs.playerId)?.name ?? fs.playerId)
    .join(' e ');
}

/** `true` se la partita ha le stazioni (Europe): qualcuno ne ha ancora o ne è già stata costruita una. */
export function rulesetHasStations(players: PlayerSummary[], stations: StationPlacement[]): boolean {
  return stations.length > 0 || players.some((p) => p.stations_remaining > 0);
}

/** "Città → Rotta A–B" di una rotta presa in prestito da una stazione; con mappa non caricata, gli id. */
export function borrowedRouteLabel(b: BorrowedRoute, map: MapData | null): string {
  const city = (id: string) => map?.cities.find((c) => c.id === id)?.name ?? id;
  const route = map?.routes.find((r) => r.id === b.routeId);
  const via = route ? `${city(route.from)}–${city(route.to)}` : b.routeId;
  return `${city(b.cityId)} → ${via}`;
}
