import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  LobbyClient,
  LobbyError,
  savePlayerName,
  loadPlayerName,
  saveLastGame,
  loadLastGame,
  clearLastGame,
} from '../lobby';

// Stub minimale di localStorage (environment node).
function makeStorage() {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
  };
}

beforeEach(() => {
  (globalThis as Record<string, unknown>).localStorage = makeStorage();
});

describe('player name', () => {
  it('ritorna stringa vuota se mai salvato', () => {
    expect(loadPlayerName()).toBe('');
  });

  it('salva e ricarica', () => {
    savePlayerName('Gabriele');
    expect(loadPlayerName()).toBe('Gabriele');
  });
});

describe('last game', () => {
  it('ritorna null se mai salvata', () => {
    expect(loadLastGame()).toBeNull();
  });

  it('salva e ricarica gameId + hostToken', () => {
    saveLastGame({ gameId: 'ABC123', hostToken: 'tok' });
    expect(loadLastGame()).toEqual({ gameId: 'ABC123', hostToken: 'tok' });
  });

  it('ritorna null su JSON corrotto', () => {
    localStorage.setItem('ciufciuf-last-game', '{non-json');
    expect(loadLastGame()).toBeNull();
  });

  it('clearLastGame rimuove il record', () => {
    saveLastGame({ gameId: 'ABC123', hostToken: '' });
    clearLastGame();
    expect(loadLastGame()).toBeNull();
  });
});

describe('LobbyClient', () => {
  function mockFetch(response: Response) {
    const calls: Array<{ url: string; init: RequestInit }> = [];
    vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
      calls.push({ url, init });
      return response;
    });
    return calls;
  }

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('joinGame invia il token e codifica il gameId', async () => {
    const calls = mockFetch(Response.json({ playerId: 'p', token: 't', wsUrl: '' }));
    await new LobbyClient('http://h:1/').joinGame('a/b c', 'Ann', 'RED', 'tok');
    expect(calls[0]?.url).toBe('http://h:1/api/games/a%2Fb%20c/join');
    expect(JSON.parse(String(calls[0]?.init.body))).toEqual({ name: 'Ann', color: 'RED', token: 'tok' });
  });

  it('joinGame senza token non lo invia', async () => {
    const calls = mockFetch(Response.json({ playerId: 'p', token: 't', wsUrl: '' }));
    await new LobbyClient('http://h:1').joinGame('g', 'Ann');
    expect(JSON.parse(String(calls[0]?.init.body))).toEqual({ name: 'Ann' });
  });

  it('reassignPlayer invia hostToken e nome', async () => {
    const calls = mockFetch(Response.json({ playerId: 'p', token: 'new' }));
    const who = await new LobbyClient('http://h:1').reassignPlayer('g/1', 'host', 'Ann');
    expect(who).toEqual({ playerId: 'p', token: 'new' });
    expect(calls[0]?.url).toBe('http://h:1/api/games/g%2F1/reassign');
    expect(JSON.parse(String(calls[0]?.init.body))).toEqual({ hostToken: 'host', name: 'Ann' });
  });

  it('un errore del server diventa LobbyError con codice e testo italiano', async () => {
    mockFetch(Response.json({ error: 'RESUME_TOKEN_MISMATCH', detail: null }, { status: 401 }));
    const err = await new LobbyClient('http://h:1')
      .joinGame('g', 'Ann')
      .catch((e: unknown) => e);
    expect(err).toBeInstanceOf(LobbyError);
    expect((err as LobbyError).status).toBe(401);
    expect((err as LobbyError).code).toBe('RESUME_TOKEN_MISMATCH');
    expect((err as LobbyError).message).toContain('riassegnarti');
  });
});
