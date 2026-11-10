// WebSocket framework-agnostic: connessione al server CiufCiuf!.
//
// - Connessione a `ws(s)://<host>/ws/{gameId}?playerId=…&token=…`.
// - Auth via query string: l'identità viaggia SOLO nella connessione, mai nel payload.
// - Envelope `ServerMessage`: `type` + `payload` annidato, validato prima dell'uso.
// - Dedup dei messaggi privati con `PrivateState.seq`; il server rinumera i seq a ogni
//   sessione, quindi i contatori si azzerano a ogni nuova connessione.
// - Riconnessione: backoff esponenziale con jitter, azzerato al primo SNAPSHOT;
//   `visibilitychange` riapre il socket e richiede uno SNAPSHOT.
// - Errori fatali (partita inesistente, non autorizzato, protocollo diverso): niente riconnessione.

import { parseWithBigInt, stringifyWithBigInt } from './json';
import { isRecord } from './guards';
import type {
  InboundAction,
  ServerMessage,
  Snapshot,
  PublicState,
  PrivateState,
  ErrorPayload,
} from './proto';

// ── Tipi ─────────────────────────────────────────────────────────────

/** Motivo per cui la connessione è stata abbandonata senza riprovare. */
export type FatalReason = 'GAME_NOT_FOUND' | 'UNAUTHORIZED' | 'PROTOCOL_VERSION';

/** Callback per i messaggi ricevuti dal server. */
export interface GameCallbacks {
  onSnapshot(snapshot: Snapshot): void;
  onPublicState(state: PublicState): void;
  onPrivateState(state: PrivateState): void;
  onError(error: ErrorPayload): void;
  onConnectionChange(connected: boolean): void;
  /** Connessione chiusa definitivamente: nessun tentativo di riconnessione. */
  onFatal(reason: FatalReason): void;
}

/** Sottoinsieme di `WebSocket` usato dal client (permette un socket finto nei test). */
export interface SocketLike {
  readonly readyState: number;
  send(data: string): void;
  close(): void;
  addEventListener(type: string, listener: (event: Event) => void): void;
  removeEventListener(type: string, listener: (event: Event) => void): void;
}

/** Configurazione della connessione. */
export interface GameConnectionConfig {
  host: string;
  gameId: string;
  playerId?: string;
  token?: string;
  callbacks: GameCallbacks;
  /** Crea il socket per un URL; default `new WebSocket(url)`. */
  createSocket?: (url: string) => SocketLike;
  /** Sorgente di casualità in [0, 1) per il jitter del backoff; default `Math.random`. */
  random?: () => number;
}

/** Connessione attiva a una partita. */
export interface GameConnection {
  /** Invia un'azione inbound (client → server). `false` se il socket è chiuso. */
  send(action: InboundAction): boolean;
  /** Richiedi uno SNAPSHOT esplicito (es. dopo reconnect). */
  requestSnapshot(): void;
  /** Chiudi la connessione e rimuovi i listener. */
  close(): void;
  /** `true` se il WebSocket è aperto. */
  readonly connected: boolean;
}

// ── Costanti ─────────────────────────────────────────────────────────

const PROTOCOL_VERSION = 1;

const INITIAL_BACKOFF_MS = 500;
const MAX_BACKOFF_MS = 30_000;
const BACKOFF_MULTIPLIER = 2;
/** Il ritardo effettivo varia di ±20% attorno al valore del backoff. */
const BACKOFF_JITTER = 0.2;

const READY_STATE_CONNECTING = 0;
const READY_STATE_OPEN = 1;

const MESSAGE_TYPES: Record<string, true> = {
  PUBLIC_STATE: true,
  PRIVATE_STATE: true,
  ERROR: true,
  SNAPSHOT: true,
};

/** Errori che, prima del primo SNAPSHOT, rendono inutile riprovare. */
const FATAL_ERROR_REASONS: Record<string, true> = {
  GAME_NOT_FOUND: true,
  UNAUTHORIZED: true,
};

/** `true` se `value` ha la forma dell'envelope `ServerMessage` (tipo noto + payload oggetto). */
export function isServerMessage(value: unknown): value is ServerMessage {
  return (
    isRecord(value) &&
    typeof value.type === 'string' &&
    Object.hasOwn(MESSAGE_TYPES, value.type) &&
    isRecord(value.payload)
  );
}

// ── Implementazione ──────────────────────────────────────────────────

export function createGameConnection(config: GameConnectionConfig): GameConnection {
  const { host, gameId, playerId, token, callbacks } = config;
  const createSocket = config.createSocket ?? ((url: string) => new WebSocket(url));
  const random = config.random ?? Math.random;

  let ws: SocketLike | null = null;
  let detachSocket: (() => void) | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let backoffMs = INITIAL_BACKOFF_MS;
  let _connected = false;
  let closed = false;
  let snapshotSeen = false;
  let lastPrivateSeq = 0n;
  let lastSnapshotSeq = 0n;

  // ── Costruzione URL ──────────────────────────────────────────

  function buildWsUrl(): string {
    const protocol = host.startsWith('https') ? 'wss' : 'ws';
    const cleanHost = host.replace(/^https?:\/\//, '');
    let url = `${protocol}://${cleanHost}/ws/${encodeURIComponent(gameId)}`;
    const params = new URLSearchParams();
    if (playerId) params.set('playerId', playerId);
    if (token) params.set('token', token);
    const qs = params.toString();
    if (qs) url += '?' + qs;
    return url;
  }

  // ── Gestione messaggi ────────────────────────────────────────

  function handleMessage(raw: unknown): void {
    if (typeof raw !== 'string') return;

    let parsed: unknown;
    try {
      parsed = parseWithBigInt(raw);
    } catch (e) {
      console.error('[ws] Parse error:', e, 'raw:', raw.slice(0, 200));
      return;
    }
    if (!isServerMessage(parsed)) {
      console.warn('[ws] Messaggio non riconosciuto:', raw.slice(0, 200));
      return;
    }

    switch (parsed.type) {
      case 'SNAPSHOT': {
        const snap = parsed.payload;
        if (snap.protocol_version !== PROTOCOL_VERSION) {
          fail('PROTOCOL_VERSION');
          return;
        }
        if (!isRecord(snap.public) || (snap.private !== null && !hasSeq(snap.private))) {
          console.warn('[ws] SNAPSHOT malformato');
          return;
        }
        // Il server risponde: da qui in poi gli errori non sono più fatali e il backoff riparte.
        snapshotSeen = true;
        backoffMs = INITIAL_BACKOFF_MS;
        if (snap.private) {
          if (snap.private.seq <= lastSnapshotSeq) return;
          lastSnapshotSeq = snap.private.seq;
          if (snap.private.seq > lastPrivateSeq) {
            lastPrivateSeq = snap.private.seq;
          }
        }
        callbacks.onSnapshot(snap);
        break;
      }
      case 'PUBLIC_STATE': {
        const ps = parsed.payload;
        if (ps.protocol_version !== PROTOCOL_VERSION) {
          fail('PROTOCOL_VERSION');
          return;
        }
        callbacks.onPublicState(ps);
        break;
      }
      case 'PRIVATE_STATE': {
        const ps = parsed.payload;
        if (!hasSeq(ps)) {
          console.warn('[ws] PRIVATE_STATE malformato');
          return;
        }
        if (ps.seq <= lastPrivateSeq) return;
        lastPrivateSeq = ps.seq;
        callbacks.onPrivateState(ps);
        break;
      }
      case 'ERROR': {
        const ep = parsed.payload;
        if (typeof ep.reason !== 'string') {
          console.warn('[ws] ERROR malformato');
          return;
        }
        if (!snapshotSeen && Object.hasOwn(FATAL_ERROR_REASONS, ep.reason)) {
          fail(ep.reason as FatalReason);
          return;
        }
        callbacks.onError(ep);
        break;
      }
    }
  }

  function hasSeq(state: { seq: bigint }): boolean {
    return typeof state.seq === 'bigint';
  }

  // ── Eventi socket ────────────────────────────────────────────

  /** Registra i listener del socket; restituisce la funzione che li rimuove. */
  function attach(sock: SocketLike): () => void {
    const onOpen = () => {
      if (sock !== ws) return;
      _connected = true;
      callbacks.onConnectionChange(true);
    };
    const onMessage = (event: Event) => {
      if (sock !== ws) return;
      handleMessage((event as MessageEvent).data);
    };
    const onClose = () => {
      if (sock !== ws) return;
      detachSocket?.();
      detachSocket = null;
      ws = null;
      _connected = false;
      callbacks.onConnectionChange(false);
      if (!closed) scheduleReconnect();
    };
    const onError = () => {
      // Il browser chiude il socket dopo l'errore: onClose gestisce la riconnessione.
      console.warn('[ws] WebSocket error');
    };
    sock.addEventListener('open', onOpen);
    sock.addEventListener('message', onMessage);
    sock.addEventListener('close', onClose);
    sock.addEventListener('error', onError);
    return () => {
      sock.removeEventListener('open', onOpen);
      sock.removeEventListener('message', onMessage);
      sock.removeEventListener('close', onClose);
      sock.removeEventListener('error', onError);
    };
  }

  // ── Connessione / riconnessione ──────────────────────────────

  function connect(): void {
    if (closed) return;
    if (ws && (ws.readyState === READY_STATE_OPEN || ws.readyState === READY_STATE_CONNECTING)) {
      return;
    }

    lastPrivateSeq = 0n;
    lastSnapshotSeq = 0n;
    snapshotSeen = false;

    try {
      const sock = createSocket(buildWsUrl());
      ws = sock;
      detachSocket = attach(sock);
    } catch (e) {
      console.error('[ws] Connection failed:', e);
      ws = null;
      scheduleReconnect();
    }
  }

  function scheduleReconnect(): void {
    if (closed || reconnectTimer) return;

    const jitter = 1 + (random() * 2 - 1) * BACKOFF_JITTER;
    const delay = Math.round(backoffMs * jitter);
    console.log(`[ws] Reconnect in ${delay}ms`);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delay);

    backoffMs = Math.min(backoffMs * BACKOFF_MULTIPLIER, MAX_BACKOFF_MS);
  }

  // ── Visibility change ────────────────────────────────────────

  function onVisibilityChange(): void {
    if (document.hidden) return;

    // Pagina tornata visibile: se WS chiuso, riapri e chiedi SNAPSHOT
    if (!ws || ws.readyState > READY_STATE_OPEN) {
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      backoffMs = INITIAL_BACKOFF_MS;
      connect();
    }

    // Anche con il WS ancora aperto lo stato potrebbe essere stantio.
    if (ws && ws.readyState === READY_STATE_OPEN) {
      requestSnapshot();
    }
  }

  // ── Chiusura ─────────────────────────────────────────────────

  /** Ferma timer, listener e socket; dopo la chiamata non avviene più alcuna riconnessione. */
  function teardown(): void {
    closed = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', onVisibilityChange);
    }
    detachSocket?.();
    detachSocket = null;
    ws?.close();
    ws = null;
    _connected = false;
    callbacks.onConnectionChange(false);
  }

  function fail(reason: FatalReason): void {
    if (closed) return;
    teardown();
    callbacks.onFatal(reason);
  }

  // ── API pubblica ─────────────────────────────────────────────

  function send(action: InboundAction): boolean {
    if (!ws || ws.readyState !== READY_STATE_OPEN) {
      console.warn('[ws] Cannot send: not connected');
      return false;
    }
    ws.send(stringifyWithBigInt(action));
    return true;
  }

  function requestSnapshot(): void {
    // `player_id` è omesso nelle connessioni spettatore.
    const action: Record<string, unknown> = {
      actionType: 'REQUEST_SNAPSHOT',
      client_seq: null,
    };
    if (playerId) action['player_id'] = playerId;
    if (ws && ws.readyState === READY_STATE_OPEN) {
      ws.send(stringifyWithBigInt(action));
    }
  }

  function close(): void {
    if (closed) return;
    teardown();
  }

  // ── Avvio ────────────────────────────────────────────────────

  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', onVisibilityChange);
  }
  connect();

  return {
    send,
    requestSnapshot,
    close,
    get connected() {
      return _connected;
    },
  };
}
