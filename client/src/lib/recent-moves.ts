// Testi delle "Ultime mosse" della TV: funzioni pure, nessuna regola di gioco.
// Il server manda le mosse già risolte (`PublicState.recent_moves`); qui si traducono in righe
// leggibili con i nomi di giocatori, città e colori. Id sconosciuti → si mostra l'id.

import type { CardColor, PlayerColor, RecentMove, TunnelOutcome } from './proto';
import { CARD_COLOR_NAME } from './colors';
import { tunnelCardRaisesCost } from './payment';

/** Quante mosse mostra la TV (le più recenti): il resto non deve spingere via le carte scoperte. */
export const RECENT_MOVES_SHOWN = 5;

export interface MoveContext {
  players: ReadonlyArray<{ player_id: string; name: string; color: PlayerColor }>;
  map: {
    cities: ReadonlyArray<{ id: string; name: string }>;
    routes: ReadonlyArray<{ id: string; from: string; to: string }>;
  } | null;
}

export interface MoveLine {
  /** Nome del giocatore (l'id se sconosciuto). */
  name: string;
  /** Colore del giocatore; `null` se sconosciuto. */
  color: PlayerColor | null;
  /** Testo dopo il nome (comincia con uno spazio o con ":"). */
  rest: string;
  /** Solo tunnel: carte rivelate, con l'evidenza di quelle che alzano il costo. */
  cards?: Array<{ color: CardColor; raises: boolean }>;
  /** Solo tunnel: testo dopo le carte rivelate. */
  suffix?: string;
  /** Riga intera in testo semplice (nome + rest + carte + suffix). */
  text: string;
}

const OUTCOME_LABEL: Record<TunnelOutcome, string> = {
  CLAIMED: 'preso subito',
  PENDING: 'in attesa',
  PAID: 'pagato',
  DECLINED: 'rinuncia',
};

/** Le ultime `limit` mosse, dalla più recente. */
export function latestMoves(moves: readonly RecentMove[], limit: number = RECENT_MOVES_SHOWN): RecentMove[] {
  return moves.slice(-limit).reverse();
}

function drawCardsText(deck: number, faceUp: readonly CardColor[]): string {
  const total = deck + faceUp.length;
  const parts: string[] = [];
  if (deck > 0) parts.push(`${deck} ${deck === 1 ? 'coperta' : 'coperte'}`);
  for (const c of faceUp) parts.push(CARD_COLOR_NAME[c]);
  const detail = parts.length > 0 ? ` (${parts.join(' + ')})` : '';
  return ` pesca ${total} ${total === 1 ? 'carta' : 'carte'}${detail}`;
}

/** Riga di una mossa recente. */
export function describeMove(move: RecentMove, ctx: MoveContext): MoveLine {
  const player = ctx.players.find((p) => p.player_id === move.player_id);
  const name = player?.name ?? move.player_id;
  const color = player?.color ?? null;
  const city = (id: string) => ctx.map?.cities.find((c) => c.id === id)?.name ?? id;
  const route = (id: string) => {
    const r = ctx.map?.routes.find((x) => x.id === id);
    return r ? `${city(r.from)} → ${city(r.to)}` : id;
  };
  const line = (rest: string): MoveLine => ({ name, color, rest, text: name + rest });

  switch (move.kind) {
    case 'DRAW_CARDS':
      return line(drawCardsText(move.deck, move.face_up));
    case 'DRAW_TICKETS':
      return line(
        move.kept === null
          ? ' sceglie i biglietti…'
          : ` prende ${move.kept} ${move.kept === 1 ? 'biglietto' : 'biglietti'}`,
      );
    case 'CLAIM_ROUTE':
      return line(`: ${route(move.route_id)} (+${move.points})`);
    case 'TUNNEL': {
      const cards = (move.revealed ?? []).map((c) => ({ color: c, raises: tunnelCardRaisesCost(c, move.color) }));
      const outcome = OUTCOME_LABEL[move.outcome];
      const extra = move.extra_cost > 0 ? `+${move.extra_cost}` : 'nessun extra';
      const head = `: tunnel ${route(move.route_id)} — ${cards.length > 0 ? 'rivelate' : 'nessuna carta rivelata'}`;
      const suffix = ` · ${extra} → ${outcome}`;
      const names = cards.map((c) => CARD_COLOR_NAME[c.color]).join(', ');
      return {
        name,
        color,
        rest: head,
        cards,
        suffix,
        text: name + head + (names ? ` ${names}` : '') + suffix,
      };
    }
    case 'BUILD_STATION':
      return line(` costruisce una stazione a ${city(move.city_id)}`);
    default:
      // Mossa di una versione più recente del server: riga neutra, la TV non si blocca.
      return line(' ha fatto una mossa');
  }
}
