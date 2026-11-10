// Testi italiani per gli errori del server, mostrati nel banner del controller.

import type { ErrorPayload, ErrorReason } from './proto';
import type { FatalReason } from './ws';

const REASON_TEXT: Record<ErrorReason, string> = {
  INSUFFICIENT_CARDS: 'Carte insufficienti per questa rotta',
  ROUTE_ALREADY_CLAIMED: 'Rotta già presa',
  ROUTE_BLOCKED_DOUBLE_VARIANT: 'Rotta doppia bloccata con pochi giocatori',
  INVALID_ACTION: 'Azione non valida',
  NOT_YOUR_TURN: 'Non è il tuo turno',
  WRONG_STATE: 'Azione non disponibile in questo momento',
  TICKET_CHOICE_OUT_OF_RANGE: 'Scelta dei biglietti non valida',
  GAME_NOT_FOUND: 'Partita non trovata',
  UNAUTHORIZED: 'Accesso non autorizzato',
  GAME_FULL: 'Partita al completo',
  NAME_TAKEN: 'Nome già in uso',
};

/** Messaggio leggibile: il dettaglio del server se c'è, altrimenti il motivo. */
export function errorMessage(err: ErrorPayload): string {
  return err.detail ?? REASON_TEXT[err.reason] ?? 'Errore dal server';
}

/** Messaggio italiano mostrato quando la connessione viene abbandonata senza riprovare. */
export function fatalMessage(reason: FatalReason): string {
  switch (reason) {
    case 'GAME_NOT_FOUND':
      return 'La partita non esiste più';
    case 'UNAUTHORIZED':
      return 'Sessione non valida';
    case 'PROTOCOL_VERSION':
      return 'Versione incompatibile con il server: ricarica la pagina';
  }
}

/** Errore locale: l'azione non è partita perché il WebSocket è chiuso. */
export function notConnectedError(): ErrorPayload {
  return {
    seq: 0n,
    reason: 'INVALID_ACTION',
    action_ref: null,
    detail: 'Non connesso al server: riprova tra un attimo',
  };
}
