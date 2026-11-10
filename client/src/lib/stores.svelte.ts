// Store reattivi per lo stato di gioco (Svelte 5 runes).
//
// Un canale pubblico (`PublicState`) e uno privato (`PrivateState`),
// aggiornati applicando i messaggi/SNAPSHOT dal WebSocket.
// Zero logica di gioco: le mosse legali arrivano da `legalMoves` del server.

import type {
  PublicState,
  PrivateState,
  Snapshot,
  ErrorPayload,
  GameState,
  PlayerSummary,
  ClaimedRoute,
  FinalScoreInfo,
  CardColor,
  TicketInfo,
  LegalMoves,
} from './proto';

// ── Store pubblico ────────────────────────────────────────────────────

/** Stato pubblico della partita corrente (reactive via $state). */
class PublicStore {
  gameId = $state<string>('');
  protocolVersion = $state<number>(0);
  state = $state<GameState>('WAITING_FOR_PLAYERS');
  currentTurnPlayerId = $state<string | null>(null);
  lastRound = $state<boolean>(false);
  lastRoundTriggerPlayerId = $state<string | null>(null);
  visibleDeck = $state<(CardColor | null)[]>([]);
  /** Carte rimaste nel mazzo coperto (Fase 3e-server). */
  deckSize = $state<number>(0);
  claimedRoutes = $state<ClaimedRoute[]>([]);
  playersSummary = $state<PlayerSummary[]>([]);
  tunnelReveal = $state<unknown>(null);
  stations = $state<unknown[]>([]);
  finalScores = $state<FinalScoreInfo[] | null>(null);
  longestPathPlayerIds = $state<string[] | null>(null);

  apply(ps: PublicState): void {
    this.gameId = ps.game_id;
    this.protocolVersion = ps.protocol_version;
    this.state = ps.state;
    this.currentTurnPlayerId = ps.current_turn_player_id;
    this.lastRound = ps.last_round;
    this.lastRoundTriggerPlayerId = ps.last_round_trigger_player_id;
    this.visibleDeck = ps.visible_deck;
    this.deckSize = ps.deck_size;
    this.claimedRoutes = ps.claimed_routes;
    this.playersSummary = ps.players_summary;
    this.tunnelReveal = ps.tunnel_reveal;
    this.stations = ps.stations;
    this.finalScores = ps.final_scores;
    this.longestPathPlayerIds = ps.longest_path_player_ids;
  }

  reset(): void {
    this.gameId = '';
    this.protocolVersion = 0;
    this.state = 'WAITING_FOR_PLAYERS';
    this.currentTurnPlayerId = null;
    this.lastRound = false;
    this.lastRoundTriggerPlayerId = null;
    this.visibleDeck = [];
    this.deckSize = 0;
    this.claimedRoutes = [];
    this.playersSummary = [];
    this.tunnelReveal = null;
    this.stations = [];
    this.finalScores = null;
    this.longestPathPlayerIds = null;
  }
}

/** Istanza singleton dello store pubblico. */
export const publicStore = new PublicStore();

// ── Store privato ─────────────────────────────────────────────────────

/** Stato privato del giocatore corrente (reactive via $state). */
class PrivateStore {
  seq = $state<bigint>(0n);
  hand = $state<CardColor[]>([]);
  tickets = $state<TicketInfo[]>([]);
  offeredTickets = $state<TicketInfo[]>([]);
  legalMoves = $state<LegalMoves | null>(null);
  /** Il server ci ha detto chi siamo (dal SNAPSHOT o dalla lobby). */
  playerId = $state<string>('');

  apply(ps: PrivateState): void {
    this.seq = ps.seq;
    this.hand = ps.hand;
    this.tickets = ps.tickets;
    this.offeredTickets = ps.offered_tickets;
    this.legalMoves = ps.legal_moves;
  }

  reset(): void {
    this.seq = 0n;
    this.hand = [];
    this.tickets = [];
    this.offeredTickets = [];
    this.legalMoves = null;
  }
}

/** Istanza singleton dello store privato. */
export const privateStore = new PrivateStore();

// ── Store connessione ─────────────────────────────────────────────────

class ConnectionStore {
  connected = $state<boolean>(false);
  lastError = $state<ErrorPayload | null>(null);
  /** Se true, il client è in stato di reconnect. */
  reconnecting = $state<boolean>(false);
}

export const connectionStore = new ConnectionStore();

// ── Helpers ───────────────────────────────────────────────────────────

/** Applica uno Snapshot completo (dopo connect/reconnect). */
export function applySnapshot(snap: Snapshot): void {
  publicStore.apply(snap.public);
  if (snap.private) {
    privateStore.apply(snap.private);
    // Il playerId non è nello Snapshot direttamente — lo deduciamo
    // dal primo PRIVATE_STATE o lo teniamo dall'identity salvata.
  }
}

/** Reset di tutti gli store (es. uscita dalla partita). */
export function resetStores(): void {
  publicStore.reset();
  privateStore.reset();
  connectionStore.lastError = null;
  connectionStore.reconnecting = false;
}