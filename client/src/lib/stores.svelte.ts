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
  StationPlacement,
  TunnelReveal,
  TicketInfo,
  LegalMoves,
  RecentMove,
  CountryCards,
  CityPassengers,
} from './proto';
import { normalizePublicState } from './public-state';

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
  /** Carte rimaste nel mazzo coperto. */
  deckSize = $state<number>(0);
  claimedRoutes = $state<ClaimedRoute[]>([]);
  playersSummary = $state<PlayerSummary[]>([]);
  tunnelReveal = $state<TunnelReveal | null>(null);
  stations = $state<StationPlacement[]>([]);
  finalScores = $state<FinalScoreInfo[] | null>(null);
  longestPathPlayerIds = $state<string[] | null>(null);
  /** Rotte del percorso più lungo di ogni giocatore in `longestPathPlayerIds` (solo a GAME_OVER). */
  longestPathRouteIds = $state<string[]>([]);
  /** Ultime mosse (la più vecchia per prima), dal server. */
  recentMoves = $state<RecentMove[]>([]);
  /** Poland: carte Paese rimaste in ogni pila (dall'alto in basso). */
  countryStacks = $state<CountryCards[]>([]);
  /** Germany 2017: passeggeri ancora sulle città. */
  passengers = $state<CityPassengers[]>([]);
  /** Binari liberi che nessuno può più reclamare (gruppo parallelo già preso con pochi giocatori). */
  blockedRouteIds = $state<string[]>([]);
  /** Biglietti ancora pescabili con DRAW_TICKETS. */
  ticketDeckSize = $state<number>(0);

  apply(raw: PublicState): void {
    const ps = normalizePublicState(raw);
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
    this.longestPathRouteIds = ps.longest_path_route_ids;
    this.recentMoves = ps.recent_moves;
    this.countryStacks = ps.country_stacks;
    this.passengers = ps.passengers;
    this.blockedRouteIds = ps.blocked_route_ids;
    this.ticketDeckSize = ps.ticket_deck_size;
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
    this.longestPathRouteIds = [];
    this.recentMoves = [];
    this.countryStacks = [];
    this.passengers = [];
    this.blockedRouteIds = [];
    this.ticketDeckSize = 0;
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