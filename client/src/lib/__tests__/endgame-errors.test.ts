import { describe, expect, it } from 'vitest';
import { errorMessage, notConnectedError } from '../errors';
import { rankedScores, winnerNames } from '../scores';
import type { FinalScoreInfo, PlayerSummary } from '../proto';

function fs(playerId: string, total: number, winner: boolean): FinalScoreInfo {
  return {
    playerId,
    routePoints: total,
    ticketCompleted: 0,
    ticketFailed: 0,
    longestPathBonus: 0,
    total,
    winner,
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
