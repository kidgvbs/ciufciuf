// Classifica finale condivisa da TV e controller.

import type { FinalScoreInfo, PlayerSummary } from './proto';

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
