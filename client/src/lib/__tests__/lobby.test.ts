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

  it('listSaves restituisce i salvataggi dal corpo', async () => {
    const saves = [
      {
        gameId: 'g1',
        name: 'Serata',
        rulesetId: 'usa_v2',
        players: ['Ann', 'Bob'],
        createdAt: '2026-11-10T10:00:00Z',
        updatedAt: '2026-11-10T11:00:00Z',
      },
    ];
    const calls = mockFetch(Response.json({ saves }));
    expect(await new LobbyClient('http://h:1').listSaves()).toEqual(saves);
    expect(calls[0]?.url).toBe('http://h:1/api/saves');
  });

  it('resumeSave posta senza corpo sul gameId codificato e restituisce l’hostToken', async () => {
    const created = { gameId: 'g/1', hostToken: 'host', rulesetId: 'usa_v2', maxPlayers: 5 };
    const calls = mockFetch(Response.json(created, { status: 201 }));
    expect(await new LobbyClient('http://h:1').resumeSave('g/1')).toEqual(created);
    expect(calls[0]?.url).toBe('http://h:1/api/saves/g%2F1/resume');
    expect(calls[0]?.init.method).toBe('POST');
    expect(calls[0]?.init.body).toBeUndefined();
  });

  it('resumeSave su una partita già attiva lancia un LobbyError col dettaglio del server', async () => {
    mockFetch(
      Response.json(
        { error: 'GAME_ALREADY_ACTIVE', detail: 'La partita è già attiva.' },
        { status: 409 },
      ),
    );
    const err = await new LobbyClient('http://h:1').resumeSave('g').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(LobbyError);
    expect((err as LobbyError).status).toBe(409);
    expect((err as LobbyError).code).toBe('GAME_ALREADY_ACTIVE');
    expect((err as LobbyError).message).toBe('La partita è già attiva.');
  });

  it('deleteSave invia DELETE senza corpo sul gameId codificato e accetta il 204', async () => {
    const calls = mockFetch(new Response(null, { status: 204 }));
    await expect(new LobbyClient('http://h:1').deleteSave('g/1')).resolves.toBeUndefined();
    expect(calls[0]?.url).toBe('http://h:1/api/saves/g%2F1');
    expect(calls[0]?.init.method).toBe('DELETE');
    expect(calls[0]?.init.body).toBeUndefined();
  });

  it('deleteSave su una partita attiva lancia un LobbyError col dettaglio del server', async () => {
    mockFetch(
      Response.json(
        { error: 'GAME_ALREADY_ACTIVE', detail: 'La partita è già attiva.' },
        { status: 409 },
      ),
    );
    const err = await new LobbyClient('http://h:1').deleteSave('g').catch((e: unknown) => e);
    expect(err).toBeInstanceOf(LobbyError);
    expect((err as LobbyError).status).toBe(409);
    expect((err as LobbyError).code).toBe('GAME_ALREADY_ACTIVE');
    expect((err as LobbyError).message).toBe('La partita è già attiva.');
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
