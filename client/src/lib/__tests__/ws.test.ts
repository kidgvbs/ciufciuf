import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createGameConnection,
  isServerMessage,
  type FatalReason,
  type GameCallbacks,
  type SocketLike,
} from '../ws';
import { resolveServerHost, isAllowedHost } from '../server-host';
import { saveIdentity, loadIdentity, clearIdentity } from '../lobby';

// ── Socket finto ─────────────────────────────────────────────────────

type Listener = (event: Event) => void;

class FakeSocket implements SocketLike {
  static instances: FakeSocket[] = [];

  readyState = 0;
  sent: string[] = [];
  closeCalls = 0;
  private listeners = new Map<string, Set<Listener>>();

  constructor(readonly url: string) {
    FakeSocket.instances.push(this);
  }

  addEventListener(type: string, listener: Listener): void {
    const set = this.listeners.get(type) ?? new Set<Listener>();
    set.add(listener);
    this.listeners.set(type, set);
  }

  removeEventListener(type: string, listener: Listener): void {
    this.listeners.get(type)?.delete(listener);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closeCalls++;
    this.readyState = 3;
  }

  private emit(type: string, event: object): void {
    for (const l of [...(this.listeners.get(type) ?? [])]) l(event as Event);
  }

  open(): void {
    this.readyState = 1;
    this.emit('open', {});
  }

  /** Il server chiude la connessione. */
  drop(): void {
    this.readyState = 3;
    this.emit('close', {});
  }

  receive(message: unknown): void {
    this.emit('message', { data: JSON.stringify(message) });
  }
}

function publicState(version = 1) {
  return { game_id: 'g1', protocol_version: version, state: 'PLAYER_TURN' };
}

function privateState(seq: number) {
  return { seq, hand: [], tickets: [], offered_tickets: [], legal_moves: null };
}

function snapshot(seq: number | null, version = 1) {
  return {
    type: 'SNAPSHOT',
    payload: {
      protocol_version: version,
      public: publicState(version),
      private: seq === null ? null : privateState(seq),
    },
  };
}

function errorMessage(reason: string) {
  return { type: 'ERROR', payload: { seq: 1, reason, action_ref: null, detail: null } };
}

function makeCallbacks() {
  return {
    onSnapshot: vi.fn(),
    onPublicState: vi.fn(),
    onPrivateState: vi.fn(),
    onError: vi.fn(),
    onConnectionChange: vi.fn(),
    onFatal: vi.fn(),
  } satisfies GameCallbacks;
}

function connect(overrides: { random?: () => number; playerId?: string; gameId?: string } = {}) {
  const callbacks = makeCallbacks();
  const connection = createGameConnection({
    host: 'http://tv.local:8080',
    gameId: overrides.gameId ?? 'g1',
    playerId: overrides.playerId ?? 'p1',
    token: 't',
    callbacks,
    createSocket: (url) => new FakeSocket(url),
    random: overrides.random ?? (() => 0.5),
  });
  return { callbacks, connection };
}

function lastSocket(): FakeSocket {
  const s = FakeSocket.instances.at(-1);
  if (!s) throw new Error('nessun socket creato');
  return s;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, 'log').mockImplementation(() => {});
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  FakeSocket.instances = [];
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

// ── Connessione ──────────────────────────────────────────────────────

describe('createGameConnection', () => {
  it('codifica il gameId nell URL', () => {
    connect({ gameId: 'a/b c' });
    expect(lastSocket().url).toBe('ws://tv.local:8080/ws/a%2Fb%20c?playerId=p1&token=t');
  });

  it('fatal_error_stops_reconnect: GAME_NOT_FOUND prima dello snapshot è fatale', () => {
    const { callbacks } = connect();
    const sock = lastSocket();
    sock.open();
    sock.receive(errorMessage('GAME_NOT_FOUND'));

    expect(callbacks.onFatal).toHaveBeenCalledExactlyOnceWith('GAME_NOT_FOUND' satisfies FatalReason);
    expect(callbacks.onError).not.toHaveBeenCalled();
    expect(sock.closeCalls).toBe(1);

    // Il server chiude il socket: nessun tentativo di riconnessione, mai.
    sock.drop();
    vi.advanceTimersByTime(120_000);
    expect(FakeSocket.instances).toHaveLength(1);
    expect(callbacks.onFatal).toHaveBeenCalledTimes(1);
  });

  it('fatal_error_stops_reconnect: UNAUTHORIZED è fatale, un errore dopo lo snapshot no', () => {
    const first = connect();
    lastSocket().receive(errorMessage('UNAUTHORIZED'));
    expect(first.callbacks.onFatal).toHaveBeenCalledWith('UNAUTHORIZED');

    const second = connect();
    const sock = lastSocket();
    sock.open();
    sock.receive(snapshot(1));
    sock.receive(errorMessage('GAME_NOT_FOUND'));
    expect(second.callbacks.onFatal).not.toHaveBeenCalled();
    expect(second.callbacks.onError).toHaveBeenCalledTimes(1);
  });

  it('versione di protocollo diversa è fatale', () => {
    const { callbacks } = connect();
    lastSocket().receive(snapshot(1, 2));
    expect(callbacks.onFatal).toHaveBeenCalledWith('PROTOCOL_VERSION');
    expect(callbacks.onSnapshot).not.toHaveBeenCalled();
    vi.advanceTimersByTime(120_000);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it('ignora messaggi malformati senza propagarli', () => {
    const { callbacks } = connect();
    const sock = lastSocket();
    sock.receive({ type: 'BOGUS', payload: {} });
    sock.receive({ type: 'SNAPSHOT' });
    sock.receive('testo');
    sock.receive({ type: 'PRIVATE_STATE', payload: { hand: [] } });
    expect(callbacks.onSnapshot).not.toHaveBeenCalled();
    expect(callbacks.onPrivateState).not.toHaveBeenCalled();
    expect(callbacks.onFatal).not.toHaveBeenCalled();
    expect(isServerMessage({ type: 'ERROR', payload: {} })).toBe(true);
    expect(isServerMessage({ type: 'ERROR' })).toBe(false);
  });

  it('backoff_grows_without_snapshot: il ritardo cresce finché non arriva uno snapshot', () => {
    connect();
    const delays = [500, 1000, 2000, 4000, 8000, 16_000, 30_000, 30_000];
    delays.forEach((delay, i) => {
      const sock = lastSocket();
      expect(FakeSocket.instances).toHaveLength(i + 1);
      sock.open(); // l'apertura da sola non azzera il backoff
      sock.drop();
      vi.advanceTimersByTime(delay - 1);
      expect(FakeSocket.instances).toHaveLength(i + 1);
      vi.advanceTimersByTime(1);
      expect(FakeSocket.instances).toHaveLength(i + 2);
    });
  });

  it('backoff_grows_without_snapshot: il primo snapshot azzera il backoff', () => {
    connect();
    for (let i = 0; i < 3; i++) {
      lastSocket().open();
      lastSocket().drop();
      vi.advanceTimersByTime(30_000);
    }
    const sock = lastSocket();
    sock.open();
    sock.receive(snapshot(1));
    sock.drop();
    const count = FakeSocket.instances.length;
    vi.advanceTimersByTime(499);
    expect(FakeSocket.instances).toHaveLength(count);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(count + 1);
  });

  it('applica un jitter di ±20% al ritardo', () => {
    connect({ random: () => 0 });
    lastSocket().drop();
    vi.advanceTimersByTime(399);
    expect(FakeSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(2);

    lastSocket().drop();
    const before = FakeSocket.instances.length;
    // Secondo ritardo: base 1000 ms, minimo 800 ms.
    vi.advanceTimersByTime(799);
    expect(FakeSocket.instances).toHaveLength(before);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(before + 1);

    FakeSocket.instances = [];
    connect({ random: () => 0.999999 });
    lastSocket().drop();
    vi.advanceTimersByTime(599);
    expect(FakeSocket.instances).toHaveLength(1);
    vi.advanceTimersByTime(2);
    expect(FakeSocket.instances).toHaveLength(2);
  });

  it('seq_counters_reset_on_new_connection: il server rinumera i seq a ogni sessione', () => {
    const { callbacks } = connect();
    const first = lastSocket();
    first.open();
    first.receive(snapshot(5));
    first.receive({ type: 'PRIVATE_STATE', payload: privateState(9) });
    expect(callbacks.onSnapshot).toHaveBeenCalledTimes(1);
    expect(callbacks.onPrivateState).toHaveBeenCalledTimes(1);

    // Stessa connessione: un seq già visto viene scartato.
    first.receive({ type: 'PRIVATE_STATE', payload: privateState(9) });
    expect(callbacks.onPrivateState).toHaveBeenCalledTimes(1);

    // Server riavviato: nuova connessione, seq ripartono da 1.
    first.drop();
    vi.advanceTimersByTime(500);
    const second = lastSocket();
    expect(second).not.toBe(first);
    second.open();
    second.receive(snapshot(1));
    second.receive({ type: 'PRIVATE_STATE', payload: privateState(2) });
    expect(callbacks.onSnapshot).toHaveBeenCalledTimes(2);
    expect(callbacks.onPrivateState).toHaveBeenCalledTimes(2);
  });

  it('close() ferma tutto e non riconnette', () => {
    const { connection, callbacks } = connect();
    const sock = lastSocket();
    sock.open();
    connection.close();
    sock.drop();
    vi.advanceTimersByTime(120_000);
    expect(FakeSocket.instances).toHaveLength(1);
    expect(connection.connected).toBe(false);
    expect(callbacks.onFatal).not.toHaveBeenCalled();
  });

  it('requestSnapshot omette player_id per gli spettatori', () => {
    const callbacks = makeCallbacks();
    const connection = createGameConnection({
      host: 'http://tv.local:8080',
      gameId: 'g1',
      callbacks,
      createSocket: (url) => new FakeSocket(url),
    });
    const sock = lastSocket();
    sock.open();
    connection.requestSnapshot();
    expect(JSON.parse(sock.sent[0] ?? '{}')).toEqual({
      actionType: 'REQUEST_SNAPSHOT',
      client_seq: null,
    });
  });
});

// ── Host del server ──────────────────────────────────────────────────

describe('server host', () => {
  const env = { origin: 'http://tv.local:8080', dev: false };

  it('server_param_ignored_when_not_same_origin', () => {
    expect(resolveServerHost('?server=http://evil.example:9', env)).toBe(env.origin);
    expect(resolveServerHost('', env)).toBe(env.origin);
    expect(resolveServerHost('?server=http://tv.local:8080/', env)).toBe(env.origin);
  });

  it('?server= è accettato in dev', () => {
    expect(resolveServerHost('?server=http://192.168.1.9:8080', { ...env, dev: true })).toBe(
      'http://192.168.1.9:8080',
    );
  });

  it('server_param_ignored_when_not_same_origin: identità con host diverso ignorata', () => {
    const storage = new Map<string, string>();
    (globalThis as Record<string, unknown>).localStorage = {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => void storage.set(k, v),
      removeItem: (k: string) => void storage.delete(k),
    };
    saveIdentity({ playerId: 'p', token: 't', gameId: 'g', host: 'http://evil.example' });
    expect(loadIdentity('g', env)).toBeNull();
    expect(loadIdentity('g', { ...env, dev: true })).not.toBeNull();
    expect(isAllowedHost(env.origin, env)).toBe(true);
    expect(isAllowedHost('', env)).toBe(false);
  });
});

// ── Identità per partita ─────────────────────────────────────────────

describe('identità', () => {
  const env = { origin: 'http://tv.local:8080', dev: false };
  let storage: Map<string, string>;

  beforeEach(() => {
    storage = new Map();
    (globalThis as Record<string, unknown>).localStorage = {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => void storage.set(k, v),
      removeItem: (k: string) => void storage.delete(k),
    };
  });

  const identity = (gameId: string) => ({
    playerId: `p-${gameId}`,
    token: `t-${gameId}`,
    gameId,
    host: env.origin,
  });

  it('identity_is_per_game', () => {
    saveIdentity(identity('A'));
    saveIdentity(identity('B'));

    expect(loadIdentity('A', env)).toEqual(identity('A'));
    expect(loadIdentity('B', env)).toEqual(identity('B'));
    // Senza gameId: l'ultima partita usata.
    expect(loadIdentity(undefined, env)).toEqual(identity('B'));

    clearIdentity('A');
    expect(loadIdentity('A', env)).toBeNull();
    expect(loadIdentity('B', env)).toEqual(identity('B'));
    expect(storage.has('ciufciuf-identity:B')).toBe(true);

    clearIdentity();
    expect(loadIdentity(undefined, env)).toBeNull();
  });

  it('identity_is_per_game: JSON corrotto o incompleto viene ignorato', () => {
    storage.set('ciufciuf-identity:X', '{non-json');
    expect(loadIdentity('X', env)).toBeNull();
    storage.set('ciufciuf-identity:Y', JSON.stringify({ playerId: 'p', gameId: 'Y', host: env.origin }));
    expect(loadIdentity('Y', env)).toBeNull();
  });

  it('identity_is_per_game: localStorage che lancia non rompe nulla', () => {
    (globalThis as Record<string, unknown>).localStorage = {
      getItem: () => { throw new Error('SecurityError'); },
      setItem: () => { throw new Error('QuotaExceededError'); },
      removeItem: () => { throw new Error('SecurityError'); },
    };
    expect(() => saveIdentity(identity('A'))).not.toThrow();
    expect(loadIdentity('A', env)).toBeNull();
    expect(() => clearIdentity('A')).not.toThrow();
  });
});
