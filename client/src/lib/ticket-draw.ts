// Pesca dei biglietti con le quote per mazzo (ruleset con `chooseFrom`, es. Germany: corti e
// lunghi). Il server dice quanti biglietti pescare in tutto e quanti ne restano per mazzo
// (`legal_moves.ticket_draw`); qui si sceglie solo la ripartizione, l'arbitro resta il server.

import type { TicketDeckCount, TicketDrawMoves } from './proto';

/** Nome del mazzo nell'interfaccia: in Germany il mazzo `regular` sono i biglietti corti. */
export function deckLabel(deck: string): string {
  if (deck === 'regular') return 'Corti';
  if (deck === 'long') return 'Lunghi';
  return deck;
}

/** Ripartizione iniziale: si riempie un mazzo dopo l'altro, nell'ordine del server. */
export function defaultQuotas(draw: TicketDrawMoves): number[] {
  let left = draw.total;
  return draw.available.map((a) => {
    const n = Math.min(left, a.count);
    left -= n;
    return n;
  });
}

/** Valida se la somma è il totale richiesto e nessuna quota supera i biglietti del suo mazzo. */
export function canConfirmQuotas(draw: TicketDrawMoves, quotas: readonly number[]): boolean {
  if (quotas.length !== draw.available.length) return false;
  const sum = quotas.reduce((a, b) => a + b, 0);
  return (
    sum === draw.total &&
    quotas.every((q, i) => Number.isInteger(q) && q >= 0 && q <= (draw.available[i]?.count ?? 0))
  );
}

/** Sposta un biglietto dal mazzo `from` al mazzo `to`, se possibile. */
export function moveQuota(
  draw: TicketDrawMoves,
  quotas: readonly number[],
  from: number,
  to: number,
): number[] {
  const next = [...quotas];
  const fromQ = next[from] ?? 0;
  const toQ = next[to] ?? 0;
  if (from === to || fromQ <= 0 || toQ >= (draw.available[to]?.count ?? 0)) return next;
  next[from] = fromQ - 1;
  next[to] = toQ + 1;
  return next;
}

/** Quote per `DRAW_TICKETS.counts`, senza i mazzi a zero. */
export function quotasPayload(draw: TicketDrawMoves, quotas: readonly number[]): TicketDeckCount[] {
  return draw.available
    .map((a, i) => ({ deck: a.deck, count: quotas[i] ?? 0 }))
    .filter((q) => q.count > 0);
}

/**
 * Un solo invio di `DRAW_TICKETS` per volta: il picker del draft iniziale resta visibile
 * finché non arriva l'offerta, quindi un doppio tap invierebbe due richieste.
 * `release()` quando arriva l'offerta o un errore.
 */
export function createDrawLatch(onChange: (pending: boolean) => void) {
  let pending = false;
  const set = (value: boolean) => {
    if (pending === value) return;
    pending = value;
    onChange(value);
  };
  return {
    get pending() {
      return pending;
    },
    /** Esegue `send` se non c'è un invio in corso; resta bloccato solo se è partito. */
    submit(send: () => boolean): boolean {
      if (pending) return false;
      const sent = send();
      if (sent) set(true);
      return sent;
    },
    release() {
      set(false);
    },
  };
}
