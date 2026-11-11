// WebSocket framework-agnostic: connessione al server CiufCiuf!.
//
// - Connessione a `ws(s)://<host>/ws/{gameId}?playerId=…&token=…`.
// - Auth via query string: l'identità viaggia SOLO nella connessione, mai nel payload.
// - Envelope `ServerMessage`: `type` + `payload` annidato, validato prima dell'uso.
// - Dedup dei messaggi privati con `PrivateState.seq`; il server rinumera i seq a ogni
//   sessione, quindi i contatori si azzerano a ogni nuova connessione.
// - Riconnessione: backoff esponenziale con jitter, azzerato solo dopo 10 s di connessione
//   stabile (il timer parte dal primo SNAPSHOT e gli SNAPSHOT successivi non lo rinviano);
//   `visibilitychange` e `online` richiedono uno SNAPSHOT e, se il socket resta muto per 3 s, lo
//   riaprono; un socket in CONNECTING da oltre 5 s viene sostituito.
// - Messaggi con forma non valida: scartati, con al più un REQUEST_SNAPSHOT ogni 5 s.
// - Errori fatali (partita inesistente, non autorizzato, protocollo diverso, troppi spettatori):
//   niente riconnessione.

import { parseWithBigInt, stringifyWithBigInt } from './json';
import { isRecord, isPrivateState, isPublicState, isSnapshot } from './guards';
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
export type FatalReason = 'GAME_NOT_FOUND' | 'UNAUTHORIZED' | 'PROTOCOL_VERSION' | 'TOO_MANY_SPECTATORS';

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
/** Una connessione con SNAPSHOT aperta per tanto tempo azzera il backoff. */
const STABLE_MS = 10_000;
/** Attesa di una risposta a REQUEST_SNAPSHOT prima di riaprire il socket. */
const WATCHDOG_MS = 3_000;
/** Intervallo minimo tra due REQUEST_SNAPSHOT causati da messaggi malformati. */
const BAD_MESSAGE_SNAPSHOT_INTERVAL_MS = 5_000;

/** Un socket ancora in `CONNECTING` dopo tanto tempo viene sostituito al ritorno in primo piano. */
const CONNECTING_STUCK_MS = 5_000;

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
  let stableTimer: ReturnType<typeof setTimeout> | null = null;
  let watchdogTimer: ReturnType<typeof setTimeout> | null = null;
  let lastBadMessageRequest = -Infinity;
  let connectStartedAt = 0;

  function clearTimer(timer: ReturnType<typeof setTimeout> | null): null {
    if (timer) clearTimeout(timer);
    return null;
  }

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
        if (!isSnapshot(snap)) {
          dropMalformed('SNAPSHOT');
          return;
        }
        // Il server risponde: da qui in poi gli errori non sono più fatali; il backoff riparte
        // solo se la connessione resta stabile.
        snapshotSeen = true;
        watchdogTimer = clearTimer(watchdogTimer);
        stableTimer ??= setTimeout(() => {
          stableTimer = null;
          backoffMs = INITIAL_BACKOFF_MS;
        }, STABLE_MS);
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
        if (!isPublicState(ps)) {
          dropMalformed('PUBLIC_STATE');
          return;
        }
        callbacks.onPublicState(ps);
        break;
      }
      case 'PRIVATE_STATE': {
        const ps = parsed.payload;
        if (!isPrivateState(ps)) {
          dropMalformed('PRIVATE_STATE');
          return;
        }
        watchdogTimer = clearTimer(watchdogTimer);
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
        if (!snapshotSeen && ep.reason === 'GAME_FULL' && ep.detail === 'TOO_MANY_SPECTATORS') {
          callbacks.onError(ep);
          fail('TOO_MANY_SPECTATORS');
          return;
        }
        callbacks.onError(ep);
        break;
      }
    }
  }

  /** Scarta un messaggio con forma non valida e, con throttle, chiede uno SNAPSHOT fresco. */
  function dropMalformed(kind: string): void {
    console.warn(`[ws] ${kind} malformato`);
    const now = Date.now();
    if (now - lastBadMessageRequest < BAD_MESSAGE_SNAPSHOT_INTERVAL_MS) return;
    lastBadMessageRequest = now;
    requestSnapshot();
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
      stableTimer = clearTimer(stableTimer);
      watchdogTimer = clearTimer(watchdogTimer);
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
      connectStartedAt = Date.now();
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

  // ── Ritorno in primo piano / rete ────────────────────────────

  /** Abbandona il socket corrente (anche muto) e ne apre uno nuovo senza attendere il backoff. */
  function forceReconnect(): void {
    reconnectTimer = clearTimer(reconnectTimer);
    stableTimer = clearTimer(stableTimer);
    watchdogTimer = clearTimer(watchdogTimer);
    detachSocket?.();
    detachSocket = null;
    const old = ws;
    ws = null;
    if (_connected) {
      _connected = false;
      callbacks.onConnectionChange(false);
    }
    old?.close();
    backoffMs = INITIAL_BACKOFF_MS;
    connect();
  }

  /**
   * Verifica la connessione: socket chiuso o bloccato in `CONNECTING` da oltre 5 s → riapri;
   * socket aperto → SNAPSHOT con watchdog.
   */
  function refresh(): void {
    if (closed) return;
    if (!ws || ws.readyState > READY_STATE_OPEN) {
      forceReconnect();
      return;
    }
    if (
      ws.readyState === READY_STATE_CONNECTING &&
      Date.now() - connectStartedAt > CONNECTING_STUCK_MS
    ) {
      forceReconnect();
      return;
    }
    // Anche con il WS aperto lo stato potrebbe essere stantio: se non risponde, riapri.
    if (ws.readyState === READY_STATE_OPEN) {
      requestSnapshot();
      watchdogTimer = clearTimer(watchdogTimer);
      watchdogTimer = setTimeout(() => {
        watchdogTimer = null;
        forceReconnect();
      }, WATCHDOG_MS);
    }
  }

  function onVisibilityChange(): void {
    if (document.hidden) return;
    refresh();
  }

  function onOnline(): void {
    refresh();
  }

  // ── Chiusura ─────────────────────────────────────────────────

  /** Ferma timer, listener e socket; dopo la chiamata non avviene più alcuna riconnessione. */
  function teardown(): void {
    closed = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    stableTimer = clearTimer(stableTimer);
    watchdogTimer = clearTimer(watchdogTimer);
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', onVisibilityChange);
    }
    if (typeof window !== 'undefined') {
      window.removeEventListener('online', onOnline);
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
    const action: Record<string, unknown> = { actionType: 'REQUEST_SNAPSHOT' };
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
  if (typeof window !== 'undefined') {
    window.addEventListener('online', onOnline);
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
