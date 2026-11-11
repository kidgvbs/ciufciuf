import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  createGameConnection,
  isServerMessage,
  type FatalReason,
  type GameCallbacks,
  type SocketLike,
} from '../ws';
import { isAllowedHost } from '../server-host';
import {
  saveIdentity,
  loadIdentity,
  clearIdentity,
  forgetLastIdentity,
  saveLastGame,
  loadLastGame,
  clearLastGame,
  parseSeed,
} from '../lobby';

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
  return {
    game_id: 'g1',
    protocol_version: version,
    state: 'PLAYER_TURN',
    deck_size: 10,
    visible_deck: [],
    claimed_routes: [],
    players_summary: [],
  };
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

/** Documento finto: permette di simulare `visibilitychange` nei test in ambiente node. */
class FakeEventTarget {
  hidden = false;
  private listeners = new Map<string, Set<() => void>>();
  addEventListener(type: string, l: () => void): void {
    const set = this.listeners.get(type) ?? new Set<() => void>();
    set.add(l);
    this.listeners.set(type, set);
  }
  removeEventListener(type: string, l: () => void): void {
    this.listeners.get(type)?.delete(l);
  }
  dispatch(type: string): void {
    for (const l of [...(this.listeners.get(type) ?? [])]) l();
  }
  count(type: string): number {
    return this.listeners.get(type)?.size ?? 0;
  }
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

  it('repeated_close_after_snapshot_keeps_growing_backoff', () => {
    connect({ random: () => 0.5 });
    const delays: number[] = [];
    for (let i = 0; i < 6; i++) {
      const sock = lastSocket();
      sock.open();
      sock.receive(snapshot(1));
      const before = FakeSocket.instances.length;
      sock.drop();
      let waited = 0;
      while (FakeSocket.instances.length === before) {
        vi.advanceTimersByTime(1);
        waited++;
      }
      delays.push(waited);
    }
    expect(delays).toEqual([500, 1000, 2000, 4000, 8000, 16_000]);
  });

  it('stable_connection_resets_backoff', () => {
    connect();
    // Due cadute rapide: il backoff sale a 2000 ms.
    for (let i = 0; i < 2; i++) {
      lastSocket().open();
      lastSocket().receive(snapshot(1));
      lastSocket().drop();
      vi.advanceTimersByTime(30_000);
    }
    const sock = lastSocket();
    sock.open();
    sock.receive(snapshot(1));
    vi.advanceTimersByTime(10_001);
    sock.drop();
    const count = FakeSocket.instances.length;
    vi.advanceTimersByTime(499);
    expect(FakeSocket.instances).toHaveLength(count);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(count + 1);
  });

  it('spectator_limit_error_is_surfaced', () => {
    const { callbacks } = connect();
    const sock = lastSocket();
    sock.open();
    sock.receive({
      type: 'ERROR',
      payload: { seq: 1, reason: 'GAME_FULL', action_ref: null, detail: 'TOO_MANY_SPECTATORS' },
    });
    expect(callbacks.onError).toHaveBeenCalledTimes(1);
    expect(callbacks.onFatal).toHaveBeenCalledExactlyOnceWith(
      'TOO_MANY_SPECTATORS' satisfies FatalReason,
    );
    sock.drop();
    vi.advanceTimersByTime(120_000);
    expect(FakeSocket.instances).toHaveLength(1);
  });

  it('error_before_snapshot_is_surfaced_and_reconnects', () => {
    const { callbacks } = connect();
    const sock = lastSocket();
    sock.open();
    sock.receive(errorMessage('NAME_TAKEN'));
    expect(callbacks.onError).toHaveBeenCalledTimes(1);
    expect(callbacks.onFatal).not.toHaveBeenCalled();
    sock.drop();
    vi.advanceTimersByTime(500);
    expect(FakeSocket.instances).toHaveLength(2);
  });

  it('malformed_public_state_is_dropped', () => {
    const { callbacks } = connect();
    const sock = lastSocket();
    sock.open();
    sock.receive({ type: 'PUBLIC_STATE', payload: { game_id: 'g1', protocol_version: 1 } });
    expect(callbacks.onPublicState).not.toHaveBeenCalled();
    // Il messaggio scartato chiede uno SNAPSHOT, ma al più uno ogni 5 s.
    const requests = () => sock.sent.filter((m) => m.includes('REQUEST_SNAPSHOT')).length;
    expect(requests()).toBe(1);
    sock.receive({ type: 'PUBLIC_STATE', payload: { game_id: 'g1', protocol_version: 1 } });
    expect(requests()).toBe(1);
    vi.advanceTimersByTime(5_000);
    sock.receive({ type: 'PUBLIC_STATE', payload: { game_id: 'g1', protocol_version: 1 } });
    expect(requests()).toBe(2);

    sock.receive({ type: 'PUBLIC_STATE', payload: publicState() });
    expect(callbacks.onPublicState).toHaveBeenCalledTimes(1);
  });

  it('malformed_snapshot_and_private_state_are_dropped', () => {
    const { callbacks } = connect();
    const sock = lastSocket();
    sock.open();
    sock.receive({
      type: 'SNAPSHOT',
      payload: { protocol_version: 1, public: publicState(), private: { seq: 1, hand: [] } },
    });
    sock.receive({ type: 'PRIVATE_STATE', payload: { ...privateState(2), tickets: 'x' } });
    expect(callbacks.onSnapshot).not.toHaveBeenCalled();
    expect(callbacks.onPrivateState).not.toHaveBeenCalled();
  });

  it('frequent_snapshots_do_not_postpone_backoff_reset', () => {
    connect();
    // Due cadute rapide: il backoff sale a 2000 ms.
    for (let i = 0; i < 2; i++) {
      lastSocket().open();
      lastSocket().receive(snapshot(1));
      lastSocket().drop();
      vi.advanceTimersByTime(30_000);
    }
    const sock = lastSocket();
    sock.open();
    // Uno SNAPSHOT ogni 4 s (es. cambi di scheda frequenti): il timer stabile parte dal primo.
    for (let seq = 1; seq <= 4; seq++) {
      sock.receive(snapshot(seq));
      vi.advanceTimersByTime(4_000);
    }
    sock.drop();
    const count = FakeSocket.instances.length;
    vi.advanceTimersByTime(499);
    expect(FakeSocket.instances).toHaveLength(count);
    vi.advanceTimersByTime(1);
    expect(FakeSocket.instances).toHaveLength(count + 1);
  });

  describe('visibilità e rete', () => {
    let doc: FakeEventTarget;
    let win: FakeEventTarget;

    beforeEach(() => {
      doc = new FakeEventTarget();
      win = new FakeEventTarget();
      (globalThis as Record<string, unknown>).document = doc;
      (globalThis as Record<string, unknown>).window = win;
    });

    afterEach(() => {
      delete (globalThis as Record<string, unknown>).document;
      delete (globalThis as Record<string, unknown>).window;
    });

    it('visible_with_silent_open_socket_reconnects_after_timeout', () => {
      const { callbacks } = connect();
      const sock = lastSocket();
      sock.open();
      sock.receive(snapshot(1));
      doc.dispatch('visibilitychange');
      expect(sock.sent.some((m) => m.includes('REQUEST_SNAPSHOT'))).toBe(true);

      vi.advanceTimersByTime(2_999);
      expect(FakeSocket.instances).toHaveLength(1);
      vi.advanceTimersByTime(1);
      expect(sock.closeCalls).toBe(1);
      expect(FakeSocket.instances).toHaveLength(2);
      expect(callbacks.onConnectionChange).toHaveBeenLastCalledWith(false);
    });

    it('closing_socket_is_replaced_cleanly_on_visibility', () => {
      const { callbacks } = connect();
      const old = lastSocket();
      old.open();
      old.receive(snapshot(1));
      expect(callbacks.onConnectionChange).toHaveBeenLastCalledWith(true);
      old.readyState = 2;

      vi.advanceTimersByTime(1_000);
      doc.dispatch('visibilitychange');
      expect(FakeSocket.instances).toHaveLength(2);
      expect(callbacks.onConnectionChange).toHaveBeenLastCalledWith(false);

      // Il nuovo socket cade subito: backoff 500 ms, poi sale a 1000 ms.
      lastSocket().drop();
      vi.advanceTimersByTime(500);
      expect(FakeSocket.instances).toHaveLength(3);

      // Il timer di stabilità del vecchio socket (scadenza a 10 s) non deve azzerare il backoff.
      vi.advanceTimersByTime(10_000);
      expect(FakeSocket.instances).toHaveLength(3);
      lastSocket().drop();
      vi.advanceTimersByTime(999);
      expect(FakeSocket.instances).toHaveLength(3);
      vi.advanceTimersByTime(1);
      expect(FakeSocket.instances).toHaveLength(4);
    });

    it('stuck_connecting_socket_is_replaced_on_visibility', () => {
      const { callbacks } = connect();
      const stuck = lastSocket();
      expect(stuck.readyState).toBe(0);

      // Da meno di 5 s in CONNECTING: si aspetta.
      vi.advanceTimersByTime(4_999);
      doc.dispatch('visibilitychange');
      expect(stuck.closeCalls).toBe(0);
      expect(FakeSocket.instances).toHaveLength(1);

      // Oltre 5 s: il socket bloccato viene chiuso e sostituito, senza attendere il backoff.
      vi.advanceTimersByTime(2);
      doc.dispatch('visibilitychange');
      expect(stuck.closeCalls).toBe(1);
      expect(FakeSocket.instances).toHaveLength(2);
      expect(lastSocket()).not.toBe(stuck);

      // Il vecchio socket, se si chiude in ritardo, non innesca altre riconnessioni.
      stuck.drop();
      vi.advanceTimersByTime(120_000);
      expect(FakeSocket.instances).toHaveLength(2);
      expect(callbacks.onFatal).not.toHaveBeenCalled();
    });

    it('visible socket that answers is kept', () => {
      connect();
      const sock = lastSocket();
      sock.open();
      sock.receive(snapshot(1));
      doc.dispatch('visibilitychange');
      sock.receive(snapshot(2));
      vi.advanceTimersByTime(10_000);
      expect(sock.closeCalls).toBe(0);
      expect(FakeSocket.instances).toHaveLength(1);
    });

    it('online event with silent socket reconnects, and is removed on close', () => {
      const { connection } = connect();
      const sock = lastSocket();
      sock.open();
      win.dispatch('online');
      vi.advanceTimersByTime(3_000);
      expect(FakeSocket.instances).toHaveLength(2);

      connection.close();
      expect(win.count('online')).toBe(0);
      expect(doc.count('visibilitychange')).toBe(0);
    });
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
    expect(JSON.parse(sock.sent[0] ?? '{}')).toEqual({ actionType: 'REQUEST_SNAPSHOT' });
  });
});

// ── Host del server ──────────────────────────────────────────────────

describe('server host', () => {
  const env = { origin: 'http://tv.local:8080' };

  it('only_page_origin_is_allowed', () => {
    expect(isAllowedHost('http://tv.local:8080', env)).toBe(true);
    expect(isAllowedHost('http://tv.local:8080/', env)).toBe(true);
    expect(isAllowedHost('http://192.168.1.9:8080', env)).toBe(false);
    expect(isAllowedHost('http://evil.example:9', env)).toBe(false);
    expect(isAllowedHost('', env)).toBe(false);
  });

  it('identità con host diverso dall origin ignorata', () => {
    const storage = new Map<string, string>();
    (globalThis as Record<string, unknown>).localStorage = {
      getItem: (k: string) => storage.get(k) ?? null,
      setItem: (k: string, v: string) => void storage.set(k, v),
      removeItem: (k: string) => void storage.delete(k),
    };
    saveIdentity({ playerId: 'p', token: 't', gameId: 'g', host: 'http://192.168.1.9:8080' });
    expect(loadIdentity('g', env)).toBeNull();
    saveIdentity({ playerId: 'p', token: 't', gameId: 'h', host: env.origin });
    expect(loadIdentity('h', env)).not.toBeNull();
  });
});

// ── Identità per partita ─────────────────────────────────────────────

describe('identità', () => {
  const env = { origin: 'http://tv.local:8080' };
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

  it('forgetLastIdentity ferma la riconnessione automatica ma conserva l identità', () => {
    saveIdentity(identity('A'));
    saveIdentity(identity('B'));

    // Un'altra partita non toglie il puntatore all'ultima usata.
    forgetLastIdentity('A');
    expect(loadIdentity(undefined, env)).toEqual(identity('B'));

    forgetLastIdentity('B');
    expect(loadIdentity(undefined, env)).toBeNull();
    expect(loadIdentity('B', env)).toEqual(identity('B'));
    expect(loadIdentity('A', env)).toEqual(identity('A'));
  });

  it('expired_identity_is_ignored', () => {
    const t0 = 1_700_000_000_000;
    const day = 24 * 60 * 60 * 1000;
    saveIdentity(identity('A'), t0);
    expect(loadIdentity('A', env, t0 + day - 1)).toEqual(identity('A'));
    expect(loadIdentity('A', env, t0 + day + 1)).toBeNull();

    // Identità senza savedAt (vecchio formato): ignorata.
    storage.set('ciufciuf-identity:B', JSON.stringify(identity('B')));
    expect(loadIdentity('B', env)).toBeNull();
  });

  it('last game keeps hostToken only in sessionStorage', () => {
    const session = new Map<string, string>();
    (globalThis as Record<string, unknown>).sessionStorage = {
      getItem: (k: string) => session.get(k) ?? null,
      setItem: (k: string, v: string) => void session.set(k, v),
      removeItem: (k: string) => void session.delete(k),
    };
    saveLastGame({ gameId: 'G', hostToken: 'secret' });
    expect([...storage.values()].join()).not.toContain('secret');
    expect(loadLastGame()).toEqual({ gameId: 'G', hostToken: 'secret' });
    clearLastGame();
    expect(loadLastGame()).toBeNull();
    delete (globalThis as Record<string, unknown>).sessionStorage;
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

describe('parseSeed', () => {
  it('accetta solo interi non negativi sicuri', () => {
    expect(parseSeed('')).toBeUndefined();
    expect(parseSeed(' 42 ')).toBe(42);
    expect(parseSeed('0')).toBe(0);
    for (const bad of ['-1', 'abc', '1.5', '1e3', '9007199254740993', 'NaN']) {
      expect(parseSeed(bad)).toBeNull();
    }
  });
});
