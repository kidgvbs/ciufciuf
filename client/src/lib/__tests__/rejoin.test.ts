import { describe, it, expect, vi } from 'vitest';
import { buildRejoinUrl, parseRejoinHash, pruneRejoinLinks, reassignSeat } from '../rejoin';

describe('rejoin', () => {
  const identity = { gameId: 'g 1', playerId: 'p/1', token: 'a+b&c' };

  it('l URL porta l identità nel fragment e si legge di nuovo', () => {
    const url = buildRejoinUrl('http://192.168.1.5:8080/', identity);
    expect(url.startsWith('http://192.168.1.5:8080/controller#')).toBe(true);
    const hash = url.slice(url.indexOf('#'));
    expect(parseRejoinHash(hash)).toEqual(identity);
    expect(url).not.toContain('?');
  });

  it('un fragment incompleto non è un identità', () => {
    expect(parseRejoinHash('')).toBeNull();
    expect(parseRejoinHash('#gameId=g&playerId=p')).toBeNull();
  });
});

describe('pruneRejoinLinks', () => {
  const links = { Anna: 'http://x/a', Bruno: 'http://x/b' };

  it('non cambia nulla (stesso oggetto) se tutti aspettano ancora di rientrare', () => {
    const players = [
      { name: 'Anna', connected: false },
      { name: 'Bruno', connected: false },
    ];
    expect(pruneRejoinLinks(links, players, [])).toBe(links);
  });

  it('toglie la voce di chi è tornato connesso', () => {
    const players = [
      { name: 'Anna', connected: true },
      { name: 'Bruno', connected: false },
    ];
    expect(pruneRejoinLinks(links, players, [])).toEqual({ Bruno: 'http://x/b' });
  });

  it('toglie la voce di chi è uscito dal roster', () => {
    expect(pruneRejoinLinks(links, [{ name: 'Anna', connected: false }], [])).toEqual({
      Anna: 'http://x/a',
    });
  });

  it('in lobby di resume tiene i nomi del salvataggio non ancora rientrati', () => {
    expect(pruneRejoinLinks(links, [], ['Anna', 'Bruno'])).toBe(links);
    expect(
      pruneRejoinLinks(links, [{ name: 'Bruno', connected: true }], ['Anna']),
    ).toEqual({ Anna: 'http://x/a' });
  });

  it('senza link restituisce lo stesso oggetto', () => {
    const empty = {};
    expect(pruneRejoinLinks(empty, [], [])).toBe(empty);
  });
});

describe('reassignSeat', () => {
  const opts = {
    gameId: 'g1',
    token: 'host',
    name: 'Anna',
    controllerBase: 'http://tv:8080',
  };

  it('costruisce il QR della partita su cui è stata chiesta', async () => {
    const request = vi.fn(async () => ({ playerId: 'p1', token: 'tok' }));
    const url = await reassignSeat({ ...opts, request, currentGameId: () => 'g1' });
    expect(request).toHaveBeenCalledWith('g1', 'host', 'Anna');
    expect(url).not.toBeNull();
    expect(parseRejoinHash(new URL(url!).hash)).toEqual({ gameId: 'g1', playerId: 'p1', token: 'tok' });
  });

  it('reassign_response_after_game_change_leaves_links_untouched', async () => {
    let current = 'g1';
    let resolve!: (v: { playerId: string; token: string }) => void;
    const request = () => new Promise<{ playerId: string; token: string }>((r) => { resolve = r; });
    const rejoinLinks: Record<string, string> = { Bruno: 'http://x/b' };
    const pending = reassignSeat({ ...opts, request, currentGameId: () => current });
    // La TV cambia partita mentre il server pensa.
    current = 'g2';
    resolve({ playerId: 'p1', token: 'tok' });
    const url = await pending;
    expect(url).toBeNull();
    // Come fa la vista: con `null` i link non vengono toccati.
    const next = url === null ? rejoinLinks : { ...rejoinLinks, Anna: url };
    expect(next).toBe(rejoinLinks);
    expect(next).toEqual({ Bruno: 'http://x/b' });
  });

  it('se la TV ha lasciato la partita (id vuoto) l esito è scartato', async () => {
    const url = await reassignSeat({
      ...opts,
      request: async () => ({ playerId: 'p1', token: 'tok' }),
      currentGameId: () => '',
    });
    expect(url).toBeNull();
  });

  it('un errore della richiesta si propaga', async () => {
    await expect(
      reassignSeat({
        ...opts,
        request: async () => { throw new Error('boom'); },
        currentGameId: () => 'g1',
      }),
    ).rejects.toThrow('boom');
  });
});
