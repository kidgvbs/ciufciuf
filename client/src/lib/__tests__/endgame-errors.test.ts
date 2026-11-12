import { describe, expect, it } from 'vitest';
import { errorMessage, notConnectedError } from '../errors';
import { borrowedRouteLabel, rankedScores, rulesetHasStations, winnerNames } from '../scores';
import type { FinalScoreInfo, PlayerSummary } from '../proto';
import type { MapData } from '../map-utils';

function fs(playerId: string, total: number, winner: boolean): FinalScoreInfo {
  return {
    playerId,
    routePoints: total,
    ticketCompleted: 0,
    ticketFailed: 0,
    longestPathBonus: 0,
    globetrotterBonus: 0,
    stationPoints: 0,
    total,
    winner,
    borrowedRoutes: [],
  };
}

describe('errorMessage', () => {
  it('preferisce il dettaglio del server', () => {
    expect(
      errorMessage({ seq: 1n, reason: 'NOT_YOUR_TURN', action_ref: null, detail: 'dettaglio' }),
    ).toBe('dettaglio');
  });
  it('ricade sul testo del motivo', () => {
    expect(
      errorMessage({ seq: 1n, reason: 'NOT_YOUR_TURN', action_ref: null, detail: null }),
    ).toBe('Non è il tuo turno');
  });
  it('errore locale di connessione', () => {
    expect(errorMessage(notConnectedError())).toMatch(/Non connesso/);
  });
});

describe('scores', () => {
  const scores = [fs('a', 50, false), fs('b', 80, true), fs('c', 80, true), fs('d', 60, false)];
  const players = [
    { player_id: 'b', name: 'Bea' },
    { player_id: 'c', name: 'Ciro' },
  ] as PlayerSummary[];

  it('ordina vincitori prima, poi per totale', () => {
    expect(rankedScores(scores).map((s) => s.playerId)).toEqual(['b', 'c', 'd', 'a']);
  });
  it('nomi dei vincitori a pari merito', () => {
    expect(winnerNames(scores, players)).toBe('Bea e Ciro');
  });
});

describe('stazioni (Europe)', () => {
  const map = {
    cities: [{ id: 'a', name: 'Alfa' }, { id: 'b', name: 'Beta' }, { id: 'c', name: 'Gamma' }],
    routes: [{ id: 'r1', from: 'b', to: 'c' }],
  } as unknown as MapData;

  it('mostra le stazioni solo se la partita le ha', () => {
    const none = [{ stations_remaining: 0 }] as PlayerSummary[];
    const some = [{ stations_remaining: 2 }] as PlayerSummary[];
    expect(rulesetHasStations(none, [])).toBe(false);
    expect(rulesetHasStations(some, [])).toBe(true);
    expect(rulesetHasStations(none, [{ city_id: 'a', player_id: 'p' }])).toBe(true);
  });

  it('etichetta la rotta in prestito come "Città → A–B"', () => {
    expect(borrowedRouteLabel({ cityId: 'a', routeId: 'r1' }, map)).toBe('Alfa → Beta–Gamma');
  });

  it('senza mappa o rotta usa gli id', () => {
    expect(borrowedRouteLabel({ cityId: 'a', routeId: 'r9' }, map)).toBe('Alfa → r9');
    expect(borrowedRouteLabel({ cityId: 'a', routeId: 'r9' }, null)).toBe('a → r9');
  });
});
