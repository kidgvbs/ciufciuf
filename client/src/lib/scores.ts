// Classifica finale condivisa da TV e controller.

import type { BorrowedRoute, FinalScoreInfo, PlayerSummary, StationPlacement, TicketResultInfo } from './proto';
import { ticketLabel, type MapData } from './map-utils';

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

/**
 * Riga riassuntiva del bonus della rete più grande (Asian Explorer): «Anna, Luca (9 città)», cioè chi
 * ha preso il bonus e la dimensione della rete (uguale per tutti i pari merito); `null` se nessuno l'ha preso.
 */
export function largestNetworkWinners(scores: FinalScoreInfo[], players: PlayerSummary[]): string | null {
  const winners = scores.filter((fs) => fs.largestNetworkBonus > 0);
  const [first] = winners;
  if (first === undefined) return null;
  const names = winners.map((fs) => players.find((p) => p.player_id === fs.playerId)?.name ?? fs.playerId);
  return `${names.join(', ')} (${first.largestNetwork} città)`;
}

/**
 * Bonus Regioni di un giocatore: «+6 (7 + 6 Regioni)» (punti e Regioni di ogni rete, in ordine
 * decrescente come dal server) oppure «0 (4 Regioni)» sotto la soglia; `null` senza reti.
 */
export function regionsText(bonus: number, networks: readonly number[]): string | null {
  if (networks.length === 0) return null;
  return `${bonus > 0 ? `+${bonus}` : '0'} (${networks.join(' + ')} Regioni)`;
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

/**
 * Una riga del dettaglio biglietti del punteggio finale: «✓ Origine → Destinazione realizzata +N»
 * per un biglietto completato (con più destinazioni solo quella realizzata), «✗ Origine → A / B −N»
 * per uno fallito (tutte le destinazioni). Con mappa o biglietto sconosciuti restano gli id.
 * Un biglietto che conta per il bonus Mandala (due percorsi distinti) ha il suffisso « 🪷».
 */
export function ticketResultLabel(r: TicketResultInfo, map: MapData | null): string {
  const city = (id: string) => map?.cities.find((c) => c.id === id)?.name ?? id;
  const def = map?.tickets.find((t) => t.id === r.ticketId);
  if (r.completed) {
    const to = r.cityId !== null ? city(r.cityId) : '';
    return `✓ ${def ? city(def.from) : r.ticketId} → ${to} +${r.points}${r.mandala ? ' 🪷' : ''}`;
  }
  return `✗ ${def ? ticketLabel(def, city) : r.ticketId} ${r.points}`;
}
