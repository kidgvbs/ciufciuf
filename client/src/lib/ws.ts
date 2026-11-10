// WebSocket framework-agnostic: connessione al server CiufCiuf!.
//
// - Connessione a `ws(s)://<host>/ws/{gameId}?playerId=…&token=…` (E9).
// - Auth via query string: l'identità viaggia SOLO nella connessione, mai nel payload.
// - Gestione envelope `ServerMessage` (E1): `type` + `payload` annidato.
// - Dedup idempotente lato client per `SNAPSHOT`/risposte usando `PrivateState.seq` (E6).
// - Reconnect primario: `visibilitychange` → riapertura WS + `REQUEST_SNAPSHOT` (D17).
// - Backoff esponenziale se il server è giù.

import { parseWithBigInt, stringifyWithBigInt } from './json';
import type {
  InboundAction,
  ServerMessage,
  Snapshot,
  PublicState,
  PrivateState,
  ErrorPayload,
} from './proto';

// ── Tipi ─────────────────────────────────────────────────────────────

/** Callback per i messaggi ricevuti dal server. */
export interface GameCallbacks {
  onSnapshot(snapshot: Snapshot, isReconnect: boolean): void;
  onPublicState(state: PublicState): void;
  onPrivateState(state: PrivateState): void;
  onError(error: ErrorPayload): void;
  onConnectionChange(connected: boolean): void;
}

/** Configurazione della connessione. */
export interface GameConnectionConfig {
  host: string;
  gameId: string;
  playerId?: string;
  token?: string;
  callbacks: GameCallbacks;
}

/** Connessione attiva a una partita. */
export interface GameConnection {
  /** Invia un'azione inbound (client → server). `false` se il socket è chiuso. */
  send(action: InboundAction): boolean;
  /** Richiedi uno SNAPSHOT esplicito (es. dopo reconnect). */
  requestSnapshot(clientSeq?: bigint): void;
  /** Chiudi la connessione e rimuovi i listener. */
  close(): void;
  /** `true` se il WebSocket è aperto. */
  readonly connected: boolean;
}

// ── Backoff ──────────────────────────────────────────────────────────

const INITIAL_BACKOFF_MS = 500;
const MAX_BACKOFF_MS = 30_000;
const BACKOFF_MULTIPLIER = 2;

// ── Implementazione ──────────────────────────────────────────────────

export function createGameConnection(config: GameConnectionConfig): GameConnection {
  const { host, gameId, playerId, token, callbacks } = config;

  let ws: WebSocket | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let backoffMs = INITIAL_BACKOFF_MS;
  let _connected = false;
  let closed = false;
  let lastPrivateSeq = 0n;
  let lastSnapshotSeq = 0n;
  let isReconnecting = false; // N5: traccia se la connessione corrente è un reconnect

  // ── Costruzione URL ──────────────────────────────────────────

  function buildWsUrl(): string {
    const protocol = host.startsWith('https') ? 'wss' : 'ws';
    const cleanHost = host.replace(/^https?:\/\//, '');
    let url = `${protocol}://${cleanHost}/ws/${gameId}`;
    const params = new URLSearchParams();
    if (playerId) params.set('playerId', playerId);
    if (token) params.set('token', token);
    const qs = params.toString();
    if (qs) url += '?' + qs;
    return url;
  }

  // ── Gestione messaggi ────────────────────────────────────────

  function handleMessage(event: MessageEvent): void {
    const raw = event.data;
    if (typeof raw !== 'string') return;

    let parsed: ServerMessage;
    try {
      parsed = parseWithBigInt(raw) as ServerMessage;
    } catch (e) {
      console.error('[ws] Parse error:', e, 'raw:', raw.slice(0, 200));
      return;
    }

    switch (parsed.type) {
      case 'SNAPSHOT': {
        const snap = parsed.payload as Snapshot;
        // Dedup: scarta snapshot già visti
        if (snap.private) {
          if (snap.private.seq <= lastSnapshotSeq) return;
          lastSnapshotSeq = snap.private.seq;
          if (snap.private.seq > lastPrivateSeq) {
            lastPrivateSeq = snap.private.seq;
          }
        }
        // Reset backoff: il server risponde
        backoffMs = INITIAL_BACKOFF_MS;
        callbacks.onSnapshot(snap, isReconnecting);
        isReconnecting = false;
        break;
      }
      case 'PUBLIC_STATE': {
        callbacks.onPublicState(parsed.payload as PublicState);
        break;
      }
      case 'PRIVATE_STATE': {
        const ps = parsed.payload as PrivateState;
        // Dedup E6: scarta messaggi con seq già visto
        if (ps.seq <= lastPrivateSeq) return;
        lastPrivateSeq = ps.seq;
        callbacks.onPrivateState(ps);
        break;
      }
      case 'ERROR': {
        const ep = parsed.payload as ErrorPayload;
        callbacks.onError(ep);
        break;
      }
    }
  }

  // ── Eventi socket ────────────────────────────────────────────

  function onOpen(): void {
    _connected = true;
    backoffMs = INITIAL_BACKOFF_MS;
    callbacks.onConnectionChange(true);
  }

  function onClose(event: CloseEvent): void {
    // Ignora la chiusura di un socket che non è più quello corrente.
    if (event.target !== ws) return;
    _connected = false;
    callbacks.onConnectionChange(false);
    ws = null;

    if (!closed) {
      scheduleReconnect();
    }
  }

  function onError(_event: Event): void {
    // Il browser chiuderà il socket dopo l'errore → onClose gestisce il reconnect.
    console.warn('[ws] WebSocket error');
  }

  // ── Connessione / riconnessione ──────────────────────────────

  function connect(): void {
    if (closed) return;
    if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    const url = buildWsUrl();
    try {
      ws = new WebSocket(url);
      ws.addEventListener('open', onOpen);
      ws.addEventListener('message', handleMessage);
      ws.addEventListener('close', onClose);
      ws.addEventListener('error', onError);
    } catch (e) {
      console.error('[ws] Connection failed:', e);
      scheduleReconnect();
    }
  }

  function scheduleReconnect(): void {
    if (closed || reconnectTimer) return;

    // N5: la prossima connessione è un reconnect (chiusura inattesa).
    isReconnecting = true;
    console.log(`[ws] Reconnect in ${backoffMs}ms`);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, backoffMs);

    backoffMs = Math.min(backoffMs * BACKOFF_MULTIPLIER, MAX_BACKOFF_MS);
  }

  // ── Visibility change (D17) ──────────────────────────────────

  function onVisibilityChange(): void {
    if (document.hidden) return;

    // Pagina tornata visibile: se WS chiuso, riapri e chiedi SNAPSHOT
    if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
      // Annulla timer di backoff pendente
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      backoffMs = INITIAL_BACKOFF_MS;
      isReconnecting = true; // N5: questo è un reconnect
      connect();
    }

    // Invia REQUEST_SNAPSHOT per riallinearsi
    // (anche se il WS era ancora aperto: lo stato potrebbe essere stantio)
    if (ws && ws.readyState === WebSocket.OPEN) {
      requestSnapshot();
    }
  }

  // ── API pubblica ─────────────────────────────────────────────

  function send(action: InboundAction): boolean {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      console.warn('[ws] Cannot send: not connected');
      return false;
    }
    ws.send(stringifyWithBigInt(action));
    return true;
  }

  function requestSnapshot(clientSeq?: bigint): void {
    // Costruisci manualmente l'azione REQUEST_SNAPSHOT.
    // N1: ometti `player_id` quando non c'è identità (connessione spettatore).
    const payload: Record<string, unknown> = {
      client_seq: clientSeq ?? null,
    };
    if (playerId) {
      payload['player_id'] = playerId;
    }
    const action = {
      actionType: 'REQUEST_SNAPSHOT',
      ...payload,
    };
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(stringifyWithBigInt(action));
    }
  }

  function close(): void {
    closed = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    document.removeEventListener('visibilitychange', onVisibilityChange);
    if (ws) {
      ws.removeEventListener('open', onOpen);
      ws.removeEventListener('message', handleMessage);
      ws.removeEventListener('close', onClose);
      ws.removeEventListener('error', onError);
      ws.close();
      ws = null;
    }
    _connected = false;
    callbacks.onConnectionChange(false);
  }

  // ── Avvio ────────────────────────────────────────────────────

  document.addEventListener('visibilitychange', onVisibilityChange);
  connect();

  return {
    send,
    requestSnapshot: (cs?: bigint) => requestSnapshot(cs),
    close,
    get connected() {
      return _connected;
    },
  };
}